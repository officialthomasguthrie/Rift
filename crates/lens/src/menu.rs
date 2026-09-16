//! The Applications menu: the surface that hangs under the Applications button. It holds the
//! field and, under it, what the field matched, printed or answered. The app list goes under the
//! field next.

use iced::widget::{column, container, text, text_input};
use iced::{Border, Element, Font, Length, Theme, window};
use librift::os::Action;

use crate::bar;
use crate::launcher::App;
use crate::route;
use crate::theme::Palette;
use crate::ui::{FONT, MONO, Message};

/// How wide the menu is in logical pixels.
pub const WIDTH: u32 = 496;
/// The padding inside the menu, and the margin that lines its left edge up with the button.
pub const PAD: u32 = 8;
/// The same padding where a widget wants it.
const INSIDE: u16 = 8;
/// The gap between the field, the rows and the line under them.
const GAP: u32 = 4;
/// The field inside the menu.
pub const FIELD_WIDTH: f32 = 480.0;
/// How tall the field is: the line of text plus its padding.
pub const FIELD_HEIGHT: u32 = 32;
/// The height of one row of the list, and of the line under it.
pub const ROW_HEIGHT: u32 = 28;
/// How many rows the list shows at once.
pub const ROWS: usize = 8;
/// The line of text inside the field, so its height is the same everywhere.
const FIELD_LINE: f32 = 18.0;
/// The padding inside the field: 18 + 7 + 7 is the field's height.
const FIELD_PAD: u16 = 7;
/// The field's widget id, for the focus operation.
const FIELD_ID: &str = "field";
/// What the field says when it is empty.
const PLACEHOLDER: &str = "Type an app, a command or a question";

/// What the list under the field is showing.
#[derive(Debug)]
pub enum Results {
    /// Nothing, and the menu is only the field high.
    None,
    /// The apps the words match, best first. One of them is selected.
    Matches(Vec<App>),
    /// What a command or a pipeline printed.
    Output(Vec<String>),
    /// Quasar's answer, wrapped into rows.
    Answer(Vec<String>),
}

impl Results {
    /// How many rows it has.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::None => 0,
            Self::Matches(apps) => apps.len(),
            Self::Output(lines) | Self::Answer(lines) => lines.len(),
        }
    }

    /// Whether there is nothing to show.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The menu while it is open.
#[derive(Debug)]
pub struct Menu {
    /// The surface it draws on.
    pub id: window::Id,
    /// What is in the field.
    pub input: String,
    /// What is under the field.
    pub results: Results,
    /// Which row Enter would take.
    pub selected: usize,
    /// What went wrong, on the line under the list.
    pub error: Option<String>,
    /// What is happening, on the same line when there is no error.
    pub notice: Option<String>,
    /// A command that changes something, waiting for a second Enter.
    pub pending: Option<Action>,
    /// The height the surface has been told to be.
    pub height: u32,
}

impl Menu {
    /// An open menu with an empty field.
    #[must_use]
    pub fn new(id: window::Id) -> Self {
        Self {
            id,
            input: String::new(),
            results: Results::None,
            selected: 0,
            error: None,
            notice: None,
            pending: None,
            height: height(0, false),
        }
    }

    /// New words in the field: what they match goes in the list, and anything the last line left
    /// behind goes away.
    pub fn typed(&mut self, apps: &[App], value: String) {
        self.pending = None;
        self.notice = None;
        self.error = None;
        self.input = value;
        self.selected = 0;
        // only an app shows a list while typing. a command or a pipeline has nothing to show
        // until it has run, and a list that does not agree with what Enter does is a trap
        self.results = match route::route(&self.input, apps) {
            route::Interpretation::Launch(_) => {
                let mut found = route::matches(&self.input, apps);
                found.truncate(ROWS);
                Results::Matches(found.into_iter().cloned().collect())
            }
            _ => Results::None,
        };
    }

    /// Empty field, empty list, nothing pending.
    pub fn clear(&mut self) {
        self.input.clear();
        self.results = Results::None;
        self.selected = 0;
        self.error = None;
        self.notice = None;
        self.pending = None;
    }

    /// Whether there is anything to clear before the menu closes.
    #[must_use]
    pub fn has_anything(&self) -> bool {
        !self.input.is_empty()
            || !self.results.is_empty()
            || self.error.is_some()
            || self.notice.is_some()
    }

    /// Up and down walk the matches. Output rows are not a menu, nothing to select there.
    pub fn step(&mut self, step: isize) {
        let Results::Matches(apps) = &self.results else {
            return;
        };
        let last = apps.len().saturating_sub(1);
        if step > 0 {
            self.selected = if self.selected >= last {
                0
            } else {
                self.selected + 1
            };
        } else {
            self.selected = if self.selected == 0 {
                last
            } else {
                self.selected - 1
            };
        }
    }

    /// How tall the surface should be for what it holds now.
    #[must_use]
    pub fn wanted_height(&self) -> u32 {
        height(self.results.len(), self.line().is_some())
    }

    /// The line under the list: what went wrong, or what is happening.
    #[must_use]
    pub fn line(&self) -> Option<(&str, bool)> {
        self.error
            .as_deref()
            .map(|why| (why, true))
            .or_else(|| self.notice.as_deref().map(|notice| (notice, false)))
    }
}

/// How tall a menu with this many rows, with or without the line under them, is.
#[must_use]
pub fn height(rows: usize, line: bool) -> u32 {
    let rows = u32::try_from(rows).unwrap_or(0);
    let list = if rows == 0 {
        0
    } else {
        GAP + rows * ROW_HEIGHT
    };
    let under = if line { GAP + ROW_HEIGHT } else { 0 };
    PAD + FIELD_HEIGHT + list + under + PAD
}

/// The menu's surface: the field, the rows and the line, on the menu's gray inside its border.
pub fn view(look: Palette, menu: &Menu) -> Element<'_, Message> {
    let field = text_input(PLACEHOLDER, &menu.input)
        .id(FIELD_ID)
        .on_input(Message::Input)
        .on_submit(Message::Submit)
        .width(FIELD_WIDTH)
        .size(bar::TEXT_SIZE)
        .line_height(iced::widget::text::LineHeight::Absolute(FIELD_LINE.into()))
        .padding([FIELD_PAD, 8])
        .style(move |_: &Theme, status| field_style(look, status));
    let mut body = column![field].spacing(GAP);
    if !menu.results.is_empty() {
        body = body.push(list(look, menu));
    }
    if let Some((line, wrong)) = menu.line() {
        let colour = if wrong { look.error } else { look.dim };
        body = body.push(
            container(text(line).size(bar::TEXT_SIZE).color(colour))
                .width(FIELD_WIDTH)
                .height(ROW_HEIGHT)
                .padding([0, 4])
                .align_y(iced::Center)
                .clip(true),
        );
    }
    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(INSIDE)
        .style(move |_: &Theme| container::Style {
            background: Some(look.menu.into()),
            text_color: Some(look.text),
            border: Border {
                color: look.edge,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// The rows under the field: the apps the words match, what the last line printed, or Quasar's
/// answer.
fn list(look: Palette, menu: &Menu) -> Element<'_, Message> {
    let mut rows = column![];
    match &menu.results {
        Results::None => {}
        Results::Matches(apps) => {
            for (index, app) in apps.iter().enumerate() {
                rows = rows.push(entry(look, &app.name, FONT, index == menu.selected));
            }
        }
        Results::Output(lines) => {
            for output in lines {
                rows = rows.push(entry(look, output, MONO, false));
            }
        }
        Results::Answer(lines) => {
            for answer in lines {
                rows = rows.push(entry(look, answer, FONT, false));
            }
        }
    }
    container(rows).width(FIELD_WIDTH).clip(true).into()
}

fn entry(look: Palette, label: &str, font: Font, selected: bool) -> Element<'_, Message> {
    let colour = if selected { look.selected } else { look.text };
    let body = text(label)
        .size(bar::TEXT_SIZE)
        .font(font)
        .color(colour)
        .wrapping(iced::widget::text::Wrapping::None);
    container(body)
        .width(Length::Fill)
        .height(ROW_HEIGHT)
        .padding([0, 4])
        .align_y(iced::Center)
        .clip(true)
        .style(move |_: &Theme| container::Style {
            background: selected.then(|| look.accent.into()),
            border: Border {
                color: iced::Color::TRANSPARENT,
                width: 0.0,
                radius: 4.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn field_style(look: Palette, status: text_input::Status) -> text_input::Style {
    let (edge, width) = match status {
        text_input::Status::Focused { .. } => (look.accent, 2.0),
        _ => (look.edge, 1.0),
    };
    text_input::Style {
        background: look.field.into(),
        border: Border {
            color: edge,
            width,
            radius: 4.0.into(),
        },
        icon: look.text,
        placeholder: look.dim,
        value: look.text,
        selection: iced::Color {
            a: 0.4,
            ..look.accent
        },
    }
}

/// The operation that puts the cursor in the field.
pub fn focus_field() -> iced::Task<Message> {
    iced::widget::operation::focus(FIELD_ID)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_height_follows_what_is_in_it() {
        assert_eq!(height(0, false), 48);
        assert_eq!(height(0, true), 48 + GAP + ROW_HEIGHT);
        assert_eq!(height(3, false), 48 + GAP + 3 * ROW_HEIGHT);
        assert_eq!(
            height(ROWS, true),
            48 + GAP + 8 * ROW_HEIGHT + GAP + ROW_HEIGHT
        );
    }

    #[test]
    fn the_field_fits_the_menu_and_its_line_fits_the_field() {
        assert!(f64::from(FIELD_WIDTH) + f64::from(2 * PAD) <= f64::from(WIDTH));
        assert_eq!(FIELD_HEIGHT, u32::from(2 * FIELD_PAD) + 18);
        assert_eq!(PAD, u32::from(INSIDE));
    }

    #[test]
    fn a_menu_with_nothing_in_it_has_nothing_to_clear() {
        let mut menu = Menu::new(window::Id::unique());
        assert!(!menu.has_anything());
        menu.input = "wifi".into();
        assert!(menu.has_anything());
        menu.clear();
        assert!(!menu.has_anything());
        assert_eq!(menu.wanted_height(), height(0, false));
        menu.error = Some("Say wifi on or wifi off.".into());
        assert_eq!(menu.wanted_height(), height(0, true));
        assert_eq!(menu.line(), Some(("Say wifi on or wifi off.", true)));
    }

    #[test]
    fn up_and_down_walk_the_matches_and_wrap() {
        let mut menu = Menu::new(window::Id::unique());
        menu.results = Results::Output(vec!["one".into(), "two".into()]);
        menu.step(1);
        assert_eq!(menu.selected, 0, "output rows are not a menu");
        menu.results = Results::Matches(Vec::new());
        assert_eq!(menu.results.len(), 0);
    }
}
