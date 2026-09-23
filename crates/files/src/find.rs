//! Search: the field in the header bar. By name it walks the folder and what is under it as a
//! person types, so the list follows every letter; by meaning, on Enter, it asks the index Quasar's
//! model made of home, which only the owner can read. Each row is named by its path under the
//! folder searched, since two folders can hold the same name.

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use librift::files::{self, Entry, mime};
use librift::search::{self, Missing};

/// How many files a search by name lists.
const BY_NAME: usize = 1000;
/// How many files the index is asked for, before the ones outside the folder are left out.
const RANKED: usize = 200;
/// How many of those the list shows.
const BY_MEANING: usize = 50;
/// How many entries a walk looks at before it gives up, so no search runs away with a folder.
const LOOKED_AT: usize = 100_000;

/// The files under `root` whose name has `words` in it, whatever the case, in the order the walk
/// found them: each folder's own things first, then what is under them.
#[must_use]
pub fn by_name(root: &Path, words: &str, hidden: bool, types: &mime::Database) -> Vec<Entry> {
    let wanted = words.trim().to_lowercase();
    let mut found = Vec::new();
    if wanted.is_empty() {
        return found;
    }
    let mut folders = vec![PathBuf::new()];
    let mut seen = 0;
    while let Some(under) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(root.join(&under)) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            seen += 1;
            if seen > LOOKED_AT || found.len() >= BY_NAME {
                return found;
            }
            let name = entry.file_name();
            let label = name.to_string_lossy();
            if !hidden && files::is_hidden(&label, "") {
                continue;
            }
            let here = under.join(&name);
            if label.to_lowercase().contains(&wanted) {
                found.push(row(&root.join(&here), &here, types));
            }
            // a link is never walked into, so no loop of links can be walked for ever
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                folders.push(here);
            }
        }
    }
    found
}

/// The files under `root` closest in meaning to `words`, closest first.
///
/// # Errors
///
/// The sentence to show in place of them: there is no index yet, the model is still loading, or
/// the folder is not one the index covers.
pub fn by_meaning(root: &Path, words: &str, types: &mime::Database) -> Result<Vec<Entry>, String> {
    let home = env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .ok_or("There is no home folder, so there is nothing indexed to search.")?;
    let under = root
        .strip_prefix(&home)
        .map_err(|_| OUTSIDE.to_string())?
        .to_path_buf();
    let hits = search::find(
        &home,
        env::var_os("XDG_CACHE_HOME").as_deref(),
        words,
        RANKED,
    )
    .map_err(said)?;
    let found: Vec<Entry> = hits
        .iter()
        .filter_map(|hit| {
            let here = Path::new(&hit.path).strip_prefix(&under).ok()?;
            let full = home.join(&hit.path);
            full.is_file().then(|| row(&full, here, types))
        })
        .take(BY_MEANING)
        .collect();
    Ok(found)
}

/// One row: the file as a list shows it, named by its path under the folder searched.
fn row(full: &Path, here: &Path, types: &mime::Database) -> Entry {
    let name = full.file_name().unwrap_or(full.as_os_str()).to_owned();
    Entry {
        name: OsString::from(here),
        ..files::entry_of(full, name, types)
    }
}

/// The folder each row is in, as its column says it: the path under the folder searched, and the
/// folder's own name for what is in the folder itself.
#[must_use]
pub fn under(root: &Path, entry: &Entry) -> String {
    match Path::new(&entry.name).parent() {
        Some(folder) if folder.as_os_str().is_empty() => files::shown(root),
        Some(folder) => folder.display().to_string(),
        None => files::shown(root),
    }
}

/// What the list says when a search by meaning found nothing, or when there was nothing to
/// search with.
const OUTSIDE: &str = "Only your home folder is indexed, so there is nothing here to search by \
                       meaning.";

/// What a window says when a search by meaning cannot run: the page that makes the index, since
/// that is where a person goes from here.
fn said(missing: Missing) -> String {
    match missing {
        Missing::NotIndexed | Missing::Empty => {
            "Nothing is indexed yet. The Search page in Settings makes the index of your home \
             folder."
                .to_string()
        }
        Missing::Loading => {
            "The model that reads meaning is still loading. Try again in a moment.".to_string()
        }
        Missing::Model(index, running) => format!(
            "The index was made with {index}, and Quasar runs {running}. The Search page in \
             Settings makes it again."
        ),
        Missing::Failed(why) => why,
    }
}

/// Whether a search found nothing to show, and what to say then.
#[must_use]
pub fn nothing(meaning: bool) -> &'static str {
    if meaning {
        "Nothing in this folder is close to those words."
    } else {
        "Nothing here has that in its name."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> PathBuf {
        let folder = std::env::temp_dir().join(format!("files-find-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("notes/old")).unwrap();
        std::fs::create_dir_all(folder.join(".cache")).unwrap();
        for (path, text) in [
            ("Report 2026.txt", "one"),
            ("notes/report on the year.md", "two"),
            ("notes/old/report.txt", "three"),
            ("notes/soup.txt", "four"),
            (".cache/report.bin", "five"),
        ] {
            std::fs::write(folder.join(path), text).unwrap();
        }
        folder
    }

    fn names(found: &[Entry]) -> Vec<String> {
        let mut names: Vec<String> = found
            .iter()
            .map(|entry| Path::new(&entry.name).display().to_string())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_name_is_looked_for_under_the_folder_whatever_the_case() {
        let root = folder();
        let types = mime::Database::load();
        let found = by_name(&root, "report", false, &types);
        assert_eq!(
            names(&found),
            [
                "Report 2026.txt",
                "notes/old/report.txt",
                "notes/report on the year.md"
            ]
        );
        // the label is the name, the row's own name is where it lies under the folder
        let first = found
            .iter()
            .find(|entry| entry.label == "report.txt")
            .expect("report.txt was not found");
        assert_eq!(under(&root, first), "notes/old");
        let top = found
            .iter()
            .find(|entry| entry.label == "Report 2026.txt")
            .expect("Report 2026.txt was not found");
        assert_eq!(under(&root, top), files::shown(&root));
        assert!(by_name(&root, "  ", false, &types).is_empty());
        assert!(by_name(&root, "nothing of the sort", false, &types).is_empty());
        let with_hidden = by_name(&root, "report", true, &types);
        assert_eq!(with_hidden.len(), 4, "the hidden folder is searched too");
        let _ = std::fs::remove_dir_all(&root);
    }
}
