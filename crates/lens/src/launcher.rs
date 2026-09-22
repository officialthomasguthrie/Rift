//! Starting an app. The desktop entries are librift's, since Settings names the apps the dock keeps
//! too, and so is the start itself, in a scope of its own under the owner's systemd, which the file
//! manager makes the same way when it opens a file.

pub use librift::apps::{App, load};

/// Start the app and let it go.
///
/// The shell is a unit of the owner's systemd, and a child left in that unit is stopped with it, so
/// a crash of the shell would take every app it started. librift makes the app a scope of its own
/// before the app starts, which is what Horizon does for what a key starts, so nothing the app
/// starts early is left behind in the shell's unit either.
///
/// # Errors
///
/// When the program cannot be started.
pub fn launch(app: &App) -> Result<(), String> {
    librift::apps::launch(app, &[])
}
