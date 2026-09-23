//! rift-files: Files, the file manager. A window for each folder, with the places down the left,
//! what is in the folder in a list, and the trash; files open with the apps that open their kind.
//!
//! The header bar searches the folder and what is under it, by name as it is typed and by meaning
//! on Enter, and the Timeline shows the folder as it was at one of Vault's snapshots of home.
//!
//! `rift-files` opens a window on home, and `rift-files <folder>` one on that folder, or on the
//! folder of a file with the file selected; with Files running, it asks that one for the window.
//! `rift-files --set <name> <value>` does what pressing it would in the window in front, and
//! `rift-files --state` prints what that window shows.

mod actions;
mod browser;
mod control;
mod dialogs;
mod find;
mod jobs;
mod keys;
mod list;
mod menus;
mod timeline;
mod ui;
mod view;

use std::path::PathBuf;
use std::process::ExitCode;

use control::Command;
// the colours, the rows, the menus and the icons, which Settings and Welcome draw with too
use rift_ui::{icons, theme, widgets};

const USAGE: &str = "Usage: rift-files [<folder or file>...] [--screenshot <png>]\n       rift-files [--set <name> <value> | --state]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["--help" | "-h"] => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        ["--version"] => {
            println!("rift-files {}", librift::VERSION);
            ExitCode::SUCCESS
        }
        ["--state"] => match control::ask(&Command::State) {
            Ok(lines) => {
                print!("{lines}");
                ExitCode::SUCCESS
            }
            Err(why) => fail(&why),
        },
        ["--set", name, rest @ ..] if !rest.is_empty() => {
            tell(&Command::Set((*name).to_string(), rest.join(" ")))
        }
        [paths @ .., "--screenshot", png] => open(paths, Some(PathBuf::from(*png))),
        [other, ..] if other.starts_with("--") => {
            fail(&format!("rift-files: unknown option {other}\n{USAGE}"))
        }
        paths => open(paths, None),
    }
}

/// Open a window for each path, or on home with none, or ask the Files that is running for them. A
/// window that is only there to have its picture taken opens whatever else is running.
fn open(given: &[&str], screenshot: Option<PathBuf>) -> ExitCode {
    let paths: Vec<PathBuf> = given
        .iter()
        .map(|given| {
            let path = librift::files::path_of(given);
            std::path::absolute(&path).unwrap_or(path)
        })
        .collect();
    if screenshot.is_none() && control::already_open() {
        if paths.is_empty() {
            return tell(&Command::Open(String::new()));
        }
        for path in &paths {
            if let code @ ExitCode::FAILURE = tell(&Command::Open(path.display().to_string())) {
                return code;
            }
        }
        return ExitCode::SUCCESS;
    }
    match ui::run(ui::Start {
        open: paths,
        screenshot,
    }) {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => fail(&format!("rift-files: {why}")),
    }
}

/// Send one line to the Files that is running.
fn tell(command: &Command) -> ExitCode {
    match control::ask(command) {
        Ok(_) => ExitCode::SUCCESS,
        Err(why) => fail(&why),
    }
}

fn fail(why: &str) -> ExitCode {
    eprintln!("{why}");
    ExitCode::FAILURE
}
