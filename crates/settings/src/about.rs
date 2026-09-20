//! The About page: the mark, what the system calls itself, and what Orbit knows about the machine
//! it is running on.

use std::fs;
use std::path::Path;
use std::thread;

use iced::futures::channel::oneshot;
use iced::widget::{column, container, image, row, text};
use iced::{Center, Element, Fill, Length, Task};
use librift::orbit::Host;
use librift::{paths, release};

use crate::theme::Colors;
use crate::ui::{Message, Settings};
use crate::widgets::{BOLD, GAP, TEXT_SIZE, TITLE_SIZE, group, line, note};

/// How wide the mark is drawn. It is a wide drawing, so this is about sixty pixels tall.
const MARK: f32 = 180.0;

/// What the system calls itself, from os-release, or the version of this build anywhere else.
#[must_use]
pub fn release() -> String {
    release::name(&fs::read_to_string(release::PATH).unwrap_or_default())
}

/// The value of one os-release key, when the file has it.
fn field(key: &str) -> Option<String> {
    release::value(&fs::read_to_string(release::PATH).unwrap_or_default(), key)
        .filter(|value| !value.is_empty())
}

/// Ask Orbit about this machine on a thread of its own: the system bus takes a moment, and the
/// window opens without waiting for it.
pub fn ask_orbit() -> Task<Message> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let _ = sender.send(librift::orbit::host());
    });
    Task::perform(receiver, |answered| {
        Message::Host(answered.unwrap_or_else(|_| Err("Orbit did not answer.".to_string())))
    })
}

/// The page.
pub fn view<'a>(state: &'a Settings, look: Colors) -> Element<'a, Message> {
    let mut page = column![mark(), name(state, look)]
        .spacing(GAP)
        .align_x(Center)
        .width(Fill);
    let mut rows: Vec<Element<'a, Message>> = Vec::new();
    if let Some(build) = field("BUILD_ID") {
        rows.push(fact(look, "Build", build));
    }
    if let Some(version) = field("IMAGE_VERSION") {
        rows.push(fact(look, "Image", version));
    }
    let mut why = None;
    match &state.host {
        None => rows.push(fact(look, "Machine", "Asking Orbit.".to_string())),
        Some(Err(said)) => why = Some(said.as_str()),
        Some(Ok(host)) => {
            rows.push(fact(look, "Machine", fingerprint(&host.fingerprint)));
            rows.push(fact(look, "This machine is", class(&host.class)));
            rows.push(fact(look, "Screens", screens(host)));
            rows.push(fact(look, "Graphics", host.gpu_path.clone()));
            rows.push(fact(look, "AI", format!("{} models", host.ai_tier)));
        }
    }
    page = page.push(group(look, rows));
    if let Some(why) = why {
        page = page.push(note(
            look,
            "Orbit is not answering, so what this machine is is not here.",
        ));
        page = page.push(note(look, why));
    }
    page.push(note(
        look,
        "Rift is free software under the GPL, version 3 or later.",
    ))
    .into()
}

/// The mark, when the image has it.
fn mark<'a>() -> Element<'a, Message> {
    let path = Path::new(paths::LOGO_MARK);
    if path.is_file() {
        container(image(image::Handle::from_path(path)).width(Length::Fixed(MARK)))
            .padding([GAP, 0.0])
            .into()
    } else {
        container(iced::widget::space().height(0.0)).into()
    }
}

/// The name and the version, as os-release has them.
fn name(state: &Settings, look: Colors) -> Element<'_, Message> {
    text(&state.release)
        .size(TITLE_SIZE)
        .font(BOLD)
        .color(look.text)
        .into()
}

/// One row: what it is at the left, what it says at the right.
fn fact(look: Colors, label: &str, value: String) -> Element<'_, Message> {
    container(
        row![
            container(line(look, label)).width(Length::Fixed(150.0)),
            text(value).size(TEXT_SIZE).color(look.dim).width(Fill),
        ]
        .align_y(Center)
        .spacing(GAP),
    )
    .width(Fill)
    .padding([8, 12])
    .into()
}

/// The first half of the fingerprint. The whole of it is a sha256 in hex, which no one reads off a
/// page, and half of it is still this machine and no other.
fn fingerprint(whole: &str) -> String {
    let short: String = whole.chars().take(32).collect();
    if short.is_empty() {
        "Not fingerprinted yet.".to_string()
    } else {
        short
    }
}

/// What the class means, in the words the goal uses.
fn class(word: &str) -> String {
    match word {
        "owned" => "yours, and it may keep a key".to_string(),
        "trusted" => "trusted, and it may remember you".to_string(),
        "borrowed" => "borrowed, and it keeps nothing".to_string(),
        other => other.to_string(),
    }
}

/// Every screen Orbit found, by its connector and its size.
fn screens(host: &Host) -> String {
    if host.outputs.is_empty() {
        return "None".to_string();
    }
    host.outputs
        .iter()
        .map(|output| {
            format!(
                "{} {}x{} at scale {}",
                output.connector, output.width, output.height, output.scale
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use librift::orbit::Output;

    #[test]
    fn a_fingerprint_is_shown_by_its_first_half() {
        let whole = "a".repeat(64);
        assert_eq!(fingerprint(&whole).len(), 32);
        assert!(whole.starts_with(&fingerprint(&whole)));
        assert_eq!(fingerprint(""), "Not fingerprinted yet.");
    }

    #[test]
    fn a_class_is_said_in_words() {
        assert!(class("owned").starts_with("yours"));
        assert!(class("borrowed").contains("keeps nothing"));
        assert_eq!(class("something else"), "something else");
    }

    #[test]
    fn the_screens_are_listed_by_connector() {
        let host = Host {
            fingerprint: String::new(),
            class: "owned".to_string(),
            outputs: vec![
                Output {
                    connector: "Virtual-1".to_string(),
                    width: 1280,
                    height: 800,
                    width_cm: 32,
                    height_cm: 20,
                    scale: 1,
                },
                Output {
                    connector: "eDP-1".to_string(),
                    width: 2880,
                    height: 1800,
                    width_cm: 30,
                    height_cm: 19,
                    scale: 2,
                },
            ],
            gpu_path: "none".to_string(),
            ai_tier: "small".to_string(),
        };
        assert_eq!(
            screens(&host),
            "Virtual-1 1280x800 at scale 1, eDP-1 2880x1800 at scale 2"
        );
        assert_eq!(
            screens(&Host {
                outputs: vec![],
                ..host
            }),
            "None"
        );
    }
}
