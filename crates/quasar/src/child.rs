//! A program quasard runs once and then forgets: the voice for one sentence, whisper for one
//! recording. Neither of them is a server, so neither is supervised the way the two llama-servers
//! are. Each is started, read to the end, and killed if it stops answering.

use std::io::Read;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// How often the program is looked at while it runs.
const POLL: Duration = Duration::from_millis(20);
/// How much of what it wrote is kept, on each stream.
const MOST: u64 = 1 << 20;

/// What a program that was run once did.
#[derive(Debug)]
pub struct Ended {
    /// Whether it ended by itself and said it went well.
    pub ok: bool,
    /// Whether it was killed for taking too long.
    pub stopped: bool,
    /// What it wrote on its output.
    pub out: String,
    /// The last line it said about trouble, or how it ended when it said nothing.
    pub why: String,
}

/// Runs the program to its end, reading both of its streams, and kills it if it takes longer than
/// the deadline. `what` names it in a sentence, as in "could not start the voice".
///
/// # Errors
///
/// A sentence when the program could not be started or waited for.
pub fn run(
    program: &Path,
    args: &[String],
    timeout: Duration,
    what: &str,
) -> Result<Ended, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not start {what}: {e}."))?;
    // both streams are read on threads of their own, so a full pipe never stops the program. a
    // model says a few lines about itself on its way, and everything it says about a failure is on
    // the same stream as that
    let mut out = child.stdout.take().map(reader);
    let mut trouble = child.stderr.take().map(reader);
    let deadline = Instant::now() + timeout;
    let mut stopped = false;
    let exit = loop {
        match child.try_wait() {
            Ok(Some(exit)) => break Ok(exit),
            Ok(None) => {}
            Err(e) => return Err(format!("Could not wait for {what}: {e}.")),
        }
        if Instant::now() > deadline {
            stopped = true;
            let _ = child.kill();
            break child
                .wait()
                .map_err(|e| format!("Could not wait for {what}: {e}."));
        }
        thread::sleep(POLL);
    };
    let exit = exit?;
    let read = |stream: Option<thread::JoinHandle<String>>| {
        stream
            .and_then(|stream| stream.join().ok())
            .unwrap_or_default()
    };
    Ok(Ended {
        ok: exit.success() && !stopped,
        stopped,
        out: read(out.take()),
        why: why(&read(trouble.take()), exit),
    })
}

/// Reads one of the program's streams to its end on a thread of its own.
fn reader(stream: impl Read + Send + 'static) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut text = String::new();
        let _ = stream.take(MOST).read_to_string(&mut text);
        text
    })
}

/// What to put in front of a caller about a failed run: the last line the program said, or the
/// way it ended when it said nothing.
fn why(trouble: &str, exit: ExitStatus) -> String {
    trouble
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .map_or_else(|| exit.to_string(), ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn what_the_program_wrote_comes_back() {
        let ended = run(
            Path::new("echo"),
            &words(&["the words it said"]),
            Duration::from_secs(20),
            "the voice",
        )
        .unwrap();
        assert!(ended.ok);
        assert!(!ended.stopped);
        assert_eq!(ended.out.trim(), "the words it said");
    }

    #[test]
    fn a_failure_says_the_last_line_the_program_said() {
        let ended = run(
            Path::new("sh"),
            &words(&["-c", "echo loading >&2; echo no sample rate >&2; exit 3"]),
            Duration::from_secs(20),
            "the voice",
        )
        .unwrap();
        assert!(!ended.ok);
        assert_eq!(ended.why, "no sample rate");
        let quiet = run(
            Path::new("false"),
            &[],
            Duration::from_secs(20),
            "the voice",
        )
        .unwrap();
        assert!(!quiet.ok);
        assert!(quiet.why.contains('1'), "{}", quiet.why);
    }

    #[test]
    fn one_that_stops_answering_is_killed() {
        let ended = run(
            Path::new("sleep"),
            &words(&["30"]),
            Duration::from_millis(100),
            "whisper",
        )
        .unwrap();
        assert!(ended.stopped);
        assert!(!ended.ok);
    }

    #[test]
    fn a_program_that_is_not_there_says_so() {
        let why = run(
            Path::new("/nonexistent/quasar-test"),
            &[],
            Duration::from_secs(20),
            "whisper",
        )
        .unwrap_err();
        assert!(why.starts_with("Could not start whisper:"), "{why}");
    }
}
