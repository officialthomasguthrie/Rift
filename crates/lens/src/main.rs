//! lens: the shell. The top bar along the top of the screen, the Applications menu under it,
//! and four interpreters behind its field: the app launcher, the OS commands, nushell and Quasar.
//!
//! `lens` draws the bar, the dock, the menus and the dialogs as layer-shell surfaces on the running
//! session. `lens
//! --route <words>` prints what the field would do with those words and runs nothing. `lens --do
//! [--yes] <words>` does it from a terminal instead, with `--yes` standing in for the
//! confirmation the field asks for. `lens --type <words>`, `lens --enter [<words>]` and
//! `lens --escape` type into the field of the shell that is already running, `lens --menu` opens
//! and closes the Applications menu, and `lens --state` prints what the bar shows. `lens --volume
//! up|down|mute` and `lens --brightness up|down` are what the keys for them run: they make the
//! change and the shell shows the level in the key popup. `lens --record`, `lens --screen-reader`
//! and `lens --keyboard` are the keys for the screen recorder, the screen reader and the on-screen
//! keyboard: each starts its program, and stops it again when it is already running.

#[cfg(target_os = "linux")]
mod access;
mod answer;
#[cfg(target_os = "linux")]
mod banner;
#[cfg(target_os = "linux")]
mod bar;
mod calendar;
#[cfg(target_os = "linux")]
mod clock;
mod control;
#[cfg(target_os = "linux")]
mod datemenu;
#[cfg(target_os = "linux")]
mod dialog;
#[cfg(target_os = "linux")]
mod dock;
mod horizon;
#[cfg(target_os = "linux")]
mod icons;
#[cfg(target_os = "linux")]
mod keys;
mod launcher;
#[cfg(target_os = "linux")]
mod menu;
#[cfg(target_os = "linux")]
mod notice;
mod nu;
#[cfg(target_os = "linux")]
mod popup;
mod route;
#[cfg(target_os = "linux")]
mod status;
#[cfg(target_os = "linux")]
mod system;
#[cfg(target_os = "linux")]
mod theme;
#[cfg(target_os = "linux")]
mod ui;
#[cfg(target_os = "linux")]
mod watch;

use std::env;
use std::process::ExitCode;

use librift::{os, quasar};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") => {
            println!("lens {}", librift::VERSION);
            ExitCode::SUCCESS
        }
        Some("--route") => {
            let apps = launcher::load();
            println!("{:?}", route::route(&args[1..].join(" "), &apps));
            ExitCode::SUCCESS
        }
        Some("--do") => {
            let yes = args.get(1).is_some_and(|a| a == "--yes");
            let words = &args[if yes { 2 } else { 1 }..];
            act(&words.join(" "), yes)
        }
        Some("--type") => tell(&control::Command::Type(args[1..].join(" "))),
        Some("--enter") => tell(&control::Command::Enter(args[1..].join(" "))),
        Some("--escape") => tell(&control::Command::Escape),
        Some("--menu") => tell(&control::Command::Menu),
        Some("--state") => show(),
        #[cfg(target_os = "linux")]
        Some("--volume") => key(keys::volume(args.get(1).map_or("", String::as_str))),
        #[cfg(target_os = "linux")]
        Some("--brightness") => key(keys::brightness(args.get(1).map_or("", String::as_str))),
        #[cfg(target_os = "linux")]
        Some("--record") => turn(access::Tool::Recorder),
        #[cfg(target_os = "linux")]
        Some("--screen-reader") => turn(access::Tool::Reader),
        #[cfg(target_os = "linux")]
        Some("--keyboard") => turn(access::Tool::Keyboard),
        Some(other) => {
            eprintln!(
                "lens: unknown option {other}. lens [--version | --route <words> | --do [--yes] <words> | --type <words> | --enter [<words>] | --escape | --menu | --state | --volume up|down|mute | --brightness up|down | --record | --screen-reader | --keyboard]"
            );
            ExitCode::from(2)
        }
        None => shell(),
    }
}

/// What the bar of the shell that is running shows, one line per part.
fn show() -> ExitCode {
    match control::ask(&control::Command::State) {
        Ok(lines) => {
            print!("{lines}");
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("{why}");
            ExitCode::FAILURE
        }
    }
}

/// The screen recorder, the screen reader or the on-screen keyboard: it starts, or stops when it
/// is already running, and the line says which.
#[cfg(target_os = "linux")]
fn turn(tool: access::Tool) -> ExitCode {
    match access::toggle(tool) {
        Ok(said) => {
            println!("{said}");
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("{why}");
            ExitCode::FAILURE
        }
    }
}

/// A volume or a brightness key: the change is made, and the shell that is running shows the level.
/// A shell that is not running changes nothing about the key.
#[cfg(target_os = "linux")]
fn key(done: Result<control::Level, String>) -> ExitCode {
    match done {
        Ok(level) => {
            let _ = control::send(&control::Command::Popup(level));
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("{why}");
            ExitCode::FAILURE
        }
    }
}

/// Type into the field of the shell that is running.
fn tell(command: &control::Command) -> ExitCode {
    match control::send(command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("{why}");
            ExitCode::FAILURE
        }
    }
}

/// What the field does on Enter, from a terminal.
fn act(input: &str, yes: bool) -> ExitCode {
    use route::Interpretation;
    let apps = launcher::load();
    let outcome = match route::route(input, &apps) {
        Interpretation::Nothing => Ok(String::new()),
        Interpretation::Launch(app) => {
            launcher::launch(&app).map(|()| format!("Starting {}", app.name))
        }
        Interpretation::Os(action) => command(&action, yes),
        Interpretation::Usage(usage) => Err(usage.to_string()),
        Interpretation::Shell(line) => nu::run(&line),
        Interpretation::Ask(question) => {
            quasar::ask(&question).and_then(|(kind, text)| match quasar::read(&kind, &text) {
                quasar::Reply::Answer(answer) => Ok(answer),
                quasar::Reply::Action(action) => command(&action, yes),
                quasar::Reply::Refused(why) => Err(why),
            })
        }
    };
    match outcome {
        Ok(output) => {
            if !output.is_empty() {
                println!("{output}");
            }
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("{why}");
            ExitCode::FAILURE
        }
    }
}

/// An OS command from a terminal, typed or proposed by Quasar. One that changes something waits
/// for `--yes`.
fn command(action: &os::Action, yes: bool) -> Result<String, String> {
    if action.mutating && !yes {
        Err(format!("{}? Run again with --yes.", action.summary))
    } else {
        os::run(action)
    }
}

#[cfg(target_os = "linux")]
fn shell() -> ExitCode {
    let apps = launcher::load();
    eprintln!(
        "lens {}: {} apps, opening the bar",
        librift::VERSION,
        apps.len()
    );
    match ui::run(apps) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("lens: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn shell() -> ExitCode {
    eprintln!("lens: the shell needs a Wayland session on Linux");
    ExitCode::FAILURE
}
