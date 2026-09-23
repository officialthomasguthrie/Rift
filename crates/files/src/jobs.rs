//! The work that takes a while: copying, moving, the trash and deleting. Each job runs on a thread
//! of its own and says how far it has got, and the app goes on while it does, closed windows and
//! all, until every job is done. Nothing is ever written over: a copy or a move into a folder that
//! already has the name gets a name of its own beside it, and when the owner asks to replace what
//! is there, what is there goes to the trash first, so it can still be brought back.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use iced::Task;
use iced::futures::channel::mpsc;
use librift::files::trash::Trash;
use librift::files::{self, free_name, remove};

use crate::ui::Message;

/// How much of a file is copied between two looks at whether the job was stopped.
const CHUNK: usize = 1 << 20;
/// How often a job says how far it has got, at most.
const EVERY: Duration = Duration::from_millis(100);

/// What a job does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Work {
    /// Copy these into a folder.
    Copy {
        /// What is copied.
        from: Vec<PathBuf>,
        /// Where to.
        into: PathBuf,
        /// Whether a name that is taken is replaced, what was there going to the trash.
        replace: bool,
    },
    /// Bring these back out of a snapshot into the folder they were in, which is a copy out of a
    /// read-only folder and follows the same rule: a name that is taken is only replaced when the
    /// owner asks, and what was there goes to the trash.
    Bring {
        /// The files in the snapshot.
        from: Vec<PathBuf>,
        /// The folder in home they go back into.
        into: PathBuf,
        /// Whether a name that is taken is replaced, what was there going to the trash.
        replace: bool,
    },
    /// Move these into a folder.
    Move {
        /// What is moved.
        from: Vec<PathBuf>,
        /// Where to.
        into: PathBuf,
        /// Whether a name that is taken is replaced, what was there going to the trash.
        replace: bool,
    },
    /// Move these into the trash: the one in home, or the drive's own.
    Trash(Vec<PathBuf>),
    /// Delete these for good.
    Delete(Vec<PathBuf>),
    /// Put these back from the trash, each by where it lies in the trash it is in.
    Restore(Vec<PathBuf>),
    /// Delete these in the trash for good, each by where it lies in the trash it is in.
    Forget(Vec<PathBuf>),
    /// Delete everything in these trashes for good.
    Empty(Vec<PathBuf>),
}

/// How a job ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Where each thing it copied, moved or put back ended up.
    pub made: Vec<PathBuf>,
    /// Where what it moved to the trash lies there, for Undo.
    pub trashed: Vec<PathBuf>,
    /// How many things it did.
    pub count: usize,
    /// How many names it replaced, what was there going to the trash.
    pub replaced: usize,
    /// The first thing that went wrong.
    pub problem: Option<String>,
    /// Whether it was stopped before the end.
    pub stopped: bool,
}

/// What a job says as it goes.
#[derive(Debug, Clone)]
pub enum Step {
    /// How much there is to do: bytes for a copy, things for the rest.
    Counted(u64),
    /// How much is done.
    Moved(u64),
    /// It has ended.
    Finished(Box<Outcome>),
}

/// A job the app keeps an eye on.
#[derive(Debug)]
pub struct Job {
    /// Its number, which the messages about it carry.
    pub number: u64,
    /// What it does.
    pub work: Work,
    /// The window it was started from, where it says how it went.
    pub window: Option<iced::window::Id>,
    /// When it started, so a job that is over quickly never shows its progress.
    pub started: Instant,
    /// How much there is to do.
    pub total: u64,
    /// How much is done.
    pub done: u64,
    /// How it ended, once it has.
    pub outcome: Option<Outcome>,
    /// Set to stop it.
    pub stop: Arc<AtomicBool>,
}

impl Job {
    /// A job that has not started.
    #[must_use]
    pub fn new(number: u64, work: Work, window: Option<iced::window::Id>) -> Self {
        Self {
            number,
            work,
            window,
            started: Instant::now(),
            total: 0,
            done: 0,
            outcome: None,
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// How far it has got, out of a hundred.
    #[must_use]
    pub fn percent(&self) -> u32 {
        if self.total == 0 {
            return 0;
        }
        u32::try_from(self.done.min(self.total) * 100 / self.total).unwrap_or(100)
    }

    /// Whether it is still going.
    #[must_use]
    pub fn running(&self) -> bool {
        self.outcome.is_none()
    }
}

impl Work {
    /// What it is doing, for the line under the list while it runs.
    #[must_use]
    pub fn doing(&self) -> String {
        match self {
            Self::Copy { from, into, .. } => {
                format!("Copying {} to {}", things(from), files::shown(into))
            }
            Self::Move { from, into, .. } => {
                format!("Moving {} to {}", things(from), files::shown(into))
            }
            Self::Bring { from, into, .. } => {
                format!("Putting {} back in {}", things(from), files::shown(into))
            }
            Self::Trash(from) => format!("Moving {} to the trash", things(from)),
            Self::Delete(from) => format!("Deleting {}", things(from)),
            Self::Restore(names) => format!("Putting back {}", count_words(names.len())),
            Self::Forget(names) => format!("Deleting {}", count_words(names.len())),
            Self::Empty(_) => "Emptying the trash".to_string(),
        }
    }

    /// What it did, for the toast when it is over.
    #[must_use]
    pub fn done(&self, outcome: &Outcome) -> String {
        if let Some(problem) = &outcome.problem {
            return problem.clone();
        }
        let made = |fallback: &[PathBuf]| {
            if outcome.made.len() == 1 {
                name_of(&outcome.made[0])
            } else {
                things(fallback)
            }
        };
        let what = match self {
            Self::Copy { from, into, .. } => {
                format!("Copied {} to {}", made(from), files::shown(into))
            }
            Self::Move { from, into, .. } => {
                format!("Moved {} to {}", made(from), files::shown(into))
            }
            Self::Bring { from, into, .. } => {
                format!("Put {} back in {}", made(from), files::shown(into))
            }
            Self::Trash(from) => format!("Moved {} to the trash", things(from)),
            Self::Delete(from) => format!("Deleted {}", things(from)),
            Self::Restore(_) if outcome.made.len() == 1 => format!(
                "Put {} back in {}",
                name_of(&outcome.made[0]),
                outcome.made[0]
                    .parent()
                    .map_or_else(|| "/".to_string(), files::shown)
            ),
            Self::Restore(names) => format!("Put back {}", count_words(names.len())),
            Self::Forget(names) => format!("Deleted {}", count_words(names.len())),
            Self::Empty(_) => "Emptied the trash".to_string(),
        };
        let old = match outcome.replaced {
            0 => String::new(),
            1 => " The one that was there is in the trash.".to_string(),
            many => format!(" The {many} that were there are in the trash."),
        };
        match (outcome.stopped, outcome.made.is_empty(), self) {
            (true, true, Self::Move { .. }) => "Stopped. Nothing was moved.".to_string(),
            (true, true, Self::Bring { .. }) => "Stopped. Nothing was put back.".to_string(),
            (true, true, _) => "Stopped. Nothing was copied.".to_string(),
            (true, false, _) => format!("Stopped. {what}.{old}"),
            (false, _, _) => format!("{what}.{old}"),
        }
    }

    /// Whether it can be stopped halfway: a copy, a move, which may copy, and a file brought back
    /// out of a snapshot, which is a copy.
    #[must_use]
    pub const fn stoppable(&self) -> bool {
        matches!(
            self,
            Self::Copy { .. } | Self::Move { .. } | Self::Bring { .. }
        )
    }
}

/// Things by name when there is one, and by how many otherwise.
fn things(paths: &[PathBuf]) -> String {
    match paths {
        [one] => name_of(one),
        _ => count_words(paths.len()),
    }
}

fn count_words(count: usize) -> String {
    if count == 1 {
        "1 item".to_string()
    } else {
        format!("{count} items")
    }
}

fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Start a job on a thread of its own. The task carries everything it says.
pub fn start(job: &Job, offset: i32) -> Task<Message> {
    let (sender, receiver) = mpsc::unbounded();
    let work = job.work.clone();
    let number = job.number;
    let stop = Arc::clone(&job.stop);
    thread::spawn(move || {
        let mut said = Said {
            sender,
            number,
            done: 0,
            last: Instant::now(),
        };
        let outcome = run(&work, &stop, &mut said, offset);
        said.send(Step::Finished(Box::new(outcome)));
    });
    Task::stream(receiver)
}

/// How a job says how far it has got.
struct Said {
    sender: mpsc::UnboundedSender<Message>,
    number: u64,
    done: u64,
    last: Instant,
}

impl Said {
    fn send(&self, step: Step) {
        let _ = self.sender.unbounded_send(Message::Job(self.number, step));
    }

    /// Count some more as done, and say so when the last time was a while ago.
    fn add(&mut self, amount: u64) {
        self.done += amount;
        if self.last.elapsed() >= EVERY {
            self.last = Instant::now();
            self.send(Step::Moved(self.done));
        }
    }
}

/// Why a copy did not finish.
enum Halt {
    /// The job was stopped.
    Stopped,
    /// Something went wrong, in these words.
    Failed(String),
}

fn run(work: &Work, stop: &AtomicBool, said: &mut Said, offset: i32) -> Outcome {
    let mut outcome = Outcome::default();
    match work {
        Work::Copy {
            from,
            into,
            replace,
        }
        | Work::Bring {
            from,
            into,
            replace,
        } => {
            said.send(Step::Counted(from.iter().map(|path| bytes(path)).sum()));
            let how = How {
                moving: false,
                replace: *replace,
                in_bytes: true,
                offset,
            };
            transfer(from, into, stop, said, &mut outcome, how);
        }
        Work::Move {
            from,
            into,
            replace,
        } => {
            // a move onto another disk copies every byte, so it counts them; one within a disk is
            // a rename each, and counting the things is what shows how far it has got
            let in_bytes = crossing(from, into);
            said.send(Step::Counted(if in_bytes {
                from.iter().map(|path| bytes(path)).sum()
            } else {
                from.len() as u64
            }));
            let how = How {
                moving: true,
                replace: *replace,
                in_bytes,
                offset,
            };
            transfer(from, into, stop, said, &mut outcome, how);
        }
        Work::Trash(from) => {
            said.send(Step::Counted(from.len() as u64));
            into_the_trash(from, said, &mut outcome, offset);
        }
        Work::Delete(from) => {
            said.send(Step::Counted(from.len() as u64));
            for source in from {
                match remove(source) {
                    Ok(()) => outcome.count += 1,
                    Err(why) => {
                        outcome.problem.get_or_insert(why);
                    }
                }
                said.add(1);
            }
        }
        Work::Restore(files) | Work::Forget(files) => {
            said.send(Step::Counted(files.len() as u64));
            for file in files {
                let done = match Trash::of(file) {
                    Some((trash, name)) if matches!(work, Work::Restore(_)) => {
                        trash.restore(&name).map(|back| outcome.made.push(back))
                    }
                    Some((trash, name)) => trash.delete(&name),
                    None => Err(format!("{} is not in the trash.", file.display())),
                };
                match done {
                    Ok(()) => outcome.count += 1,
                    Err(why) => {
                        outcome.problem.get_or_insert(why);
                    }
                }
                said.add(1);
            }
        }
        Work::Empty(roots) => {
            said.send(Step::Counted(roots.len() as u64));
            for root in roots {
                match Trash::at(root.clone()).empty() {
                    Ok(()) => outcome.count += 1,
                    Err(why) => {
                        outcome.problem.get_or_insert(why);
                    }
                }
                said.add(1);
            }
        }
    }
    outcome
}

/// Move each of these to the trash: the one in home for anything on home's file system, and the
/// drive's own for anything on a drive.
fn into_the_trash(from: &[PathBuf], said: &mut Said, outcome: &mut Outcome, offset: i32) {
    let now = librift::time::now();
    let uid = files::uid();
    for source in from {
        let put = Trash::for_path(source, uid)
            .ok_or_else(|| {
                format!(
                    "{} is on a disk with no trash.",
                    source
                        .file_name()
                        .unwrap_or(source.as_os_str())
                        .to_string_lossy()
                )
            })
            .and_then(|trash| trash.put(source, now, offset));
        match put {
            Ok(trashed) => {
                outcome.trashed.push(trashed.file());
                outcome.count += 1;
            }
            Err(why) => {
                outcome.problem.get_or_insert(why);
            }
        }
        said.add(1);
    }
}

/// How a copy or a move goes.
#[derive(Debug, Clone, Copy)]
struct How {
    /// Whether it moves what it copies.
    moving: bool,
    /// Whether a name that is taken is replaced, what was there going to the trash first.
    replace: bool,
    /// Whether it counts bytes rather than things.
    in_bytes: bool,
    /// The local zone's distance from UTC, for the note the trash writes.
    offset: i32,
}

/// Copy or move each of `from` into a folder, until the job is stopped.
fn transfer(
    from: &[PathBuf],
    into: &Path,
    stop: &AtomicBool,
    said: &mut Said,
    outcome: &mut Outcome,
    how: How,
) {
    for source in from {
        // what a rename moves in one go, which no copy will count
        let whole = if how.moving && how.in_bytes {
            bytes(source)
        } else {
            0
        };
        let taken = how.replace
            && source
                .file_name()
                .is_some_and(|name| fs::symlink_metadata(into.join(name)).is_ok());
        let before = said.done;
        let done = if how.moving {
            move_into(source, into, stop, said, how)
        } else {
            copy_into(source, into, stop, said, how)
        };
        match done {
            Ok(made) => {
                // it kept the name only because what was there went to the trash
                if taken && made.file_name() == source.file_name() {
                    outcome.replaced += 1;
                }
                outcome.made.push(made);
                outcome.count += 1;
            }
            Err(Halt::Stopped) => {
                outcome.stopped = true;
                return;
            }
            Err(Halt::Failed(why)) => {
                outcome.problem.get_or_insert(why);
            }
        }
        if how.moving {
            if how.in_bytes {
                said.add(whole.saturating_sub(said.done - before));
            } else {
                said.add(1);
            }
        }
    }
}

/// Whether any of these is on another file system from the folder they are going to, which is
/// what makes a move a copy.
fn crossing(from: &[PathBuf], into: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;

    let Ok(target) = fs::metadata(into) else {
        return false;
    };
    from.iter()
        .any(|path| fs::symlink_metadata(path).is_ok_and(|meta| meta.dev() != target.dev()))
}

/// Where something of this name goes in a folder: its own name when it is free, and a name of its
/// own beside it when it is not. When the owner asked to replace what is there, what is there goes
/// to the trash first, so the name is free again and nothing a person had is written over; if that
/// disk has no trash, it keeps both after all.
fn free_target(source: &Path, into: &Path, copy: bool, how: How) -> PathBuf {
    let name = source.file_name().unwrap_or(source.as_os_str());
    let taken = into.join(name);
    // nothing is put in the way of itself: a copy into the folder it is in already keeps both
    if how.replace && taken != source && fs::symlink_metadata(&taken).is_ok() {
        if let Some(trash) = Trash::for_path(&taken, files::uid()) {
            let _ = trash.put(&taken, librift::time::now(), how.offset);
        }
    }
    into.join(free_name(into, name, copy))
}

/// How many bytes a copy of this will write: a file's size, a folder's files together, and
/// nothing for a link, which is copied as a link.
fn bytes(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.is_dir() {
        fs::read_dir(path).map_or(0, |entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| bytes(&entry.path()))
                .sum()
        })
    } else if meta.is_file() {
        meta.len()
    } else {
        0
    }
}

/// Copy one thing into a folder, under its own name, or a name of its own when that is taken, and
/// say where it went.
fn copy_into(
    source: &Path,
    into: &Path,
    stop: &AtomicBool,
    said: &mut Said,
    how: How,
) -> Result<PathBuf, Halt> {
    let name = source
        .file_name()
        .ok_or_else(|| Halt::Failed(format!("{} cannot be copied.", source.display())))?;
    if into.starts_with(source) {
        return Err(Halt::Failed(format!(
            "{} cannot be copied into itself.",
            name.to_string_lossy()
        )));
    }
    let target = free_target(source, into, true, how);
    copy_whole(source, &target, stop, said)?;
    Ok(target)
}

/// Copy one thing to a free path, and take away what was written when the copy does not finish:
/// half a copy is worse than none.
fn copy_whole(
    source: &Path,
    target: &Path,
    stop: &AtomicBool,
    said: &mut Said,
) -> Result<(), Halt> {
    copy_tree(source, target, stop, said).inspect_err(|_| {
        if fs::symlink_metadata(target).is_ok() {
            let _ = remove(target);
        }
    })
}

/// Move one thing into a folder: a rename when the folder is on the same file system, otherwise a
/// copy and then the original deleted, but only once the copy is whole.
fn move_into(
    source: &Path,
    into: &Path,
    stop: &AtomicBool,
    said: &mut Said,
    how: How,
) -> Result<PathBuf, Halt> {
    let name = source
        .file_name()
        .ok_or_else(|| Halt::Failed(format!("{} cannot be moved.", source.display())))?;
    if source.parent() == Some(into) {
        return Ok(source.to_path_buf());
    }
    if into.starts_with(source) {
        return Err(Halt::Failed(format!(
            "{} cannot be moved into itself.",
            name.to_string_lossy()
        )));
    }
    let target = free_target(source, into, false, how);
    match rename_new(source, &target) {
        Ok(()) => Ok(target),
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            // another file system, so a copy, and the original goes once the copy is whole. a move
            // onto another disk counts bytes, so the copy says how it is going; one that counts
            // things says nothing of its own
            if how.in_bytes {
                copy_whole(source, &target, stop, said)?;
            } else {
                let mut quiet = Said {
                    sender: said.sender.clone(),
                    number: said.number,
                    done: 0,
                    last: Instant::now(),
                };
                copy_whole(source, &target, stop, &mut quiet)?;
            }
            remove(source).map_err(Halt::Failed)?;
            Ok(target)
        }
        Err(e) => Err(Halt::Failed(format!(
            "{} could not be moved: {e}.",
            name.to_string_lossy()
        ))),
    }
}

/// Rename `from` to `to` only when nothing is there: in one call where the file system can, and
/// after a look where it cannot.
///
/// # Errors
///
/// When something is at `to` already, or the rename fails.
pub fn rename_new(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        use rustix::fs::{CWD, RenameFlags, renameat_with};
        use rustix::io::Errno;
        match renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE) {
            Ok(()) => return Ok(()),
            // a file system that cannot do it in one call
            Err(Errno::INVAL | Errno::NOSYS) => {}
            Err(e) => return Err(e.into()),
        }
    }
    if fs::symlink_metadata(to).is_ok() {
        return Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists));
    }
    fs::rename(from, to)
}

/// Copy a file, a link or a whole folder to a path that is free: a link as a link, a file with its
/// permissions and its modified time, a folder with everything in it. A device, a socket or a pipe
/// is left out.
fn copy_tree(from: &Path, to: &Path, stop: &AtomicBool, said: &mut Said) -> Result<(), Halt> {
    let failed =
        |e: std::io::Error| Halt::Failed(format!("{} could not be copied: {e}.", name_of(from)));
    let meta = fs::symlink_metadata(from).map_err(failed)?;
    let kind = meta.file_type();
    if kind.is_symlink() {
        let points = fs::read_link(from).map_err(failed)?;
        std::os::unix::fs::symlink(points, to).map_err(failed)?;
    } else if kind.is_dir() {
        fs::create_dir(to).map_err(failed)?;
        let entries = fs::read_dir(from).map_err(failed)?;
        for entry in entries {
            let entry = entry.map_err(failed)?;
            copy_tree(&entry.path(), &to.join(entry.file_name()), stop, said)?;
        }
        // a folder that is read only gets that last, once everything is in it
        fs::set_permissions(to, meta.permissions()).map_err(failed)?;
    } else if kind.is_file() {
        copy_file(from, to, &meta, stop, said).map_err(|halt| match halt {
            Halt::Failed(why) => {
                Halt::Failed(format!("{} could not be copied: {why}.", name_of(from)))
            }
            Halt::Stopped => Halt::Stopped,
        })?;
    }
    Ok(())
}

/// Copy the bytes of one file. On a file system that shares blocks between files, btrfs on the
/// drive, the copy shares them and takes no time; anywhere else it is written a chunk at a time,
/// looking at whether the job was stopped between two chunks.
fn copy_file(
    from: &Path,
    to: &Path,
    meta: &fs::Metadata,
    stop: &AtomicBool,
    said: &mut Said,
) -> Result<(), Halt> {
    let failed = |e: std::io::Error| Halt::Failed(e.to_string());
    let mut source = File::open(from).map_err(failed)?;
    let mut target = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)
        .map_err(failed)?;
    if shared(&source, &target) {
        said.add(meta.len());
    } else {
        let mut chunk = vec![0; CHUNK];
        loop {
            if stop.load(Ordering::Relaxed) {
                return Err(Halt::Stopped);
            }
            let read = source.read(&mut chunk).map_err(failed)?;
            if read == 0 {
                break;
            }
            target.write_all(&chunk[..read]).map_err(failed)?;
            said.add(read as u64);
        }
    }
    target.set_permissions(meta.permissions()).map_err(failed)?;
    if let Ok(modified) = meta.modified() {
        let _ = target.set_modified(modified);
    }
    Ok(())
}

/// Make `target` share the blocks of `source`, which btrfs does in one call for a whole file.
#[cfg(target_os = "linux")]
fn shared(source: &File, target: &File) -> bool {
    rustix::fs::ioctl_ficlone(target, source).is_ok()
}

/// Anywhere but Linux the bytes are copied.
#[cfg(not(target_os = "linux"))]
fn shared(_: &File, _: &File) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("files-jobs-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("the test folder");
        root
    }

    /// A copy or a move that keeps both when a name is taken, which is what a paste does until
    /// the owner answers the question with Replace.
    fn both(moving: bool) -> How {
        How {
            moving,
            replace: false,
            in_bytes: !moving,
            offset: 0,
        }
    }

    fn quiet() -> (Said, mpsc::UnboundedReceiver<Message>) {
        let (sender, receiver) = mpsc::unbounded();
        (
            Said {
                sender,
                number: 1,
                done: 0,
                last: Instant::now(),
            },
            receiver,
        )
    }

    #[test]
    fn a_copy_keeps_everything_and_writes_over_nothing() {
        let root = temporary("copy");
        fs::create_dir_all(root.join("Notes/inside")).unwrap();
        fs::write(root.join("Notes/inside/a.txt"), "a").unwrap();
        std::os::unix::fs::symlink("inside/a.txt", root.join("Notes/link")).unwrap();
        fs::create_dir(root.join("Backup")).unwrap();
        let (mut said, _heard) = quiet();
        let stop = AtomicBool::new(false);
        let made = copy_into(
            &root.join("Notes"),
            &root.join("Backup"),
            &stop,
            &mut said,
            both(false),
        )
        .unwrap_or_else(|_| panic!("the copy"));
        assert_eq!(made, root.join("Backup/Notes"));
        assert_eq!(
            fs::read_to_string(root.join("Backup/Notes/inside/a.txt")).unwrap(),
            "a"
        );
        assert_eq!(
            fs::read_link(root.join("Backup/Notes/link")).unwrap(),
            Path::new("inside/a.txt")
        );
        // again, and the second copy has a name of its own
        let again = copy_into(
            &root.join("Notes"),
            &root.join("Backup"),
            &stop,
            &mut said,
            both(false),
        )
        .unwrap_or_else(|_| panic!("the second copy"));
        assert_eq!(again, root.join("Backup/Notes (copy)"));
        assert_eq!(said.done, 2);
        // and a folder never goes into itself
        assert!(matches!(
            copy_into(
                &root.join("Notes"),
                &root.join("Notes/inside"),
                &stop,
                &mut said,
                both(false)
            ),
            Err(Halt::Failed(why)) if why == "Notes cannot be copied into itself."
        ));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_stopped_copy_leaves_nothing_half_done() {
        let root = temporary("stop");
        fs::write(root.join("big"), vec![1; CHUNK * 2]).unwrap();
        fs::create_dir(root.join("to")).unwrap();
        let (mut said, _heard) = quiet();
        let stop = AtomicBool::new(true);
        assert!(matches!(
            copy_into(
                &root.join("big"),
                &root.join("to"),
                &stop,
                &mut said,
                both(false)
            ),
            Err(Halt::Stopped)
        ));
        assert!(!root.join("to/big").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_move_renames_and_never_writes_over() {
        let root = temporary("move");
        fs::create_dir_all(root.join("a")).unwrap();
        fs::create_dir_all(root.join("b")).unwrap();
        fs::write(root.join("a/x.txt"), "moved").unwrap();
        fs::write(root.join("b/x.txt"), "there before").unwrap();
        let (mut said, _heard) = quiet();
        let stop = AtomicBool::new(false);
        let made = move_into(
            &root.join("a/x.txt"),
            &root.join("b"),
            &stop,
            &mut said,
            both(true),
        )
        .unwrap_or_else(|_| panic!("the move"));
        assert_eq!(made, root.join("b/x (2).txt"));
        assert_eq!(
            fs::read_to_string(root.join("b/x.txt")).unwrap(),
            "there before"
        );
        assert!(!root.join("a/x.txt").exists());
        // into the folder it is in already is nothing to do
        let still = move_into(&made, &root.join("b"), &stop, &mut said, both(true))
            .unwrap_or_else(|_| panic!("the move"));
        assert_eq!(still, made);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn what_is_replaced_goes_to_the_trash_and_the_name_is_free() {
        let root = temporary("replace");
        fs::create_dir_all(root.join("a")).unwrap();
        fs::create_dir_all(root.join("b")).unwrap();
        fs::write(root.join("a/x.txt"), "new").unwrap();
        fs::write(root.join("b/x.txt"), "there before").unwrap();
        // the trash this file belongs in, made now so that putting something in it works. a
        // machine with nowhere to keep one, which a build sandbox is, has nothing to test here
        let Some(trash) = Trash::for_path(&root.join("b/x.txt"), files::uid())
            .filter(|trash| fs::create_dir_all(trash.root().join("files")).is_ok())
        else {
            let _ = fs::remove_dir_all(&root);
            return;
        };
        let (mut said, _heard) = quiet();
        let stop = AtomicBool::new(false);
        let mut outcome = Outcome::default();
        let how = How {
            moving: false,
            replace: true,
            in_bytes: true,
            offset: 0,
        };
        transfer(
            &[root.join("a/x.txt")],
            &root.join("b"),
            &stop,
            &mut said,
            &mut outcome,
            how,
        );
        assert_eq!(outcome.made, vec![root.join("b/x.txt")]);
        assert_eq!(outcome.replaced, 1);
        assert_eq!(fs::read_to_string(root.join("b/x.txt")).unwrap(), "new");
        // the old one is in the trash of the file system it was on, not written over
        let was = fs::canonicalize(&root).unwrap().join("b/x.txt");
        let found = trash
            .list(0)
            .into_iter()
            .find(|item| item.path == was)
            .expect("the one that was there, in the trash");
        assert_eq!(found.label(), "x.txt");
        // and the test leaves nothing behind in it
        trash.delete(&found.name).unwrap();
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_job_says_what_it_did() {
        let one = vec![PathBuf::from("/home/rift/report.pdf")];
        let two = vec![PathBuf::from("/home/rift/a"), PathBuf::from("/home/rift/b")];
        let copy = Work::Copy {
            from: two.clone(),
            into: PathBuf::from("/home/rift/Documents"),
            replace: false,
        };
        assert_eq!(copy.doing(), "Copying 2 items to Documents");
        let done = Outcome {
            count: 2,
            made: two.clone(),
            ..Outcome::default()
        };
        assert_eq!(copy.done(&done), "Copied 2 items to Documents.");
        assert_eq!(
            Work::Trash(one.clone()).doing(),
            "Moving report.pdf to the trash"
        );
        assert_eq!(
            Work::Trash(one).done(&Outcome::default()),
            "Moved report.pdf to the trash."
        );
        let back = Outcome {
            made: vec![PathBuf::from("/home/rift/Documents/report.pdf")],
            count: 1,
            ..Outcome::default()
        };
        assert_eq!(
            Work::Restore(vec![PathBuf::from(
                "/home/rift/.local/share/Trash/files/report.pdf"
            )])
            .done(&back),
            "Put report.pdf back in Documents."
        );
        // and what it replaced is in the trash, so it is still there to bring back
        let over = Outcome {
            count: 2,
            made: two,
            replaced: 1,
            ..Outcome::default()
        };
        assert_eq!(
            copy.done(&over),
            "Copied 2 items to Documents. The one that was there is in the trash."
        );
        let broke = Outcome {
            problem: Some("a could not be copied: Permission denied.".to_string()),
            ..Outcome::default()
        };
        assert_eq!(
            copy.done(&broke),
            "a could not be copied: Permission denied."
        );
        assert!(copy.stoppable() && !Work::Empty(Vec::new()).stoppable());
    }
}
