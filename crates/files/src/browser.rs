//! One window: the place it shows, where it has been, what is in the place in the order of the
//! list, what is selected, and what stands over the list at the moment, a menu or a dialog. A
//! place is a folder, the trash, or a folder as it was at a moment in the Timeline; a search of
//! the folder and what is under it puts its own rows in the list while the field has words in it.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use iced::{Point, Size};
use librift::files::trash::Trashed;
use librift::files::{self, Entry, Kind, Options, mime};

use crate::dialogs::Dialog;
use crate::menus::Menu;

/// What a window shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Location {
    /// A folder.
    Folder(PathBuf),
    /// The trash.
    Trash,
    /// A folder in home as it was at a moment: the name of the snapshot, and the folder as it is
    /// now. Nothing in it can be changed, since a snapshot is read only.
    Moment {
        /// The snapshot's name, which is the time it was taken.
        at: String,
        /// The folder in home the moment is of.
        folder: PathBuf,
    },
}

impl Location {
    /// The name the window's title and the path bar give it.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Folder(path) => files::shown(path),
            Self::Trash => "Trash".to_string(),
            Self::Moment { folder, .. } => files::shown(folder),
        }
    }

    /// What `--state` prints for it: the folder's path, `trash`, or the moment and its folder.
    #[must_use]
    pub fn word(&self) -> String {
        match self {
            Self::Folder(path) => path.display().to_string(),
            Self::Trash => "trash".to_string(),
            Self::Moment { at, folder } => format!("moment {at} {}", folder.display()),
        }
    }

    /// The folder whose things can be changed, which is a folder now and never a moment.
    #[must_use]
    pub fn folder(&self) -> Option<&Path> {
        match self {
            Self::Folder(path) => Some(path),
            Self::Trash | Self::Moment { .. } => None,
        }
    }

    /// The folder this is about as it is now: the folder itself, or the one a moment is of.
    #[must_use]
    pub fn about(&self) -> Option<&Path> {
        match self {
            Self::Folder(path) | Self::Moment { folder: path, .. } => Some(path),
            Self::Trash => None,
        }
    }

    /// Where its rows are read from: the folder, or the folder inside the snapshot.
    #[must_use]
    pub fn place(&self) -> Option<PathBuf> {
        match self {
            Self::Folder(path) => Some(path.clone()),
            Self::Trash => None,
            Self::Moment { at, folder } => librift::vault::in_snapshot(at, folder),
        }
    }

    /// The moment it is of, when it is one.
    #[must_use]
    pub fn at(&self) -> Option<&str> {
        match self {
            Self::Moment { at, .. } => Some(at),
            Self::Folder(_) | Self::Trash => None,
        }
    }

    /// The place a folder in this one is: a folder now, or the same folder at the same moment.
    #[must_use]
    pub fn inside(&self, name: &std::ffi::OsStr) -> Option<Self> {
        match self {
            Self::Folder(path) => Some(Self::Folder(path.join(name))),
            Self::Moment { at, folder } => Some(Self::Moment {
                at: at.clone(),
                folder: folder.join(name),
            }),
            Self::Trash => None,
        }
    }
}

/// A search of the folder and what is under it, while the header bar's field has words in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    /// What is typed in the field.
    pub words: String,
    /// Whether the rows are the ones the index found, closest in meaning first, rather than the
    /// ones whose name has the words in it.
    pub meaning: bool,
    /// The sentence in place of the files closest in meaning, when there are none to be had.
    pub problem: Option<String>,
}

/// A short line at the bottom of the window about what just happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    /// What it says.
    pub said: String,
    /// The job whose work Undo takes back, when there is one.
    pub undo: Option<u64>,
    /// Which toast of the window this is, so an old one's time running out leaves a new one alone.
    pub number: u64,
}

/// One window's state.
#[derive(Debug)]
pub struct Browser {
    /// Which window of the app this is, counting from one in the order they opened.
    pub number: usize,
    /// What it shows.
    pub location: Location,
    /// Where it was before, the last place last.
    pub back: Vec<Location>,
    /// Where it went back from, the next place last.
    pub forward: Vec<Location>,
    /// Everything read in the folder, in the folder's own order.
    pub read: Vec<Entry>,
    /// What the list shows, in its order: the folder without what is hidden, or the trash.
    pub rows: Vec<Entry>,
    /// Where each thing in the trash was, by where it lies in the trash it is in, which is the
    /// name its row carries: two trashes can hold the same name.
    pub origins: HashMap<OsString, PathBuf>,
    /// Whether the folder has been read since the window went there.
    pub ready: bool,
    /// Why the folder cannot be shown.
    pub problem: Option<String>,
    /// The names of the rows that are selected.
    pub selected: HashSet<OsString>,
    /// The row the keyboard moves from, by name.
    pub cursor: Option<OsString>,
    /// The row a range selected with Shift starts from, by name.
    pub anchor: Option<OsString>,
    /// The row under the pointer.
    pub hover: Option<usize>,
    /// How far the list is scrolled, and how tall the part of it on screen is.
    pub scroll: f32,
    /// How tall the part of the list on screen is.
    pub viewport: f32,
    /// How big the window is.
    pub size: Size,
    /// Where the last press landed, which is where a menu opens.
    pub pointer: Point,
    /// The menu that is open.
    pub menu: Option<Menu>,
    /// The dialog that is open.
    pub dialog: Option<Dialog>,
    /// What is typed in the path bar while it is a field.
    pub typing: Option<String>,
    /// The search in the header bar's field, while it is open.
    pub search: Option<Query>,
    /// Whether the rows are in an order of their own, which the list's own order leaves alone:
    /// the files closest in meaning, closest first.
    pub ranked: bool,
    /// The line at the bottom about what just happened.
    pub toast: Option<Toast>,
    /// How many toasts the window has shown.
    pub toasts: u64,
    /// What the place's own entries said when they were last read, to tell when one changes: the
    /// folder's, or every trash's.
    pub stamp: crate::ui::Stamp,
    /// Names to select once the folder has been read again: what was just made, pasted or left.
    pub select_after: Vec<OsString>,
}

impl Browser {
    /// A window on `location` that has read nothing yet.
    #[must_use]
    pub fn new(number: usize, location: Location, size: Size) -> Self {
        Self {
            number,
            location,
            back: Vec::new(),
            forward: Vec::new(),
            read: Vec::new(),
            rows: Vec::new(),
            origins: HashMap::new(),
            ready: false,
            problem: None,
            selected: HashSet::new(),
            cursor: None,
            anchor: None,
            hover: None,
            scroll: 0.0,
            viewport: 0.0,
            size,
            pointer: Point::ORIGIN,
            menu: None,
            dialog: None,
            typing: None,
            search: None,
            ranked: false,
            toast: None,
            toasts: 0,
            stamp: Vec::new(),
            select_after: Vec::new(),
        }
    }

    /// Go somewhere new: what is shown now goes on the way back, and the way forward is gone.
    pub fn go(&mut self, location: Location) {
        if location == self.location {
            return;
        }
        let was = std::mem::replace(&mut self.location, location);
        self.back.push(was);
        self.forward.clear();
        self.arrive();
    }

    /// Go back one place. False when there is nowhere to go.
    pub fn go_back(&mut self) -> bool {
        let Some(place) = self.back.pop() else {
            return false;
        };
        let was = std::mem::replace(&mut self.location, place);
        self.forward.push(was);
        self.arrive();
        true
    }

    /// Go forward one place. False when there is nowhere to go.
    pub fn go_forward(&mut self) -> bool {
        let Some(place) = self.forward.pop() else {
            return false;
        };
        let was = std::mem::replace(&mut self.location, place);
        self.back.push(was);
        self.arrive();
        true
    }

    /// Everything that belonged to the place the window has left goes.
    fn arrive(&mut self) {
        self.read.clear();
        self.rows.clear();
        self.origins.clear();
        self.ready = false;
        self.problem = None;
        self.selected.clear();
        self.cursor = None;
        self.anchor = None;
        self.hover = None;
        self.scroll = 0.0;
        self.menu = None;
        self.typing = None;
        self.search = None;
        self.ranked = false;
        self.stamp.clear();
        self.select_after.clear();
    }

    /// The folder as it was read, shown the way the options say: the list is made again, and a
    /// selected name that is gone is no longer selected.
    pub fn show(&mut self, read: Vec<Entry>, options: Options) {
        self.read = read;
        self.ready = true;
        self.problem = None;
        self.ranked = false;
        self.arrange(options);
    }

    /// What a search found, each row named by its path under the folder. The ones the index found
    /// keep the order they came in, closest in meaning first.
    pub fn show_found(&mut self, found: Vec<Entry>, ranked: bool, options: Options) {
        self.read = found;
        self.ready = true;
        self.problem = None;
        self.ranked = ranked;
        self.arrange(options);
    }

    /// The words in the search field, when there are any.
    #[must_use]
    pub fn searching(&self) -> Option<&str> {
        self.search
            .as_ref()
            .map(|query| query.words.trim())
            .filter(|words| !words.is_empty())
    }

    /// The trash as it was read, the newest first, with where each thing was.
    pub fn show_trash(&mut self, trashed: Vec<Trashed>, types: &mime::Database, options: Options) {
        self.origins = trashed
            .iter()
            .map(|item| (item.file().into_os_string(), item.path.clone()))
            .collect();
        self.read = trashed
            .into_iter()
            .map(|item| {
                let label = item.label();
                let name = item.file().into_os_string();
                let mime = match item.kind {
                    Kind::Folder => "inode/directory".to_string(),
                    _ => types
                        .by_name(&label)
                        .unwrap_or("application/octet-stream")
                        .to_string(),
                };
                Entry {
                    name,
                    label,
                    kind: item.kind,
                    link: false,
                    size: item.size,
                    modified: item.deleted,
                    mime,
                    hidden: false,
                    items: None,
                }
            })
            .collect();
        self.ready = true;
        self.problem = None;
        self.arrange(Options {
            hidden: true,
            ..options
        });
    }

    /// Put the rows in the order and the filter the options say.
    pub fn arrange(&mut self, options: Options) {
        let shows_hidden = options.hidden || self.location == Location::Trash;
        self.rows = self
            .read
            .iter()
            .filter(|entry| shows_hidden || !entry.hidden)
            .cloned()
            .collect();
        if !self.ranked {
            files::sort(&mut self.rows, options);
        }
        let names: HashSet<&OsString> = self.rows.iter().map(|entry| &entry.name).collect();
        self.selected.retain(|name| names.contains(name));
        for kept in [&mut self.cursor, &mut self.anchor] {
            if kept.as_ref().is_some_and(|name| !names.contains(name)) {
                *kept = None;
            }
        }
        self.hover = None;
    }

    /// Where a row is in the list, by its name.
    #[must_use]
    pub fn position(&self, name: &OsString) -> Option<usize> {
        self.rows.iter().position(|entry| &entry.name == name)
    }

    /// The selected rows, in the order of the list.
    #[must_use]
    pub fn chosen(&self) -> Vec<&Entry> {
        self.rows
            .iter()
            .filter(|entry| self.selected.contains(&entry.name))
            .collect()
    }

    /// The paths of the selected rows, in the order of the list. Nothing in the trash, whose rows
    /// are not in a folder a person picked.
    #[must_use]
    pub fn chosen_paths(&self) -> Vec<PathBuf> {
        let Some(folder) = self.location.folder() else {
            return Vec::new();
        };
        self.chosen()
            .iter()
            .map(|entry| folder.join(&entry.name))
            .collect()
    }

    /// Select one row and nothing else, as a click does.
    pub fn select_only(&mut self, at: usize) {
        let Some(name) = self.rows.get(at).map(|entry| entry.name.clone()) else {
            return;
        };
        self.selected.clear();
        self.selected.insert(name.clone());
        self.cursor = Some(name.clone());
        self.anchor = Some(name);
    }

    /// Add a row to the selection or take it out, as a click with Ctrl does.
    pub fn toggle(&mut self, at: usize) {
        let Some(name) = self.rows.get(at).map(|entry| entry.name.clone()) else {
            return;
        };
        if !self.selected.remove(&name) {
            self.selected.insert(name.clone());
        }
        self.cursor = Some(name.clone());
        self.anchor = Some(name);
    }

    /// Select every row from the anchor to this one, as a click with Shift does.
    pub fn extend_to(&mut self, at: usize) {
        let Some(name) = self.rows.get(at).map(|entry| entry.name.clone()) else {
            return;
        };
        let from = self
            .anchor
            .as_ref()
            .and_then(|anchor| self.position(anchor))
            .unwrap_or(at);
        let (low, high) = (from.min(at), from.max(at));
        self.selected = self.rows[low..=high]
            .iter()
            .map(|entry| entry.name.clone())
            .collect();
        self.cursor = Some(name);
    }

    /// Select every row.
    pub fn select_all(&mut self) {
        self.selected = self.rows.iter().map(|entry| entry.name.clone()).collect();
    }

    /// Select nothing.
    pub fn select_none(&mut self) {
        self.selected.clear();
        self.anchor = None;
    }

    /// Move the keyboard's row by `by` rows, or to either end when `by` is past it, selecting it
    /// alone, or everything from the anchor with `extend`. The row it lands on, when there is one.
    pub fn step(&mut self, by: isize, extend: bool) -> Option<usize> {
        if self.rows.is_empty() {
            return None;
        }
        let last = self.rows.len() - 1;
        let at = match self.cursor.as_ref().and_then(|name| self.position(name)) {
            Some(now) => now.saturating_add_signed(by).min(last),
            // with nothing chosen yet, down starts at the top and up at the bottom
            None if by < 0 => last,
            None => 0,
        };
        if extend {
            if self.anchor.is_none() {
                self.anchor = self
                    .cursor
                    .clone()
                    .or_else(|| Some(self.rows[at].name.clone()));
            }
            self.extend_to(at);
        } else {
            self.select_only(at);
        }
        Some(at)
    }

    /// The first rows of the list that are worth drawing, and how many: the ones on screen with a
    /// few either side. Before the list has said how tall it is, a screenful.
    #[must_use]
    pub fn window_of_rows(&self, row: f32) -> (usize, usize) {
        let tall = if self.viewport > 0.0 {
            self.viewport
        } else {
            1200.0
        };
        let rows = self.rows.len();
        let first = to_row(self.scroll / row).saturating_sub(4).min(rows);
        let shown = (to_row(tall / row) + 9).min(rows - first);
        (first, shown)
    }

    /// Where the list should scroll to for a row to be on screen, when it is not.
    #[must_use]
    pub fn scroll_for(&self, at: usize, row: f32) -> Option<f32> {
        #[allow(clippy::cast_precision_loss)]
        let top = at as f32 * row;
        if top < self.scroll {
            Some(top)
        } else if self.viewport > 0.0 && top + row > self.scroll + self.viewport {
            Some(top + row - self.viewport)
        } else {
            None
        }
    }
}

/// A count of rows from a length in them, never less than none.
fn to_row(rows: f32) -> usize {
    if rows.is_finite() && rows > 0.0 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let whole = rows.floor() as usize;
        whole
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, kind: Kind) -> Entry {
        Entry {
            name: OsString::from(name),
            label: name.to_string(),
            kind,
            link: false,
            size: 1,
            modified: Some(1),
            mime: String::new(),
            hidden: name.starts_with('.'),
            items: None,
        }
    }

    fn browser() -> Browser {
        let mut browser = Browser::new(
            1,
            Location::Folder("/home/rift".into()),
            Size::new(800.0, 600.0),
        );
        browser.show(
            vec![
                entry("b.txt", Kind::File),
                entry("Documents", Kind::Folder),
                entry(".cache", Kind::Folder),
                entry("a.txt", Kind::File),
                entry("c.txt", Kind::File),
            ],
            Options::default(),
        );
        browser
    }

    fn selected(browser: &Browser) -> Vec<String> {
        browser
            .chosen()
            .iter()
            .map(|entry| entry.label.clone())
            .collect()
    }

    #[test]
    fn the_list_leaves_out_what_is_hidden_until_it_is_asked_for() {
        let mut browser = browser();
        let labels = |browser: &Browser| {
            browser
                .rows
                .iter()
                .map(|entry| entry.label.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(labels(&browser), ["Documents", "a.txt", "b.txt", "c.txt"]);
        browser.arrange(Options {
            hidden: true,
            ..Options::default()
        });
        assert_eq!(
            labels(&browser),
            [".cache", "Documents", "a.txt", "b.txt", "c.txt"]
        );
    }

    #[test]
    fn clicks_select_the_way_a_file_manager_does() {
        let mut browser = browser();
        browser.select_only(1);
        assert_eq!(selected(&browser), ["a.txt"]);
        browser.toggle(3);
        assert_eq!(selected(&browser), ["a.txt", "c.txt"]);
        browser.toggle(1);
        assert_eq!(selected(&browser), ["c.txt"]);
        // a range runs from the last click, even one that let a row go
        browser.extend_to(0);
        assert_eq!(selected(&browser), ["Documents", "a.txt"]);
        browser.select_only(3);
        browser.extend_to(0);
        assert_eq!(selected(&browser), ["Documents", "a.txt", "b.txt", "c.txt"]);
        browser.select_none();
        assert!(browser.chosen().is_empty());
        browser.select_all();
        assert_eq!(browser.chosen().len(), 4);
    }

    #[test]
    fn the_keyboard_walks_the_list() {
        let mut browser = browser();
        assert_eq!(browser.step(1, false), Some(0));
        assert_eq!(browser.step(1, false), Some(1));
        assert_eq!(selected(&browser), ["a.txt"]);
        assert_eq!(browser.step(2, true), Some(3));
        assert_eq!(selected(&browser), ["a.txt", "b.txt", "c.txt"]);
        assert_eq!(browser.step(isize::MIN, false), Some(0));
        assert_eq!(browser.step(isize::MAX, false), Some(3));
        assert_eq!(selected(&browser), ["c.txt"]);
    }

    #[test]
    fn a_selected_name_that_is_gone_is_let_go() {
        let mut browser = browser();
        browser.select_all();
        browser.show(vec![entry("a.txt", Kind::File)], Options::default());
        assert_eq!(selected(&browser), ["a.txt"]);
    }

    #[test]
    fn the_way_back_and_forward() {
        let mut browser = browser();
        browser.go(Location::Folder("/home/rift/Documents".into()));
        browser.go(Location::Trash);
        assert!(browser.rows.is_empty());
        assert!(browser.go_back());
        assert_eq!(
            browser.location,
            Location::Folder("/home/rift/Documents".into())
        );
        assert!(browser.go_back());
        assert!(!browser.go_back());
        assert!(browser.go_forward());
        browser.go(Location::Folder("/tmp".into()));
        assert!(
            !browser.go_forward(),
            "going somewhere new ends the way forward"
        );
    }

    #[test]
    fn only_the_rows_on_screen_are_drawn() {
        let mut browser = browser();
        browser.rows = (0..1000)
            .map(|at| entry(&format!("{at}"), Kind::File))
            .collect();
        browser.viewport = 320.0;
        browser.scroll = 3200.0;
        assert_eq!(browser.window_of_rows(32.0), (96, 19));
        assert_eq!(browser.scroll_for(90, 32.0), Some(2880.0));
        assert_eq!(browser.scroll_for(101, 32.0), None);
        assert_eq!(browser.scroll_for(120, 32.0), Some(3552.0));
        browser.scroll = 32_000.0;
        let (first, shown) = browser.window_of_rows(32.0);
        assert!(first + shown <= 1000);
    }
}
