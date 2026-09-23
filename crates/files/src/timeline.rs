//! Timeline: a folder as it was, out of the snapshots Vault takes of home every hour. Vault lists
//! them on the system bus; the snapshots themselves are read-only copies of home beside it on the
//! drive, and they keep home's own permissions, so the owner reads their own files in them the way
//! they read any other folder. Putting a file back is an ordinary copy out of one, which is why it
//! follows the rule the rest of Files follows: nothing is ever written over without being asked,
//! and what was there goes to the trash.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use librift::files;
use librift::vault;

/// The snapshots of home, oldest first, as Vault lists them.
///
/// # Errors
///
/// A sentence when Vault is not there or could not read them.
pub fn moments() -> Result<Vec<String>, String> {
    let mut found = vault::list()?;
    found.retain(|name| vault::snapshot_time(name).is_some());
    Ok(found)
}

/// Whether a folder has a timeline at all. Only home is snapshotted, so a folder on a disk that
/// was plugged in, or on the drive's exchange partition, has none.
#[must_use]
pub fn covers(folder: &Path) -> bool {
    folder.starts_with(vault::HOME)
}

/// What a moment is called, the way the bar over the list says it: the day and the time it was
/// taken, in the local zone.
#[must_use]
pub fn label(at: &str, now: i64, offset: i32) -> String {
    vault::snapshot_time(at).map_or_else(
        || at.to_string(),
        |taken| files::when_moment(taken, now, offset),
    )
}

/// The moment before or after this one, when there is one. `earlier` steps back in time.
#[must_use]
pub fn step<'a>(moments: &'a [String], at: &str, earlier: bool) -> Option<&'a String> {
    let here = moments.iter().position(|name| name == at)?;
    if earlier {
        here.checked_sub(1).and_then(|before| moments.get(before))
    } else {
        moments.get(here + 1)
    }
}

/// What a restore copies and what it would replace: where each name lies in the snapshot, and the
/// names of those already in the folder now. With no names it is everything in the moment.
#[must_use]
pub fn bringing(place: &Path, folder: &Path, names: &[OsString]) -> (Vec<PathBuf>, Vec<String>) {
    let names: Vec<OsString> = if names.is_empty() {
        std::fs::read_dir(place).map_or_else(
            |_| Vec::new(),
            |read| {
                read.filter_map(Result::ok)
                    .map(|entry| entry.file_name())
                    .collect()
            },
        )
    } else {
        names.to_vec()
    };
    let taken = names
        .iter()
        .filter(|name| std::fs::symlink_metadata(folder.join(name)).is_ok())
        .map(|name| name.to_string_lossy().into_owned())
        .collect();
    let from = names.iter().map(|name| place.join(name)).collect();
    (from, taken)
}

/// What the button says: Restore for what is selected, and everything in the folder when nothing
/// is.
#[must_use]
pub const fn restoring(selected: bool) -> &'static str {
    if selected {
        "Restore"
    } else {
        "Restore everything"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_moment_reads_as_a_day_and_a_time() {
        // 2026-09-22 10:42:00 UTC
        let now = 1_790_073_720;
        assert_eq!(label("2026-09-22T09:00:07Z", now, 0), "Today at 09:00");
        assert_eq!(label("2026-09-21T23:00:07Z", now, 0), "Yesterday at 23:00");
        assert_eq!(label("never", now, 0), "never");
    }

    #[test]
    fn the_timeline_is_of_home_alone() {
        assert!(covers(Path::new("/home/rift/Documents")));
        assert!(!covers(Path::new("/exchange")));
        assert!(!covers(Path::new("/run/media/rift/STICK")));
    }

    #[test]
    fn stepping_runs_from_the_oldest_to_the_newest() {
        let moments: Vec<String> = [
            "2026-09-20T08:00:00Z",
            "2026-09-21T08:00:00Z",
            "2026-09-22T08:00:00Z",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            step(&moments, "2026-09-21T08:00:00Z", true).map(String::as_str),
            Some("2026-09-20T08:00:00Z")
        );
        assert_eq!(
            step(&moments, "2026-09-21T08:00:00Z", false).map(String::as_str),
            Some("2026-09-22T08:00:00Z")
        );
        assert_eq!(step(&moments, "2026-09-20T08:00:00Z", true), None);
        assert_eq!(step(&moments, "2026-09-22T08:00:00Z", false), None);
        assert_eq!(step(&moments, "2026-09-19T08:00:00Z", true), None);
    }

    #[test]
    fn what_a_restore_copies_and_what_it_would_replace() {
        let folder = std::env::temp_dir().join(format!("files-timeline-{}", std::process::id()));
        let (place, now) = (folder.join("moment"), folder.join("now"));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&place).unwrap();
        std::fs::create_dir_all(&now).unwrap();
        for name in ["notes.txt", "todo.txt"] {
            std::fs::write(place.join(name), "then").unwrap();
        }
        std::fs::write(now.join("notes.txt"), "now").unwrap();
        let (from, taken) = bringing(&place, &now, &[OsString::from("notes.txt")]);
        assert_eq!(from, [place.join("notes.txt")]);
        assert_eq!(taken, ["notes.txt"]);
        let (all, both) = bringing(&place, &now, &[]);
        assert_eq!(all.len(), 2, "everything in the moment");
        assert_eq!(both, ["notes.txt"]);
        assert_eq!(restoring(true), "Restore");
        assert_eq!(restoring(false), "Restore everything");
        let _ = std::fs::remove_dir_all(&folder);
    }
}
