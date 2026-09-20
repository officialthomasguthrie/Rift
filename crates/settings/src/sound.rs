//! The Sound page: what the machine plays through and what it listens with, each with its volume,
//! a mute switch and the devices there are to pick between.
//!
//! `PipeWire` answers through `wpctl`, which is what the shell's system menu asks with, and the
//! page follows it while it is open, so a headset that is plugged in turns up on the list without
//! anyone opening the page again.

use std::thread;

use iced::futures::channel::oneshot;
use iced::widget::{column, text};
use iced::{Element, Fill, Task};
use librift::sound::{self, Picture, Side};

use crate::theme::Colors;
use crate::ui::{Message, Settings};
use crate::widgets::{GAP, TEXT_SIZE, choice, group, heading, note, setting, steps, switch};

/// Each half of the sound: its heading, the name of its volume row, and what a machine with no
/// device for it says.
const HALVES: [(Side, &str, &str, &str); 2] = [
    (
        Side::Output,
        "Output",
        "Volume",
        "This machine has nothing to play sound through.",
    ),
    (
        Side::Input,
        "Input",
        "Input volume",
        "This machine has nothing to listen with.",
    ),
];

/// What `PipeWire` said, when it has answered.
fn picture(state: &Settings) -> Option<&Picture> {
    state
        .sound
        .as_ref()
        .and_then(|answered| answered.as_ref().ok())
}

/// What a half's slider stands at: where it is being dragged to, or the level `PipeWire` gives.
fn level(state: &Settings, side: Side) -> Option<u32> {
    if let Some((dragged, level)) = state.moving
        && dragged == side
    {
        return Some(level);
    }
    picture(state)
        .and_then(|picture| picture.volume(side))
        .map(|volume| u32::from(volume.level.min(100)))
}

/// Write the level a half's slider was let go at. The level travels with the message rather than
/// being read back out of the window, so a reading that lands between the last move and the letting
/// go does not lose it. A machine with no device for that half has no slider on the page, so it has
/// none from a terminal either.
pub fn set_volume(state: &mut Settings, side: Side, level: u32) -> Task<Message> {
    if state.moving.is_some_and(|(dragged, _)| dragged == side) {
        state.moving = None;
    }
    if !has(state, side) {
        return Task::none();
    }
    let percent = u8::try_from(level.min(100)).unwrap_or(100);
    wrote(move || sound::set_volume(side, percent))
}

/// Mute a half or unmute it.
pub fn set_muted(state: &Settings, side: Side, muted: bool) -> Task<Message> {
    if !has(state, side) {
        return Task::none();
    }
    wrote(move || sound::set_muted(side, muted))
}

/// Use the device at this place in a half's list.
pub fn pick(state: &Settings, side: Side, at: usize) -> Task<Message> {
    let Some(device) = picture(state).and_then(|picture| picture.devices(side).get(at)) else {
        return Task::none();
    };
    if device.default {
        return Task::none();
    }
    let id = device.id;
    wrote(move || sound::set_default(id))
}

/// Whether this half has a device at all.
fn has(state: &Settings, side: Side) -> bool {
    picture(state).is_some_and(|picture| !picture.devices(side).is_empty())
}

/// The place in a half's list of the device with this name, for `--set output` and `--set input`.
#[must_use]
pub fn named(state: &Settings, side: Side, name: &str) -> Option<usize> {
    let name = name.trim();
    picture(state)?
        .devices(side)
        .iter()
        .position(|device| device.name == name)
}

/// Change something about the sound on a thread of its own, then read `PipeWire` back, so the page
/// follows a change it made itself even where `pw-mon` is not running.
///
/// The reading is asked for inside the closure rather than beside the writing: a task built now
/// would start its thread now, and read `PipeWire` while the write was still on its way to it.
fn wrote(work: impl FnOnce() -> Result<(), String> + Send + 'static) -> Task<Message> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let _ = sender.send(work());
    });
    Task::perform(receiver, |said| {
        said.unwrap_or_else(|_| Err("It stopped before it finished.".to_string()))
    })
    .then(|said| Task::done(Message::Acted(said)).chain(read()))
}

/// Read `PipeWire` again, on a thread of its own.
fn read() -> Task<Message> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let _ = sender.send(sound::read());
    });
    Task::perform(receiver, |said| {
        Message::Sound(Box::new(said.unwrap_or_else(|_| {
            Err("PipeWire stopped before it answered.".to_string())
        })))
    })
}

/// The lines `rift-settings --state` prints about the sound, once `PipeWire` has answered: the
/// level and the mute of each half, the device each is using, and how many there are to pick from.
#[must_use]
pub fn state(state: &Settings) -> Vec<String> {
    let Some(picture) = picture(state) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for (side, ..) in HALVES {
        let (volume, mute, devices) = match side {
            Side::Output => ("volume", "mute", "outputs"),
            Side::Input => ("input-volume", "input-mute", "inputs"),
        };
        match picture.volume(side) {
            Some(loudness) => {
                lines.push(format!("{volume} {}", loudness.level));
                lines.push(format!(
                    "{mute} {}",
                    if loudness.muted { "on" } else { "off" }
                ));
            }
            None => lines.push(format!("{volume} none")),
        }
        lines.push(format!(
            "{} {}",
            side.word(),
            picture
                .using(side)
                .map_or("none", |device| device.name.as_str())
        ));
        lines.push(format!("{devices} {}", picture.devices(side).len()));
    }
    lines
}

/// The page.
pub fn view(state: &Settings, look: Colors) -> Element<'_, Message> {
    let mut page = column![].spacing(GAP).width(Fill);
    match &state.sound {
        None => return note(look, "Asking PipeWire what this machine has."),
        Some(Err(why)) => {
            page = page.push(note(
                look,
                "PipeWire is not answering, so the sound is not here.",
            ));
            page = page.push(note(look, why));
        }
        Some(Ok(picture)) => {
            for (side, name, loudness, nothing) in HALVES {
                page = page.push(half(state, look, picture, (side, name, loudness, nothing)));
            }
        }
    }
    if let Some(why) = &state.problem {
        page = page.push(text(why).size(TEXT_SIZE).color(look.error));
    }
    page.push(note(
        look,
        "The volume of one app on its own, and which device an app plays through, are not in \
         Settings yet. The mixer in the system menu is the volume of the whole machine.",
    ))
    .into()
}

/// One half of the sound: its volume, its mute switch and its devices, the one in use marked.
fn half<'a>(
    state: &'a Settings,
    look: Colors,
    picture: &'a Picture,
    (side, name, loudness, nothing): (Side, &'a str, &'a str, &'a str),
) -> Element<'a, Message> {
    let devices = picture.devices(side);
    if devices.is_empty() {
        return column![heading(look, name), note(look, nothing)]
            .spacing(8)
            .into();
    }
    let mut rows = Vec::new();
    if let Some(volume) = picture.volume(side) {
        let standing = level(state, side).unwrap_or(0);
        rows.push(setting(
            look,
            loudness,
            None,
            steps(
                look,
                0..=100,
                1,
                standing,
                "%",
                move |level| Message::Volume(side, level),
                Message::Volumed(side, standing),
            ),
        ));
        rows.push(setting(
            look,
            "Mute",
            None,
            switch(look, volume.muted, move |muted| Message::Muted(side, muted)),
        ));
    }
    for (at, device) in devices.iter().enumerate() {
        rows.push(choice(
            look,
            &device.name,
            None,
            None,
            device.default,
            Message::Pick(side, at),
        ));
    }
    column![heading(look, name), group(look, rows)]
        .spacing(8)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use librift::sound::{Device, Volume};

    fn device(id: u32, name: &str, default: bool) -> Device {
        Device {
            id,
            name: name.to_string(),
            default,
        }
    }

    fn settings(answered: Option<Result<Picture, String>>) -> Settings {
        let mut state = Settings::bare();
        state.sound = answered;
        state
    }

    fn card() -> Picture {
        Picture {
            output: Some(Volume {
                level: 40,
                muted: false,
            }),
            input: Some(Volume {
                level: 100,
                muted: true,
            }),
            outputs: vec![
                device(51, "Built-in Audio Analog Stereo", true),
                device(52, "HDMI / DisplayPort", false),
            ],
            inputs: vec![device(53, "Built-in Audio Analog Stereo", true)],
        }
    }

    #[test]
    fn a_card_prints_both_halves_with_the_device_each_is_using() {
        let kept = settings(Some(Ok(card())));
        assert_eq!(
            state(&kept),
            [
                "volume 40",
                "mute off",
                "output Built-in Audio Analog Stereo",
                "outputs 2",
                "input-volume 100",
                "input-mute on",
                "input Built-in Audio Analog Stereo",
                "inputs 1",
            ]
        );
        assert_eq!(named(&kept, Side::Output, " HDMI / DisplayPort "), Some(1));
        assert_eq!(named(&kept, Side::Output, "Nothing"), None);
        assert_eq!(named(&kept, Side::Input, "HDMI / DisplayPort"), None);
    }

    #[test]
    fn a_machine_with_no_sound_says_none_and_writes_nothing() {
        // nothing has answered yet, and a failure is on the page, not in the state
        assert!(state(&settings(None)).is_empty());
        assert!(state(&settings(Some(Err("wpctl is not there.".into())))).is_empty());
        let silent = settings(Some(Ok(Picture::default())));
        assert_eq!(
            state(&silent),
            [
                "volume none",
                "output none",
                "outputs 0",
                "input-volume none",
                "input none",
                "inputs 0",
            ]
        );
        assert!(!has(&silent, Side::Output));
        assert!(!has(&silent, Side::Input));
    }

    #[test]
    fn the_slider_shows_where_it_is_being_dragged_and_then_what_pipewire_says() {
        let mut kept = settings(Some(Ok(card())));
        assert_eq!(level(&kept, Side::Output), Some(40));
        kept.moving = Some((Side::Output, 75));
        assert_eq!(level(&kept, Side::Output), Some(75));
        // the other half is left where it stands
        assert_eq!(level(&kept, Side::Input), Some(100));
    }
}
