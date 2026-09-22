//! The trash, the way the freedesktop trash specification keeps it, which is also where every GTK
//! app's Move to trash and the portal put things: `~/.local/share/Trash`, with the file itself in
//! `files` and a note in `info` of where it was and when it went, under the same name. A file
//! goes into the trash by a rename, so only a file on the same file system as the trash can; a
//! file anywhere else is deleted instead, when the owner says so.

use std::ffi::{OsStr, OsString};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use super::{Kind, escaped, free_name, local_stamp, remove, stamp_seconds, unescaped};

/// The ending of a note in `info`.
const NOTE: &str = ".trashinfo";

/// A trash: the folder that holds `files` and `info`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trash {
    root: PathBuf,
}

/// Something in the trash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trashed {
    /// Its name in the trash, which is its note's name too.
    pub name: OsString,
    /// Where it was.
    pub path: PathBuf,
    /// When it went into the trash, in seconds since 1970, when the note says.
    pub deleted: Option<i64>,
    /// What it is.
    pub kind: Kind,
    /// Its size in bytes, when it is a file.
    pub size: u64,
}

impl Trashed {
    /// The name it had, which is the name it gets back.
    #[must_use]
    pub fn label(&self) -> String {
        self.path.file_name().map_or_else(
            || self.name.to_string_lossy().into_owned(),
            |name| name.to_string_lossy().into_owned(),
        )
    }
}

impl Trash {
    /// The owner's trash, in their data folder.
    #[must_use]
    pub fn home() -> Option<Self> {
        let data = std::env::var_os("XDG_DATA_HOME")
            .filter(|data| !data.is_empty())
            .map(PathBuf::from)
            .or_else(|| super::home().map(|home| home.join(".local/share")))?;
        Some(Self::at(data.join("Trash")))
    }

    /// The trash in this folder.
    #[must_use]
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    /// The folder the trash is.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn files(&self) -> PathBuf {
        self.root.join("files")
    }

    fn info(&self) -> PathBuf {
        self.root.join("info")
    }

    fn note(&self, name: &OsStr) -> PathBuf {
        let mut file = name.to_owned();
        file.push(NOTE);
        self.info().join(file)
    }

    /// Whether a file can go into this trash: it is on the same file system as the trash, and not
    /// in the trash already.
    #[must_use]
    pub fn takes(&self, path: &Path) -> bool {
        if path.starts_with(&self.root) {
            return false;
        }
        let Ok(own) = fs::symlink_metadata(path) else {
            return false;
        };
        // the trash may not be there yet, and then the folder it will be made in says
        let device = self
            .root
            .ancestors()
            .find_map(|folder| fs::metadata(folder).ok())
            .map(|meta| meta.dev());
        device == Some(own.dev())
    }

    /// Move a file or a folder into the trash, with a note of where it was and when, `now` being
    /// seconds since 1970 and `offset` the local zone's distance from UTC.
    ///
    /// # Errors
    ///
    /// A sentence when it is not there, is not on the trash's file system, or could not be moved.
    pub fn put(&self, path: &Path, now: i64, offset: i32) -> Result<Trashed, String> {
        let shown = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let own =
            fs::symlink_metadata(path).map_err(|_| format!("{shown} is not there any more."))?;
        if !self.takes(path) {
            return Err(format!("{shown} cannot be moved to the trash."));
        }
        for folder in [self.files(), self.info()] {
            fs::create_dir_all(&folder)
                .map_err(|e| format!("Could not make the trash at {}: {e}.", folder.display()))?;
            let _ = fs::set_permissions(&folder, fs::Permissions::from_mode(0o700));
        }
        // where it was, with the folder it was in written out, and itself as it is
        let whole = match (path.parent(), path.file_name()) {
            (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
                fs::canonicalize(parent)
                    .map_or_else(|_| path.to_path_buf(), |parent| parent.join(name))
            }
            _ => fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
        };
        let text = format!(
            "[Trash Info]\nPath={}\nDeletionDate={}\n",
            escaped(&whole),
            local_stamp(now, offset)
        );
        let base = path.file_name().unwrap_or(path.as_os_str()).to_owned();
        let mut number = 1;
        let name = loop {
            let mut name = base.clone();
            if number > 1 {
                name.push(format!(".{number}"));
            }
            number += 1;
            if fs::symlink_metadata(self.files().join(&name)).is_ok() {
                continue;
            }
            // the note first, made only when there is none, so two moves at once never share a
            // name, as the specification asks
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.note(&name))
            {
                Ok(mut note) => {
                    note.write_all(text.as_bytes()).map_err(|e| {
                        format!("Could not write the trash's note for {shown}: {e}.")
                    })?;
                    break name;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => {
                    return Err(format!(
                        "Could not write the trash's note for {shown}: {e}."
                    ));
                }
            }
        };
        if let Err(e) = fs::rename(path, self.files().join(&name)) {
            let _ = fs::remove_file(self.note(&name));
            return Err(format!("{shown} could not be moved to the trash: {e}."));
        }
        let kind = kind_of(&own, &self.files().join(&name));
        Ok(Trashed {
            name,
            path: whole,
            deleted: Some(now),
            kind,
            size: if own.is_file() { own.len() } else { 0 },
        })
    }

    /// Everything in the trash, the newest first. A note whose file is gone is left out, and so is
    /// a file with no note.
    #[must_use]
    pub fn list(&self, offset: i32) -> Vec<Trashed> {
        let Ok(notes) = fs::read_dir(self.info()) else {
            return Vec::new();
        };
        let mut found: Vec<Trashed> = notes
            .filter_map(Result::ok)
            .filter_map(|note| {
                let file = note.file_name();
                let name = file.as_bytes().strip_suffix(NOTE.as_bytes())?;
                self.read(OsStr::from_bytes(name), offset)
            })
            .collect();
        found.sort_by(|one, other| {
            other
                .deleted
                .cmp(&one.deleted)
                .then_with(|| super::natural(&one.label(), &other.label()))
        });
        found
    }

    /// One thing in the trash by its name there.
    #[must_use]
    pub fn read(&self, name: &OsStr, offset: i32) -> Option<Trashed> {
        let text = fs::read_to_string(self.note(name)).ok()?;
        let (path, deleted) = read_note(&text)?;
        let file = self.files().join(name);
        let own = fs::symlink_metadata(&file).ok()?;
        Some(Trashed {
            name: name.to_owned(),
            path,
            deleted: deleted.and_then(|written| stamp_seconds(&written, offset)),
            kind: kind_of(&own, &file),
            size: if own.is_file() { own.len() } else { 0 },
        })
    }

    /// Put something in the trash back where it was. When that name has been taken since, it comes
    /// back beside it with a number, and a folder that is gone is made again.
    ///
    /// # Errors
    ///
    /// A sentence when it is not in the trash or could not be moved back.
    pub fn restore(&self, name: &OsStr) -> Result<PathBuf, String> {
        let text = fs::read_to_string(self.note(name))
            .map_err(|_| format!("{} is not in the trash.", name.to_string_lossy()))?;
        let (path, _) = read_note(&text).ok_or_else(|| {
            format!(
                "The trash's note for {} cannot be read.",
                name.to_string_lossy()
            )
        })?;
        let shown = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let folder = path.parent().unwrap_or(Path::new("/"));
        fs::create_dir_all(folder)
            .map_err(|e| format!("Could not make {} again: {e}.", folder.display()))?;
        let target = folder.join(free_name(folder, path.file_name().unwrap_or(name), false));
        fs::rename(self.files().join(name), &target)
            .map_err(|e| format!("{shown} could not be put back: {e}."))?;
        let _ = fs::remove_file(self.note(name));
        Ok(target)
    }

    /// Delete something in the trash for good.
    ///
    /// # Errors
    ///
    /// A sentence when it could not be deleted.
    pub fn delete(&self, name: &OsStr) -> Result<(), String> {
        let file = self.files().join(name);
        if fs::symlink_metadata(&file).is_ok() {
            remove(&file)?;
        }
        match fs::remove_file(self.note(name)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!(
                "Could not delete the trash's note for {}: {e}.",
                name.to_string_lossy()
            )),
            _ => Ok(()),
        }
    }

    /// Delete everything in the trash for good, notes and all.
    ///
    /// # Errors
    ///
    /// A sentence for the first thing that could not be deleted. Everything else is deleted all
    /// the same.
    pub fn empty(&self) -> Result<(), String> {
        let mut first = None;
        for folder in [self.files(), self.info()] {
            let Ok(entries) = fs::read_dir(&folder) else {
                continue;
            };
            for entry in entries.filter_map(Result::ok) {
                if let Err(why) = remove(&entry.path()) {
                    first.get_or_insert(why);
                }
            }
        }
        let _ = fs::remove_file(self.root.join("directorysizes"));
        first.map_or(Ok(()), Err)
    }

    /// How many things are in the trash.
    #[must_use]
    pub fn count(&self) -> usize {
        fs::read_dir(self.files()).map_or(0, Iterator::count)
    }
}

/// What a file in the trash is: a link counts as what it points at, as in a folder.
fn kind_of(own: &fs::Metadata, path: &Path) -> Kind {
    if own.file_type().is_symlink() {
        return match fs::metadata(path) {
            Ok(meta) if meta.is_dir() => Kind::Folder,
            Ok(meta) if meta.is_file() => Kind::File,
            Ok(_) => Kind::Other,
            Err(_) => Kind::Broken,
        };
    }
    if own.is_dir() {
        Kind::Folder
    } else if own.is_file() {
        Kind::File
    } else {
        Kind::Other
    }
}

/// Where a note says its file was, and when it went, as written.
#[must_use]
pub fn read_note(text: &str) -> Option<(PathBuf, Option<String>)> {
    let mut inside = false;
    let mut path = None;
    let mut deleted = None;
    for line in text.lines() {
        let line = line.trim_end();
        if line.starts_with('[') {
            inside = line == "[Trash Info]";
            continue;
        }
        if !inside {
            continue;
        }
        if let Some(value) = line.strip_prefix("Path=") {
            path = Some(unescaped(value));
        } else if let Some(value) = line.strip_prefix("DeletionDate=") {
            deleted = Some(value.to_string());
        }
    }
    path.filter(|path| path.is_absolute())
        .map(|path| (path, deleted))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("librift-trash-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("the test folder");
        fs::canonicalize(&root).expect("the test folder")
    }

    // 2026-09-22 10:42:00 UTC
    const NOW: i64 = 1_790_073_720;

    #[test]
    fn a_file_goes_into_the_trash_and_comes_back() {
        let root = temporary("round");
        let trash = Trash::at(root.join("Trash"));
        let docs = root.join("My documents");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("report.txt"), "twelve bytes").unwrap();
        let put = trash.put(&docs.join("report.txt"), NOW, 3600).unwrap();
        assert!(!docs.join("report.txt").exists());
        assert_eq!(put.path, docs.join("report.txt"));
        let note = fs::read_to_string(root.join("Trash/info/report.txt.trashinfo")).unwrap();
        assert_eq!(
            note,
            format!(
                "[Trash Info]\nPath={}/My%20documents/report.txt\nDeletionDate=2026-09-22T11:42:00\n",
                escaped(&root)
            )
        );
        let listed = trash.list(3600);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].label(), "report.txt");
        assert_eq!(listed[0].deleted, Some(NOW));
        assert_eq!((listed[0].kind, listed[0].size), (Kind::File, 12));
        assert_eq!(trash.count(), 1);
        let back = trash.restore(&listed[0].name).unwrap();
        assert_eq!(back, docs.join("report.txt"));
        assert_eq!(fs::read_to_string(&back).unwrap(), "twelve bytes");
        assert!(trash.list(0).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn two_files_of_one_name_keep_their_own_notes() {
        let root = temporary("names");
        let trash = Trash::at(root.join("Trash"));
        for folder in ["a", "b"] {
            fs::create_dir_all(root.join(folder)).unwrap();
            fs::write(root.join(folder).join("notes.md"), folder).unwrap();
            trash
                .put(&root.join(folder).join("notes.md"), NOW, 0)
                .unwrap();
        }
        let mut names: Vec<String> = trash
            .list(0)
            .iter()
            .map(|item| item.name.to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["notes.md", "notes.md.2"]);
        // a name taken since comes back beside it
        fs::write(root.join("a/notes.md"), "new").unwrap();
        let back = trash.restore(OsStr::new("notes.md")).unwrap();
        assert_eq!(back, root.join("a/notes (2).md"));
        assert_eq!(fs::read_to_string(root.join("a/notes.md")).unwrap(), "new");
        // and a folder that is gone is made again
        fs::remove_dir_all(root.join("b")).unwrap();
        assert_eq!(
            trash.restore(OsStr::new("notes.md.2")).unwrap(),
            root.join("b/notes.md")
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_folder_is_deleted_and_the_trash_emptied() {
        let root = temporary("empty");
        let trash = Trash::at(root.join("Trash"));
        fs::create_dir_all(root.join("Old/inside")).unwrap();
        fs::write(root.join("Old/inside/a"), "a").unwrap();
        fs::write(root.join("b.txt"), "b").unwrap();
        let old = trash.put(&root.join("Old"), NOW, 0).unwrap();
        assert_eq!(old.kind, Kind::Folder);
        trash.put(&root.join("b.txt"), NOW - 60, 0).unwrap();
        // the newest first
        let labels: Vec<String> = trash.list(0).iter().map(Trashed::label).collect();
        assert_eq!(labels, ["Old", "b.txt"]);
        trash.delete(&old.name).unwrap();
        assert_eq!(trash.count(), 1);
        trash.empty().unwrap();
        assert_eq!(trash.count(), 0);
        assert!(trash.list(0).is_empty());
        assert_eq!(fs::read_dir(root.join("Trash/info")).unwrap().count(), 0);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_trash_takes_nothing_of_its_own_and_nothing_gone() {
        let root = temporary("refuse");
        let trash = Trash::at(root.join("Trash"));
        fs::write(root.join("a"), "a").unwrap();
        assert!(trash.takes(&root.join("a")));
        let put = trash.put(&root.join("a"), NOW, 0).unwrap();
        assert!(!trash.takes(&root.join("Trash/files").join(&put.name)));
        assert_eq!(
            trash.put(&root.join("a"), NOW, 0).unwrap_err(),
            "a is not there any more."
        );
        assert_eq!(
            read_note("[Trash Info]\nPath=relative/a\n"),
            None,
            "the home trash keeps whole paths"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
