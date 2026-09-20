//! The Displays page: the screens Orbit found, what each one is, and how big it is drawn.
//!
//! The size lives in the host profile, which is root's, so Orbit writes it and the page asks over
//! the system bus. Orbit works out a size for every screen from how dense it is; what is chosen
//! here takes its place, on this machine, until it is changed again.

use std::thread;

use iced::futures::channel::oneshot;
use iced::widget::{column, text};
use iced::{Element, Fill, Task};
use librift::orbit::Output;

use crate::theme::Colors;
use crate::ui::{Message, Settings};
use crate::widgets::{GAP, TEXT_SIZE, choice, group, heading, note, setting};

/// The sizes a screen is drawn at, with the name of each and the sentence under it.
const SCALES: [(u32, &str, &str); 2] = [
    (1, "Normal", "Text and windows are drawn at their own size."),
    (
        2,
        "Twice as big",
        "For a dense screen, where everything at its own size is small.",
    ),
];

/// Write the size one screen is drawn at, on a thread of its own: Orbit puts it in the profile,
/// and the part of the compositor's config written after it makes the screen follow at once.
pub fn set_scale(connector: String, scale: u32) -> Task<Message> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let said = librift::orbit::set_display_scale(&connector, scale)
            .and_then(|()| librift::orbit::host())
            .and_then(|host| librift::orbit::write_horizon(&host.outputs).map(|()| host));
        let _ = sender.send(said);
    });
    Task::perform(receiver, |said| {
        Message::Scaled(said.unwrap_or_else(|_| Err("Orbit did not answer.".to_string())))
    })
}

/// The page.
pub fn view(state: &Settings, look: Colors) -> Element<'_, Message> {
    let mut page = column![].spacing(GAP).width(Fill);
    match &state.host {
        None => page = page.push(note(look, "Asking Orbit what the screens are.")),
        Some(Err(why)) => {
            page = page.push(note(
                look,
                "Orbit is not answering, so the screens are not here.",
            ));
            page = page.push(note(look, why));
        }
        Some(Ok(host)) if host.outputs.is_empty() => {
            page = page.push(note(look, "Orbit found no screen on this machine."));
        }
        Some(Ok(host)) => {
            for output in &host.outputs {
                page = page.push(screen(output, look));
            }
        }
    }
    if let Some(why) = &state.problem {
        page = page.push(text(why).size(TEXT_SIZE).color(look.error));
    }
    page.push(note(
        look,
        "Night light, and where the screens sit next to each other, are not in Settings yet.",
    ))
    .into()
}

/// One screen: what it is, and the size it is drawn at.
fn screen(output: &Output, look: Colors) -> Element<'_, Message> {
    let mut rows = vec![
        setting(look, "Resolution", None, said(look, pixels(output))),
        setting(look, "Size", None, said(look, size(output))),
    ];
    for (scale, label, under) in SCALES {
        rows.push(choice(
            look,
            label,
            Some(under),
            None,
            output.scale == scale,
            Message::Scale(output.connector.clone(), scale),
        ));
    }
    column![heading(look, &output.connector), group(look, rows)]
        .spacing(8)
        .into()
}

/// What a row is set to, at the right of it.
fn said<'a>(look: Colors, value: String) -> Element<'a, Message> {
    text(value).size(TEXT_SIZE).color(look.dim).into()
}

/// The mode the screen is in, in pixels.
fn pixels(output: &Output) -> String {
    if output.width == 0 || output.height == 0 {
        return "The screen does not say.".to_string();
    }
    format!("{} x {}", output.width, output.height)
}

/// How big the screen is, and how dense, which is what the size it is drawn at comes from.
fn size(output: &Output) -> String {
    if output.width_cm == 0 || output.height_cm == 0 {
        return "The screen does not say.".to_string();
    }
    let size = format!("{} by {} cm", output.width_cm, output.height_cm);
    match output.dpi() {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(dpi) => format!("{size}, {} dots per inch", dpi.round() as u32),
        None => size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(width: u32, height: u32, width_cm: u32, height_cm: u32) -> Output {
        Output {
            connector: "eDP-1".to_string(),
            width,
            height,
            width_cm,
            height_cm,
            scale: 1,
        }
    }

    #[test]
    fn a_screen_reads_as_its_mode_and_its_size() {
        let panel = output(2880, 1800, 30, 19);
        assert_eq!(pixels(&panel), "2880 x 1800");
        assert_eq!(size(&panel), "30 by 19 cm, 243 dots per inch");
        // the boot test's virtual screen, which is about as dense as an old monitor
        assert_eq!(
            size(&output(1280, 800, 32, 20)),
            "32 by 20 cm, 102 dots per inch"
        );
    }

    #[test]
    fn a_screen_that_gives_no_edid_says_so() {
        let bare = output(0, 0, 0, 0);
        assert_eq!(pixels(&bare), "The screen does not say.");
        assert_eq!(size(&bare), "The screen does not say.");
        // a size with no mode is still a size, and says nothing about how dense it is
        assert_eq!(size(&output(0, 0, 52, 29)), "52 by 29 cm");
    }

    #[test]
    fn both_sizes_are_offered_and_one_of_them_is_what_orbit_says() {
        assert_eq!(SCALES.map(|(scale, ..)| scale), [1, 2]);
        for (_, label, under) in SCALES {
            assert!(under.ends_with('.'), "{label}: {under}");
            assert!(label.is_ascii() && under.is_ascii());
        }
    }
}
