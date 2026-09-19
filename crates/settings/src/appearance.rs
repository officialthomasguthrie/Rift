//! The Appearance page: the wallpaper, dark or light, the accent, the interface text size, the
//! gaps between windows, the corner radius of a window, the terminal colours and the terminal
//! greeting. Every change is written to the owner's files and handed to the apps, the compositor
//! and the shell at once.

use iced::widget::{button, column, container, row, text};
use iced::{Border, Center, Element, Fill, Length, Theme};
use librift::appearance::{
    Accent, GAPS_MOST, RADIUS_MOST, Scheme, TEXT_LEAST, TEXT_MOST, TEXT_STEP, Theme as Mode,
};
use librift::wallpaper::{self, Wallpaper};

use crate::theme::{Colors, hex};
use crate::ui::{Message, Settings, fill};
use crate::widgets::{GAP, MONO, TEXT_SIZE, choice, group, heading, note, setting, steps, switch};

/// How wide and tall a swatch of an accent colour is.
const SWATCH: f32 = 28.0;
/// How wide and tall the two letters of a terminal colour scheme are drawn in it.
const SAMPLE: (f32, f32) = (44.0, 22.0);

/// One wallpaper the page offers.
#[derive(Debug, Clone)]
pub struct Choice {
    /// The wallpaper itself.
    pub wallpaper: Wallpaper,
    /// What it shows, or the name of the colour.
    pub title: String,
    /// Who took the photograph, when it is one.
    pub credit: String,
    /// The word `rift wallpaper set` and the control socket take for it.
    pub word: String,
}

impl Choice {
    /// Whether a word names this one: its short name, or the setting itself.
    #[must_use]
    pub fn names(&self, word: &str) -> bool {
        let word = word.trim();
        word.eq_ignore_ascii_case(&self.word) || word == self.wallpaper.setting()
    }
}

/// The wallpapers to choose from: the photographs Rift ships, then the flat colours.
#[must_use]
pub fn choices() -> Vec<Choice> {
    let mut choices: Vec<Choice> = wallpaper::shipped()
        .into_iter()
        .map(|photo| Choice {
            wallpaper: Wallpaper::Picture(photo.path),
            title: photo.title,
            credit: photo.credit,
            word: photo.name,
        })
        .collect();
    for (colour, name) in wallpaper::GRAYS {
        choices.push(Choice {
            wallpaper: Wallpaper::Color(colour.to_string()),
            title: name.to_string(),
            credit: String::new(),
            word: colour.to_string(),
        });
    }
    choices
}

/// The page.
pub fn view(state: &Settings, look: Colors) -> Element<'_, Message> {
    let later = note(look, "The boot style comes with the rest of Settings.");
    let mut page = column![
        themes(state, look),
        accents(state, look),
        wallpapers(state, look),
        text_size(state, look),
        windows(state, look),
        terminal(state, look),
        later,
    ]
    .spacing(GAP)
    .width(Fill);
    if let Some(why) = &state.problem {
        page = page.push(text(why).size(TEXT_SIZE).color(look.error));
    }
    page.into()
}

/// The wallpapers: every photograph Rift ships, then the flat colours.
fn wallpapers(state: &Settings, look: Colors) -> Element<'_, Message> {
    let current = &state.look.wallpaper;
    let rows: Vec<Element<'_, Message>> = state
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
    let mut part = column![heading(look, "Wallpaper")].spacing(8);
    // a picture of the owner's own, set from the rift command, is not in the list
    if state.choices.iter().all(|one| &one.wallpaper != current) {
        part = part.push(note(
            look,
            "The desktop has a picture of your own. Choosing one below takes its place.",
        ));
    }
    part.push(group(look, rows)).into()
}

/// Dark or light.
fn themes(state: &Settings, look: Colors) -> Element<'_, Message> {
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
fn accents(state: &Settings, look: Colors) -> Element<'_, Message> {
    let inside = container(swatches(state.look.theme, state.look.accent, look))
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

/// The size of the text of the interface, which the apps and the shell both follow.
fn text_size(state: &Settings, look: Colors) -> Element<'_, Message> {
    let rows = vec![setting(
        look,
        "Interface text size",
        Some("Apps, the bar, the dock and the menus are all drawn at this size."),
        steps(
            look,
            TEXT_LEAST..=TEXT_MOST,
            TEXT_STEP,
            state.look.text,
            "%",
            Message::Text,
            Message::Wrote,
        ),
    )];
    column![heading(look, "Text"), group(look, rows)]
        .spacing(8)
        .into()
}

/// The gap between windows and the corner radius of one.
fn windows(state: &Settings, look: Colors) -> Element<'_, Message> {
    let rows = vec![
        setting(
            look,
            "Gap between windows",
            None,
            steps(
                look,
                0..=GAPS_MOST,
                1,
                state.look.gaps,
                "px",
                Message::Gaps,
                Message::Wrote,
            ),
        ),
        setting(
            look,
            "Corner radius",
            None,
            steps(
                look,
                0..=RADIUS_MOST,
                1,
                state.look.radius,
                "px",
                Message::Radius,
                Message::Wrote,
            ),
        ),
    ];
    column![heading(look, "Windows"), group(look, rows)]
        .spacing(8)
        .into()
}

/// The terminal colours and the greeting.
fn terminal(state: &Settings, look: Colors) -> Element<'_, Message> {
    let mut rows: Vec<Element<'_, Message>> = Scheme::ALL
        .into_iter()
        .map(|scheme| {
            choice(
                look,
                scheme.label(),
                None,
                Some(sample(scheme, look)),
                scheme == state.look.terminal,
                Message::Terminal(scheme),
            )
        })
        .collect();
    rows.push(setting(
        look,
        "Greeting",
        Some("The first shell of a session shows the logo and what the machine is."),
        switch(look, state.greeting, Message::Greeting),
    ));
    column![heading(look, "Terminal"), group(look, rows)]
        .spacing(8)
        .into()
}

/// Two letters in a colour scheme, drawn on it: what a terminal in it looks like.
fn sample<'a>(scheme: Scheme, look: Colors) -> Element<'a, Message> {
    let colour = hex(scheme.background());
    container(
        text("Ab")
            .size(TEXT_SIZE)
            .font(MONO)
            .color(hex(scheme.foreground())),
    )
    .center_x(Length::Fixed(SAMPLE.0))
    .center_y(Length::Fixed(SAMPLE.1))
    .style(move |_: &Theme| container::Style {
        background: Some(colour.into()),
        border: Border {
            color: look.line,
            width: 1.0,
            radius: 3.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}

/// The nine colours, each a square of itself, the one in use ringed.
fn swatches<'a>(mode: Mode, chosen: Accent, look: Colors) -> Element<'a, Message> {
    let mut swatches = row![].spacing(10).align_y(Center);
    for accent in Accent::ALL {
        let colour = hex(accent.hex(mode));
        let here = accent == chosen;
        swatches = swatches.push(
            button(
                container(
                    iced::widget::space()
                        .width(Length::Fixed(SWATCH))
                        .height(Length::Fixed(SWATCH)),
                )
                .style(move |_: &Theme| fill(colour)),
            )
            .padding(2)
            .on_press(Message::Accent(accent))
            .style(move |_: &Theme, status| button::Style {
                background: None,
                text_color: look.text,
                border: Border {
                    color: match (here, status) {
                        (true, _) => look.text,
                        (false, button::Status::Hovered | button::Status::Pressed) => look.edge,
                        _ => iced::Color::TRANSPARENT,
                    },
                    width: 2.0,
                    radius: 6.0.into(),
                },
                ..button::Style::default()
            }),
        );
    }
    swatches.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flat_colours_are_in_the_list_after_the_photographs() {
        // no photographs are installed while the tests run, so the list is the colours alone
        let choices = choices();
        assert_eq!(choices.len(), wallpaper::GRAYS.len());
        for (at, (colour, name)) in wallpaper::GRAYS.iter().enumerate() {
            assert_eq!(
                choices[at].wallpaper,
                Wallpaper::Color((*colour).to_string())
            );
            assert_eq!(choices[at].title, *name);
            assert!(choices[at].credit.is_empty());
            assert!(choices[at].names(colour));
            assert!(choices[at].names(&format!(" {colour} ")));
            assert!(!choices[at].names("earthset"));
        }
    }
}
