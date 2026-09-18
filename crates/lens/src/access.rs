//! The three things a key turns on: the screen recorder, the screen reader and the on-screen
//! keyboard. Horizon runs `lens --record`, `lens --screen-reader` and `lens --keyboard` for the
//! keys, and the Applications menu has a row for the last two. Each of them starts a program, and
//! stops it again when it is already running.
//!
//! What is running is remembered in the session's runtime directory, one file per program holding
//! its process id and, for a recording, the file it is writing. The runtime directory belongs to
//! one person and is emptied when the session ends, so the note outlives the shell but never the
//! login. The shell reads the same notes for `lens --state`, and the recorder tells it over the
//! socket, so the bar shows a mark while the screen is being recorded.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::{fs, thread};

use librift::appearance::Theme;

use crate::control::{self, Recording};
use crate::theme::{self, Palette};

/// Where a recording is written, under home, beside the Pictures folder Horizon writes screenshots
/// into. The name is the one Horizon gives a screenshot, with the day and the time in it.
const FOLDER: &str = "Videos";
/// How `date` writes the part of the name that says when it was taken.
const WHEN: &str = "+%Y-%m-%d %H-%M-%S";
/// How tall the on-screen keyboard is, in logical pixels, on a screen either way up.
const KEYBOARD: u32 = 260;
/// How long to wait for a program to finish after it has been asked to.
const STOPPING: Duration = Duration::from_secs(10);
/// How often to look while waiting for that.
const LOOK: Duration = Duration::from_millis(100);

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
    /// The word `lens --state` prints it under.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Recorder => "recording",
            Self::Reader => "screen-reader",
            Self::Keyboard => "keyboard",
        }
    }

    /// The program it starts.
    const fn program(self) -> &'static str {
        match self {
            Self::Recorder => "wf-recorder",
            Self::Reader => "orca",
            Self::Keyboard => "wvkbd-mobintl",
        }
    }

    /// The signal that stops it. The recorder writes the end of the file when it is interrupted,
    /// the way it does for Ctrl+C in a terminal, so anything harsher leaves a file nothing plays.
    const fn signal(self) -> &'static str {
        match self {
            Self::Recorder => "-INT",
            Self::Reader | Self::Keyboard => "-TERM",
        }
    }

    /// The note in the runtime directory that says it is running.
    fn note(self) -> Option<PathBuf> {
        let dir = std::env::var_os("XDG_RUNTIME_DIR")?;
        Some(PathBuf::from(dir).join(format!("lens-{}", self.word())))
    }
}

/// What a key left behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// Whether it is on now.
    pub on: bool,
    /// The file a recording was written to, for the notification and the line the terminal prints.
    pub file: Option<String>,
}

/// Turn one of the three on, or off when it is already on, and say what happened in a sentence.
///
/// # Errors
///
/// When the session has no runtime directory, or the program cannot be started or stopped.
pub fn toggle(tool: Tool) -> Result<String, String> {
    let change = switch(tool)?;
    if tool == Tool::Recorder {
        let file = change.file.clone().unwrap_or_default();
        let _ = control::send(&control::Command::Record(if change.on {
            Recording::On(file)
        } else {
            Recording::Off(file)
        }));
    }
    Ok(said(tool, &change))
}

/// The sentence for what happened, which the terminal prints and the shell shows.
fn said(tool: Tool, change: &Change) -> String {
    let file = change.file.as_deref().unwrap_or_default();
    match (tool, change.on) {
        (Tool::Recorder, true) => format!("Recording the screen to {file}"),
        (Tool::Recorder, false) => format!("The screen recording is in {file}"),
        (Tool::Reader, true) => "The screen reader is on".to_string(),
        (Tool::Reader, false) => "The screen reader is off".to_string(),
        (Tool::Keyboard, true) => "The on-screen keyboard is on".to_string(),
        (Tool::Keyboard, false) => "The on-screen keyboard is off".to_string(),
    }
}

/// Start it, or stop the one that is running.
fn switch(tool: Tool) -> Result<Change, String> {
    let note = tool.note().ok_or_else(|| {
        "Lens could not find the session runtime directory, so it did not start anything"
            .to_string()
    })?;
    if let Some((pid, file)) = running(tool) {
        stop(tool, pid)?;
        let _ = fs::remove_file(&note);
        return Ok(Change { on: false, file });
    }
    let file = (tool == Tool::Recorder).then(recording).transpose()?;
    let pid = start(tool, file.as_deref())?;
    let line = file
        .as_ref()
        .map_or_else(|| pid.to_string(), |file| format!("{pid} {file}"));
    fs::write(&note, line)
        .map_err(|e| format!("Could not write {}: {e}", note.display()))
        .map(|()| Change { on: true, file })
}

/// The process id of the program, and the file it is writing, when it is running. A note whose
/// process is gone, or has been taken by something else, is cleared away.
#[must_use]
pub fn running(tool: Tool) -> Option<(u32, Option<String>)> {
    let note = tool.note()?;
    let line = fs::read_to_string(&note).ok()?;
    let (pid, file) = match line.trim().split_once(' ') {
        Some((pid, file)) => (pid, Some(file.to_string())),
        None => (line.trim(), None),
    };
    let pid: u32 = pid.parse().ok()?;
    if runs(pid, tool.program()) {
        return Some((pid, file));
    }
    let _ = fs::remove_file(&note);
    None
}

/// The lines `lens --state` prints for the three, whether the shell is drawing anything for them
/// or not: the file a recording is being written to, or `off`.
#[must_use]
pub fn lines() -> String {
    let mut lines = String::new();
    for tool in [Tool::Recorder, Tool::Reader, Tool::Keyboard] {
        let said = match running(tool) {
            None => "off".to_string(),
            Some((_, None)) => "on".to_string(),
            Some((_, Some(file))) => file,
        };
        lines.push_str(tool.word());
        lines.push(' ');
        lines.push_str(&said);
        lines.push('\n');
    }
    lines
}

/// Whether the process with this id is still the program the note was written for. The kernel cuts
/// the name it keeps to fifteen characters and a wrapper runs under a name of its own, so the whole
/// command line is what is read: it holds the path the program was started from.
fn runs(pid: u32, program: &str) -> bool {
    fs::read(format!("/proc/{pid}/cmdline"))
        .is_ok_and(|line| String::from_utf8_lossy(&line).contains(program))
}

/// Whether there is a process with this id at all.
fn alive(pid: u32) -> bool {
    fs::metadata(format!("/proc/{pid}")).is_ok()
}

/// Where a recording started now is written. The folder is made when it is not there, the way an
/// app makes the folder it saves into.
fn recording() -> Result<String, String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "Lens could not find your home folder".to_string())?;
    let folder = home.join(FOLDER);
    fs::create_dir_all(&folder).map_err(|e| format!("Could not make {}: {e}", folder.display()))?;
    let when = Command::new("date")
        .arg(WHEN)
        .output()
        .map_err(|e| format!("Could not read the time: {e}"))?;
    let when = String::from_utf8_lossy(&when.stdout).trim().to_string();
    Ok(folder
        .join(format!("Screencast {when}.mp4"))
        .to_string_lossy()
        .into_owned())
}

/// Start the program, with the arguments this tool takes, and let it go.
fn start(tool: Tool, file: Option<&str>) -> Result<u32, String> {
    let mut command = Command::new(tool.program());
    match tool {
        // the screen is read frame by frame over wlr-screencopy, the protocol the screenshot key
        // uses, and encoded on the processor, so a machine with no video encoder of its own
        // records all the same. sound is not recorded: it would need a source to record from, and
        // a screen recording with silence where the sound should be is worse than one with none
        Tool::Recorder => {
            command.args(["-f", file.unwrap_or_default()]);
        }
        // one screen reader at a time: --replace takes over from one that is already there, which
        // a plain start refuses to do
        Tool::Reader => {
            command.arg("--replace");
        }
        Tool::Keyboard => {
            command.args(keyboard(theme::palette(Theme::read())));
        }
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|child| child.id())
        .map_err(|e| format!("Could not start {}: {e}", tool.program()))
}

/// The on-screen keyboard's size and colours: the shell's own, so it looks like the bar and the
/// menus above it. The background is the hairline gray, which shows between the keys, the keys are
/// the gray of a menu, the wider keys a step lighter, and the key under the finger takes the
/// accent, the way everything else the shell marks as active does.
fn keyboard(look: Palette) -> Vec<String> {
    let tall = KEYBOARD.to_string();
    let mut words = vec![
        "-H".to_string(),
        tall.clone(),
        "-L".to_string(),
        tall,
        "--fn".to_string(),
        "Noto Sans 14".to_string(),
    ];
    for (flag, colour) in [
        ("--bg", look.line),
        ("--fg", look.menu),
        ("--fg-sp", look.press),
        ("--press", look.accent),
        ("--press-sp", look.accent),
        ("--text", look.text),
        ("--text-sp", look.text),
    ] {
        words.push(flag.to_string());
        words.push(hex(colour));
    }
    words
}

/// A colour of the shell as the keyboard takes it, six hex digits with no hash.
fn hex(colour: iced::Color) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let byte = |part: f32| (part.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "{:02x}{:02x}{:02x}",
        byte(colour.r),
        byte(colour.g),
        byte(colour.b)
    )
}

/// Ask the program to finish, and wait until it has. The recorder takes a moment to write the end
/// of the file, and the answer is only true once it is gone.
fn stop(tool: Tool, pid: u32) -> Result<(), String> {
    let asked = Command::new("kill")
        .args([tool.signal(), &pid.to_string()])
        .status()
        .map_err(|e| format!("Could not stop {}: {e}", tool.program()))?;
    if !asked.success() {
        return Err(format!(
            "{} would not stop: kill exited with {asked}",
            tool.program()
        ));
    }
    let until = Instant::now() + STOPPING;
    while Instant::now() < until {
        if !alive(pid) {
            return Ok(());
        }
        thread::sleep(LOOK);
    }
    Err(format!(
        "{} is still running after it was asked to stop",
        tool.program()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_one_has_its_own_word_and_program() {
        let tools = [Tool::Recorder, Tool::Reader, Tool::Keyboard];
        for (i, one) in tools.iter().enumerate() {
            for other in &tools[i + 1..] {
                assert_ne!(one.word(), other.word());
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
    fn a_colour_of_the_shell_is_six_hex_digits() {
        assert_eq!(hex(theme::DARK.line), "141414");
        assert_eq!(hex(theme::DARK.accent), "78aeed");
        assert_eq!(hex(theme::LIGHT.menu), "fafafa");
    }

    #[test]
    fn the_keyboard_takes_its_colours_from_the_theme() {
        let words = keyboard(theme::DARK);
        let after = |flag: &str| {
            words
                .iter()
                .position(|word| word == flag)
                .and_then(|at| words.get(at + 1))
                .cloned()
                .unwrap_or_default()
        };
        assert_eq!(after("--bg"), "141414");
        assert_eq!(after("--fg"), "2e2e2e");
        assert_eq!(after("--press"), "78aeed");
        assert_eq!(after("-H"), KEYBOARD.to_string());
        assert_eq!(after("-L"), KEYBOARD.to_string());
        assert_eq!(after("--fn"), "Noto Sans 14");
    }

    #[test]
    fn what_the_terminal_prints() {
        let file = Some("/home/rift/Videos/Screencast 2026-09-19 10-11-12.mp4".to_string());
        assert!(
            said(
                Tool::Recorder,
                &Change {
                    on: true,
                    file: file.clone()
                }
            )
            .starts_with("Recording the screen to /home/")
        );
        assert!(
            said(Tool::Recorder, &Change { on: false, file }).contains("Screencast 2026-09-19")
        );
        assert_eq!(
            said(
                Tool::Reader,
                &Change {
                    on: true,
                    file: None
                }
            ),
            "The screen reader is on"
        );
        assert_eq!(
            said(
                Tool::Keyboard,
                &Change {
                    on: false,
                    file: None
                }
            ),
            "The on-screen keyboard is off"
        );
    }
}
