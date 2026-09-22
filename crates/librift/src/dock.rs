//! The dock's settings: the apps it keeps, in their order, and where and how big it is drawn.
//!
//! The shell draws the dock and writes the list when an app is pinned from its menu. Settings
//! writes the list and the rest from the Dock page, then tells the shell to read both again. The
//! list is one app id per line in `~/.config/rift/dock`; the rest is a line each in
//! `~/.config/rift/dock-options`, in the words `rift-settings --set` takes.

use std::fs;

use crate::appearance::{home, write_beside};

/// Where the list of apps the dock keeps lives, under home.
pub const LIST: &str = ".config/rift/dock";
/// Where the rest of the dock's settings live, under home.
pub const OPTIONS: &str = ".config/rift/dock-options";

/// The apps the dock keeps until the owner has a list of their own: the browser, the terminal, the
/// editor and Settings, the four of the image a person opens first.
pub const KEPT: [&str; 4] = [
    "firefox",
    "com.mitchellh.ghostty",
    "dev.zed.Zed",
    "dev.rift.Settings",
];

/// The names of the settings, in the order the file and `rift-settings --state` have them.
pub const NAMES: [&str; 4] = ["dock-position", "dock-extend", "dock-icons", "dock-hide"];

/// The apps the dock keeps, in their order. The four the image ships with when the owner has not
/// said otherwise.
#[must_use]
pub fn pinned() -> Vec<String> {
    match home().and_then(|home| fs::read_to_string(home.join(LIST)).ok()) {
        Some(text) => read(&text),
        None => KEPT.iter().map(|&key| key.to_string()).collect(),
    }
}

/// The list a file holds: one app id per line, blank lines and notes left out, each app once.
#[must_use]
pub fn read(text: &str) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !keys.iter().any(|kept| kept == line) {
            keys.push(line.to_string());
        }
    }
    keys
}

/// Write the list back, so a restart of the shell finds it the way the owner left it.
///
/// # Errors
///
/// A sentence when there is no home or the file could not be written.
pub fn save(pinned: &[String]) -> Result<(), String> {
    let home = home().ok_or("There is no home to keep the dock's apps in.")?;
    let mut text = String::new();
    for key in pinned {
        text.push_str(key);
        text.push('\n');
    }
    write_beside(&home.join(LIST), &text).map(|_| ())
}

/// A change to the list of apps the dock keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Keep this app, at the end.
    Pin(String),
    /// Stop keeping it.
    Unpin(String),
    /// Move it one place towards the start.
    Up(String),
    /// Move it one place towards the end.
    Down(String),
}

impl Change {
    /// The list after the change, or nothing when it changes nothing: an app already kept, one that
    /// is not, the first moved up or the last moved down.
    #[must_use]
    pub fn apply(&self, list: &[String]) -> Option<Vec<String>> {
        let at = |key: &str| list.iter().position(|kept| kept == key);
        let mut after = list.to_vec();
        match self {
            Self::Pin(key) => {
                if key.trim().is_empty() || at(key).is_some() {
                    return None;
                }
                after.push(key.clone());
            }
            Self::Unpin(key) => {
                after.remove(at(key)?);
            }
            Self::Up(key) => {
                let found = at(key).filter(|found| *found > 0)?;
                after.swap(found, found - 1);
            }
            Self::Down(key) => {
                let found = at(key).filter(|found| found + 1 < list.len())?;
                after.swap(found, found + 1);
            }
        }
        Some(after)
    }
}

/// The edge of the screen the dock stands on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Edge {
    /// Along the bottom, under the windows.
    #[default]
    Bottom,
    /// Along the top, under the bar.
    Top,
}

impl Edge {
    /// The word the file and `rift-settings --set` use.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Bottom => "bottom",
            Self::Top => "top",
        }
    }
}

/// How big the icons in the dock are, and with them the dock.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Size {
    /// 32 pixels, in a dock 44 tall.
    #[default]
    Small,
    /// 40 pixels.
    Medium,
    /// 48 pixels.
    Large,
}

impl Size {
    /// Every size, smallest first.
    pub const ALL: [Self; 3] = [Self::Small, Self::Medium, Self::Large];

    /// The word the file and `rift-settings --set` use.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }

    /// What the page calls it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Small => "Small",
            Self::Medium => "Medium",
            Self::Large => "Large",
        }
    }

    /// How big an app's icon is drawn, in logical pixels.
    #[must_use]
    pub const fn icon(self) -> u32 {
        match self {
            Self::Small => 32,
            Self::Medium => 40,
            Self::Large => 48,
        }
    }
}

/// Where the dock stands and how it is drawn. None of it is the default but the first of each:
/// flush with the bottom edge, from one side of the screen to the other, with 32 pixel icons, and
/// always there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// The edge it stands on.
    pub edge: Edge,
    /// Whether it runs from one side of the screen to the other. When it does not, it is as wide
    /// as what it holds and stands in the middle of its edge, a gap away from it.
    pub extend: bool,
    /// How big its icons are.
    pub size: Size,
    /// Whether it hides until the pointer reaches the bottom edge of the screen. Windows then have
    /// the room it stood in. Along the top it never hides: the bar holds that edge.
    pub hide: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            edge: Edge::Bottom,
            extend: true,
            size: Size::Small,
            hide: false,
        }
    }
}

impl Options {
    /// The owner's settings, or the image's own where there is no file or a line is not there.
    #[must_use]
    pub fn read() -> Self {
        home()
            .and_then(|home| fs::read_to_string(home.join(OPTIONS)).ok())
            .map_or_else(Self::default, |text| Self::parse(&text))
    }

    /// The settings a file holds, the image's own for any it does not.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut options = Self::default();
        for line in text.lines() {
            if let Some((name, value)) = line.trim().split_once(' ') {
                options.set(name, value);
            }
        }
        options
    }

    /// Change one setting by its name, the way the file and `rift-settings --set` name it. Whether
    /// the name and the value were both understood.
    pub fn set(&mut self, name: &str, value: &str) -> bool {
        let value = value.trim().to_ascii_lowercase();
        match (name, value.as_str()) {
            ("dock-position", "bottom") => self.edge = Edge::Bottom,
            ("dock-position", "top") => self.edge = Edge::Top,
            ("dock-extend", "on") => self.extend = true,
            ("dock-extend", "off") => self.extend = false,
            ("dock-hide", "on") => self.hide = true,
            ("dock-hide", "off") => self.hide = false,
            ("dock-icons", word) => {
                let Some(size) = Size::ALL.into_iter().find(|size| size.word() == word) else {
                    return false;
                };
                self.size = size;
            }
            _ => return false,
        }
        true
    }

    /// Whether the dock hides where it stands: along the bottom with hiding on. The pointer comes
    /// to the top edge on its way to the bar's buttons, so a dock along the top stays.
    #[must_use]
    pub const fn hides(&self) -> bool {
        self.hide && matches!(self.edge, Edge::Bottom)
    }

    /// A line for each setting, its name and its value, in the order of [`NAMES`].
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let word = |on: bool| if on { "on" } else { "off" };
        let values = [
            self.edge.word(),
            word(self.extend),
            self.size.word(),
            word(self.hide),
        ];
        NAMES
            .iter()
            .zip(values)
            .map(|(name, value)| format!("{name} {value}"))
            .collect()
    }

    /// Write the settings, which the shell reads when it is told to.
    ///
    /// # Errors
    ///
    /// A sentence when there is no home or the file could not be written.
    pub fn save(&self) -> Result<(), String> {
        let home = home().ok_or("There is no home to keep the dock's settings in.")?;
        write_beside(&home.join(OPTIONS), &(self.lines().join("\n") + "\n")).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|&key| key.to_string()).collect()
    }

    #[test]
    fn the_list_is_one_id_per_line() {
        let text = "# the apps in the dock\nfirefox\n\n  com.mitchellh.ghostty  \nfirefox\n";
        assert_eq!(read(text), ["firefox", "com.mitchellh.ghostty"]);
        assert!(read("").is_empty());
    }

    #[test]
    fn a_change_moves_or_takes_off_the_app_it_names() {
        let kept = list(&["firefox", "com.mitchellh.ghostty", "dev.zed.Zed"]);
        let change = |change: Change| change.apply(&kept);
        assert_eq!(
            change(Change::Up("dev.zed.Zed".into())),
            Some(list(&["firefox", "dev.zed.Zed", "com.mitchellh.ghostty"]))
        );
        assert_eq!(
            change(Change::Down("firefox".into())),
            Some(list(&["com.mitchellh.ghostty", "firefox", "dev.zed.Zed"]))
        );
        assert_eq!(
            change(Change::Unpin("com.mitchellh.ghostty".into())),
            Some(list(&["firefox", "dev.zed.Zed"]))
        );
        assert_eq!(
            change(Change::Pin("Helix".into())),
            Some(list(&[
                "firefox",
                "com.mitchellh.ghostty",
                "dev.zed.Zed",
                "Helix"
            ]))
        );
    }

    #[test]
    fn a_change_that_changes_nothing_is_nothing() {
        let kept = list(&["firefox", "dev.zed.Zed"]);
        assert_eq!(Change::Up("firefox".into()).apply(&kept), None);
        assert_eq!(Change::Down("dev.zed.Zed".into()).apply(&kept), None);
        assert_eq!(Change::Unpin("Helix".into()).apply(&kept), None);
        assert_eq!(Change::Up("Helix".into()).apply(&kept), None);
        assert_eq!(Change::Pin("firefox".into()).apply(&kept), None);
        assert_eq!(Change::Pin(" ".into()).apply(&kept), None);
    }

    #[test]
    fn the_settings_read_back_the_way_they_are_written() {
        let options = Options::default();
        assert_eq!(
            options.lines(),
            [
                "dock-position bottom",
                "dock-extend on",
                "dock-icons small",
                "dock-hide off"
            ]
        );
        let mut chosen = options;
        assert!(chosen.set("dock-position", "Top"));
        assert!(chosen.set("dock-extend", "off"));
        assert!(chosen.set("dock-icons", "large"));
        assert!(chosen.set("dock-hide", "on"));
        assert_eq!(Options::parse(&chosen.lines().join("\n")), chosen);
        assert_eq!(chosen.size.icon(), 48);
        // a dock along the top stays whatever the setting says, and hides again along the bottom
        assert!(chosen.hide && !chosen.hides());
        assert!(chosen.set("dock-position", "bottom"));
        assert!(chosen.hides());
        // a file written before there was a setting for hiding reads as a dock that stays
        assert!(!Options::parse("dock-position bottom\ndock-extend on\ndock-icons small\n").hide);
        // a word that is not one changes nothing
        assert!(!chosen.set("dock-position", "left"));
        assert!(!chosen.set("dock-icons", "huge"));
        assert!(!chosen.set("autohide", "on"));
        assert_eq!(chosen.edge, Edge::Bottom);
        // and a file with nothing in it is the image's own dock
        assert_eq!(Options::parse(""), Options::default());
    }
}
