//! The list of what is in a folder: a row for each thing, with its icon, its name, its size and
//! when it last changed, under headings that put the list in their order. Only the rows on screen
//! are drawn, so a folder of a hundred thousand files scrolls like one of ten. In the trash the
//! columns are where each thing was and when it went, and in a search the folder each one is in.

use std::path::Path;

use iced::widget::{button, column, container, mouse_area, row, space, text};
use iced::{Border, Center, Color, Element, Fill, Length, Theme, window};
use librift::files::{self, Entry, Kind, Sort};

use crate::browser::{Browser, Location};
use crate::icons;
use crate::theme::Colors;
use crate::ui::{Act, Files, Message};
use crate::widgets::{TEXT_SIZE, fill, hairline, pointed, scroll};

/// How tall a row is.
pub const ROW: f32 = 32.0;
/// How tall the headings are.
pub const HEADS: f32 = 30.0;
/// How big a row's icon is.
const ICON: f32 = 20.0;
/// How wide the size column is.
const SIZE_COLUMN: f32 = 110.0;
/// How wide the column of times is.
const TIME_COLUMN: f32 = 120.0;
/// How wide the column of where things in the trash were is.
const PLACE_COLUMN: f32 = 250.0;
/// How many rows `--state` prints, at most.
const PRINTED: usize = 500;

/// The id of the list of window `number`. Each window has its own, since an operation on a list
/// reaches every window.
#[must_use]
pub fn list_id(number: usize) -> String {
    format!("files-list-{number}")
}

/// The id of the path bar's field of window `number`.
#[must_use]
pub fn path_id(number: usize) -> String {
    format!("files-path-{number}")
}

/// The id of the search field of window `number`.
#[must_use]
pub fn search_id(number: usize) -> String {
    format!("files-search-{number}")
}

/// The lines `--state` prints about a window.
#[must_use]
pub fn state(browser: &Browser) -> Vec<String> {
    let mut lines = vec![
        format!("window {}", browser.number),
        format!("location {}", browser.location.word()),
        format!("title {}", browser.location.label()),
        format!("ready {}", if browser.ready { "yes" } else { "no" }),
        format!("rows {}", browser.rows.len()),
    ];
    if let Some(at) = browser.location.at() {
        lines.push(format!("moment {at}"));
    }
    if let Some(query) = &browser.search {
        lines.push(format!("search {}", query.words));
        lines.push(format!(
            "search-kind {}",
            if query.meaning { "meaning" } else { "name" }
        ));
        if let Some(why) = &query.problem {
            lines.push(format!("search-problem {why}"));
        }
    }
    let searching = browser.searching().is_some();
    for entry in browser.rows.iter().take(PRINTED) {
        match (&browser.location, browser.origins.get(&entry.name)) {
            (Location::Trash, Some(was)) => {
                lines.push(format!(
                    "row {} {} from {}",
                    entry.kind.word(),
                    entry.label,
                    was.display()
                ));
            }
            _ if searching => lines.push(format!(
                "row {} {} in {}",
                entry.kind.word(),
                entry.label,
                Path::new(&entry.name).display()
            )),
            _ => lines.push(format!("row {} {}", entry.kind.word(), entry.label)),
        }
    }
    for entry in browser.chosen() {
        lines.push(format!("selected {}", entry.label));
    }
    lines.push(format!(
        "menu {}",
        browser
            .menu
            .as_ref()
            .map_or("none", |menu| menu.which.word())
    ));
    lines.push(format!(
        "dialog {}",
        browser
            .dialog
            .as_ref()
            .map_or("none", crate::dialogs::Dialog::word)
    ));
    if let Some(dialog) = &browser.dialog {
        if let crate::dialogs::Dialog::Properties(facts) = dialog {
            for (label, said) in facts.rows() {
                lines.push(format!("property {} {said}", label.to_lowercase()));
            }
        }
        if let Some(typed) = dialog.typed() {
            lines.push(format!("dialog-name {typed}"));
        }
        if let Some(problem) = browser
            .location
            .folder()
            .and_then(|folder| dialog.problem(folder))
        {
            lines.push(format!("dialog-problem {problem}"));
        }
    }
    if let Some(typed) = &browser.typing {
        lines.push(format!("typing {typed}"));
    }
    if let Some(toast) = &browser.toast {
        lines.push(format!("toast {}", toast.said));
    }
    if let Some(problem) = &browser.problem {
        lines.push(format!("problem {problem}"));
    }
    lines
}

/// The list of a window, with its headings.
#[must_use]
pub fn view<'a>(
    files: &'a Files,
    id: window::Id,
    browser: &'a Browser,
    look: Colors,
) -> Element<'a, Message> {
    let body: Element<'a, Message> = if !browser.ready {
        space().width(Fill).height(Fill).into()
    } else if let Some(problem) = &browser.problem {
        middle(look, problem.clone())
    } else if browser.rows.is_empty() {
        // with a sentence in the bar about the index, the rows are still the ones the names found
        let meaning = browser
            .search
            .as_ref()
            .is_some_and(|query| query.meaning && query.problem.is_none());
        middle(
            look,
            match (&browser.location, browser.read.is_empty()) {
                _ if browser.searching().is_some() => crate::find::nothing(meaning),
                (Location::Trash, _) => "The trash is empty.",
                (_, true) => "This folder is empty.",
                (_, false) => "Everything here is hidden.",
            }
            .to_string(),
        )
    } else if files.options.grid {
        crate::grid::view(files, id, browser, look)
    } else {
        rows(files, id, browser, look)
    };
    let area = mouse_area(container(body).width(Fill).height(Fill))
        .on_press(Message::Blank(id))
        .on_right_press(Message::BlankMenu(id));
    let pointing = pointed(area, move |point| Message::At(id, point));
    // the grid has no columns, so it has no headings over it either; the main menu puts it in
    // whichever order the headings would
    if files.options.grid {
        return pointing;
    }
    column![heads(files, id, browser, look), hairline(look), pointing,].into()
}

/// A sentence in the middle of the list, for a folder with nothing to show.
fn middle<'a>(look: Colors, said: String) -> Element<'a, Message> {
    container(text(said).size(TEXT_SIZE).color(look.dim))
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .into()
}

/// The headings, over the columns of the rows. A heading that puts the list in its order has the
/// arrow of the way it runs, and a press puts the list in its order, or the other way round.
fn heads<'a>(
    files: &'a Files,
    id: window::Id,
    browser: &Browser,
    look: Colors,
) -> Element<'a, Message> {
    let options = files.options;
    let head = |label: &'static str, sort: Option<Sort>, width: Length| -> Element<'a, Message> {
        let arrow: Element<'a, Message> = match sort.filter(|sort| *sort == options.sort) {
            Some(sort) => icons::symbolic(
                look.dim,
                if sort.ascending(options.reversed) {
                    "pan-up-symbolic"
                } else {
                    "pan-down-symbolic"
                },
                12.0,
            ),
            None => space().width(12.0).into(),
        };
        let inside = container(
            row![text(label).size(TEXT_SIZE).color(look.dim), arrow]
                .spacing(4)
                .align_y(Center),
        )
        .center_y(Fill);
        match sort {
            Some(sort) => button(inside)
                .width(width)
                .height(Length::Fixed(HEADS))
                .padding(0)
                .on_press(Message::Do(id, Act::Sort(sort)))
                .style(move |_: &Theme, _| button::Style {
                    background: None,
                    text_color: look.dim,
                    border: Border::default(),
                    ..button::Style::default()
                })
                .into(),
            None => container(inside)
                .width(width)
                .height(Length::Fixed(HEADS))
                .into(),
        }
    };
    // the same padding and spacing as a row, so each heading stands over its column
    let columns = if browser.location == Location::Trash {
        row![
            head("Name", Some(Sort::Name), Fill),
            head("Original location", None, Length::Fixed(PLACE_COLUMN)),
            head("Deleted", Some(Sort::Modified), Length::Fixed(TIME_COLUMN)),
        ]
    } else if browser.searching().is_some() {
        row![
            head("Name", Some(Sort::Name), Fill),
            head("Folder", None, Length::Fixed(PLACE_COLUMN)),
            head("Modified", Some(Sort::Modified), Length::Fixed(TIME_COLUMN)),
        ]
    } else {
        row![
            head("Name", Some(Sort::Name), Fill),
            head("Size", Some(Sort::Size), Length::Fixed(SIZE_COLUMN)),
            head("Modified", Some(Sort::Modified), Length::Fixed(TIME_COLUMN)),
        ]
    };
    container(container(columns.spacing(10)).padding([0, 12]))
        .width(Fill)
        .padding([0, 8])
        .style(move |_: &Theme| fill(look.view))
        .into()
}

/// The rows on screen, with empty space above and below them for the ones that are not, so the
/// scroller stands where it would with every row drawn.
fn rows<'a>(
    files: &'a Files,
    id: window::Id,
    browser: &'a Browser,
    look: Colors,
) -> Element<'a, Message> {
    let (first, shown) = browser.window_of_rows(ROW);
    let now = librift::time::now();
    #[allow(clippy::cast_precision_loss)]
    let above = first as f32 * ROW;
    #[allow(clippy::cast_precision_loss)]
    let below = (browser.rows.len() - first - shown) as f32 * ROW;
    let mut list = column![space().height(above)].width(Fill).padding([0, 8]);
    for at in first..first + shown {
        list = list.push(line(files, id, browser, at, now, look));
    }
    list = list.push(space().height(below));
    container(
        scroll(look, list)
            .id(list_id(browser.number))
            .on_scroll(move |viewport| Message::Scrolled(id, viewport))
            .height(Fill),
    )
    .width(Fill)
    .height(Fill)
    .style(move |_: &Theme| fill(look.view))
    .into()
}

/// One row: pressed to select it, twice to open it, with the right button for its menu.
fn line<'a>(
    files: &'a Files,
    id: window::Id,
    browser: &'a Browser,
    at: usize,
    now: i64,
    look: Colors,
) -> Element<'a, Message> {
    let entry = &browser.rows[at];
    let selected = browser.selected.contains(&entry.name);
    let place = browser.location.place();
    let folder = place.as_deref();
    let cut = files.clipboard.as_ref().is_some_and(|clip| {
        clip.cut && folder.is_some_and(|folder| clip.paths.contains(&folder.join(&entry.name)))
    });
    let colour = if entry.hidden || cut {
        look.dim
    } else {
        look.text
    };
    let name = container(
        text(entry.label.as_str())
            .size(TEXT_SIZE)
            .color(colour)
            .wrapping(text::Wrapping::None),
    )
    .width(Fill)
    .clip(true);
    let when = entry
        .modified
        .map(|seconds| files::when_words(seconds, now, files.offset))
        .unwrap_or_default();
    let middle_column = if browser.location == Location::Trash {
        let was = browser
            .origins
            .get(&entry.name)
            .and_then(|path| path.parent())
            .map(|path| where_words(files, path))
            .unwrap_or_default();
        dim(look, was, PLACE_COLUMN)
    } else if browser.searching().is_some() {
        let under = folder.map(|root| crate::find::under(root, entry));
        dim(look, under.unwrap_or_default(), PLACE_COLUMN)
    } else {
        let size = match entry.kind {
            Kind::Folder => entry.items.map(files::items_words).unwrap_or_default(),
            Kind::File => files::size_words(entry.size),
            Kind::Broken | Kind::Other => String::new(),
        };
        dim(look, size, SIZE_COLUMN)
    };
    let inside = row![
        icons::of_file(look.text, &icon_names(files, folder, entry), ICON),
        name,
        middle_column,
        dim(look, when, TIME_COLUMN),
    ]
    .spacing(10)
    .align_y(Center);
    let hover = browser.hover == Some(at);
    let background = if selected {
        Color {
            a: 0.3,
            ..look.accent
        }
    } else if hover {
        look.hover
    } else {
        Color::TRANSPARENT
    };
    mouse_area(
        container(inside)
            .width(Fill)
            .height(Length::Fixed(ROW))
            .padding([0, 12])
            .center_y(Length::Fixed(ROW))
            .style(move |_: &Theme| container::Style {
                background: Some(background.into()),
                border: Border {
                    radius: 4.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }),
    )
    .on_press(Message::Press(id, at))
    .on_double_click(Message::Twice(id, at))
    .on_right_press(Message::RowMenu(id, at))
    .on_enter(Message::Hover(id, at))
    .on_exit(Message::Unhover(id, at))
    .into()
}

/// A column's text in the dim colour, one line, cut at the column's edge.
fn dim<'a>(look: Colors, said: String, width: f32) -> Element<'a, Message> {
    container(
        text(said)
            .size(TEXT_SIZE)
            .color(look.dim)
            .wrapping(text::Wrapping::None),
    )
    .width(Length::Fixed(width))
    .clip(true)
    .into()
}

/// Where a folder is, the way the trash's column says it: Home, a path under home without it in
/// front, or the whole path.
pub fn where_words(files: &Files, folder: &Path) -> String {
    let Some(home) = files.places.first().map(|place| place.path.as_path()) else {
        return folder.display().to_string();
    };
    match folder.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "Home".to_string(),
        Ok(rest) => rest.display().to_string(),
        Err(_) => folder.display().to_string(),
    }
}

/// The icons that can draw a row, the one that fits best first: a place's own folder, any folder,
/// a link to nothing, or the kind of a file.
pub fn icon_names(files: &Files, folder: Option<&Path>, entry: &Entry) -> Vec<String> {
    match entry.kind {
        Kind::Folder => {
            let place = folder.and_then(|folder| {
                librift::files::places::colour_of(&files.places, &folder.join(&entry.name))
            });
            place
                .into_iter()
                .map(ToString::to_string)
                .chain(["folder".to_string(), "inode-directory".to_string()])
                .collect()
        }
        Kind::Broken => vec![
            "inode-symlink".to_string(),
            "application-x-generic".to_string(),
        ],
        Kind::Other => vec!["application-x-generic".to_string()],
        Kind::File => files.types.icons(&entry.mime),
    }
}
