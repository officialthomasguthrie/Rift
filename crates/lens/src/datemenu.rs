//! The clock menu, which the clock in the middle of the bar opens: a month with today marked,
//! then the notifications so far with Clear, and a Do not disturb switch at the bottom, the way
//! GNOME's date menu has them. A list of rows on the menu's gray inside a border, like every
//! other menu of the shell.

use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Border, Element, Length, Padding, Shadow, Theme, window};

use crate::banner;
use crate::bar;
use crate::calendar::{self, Day, Month, Weekday};
use crate::icons;
use crate::menu;
use crate::notice::{Kept, Notices, Notification};
use crate::system;
use crate::theme::Palette;
use crate::ui::{FONT, HEADING, Message};

/// How wide the menu is in logical pixels.
pub const WIDTH: u32 = 340;
/// The padding inside it.
pub const PAD: u32 = 8;
/// The same padding where a widget wants it.
const INSIDE: u16 = 8;
/// A day of the month: this wide and `CELL` tall.
pub const CELL_WIDTH: u32 = 32;
/// How tall a day is, and the letters over the days.
pub const CELL: u32 = 28;
/// The weeks every page shows.
pub const WEEKS: u32 = 6;
/// The row with the month's name and the buttons that turn the page.
pub const HEADER: u32 = 32;
/// A row: the heading over the notifications, the line that says there are none, and Do not
/// disturb.
pub const ROW: u32 = 32;
/// A notification in the list: its summary over the first line of its body.
pub const NOTICE: u32 = 48;
/// How many notifications the list shows before it scrolls.
pub const LISTED: usize = 5;
/// The icon of a notification in the list.
const ICON: u32 = 24;
/// The gap between that icon and the words.
const ICON_GAP: u32 = 12;
/// The space at each end of a row.
const INSET: u32 = 8;
/// The room the minute a notification came in takes at the right of its summary.
const TIME: u32 = 48;
/// A button that turns the page.
const TURN: u32 = 28;
/// The symbol in it.
const SYMBOL: f32 = 16.0;
/// A line of the words of a notification in the list.
const LINE: f32 = 20.0;
/// The corner of a button and of the mark on today.
const RADIUS: f32 = 4.0;

/// What the menu's buttons and its switch ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// The page before.
    Previous,
    /// The page after.
    Next,
    /// Clear: every notification goes.
    Clear,
    /// The Do not disturb switch.
    Quiet(bool),
}

/// The menu while it is open.
#[derive(Debug)]
pub struct Menu {
    /// The surface it draws on.
    pub id: window::Id,
    /// The height the surface has been told to be.
    pub height: u32,
    /// The page it shows.
    pub month: Month,
}

/// How tall the menu is with this many notifications kept.
#[must_use]
pub fn height(kept: usize) -> u32 {
    let listed = u32::try_from(kept.min(LISTED)).unwrap_or(0);
    let list = if listed == 0 { ROW } else { listed * NOTICE };
    2 * PAD
        + HEADER
        + CELL
        + WEEKS * CELL
        + system::SEPARATOR
        + ROW
        + list
        + system::SEPARATOR
        + ROW
}

/// The width the words of a notification in the list have, beside its icon when it has one.
#[must_use]
pub fn text_width(icon: bool) -> f32 {
    let taken = 2 * PAD + 2 * INSET + if icon { ICON + ICON_GAP } else { 0 };
    to_length(WIDTH - taken)
}

/// The summary and the first line of the body as a row of the list has room for them. The summary
/// leaves room for the minute at its right.
#[must_use]
pub fn texts(notification: &Notification, icon: bool) -> (String, String) {
    let width = text_width(icon);
    let (summary, _) = banner::fit(&notification.summary, HEADING, width - to_length(TIME), 1);
    let first = notification.body.lines().next().unwrap_or_default();
    let (body, _) = banner::fit(first, FONT, width, 1);
    (summary, body)
}

/// A length in logical pixels from a count of them.
fn to_length(pixels: u32) -> f32 {
    f32::from(u16::try_from(pixels).unwrap_or(u16::MAX))
}

/// The menu: the month, the notifications and the switch.
pub fn view<'a>(
    look: Palette,
    menu: &'a Menu,
    today: Option<Day>,
    first: Weekday,
    notices: &'a Notices,
) -> Element<'a, Message> {
    let body = column![
        page(look, menu.month, today, first),
        system::separator(look),
        heading(look, notices.any()),
        list(look, &notices.kept),
        system::separator(look),
        quiet(look, notices.quiet),
    ];
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

/// The page of the calendar: the month's name with the buttons that turn the page, the letters of
/// the weekdays, and six weeks of days, in a column as wide as seven days in the middle of the menu.
fn page(
    look: Palette,
    month: Month,
    today: Option<Day>,
    first: Weekday,
) -> Element<'static, Message> {
    let wide = to_length(7 * CELL_WIDTH);
    let name = container(
        text(month.name())
            .size(bar::TEXT_SIZE)
            .font(HEADING)
            .color(look.text)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(Length::Fill)
    .height(to_length(HEADER))
    .align_y(iced::Center)
    .padding(Padding {
        left: 4.0,
        ..Padding::ZERO
    })
    .clip(true);
    let header = row![
        name,
        turn(look, "go-previous-symbolic", Event::Previous),
        turn(look, "go-next-symbolic", Event::Next),
    ]
    .spacing(4)
    .width(wide)
    .align_y(iced::Center);
    let mut letters = row![].width(wide);
    for letter in calendar::letters(first) {
        letters = letters.push(cell(text(letter).color(look.dim)));
    }
    let mut page = column![header, letters].width(wide);
    for week in month.weeks(first).chunks(7) {
        let mut days = row![].width(wide);
        for day in week {
            days = days.push(date(look, *day, month, today));
        }
        page = page.push(days);
    }
    container(page).center_x(Length::Fill).into()
}

/// A button that turns the page.
fn turn(look: Palette, icon: &str, event: Event) -> Element<'static, Message> {
    let symbol = container(icons::symbolic(look.text, icon, SYMBOL)).center(Length::Fill);
    button(symbol)
        .width(to_length(TURN))
        .height(to_length(TURN))
        .padding(0)
        .on_press(Message::Clock(event))
        .style(move |_: &Theme, state| system::fill(look, state))
        .into()
}

/// One cell of the page, with its words in the middle.
fn cell(words: iced::widget::Text<'_>) -> Element<'_, Message> {
    container(
        words
            .size(bar::TEXT_SIZE)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(to_length(CELL_WIDTH))
    .height(to_length(CELL))
    .center_x(to_length(CELL_WIDTH))
    .center_y(to_length(CELL))
    .into()
}

/// A day: today in the accent with the selected text colour and in bold, a day of another month in
/// the dim gray.
fn date(look: Palette, day: Day, month: Month, today: Option<Day>) -> Element<'static, Message> {
    let number = text(day.date.to_string());
    if today == Some(day) {
        let marked = container(
            number
                .size(bar::TEXT_SIZE)
                .font(HEADING)
                .color(look.selected),
        )
        .center(Length::Fill)
        .style(move |_: &Theme| container::Style {
            background: Some(look.accent.into()),
            border: Border {
                radius: RADIUS.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });
        return container(marked)
            .width(to_length(CELL_WIDTH))
            .height(to_length(CELL))
            .padding([1, 2])
            .into();
    }
    let colour = if Month::of(day) == month {
        look.text
    } else {
        look.dim
    };
    cell(number.color(colour))
}

/// The heading over the notifications, with Clear at its right while there is something to clear.
fn heading(look: Palette, any: bool) -> Element<'static, Message> {
    let title = container(
        text("Notifications")
            .size(bar::TEXT_SIZE)
            .font(HEADING)
            .color(look.text),
    )
    .width(Length::Fill)
    .align_y(iced::Center);
    let clear = container(text("Clear").size(bar::TEXT_SIZE)).center_y(Length::Fill);
    let clear = button(clear)
        .height(24)
        .padding([0, 12])
        .on_press_maybe(any.then_some(Message::Clock(Event::Clear)))
        .style(move |_: &Theme, state| {
            let mut style = system::fill(look, state);
            style.border = Border {
                color: look.edge,
                width: 1.0,
                radius: RADIUS.into(),
            };
            if state == button::Status::Disabled {
                style.text_color = look.dim;
            }
            style
        });
    container(row![title, clear].align_y(iced::Center))
        .width(Length::Fill)
        .height(to_length(ROW))
        .padding([0, u16::try_from(INSET).unwrap_or(8)])
        .align_y(iced::Center)
        .into()
}

/// The notifications kept, newest first, five on the page and the rest a scroll away, or a line
/// that says there are none.
fn list(look: Palette, kept: &[Kept]) -> Element<'_, Message> {
    if kept.is_empty() {
        return container(
            text("No notifications")
                .size(bar::TEXT_SIZE)
                .color(look.dim),
        )
        .width(Length::Fill)
        .height(to_length(ROW))
        .padding([0, u16::try_from(INSET).unwrap_or(8)])
        .align_y(iced::Center)
        .into();
    }
    let mut rows = column![];
    for one in kept {
        rows = rows.push(notice(look, one));
    }
    let shown = u32::try_from(kept.len().min(LISTED)).unwrap_or(0);
    scrollable(rows)
        .width(Length::Fill)
        .height(to_length(shown * NOTICE))
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new().width(4).scroller_width(4),
        ))
        .style(move |_: &Theme, _| scrollable::Style {
            container: container::Style::default(),
            vertical_rail: menu::rail(look),
            horizontal_rail: menu::rail(look),
            gap: None,
            auto_scroll: scrollable::AutoScroll {
                background: look.menu.into(),
                border: Border {
                    color: look.edge,
                    width: 1.0,
                    radius: RADIUS.into(),
                },
                shadow: Shadow::default(),
                icon: look.text,
            },
        })
        .into()
}

/// A notification in the list: its icon, its summary with the minute it came in, and the first line
/// of its body.
fn notice(look: Palette, kept: &Kept) -> Element<'_, Message> {
    let fitted = &kept.fitted;
    let width = text_width(fitted.icon.is_some());
    let summary = row![
        container(
            text(fitted.row_summary.as_str())
                .size(bar::TEXT_SIZE)
                .font(HEADING)
                .color(look.text)
                .wrapping(iced::widget::text::Wrapping::None),
        )
        .width(Length::Fill)
        .clip(true),
        text(kept.time.as_str())
            .size(bar::TEXT_SIZE)
            .color(look.dim)
            .wrapping(iced::widget::text::Wrapping::None),
    ]
    .width(width)
    .height(LINE)
    .align_y(iced::Center);
    let mut words = column![summary].width(width);
    if !fitted.row_body.is_empty() {
        words = words.push(
            container(
                text(fitted.row_body.as_str())
                    .size(bar::TEXT_SIZE)
                    .color(look.dim)
                    .wrapping(iced::widget::text::Wrapping::None),
            )
            .width(width)
            .height(LINE)
            .align_y(iced::Center)
            .clip(true),
        );
    }
    let mut line = row![].spacing(to_length(ICON_GAP)).align_y(iced::Center);
    if let Some(path) = &fitted.icon {
        line = line.push(icons::picture(look.text, path, to_length(ICON)));
    }
    line = line.push(words);
    container(line)
        .width(Length::Fill)
        .height(to_length(NOTICE))
        .padding([0, u16::try_from(INSET).unwrap_or(8)])
        .align_y(iced::Center)
        .clip(true)
        .into()
}

/// Do not disturb and its switch.
fn quiet(look: Palette, on: bool) -> Element<'static, Message> {
    let label = container(text("Do not disturb").size(bar::TEXT_SIZE).color(look.text))
        .width(Length::Fill)
        .align_y(iced::Center);
    let switch = system::switch(look, on, Some(|on| Message::Clock(Event::Quiet(on))));
    container(row![label, switch].align_y(iced::Center))
        .width(Length::Fill)
        .height(to_length(ROW))
        .padding([0, u16::try_from(INSET).unwrap_or(8)])
        .align_y(iced::Center)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_grows_with_the_list_and_stops_at_five() {
        let empty = height(0);
        assert_eq!(
            empty,
            2 * PAD + HEADER + CELL + WEEKS * CELL + 2 * system::SEPARATOR + 3 * ROW
        );
        assert_eq!(height(1), empty - ROW + NOTICE);
        assert_eq!(height(LISTED), height(LISTED + 20));
        // the tallest menu fits between the bar and the dock of a 768 px screen
        assert!(height(LISTED) <= 768 - 32 - 44);
    }

    #[test]
    fn the_page_fits_in_the_menu() {
        const {
            assert!(7 * CELL_WIDTH <= WIDTH - 2 * PAD);
            assert!(TURN <= HEADER);
        }
        assert!(text_width(true) < text_width(false));
        assert!(to_length(TIME) < text_width(true));
    }
}
