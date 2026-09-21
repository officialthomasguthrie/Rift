//! The Accessibility page: the screen reader, the size of the text, and the on-screen keyboard.
//!
//! The screen reader and the keyboard are Lens's to start and stop, for the keys and for the rows
//! in the Applications menu. Each switch here runs the same `lens --screen-reader` or `lens
//! --keyboard` its key runs, in a scope of its own the way Horizon starts what a key runs, so
//! closing Settings leaves it running. Whether each is running is read from the notes Lens keeps,
//! when the page comes up and every second while it is up, so a key pressed while the page is open
//! turns its switch too. The text size is the interface text size the Appearance page sets: the
//! same setting, drawn here as well.

use std::process::Command;
use std::thread;
use std::time::Duration;

use iced::futures::channel::{mpsc, oneshot};
use iced::widget::{column, text};
use iced::{Element, Fill, Subscription, Task};
use librift::access::{self, Tool};
use librift::appearance::{TEXT_LEAST, TEXT_MOST, TEXT_STEP};

use crate::theme::Colors;
use crate::ui::{Message, Settings};
use crate::widgets::{GAP, TEXT_SIZE, group, heading, note, setting, steps, switch};

/// How often the page looks again while it is up.
const EVERY: Duration = Duration::from_secs(1);

/// Which of the two is running.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Running {
    /// The screen reader.
    pub reader: bool,
    /// The on-screen keyboard.
    pub keyboard: bool,
}

impl Running {
    /// The same with this one on or off.
    #[must_use]
    pub const fn with(self, tool: Tool, on: bool) -> Self {
        match tool {
            Tool::Reader => Self { reader: on, ..self },
            Tool::Keyboard => Self {
                keyboard: on,
                ..self
            },
            Tool::Recorder => self,
        }
    }
}

/// Read Lens's notes now. It is two small files and two lines of /proc, so the window does it
/// itself as the page comes up, and nothing is drawn as off before it is known.
#[must_use]
pub fn running() -> Running {
    Running {
        reader: access::running(Tool::Reader).is_some(),
        keyboard: access::running(Tool::Keyboard).is_some(),
    }
}

/// Which is running, while the page is up: read every second, on a thread that ends at the first
/// send after the page has gone.
pub fn following() -> Subscription<Message> {
    Subscription::run_with("access", |_| {
        let (sender, receiver) = mpsc::unbounded();
        thread::spawn(move || {
            while sender.unbounded_send(Message::Access(running())).is_ok() {
                thread::sleep(EVERY);
            }
        });
        receiver
    })
}

/// Turn one of them on or off from its switch. The switch moves at once; what Lens left behind is
/// read again once it has finished, and that is what the page shows.
pub fn turn(state: &mut Settings, tool: Tool, on: bool) -> Task<Message> {
    state.problem = None;
    state.access = Some(state.access.unwrap_or_default().with(tool, on));
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let _ = sender.send(switched(tool, on));
    });
    Task::perform(receiver, |said| {
        said.unwrap_or_else(|_| Err("It stopped before it finished.".to_string()))
    })
    .then(|said| Task::done(Message::Acted(said)).chain(Task::done(Message::Access(running()))))
}

/// Run what the key runs, unless it is already the way the switch asks. Lens starts the program,
/// or stops the one running, and the scope keeps the program apart from this window.
fn switched(tool: Tool, on: bool) -> Result<(), String> {
    if access::running(tool).is_some() == on {
        return Ok(());
    }
    let output = Command::new("systemd-run")
        .args([
            "--user",
            "--scope",
            "--quiet",
            "--collect",
            "--",
            "lens",
            tool.option(),
        ])
        .output()
        .map_err(|e| format!("Could not run lens {}: {e}", tool.option()))?;
    if output.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if said.is_empty() {
        format!("lens {} did not finish.", tool.option())
    } else {
        said
    })
}

/// The lines `rift-settings --state` prints, once the page has looked.
#[must_use]
pub fn state(state: &Settings) -> Vec<String> {
    let Some(running) = state.access else {
        return Vec::new();
    };
    let word = |on: bool| if on { "on" } else { "off" };
    vec![
        format!("screen-reader {}", word(running.reader)),
        format!("on-screen-keyboard {}", word(running.keyboard)),
    ]
}

/// The page.
pub fn view(state: &Settings, look: Colors) -> Element<'_, Message> {
    let running = state.access.unwrap_or_default();
    let seeing = vec![
        setting(
            look,
            "Screen reader",
            Some(READER),
            switch(look, running.reader, |on| Message::Turn(Tool::Reader, on)),
        ),
        setting(
            look,
            "Larger text",
            Some(TEXT),
            steps(
                look,
                TEXT_LEAST..=TEXT_MOST,
                TEXT_STEP,
                state.look.text,
                "%",
                Message::Text,
                Message::Wrote,
            ),
        ),
    ];
    let typing = vec![setting(
        look,
        "On-screen keyboard",
        Some(KEYBOARD),
        switch(look, running.keyboard, |on| {
            Message::Turn(Tool::Keyboard, on)
        }),
    )];
    let mut page = column![
        column![
            heading(look, "Seeing"),
            group(look, seeing),
            note(look, ZOOM),
            note(look, CONTRAST),
        ]
        .spacing(8),
        column![heading(look, "Typing"), group(look, typing)].spacing(8),
    ]
    .spacing(GAP)
    .width(Fill);
    if let Some(why) = &state.problem {
        page = page.push(text(why).size(TEXT_SIZE).color(look.error));
    }
    page.into()
}

/// Under the screen reader's switch.
const READER: &str = "Orca reads out what is on the screen. Mod+Alt+S turns it on and off.";
/// Under the text size.
const TEXT: &str = "The interface text size, as on the Appearance page.";
/// Under the keyboard's switch.
const KEYBOARD: &str = "Keys along the bottom of the screen. Mod+Alt+K shows and hides it.";
/// What Rift does not have yet.
const ZOOM: &str = "Zooming in on part of the screen is not in Rift yet.";
const CONTRAST: &str = "A high contrast look is not in Rift yet.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_says_which_is_running_once_the_page_has_looked() {
        assert!(state(&Settings::bare()).is_empty());
        let mut kept = Settings::bare();
        kept.access = Some(Running::default().with(Tool::Keyboard, true));
        assert_eq!(state(&kept), ["screen-reader off", "on-screen-keyboard on"]);
    }

    #[test]
    fn a_switch_turns_its_own_one() {
        let both = Running::default()
            .with(Tool::Reader, true)
            .with(Tool::Keyboard, true);
        assert!(both.reader && both.keyboard);
        let reader = both.with(Tool::Keyboard, false);
        assert!(reader.reader && !reader.keyboard);
        // the recorder has a key of its own and no switch here
        assert_eq!(both.with(Tool::Recorder, false), both);
    }

    #[test]
    fn the_sentences_are_sentences() {
        for sentence in [READER, TEXT, KEYBOARD, ZOOM, CONTRAST] {
            assert!(sentence.ends_with('.'), "{sentence}");
            assert!(sentence.is_ascii(), "{sentence}");
        }
    }
}
