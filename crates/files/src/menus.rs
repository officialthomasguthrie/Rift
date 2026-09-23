//! The menus: a right click on the selection, a right click on the empty part of the list, and the
//! button at the right end of the header bar. A menu opens where the pointer is, or under its
//! button, and moves in from an edge it would run over.

use iced::{Point, Size, window};
use librift::files::Kind;
use rift_ui::widgets::{Item, menu_height, menu_width};

use crate::browser::{Browser, Location};
use crate::ui::{Act, Files, Message};

/// How far a menu stays from the edges of the window.
const MARGIN: f32 = 4.0;

/// Which menu is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    /// What can be done with the selection.
    Selection,
    /// What can be done in the folder.
    Folder,
    /// The header bar's menu.
    Main,
}

impl Which {
    /// The word `--state` prints and `--set menu` takes.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Selection => "selection",
            Self::Folder => "folder",
            Self::Main => "main",
        }
    }

    /// The menu a word names.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        [Self::Selection, Self::Folder, Self::Main]
            .into_iter()
            .find(|which| which.word() == word.trim())
    }
}

/// An app that opens the selection: its id and its name.
pub type Opener = (String, String);

/// A menu that is open.
#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    /// Which it is.
    pub which: Which,
    /// Where its top left corner is in the window.
    pub at: Point,
    /// The app the selection opens with, found as the menu opened.
    pub default: Option<Opener>,
    /// The other apps that say they open it.
    pub others: Vec<Opener>,
}

/// The rows of the menu that is open.
#[must_use]
pub fn items(files: &Files, browser: &Browser, id: window::Id, menu: &Menu) -> Vec<Item<Message>> {
    let act = |act: Act| Some(Message::Do(id, act));
    let in_trash = browser.location == Location::Trash;
    let hidden = Item::ticked(
        "Show hidden files",
        files.options.hidden,
        Message::Do(id, Act::Hidden),
    );
    // nothing in a moment can be changed: a snapshot is read only, and what it holds is brought
    // back into the folder rather than worked on where it lies
    if let Some(items) = in_a_moment(browser, id, menu.which) {
        return items;
    }
    match (menu.which, in_trash) {
        (Which::Selection, false) => selection(browser, id, menu),
        (Which::Selection, true) => vec![
            Item::new("Restore", act(Act::Restore)),
            Item::new("Delete for good", act(Act::Forget)),
        ],
        (Which::Folder, false) => vec![
            Item::new("New folder", act(Act::NewFolder)),
            Item::new(
                "Paste",
                files.clipboard.as_ref().and_then(|_| act(Act::Paste)),
            ),
            Item::Line,
            Item::new("Select all", act(Act::SelectAll)),
            hidden,
            Item::Line,
            Item::new("Open in terminal", act(Act::Terminal)),
        ],
        (Which::Folder, true) => vec![
            Item::new(
                "Empty trash",
                (!browser.rows.is_empty()).then(|| Message::Do(id, Act::Empty)),
            ),
            Item::new("Select all", act(Act::SelectAll)),
        ],
        (Which::Main, false) => {
            let mut items = vec![
                Item::new("New window", act(Act::NewWindow)),
                Item::new("New folder", act(Act::NewFolder)),
                Item::Line,
                Item::new("Search", act(Act::Search)),
            ];
            if browser
                .location
                .about()
                .is_some_and(crate::timeline::covers)
            {
                items.push(Item::new("Timeline", act(Act::Timeline)));
            }
            items.extend([
                Item::Line,
                hidden,
                Item::Line,
                Item::new("Open in terminal", act(Act::Terminal)),
            ]);
            items
        }
        (Which::Main, true) => vec![
            Item::new("New window", act(Act::NewWindow)),
            Item::new(
                "Empty trash",
                (!browser.rows.is_empty()).then(|| Message::Do(id, Act::Empty)),
            ),
        ],
    }
}

/// The menus of a moment in the Timeline: what is selected is opened as it was or put back, and
/// the folder itself can be put back whole. `None` when the window is not in one.
fn in_a_moment(browser: &Browser, id: window::Id, which: Which) -> Option<Vec<Item<Message>>> {
    browser.location.at()?;
    let act = |act: Act| Some(Message::Do(id, act));
    let items = match which {
        Which::Selection => vec![
            Item::new("Open", act(Act::Open)),
            Item::Line,
            Item::new("Restore", act(Act::Bring)),
        ],
        Which::Folder => vec![
            Item::new("Restore everything", act(Act::Bring)),
            Item::new("Select all", act(Act::SelectAll)),
        ],
        Which::Main => vec![
            Item::new("New window", act(Act::NewWindow)),
            Item::new("Restore everything", act(Act::Bring)),
            Item::Line,
            Item::new("Back to now", act(Act::Now)),
        ],
    };
    Some(items)
}

/// What can be done with the selection in a folder: open it, with the app it opens with or
/// another, cut or copy it, rename one thing, and move it to the trash.
fn selection(browser: &Browser, id: window::Id, menu: &Menu) -> Vec<Item<Message>> {
    let act = |act: Act| Some(Message::Do(id, act));
    let chosen = browser.chosen();
    let one = chosen.len() == 1;
    let mut items = Vec::new();
    if chosen.iter().all(|entry| entry.kind == Kind::Folder) {
        items.push(Item::new("Open", act(Act::Open)));
        if one {
            items.push(Item::new("Open in new window", act(Act::OpenWindow)));
        }
    } else {
        match &menu.default {
            Some((_, name)) => items.push(Item::new(format!("Open with {name}"), act(Act::Open))),
            None => items.push(Item::new("Open", None)),
        }
        for (app, name) in menu.others.iter().take(3) {
            items.push(Item::new(
                format!("Open with {name}"),
                act(Act::OpenWith(app.clone())),
            ));
        }
    }
    items.push(Item::Line);
    items.push(Item::new("Cut", act(Act::Cut)));
    items.push(Item::new("Copy", act(Act::Copy)));
    if one {
        items.push(Item::new("Rename", act(Act::Rename)));
    }
    items.push(Item::Line);
    items.push(Item::new("Move to trash", act(Act::Trash)));
    items
}

/// Where a menu of these rows goes for a press at `at` in a window of `size`: at the pointer,
/// moved in from the right edge, and above the pointer when there is no room under it.
#[must_use]
pub fn place(at: Point, size: Size, items: &[Item<Message>]) -> Point {
    let tall = menu_height(items);
    let x =
        at.x.min(size.width - menu_width(items) - MARGIN)
            .max(MARGIN);
    let y = if at.y + tall > size.height - MARGIN {
        (at.y - tall).max(MARGIN)
    } else {
        at.y
    };
    Point::new(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_stays_inside_the_window() {
        let size = Size::new(800.0, 600.0);
        let items: Vec<Item<Message>> = vec![Item::new("One", None), Item::new("Two", None)];
        assert_eq!(
            place(Point::new(100.0, 100.0), size, &items),
            Point::new(100.0, 100.0)
        );
        let near_the_corner = place(Point::new(790.0, 590.0), size, &items);
        assert!((near_the_corner.x - (800.0 - menu_width(&items) - MARGIN)).abs() < f32::EPSILON);
        assert!(near_the_corner.y + menu_height(&items) <= 590.0);
        assert_eq!(Which::from_word("folder"), Some(Which::Folder));
        assert_eq!(Which::from_word("nowhere"), None);
    }
}
