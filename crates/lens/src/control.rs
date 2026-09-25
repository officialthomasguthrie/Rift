//! The shell, driven from a terminal. Lens listens on a socket in the session's runtime
//! directory; `lens --type`, `lens --enter`, `lens --escape`, `lens --menu`, `lens --listen`,
//! `lens --look`, `lens --dock`, `lens --notifications` and `lens --state` write one line to it,
//! and `--state` reads the answer back. `lens --volume` and `lens --brightness`, which the keys
//! for them run, write the level they left behind, and the shell shows it in the key popup. The
//! runtime directory belongs to one person, so only that person can type into their field.

// only the shell listens on the socket, and the shell is linux only
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

/// The name of the socket inside the runtime directory.
const SOCKET: &str = "lens.sock";

/// One line of the protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Put these words in the field and show what they match, opening the menu when it is closed.
    Type(String),
    /// Put them in the field, when there are any, then press Enter.
    Enter(String),
    /// Clear the field, the list and the error line, and close the menu when it is already empty.
    Escape,
    /// Open the Applications menu, or close it when it is open. What Mod+Space does.
    Menu,
    /// Start listening, or stop when the shell is already listening. What Mod+H does.
    Listen,
    /// Print what the bar shows.
    State,
    /// Read the appearance settings again and draw with them. Settings sends this when the owner
    /// changes the theme or the accent.
    Look,
    /// Read the dock's apps and its settings again, and stand it where they say. Settings sends
    /// this from the Dock page.
    Dock,
    /// Read Do not disturb and the apps kept quiet again. Settings sends this from the
    /// Notifications page.
    Notifications,
    /// Show the key popup with this level: a volume or a brightness key was pressed.
    Popup(Level),
    /// A screen recording started, or stopped and left a file behind.
    Record(Recording),
}

/// What the screen recorder did, for the mark in the bar and the notification that names the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recording {
    /// It started, and is writing this file.
    On(String),
    /// It stopped, and the file it wrote is this one.
    Off(String),
}

impl Recording {
    /// The words after `record` on the socket: `on <file>` or `off <file>`.
    #[must_use]
    pub fn words(&self) -> String {
        match self {
            Self::On(file) => format!("on {file}"),
            Self::Off(file) => format!("off {file}"),
        }
    }

    /// Read those words back.
    #[must_use]
    pub fn read(words: &str) -> Option<Self> {
        match words.split_once(' ') {
            Some(("on", file)) => Some(Self::On(file.to_string())),
            Some(("off", file)) => Some(Self::Off(file.to_string())),
            _ => None,
        }
    }

    /// The file it is writing, or wrote.
    #[must_use]
    pub fn file(&self) -> &str {
        match self {
            Self::On(file) | Self::Off(file) => file,
        }
    }
}

/// What a volume or a brightness key left behind, for the popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// The default sink's volume in percent, and whether it is muted.
    Volume {
        /// Percent of full.
        level: u8,
        /// Muted, whatever the level is.
        muted: bool,
    },
    /// The backlight in percent.
    Brightness(u8),
}

impl Level {
    /// The words after `popup` on the socket: `volume 45`, `volume 45 muted`, `brightness 80`.
    #[must_use]
    pub fn words(self) -> String {
        match self {
            Self::Volume {
                level,
                muted: false,
            } => format!("volume {level}"),
            Self::Volume { level, muted: true } => format!("volume {level} muted"),
            Self::Brightness(level) => format!("brightness {level}"),
        }
    }

    /// Read those words back. A level past a hundred is a hundred.
    #[must_use]
    pub fn read(words: &str) -> Option<Self> {
        let mut words = words.split_whitespace();
        let kind = words.next()?;
        let level = words.next()?.parse::<u16>().ok()?.min(100);
        let level = u8::try_from(level).ok()?;
        let rest: Vec<&str> = words.collect();
        match (kind, rest.as_slice()) {
            ("volume", []) => Some(Self::Volume {
                level,
                muted: false,
            }),
            ("volume", ["muted"]) => Some(Self::Volume { level, muted: true }),
            ("brightness", []) => Some(Self::Brightness(level)),
            _ => None,
        }
    }
}

impl Command {
    /// The line that carries this command.
    #[must_use]
    pub fn line(&self) -> String {
        match self {
            Self::Type(words) => format!("type {words}"),
            Self::Enter(words) => format!("enter {words}"),
            Self::Escape => "escape".to_string(),
            Self::Menu => "menu".to_string(),
            Self::Listen => "listen".to_string(),
            Self::State => "state".to_string(),
            Self::Look => "look".to_string(),
            Self::Dock => "dock".to_string(),
            Self::Notifications => "notifications".to_string(),
            Self::Popup(level) => format!("popup {}", level.words()),
            Self::Record(recording) => format!("record {}", recording.words()),
        }
    }
}

/// Read one line of the protocol. `None` when it is not one of the eleven.
#[must_use]
pub fn parse(line: &str) -> Option<Command> {
    let line = line.trim_end_matches(['\r', '\n']);
    let (verb, rest) = line.split_once(' ').unwrap_or((line, ""));
    match verb {
        "type" => Some(Command::Type(rest.to_string())),
        "enter" => Some(Command::Enter(rest.to_string())),
        "escape" => Some(Command::Escape),
        "menu" => Some(Command::Menu),
        "listen" => Some(Command::Listen),
        "state" => Some(Command::State),
        "look" => Some(Command::Look),
        "dock" => Some(Command::Dock),
        "notifications" => Some(Command::Notifications),
        "popup" => Level::read(rest).map(Command::Popup),
        "record" => Recording::read(rest).map(Command::Record),
        _ => None,
    }
}

/// Where the socket is. `None` when the session has no runtime directory.
#[must_use]
pub fn path() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|dir| PathBuf::from(dir).join(SOCKET))
}

/// Send one command to the shell that is running, and read back whatever it answers.
///
/// # Errors
///
/// When there is no runtime directory, or the shell is not listening on the socket.
pub fn ask(command: &Command) -> Result<String, String> {
    let path = path().ok_or("Lens could not find the session runtime directory")?;
    let mut stream = UnixStream::connect(&path)
        .map_err(|e| format!("Could not reach the shell on {}: {e}", path.display()))?;
    writeln!(stream, "{}", command.line()).map_err(|e| format!("Could not write to it: {e}"))?;
    let mut answer = String::new();
    stream
        .read_to_string(&mut answer)
        .map_err(|e| format!("Could not read its answer: {e}"))?;
    Ok(answer)
}

/// Send one command and drop whatever comes back.
///
/// # Errors
///
/// When there is no runtime directory, or the shell is not listening on the socket.
pub fn send(command: &Command) -> Result<(), String> {
    ask(command).map(|_| ())
}

/// Listen on the socket and hand every command to `each`, writing back what it returns. Blocks;
/// the shell calls it on a thread of its own.
///
/// # Errors
///
/// When there is no runtime directory or the socket cannot be opened.
pub fn serve<F: Fn(Command) -> Option<String>>(each: F) -> Result<(), String> {
    let path = path().ok_or("Lens could not find the session runtime directory")?;
    // a socket file an earlier run left behind refuses the bind, and no one else owns this name
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)
        .map_err(|e| format!("Could not listen on {}: {e}", path.display()))?;
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let Ok(reading) = stream.try_clone() else {
            continue;
        };
        let mut line = String::new();
        if BufReader::new(reading).read_line(&mut line).is_err() {
            continue;
        }
        if let Some(command) = parse(&line) {
            if let Some(answer) = each(command) {
                let _ = writeln!(&stream, "{}", answer.trim_end_matches('\n'));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_verbs_read_back() {
        assert_eq!(
            parse("type wifi off\n"),
            Some(Command::Type("wifi off".into()))
        );
        assert_eq!(
            parse("enter ls | first 3"),
            Some(Command::Enter("ls | first 3".into()))
        );
        assert_eq!(parse("escape\r\n"), Some(Command::Escape));
        assert_eq!(parse("menu\n"), Some(Command::Menu));
        assert_eq!(parse("listen\n"), Some(Command::Listen));
        assert_eq!(parse("state\n"), Some(Command::State));
        assert_eq!(parse("enter"), Some(Command::Enter(String::new())));
        assert_eq!(parse("type "), Some(Command::Type(String::new())));
    }

    #[test]
    fn anything_else_is_not_a_command() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("quit"), None);
        assert_eq!(parse("Type wifi"), None);
        assert_eq!(parse("popup"), None);
        assert_eq!(parse("popup volume"), None);
        assert_eq!(parse("popup volume loud"), None);
        assert_eq!(parse("popup brightness 40 muted"), None);
        assert_eq!(parse("popup battery 40"), None);
        assert_eq!(parse("record"), None);
        assert_eq!(parse("record on"), None);
        assert_eq!(parse("record stopped /home/rift/Videos/a.mp4"), None);
    }

    #[test]
    fn a_recording_carries_its_file() {
        assert_eq!(
            parse("record on /home/rift/Videos/Screencast 2026-09-19 10-11-12.mp4\n"),
            Some(Command::Record(Recording::On(
                "/home/rift/Videos/Screencast 2026-09-19 10-11-12.mp4".into()
            )))
        );
        assert_eq!(
            parse("record off /home/rift/Videos/a.mp4"),
            Some(Command::Record(Recording::Off(
                "/home/rift/Videos/a.mp4".into()
            )))
        );
        assert_eq!(
            Recording::Off("/home/rift/Videos/a.mp4".into()).file(),
            "/home/rift/Videos/a.mp4"
        );
    }

    #[test]
    fn a_popup_carries_its_level() {
        assert_eq!(
            parse("popup volume 45\n"),
            Some(Command::Popup(Level::Volume {
                level: 45,
                muted: false
            }))
        );
        assert_eq!(
            parse("popup volume 0 muted"),
            Some(Command::Popup(Level::Volume {
                level: 0,
                muted: true
            }))
        );
        assert_eq!(
            parse("popup brightness 150"),
            Some(Command::Popup(Level::Brightness(100)))
        );
    }

    #[test]
    fn a_command_survives_the_round_trip() {
        for command in [
            Command::Type("power off".into()),
            Command::Enter("echo hello".into()),
            Command::Escape,
            Command::Menu,
            Command::Listen,
            Command::State,
            Command::Look,
            Command::Dock,
            Command::Notifications,
            Command::Popup(Level::Volume {
                level: 100,
                muted: true,
            }),
            Command::Popup(Level::Brightness(5)),
            Command::Record(Recording::On("/home/rift/Videos/a.mp4".into())),
            Command::Record(Recording::Off("/home/rift/Videos/a b.mp4".into())),
        ] {
            assert_eq!(parse(&command.line()), Some(command));
        }
    }
}
