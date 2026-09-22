//! Flatpak from a client's side: the remotes the system installation has, how much room an app on
//! a remote takes, which apps are installed, and an install with how far it has got. It is
//! flatpak's own command line, run with its output going into a pipe, where flatpak prints plain
//! lines with tabs between the columns and no colours. The lines are read here, and the rules
//! are tested on the lines flatpak 1.18 prints, without flatpak.
//!
//! Apps go into the system installation, which is the drive's `@flatpak` volume: not in home, so
//! the snapshots and the backups of home do not carry them, and flatpak's own polkit rule allows an
//! administrator at the machine to install without a password.

use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};

/// Flathub's name as a remote. The image gives the system installation a file for it in
/// `/etc/flatpak/remotes.d`, which flatpak turns into the remote the first time anything uses
/// that installation, with no network needed.
pub const FLATHUB: &str = "flathub";

/// The locale flatpak runs in, so the numbers and the words it prints are the ones read here.
const LOCALE: &str = "C.UTF-8";

/// The names of the system installation's remotes.
///
/// # Errors
///
/// What flatpak said when it failed, as a sentence, or that it could not be run.
pub fn remotes() -> Result<Vec<String>, String> {
    run(&["remotes", "--system", "--columns=name"]).map(|printed| names(&printed))
}

/// How much room each app a remote offers takes once it is installed, as flatpak says it: the app
/// id and `284.6 MB`. It reads the remote's summary, which needs the network.
///
/// # Errors
///
/// What flatpak said when it failed, as a sentence: no network, or no such remote.
pub fn sizes(remote: &str) -> Result<Vec<(String, String)>, String> {
    run(&[
        "remote-ls",
        "--system",
        "--app",
        "--columns=application,installed-size",
        remote,
    ])
    .map(|printed| parse_sizes(&printed))
}

/// The apps installed, in either installation.
///
/// # Errors
///
/// What flatpak said when it failed, as a sentence, or that it could not be run.
pub fn installed() -> Result<Vec<String>, String> {
    run(&["list", "--app", "--columns=application"]).map(|printed| names(&printed))
}

/// Install an app and whatever it needs from a remote into the system installation. `each` hears
/// how far it has got every time that moves. Blocks until flatpak is done.
///
/// # Errors
///
/// What flatpak said when the install failed, as a sentence, or that it could not be run.
pub fn install(remote: &str, id: &str, mut each: impl FnMut(&Progress)) -> Result<(), String> {
    let mut child = flatpak(&["install", "--system", "--assumeyes", remote, id])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not run flatpak: {e}."))?;
    // what goes wrong is on stderr, read on a thread of its own so a full pipe never holds flatpak
    let stderr = child.stderr.take();
    let errors = std::thread::spawn(move || {
        let mut said = String::new();
        if let Some(mut stderr) = stderr {
            let _ = stderr.read_to_string(&mut said);
        }
        said
    });
    let mut progress = Progress::default();
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if progress.line(&line) {
                each(&progress);
            }
        }
    }
    let status = child
        .wait()
        .map_err(|e| format!("Could not wait for flatpak: {e}."))?;
    let said = errors.join().unwrap_or_default();
    if status.success() {
        Ok(())
    } else {
        Err(sentence(&said).unwrap_or_else(|| format!("flatpak stopped with {status}.")))
    }
}

/// Run flatpak with its output going into a pipe, and answer what it printed.
fn run(args: &[&str]) -> Result<String, String> {
    let output = flatpak(args)
        .output()
        .map_err(|e| format!("Could not run flatpak: {e}."))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let said = String::from_utf8_lossy(&output.stderr);
        Err(sentence(&said).unwrap_or_else(|| format!("flatpak stopped with {}.", output.status)))
    }
}

/// flatpak with these arguments, in the locale the lines are read in, with nothing to read.
fn flatpak(args: &[&str]) -> Command {
    let mut command = Command::new("flatpak");
    command
        .args(args)
        .env("LC_ALL", LOCALE)
        .stdin(Stdio::null());
    command
}

/// What flatpak said went wrong, as a sentence: its last `error:` line, or its last line when
/// none says so.
#[must_use]
pub fn sentence(said: &str) -> Option<String> {
    let lines: Vec<&str> = said
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let chosen = lines
        .iter()
        .rev()
        .find_map(|line| {
            line.strip_prefix("error:")
                .or_else(|| line.strip_prefix("Error:"))
        })
        .or_else(|| lines.last().copied())?
        .trim();
    let mut letters = chosen.chars();
    let first = letters.next()?;
    let mut words = String::from(first.to_ascii_uppercase());
    words.extend(letters);
    if !words.ends_with(['.', '?', '!']) {
        words.push('.');
    }
    Some(words)
}

/// The names flatpak prints one a line: remotes, or installed apps.
#[must_use]
pub fn names(printed: &str) -> Vec<String> {
    printed
        .lines()
        .map(|line| line.split('\t').next().unwrap_or_default().trim())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

/// The app ids and sizes `remote-ls` prints, an app a line with a tab between the two.
#[must_use]
pub fn parse_sizes(printed: &str) -> Vec<(String, String)> {
    printed
        .lines()
        .filter_map(|line| {
            let (id, size) = line.split_once('\t')?;
            let (id, size) = (id.trim(), size.trim());
            (!id.is_empty() && !size.is_empty()).then(|| (id.to_string(), size.to_string()))
        })
        .collect()
}

/// A size as flatpak writes it, `< 1.4 MB` or `999 bytes`, in bytes. It writes SI units.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn bytes(written: &str) -> Option<u64> {
    let written = written.trim().trim_start_matches('<').trim();
    let written = written.split(" (").next().unwrap_or(written);
    let (number, unit) = written.split_once(' ')?;
    let number: f64 = number.trim().parse().ok()?;
    let scale = match unit.trim() {
        "byte" | "bytes" => 1.0,
        "kB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        _ => return None,
    };
    let value = (number * scale).round();
    // a size is never negative, and one this big is not a size
    (0.0..1e18).contains(&value).then_some(value as u64)
}

/// How far an install has got, from the lines `flatpak install` prints into a pipe: first a table
/// of the steps it will take with the download size of each, then a line as each step starts,
/// `Installing 2/3` with an ellipsis after it, and a line with the percentage of that step every
/// time it moves.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    /// The download size of each step in bytes, from the table, in the order the steps run.
    sizes: Vec<u64>,
    /// The step running now, counted from one. Nothing has started while it is naught.
    step: usize,
    /// How many steps there are.
    steps: usize,
    /// How far into the step that is running, out of a hundred.
    percent: u32,
}

impl Progress {
    /// Take one line flatpak printed. True when it moved how far the install has got.
    pub fn line(&mut self, line: &str) -> bool {
        let fields: Vec<&str> = line.split('\t').collect();
        // a row of the table: the number of the step with a dot after it first, the download size
        // last
        if fields.len() > 2
            && let Some(number) = fields[0].trim().strip_suffix('.')
            && let Ok(number) = number.parse::<usize>()
        {
            if number > 0 {
                if self.sizes.len() < number {
                    self.sizes.resize(number, 0);
                }
                self.sizes[number - 1] = fields.last().and_then(|size| bytes(size)).unwrap_or(0);
            }
            return false;
        }
        let words = line.trim_start();
        if !(words.starts_with("Installing") || words.starts_with("Updating")) {
            return false;
        }
        let (step, steps) = counted(words).unwrap_or((1, 1));
        let before = self.clone();
        if step != self.step || steps != self.steps {
            self.percent = 0;
        }
        self.step = step;
        self.steps = steps.max(step);
        if let Some(percent) = percentage(words) {
            self.percent = percent.min(100);
        }
        *self != before
    }

    /// Whether flatpak has started a step.
    #[must_use]
    pub const fn started(&self) -> bool {
        self.step > 0
    }

    /// The step running now and how many there are.
    #[must_use]
    pub const fn step(&self) -> (usize, usize) {
        (self.step, self.steps)
    }

    /// How far the whole install has got, out of a hundred: by the download size of each step when
    /// the table said them, or with every step counted alike when it did not.
    #[must_use]
    pub fn percent(&self) -> u32 {
        if self.step == 0 || self.steps == 0 {
            return 0;
        }
        let done = self.step - 1;
        let total: u64 = self.sizes.iter().sum();
        let whole = if self.sizes.len() == self.steps && total > 0 {
            let before: u64 = self.sizes[..done].iter().sum();
            let now = self.sizes[done] * u64::from(self.percent) / 100;
            (before + now) * 100 / total
        } else {
            let steps = u64::try_from(self.steps).unwrap_or(u64::MAX);
            let done = u64::try_from(done).unwrap_or(u64::MAX);
            (done.saturating_mul(100) + u64::from(self.percent)) / steps
        };
        u32::try_from(whole.min(100)).unwrap_or(100)
    }
}

/// The step and the count in `Installing 2/3` and the ellipsis after it. A transaction of one step
/// has no count.
fn counted(words: &str) -> Option<(usize, usize)> {
    let head = words.split(['\u{2026}', '.']).next()?;
    let (step, steps) = head.rsplit(' ').next()?.split_once('/')?;
    let step: usize = step.parse().ok()?;
    let steps: usize = steps.parse().ok()?;
    (step > 0).then_some((step, steps))
}

/// The percentage a progress line ends in: the number just before the `%`.
fn percentage(words: &str) -> Option<u32> {
    let before = &words[..words.find('%')?];
    let digits: String = before
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What flatpak 1.18 prints into a pipe for an app and the runtime it needs from one remote,
    /// with `--assumeyes`: the runtime found, the permissions, the table, then the two steps.
    const INSTALL: &str = "Required runtime for dev.rift.TestApp/x86_64/test (runtime/dev.rift.TestPlatform/x86_64/test) found in remote rift-test

dev.rift.TestApp permissions:
    network

 1.\t   \tdev.rift.TestPlatform\ttest\ti\trift-test\t< 3.0 MB
 2.\t   \tdev.rift.TestApp\ttest\ti\trift-test\t< 1.0 MB



Installing 1/2\u{2026}
Installing 1/2\u{2026} \u{2588}\u{2588}\u{2588}\u{258c}                  18%  1.1 MB/s
Installing 1/2\u{2026} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}              50%  1.2 MB/s
Installing 1/2\u{2026} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} 100%  1.3 MB/s
Installing 2/2\u{2026}
Installing 2/2\u{2026} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}              50%  1.0 MB/s  00:01
Installing 2/2\u{2026} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} 100%  1.1 MB/s
Installation complete.
";

    #[test]
    fn an_install_goes_by_the_size_of_each_step() {
        let mut progress = Progress::default();
        let mut seen = Vec::new();
        for line in INSTALL.lines() {
            if progress.line(line) {
                seen.push((progress.step(), progress.percent()));
            }
        }
        // the runtime is three quarters of the download, so half of it is three eighths
        assert_eq!(
            seen,
            vec![
                ((1, 2), 0),
                ((1, 2), 13),
                ((1, 2), 37),
                ((1, 2), 75),
                ((2, 2), 75),
                ((2, 2), 87),
                ((2, 2), 100),
            ]
        );
        assert!(progress.started());
    }

    #[test]
    fn without_the_table_every_step_counts_alike() {
        let mut progress = Progress::default();
        assert!(!progress.started());
        assert_eq!(progress.percent(), 0);
        assert!(progress.line("Installing 2/4\u{2026} \u{2588}\u{2588}  50%  1.0 MB/s"));
        assert_eq!(progress.percent(), 37);
        // a transaction of one step says no count
        let mut one = Progress::default();
        assert!(one.line("Installing\u{2026}"));
        assert!(one.line("Installing\u{2026} \u{2588}\u{2588}\u{2588}  60%  2.0 MB/s"));
        assert_eq!((one.step(), one.percent()), ((1, 1), 60));
        // the same line again moves nothing
        assert!(!one.line("Installing\u{2026} \u{2588}\u{2588}\u{2588}  60%  2.0 MB/s"));
        // and an update counts as a step too
        assert!(progress.line("Updating 3/4\u{2026}"));
        assert_eq!((progress.step(), progress.percent()), ((3, 4), 50));
    }

    #[test]
    fn lines_that_are_not_progress_move_nothing() {
        let mut progress = Progress::default();
        for line in [
            "",
            "Looking for matches\u{2026}",
            "    network\tipc",
            "Installation complete.",
            "Warning: something 50% odd",
        ] {
            assert!(!progress.line(line), "{line:?}");
        }
        assert_eq!(progress, Progress::default());
    }

    #[test]
    fn sizes_read_as_flatpak_writes_them() {
        assert_eq!(bytes("< 3.0 MB"), Some(3_000_000));
        assert_eq!(bytes("284.6 MB"), Some(284_600_000));
        assert_eq!(bytes("1.1 GB"), Some(1_100_000_000));
        assert_eq!(bytes("< 12.5 kB (partial)"), Some(12_500));
        assert_eq!(bytes("999 bytes"), Some(999));
        assert_eq!(bytes("1 byte"), Some(1));
        assert_eq!(bytes("0 bytes"), Some(0));
        assert_eq!(bytes("lots"), None);
        assert_eq!(bytes("3 MiB"), None);
        assert_eq!(bytes(""), None);
    }

    #[test]
    fn remote_ls_gives_an_id_and_a_size_a_line() {
        let printed =
            "org.videolan.VLC\t139.4 MB\nnet.mullvad.MullvadBrowser\t284.6 MB\n\nbroken\n";
        assert_eq!(
            parse_sizes(printed),
            vec![
                ("org.videolan.VLC".to_string(), "139.4 MB".to_string()),
                (
                    "net.mullvad.MullvadBrowser".to_string(),
                    "284.6 MB".to_string()
                ),
            ]
        );
        assert_eq!(
            names("flathub\nrift-test\n\n"),
            vec!["flathub".to_string(), "rift-test".to_string()]
        );
        assert!(names("").is_empty());
    }

    #[test]
    fn what_went_wrong_is_the_last_error_line() {
        assert_eq!(
            sentence(
                "Looking for matches\u{2026}\nerror: Unable to load summary from remote flathub: Could not resolve hostname\n"
            )
            .as_deref(),
            Some("Unable to load summary from remote flathub: Could not resolve hostname.")
        );
        assert_eq!(
            sentence("error: No remote refs found for \u{2018}x\u{2019}\n").as_deref(),
            Some("No remote refs found for \u{2018}x\u{2019}.")
        );
        assert_eq!(
            sentence("something odd happened\n").as_deref(),
            Some("Something odd happened.")
        );
        assert_eq!(sentence("\n  \n"), None);
    }
}
