//! The three things a key turns on: the screen recorder, the screen reader and the on-screen
//! keyboard. Lens starts and stops them for the keys, and remembers which is running in the
//! session's runtime directory, one file per program holding its process id and, for a recording,
//! the file it is writing. The runtime directory belongs to one person and is emptied when the
//! session ends, so a note outlives the shell but never the login.
//!
//! Whether one of them is running is read here, so `lens --state` and the Accessibility page in
//! Settings answer the same question the same way.

use std::fs;
use std::path::PathBuf;

/// One of the three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    /// The screen recorder.
    Recorder,
    /// The screen reader.
    Reader,
    /// The on-screen keyboard.
    Keyboard,
}

impl Tool {
    /// The word `lens --state` prints it under, which is also the option of lens that turns it on
    /// and off: `lens --screen-reader`.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Recorder => "recording",
            Self::Reader => "screen-reader",
            Self::Keyboard => "keyboard",
        }
    }

    /// The option of lens that starts it, or stops the one that is running, which is what its key
    /// runs.
    #[must_use]
    pub const fn option(self) -> &'static str {
        match self {
            Self::Recorder => "--record",
            Self::Reader => "--screen-reader",
            Self::Keyboard => "--keyboard",
        }
    }

    /// The program it starts.
    #[must_use]
    pub const fn program(self) -> &'static str {
        match self {
            Self::Recorder => "wf-recorder",
            Self::Reader => "orca",
            Self::Keyboard => "wvkbd-mobintl",
        }
    }

    /// The signal that stops it. The recorder writes the end of the file when it is interrupted,
    /// the way it does for Ctrl+C in a terminal, so anything harsher leaves a file nothing plays.
    #[must_use]
    pub const fn signal(self) -> &'static str {
        match self {
            Self::Recorder => "-INT",
            Self::Reader | Self::Keyboard => "-TERM",
        }
    }

    /// The note in the runtime directory that says it is running.
    #[must_use]
    pub fn note(self) -> Option<PathBuf> {
        let dir = std::env::var_os("XDG_RUNTIME_DIR")?;
        Some(PathBuf::from(dir).join(format!("lens-{}", self.word())))
    }
}

/// The process id of the program, and the file it is writing, when it is running. A note whose
/// process is gone, or has been taken by something else, is cleared away.
#[must_use]
pub fn running(tool: Tool) -> Option<(u32, Option<String>)> {
    let note = tool.note()?;
    let line = fs::read_to_string(&note).ok()?;
    let (pid, file) = read_note(&line)?;
    if runs(pid, tool.program()) {
        return Some((pid, file));
    }
    let _ = fs::remove_file(&note);
    None
}

/// The process id in a note, and the file after it when there is one.
fn read_note(line: &str) -> Option<(u32, Option<String>)> {
    let (pid, file) = match line.trim().split_once(' ') {
        Some((pid, file)) => (pid, Some(file.to_string())),
        None => (line.trim(), None),
    };
    Some((pid.parse().ok()?, file))
}

/// Whether the process with this id is still the program the note was written for. The kernel cuts
/// the name it keeps to fifteen characters and a wrapper runs under a name of its own, so the whole
/// command line is what is read: it holds the path the program was started from. Nothing at all is
/// read for a process that is gone, and the note for it is thrown away.
fn runs(pid: u32, program: &str) -> bool {
    fs::read(format!("/proc/{pid}/cmdline")).is_ok_and(|line| is_program(&line, program))
}

/// Whether a command line is the program's. An empty one belongs to a process part way through
/// starting another program in its own place, which is what the wrapper of a program in the store
/// does and what the screen reader does to itself when it is started with --replace; it is still
/// that process, so it counts, and the note it was written for is kept rather than thrown away on
/// the one read that lands in the moment between the two.
fn is_program(line: &[u8], program: &str) -> bool {
    line.is_empty() || String::from_utf8_lossy(line).contains(program)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOOLS: [Tool; 3] = [Tool::Recorder, Tool::Reader, Tool::Keyboard];

    #[test]
    fn each_one_has_its_own_word_option_and_program() {
        for (i, one) in TOOLS.iter().enumerate() {
            assert!(one.option().starts_with("--"), "{one:?}");
            for other in &TOOLS[i + 1..] {
                assert_ne!(one.word(), other.word());
                assert_ne!(one.option(), other.option());
                assert_ne!(one.program(), other.program());
            }
        }
    }

    #[test]
    fn the_recorder_is_interrupted_and_the_rest_are_asked_to_end() {
        assert_eq!(Tool::Recorder.signal(), "-INT");
        assert_eq!(Tool::Reader.signal(), "-TERM");
        assert_eq!(Tool::Keyboard.signal(), "-TERM");
    }

    #[test]
    fn a_process_starting_another_in_its_own_place_is_still_itself() {
        assert!(is_program(
            b"/nix/store/hash-orca-50.2/bin/orca\0--replace\0",
            "orca"
        ));
        assert!(is_program(b"orca\0", "orca"));
        // between the two programs the kernel has no command line to give, and the process is
        // still there
        assert!(is_program(b"", "orca"));
        assert!(!is_program(b"wvkbd-mobintl\0-H\0", "orca"));
    }

    #[test]
    fn a_note_is_a_process_id_and_maybe_a_file() {
        assert_eq!(read_note("4242\n"), Some((4242, None)));
        assert_eq!(
            read_note("4242 /home/rift/Videos/Screencast 2026-09-21 10-11-12.mp4"),
            Some((
                4242,
                Some("/home/rift/Videos/Screencast 2026-09-21 10-11-12.mp4".to_string())
            ))
        );
        assert_eq!(read_note(""), None);
        assert_eq!(read_note("orca"), None);
    }
}
