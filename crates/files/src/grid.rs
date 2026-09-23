//! The other way to show a folder: a tile for each thing, with the picture of a file that has one
//! and the icon of its kind where it has not, and its name under it. The main menu and Ctrl and G
//! switch between the grid and the list, and the choice is kept for the next window.
//!
//! Like the list, only the tiles on screen are drawn, and the pictures are made on a thread; see
//! [`crate::thumbs`].

use crate::browser::Browser;
use crate::icons;
use crate::list::{icon_names, list_id};
use crate::theme::Colors;
use crate::ui::{Files, Message};
use crate::view::SIDEBAR;
use crate::widgets::{TEXT_SIZE, fill, scroll};
use iced::widget::{column, container, image, mouse_area, row, space, text};
use iced::{Border, Center, Color, Element, Fill, Length, Theme, window};

/// How wide a tile is.
pub const TILE: f32 = 132.0;
/// How tall a tile is: room for the picture and two lines of the name.
pub const TALL: f32 = 152.0;
/// How much room the picture of a tile has.
const PICTURE: f32 = 84.0;
/// How big the icon of a file with no picture is drawn.
const ICON: f32 = 48.0;
/// How much room the name under a picture has: two lines of it.
const NAME: f32 = 36.0;

/// How many tiles stand across the part of a window the folder is in.
#[must_use]
pub fn across(width: f32) -> usize {
    let inside = (width - SIDEBAR - 24.0).max(TILE);
    crate::browser::to_row(inside / TILE).max(1)
}

/// The first row of tiles worth drawing and how many there are: the ones on screen with a row
/// either side, as entries of the list rather than rows of tiles.
#[must_use]
pub fn on_screen(browser: &Browser) -> (usize, usize) {
    let columns = across(browser.size.width);
    let tall = if browser.viewport > 0.0 {
        browser.viewport
    } else {
        800.0
    };
    let first = crate::browser::to_row(browser.scroll / TALL).saturating_sub(1) * columns;
    let first = first.min(browser.rows.len());
    let shown =
        ((crate::browser::to_row(tall / TALL) + 3) * columns).min(browser.rows.len() - first);
    (first, shown)
}

/// Where the grid should scroll to for a tile to be on screen, when it is not.
#[must_use]
pub fn scroll_for(browser: &Browser, at: usize) -> Option<f32> {
    browser.scroll_for(at / across(browser.size.width), TALL)
}

/// The grid of a window, in the place the list would be.
#[must_use]
pub fn view<'a>(
    files: &'a Files,
    id: window::Id,
    browser: &'a Browser,
    look: Colors,
) -> Element<'a, Message> {
    let columns = across(browser.size.width);
    let (first, shown) = on_screen(browser);
    let lines = browser.rows.len().div_ceil(columns);
    #[allow(clippy::cast_precision_loss)]
    let above = (first / columns) as f32 * TALL;
    #[allow(clippy::cast_precision_loss)]
    let below = (lines - (first + shown).div_ceil(columns)) as f32 * TALL;
    let mut grid = column![space().height(above)].width(Fill).padding([0, 8]);
    let mut at = first;
    while at < first + shown {
        let end = (at + columns).min(first + shown);
        let mut line = row![].spacing(0).align_y(Center);
        for which in at..end {
            line = line.push(tile(files, id, browser, which, look));
        }
        grid = grid.push(line);
        at = end;
    }
    grid = grid.push(space().height(below));
    container(
        scroll(look, grid)
            .id(list_id(browser.number))
            .on_scroll(move |viewport| Message::Scrolled(id, viewport))
            .height(Fill),
    )
    .width(Fill)
    .height(Fill)
    .style(move |_: &Theme| fill(look.view))
    .into()
}

/// One tile: its picture, its name under it, pressed to select it and twice to open it.
fn tile<'a>(
    files: &'a Files,
    id: window::Id,
    browser: &'a Browser,
    at: usize,
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
    let drawn = folder
        .map(|folder| folder.join(&entry.name))
        .and_then(|path| files.thumbs.of(&path, entry.modified));
    let picture: Element<'a, Message> = match drawn {
        Some(path) => image(image::Handle::from_path(path))
            .width(Fill)
            .height(Length::Fixed(PICTURE))
            .content_fit(iced::ContentFit::Contain)
            .into(),
        None => container(icons::of_file(
            look.text,
            &icon_names(files, folder, entry),
            ICON,
        ))
        .width(Fill)
        .height(Length::Fixed(PICTURE))
        .center(Fill)
        .into(),
    };
    let name = container(
        text(entry.label.as_str())
            .size(TEXT_SIZE)
            .color(colour)
            .center()
            .wrapping(text::Wrapping::WordOrGlyph),
    )
    .width(Fill)
    .height(Length::Fixed(NAME))
    .clip(true)
    .center_x(Fill);
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
    // the box that is marked when the tile is selected sits inside the tile, so the tiles stand a
    // few pixels apart and every one is the same width across the grid
    let marked = container(column![picture, name].spacing(4).align_x(Center))
        .width(Fill)
        .height(Fill)
        .padding(6)
        .style(move |_: &Theme| container::Style {
            background: Some(background.into()),
            border: Border {
                radius: 6.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });
    mouse_area(
        container(marked)
            .width(Length::Fixed(TILE))
            .height(Length::Fixed(TALL))
            .padding(4),
    )
    .on_press(Message::Press(id, at))
    .on_double_click(Message::Twice(id, at))
    .on_right_press(Message::RowMenu(id, at))
    .on_enter(Message::Hover(id, at))
    .on_exit(Message::Unhover(id, at))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::Location;
    use iced::Size;
    use librift::files::{Entry, Kind, Options};

    fn browser(wide: f32, rows: usize) -> Browser {
        let mut browser = Browser::new(
            1,
            Location::Folder("/home/rift/Pictures".into()),
            Size::new(wide, 640.0),
        );
        browser.show(
            (0..rows)
                .map(|at| Entry {
                    name: format!("{at}.png").into(),
                    label: format!("{at}.png"),
                    kind: Kind::File,
                    link: false,
                    size: 1,
                    modified: Some(1),
                    mime: "image/png".to_string(),
                    hidden: false,
                    items: None,
                })
                .collect(),
            Options::default(),
        );
        browser
    }

    #[test]
    fn the_tiles_on_screen_are_the_ones_drawn() {
        let mut browser = browser(960.0, 100);
        // 960 wide: the sidebar and the padding leave room for five tiles across
        assert_eq!(across(browser.size.width), 5);
        assert_eq!(across(400.0), 1);
        browser.viewport = 384.0;
        browser.scroll = 0.0;
        let (first, shown) = on_screen(&browser);
        assert_eq!(first, 0);
        assert_eq!(
            shown, 25,
            "five rows of five: the two on screen and three more"
        );
        browser.scroll = 512.0;
        let (first, shown) = on_screen(&browser);
        assert_eq!(first, 10, "the row above the first on screen");
        assert!(first + shown <= 100);
        assert_eq!(scroll_for(&browser, 0), Some(0.0));
        assert_eq!(scroll_for(&browser, 17), Some(3.0 * TALL), "the row above");
        assert_eq!(scroll_for(&browser, 21), None, "already on screen");
        assert_eq!(scroll_for(&browser, 99), Some(19.0 * TALL + TALL - 384.0));
    }
}
