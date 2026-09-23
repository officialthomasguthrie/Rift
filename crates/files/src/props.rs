//! What the Properties dialog says about what is selected: its name, its kind, how big it is,
//! where it is, when it last changed, what may be done with it, and how many pixels across a
//! picture is. The facts that are there at once come from the row itself; the size of a folder and
//! the size of a picture are worked out on a thread, and the dialog fills them in when they come.

use std::path::{Path, PathBuf};
use std::thread;

use iced::futures::channel::oneshot;
use iced::{Task, window};
use librift::files::{self, Entry, Kind};

use crate::browser::Browser;
use crate::ui::{Files, Message};

/// A folder is walked no further than this, so a dialog about home does not read the whole drive.
const DEEPEST: usize = 24;

/// What the dialog says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    /// The name of the one thing, or how many things there are.
    pub label: String,
    /// What it is: a folder, a link, or the type of the file.
    pub kind: String,
    /// The folder it is in, the way the trash says where a thing was.
    pub place: String,
    /// How big it is in words, once that is known.
    pub size: Option<String>,
    /// What a folder holds, once that is counted.
    pub items: Option<u64>,
    /// When it last changed.
    pub when: Option<String>,
    /// What may be done with it, in the letters every system writes them in.
    pub mode: Option<String>,
    /// How many pixels across and down a picture is.
    pub pixels: Option<(u32, u32)>,
    /// How many things are selected, when it is more than one.
    pub several: usize,
    /// The files the size is being worked out for.
    pub paths: Vec<PathBuf>,
}

impl Facts {
    /// The rows of the dialog, and the lines `--state` prints, a label and what it says.
    #[must_use]
    pub fn rows(&self) -> Vec<(&'static str, String)> {
        let mut rows = vec![("Name", self.label.clone()), ("Kind", self.kind.clone())];
        rows.push((
            "Size",
            match (&self.size, self.items) {
                (Some(size), Some(items)) => {
                    format!("{size}, {}", files::items_words(items).to_lowercase())
                }
                (Some(size), None) => size.clone(),
                (None, _) => "Working it out".to_string(),
            },
        ));
        rows.push(("Where", self.place.clone()));
        if let Some(when) = &self.when {
            rows.push(("Changed", when.clone()));
        }
        if let Some((wide, tall)) = self.pixels {
            rows.push(("Pixels", format!("{wide} by {tall}")));
        }
        if let Some(mode) = &self.mode {
            rows.push(("Permissions", mode.clone()));
        }
        rows
    }
}

/// What the window can say about what is selected now, or nothing when nothing is.
#[must_use]
pub fn facts(files: &Files, browser: &Browser) -> Option<Facts> {
    let chosen = browser.chosen();
    let folder = browser.location.place()?;
    let first = chosen.first()?;
    let paths: Vec<PathBuf> = chosen
        .iter()
        .map(|entry| folder.join(&entry.name))
        .collect();
    let one = chosen.len() == 1;
    let path = folder.join(&first.name);
    let in_folder = path.parent().map_or(folder.clone(), Path::to_path_buf);
    Some(Facts {
        label: if one {
            first.label.clone()
        } else {
            files::items_words(chosen.len() as u64)
        },
        kind: if one {
            kind_words(files, first, &path)
        } else {
            "Several things".to_string()
        },
        place: crate::list::where_words(files, &in_folder),
        size: (one && first.kind == Kind::File).then(|| files::size_words(first.size)),
        items: (one && first.kind == Kind::Folder)
            .then_some(first.items)
            .flatten(),
        when: one
            .then_some(first.modified)
            .flatten()
            .map(|seconds| files::when_moment(seconds, librift::time::now(), files.offset)),
        mode: one.then(|| mode_words(&path)).flatten(),
        pixels: None,
        several: if one { 0 } else { chosen.len() },
        paths,
    })
}

/// What a thing is, in the words the dialog gives it: a folder, a link and where it points, or the
/// type of the file.
fn kind_words(files: &Files, entry: &Entry, path: &Path) -> String {
    let what = match entry.kind {
        Kind::Folder => "Folder".to_string(),
        Kind::Broken => "Link to nothing".to_string(),
        Kind::Other => "Neither a file nor a folder".to_string(),
        Kind::File => files.types.canonical(&entry.mime).to_string(),
    };
    if entry.link && entry.kind != Kind::Broken {
        let to = std::fs::read_link(path)
            .map(|to| to.display().to_string())
            .unwrap_or_default();
        return format!("Link to {to}");
    }
    what
}

/// What may be done with a file, in the nine letters every system writes them in.
fn mode_words(path: &Path) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;

    let mode = std::fs::symlink_metadata(path).ok()?.permissions().mode();
    let mut letters = String::with_capacity(9);
    for (at, letter) in "rwxrwxrwx".chars().enumerate() {
        let bit = 1 << (8 - at);
        letters.push(if mode & bit == 0 { '-' } else { letter });
    }
    Some(letters)
}

/// Work out what takes a moment, on a thread of its own: how much a folder holds, and how many
/// pixels across a picture is.
pub fn measure(id: window::Id, facts: &Facts) -> Task<Message> {
    let paths = facts.paths.clone();
    let one = paths.len() == 1;
    let known = facts.size.clone();
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let pixels = one
            .then(|| paths.first().and_then(|path| files::picture_size(path)))
            .flatten();
        let size = match known {
            Some(size) => size,
            None => files::size_words(paths.iter().map(|path| bytes(path, 0)).sum()),
        };
        let _ = sender.send(Message::Measured(id, size, pixels));
    });
    Task::perform(receiver, move |said| said.unwrap_or(Message::CloseMenu(id)))
}

/// How many bytes a thing takes: a file's own, a folder's files together, and nothing for a link,
/// which is not followed.
fn bytes(path: &Path, deep: usize) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.is_dir() && deep < DEEPEST {
        return std::fs::read_dir(path).map_or(0, |inside| {
            inside
                .filter_map(Result::ok)
                .map(|entry| bytes(&entry.path(), deep + 1))
                .sum()
        });
    }
    if meta.is_file() { meta.len() } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn what_may_be_done_with_a_file_is_written_in_the_usual_letters() {
        let folder = std::env::temp_dir().join(format!("files-props-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("a.txt");
        std::fs::write(&file, "twelve bytes").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(mode_words(&file), Some("rw-r--r--".to_string()));
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(mode_words(&file), Some("rwxr-xr-x".to_string()));
        assert_eq!(mode_words(&folder.join("nowhere")), None);
        assert_eq!(bytes(&file, 0), 12);
        assert_eq!(bytes(&folder, 0), 12);
        let facts = Facts {
            label: "a.txt".to_string(),
            kind: "text/plain".to_string(),
            place: "Documents".to_string(),
            size: Some("12 bytes".to_string()),
            items: None,
            when: Some("23 September at 11:14".to_string()),
            mode: Some("rw-r--r--".to_string()),
            pixels: Some((1920, 1080)),
            several: 0,
            paths: vec![file],
        };
        let rows = facts.rows();
        assert_eq!(rows[0], ("Name", "a.txt".to_string()));
        assert_eq!(rows[2], ("Size", "12 bytes".to_string()));
        assert!(
            rows.iter()
                .any(|(label, said)| *label == "Pixels" && said == "1920 by 1080")
        );
        let waiting = Facts {
            size: None,
            ..facts
        };
        assert_eq!(waiting.rows()[2], ("Size", "Working it out".to_string()));
        let _ = std::fs::remove_dir_all(&folder);
    }
}
