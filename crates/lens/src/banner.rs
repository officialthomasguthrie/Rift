//! A notification on screen: a rectangle under the bar at the right, on the menu's gray inside a
//! border like every other surface of the shell, with the app's icon, the summary in bold, the
//! body in the dim gray in at most three lines, the notification's buttons, and a button that
//! closes it. The words are cut to fit when the notification comes in, so the surface is exactly
//! as tall as what it shows.

use std::path::{Path, PathBuf};

use iced::advanced::graphics::text::Paragraph;
use iced::advanced::text::{Alignment, LineHeight, Paragraph as _, Shaping, Text, Wrapping};
use iced::alignment::Vertical;
use iced::widget::{button, column, container, mouse_area, row, space, text};
use iced::{Border, Element, Font, Length, Pixels, Size, Theme};

use crate::bar;
use crate::icons;
use crate::launcher::App;
use crate::notice::{Banner, Notification};
use crate::system;
use crate::theme::Palette;
use crate::ui::{FONT, HEADING, Message};

/// How wide a notification is in logical pixels.
pub const WIDTH: u32 = 380;
/// The padding inside it.
pub const PAD: u32 = 12;
/// The app's icon.
pub const ICON: u32 = 24;
/// The gap between the icon and the words.
const ICON_GAP: u32 = 12;
/// The line the summary takes, as tall as the icon beside it.
const TITLE: u32 = 24;
/// One line of the body.
const LINE: u32 = 20;
/// The same line where a length is wanted.
const LINE_HEIGHT: f32 = 20.0;
/// The most lines of the body it shows.
pub const LINES: u32 = 3;
/// The button that closes it, at its top right.
pub const CLOSE: u32 = 24;
/// The gap between the words and that button.
const CLOSE_GAP: u32 = 8;
/// A button for one of its actions.
pub const BUTTON: u32 = 28;
/// The gap over the buttons and between two of them.
const BUTTON_GAP: u32 = 8;
/// The symbol in the close button.
const SYMBOL: f32 = 16.0;
/// The corner of a button.
const RADIUS: f32 = 4.0;
/// What is put after words that were cut.
const CUT: &str = "...";

/// What a notification on screen asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A click on the notification itself: its default action, and it closes.
    Press(u32),
    /// The button that closes it.
    Close(u32),
    /// One of its buttons, by the action's key.
    Action(u32, String),
    /// The pointer came over it, or left it.
    Hover(u32, bool),
}

/// How wide the words are, beside the icon when there is one and beside the close button.
#[must_use]
pub fn text_width(icon: bool) -> f32 {
    let taken = 2 * PAD + CLOSE + CLOSE_GAP + if icon { ICON + ICON_GAP } else { 0 };
    to_length(WIDTH - taken)
}

/// How tall a notification is with this many lines of body and this many buttons.
#[must_use]
pub const fn height(lines: u32, buttons: usize) -> u32 {
    let under = if buttons > 0 { BUTTON_GAP + BUTTON } else { 0 };
    PAD + TITLE + lines * LINE + under + PAD
}

/// The summary on one line and the body in at most three, cut to fit, and how many lines the
/// body takes.
#[must_use]
pub fn texts(notification: &Notification, icon: bool) -> (String, String, u32) {
    let width = text_width(icon);
    let (summary, _) = fit(&notification.summary, HEADING, width, 1);
    let (body, lines) = fit(&notification.body, FONT, width, LINES);
    (summary, body, lines)
}

/// The icon a notification shows: the image or icon it names, as a name in the icon themes, its
/// symbolic drawing, or a file, and otherwise the icon of the desktop entry it says it is from.
#[must_use]
pub fn icon(apps: &[App], notification: &Notification) -> Option<PathBuf> {
    let named = notification
        .icon_name()
        .and_then(|name| icons::app(name).or_else(|| icons::find(&format!("{name}-symbolic"))));
    named.or_else(|| {
        let entry = notification.entry.as_deref()?;
        let app = apps.iter().find(|app| app.id == entry)?;
        icons::app(app.icon.as_deref()?)
    })
}

/// Words cut to what fits in `most` lines of this width, with `...` after them when they were
/// cut, and how many lines they take.
#[must_use]
pub fn fit(words: &str, font: Font, width: f32, most: u32) -> (String, u32) {
    let whole = lines(words, font, width);
    if whole <= most {
        return (words.to_string(), whole);
    }
    // the longest start of the words that still fits with the mark after it, found by halves
    let ends: Vec<usize> = words
        .char_indices()
        .map(|(at, _)| at)
        .skip(1)
        .chain([words.len()])
        .collect();
    let (mut shortest, mut longest) = (0, ends.len());
    while shortest < longest {
        let middle = (shortest + longest).div_ceil(2);
        if lines(&cut(words, ends[middle - 1]), font, width) <= most {
            shortest = middle;
        } else {
            longest = middle - 1;
        }
    }
    let fitted = if shortest == 0 {
        CUT.to_string()
    } else {
        cut(words, ends[shortest - 1])
    };
    let taken = lines(&fitted, font, width).min(most);
    (fitted, taken)
}

/// The start of the words up to a byte, with the mark after it.
fn cut(words: &str, end: usize) -> String {
    format!("{}{CUT}", words[..end].trim_end())
}

/// How many lines words take at this width, laid out the way the text widget lays them out.
fn lines(words: &str, font: Font, width: f32) -> u32 {
    if words.is_empty() {
        return 0;
    }
    let paragraph = Paragraph::with_text(Text {
        content: words,
        bounds: Size::new(width, f32::INFINITY),
        size: Pixels(bar::TEXT_SIZE),
        line_height: LineHeight::Absolute(Pixels(LINE_HEIGHT)),
        font,
        align_x: Alignment::Default,
        align_y: Vertical::Top,
        shaping: Shaping::default(),
        wrapping: Wrapping::WordOrGlyph,
    });
    let tall = (paragraph.min_bounds().height / LINE_HEIGHT).round();
    // a paragraph is never taller than a few thousand lines of a notification's words
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = tall.clamp(1.0, 10_000.0) as u32;
    count
}

/// A length in logical pixels from a count of them.
fn to_length(pixels: u32) -> f32 {
    f32::from(u16::try_from(pixels).unwrap_or(u16::MAX))
}

/// The notification: the icon, the words and the buttons on the menu's gray inside a border. A
/// click anywhere that is not a button is a press on the notification itself.
pub fn view(look: Palette, banner: &Banner) -> Element<'_, Message> {
    let id = banner.notification.id;
    let fitted = &banner.fitted;
    let width = text_width(fitted.icon.is_some());
    let summary = container(
        text(fitted.summary.as_str())
            .size(bar::TEXT_SIZE)
            .font(HEADING)
            .color(look.text)
            .wrapping(Wrapping::None),
    )
    .width(width)
    .height(to_length(TITLE))
    .align_y(iced::Center)
    .clip(true);
    let mut words = column![summary].width(width);
    if fitted.lines > 0 {
        words = words.push(
            container(
                text(fitted.body.as_str())
                    .size(bar::TEXT_SIZE)
                    .line_height(LineHeight::Absolute(Pixels(LINE_HEIGHT)))
                    .color(look.dim)
                    .wrapping(Wrapping::WordOrGlyph),
            )
            .width(width)
            .height(to_length(fitted.lines * LINE))
            .clip(true),
        );
    }
    if !banner.notification.actions.is_empty() {
        let mut buttons = row![].spacing(to_length(BUTTON_GAP)).width(width);
        for (key, label) in &banner.notification.actions {
            buttons = buttons.push(action(look, id, key, label));
        }
        words = words
            .push(space().height(to_length(BUTTON_GAP)))
            .push(buttons);
    }
    let mut line = row![];
    if let Some(path) = &fitted.icon {
        line = line
            .push(picture(look, path))
            .push(space().width(to_length(ICON_GAP)));
    }
    line = line
        .push(words)
        .push(space().width(to_length(CLOSE_GAP)))
        .push(close(look, id));
    let inside = container(line)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(to_length(PAD))
        .style(move |_: &Theme| container::Style {
            background: Some(look.menu.into()),
            text_color: Some(look.text),
            border: Border {
                color: look.edge,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });
    mouse_area(inside)
        .on_press(Message::Banner(Event::Press(id)))
        .on_enter(Message::Banner(Event::Hover(id, true)))
        .on_exit(Message::Banner(Event::Hover(id, false)))
        .into()
}

/// The app's icon at the left, drawn as it is found: a symbolic one in the text colour.
fn picture<'a>(look: Palette, path: &Path) -> Element<'a, Message> {
    icons::picture(look.text, path, to_length(ICON))
}

/// The button at the top right that closes the notification.
fn close(look: Palette, id: u32) -> Element<'static, Message> {
    let symbol =
        container(icons::symbolic(look.dim, "window-close-symbolic", SYMBOL)).center(Length::Fill);
    button(symbol)
        .width(to_length(CLOSE))
        .height(to_length(CLOSE))
        .padding(0)
        .on_press(Message::Banner(Event::Close(id)))
        .style(move |_: &Theme, state| system::fill(look, state))
        .into()
}

/// A button for one of the notification's actions. The buttons share the width of the words
/// between them, the way GNOME lays them out.
fn action<'a>(look: Palette, id: u32, key: &str, label: &str) -> Element<'a, Message> {
    let word = container(
        text(label.to_string())
            .size(bar::TEXT_SIZE)
            .color(look.text)
            .wrapping(Wrapping::None),
    )
    .center(Length::Fill)
    .clip(true);
    button(word)
        .width(Length::FillPortion(1))
        .height(to_length(BUTTON))
        .padding([0, 8])
        .on_press(Message::Banner(Event::Action(id, key.to_string())))
        .style(move |_: &Theme, state| {
            let mut style = system::fill(look, state);
            style.border = Border {
                color: look.edge,
                width: 1.0,
                radius: RADIUS.into(),
            };
            style
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_height_follows_the_lines_and_the_buttons() {
        assert_eq!(height(0, 0), PAD + TITLE + PAD);
        assert_eq!(height(1, 0), 68);
        assert_eq!(height(LINES, 2), 144);
        // three of the tallest fit between the bar and the dock of a 768 px screen
        assert!(3 * height(LINES, 2) + 4 * 8 <= 768 - 32 - 44);
    }

    #[test]
    fn the_words_are_as_wide_as_what_is_left() {
        let total = |icon: bool| {
            text_width(icon)
                + to_length(2 * PAD + CLOSE + CLOSE_GAP)
                + if icon {
                    to_length(ICON + ICON_GAP)
                } else {
                    0.0
                }
        };
        assert!((total(true) - to_length(WIDTH)).abs() < f32::EPSILON);
        assert!((total(false) - to_length(WIDTH)).abs() < f32::EPSILON);
        assert!((text_width(true) - 288.0).abs() < f32::EPSILON);
    }

    #[test]
    fn nothing_to_say_takes_no_lines() {
        assert_eq!(fit("", FONT, 288.0, LINES), (String::new(), 0));
        assert_eq!(cut("A long line", 6), "A long...");
        assert_eq!(cut("A long line", 7), "A long...");
    }
}
