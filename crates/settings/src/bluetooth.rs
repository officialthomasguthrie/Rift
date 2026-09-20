//! The Bluetooth page: the adapter, and the devices that have been paired with this machine.
//! Connecting one and taking it off again is here; pairing a new one is not, and says so.
//!
//! A machine with no adapter runs no `BlueZ`, so the page says there is none rather than starting
//! a service that has nothing to look after.

use std::thread;

use iced::futures::channel::oneshot;
use iced::widget::{column, text};
use iced::{Element, Fill, Task};
use librift::bluetooth::{Device, Picture};

use crate::icons;
use crate::theme::Colors;
use crate::ui::{Message, Settings};
use crate::widgets::{GAP, TEXT_SIZE, choice, group, heading, note, setting, switch};

/// How big a device's icon is drawn.
const ICON: f32 = 16.0;

/// Turn the adapter on or off.
pub fn set_powered(state: &Settings, on: bool) -> Task<Message> {
    let Some(adapter) = adapter(state).map(|picture| picture.adapter.clone()) else {
        return Task::none();
    };
    acted(move || librift::bluetooth::set_powered(&adapter, on))
}

/// Connect the device at this place in the list, or disconnect the one that is connected.
pub fn connect(state: &mut Settings, at: usize) -> Task<Message> {
    let Some(device) = adapter(state).and_then(|picture| picture.devices.get(at).cloned()) else {
        return Task::none();
    };
    state.problem = None;
    state.doing = Some(format!(
        "{} {}",
        if device.connected {
            "Disconnecting"
        } else {
            "Connecting to"
        },
        device.name
    ));
    acted(move || librift::bluetooth::connect(&device.path, !device.connected))
}

/// Run something that asks `BlueZ` on a thread of its own, and say how it went.
fn acted(work: impl FnOnce() -> Result<(), String> + Send + 'static) -> Task<Message> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let _ = sender.send(work());
    });
    Task::perform(receiver, |said| {
        Message::Acted(said.unwrap_or_else(|_| Err("It stopped before it finished.".to_string())))
    })
}

/// The adapter, when the machine has one and `BlueZ` has answered.
fn adapter(state: &Settings) -> Option<&Picture> {
    state
        .bluetooth
        .as_ref()
        .and_then(|answered| answered.as_ref().ok())
        .and_then(Option::as_ref)
}

/// The place in the list of the paired device with this name, for `--set connect`.
#[must_use]
pub fn named(state: &Settings, name: &str) -> Option<usize> {
    let name = name.trim();
    adapter(state)?
        .devices
        .iter()
        .position(|device| device.name == name)
}

/// The lines `rift-settings --state` prints about Bluetooth, once `BlueZ` has answered.
#[must_use]
pub fn state(state: &Settings) -> Vec<String> {
    let Some(Ok(answered)) = state.bluetooth.as_ref() else {
        return Vec::new();
    };
    let Some(picture) = answered else {
        return vec!["bluetooth none".to_string()];
    };
    vec![
        format!("bluetooth {}", if picture.powered { "on" } else { "off" }),
        format!("devices {}", picture.devices.len()),
        format!(
            "connected {}",
            picture
                .devices
                .iter()
                .filter(|device| device.connected)
                .count()
        ),
    ]
}

/// The page.
pub fn view(state: &Settings, look: Colors) -> Element<'_, Message> {
    let mut page = column![].spacing(GAP).width(Fill);
    match &state.bluetooth {
        None => page = page.push(note(look, "Asking BlueZ what this machine has.")),
        Some(Err(why)) => {
            page = page.push(note(
                look,
                "BlueZ is not answering, so Bluetooth is not here.",
            ));
            page = page.push(note(look, why));
        }
        Some(Ok(None)) => {
            page = page.push(note(look, "This machine has no Bluetooth adapter."));
        }
        Some(Ok(Some(picture))) => {
            page = page.push(group(
                look,
                vec![setting(
                    look,
                    "Bluetooth",
                    None,
                    switch(look, picture.powered, Message::Power),
                )],
            ));
            if picture.powered {
                page = page.push(paired(look, picture));
            } else {
                page = page.push(note(
                    look,
                    "Bluetooth is off, so the devices paired with this machine are not listed.",
                ));
            }
        }
    }
    if let Some(why) = &state.problem {
        page = page.push(text(why).size(TEXT_SIZE).color(look.error));
    } else if let Some(doing) = &state.doing {
        page = page.push(note(look, doing));
    }
    page.push(note(
        look,
        "Pairing a new device is not in Settings yet: it takes a service to answer for the \
         machine while the two agree on a number, and Rift has none. bluetoothctl pairs one from \
         a terminal.",
    ))
    .into()
}

/// The devices paired with this machine, the connected ones first. A press connects one, or takes
/// off the one that is connected.
fn paired(look: Colors, picture: &Picture) -> Element<'_, Message> {
    if picture.devices.is_empty() {
        return column![
            heading(look, "Devices"),
            note(look, "No device is paired with this machine."),
        ]
        .spacing(8)
        .into();
    }
    let rows: Vec<Element<'_, Message>> = picture
        .devices
        .iter()
        .enumerate()
        .map(|(at, device)| {
            choice(
                look,
                &device.name,
                Some(if device.connected {
                    "Connected"
                } else {
                    "Paired, and not connected"
                }),
                Some(icons::symbolic(look.text, &icon(device), ICON)),
                device.connected,
                Message::Device(at),
            )
        })
        .collect();
    column![heading(look, "Devices"), group(look, rows)]
        .spacing(8)
        .into()
}

/// The icon for what a device is, when the theme has one for it, and the plain mark when it has
/// not.
fn icon(device: &Device) -> String {
    device
        .icon
        .as_ref()
        .map(|icon| format!("{icon}-symbolic"))
        .filter(|icon| icons::find(icon).is_some())
        .unwrap_or_else(|| "bluetooth-symbolic".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str, connected: bool) -> Device {
        Device {
            path: format!("/org/bluez/hci0/dev_{name}"),
            name: name.to_string(),
            icon: Some("audio-headphones".to_string()),
            connected,
        }
    }

    fn settings(answered: Option<Result<Option<Picture>, String>>) -> Settings {
        let mut state = Settings::bare();
        state.bluetooth = answered;
        state
    }

    #[test]
    fn a_machine_with_no_adapter_says_so_in_one_line() {
        assert_eq!(state(&settings(Some(Ok(None)))), ["bluetooth none"]);
        // nothing has answered yet, and a failure is on the page, not in the state
        assert!(state(&settings(None)).is_empty());
        assert!(state(&settings(Some(Err("BlueZ is not running.".into())))).is_empty());
    }

    #[test]
    fn an_adapter_prints_its_devices_and_how_many_are_connected() {
        let picture = Picture {
            adapter: "/org/bluez/hci0".to_string(),
            powered: true,
            devices: vec![device("Headphones", true), device("Speaker", false)],
        };
        let state_of = settings(Some(Ok(Some(picture.clone()))));
        assert_eq!(
            state(&state_of),
            ["bluetooth on", "devices 2", "connected 1"]
        );
        assert_eq!(named(&state_of, "Speaker"), Some(1));
        assert_eq!(named(&state_of, " Headphones "), Some(0));
        assert_eq!(named(&state_of, "Nothing"), None);
        let off = settings(Some(Ok(Some(Picture {
            powered: false,
            ..picture
        }))));
        assert_eq!(state(&off)[0], "bluetooth off");
    }

    #[test]
    fn a_device_draws_the_icon_of_what_it_is_or_the_plain_mark() {
        let mut headphones = device("Headphones", true);
        headphones.icon = Some("nothing-the-theme-has".to_string());
        assert_eq!(icon(&headphones), "bluetooth-symbolic");
        headphones.icon = None;
        assert_eq!(icon(&headphones), "bluetooth-symbolic");
    }
}
