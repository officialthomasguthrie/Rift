//! The Search page: the model that turns your files into vectors, the index of home it keeps, and
//! what goes into that index.
//!
//! The index is the owner's own file in their cache folder. Quasar only turns text into vectors and
//! never reads a file, so the update is the owner's work: `rift ai index` does it from a terminal,
//! a timer does it every fifteen minutes, and the button on this page runs that same command, which
//! takes the same lock, so two updates never write at once.

use std::env;
use std::fs::{self, File};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::UNIX_EPOCH;

use iced::futures::channel::oneshot;
use iced::widget::{column, text};
use iced::{Element, Fill, Task};
use librift::quasar::Status;
use librift::search::{self, Summary};
use librift::time;

use crate::ai;
use crate::theme::Colors;
use crate::ui::{Message, Settings};
use crate::widgets::{GAP, TEXT_SIZE, action, fact, group, heading, note, setting};

/// What the page knows about the index: what its front says, and when it was last written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Look {
    /// The model that made it and how many files it holds. None when there is no index yet.
    pub summary: Option<Summary>,
    /// When it was last written, in seconds since 1970. 0 when there is none.
    pub written: i64,
}

/// The owner's home, from `HOME`, which is where the index of it is kept as well.
fn home() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

/// Where this owner's index is.
fn index_path() -> Option<PathBuf> {
    home().map(|home| search::index_path(&home, env::var_os("XDG_CACHE_HOME").as_deref()))
}

/// Read the front of the index and when it was written. The vectors are almost all of the file and
/// nothing here needs them, so only the front is read.
fn look() -> Result<Look, String> {
    let Some(path) = index_path() else {
        return Err("There is no home folder, so there is nothing to index.".to_string());
    };
    if !path.is_file() {
        return Ok(Look {
            summary: None,
            written: 0,
        });
    }
    let written = fs::metadata(&path)
        .and_then(|about| about.modified())
        .ok()
        .and_then(|when| when.duration_since(UNIX_EPOCH).ok())
        .and_then(|since| i64::try_from(since.as_secs()).ok())
        .unwrap_or(0);
    Ok(Look {
        summary: Some(front(&path)?),
        written,
    })
}

/// The front of the index, as [`Summary`] reads it.
fn front(path: &Path) -> Result<Summary, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take(search::FRONT as u64).read_to_end(&mut bytes))
        .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    Summary::peek(&bytes)
}

/// Read the index on a thread of its own, since it is on the drive and the window is not waiting.
pub fn read() -> Task<Message> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let _ = sender.send(look());
    });
    Task::perform(receiver, |answered| {
        Message::Indexed(answered.unwrap_or_else(|_| Err("The index was not read.".to_string())))
    })
}

/// The button: bring the index up to date, unless that is running already.
pub fn start(state: &mut Settings) -> Task<Message> {
    if state.indexing {
        return Task::none();
    }
    state.problem = None;
    state.indexing = true;
    update()
}

/// Bring the index up to date, by running the command the timer runs, and read it again afterwards.
/// The reading is asked for inside the closure, so its thread starts once the update has finished
/// rather than beside it.
fn update() -> Task<Message> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let _ = sender.send(ran());
    });
    Task::perform(receiver, |said| {
        said.unwrap_or_else(|_| Err("The update stopped before it finished.".to_string()))
    })
    .then(|said| Task::done(Message::Updated(said)).chain(read()))
}

/// Run `rift ai index` and say what it said when it failed.
fn ran() -> Result<(), String> {
    let output = Command::new("rift")
        .args(["ai", "index"])
        .output()
        .map_err(|e| format!("Could not run rift ai index: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if said.is_empty() {
        "rift ai index could not bring the index up to date.".to_string()
    } else {
        said
    })
}

/// The lines `rift-settings --state` prints about search: the model, how many files are in the
/// index, how long ago it was written, and whether an update is running now.
#[must_use]
pub fn state(state: &Settings) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(status) = quasar(state) {
        lines.push(format!("search {}", status.embedding_state));
        lines.push(format!(
            "search-model {}",
            if status.embedding_model.trim().is_empty() {
                "none"
            } else {
                status.embedding_model.trim()
            }
        ));
    }
    if let Some(Ok(look)) = state.index.as_ref() {
        match &look.summary {
            Some(summary) => {
                lines.push(format!("indexed {}", summary.files));
                lines.push(format!("index {}", time::ago(time::now() - look.written)));
            }
            None => lines.push("indexed none".to_string()),
        }
    }
    lines.push(format!(
        "indexing {}",
        if state.indexing { "on" } else { "off" }
    ));
    lines
}

/// What Quasar said about itself, when it has.
fn quasar(state: &Settings) -> Option<&Status> {
    state
        .quasar
        .as_deref()
        .and_then(|picture| picture.status.as_ref().ok())
}

/// The page.
pub fn view(state: &Settings, look: Colors) -> Element<'_, Message> {
    let mut page = column![].spacing(GAP).width(Fill);
    match state.quasar.as_deref().map(|picture| &picture.status) {
        None => page = page.push(note(look, "Asking Quasar which model it reads files with.")),
        Some(Err(why)) => {
            page = page.push(note(
                look,
                "Quasar is not answering, so the model that reads your files is not here.",
            ));
            page = page.push(note(look, why));
        }
        Some(Ok(status)) => page = page.push(model(look, status)),
    }
    page = page.push(index(state, look));
    if let Some(why) = &state.problem {
        page = page.push(text(why).size(TEXT_SIZE).color(look.error));
    }
    page.push(rules(look))
        .push(note(
            look,
            "Which folders are indexed, and which to leave out, are not in Settings yet. rift ai \
             search finds files from a terminal.",
        ))
        .into()
}

/// The model that turns text into vectors, which is Quasar's second model.
fn model(look: Colors, status: &Status) -> Element<'_, Message> {
    let mut rows = vec![
        setting(
            look,
            "State",
            None,
            ai::said(look, ai::word(&status.embedding_state)),
        ),
        setting(
            look,
            "Model",
            None,
            ai::said(
                look,
                if status.embedding_model.trim().is_empty() {
                    "none"
                } else {
                    status.embedding_model.trim()
                },
            ),
        ),
    ];
    if !status.embedding_error.is_empty() {
        rows.push(fact(look, "Why", status.embedding_error.trim().to_string()));
    }
    column![
        heading(look, "The model that reads your files"),
        group(look, rows)
    ]
    .spacing(8)
    .into()
}

/// The index: how many files are in it, when it was last written, and the button that brings it up
/// to date.
fn index(state: &Settings, look: Colors) -> Element<'_, Message> {
    let press = (!state.indexing).then_some(Message::Index);
    let mut rows = Vec::new();
    match state.index.as_ref() {
        None => rows.push(fact(look, "Files", "Reading the index.".to_string())),
        Some(Err(why)) => rows.push(fact(look, "Files", why.clone())),
        Some(Ok(look_at)) => match &look_at.summary {
            None => rows.push(fact(look, "Files", "Nothing is indexed yet.".to_string())),
            Some(summary) => {
                rows.push(setting(
                    look,
                    "Files",
                    None,
                    ai::said(look, &summary.files.to_string()),
                ));
                rows.push(setting(
                    look,
                    "Last updated",
                    None,
                    ai::said(look, &time::ago(time::now() - look_at.written)),
                ));
            }
        },
    }
    rows.push(setting(
        look,
        "Bring the index up to date",
        Some(if state.indexing {
            "Reading the files that are new or changed."
        } else {
            "Files that are new or changed are read again. It also runs every fifteen minutes."
        }),
        action(look, "Update", press),
    ));
    column![heading(look, "Index"), group(look, rows)]
        .spacing(8)
        .into()
}

/// What is indexed and what is left out, from the rules the index itself follows.
fn rules<'a>(look: Colors) -> Element<'a, Message> {
    let rows = vec![
        fact(look, "Folder", "Your home folder".to_string()),
        fact(
            look,
            "Files",
            format!(
                "Plain text, markdown and code, up to {} MB each",
                search::LARGEST_FILE >> 20
            ),
        ),
        fact(
            look,
            "Left out",
            format!(
                "Hidden files and folders, and {}",
                and_then(search::SKIPPED)
            ),
        ),
    ];
    column![heading(look, "What is indexed"), group(look, rows)]
        .spacing(8)
        .into()
}

/// `a`, `a and b`, `a, b and c`.
fn and_then(words: &[&str]) -> String {
    match words {
        [] => String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use librift::search::{File as Indexed, Index, Stored};

    fn ready() -> Status {
        Status {
            state: "ready".into(),
            model: "qwen3-0.6b-q8_0".into(),
            tier: "small".into(),
            embedding_state: "ready".into(),
            embedding_model: "nomic-embed-text-v1.5-q8".into(),
            ..Status::default()
        }
    }

    fn settings(status: Option<Status>, index: Option<Result<Look, String>>) -> Settings {
        let mut state = Settings::bare();
        state.quasar = status.map(|status| {
            Box::new(crate::ai::Picture {
                status: Ok(status),
                models: Vec::new(),
                sizes: Vec::new(),
            })
        });
        state.index = index;
        state
    }

    fn look_of(files: usize, written: i64) -> Look {
        Look {
            summary: Some(Summary {
                model: "nomic-embed-text-v1.5-q8".to_string(),
                dimensions: 768,
                files,
            }),
            written,
        }
    }

    #[test]
    fn the_state_says_the_model_the_files_and_how_long_ago() {
        let kept = settings(
            Some(ready()),
            Some(Ok(look_of(5, time::now() - 12 * 60 - 5))),
        );
        assert_eq!(
            state(&kept),
            [
                "search ready",
                "search-model nomic-embed-text-v1.5-q8",
                "indexed 5",
                "index 12 minutes ago",
                "indexing off",
            ]
        );
    }

    #[test]
    fn a_home_with_no_index_says_none() {
        let kept = settings(
            Some(ready()),
            Some(Ok(Look {
                summary: None,
                written: 0,
            })),
        );
        assert_eq!(state(&kept)[2..], ["indexed none", "indexing off"]);
        // nothing has been read yet, and a failure is on the page, not in the state
        let asking = settings(Some(ready()), None);
        assert_eq!(state(&asking)[2..], ["indexing off"]);
        let broken = settings(
            Some(ready()),
            Some(Err("The search index is damaged.".to_string())),
        );
        assert_eq!(state(&broken)[2..], ["indexing off"]);
    }

    #[test]
    fn an_update_that_is_running_says_so() {
        let mut kept = settings(Some(ready()), Some(Ok(look_of(5, time::now()))));
        kept.indexing = true;
        assert_eq!(state(&kept).last().map(String::as_str), Some("indexing on"));
        assert_eq!(state(&kept)[3], "index just now");
    }

    #[test]
    fn a_model_that_is_not_there_says_none() {
        let none = Status {
            embedding_state: "none".into(),
            embedding_model: String::new(),
            embedding_error: "Search by meaning needs a model that is not on the drive.".into(),
            ..ready()
        };
        let kept = settings(Some(none), None);
        assert_eq!(state(&kept)[..2], ["search none", "search-model none"]);
        // and with no answer from Quasar at all the page says nothing about the model
        let quiet = settings(None, None);
        assert_eq!(state(&quiet), ["indexing off"]);
    }

    #[test]
    fn the_front_of_a_real_index_is_what_the_page_reads() {
        let index = Index {
            model: "nomic-embed-text-v1.5-q8".to_string(),
            files: vec![Indexed {
                path: "notes/bike.txt".to_string(),
                modified: 1_789_221_603_000_000_000,
                size: 64,
                parts: vec![Stored {
                    line: 1,
                    vector: vec![1.0, 0.0],
                }],
            }],
        };
        let path = std::env::temp_dir().join(format!("rift-search-{}.index", std::process::id()));
        fs::write(&path, index.encode()).expect("the index");
        let read = front(&path).expect("the front of the index");
        assert_eq!(read.files, 1);
        assert_eq!(read.model, index.model);
        assert_eq!(read.dimensions, 2);
        fs::write(&path, b"not an index").expect("the index");
        assert!(front(&path).is_err());
        fs::remove_file(&path).expect("the index");
    }

    #[test]
    fn the_folders_left_out_read_as_a_list() {
        assert_eq!(
            and_then(search::SKIPPED),
            "node_modules, target and __pycache__"
        );
        assert_eq!(and_then(&["one"]), "one");
        assert_eq!(and_then(&["one", "two"]), "one and two");
        assert_eq!(and_then(&[]), "");
        // the largest file the index takes, in whole megabytes
        assert_eq!(search::LARGEST_FILE >> 20, 1);
    }
}
