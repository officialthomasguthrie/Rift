//! The dialogs: a name for a new folder, a new name for a file, the question before a name is
//! replaced, and the question before anything is deleted for good. Each stands in the middle of its
//! window with the rest of the window dimmed behind it, the way GNOME's dialogs do: a title, one
//! sentence or a field, and the answers along the bottom.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use iced::widget::text;
use iced::{Element, window};
use librift::files::check_name;
use rift_ui::theme::Colors;
use rift_ui::widgets::{TEXT_SIZE, action, destructive, dialog, primary, wide_field};

use crate::ui::Message;

/// A dialog that is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dialog {
    /// A name for a new folder.
    NewFolder {
        /// What is typed.
        name: String,
    },
    /// A new name for a file or a folder.
    Rename {
        /// Its name now.
        from: OsString,
        /// What is typed.
        name: String,
        /// Whether it is a folder.
        folder: bool,
    },
    /// Delete these for good, asked with Shift and Delete, or because they are somewhere with no
    /// trash.
    Delete {
        /// What is deleted.
        paths: Vec<PathBuf>,
        /// Whether they are somewhere with no trash.
        trashless: bool,
    },
    /// Delete these in the trash for good.
    Forget {
        /// Where each of them lies in the trash it is in.
        files: Vec<PathBuf>,
        /// The names they had, for the question.
        labels: Vec<String>,
    },
    /// Empty the trash.
    Empty,
    /// A copy or a move whose name is already taken in the folder it is going to.
    Replace {
        /// What is being copied or moved.
        from: Vec<PathBuf>,
        /// Where to.
        into: PathBuf,
        /// Whether it is a move.
        moving: bool,
        /// The names that are taken there.
        names: Vec<String>,
    },
}

impl Dialog {
    /// The word `--state` prints for it.
    #[must_use]
    pub const fn word(&self) -> &'static str {
        match self {
            Self::NewFolder { .. } => "new-folder",
            Self::Rename { .. } => "rename",
            Self::Delete {
                trashless: false, ..
            } => "delete",
            Self::Delete {
                trashless: true, ..
            } => "no-trash",
            Self::Forget { .. } => "forget",
            Self::Empty => "empty",
            Self::Replace { .. } => "replace",
        }
    }

    /// What is typed in its field, when it has one.
    #[must_use]
    pub fn typed(&self) -> Option<&str> {
        match self {
            Self::NewFolder { name } | Self::Rename { name, .. } => Some(name),
            _ => None,
        }
    }

    /// Change what is typed in its field.
    pub fn type_in(&mut self, typed: String) {
        if let Self::NewFolder { name } | Self::Rename { name, .. } = self {
            *name = typed;
        }
    }

    /// What is wrong with the name typed in `folder`, as the line under the field. Nothing for a
    /// dialog with no field, and nothing for an empty name, which only dims the button.
    #[must_use]
    pub fn problem(&self, folder: &Path) -> Option<String> {
        let (name, from) = match self {
            Self::NewFolder { name } => (name, None),
            Self::Rename { name, from, .. } => (name, Some(from.as_os_str())),
            _ => return None,
        };
        name_problem(folder, name, from)
    }

    /// Whether its default button can be pressed.
    #[must_use]
    pub fn ready(&self, folder: &Path) -> bool {
        match self.typed() {
            Some(name) => check_name(name).is_ok() && self.problem(folder).is_none(),
            None => true,
        }
    }
}

/// What is wrong with a name for something new in `folder`, or for `from` there under a new name.
/// Nothing when it can be used, and nothing for an empty one.
#[must_use]
pub fn name_problem(folder: &Path, name: &str, from: Option<&OsStr>) -> Option<String> {
    if name.trim().is_empty() {
        return None;
    }
    if let Err(why) = check_name(name) {
        return Some(why.to_string());
    }
    if from == Some(OsStr::new(name)) {
        return None;
    }
    fs::symlink_metadata(folder.join(name))
        .is_ok()
        .then(|| format!("Something called {name} is already here."))
}

/// The id of the field of the dialog in window `number`. Each window has its own, since an
/// operation on a field reaches every window.
#[must_use]
pub fn field_id(number: usize) -> String {
    format!("files-dialog-{number}")
}

/// The dialog as it is drawn.
#[must_use]
pub fn view<'a>(
    shown: &'a Dialog,
    look: Colors,
    id: window::Id,
    number: usize,
    folder: &Path,
) -> Element<'a, Message> {
    let cancel = action(look, "Cancel", Some(Message::Cancel(id)));
    let ready = shown.ready(folder).then_some(Message::Confirm(id));
    match shown {
        Dialog::NewFolder { name } | Dialog::Rename { name, .. } => {
            let (title, button) = match shown {
                Dialog::Rename { folder: true, .. } => ("Rename folder", "Rename"),
                Dialog::Rename { .. } => ("Rename file", "Rename"),
                _ => ("New folder", "Create"),
            };
            let field = wide_field(
                look,
                "Name",
                name,
                field_id(number),
                move |typed| Message::Typed(id, typed),
                Message::Confirm(id),
            );
            let under: Element<'a, Message> = match shown.problem(folder) {
                Some(why) => text(why).size(TEXT_SIZE).color(look.error).into(),
                None if name.starts_with('.') => text("A name that starts with a dot is hidden.")
                    .size(TEXT_SIZE)
                    .color(look.dim)
                    .into(),
                None => text(" ").size(TEXT_SIZE).into(),
            };
            dialog(
                look,
                title.to_string(),
                vec![field, under],
                vec![cancel, primary(look, button, ready)],
            )
        }
        Dialog::Delete { paths, trashless } => {
            let names: Vec<String> = paths
                .iter()
                .map(|path| {
                    path.file_name().map_or_else(
                        || path.display().to_string(),
                        |name| name.to_string_lossy().into_owned(),
                    )
                })
                .collect();
            let said = match (names.len() == 1, trashless) {
                (true, false) => "It is not moved to the trash and cannot be brought back.",
                (false, false) => "They are not moved to the trash and cannot be brought back.",
                (true, true) => "It is on a disk with no trash, so it cannot be brought back.",
                (false, true) => {
                    "They are on a disk with no trash, so they cannot be brought back."
                }
            };
            question(
                look,
                &names,
                said,
                cancel,
                destructive(look, "Delete", ready),
            )
        }
        Dialog::Replace {
            into,
            moving,
            names,
            ..
        } => replacing(look, id, into, *moving, names, cancel, ready),
        Dialog::Forget { labels, .. } => {
            let said = if labels.len() == 1 {
                "It cannot be brought back."
            } else {
                "They cannot be brought back."
            };
            question(
                look,
                labels,
                said,
                cancel,
                destructive(look, "Delete", ready),
            )
        }
        Dialog::Empty => dialog(
            look,
            "Empty the trash?".to_string(),
            vec![sentence(look, "Everything in it is deleted for good.")],
            vec![cancel, destructive(look, "Empty trash", ready)],
        ),
    }
}

/// The question before a name in the folder is replaced. Keeping both is the default answer, the
/// way nothing in Rift is ever written over without being asked; Replace puts what is there in
/// the trash first, so it can still be brought back.
fn replacing<'a>(
    look: Colors,
    id: window::Id,
    into: &Path,
    moving: bool,
    names: &[String],
    cancel: Element<'a, Message>,
    ready: Option<Message>,
) -> Element<'a, Message> {
    let title = match names {
        [one] => format!("Replace {one}?"),
        more => format!("Replace {} items?", more.len()),
    };
    let what = if names.len() == 1 {
        "It is"
    } else {
        "They are"
    };
    let doing = if moving { "moved" } else { "copied" };
    let said = format!(
        "{what} in {} already. What is there goes to the trash, and what is {doing} takes the name.",
        librift::files::shown(into)
    );
    let body: Element<'a, Message> = text(said).size(TEXT_SIZE).color(look.text).into();
    dialog(
        look,
        title,
        vec![body],
        vec![
            cancel,
            action(look, "Replace", Some(Message::Replace(id))),
            primary(look, "Keep both", ready),
        ],
    )
}

/// The question before something is deleted for good: its name, or how many there are.
fn question<'a>(
    look: Colors,
    names: &[String],
    said: &'a str,
    cancel: Element<'a, Message>,
    delete: Element<'a, Message>,
) -> Element<'a, Message> {
    let title = match names {
        [one] => format!("Delete {one} for good?"),
        _ => format!("Delete {} items for good?", names.len()),
    };
    dialog(
        look,
        title,
        vec![sentence(look, said)],
        vec![cancel, delete],
    )
}

fn sentence(look: Colors, said: &str) -> Element<'_, Message> {
    text(said).size(TEXT_SIZE).color(look.text).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_checked_against_the_folder() {
        let folder = std::env::temp_dir().join(format!("files-dialogs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(folder.join("Notes")).unwrap();
        assert_eq!(name_problem(&folder, "", None), None);
        assert_eq!(name_problem(&folder, "Plans", None), None);
        assert_eq!(
            name_problem(&folder, "Notes", None),
            Some("Something called Notes is already here.".to_string())
        );
        // a rename to the name it has is no problem, only nothing to do
        assert_eq!(
            name_problem(&folder, "Notes", Some(OsStr::new("Notes"))),
            None
        );
        assert_eq!(
            name_problem(&folder, "a/b", None),
            Some("A name cannot have a slash in it.".to_string())
        );
        let new = Dialog::NewFolder {
            name: String::new(),
        };
        assert!(!new.ready(&folder));
        let named = Dialog::NewFolder {
            name: "Plans".to_string(),
        };
        assert!(named.ready(&folder));
        assert!(Dialog::Empty.ready(&folder));
        let _ = fs::remove_dir_all(&folder);
    }
}
