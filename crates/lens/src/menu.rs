//! The Applications menu: the surface that hangs under the Applications button. The field is at
//! the top of it, and under the field the places and the apps in their sections, or what the field
//! matched, found in home, printed or answered.

use iced::widget::{button, column, container, row, scrollable, space, text, text_input};
use iced::{Border, Color, Element, Font, Length, Shadow, Theme, window};
use librift::apps::Category;
use librift::files::places::Place;
use librift::os::Action;

use crate::bar;
use crate::find::File;
use crate::icons;
use crate::launcher::App;
use crate::route;
use crate::theme::Palette;
use crate::ui::{FONT, HEADING, MONO, Message};

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
/// The same height where a length is wanted.
const ROW: f32 = 28.0;
/// How many rows of output or of an answer the list shows.
pub const ROWS: usize = 8;
/// How many rows of the app list it shows before the list scrolls. The tallest menu is then 612
/// px, or 644 with a line under the list, which fits between the two bars on a 768 px screen.
pub const LIST_ROWS: usize = 20;
/// The app icon at the left of a row.
const ICON: f32 = 16.0;
/// The gap between the icon and the name.
const ICON_GAP: f32 = 8.0;
/// The line of text inside the field, so its height is the same everywhere.
const FIELD_LINE: f32 = 18.0;
/// The padding inside the field: 18 + 7 + 7 is the field's height.
const FIELD_PAD: u16 = 7;
/// The field's widget id, for the focus operation.
const FIELD_ID: &str = "field";
/// The list's widget id, for the scroll operation.
const LIST_ID: &str = "list";
/// What the field says when it is empty. It names all of what the field does, the files of home
/// included, since the field is the only place that teaches them.
const PLACEHOLDER: &str = "Type an app, a command, a question or what a file is about";

/// A row of the list under the field.
#[derive(Debug, Clone)]
pub enum Row {
    /// The name of a section, over the rows in it.
    Header(&'static str),
    /// A folder, in the first section. A click opens it in the file manager.
    Place(Place),
    /// An app, with its own icon at the left. Enter starts the one that is selected.
    App(App),
    /// A file of home a search by meaning found. A click opens it with the app its kind opens
    /// with.
    File(File),
}

/// The name of the section the places are in, over the app sections.
pub const PLACES: &str = "Places";
/// The name of the section the files a search found are in. They are the whole list while they are
/// up, since a line of plain words matches no app.
pub const FILES: &str = "Your files";

/// The places, then every app in its section, the sections in the menu's order. A section with
/// nothing in it has no header.
#[must_use]
pub fn sections(apps: &[App], places: &[Place]) -> Vec<Row> {
    let mut rows = Vec::new();
    if let Some(first) = places.first() {
        rows.push(Row::Header(PLACES));
        rows.push(Row::Place(first.clone()));
        rows.extend(places[1..].iter().cloned().map(Row::Place));
    }
    for category in Category::ALL {
        let mut found = apps.iter().filter(|app| app.category == category);
        if let Some(first) = found.next() {
            rows.push(Row::Header(category.name()));
            rows.push(Row::App(first.clone()));
            rows.extend(found.cloned().map(Row::App));
        }
    }
    rows
}

/// What the list under the field is showing.
#[derive(Debug)]
pub enum Results {
    /// Nothing, and the menu is only the field high.
    None,
    /// The apps: all of them in their sections when the field is empty, or the ones the words
    /// match, best first, with no sections.
    Apps(Vec<Row>),
    /// What a command or a pipeline printed.
    Output(Vec<String>),
    /// Quasar's answer, wrapped into rows.
    Answer(Vec<String>),
}

impl Results {
    /// How many rows it has, on screen or scrolled out of sight.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::None => 0,
            Self::Apps(rows) => rows.len(),
            Self::Output(lines) | Self::Answer(lines) => lines.len(),
        }
    }

    /// Whether there is nothing to show.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// How many rows are on screen. The app list is as tall as it can be and scrolls past that;
    /// output and an answer arrive short.
    #[must_use]
    pub fn shown(&self) -> usize {
        match self {
            Self::Apps(rows) => rows.len().min(LIST_ROWS),
            other => other.len().min(ROWS),
        }
    }

    /// The apps in it, in the order they are listed.
    fn apps(&self) -> impl Iterator<Item = &App> {
        let rows: &[Row] = match self {
            Self::Apps(rows) => rows,
            _ => &[],
        };
        rows.iter().filter_map(|row| match row {
            Row::App(app) => Some(app),
            Row::Header(_) | Row::Place(_) | Row::File(_) => None,
        })
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
    /// Which app Enter would start, counting only the app rows. Nothing is picked until the words
    /// match something or an arrow key walks the list.
    pub selected: Option<usize>,
    /// The first row of the list on screen, which is where the arrows scroll it to.
    pub top: usize,
    /// What went wrong, on the line under the list.
    pub error: Option<String>,
    /// What is happening, on the same line when there is no error.
    pub notice: Option<String>,
    /// A command that changes something, waiting for a second Enter.
    pub pending: Option<Action>,
    /// The places at the top of the list, as they were when the menu opened.
    pub places: Vec<Place>,
    /// The height the surface has been told to be.
    pub height: u32,
    /// Whether the line in the field is one the shell heard rather than one that was typed. It
    /// stays in the field after Enter, so what the shell heard is under your eyes, and the answer
    /// to it is read back out loud.
    pub heard: bool,
}

impl Menu {
    /// An open menu with an empty field, the places and every app under it.
    #[must_use]
    pub fn new(id: window::Id, apps: &[App], places: &[Place]) -> Self {
        let results = Results::Apps(sections(apps, places));
        Self {
            id,
            input: String::new(),
            height: height(results.shown(), false),
            results,
            selected: None,
            top: 0,
            error: None,
            notice: None,
            pending: None,
            places: places.to_vec(),
            heard: false,
        }
    }

    /// New words in the field: what they match goes in the list, and anything the last line left
    /// behind goes away.
    pub fn typed(&mut self, apps: &[App], value: String) {
        self.pending = None;
        self.notice = None;
        self.error = None;
        self.heard = false;
        self.input = value;
        self.top = 0;
        if self.input.trim().is_empty() {
            self.results = Results::Apps(sections(apps, &self.places));
            self.selected = None;
            return;
        }
        // only an app shows a list while typing. a command or a pipeline has nothing to show
        // until it has run, and a list that does not agree with what Enter does is a trap
        if let route::Interpretation::Launch(_) = route::route(&self.input, apps) {
            let mut found = route::matches(&self.input, apps);
            found.truncate(LIST_ROWS);
            self.results = Results::Apps(found.into_iter().cloned().map(Row::App).collect());
        } else if self.files().next().is_none() || route::searched(&self.input, apps).is_none() {
            // the files a search found stay up while the words they were found for are typed on,
            // since the answer for the words in the field now is what replaces them. a line that
            // is no longer one to look with takes them away at once
            self.results = Results::None;
        }
        self.selected = self.results.apps().next().is_some().then_some(0);
    }

    /// Empty field, the app list back, nothing pending.
    pub fn clear(&mut self, apps: &[App]) {
        self.typed(apps, String::new());
    }

    /// The field after Enter has taken the line: empty, unless the shell heard the line rather
    /// than reading it typed, in which case it stays where it can be read and corrected.
    pub fn taken(&mut self) {
        if !self.heard {
            self.input.clear();
        }
    }

    /// The files a search of home came back with: a section of their own under the field, in place
    /// of whatever was there. The arrows and Enter walk the apps, not these, so nothing is
    /// selected; a file is pressed, the way a place is.
    pub fn found(&mut self, files: Vec<File>) {
        let mut rows = vec![Row::Header(FILES)];
        rows.extend(files.into_iter().map(Row::File));
        self.results = Results::Apps(rows);
        self.selected = None;
        self.top = 0;
    }

    /// The files a search found, in the order they are listed.
    pub fn files(&self) -> impl Iterator<Item = &File> {
        let rows: &[Row] = match &self.results {
            Results::Apps(rows) => rows,
            _ => &[],
        };
        rows.iter().filter_map(|row| match row {
            Row::File(file) => Some(file),
            Row::Header(_) | Row::Place(_) | Row::App(_) => None,
        })
    }

    /// Whether there is anything to clear before the menu closes. The app list is what the menu
    /// is, not something the last line left behind.
    #[must_use]
    pub fn has_anything(&self) -> bool {
        !self.input.is_empty()
            || self.error.is_some()
            || self.notice.is_some()
            || matches!(self.results, Results::Output(_) | Results::Answer(_))
    }

    /// Up and down walk the apps, and the list scrolls to keep the one they are on in sight.
    /// Output rows are not a menu, nothing to select there.
    pub fn step(&mut self, step: isize) {
        let count = self.results.apps().count();
        if count == 0 {
            return;
        }
        let last = count - 1;
        self.selected = Some(match self.selected {
            None if step > 0 => 0,
            Some(at) if step > 0 => {
                if at >= last {
                    0
                } else {
                    at + 1
                }
            }
            None | Some(0) => last,
            Some(at) => at - 1,
        });
        self.see();
    }

    /// The app Enter would start.
    #[must_use]
    pub fn selected_app(&self) -> Option<&App> {
        self.results.apps().nth(self.selected?)
    }

    /// Which row the selected app is on, counting the headers above it, and whether the header
    /// over it belongs to it.
    fn selected_row(&self) -> Option<(usize, bool)> {
        let Results::Apps(rows) = &self.results else {
            return None;
        };
        let at = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| matches!(row, Row::App(_)))
            .map(|(index, _)| index)
            .nth(self.selected?)?;
        let heads = at > 0 && matches!(rows[at - 1], Row::Header(_));
        Some((at, heads))
    }

    /// Scroll as little as it takes to have the selected row on screen, with the header over it
    /// when it is the first app of its section.
    fn see(&mut self) {
        let shown = self.results.shown();
        let Some((row, heads)) = self.selected_row() else {
            return;
        };
        let first = if heads { row - 1 } else { row };
        if first < self.top {
            self.top = first;
        } else if row >= self.top + shown {
            self.top = row + 1 - shown;
        }
        self.top = self.top.min(self.results.len().saturating_sub(shown));
    }

    /// Put the list where `top` says it is.
    pub fn scroll(&self) -> iced::Task<Message> {
        let top = f32::from(u16::try_from(self.top).unwrap_or(u16::MAX));
        iced::widget::operation::scroll_to(
            LIST_ID,
            iced::widget::scrollable::AbsoluteOffset {
                x: 0.0,
                y: top * ROW,
            },
        )
    }

    /// How tall the surface should be for what it holds now.
    #[must_use]
    pub fn wanted_height(&self) -> u32 {
        height(self.results.shown(), self.line().is_some())
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

/// How tall a menu with this many rows on screen, with or without the line under them, is.
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

/// The rows under the field: the apps in their sections, the apps the words match, what the last
/// line printed, or Quasar's answer. The app list is longer than the menu is tall, so it scrolls.
fn list(look: Palette, menu: &Menu) -> Element<'_, Message> {
    let mut rows = column![];
    match &menu.results {
        Results::None => {}
        Results::Apps(listed) => {
            let (mut at, mut place, mut found) = (0, 0, 0);
            for row in listed {
                match row {
                    Row::Header(name) => rows = rows.push(header(look, name)),
                    Row::Place(where_it_is) => {
                        rows = rows.push(place_row(look, where_it_is, place));
                        place += 1;
                    }
                    Row::App(app) => {
                        rows = rows.push(app_row(look, app, at, menu.selected == Some(at)));
                        at += 1;
                    }
                    Row::File(file) => {
                        rows = rows.push(file_row(look, file, found));
                        found += 1;
                    }
                }
            }
        }
        Results::Output(lines) => {
            for output in lines {
                rows = rows.push(printed(look, output, MONO));
            }
        }
        Results::Answer(lines) => {
            for answer in lines {
                rows = rows.push(printed(look, answer, FONT));
            }
        }
    }
    let tall = ROW * f32::from(u16::try_from(menu.results.shown()).unwrap_or(u16::MAX));
    scrollable(rows)
        .id(LIST_ID)
        .width(FIELD_WIDTH)
        .height(tall)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new().width(4).scroller_width(4),
        ))
        .style(move |_: &Theme, _| scrollable::Style {
            container: container::Style::default(),
            vertical_rail: rail(look),
            horizontal_rail: rail(look),
            gap: None,
            auto_scroll: scrollable::AutoScroll {
                background: look.menu.into(),
                border: Border {
                    color: look.edge,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                shadow: iced::Shadow::default(),
                icon: look.text,
            },
        })
        .into()
}

/// The name of a section, over the apps in it.
fn header(look: Palette, name: &str) -> Element<'static, Message> {
    container(
        text(name.to_string())
            .size(bar::TEXT_SIZE)
            .font(HEADING)
            .color(look.text),
    )
    .width(Length::Fill)
    .height(ROW_HEIGHT)
    .padding([0, 4])
    .align_y(iced::Center)
    .clip(true)
    .into()
}

/// One place in the list: its icon, its name, and a click opens it in the file manager. The
/// arrows and Enter walk the apps, not the places, so a place is never the selected row.
fn place_row(look: Palette, place: &Place, at: usize) -> Element<'static, Message> {
    let body = row![
        icons::draw(look.text, Some(place.icon), ICON),
        text(place.name.clone())
            .size(bar::TEXT_SIZE)
            .color(look.text)
            .wrapping(iced::widget::text::Wrapping::None)
    ]
    .spacing(ICON_GAP)
    .align_y(iced::Center);
    let inside = container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(iced::Center)
        .clip(true);
    button(inside)
        .width(Length::Fill)
        .height(ROW)
        .padding([0, 4])
        .on_press(Message::PickPlace(at))
        .style(move |_: &Theme, status| button::Style {
            background: match status {
                button::Status::Hovered | button::Status::Pressed => Some(look.press.into()),
                _ => None,
            },
            text_color: look.text,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 4.0.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        })
        .into()
}

/// One file a search found: the drawing of its kind, its name, and the folder it is in at the
/// right. A click opens it with the app that kind opens with. Like a place, it is only ever
/// pressed, never the row Enter takes.
fn file_row(look: Palette, file: &File, at: usize) -> Element<'static, Message> {
    let body = row![
        icons::of_file(look.text, &file.icons, ICON),
        text(file.name.clone())
            .size(bar::TEXT_SIZE)
            .color(look.text)
            .wrapping(iced::widget::text::Wrapping::None),
        space().width(Length::Fill),
        text(file.folder.clone())
            .size(bar::TEXT_SIZE)
            .color(look.dim)
            .wrapping(iced::widget::text::Wrapping::None),
    ]
    .spacing(ICON_GAP)
    .align_y(iced::Center);
    let inside = container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(iced::Center)
        .clip(true);
    button(inside)
        .width(Length::Fill)
        .height(ROW)
        .padding([0, 4])
        .on_press(Message::PickFile(at))
        .style(move |_: &Theme, status| button::Style {
            background: match status {
                button::Status::Hovered | button::Status::Pressed => Some(look.press.into()),
                _ => None,
            },
            text_color: look.text,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 4.0.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        })
        .into()
}

/// One app in the list: its icon, its name, and a click starts it. The row Enter would take is
/// the one in the accent; the one under the pointer is a shade lighter.
fn app_row(look: Palette, app: &App, at: usize, selected: bool) -> Element<'static, Message> {
    let colour = if selected { look.selected } else { look.text };
    let body = row![
        icons::draw(look.text, app.icon.as_deref(), ICON),
        text(app.name.clone())
            .size(bar::TEXT_SIZE)
            .color(colour)
            .wrapping(iced::widget::text::Wrapping::None)
    ]
    .spacing(ICON_GAP)
    .align_y(iced::Center);
    // a button lays its content out at the top of its box, so the row is centred by hand
    let inside = container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(iced::Center)
        .clip(true);
    button(inside)
        .width(Length::Fill)
        .height(ROW)
        .padding([0, 4])
        .on_press(Message::Pick(at))
        .style(move |_: &Theme, status| button::Style {
            background: match (selected, status) {
                (true, _) => Some(look.accent.into()),
                (false, button::Status::Hovered | button::Status::Pressed) => {
                    Some(look.press.into())
                }
                _ => None,
            },
            text_color: colour,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 4.0.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        })
        .into()
}

/// One line of what a command printed or of an answer. Nothing to click.
fn printed(look: Palette, label: &str, font: Font) -> Element<'static, Message> {
    container(
        text(label.to_string())
            .size(bar::TEXT_SIZE)
            .font(font)
            .color(look.text)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(Length::Fill)
    .height(ROW_HEIGHT)
    .padding([0, 4])
    .align_y(iced::Center)
    .clip(true)
    .into()
}

/// The scrollbar: no rail, and a thin scroller in the menu's border gray.
pub fn rail(look: Palette) -> scrollable::Rail {
    scrollable::Rail {
        background: None,
        border: Border::default(),
        scroller: scrollable::Scroller {
            background: look.edge.into(),
            border: Border {
                color: iced::Color::TRANSPARENT,
                width: 0.0,
                radius: 2.0.into(),
            },
        },
    }
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
    use librift::apps::Category;

    fn app(name: &str, category: Category) -> App {
        App {
            id: name.to_lowercase(),
            name: name.to_string(),
            exec: vec![name.to_lowercase()],
            terminal: false,
            icon: Some(name.to_lowercase()),
            wm_class: None,
            category,
            types: Vec::new(),
            line: String::new(),
        }
    }

    fn apps() -> Vec<App> {
        vec![
            app("Files", Category::Accessories),
            app("Firefox", Category::Internet),
            app("Ghostty", Category::System),
            app("Helix", Category::Programming),
            app("Zed", Category::Programming),
        ]
    }

    fn names(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                Row::Header(name) => format!("[{name}]"),
                Row::Place(place) => place.name.clone(),
                Row::App(app) => app.name.clone(),
                Row::File(file) => file.name.clone(),
            })
            .collect()
    }

    fn file(name: &str, folder: &str) -> File {
        File {
            name: name.to_string(),
            folder: folder.to_string(),
            under: format!("{folder}/{name}"),
            path: std::path::PathBuf::from(format!("/home/rift/{folder}/{name}")),
            mime: "text/plain".to_string(),
            icons: vec!["text-plain".to_string()],
        }
    }

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

    /// The menu hangs under the 32 px bar and the dock takes 44 px from the bottom, so the
    /// tallest one has to fit in what a 768 px screen leaves between them.
    #[test]
    fn the_longest_list_fits_the_smallest_screen() {
        assert_eq!(height(LIST_ROWS, false), 612);
        assert!(height(LIST_ROWS, true) <= 768 - 32 - 44);
    }

    #[test]
    fn the_field_fits_the_menu_and_its_line_fits_the_field() {
        assert!(f64::from(FIELD_WIDTH) + f64::from(2 * PAD) <= f64::from(WIDTH));
        assert_eq!(FIELD_HEIGHT, u32::from(2 * FIELD_PAD) + 18);
        assert_eq!(PAD, u32::from(INSIDE));
        assert!((f64::from(ROW) - f64::from(ROW_HEIGHT)).abs() < f64::EPSILON);
    }

    #[test]
    fn the_apps_come_in_their_sections() {
        let rows = sections(&apps(), &[]);
        assert_eq!(
            names(&rows),
            [
                "[Accessories]",
                "Files",
                "[Internet]",
                "Firefox",
                "[Programming]",
                "Helix",
                "Zed",
                "[System]",
                "Ghostty",
            ]
        );
        // a section with nothing in it has no header
        assert!(!names(&rows).contains(&"[Office]".to_string()));
        assert!(sections(&[], &[]).is_empty());
    }

    #[test]
    fn the_places_come_first_in_a_section_of_their_own() {
        let home = Place {
            word: "home",
            name: "Home".to_string(),
            path: std::path::PathBuf::from("/home/rift"),
            icon: "user-home-symbolic",
            colour: "user-home",
        };
        let rows = sections(&apps(), std::slice::from_ref(&home));
        assert_eq!(names(&rows)[..3], ["[Places]", "Home", "[Accessories]"]);
        // the arrows and Enter walk the apps, so a place is not one of them
        let listed = Results::Apps(rows);
        assert_eq!(listed.apps().count(), apps().len());
    }

    #[test]
    fn the_files_a_search_found_are_a_section_the_arrows_do_not_walk() {
        let known = apps();
        let mut menu = Menu::new(window::Id::unique(), &known, &[]);
        menu.typed(&known, "teeth checkup booking".into());
        // plain words match no app, so the list is empty until the search answers
        assert!(menu.results.is_empty());
        menu.found(vec![file("letter.pdf", "notes"), file("bike.txt", "notes")]);
        assert_eq!(
            names(&rows_of(&menu)),
            ["[Your files]", "letter.pdf", "bike.txt"]
        );
        assert_eq!(menu.wanted_height(), height(3, false));
        // Enter still asks Quasar: no file row is ever the selected one
        menu.step(1);
        assert!(menu.selected_app().is_none());
        assert_eq!(
            menu.files()
                .map(|file| file.under.as_str())
                .collect::<Vec<_>>(),
            ["notes/letter.pdf", "notes/bike.txt"]
        );
        // typing on keeps them up, since the answer for the new words is what replaces them
        menu.typed(&known, "teeth checkup bookings".into());
        assert_eq!(menu.files().count(), 2);
        // a line that is not one to look with takes them away at once, and so does an empty field
        menu.typed(&known, "teeth checkup booking?".into());
        assert!(menu.results.is_empty());
        menu.found(vec![file("letter.pdf", "notes")]);
        menu.clear(&known);
        assert_eq!(menu.files().count(), 0);
    }

    #[test]
    fn a_menu_with_nothing_typed_shows_the_apps_and_has_nothing_to_clear() {
        let known = apps();
        let mut menu = Menu::new(window::Id::unique(), &known, &[]);
        assert_eq!(menu.results.len(), 9);
        assert!(menu.selected_app().is_none(), "nothing is picked yet");
        assert!(!menu.has_anything());
        assert_eq!(menu.wanted_height(), height(9, false));
        menu.input = "wifi".into();
        assert!(menu.has_anything());
        menu.clear(&known);
        assert!(!menu.has_anything());
        assert_eq!(menu.results.len(), 9);
        menu.error = Some("Say wifi on or wifi off.".into());
        assert_eq!(menu.wanted_height(), height(9, true));
        assert_eq!(menu.line(), Some(("Say wifi on or wifi off.", true)));
    }

    #[test]
    fn typing_filters_the_list_and_picks_the_best() {
        let known = apps();
        let mut menu = Menu::new(window::Id::unique(), &known, &[]);
        menu.typed(&known, "fi".into());
        assert_eq!(names(&rows_of(&menu)), ["Files", "Firefox"]);
        assert_eq!(
            menu.selected_app().map(|app| app.name.as_str()),
            Some("Files")
        );
        // a pipeline has nothing to show until it has run
        menu.typed(&known, "ls | first".into());
        assert!(menu.results.is_empty());
        assert!(menu.selected_app().is_none());
        // and an empty field brings the sections back
        menu.typed(&known, String::new());
        assert_eq!(menu.results.len(), 9);
        assert!(menu.selected_app().is_none());
    }

    fn rows_of(menu: &Menu) -> Vec<Row> {
        match &menu.results {
            Results::Apps(rows) => rows.clone(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn up_and_down_walk_the_apps_and_wrap() {
        let known = apps();
        let mut menu = Menu::new(window::Id::unique(), &known, &[]);
        menu.step(1);
        assert_eq!(
            menu.selected_app().map(|app| app.name.as_str()),
            Some("Files")
        );
        menu.step(-1);
        assert_eq!(
            menu.selected_app().map(|app| app.name.as_str()),
            Some("Ghostty")
        );
        menu.step(1);
        assert_eq!(
            menu.selected_app().map(|app| app.name.as_str()),
            Some("Files")
        );
        menu.results = Results::Output(vec!["one".into(), "two".into()]);
        menu.selected = None;
        menu.step(1);
        assert!(menu.selected_app().is_none(), "output rows are not a menu");
    }

    #[test]
    fn the_list_scrolls_to_keep_the_selected_row_in_sight() {
        let many: Vec<App> = (0..40)
            .map(|at| app(&format!("App {at:02}"), Category::Accessories))
            .collect();
        let mut menu = Menu::new(window::Id::unique(), &many, &[]);
        assert_eq!(menu.results.len(), 41, "a header over forty apps");
        assert_eq!(menu.results.shown(), LIST_ROWS);
        assert_eq!(menu.wanted_height(), height(LIST_ROWS, false));
        // walking down the visible rows does not move the list
        for _ in 0..LIST_ROWS - 1 {
            menu.step(1);
        }
        assert_eq!(menu.top, 0);
        // past the last visible row it follows, one row at a time
        menu.step(1);
        assert_eq!(menu.top, 1);
        // and back up at the first app it stops there, with the header over it in sight
        for _ in 0..LIST_ROWS - 1 {
            menu.step(-1);
        }
        assert_eq!(menu.selected, Some(0));
        assert_eq!(menu.top, 0);
        // one more wraps to the last app, which is the last row, so the list is at its end
        menu.step(-1);
        assert_eq!(menu.top, menu.results.len() - LIST_ROWS);
    }
}
