//! The note that says this drive has been welcomed. The session starts Welcome at every login,
//! and Welcome goes away at once while the note is there. Done and the close button write it; Open
//! later, on the page that says there is no network, leaves it out, so the next login opens
//! Welcome again. It is in home, so a clone of the drive has it too.

use std::fs;
use std::path::PathBuf;

/// The note, under home.
pub const NOTE: &str = ".local/state/rift/welcomed";

/// What the note says, for someone who finds it.
const SAYS: &str =
    "Welcome ran on this drive. Without this file it opens again at the next login.\n";

/// Where the note is, when there is a home.
fn path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(NOTE))
}

/// Whether this drive has been welcomed.
#[must_use]
pub fn welcomed() -> bool {
    path().is_some_and(|note| note.is_file())
}

/// Write the note.
///
/// # Errors
///
/// A sentence when there is no home or the note cannot be written.
pub fn keep() -> Result<(), String> {
    let note = path().ok_or("There is no home folder to keep the note in.")?;
    if let Some(folder) = note.parent() {
        fs::create_dir_all(folder)
            .map_err(|e| format!("Could not make {}: {e}", folder.display()))?;
    }
    fs::write(&note, SAYS).map_err(|e| format!("Could not write {}: {e}", note.display()))
}
