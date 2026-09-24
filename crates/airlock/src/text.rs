//! `airlock text`: the text of a document, written out in a sandbox. A PDF holds its text the way
//! it is drawn on a page, so a program has to read it out, and a PDF off the internet is untrusted
//! input to a parser written in C. So pdftotext runs with no network, the store read only, and the
//! one file it reads; it is stopped if it takes too long, and what it writes is cut at a megabyte.
//! The search index of home reads its documents through this.

use std::ffi::{OsStr, OsString};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use std::{env, fs, io, thread};

pub const USAGE: &str = "Usage: airlock text <file>";

const HELP: &str = "Writes out the text of a document. The program that reads it runs in a sandbox \
with no network, where it sees the file and nothing else of yours, and it is stopped if it takes \
too long or writes too much. PDF files are the ones it reads. The search index of your home folder \
reads its documents this way.";

/// How long the program that reads a document gets.
const DEADLINE: Duration = Duration::from_secs(20);
/// How often it is looked at while it runs.
const LOOK: Duration = Duration::from_millis(50);
/// How much text it may write. What it writes past this is dropped.
const MOST: usize = 1 << 20;
/// The last page of a document that is read.
const PAGES: &str = "200";

/// The system's programs and their libraries, the only part of the machine the sandbox sees.
const STORE: &str = "/nix/store";
const TMP: &str = "/tmp";
const HOMES: &str = "/home";

pub fn text(args: &[String]) -> ExitCode {
    let file = match args {
        [one] if matches!(one.as_str(), "--help" | "-h") => {
            println!("{USAGE}\n\n{HELP}");
            return ExitCode::SUCCESS;
        }
        [one] if !one.starts_with('-') => Path::new(one),
        _ => {
            eprintln!("airlock text: one file is needed\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match written(file) {
        Ok(text) => {
            if let Err(error) = io::stdout().write_all(&text) {
                eprintln!("Could not write the text: {error}.");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("{why}");
            ExitCode::FAILURE
        }
    }
}

/// The text of one document, from the program that reads its kind, run in the sandbox.
///
/// # Errors
///
/// A sentence when the file is not there, is not a document, or the program that reads it is not
/// installed, could not be started, took too long or ended badly.
fn written(file: &Path) -> Result<Vec<u8>, String> {
    let file = fs::canonicalize(file)
        .map_err(|error| format!("Could not find {}: {error}.", file.display()))?;
    if !fs::metadata(&file).is_ok_and(|about| about.is_file()) {
        return Err(format!("{} is not a file.", file.display()));
    }
    let reader = reader(&file)?;
    let airlock =
        env::current_exe().map_err(|error| format!("Could not find airlock itself: {error}."))?;
    started(&sandbox(&airlock, &reader, &file), &file)
}

/// The program that reads a file of this kind and the arguments that write its text on stdout,
/// each an absolute path in the store, since that is all the sandbox has. A PDF is the one kind
/// for now: pdftotext, with a form feed between pages and UTF-8 whatever the file says it is.
fn reader(file: &Path) -> Result<Vec<OsString>, String> {
    let kind = file
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase);
    if kind.as_deref() != Some("pdf") {
        return Err(format!(
            "{} is not a PDF, and a PDF is what airlock text reads.",
            file.display()
        ));
    }
    let program = found("pdftotext")
        .ok_or("pdftotext is not installed, so a PDF cannot be read.".to_string())?;
    let mut line: Vec<OsString> = vec![program.into()];
    for word in ["-q", "-enc", "UTF-8", "-eol", "unix", "-l", PAGES] {
        line.push(word.into());
    }
    line.push(file.into());
    line.push("-".into());
    Ok(line)
}

/// Where a program is, with its links followed, from PATH.
fn found(name: &str) -> Option<PathBuf> {
    env::split_paths(&env::var_os("PATH")?)
        .map(|folder| folder.join(name))
        .find_map(|path| fs::canonicalize(path).ok())
        .filter(|path| path.is_file())
}

/// bwrap's arguments: namespaces of its own for everything, so no network at all; the store read
/// only and nothing else of the system; a new /proc, a /dev with no disks in it, an empty /tmp and
/// an empty /home; and the one file read only at its own path. Then `airlock enter`, which adds the
/// Landlock rules and a seccomp filter that refuses a socket of any kind on top, and the program
/// that reads the file.
fn sandbox(airlock: &Path, reader: &[OsString], file: &Path) -> Vec<OsString> {
    let mut line = Line::default();
    line.words(&[
        "--unshare-all",
        "--unshare-user",
        "--disable-userns",
        "--die-with-parent",
        "--new-session",
        "--ro-bind",
        STORE,
        STORE,
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        TMP,
        "--tmpfs",
        HOMES,
        "--ro-bind",
    ])
    .word(file)
    .word(file)
    .words(&["--chdir", TMP, "--"])
    .word(airlock)
    .words(&[
        "enter",
        "--no-network",
        "--read",
        STORE,
        "--read",
        "/proc",
        "--write",
        "/dev",
        "--write",
        TMP,
        "--read",
    ])
    .word(file)
    .word("--");
    for word in reader {
        line.word(word);
    }
    line.0
}

/// A command line being put together.
#[derive(Default)]
struct Line(Vec<OsString>);

impl Line {
    fn word(&mut self, word: impl AsRef<OsStr>) -> &mut Self {
        self.0.push(word.as_ref().to_os_string());
        self
    }

    fn words(&mut self, words: &[&str]) -> &mut Self {
        for word in words {
            self.word(word);
        }
        self
    }
}

/// Runs the sandbox and takes what it writes. Text cut off at [`MOST`] is text all the same, so it
/// comes back; a program that runs past [`DEADLINE`] or ends badly is a failure with nothing.
fn started(line: &[OsString], file: &Path) -> Result<Vec<u8>, String> {
    let mut child = Command::new("bwrap")
        .args(line)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not run bwrap: {error}."))?;
    let mut out = child
        .stdout
        .take()
        .ok_or_else(|| "bwrap left nothing to read the text from.".to_string())?;
    let cut = Arc::new(AtomicBool::new(false));
    let reading = {
        let cut = Arc::clone(&cut);
        thread::spawn(move || {
            let mut text = Vec::new();
            // one byte over the cap says there was more
            let _ = out.by_ref().take(MOST as u64 + 1).read_to_end(&mut text);
            if text.len() > MOST {
                text.truncate(MOST);
                cut.store(true, Ordering::Relaxed);
            }
            text
        })
    };
    let until = Instant::now() + DEADLINE;
    let mut why = None;
    loop {
        match child.try_wait() {
            Err(error) => return Err(format!("Could not wait for bwrap: {error}.")),
            Ok(Some(status)) => {
                if !status.success() && !cut.load(Ordering::Relaxed) {
                    why = Some(ended(status, file));
                }
                break;
            }
            // nothing more of the text is wanted, and the program would wait for ever to write it
            Ok(None) if cut.load(Ordering::Relaxed) => {}
            Ok(None) if Instant::now() >= until => {
                why = Some(format!(
                    "Reading {} took longer than {} seconds.",
                    file.display(),
                    DEADLINE.as_secs()
                ));
            }
            Ok(None) => {
                thread::sleep(LOOK);
                continue;
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        break;
    }
    let text = reading.join().unwrap_or_default();
    match why {
        Some(why) => Err(why),
        None => Ok(text),
    }
}

/// What a program that ended badly says for itself.
fn ended(status: ExitStatus, file: &Path) -> String {
    let shown = file.display();
    match status.code() {
        Some(code) => format!("The program that reads {shown} exited with {code}."),
        None => format!("The program that reads {shown} was stopped."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_is_read_with_no_network_and_nothing_but_the_file() {
        let line = sandbox(
            Path::new("/nix/store/x-rift/bin/airlock"),
            &["/nix/store/x-poppler/bin/pdftotext", "-q", "a.pdf", "-"].map(OsString::from),
            Path::new("/home/rift/notes/a.pdf"),
        );
        let words: Vec<String> = line
            .iter()
            .map(|word| word.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            words,
            [
                "--unshare-all",
                "--unshare-user",
                "--disable-userns",
                "--die-with-parent",
                "--new-session",
                "--ro-bind",
                "/nix/store",
                "/nix/store",
                "--proc",
                "/proc",
                "--dev",
                "/dev",
                "--tmpfs",
                "/tmp",
                "--tmpfs",
                "/home",
                "--ro-bind",
                "/home/rift/notes/a.pdf",
                "/home/rift/notes/a.pdf",
                "--chdir",
                "/tmp",
                "--",
                "/nix/store/x-rift/bin/airlock",
                "enter",
                "--no-network",
                "--read",
                "/nix/store",
                "--read",
                "/proc",
                "--write",
                "/dev",
                "--write",
                "/tmp",
                "--read",
                "/home/rift/notes/a.pdf",
                "--",
                "/nix/store/x-poppler/bin/pdftotext",
                "-q",
                "a.pdf",
                "-",
            ]
        );
        // the network is never shared back in, whatever else the line says
        assert!(!words.iter().any(|word| word == "--share-net"));
    }

    #[test]
    fn only_a_document_is_read() {
        let refused = reader(Path::new("/home/rift/notes/bike.txt"));
        assert_eq!(
            refused,
            Err(
                "/home/rift/notes/bike.txt is not a PDF, and a PDF is what airlock text reads."
                    .to_string()
            )
        );
        assert!(written(Path::new("/home/rift/nothing of the sort.pdf")).is_err());
    }
}
