//! The Appearance page: dark or light, the accent and the wallpaper. They are three of the
//! settings Settings' own Appearance page writes, written the same way: into the owner's files,
//! and handed to the apps, the compositor and the shell at once. The window takes the new colours
//! as they are chosen.

use iced::widget::{column, container, text};
use iced::{Border, Element, Fill, Theme};
use librift::appearance::Theme as Mode;

use crate::theme::Colors;
use crate::ui::{Message, Welcome};
use crate::widgets::{GAP, TEXT_SIZE, choice, group, heading, note, swatches};

/// The page.
pub fn view(state: &Welcome, look: Colors) -> Element<'_, Message> {
    let mut page = column![
        note(
            look,
            "Each of these can be changed later in Settings, which also has the text size, the \
             windows, the terminal colours and how the drive starts.",
        ),
        themes(state, look),
        accents(state, look),
        wallpapers(state, look),
    ]
    .spacing(GAP)
    .width(Fill);
    if let Some(why) = &state.problem {
        page = page.push(text(why).size(TEXT_SIZE).color(look.error));
    }
    page.into()
}

/// Write the look the owner has just changed, and tell the shell to draw with it.
pub fn wrote(state: &mut Welcome) {
    state.problem = state.look.save().err();
    let _ = std::process::Command::new("lens")
        .arg("--look")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// Dark or light.
fn themes(state: &Welcome, look: Colors) -> Element<'_, Message> {
    let rows = [Mode::Dark, Mode::Light]
        .into_iter()
        .map(|mode| {
            choice(
                look,
                mode.label(),
                None,
                None,
                mode == state.look.theme,
                Message::Mode(mode),
            )
        })
        .collect();
    column![heading(look, "Theme"), group(look, rows)]
        .spacing(8)
        .into()
}

/// The nine accents, in a box of their own.
fn accents(state: &Welcome, look: Colors) -> Element<'_, Message> {
    let inside = container(swatches(
        look,
        state.look.theme,
        state.look.accent,
        Message::Accent,
    ))
    .width(Fill)
    .padding([10, 12])
    .style(move |_: &Theme| container::Style {
        background: Some(look.view.into()),
        border: Border {
            color: look.line,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..container::Style::default()
    });
    column![heading(look, "Accent colour"), inside]
        .spacing(8)
        .into()
}

/// The wallpapers: every photograph Rift ships, then the flat colours.
fn wallpapers(state: &Welcome, look: Colors) -> Element<'_, Message> {
    let current = &state.look.wallpaper;
    let rows = state
        .choices
        .iter()
        .enumerate()
        .map(|(at, one)| {
            choice(
                look,
                &one.title,
                (!one.credit.is_empty()).then_some(one.credit.as_str()),
                None,
                &one.wallpaper == current,
                Message::Wallpaper(at),
            )
        })
        .collect();
    column![heading(look, "Wallpaper"), group(look, rows)]
        .spacing(8)
        .into()
}
