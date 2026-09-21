//! The Mouse and touchpad page: which button clicks, how fast the pointer moves, how scrolling
//! goes, and what a touchpad does with a tap and while a key is typed.
//!
//! The settings are the owner's, in a file under `~/.config/rift`, and they reach Horizon as a part
//! of its config that it reads again when the file changes, so a mouse follows at once. The page
//! shows the mouse's settings while a mouse is plugged in and the touchpad's on a machine with one,
//! and says so where there is none. Which devices there are is read from the kernel as the page
//! comes up and every two seconds while it is up, so a mouse plugged in turns up on it.

use std::thread;
use std::time::Duration;

use iced::futures::channel::mpsc;
use iced::widget::{column, text};
use iced::{Element, Fill, Subscription};
use librift::pointer::{self as devices, Device, Kind, Mouse, Pointer, Scrolling, Touchpad};

use crate::theme::Colors;
use crate::ui::{Message, Settings};
use crate::widgets::{GAP, TEXT_SIZE, choice, group, heading, note, setting, speed, switch};

/// How often the page looks for devices again while it is up.
const EVERY: Duration = Duration::from_secs(2);

/// The devices while the page is up: read as the page comes up and every two seconds after, on a
/// thread that ends at the first send after the page has gone.
pub fn following() -> Subscription<Message> {
    Subscription::run_with("pointer", |_| {
        let (sender, receiver) = mpsc::unbounded();
        thread::spawn(move || {
            while sender
                .unbounded_send(Message::Devices(devices::devices()))
                .is_ok()
            {
                thread::sleep(EVERY);
            }
        });
        receiver
    })
}

/// Whether this machine has a device of this kind, once the page has looked.
fn has(state: &Settings, kind: Kind) -> bool {
    state
        .devices
        .as_ref()
        .and_then(|found| found.as_ref().ok())
        .is_some_and(|found| found.iter().any(|device| device.kind == kind))
}

/// The settings with one of them changed, from `rift-settings --set`, the way pressing it on the
/// page changes it. Nothing for a name or a value that is not one, and for a setting of a device
/// this machine does not have: the page draws no row for it.
#[must_use]
pub fn named(state: &Settings, name: &str, value: &str) -> Option<Pointer> {
    let usable = match Kind::of(name) {
        Some(kind) => has(state, kind),
        None => has(state, Kind::Mouse) || has(state, Kind::Touchpad),
    };
    let mut next = state.pointer;
    (usable && next.set(name, value)).then_some(next)
}

/// What the owner did on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Changed {
    /// A setting changed, and the settings are written.
    Chose(Pointer),
    /// A speed slider moved.
    Dragged(Pointer),
    /// A speed slider was let go, so the settings are written.
    Dropped,
}

/// A setting changed on the page or from the socket, and the settings are written and handed to
/// Horizon; or a speed slider moved, which is shown, and written once it is let go.
pub fn update(state: &mut Settings, changed: Changed) {
    match changed {
        Changed::Chose(chosen) => write(state, chosen),
        Changed::Dragged(moved) => state.pointer = moved,
        Changed::Dropped => write(state, state.pointer),
    }
}

/// Write the settings the owner chose, and hand them to Horizon.
fn write(state: &mut Settings, pointer: Pointer) {
    state.pointer = pointer;
    state.problem = pointer.save().err();
}

/// The lines `rift-settings --state` prints, once the page has looked for devices: how many mice
/// and touchpads there are with a line for each, and every setting.
#[must_use]
pub fn state(state: &Settings) -> Vec<String> {
    let Some(found) = state.devices.as_ref() else {
        return Vec::new();
    };
    let Ok(found) = found else {
        return vec!["mice none".to_string(), "touchpads none".to_string()];
    };
    let mut lines = Vec::new();
    for (kind, count) in [(Kind::Mouse, "mice"), (Kind::Touchpad, "touchpads")] {
        let these: Vec<&Device> = found.iter().filter(|device| device.kind == kind).collect();
        lines.push(format!("{count} {}", these.len()));
        for device in these {
            lines.push(format!("{} {}", kind.word(), device.name));
        }
    }
    lines.extend(state.pointer.lines());
    lines
}

/// The page.
pub fn view(state: &Settings, look: Colors) -> Element<'_, Message> {
    let mut page = column![].spacing(GAP).width(Fill);
    match state.devices.as_ref() {
        None => return note(look, "Looking for a mouse and a touchpad."),
        Some(Err(why)) => {
            page = page.push(note(
                look,
                "The mouse and the touchpad are not here, since the kernel's list of devices could \
                 not be read.",
            ));
            page = page.push(note(look, why));
        }
        Some(Ok(_)) => {
            let (mouse, touchpad) = (has(state, Kind::Mouse), has(state, Kind::Touchpad));
            if mouse || touchpad {
                page = page.push(the_button(look, state.pointer));
            }
            page = page.push(if mouse {
                the_mouse(look, state.pointer)
            } else {
                none(look, "Mouse", "No mouse is plugged in.")
            });
            page = page.push(if touchpad {
                the_touchpad(look, state.pointer)
            } else {
                none(look, "Touchpad", "This machine has no touchpad.")
            });
        }
    }
    if let Some(why) = &state.problem {
        page = page.push(text(why).size(TEXT_SIZE).color(look.error));
    }
    page.into()
}

/// A heading with the sentence that says there is nothing under it.
fn none<'a>(look: Colors, title: &'a str, said: &'a str) -> Element<'a, Message> {
    column![heading(look, title), note(look, said)]
        .spacing(8)
        .into()
}

/// The message a row or a switch sends: these settings, to be written.
fn chosen(pointer: Pointer) -> Message {
    Message::Pointer(Changed::Chose(pointer))
}

/// The message a speed slider sends while it moves.
fn dragged(pointer: Pointer) -> Message {
    Message::Pointer(Changed::Dragged(pointer))
}

/// Which button clicks, on every mouse and touchpad.
fn the_button<'a>(look: Colors, pointer: Pointer) -> Element<'a, Message> {
    let rows = [(false, "Left"), (true, "Right")]
        .into_iter()
        .map(|(left_handed, label)| {
            choice(
                look,
                label,
                None,
                None,
                pointer.left_handed == left_handed,
                chosen(Pointer {
                    left_handed,
                    ..pointer
                }),
            )
        })
        .collect();
    column![
        heading(look, "Primary button"),
        group(look, rows),
        note(look, BUTTON)
    ]
    .spacing(8)
    .into()
}

/// The mouse: its speed, whether it accelerates, and which way it scrolls.
fn the_mouse<'a>(look: Colors, pointer: Pointer) -> Element<'a, Message> {
    let with = move |mouse: Mouse| Pointer { mouse, ..pointer };
    let mouse = pointer.mouse;
    let rows = vec![
        setting(
            look,
            "Pointer speed",
            None,
            speed(
                look,
                mouse.speed,
                move |speed| dragged(with(Mouse { speed, ..mouse })),
                Message::Pointer(Changed::Dropped),
            ),
        ),
        setting(
            look,
            "Acceleration",
            Some(ACCELERATION),
            switch(look, mouse.acceleration, move |acceleration| {
                chosen(with(Mouse {
                    acceleration,
                    ..mouse
                }))
            }),
        ),
        setting(
            look,
            "Natural scrolling",
            Some(NATURAL),
            switch(look, mouse.natural, move |natural| {
                chosen(with(Mouse { natural, ..mouse }))
            }),
        ),
    ];
    column![heading(look, "Mouse"), group(look, rows)]
        .spacing(8)
        .into()
}

/// The touchpad: its speed, a tap, which way it scrolls and how, and a palm while typing.
fn the_touchpad<'a>(look: Colors, pointer: Pointer) -> Element<'a, Message> {
    let with = move |touchpad: Touchpad| Pointer {
        touchpad,
        ..pointer
    };
    let pad = pointer.touchpad;
    let rows = vec![
        setting(
            look,
            "Pointer speed",
            None,
            speed(
                look,
                pad.speed,
                move |speed| dragged(with(Touchpad { speed, ..pad })),
                Message::Pointer(Changed::Dropped),
            ),
        ),
        setting(
            look,
            "Tap to click",
            Some(TAP),
            switch(look, pad.tap, move |tap| {
                chosen(with(Touchpad { tap, ..pad }))
            }),
        ),
        setting(
            look,
            "Natural scrolling",
            Some(NATURAL),
            switch(look, pad.natural, move |natural| {
                chosen(with(Touchpad { natural, ..pad }))
            }),
        ),
        setting(
            look,
            "Edge scrolling",
            Some(EDGE),
            switch(look, pad.scrolling == Scrolling::Edge, move |edge| {
                let scrolling = if edge {
                    Scrolling::Edge
                } else {
                    Scrolling::TwoFingers
                };
                chosen(with(Touchpad { scrolling, ..pad }))
            }),
        ),
        setting(
            look,
            "Disable while typing",
            Some(TYPING),
            switch(look, pad.off_while_typing, move |off_while_typing| {
                chosen(with(Touchpad {
                    off_while_typing,
                    ..pad
                }))
            }),
        ),
    ];
    column![heading(look, "Touchpad"), group(look, rows)]
        .spacing(8)
        .into()
}

/// Under the primary button.
const BUTTON: &str = "The primary button clicks and the other one opens menus, on every mouse and \
                      touchpad. Right suits a left hand.";
/// Under acceleration.
const ACCELERATION: &str = "The pointer goes further when the mouse moves faster.";
/// Under natural scrolling.
const NATURAL: &str = "Scrolling moves the content, not the view.";
/// Under tap to click.
const TAP: &str = "A tap on the touchpad clicks.";
/// Under edge scrolling.
const EDGE: &str = "One finger along the right edge scrolls, in place of two fingers.";
/// Under disable while typing.
const TYPING: &str = "The touchpad ignores a palm while a key is typed.";

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(found: Result<Vec<Device>, String>) -> Settings {
        let mut state = Settings::bare();
        state.devices = Some(found);
        state
    }

    fn device(name: &str, kind: Kind) -> Device {
        Device {
            name: name.to_string(),
            kind,
        }
    }

    #[test]
    fn the_state_says_what_the_machine_has_and_every_setting() {
        assert!(state(&Settings::bare()).is_empty());
        let vm = settings(Ok(vec![
            device("ImExPS/2 Generic Explorer Mouse", Kind::Mouse),
            device("QEMU Virtio Tablet", Kind::Mouse),
        ]));
        let lines = state(&vm);
        assert_eq!(
            lines[..5],
            [
                "mice 2",
                "mouse ImExPS/2 Generic Explorer Mouse",
                "mouse QEMU Virtio Tablet",
                "touchpads 0",
                "primary-button left",
            ]
        );
        assert_eq!(lines.len(), 4 + devices::NAMES.len());
        let failed = settings(Err("Could not read the list.".to_string()));
        assert_eq!(state(&failed), ["mice none", "touchpads none"]);
    }

    #[test]
    fn a_setting_for_a_device_the_machine_has_not_got_does_nothing() {
        let vm = settings(Ok(vec![device("QEMU Virtio Tablet", Kind::Mouse)]));
        let natural = named(&vm, "mouse-natural-scrolling", "on").expect("a mouse");
        assert!(natural.mouse.natural);
        assert!(named(&vm, "tap-to-click", "off").is_none());
        assert!(named(&vm, "primary-button", "right").is_some_and(|next| next.left_handed));
        assert!(named(&vm, "mouse-speed", "fast").is_none());
        let bare = settings(Ok(Vec::new()));
        assert!(named(&bare, "primary-button", "right").is_none());
        let laptop = settings(Ok(vec![device(
            "SynPS/2 Synaptics TouchPad",
            Kind::Touchpad,
        )]));
        assert!(named(&laptop, "tap-to-click", "off").is_some_and(|next| !next.touchpad.tap));
        assert!(named(&laptop, "mouse-speed", "3").is_none());
    }

    #[test]
    fn the_sentences_are_sentences() {
        for sentence in [BUTTON, ACCELERATION, NATURAL, TAP, EDGE, TYPING] {
            assert!(sentence.ends_with('.'), "{sentence}");
            assert!(sentence.is_ascii(), "{sentence}");
        }
    }
}
