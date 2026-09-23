//! The small pictures of files the grid draws. A picture is made once by one of the programs the
//! system has for that kind of file, kept in the owner's own cache, and read from there after
//! that; it is made on a thread of its own, never while the window is being drawn, and a few at a
//! time, so a folder of a thousand photographs does not start a thousand programs.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use iced::futures::channel::oneshot;
use iced::{Task, window};
use librift::files::Entry;
use librift::files::thumbnails::Makers;

use crate::ui::{Files, Message};

/// How many pictures are made at the same time.
const AT_ONCE: usize = 2;

/// A file bigger than this gets no picture: reading one would take more memory than a picture in a
/// list is worth.
const BIGGEST: u64 = 256 * 1024 * 1024;

/// What each file a window has asked about is waiting for.
#[derive(Debug, Default)]
pub struct Thumbs {
    /// The programs the system has, read once when the app starts.
    makers: Arc<Makers>,
    /// The picture of each file that has one, by the file and the time it last changed. Nothing
    /// where the file can have no picture, so it is not asked about again.
    made: HashMap<PathBuf, (Option<i64>, Option<PathBuf>)>,
    /// The files being made now.
    making: HashSet<PathBuf>,
    /// The files waiting for a program to be free, the ones asked for first at the front.
    waiting: VecDeque<Ask>,
}

/// One file to make a picture of.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Ask {
    /// The file.
    path: PathBuf,
    /// Its kind, which says which program makes the picture.
    kind: String,
    /// When it last changed, which the picture in the cache carries.
    modified: Option<i64>,
}

impl Thumbs {
    /// The programs the system has for the kinds of file it can draw.
    #[must_use]
    pub fn load() -> Self {
        Self {
            makers: Arc::new(Makers::load()),
            ..Self::default()
        }
    }

    /// The picture of a file, when one has been made and the file has not changed since.
    #[must_use]
    pub fn of(&self, path: &Path, modified: Option<i64>) -> Option<&Path> {
        match self.made.get(path) {
            Some((when, Some(picture))) if *when == modified => Some(picture),
            _ => None,
        }
    }

    /// Whether this file could have a picture at all: a kind one of the programs makes, a file
    /// that is not too big, and nothing already asked about.
    fn worth_asking(&self, entry: &Entry, path: &Path) -> bool {
        entry.kind == librift::files::Kind::File
            && entry.size <= BIGGEST
            && self.makers.covers(&entry.mime)
            && !self.making.contains(path)
            && self
                .made
                .get(path)
                .is_none_or(|(when, picture)| picture.is_some() && *when != entry.modified)
    }

    /// What a window has asked for that is not being made yet.
    fn queue(&mut self, wanted: Vec<Ask>) {
        for ask in wanted {
            if !self.waiting.iter().any(|already| already.path == ask.path) {
                self.waiting.push_back(ask);
            }
        }
        // a window that has gone somewhere else leaves a long queue behind it
        while self.waiting.len() > 400 {
            self.waiting.pop_front();
        }
    }

    /// Start as many as may run at once, and answer the tasks that carry what they make.
    fn start(&mut self) -> Task<Message> {
        let mut started = Vec::new();
        while self.making.len() < AT_ONCE
            && let Some(ask) = self.waiting.pop_front()
        {
            self.making.insert(ask.path.clone());
            started.push(make(Arc::clone(&self.makers), ask));
        }
        Task::batch(started)
    }

    /// What a picture that was made or could not be made says about its file.
    pub fn arrived(&mut self, path: PathBuf, modified: Option<i64>, picture: Option<PathBuf>) {
        self.making.remove(&path);
        self.made.insert(path, (modified, picture));
    }

    /// How many pictures the app has, for `--state`.
    #[must_use]
    pub fn count(&self) -> usize {
        self.made
            .values()
            .filter(|(_, picture)| picture.is_some())
            .count()
    }
}

/// Make one picture on a thread of its own: the one in the cache when it is still of the file as it
/// is now, or a new one from the program for its kind.
fn make(makers: Arc<Makers>, ask: Ask) -> Task<Message> {
    let (sender, receiver) = oneshot::channel();
    let path = ask.path.clone();
    let modified = ask.modified;
    thread::spawn(move || {
        let picture = librift::files::thumbnails::made(&ask.path, ask.modified, false)
            .or_else(|| makers.make(&ask.path, &ask.kind, false).ok());
        let _ = sender.send(Message::Thumb(ask.path, ask.modified, picture));
    });
    Task::perform(receiver, move |said| {
        said.unwrap_or_else(|_| Message::Thumb(path.clone(), modified, None))
    })
}

/// Ask for the pictures of the files on screen in a window, when it is showing a grid. Rows that
/// are not on screen are left until they are.
pub fn want(state: &mut Files, id: window::Id) -> Task<Message> {
    if !state.options.grid {
        return Task::none();
    }
    let Some(browser) = state.windows.get(&id) else {
        return Task::none();
    };
    let Some(folder) = browser.location.place() else {
        return Task::none();
    };
    let (first, shown) = crate::grid::on_screen(browser);
    let wanted: Vec<Ask> = browser
        .rows
        .iter()
        .skip(first)
        .take(shown)
        .filter_map(|entry| {
            let path = folder.join(&entry.name);
            state.thumbs.worth_asking(entry, &path).then(|| Ask {
                path,
                kind: entry.mime.clone(),
                modified: entry.modified,
            })
        })
        .collect();
    if wanted.is_empty() {
        return Task::none();
    }
    state.thumbs.queue(wanted);
    state.thumbs.start()
}

/// A picture has been made, or there is none to be had. The next in the queue starts.
pub fn thumb(
    state: &mut Files,
    path: PathBuf,
    modified: Option<i64>,
    picture: Option<PathBuf>,
) -> Task<Message> {
    state.thumbs.arrived(path, modified, picture);
    state.thumbs.start()
}

#[cfg(test)]
mod tests {
    use super::*;
    use librift::files::Kind;

    fn entry(name: &str, mime: &str, size: u64) -> Entry {
        Entry {
            name: name.into(),
            label: name.to_string(),
            kind: Kind::File,
            link: false,
            size,
            modified: Some(12),
            mime: mime.to_string(),
            hidden: false,
            items: None,
        }
    }

    fn thumbs() -> Thumbs {
        Thumbs {
            makers: Arc::new(librift::files::thumbnails::Makers {
                makers: vec![
                    librift::files::thumbnails::Maker::parse(
                        "[Thumbnailer Entry]\nExec=picture %o\nMimeType=image/png;\n",
                    )
                    .expect("a thumbnailer"),
                ],
            }),
            ..Thumbs::default()
        }
    }

    #[test]
    fn only_a_file_a_program_covers_is_asked_about_and_only_once() {
        let mut thumbs = thumbs();
        let path = PathBuf::from("/home/rift/Pictures/a.png");
        assert!(thumbs.worth_asking(&entry("a.png", "image/png", 10), &path));
        // no program makes a picture of a text file, and a huge one is left alone
        assert!(!thumbs.worth_asking(&entry("a.txt", "text/plain", 10), &path));
        assert!(!thumbs.worth_asking(&entry("a.png", "image/png", BIGGEST + 1), &path));
        thumbs.arrived(path.clone(), Some(12), Some(PathBuf::from("/cache/a.png")));
        assert_eq!(
            thumbs.of(&path, Some(12)),
            Some(Path::new("/cache/a.png")),
            "the picture that was made is the one drawn"
        );
        // the file as it is now has a picture, and as it was a moment later it has not
        assert_eq!(thumbs.of(&path, Some(13)), None);
        assert!(!thumbs.worth_asking(&entry("a.png", "image/png", 10), &path));
        assert!(thumbs.worth_asking(
            &Entry {
                modified: Some(13),
                ..entry("a.png", "image/png", 10)
            },
            &path
        ));
        // a file nothing could make a picture of is not asked about again
        thumbs.arrived(path.clone(), Some(12), None);
        assert!(!thumbs.worth_asking(&entry("a.png", "image/png", 10), &path));
        assert_eq!(thumbs.count(), 0);
    }
}
