//! The files of home the field found by meaning. The index is the owner's own file, which
//! `librift::search` reads here; Quasar turns the typed words into a vector and never sees a file.
//! What comes back is one row per file: its name, the folder it is in, and the kind it is, which
//! says both how the row is drawn and what a press opens it with.

// the rows are only drawn by the panel, and the panel is linux only
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::env;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use librift::apps::App;
use librift::defaults::Found;
use librift::files::{self, mime};
use librift::search::{self, Hit, Missing};

/// How long after the last key the field waits before it looks through home. Every search is a
/// call to the model, so the shell waits until the typing has stopped.
pub const PAUSE: Duration = Duration::from_millis(500);
/// How many files the index is asked for, so files that are gone since it was made still leave
/// enough of them to fill the section.
const RANKED: usize = 20;
/// How many of them the menu lists. The menu also holds the apps and the places, so the closest
/// few are what there is room for under the field.
pub const MOST: usize = 5;

/// One file a search found, as a row draws it and a press opens it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// Its own name, which the row says first.
    pub name: String,
    /// The folder it is in, as the row says it at the right: the path of the folder under home, or
    /// the name of home itself for a file that lies in home.
    pub folder: String,
    /// Its path under home, `notes/letter.pdf`, which `lens --state` prints.
    pub under: String,
    /// Where the file is, for the app that opens it.
    pub path: PathBuf,
    /// Its kind, which decides the app it opens with.
    pub mime: String,
    /// The icons that can draw its kind, the one that fits best first.
    pub icons: Vec<String>,
}

/// The kinds of file, read once and kept. The read happens on the thread a search runs on, never
/// on the one that draws.
pub fn types() -> &'static mime::Database {
    static TYPES: OnceLock<mime::Database> = OnceLock::new();
    TYPES.get_or_init(mime::Database::load)
}

/// The files of home closest in meaning to `words`, closest first, at most [`MOST`] of them.
///
/// # Errors
///
/// The sentence to put under the field: there is no index of home yet, the model that reads
/// meaning is still loading, the index was made with another model, or Quasar did not answer.
pub fn look(words: &str) -> Result<Vec<File>, String> {
    let home = env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .ok_or("There is no home folder, so there is nothing indexed to search.")?;
    let hits = search::find(
        &home,
        env::var_os("XDG_CACHE_HOME").as_deref(),
        words,
        RANKED,
    )
    .map_err(said)?;
    Ok(rows(&home, &hits, types()))
}

/// A row for each file a search found that is still there, in the order it came, at most [`MOST`].
#[must_use]
pub fn rows(home: &Path, hits: &[Hit], types: &mime::Database) -> Vec<File> {
    hits.iter()
        .filter_map(|hit| {
            let under = Path::new(&hit.path);
            let path = home.join(under);
            let about = std::fs::metadata(&path).ok()?;
            if !about.is_file() {
                return None;
            }
            let mime = types.guess(&path, Some(&about), false);
            Some(File {
                name: under.file_name()?.to_string_lossy().into_owned(),
                folder: match under.parent() {
                    Some(folder) if !folder.as_os_str().is_empty() => folder.display().to_string(),
                    _ => files::shown(home),
                },
                under: hit.path.clone(),
                path,
                icons: types.icons(&mime),
                mime,
            })
        })
        .take(MOST)
        .collect()
}

/// The app a press on a row opens the file with: the default for its kind, found the way xdg-mime
/// finds one, so the shell and Files open the same file with the same app.
#[must_use]
pub fn opens(file: &File, apps: &[App]) -> Option<App> {
    Found::read().opener(&file.mime, apps, types(), None)
}

/// What the menu says when a search could not run. A person in the shell has no terminal in front
/// of them, so it names the page that makes the index, the way Files does.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(path: &str) -> Hit {
        Hit {
            path: path.to_string(),
            line: 1,
            modified: 0,
            score: 0.5,
        }
    }

    #[test]
    fn a_row_says_the_name_and_the_folder_the_file_is_in() {
        let home = env::temp_dir().join(format!("lens-find-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("notes")).unwrap();
        std::fs::write(home.join("notes/bike.txt"), "tyres").unwrap();
        std::fs::write(home.join("plan.md"), "a plan").unwrap();
        let types = mime::Database::parse(
            "50:text/markdown:*.md\n50:text/plain:*.txt\n",
            "text/markdown text/plain\n",
            "",
            "text:text-x-generic\n",
        );
        let found = rows(
            &home,
            &[hit("notes/bike.txt"), hit("plan.md"), hit("gone.txt")],
            &types,
        );
        let named: Vec<(&str, &str)> = found
            .iter()
            .map(|file| (file.name.as_str(), file.folder.as_str()))
            .collect();
        // a file that is gone since the index was made is not a row, and a file in home names home
        assert_eq!(
            named,
            [("bike.txt", "notes"), ("plan.md", &*files::shown(&home))]
        );
        assert_eq!(found[0].under, "notes/bike.txt");
        assert_eq!(found[0].path, home.join("notes/bike.txt"));
        assert_eq!(found[0].mime, "text/plain");
        assert!(
            found[0]
                .icons
                .first()
                .is_some_and(|name| name == "text-plain"),
            "the kind's own icon comes first: {:?}",
            found[0].icons
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_search_that_cannot_run_names_the_page_that_makes_the_index() {
        assert!(said(Missing::NotIndexed).contains("The Search page in Settings"));
        assert!(said(Missing::Empty).contains("The Search page in Settings"));
        assert!(said(Missing::Model("one".into(), "other".into())).contains("one"));
        assert_eq!(
            said(Missing::Failed("Quasar is not running.".into())),
            "Quasar is not running."
        );
    }
}
