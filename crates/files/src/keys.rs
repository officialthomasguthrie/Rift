//! The keys a file manager answers, the ones GNOME's Files and the other file managers share:
//! arrows through the list, Enter to open, Delete to the trash, F2 to rename, the clipboard on
//! Ctrl and C, X and V, Alt and the arrows through the history.

use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

use crate::ui::Act;

/// What a key asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Press {
    /// Something the menus do too.
    Act(Act),
    /// Move through the list this many rows, extending the selection when Shift is down.
    Step(isize, bool),
    /// Move through the list this many screenfuls.
    Page(isize, bool),
    /// Back through the history.
    Back,
    /// Forward through it.
    Forward,
    /// To the folder this one is in.
    Up,
    /// Home.
    Home,
}

/// What a key pressed with these modifiers asks for, in the trash or in a folder. Nothing for a
/// key the file manager leaves alone.
#[must_use]
pub fn press(key: &Key, modifiers: Modifiers, trash: bool) -> Option<Press> {
    let shift = modifiers.shift();
    let command = modifiers.control();
    let alt = modifiers.alt();
    match key.as_ref() {
        Key::Named(Named::ArrowDown) if alt => None,
        Key::Named(Named::ArrowUp) if alt => Some(Press::Up),
        Key::Named(Named::ArrowLeft) if alt => Some(Press::Back),
        Key::Named(Named::ArrowRight) if alt => Some(Press::Forward),
        Key::Named(Named::Home) if alt => Some(Press::Home),
        Key::Named(Named::ArrowDown) => Some(Press::Step(1, shift)),
        Key::Named(Named::ArrowUp) => Some(Press::Step(-1, shift)),
        Key::Named(Named::Home) => Some(Press::Step(isize::MIN, shift)),
        Key::Named(Named::End) => Some(Press::Step(isize::MAX, shift)),
        Key::Named(Named::PageDown) => Some(Press::Page(1, shift)),
        Key::Named(Named::PageUp) => Some(Press::Page(-1, shift)),
        Key::Named(Named::Enter) => Some(Press::Act(Act::Open)),
        Key::Named(Named::Backspace) => Some(Press::Back),
        Key::Named(Named::F2) if !trash => Some(Press::Act(Act::Rename)),
        Key::Named(Named::F5) => Some(Press::Act(Act::Reload)),
        Key::Named(Named::Delete) if trash => Some(Press::Act(Act::Forget)),
        Key::Named(Named::Delete) if shift => Some(Press::Act(Act::Delete)),
        Key::Named(Named::Delete) => Some(Press::Act(Act::Trash)),
        Key::Character(letter) if command => shortcut(letter, shift, trash),
        _ => None,
    }
}

/// The shortcuts with Ctrl.
fn shortcut(letter: &str, shift: bool, trash: bool) -> Option<Press> {
    let act = match (letter.to_ascii_lowercase().as_str(), shift) {
        ("n", true) if !trash => Act::NewFolder,
        ("n", false) => Act::NewWindow,
        ("a", _) => Act::SelectAll,
        ("h", _) => Act::Hidden,
        ("l", _) => Act::Location,
        ("f", _) => Act::Search,
        ("r", _) => Act::Reload,
        ("w" | "q", _) => Act::Close,
        ("c", _) if !trash => Act::Copy,
        ("x", _) if !trash => Act::Cut,
        ("v", _) if !trash => Act::Paste,
        _ => return None,
    };
    Some(Press::Act(act))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(key: Named) -> Key {
        Key::Named(key)
    }

    #[test]
    fn the_keys_a_file_manager_answers() {
        let none = Modifiers::empty();
        assert_eq!(
            press(&named(Named::ArrowDown), none, false),
            Some(Press::Step(1, false))
        );
        assert_eq!(
            press(&named(Named::ArrowUp), Modifiers::SHIFT, false),
            Some(Press::Step(-1, true))
        );
        assert_eq!(
            press(&named(Named::ArrowUp), Modifiers::ALT, false),
            Some(Press::Up)
        );
        assert_eq!(
            press(&named(Named::Enter), none, false),
            Some(Press::Act(Act::Open))
        );
        assert_eq!(
            press(&named(Named::Delete), none, false),
            Some(Press::Act(Act::Trash))
        );
        assert_eq!(
            press(&named(Named::Delete), Modifiers::SHIFT, false),
            Some(Press::Act(Act::Delete))
        );
        // in the trash, Delete is for good, and there is nothing to rename
        assert_eq!(
            press(&named(Named::Delete), none, true),
            Some(Press::Act(Act::Forget))
        );
        assert_eq!(press(&named(Named::F2), none, true), None);
        let ctrl = Modifiers::CTRL;
        let letter = |c: &str| Key::Character(c.into());
        assert_eq!(
            press(&letter("c"), ctrl, false),
            Some(Press::Act(Act::Copy))
        );
        assert_eq!(
            press(&letter("V"), ctrl, false),
            Some(Press::Act(Act::Paste))
        );
        assert_eq!(
            press(&letter("n"), ctrl | Modifiers::SHIFT, false),
            Some(Press::Act(Act::NewFolder))
        );
        assert_eq!(
            press(&letter("n"), ctrl, false),
            Some(Press::Act(Act::NewWindow))
        );
        assert_eq!(press(&letter("c"), ctrl, true), None);
        assert_eq!(press(&letter("c"), none, false), None);
    }
}
