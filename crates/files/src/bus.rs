//! `org.freedesktop.FileManager1` on the session bus: the interface every desktop's file manager
//! answers, and the one an app calls to show a file rather than open it. The portal's
//! `OpenDirectory` calls `ShowItems` first, and a browser's Show in folder does the same, so a
//! download lands in its folder with the file selected instead of a folder with nothing picked out.
//!
//! The image ships a `dbus-1/services` file for the name, so an app that calls it while Files is
//! not running starts Files, and `rift-files --bus` is what it starts: the app with no window
//! until the call that follows opens one.

use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use iced::Subscription;
use iced::futures::channel::mpsc::{self, UnboundedSender};

use crate::ui::{Message, Show};

/// The name Files takes on the session bus.
pub const NAME: &str = "org.freedesktop.FileManager1";
/// Where it hangs its object, which is the name with slashes.
pub const PATH: &str = "/org/freedesktop/FileManager1";
/// How long to wait before asking for the name again when something else has it.
const RETRY: Duration = Duration::from_secs(5);

/// The object on the bus. Every call hands the app what was asked for and answers at once: what a
/// window does with it is the app's own business, and the caller is not kept waiting for it.
struct Server {
    app: UnboundedSender<Message>,
}

impl Server {
    /// The paths of a call's addresses, which are `file://` ones. An address of something that
    /// is not a file on this machine names no path, and is passed over.
    fn paths(uris: Vec<String>) -> Vec<PathBuf> {
        uris.into_iter()
            .map(|uri| librift::files::path_of(&uri))
            .filter(|path| path.is_absolute())
            .collect()
    }

    fn show(&self, what: Show) {
        let _ = self.app.unbounded_send(Message::Show(what));
    }
}

#[zbus::interface(name = "org.freedesktop.FileManager1")]
impl Server {
    /// Show each of these files in the folder it is in, with the file selected.
    fn show_items(&self, uris: Vec<String>, startup_id: &str) {
        let _ = startup_id;
        self.show(Show::Items(Self::paths(uris)));
    }

    /// Open each of these folders.
    fn show_folders(&self, uris: Vec<String>, startup_id: &str) {
        let _ = startup_id;
        self.show(Show::Folders(Self::paths(uris)));
    }

    /// Show what is known about each of these files.
    fn show_item_properties(&self, uris: Vec<String>, startup_id: &str) {
        let _ = startup_id;
        self.show(Show::Properties(Self::paths(uris)));
    }
}

/// The interface on a thread of its own: it takes the name on the session bus and hands the app
/// every call as it comes. A bus that will not give the name, because another file manager has it,
/// is asked again every few seconds.
pub fn serve() -> Subscription<Message> {
    Subscription::run_with("file manager", |_| {
        let (app, receiver) = mpsc::unbounded();
        thread::spawn(move || {
            loop {
                match connect(Server { app: app.clone() }) {
                    Ok(connection) => {
                        eprintln!("rift-files: answering as {NAME}");
                        // zbus answers on threads of its own, so this one has nothing left to do
                        // but hold the connection open for as long as the app runs
                        let _held = connection;
                        loop {
                            thread::park();
                        }
                    }
                    Err(why) => eprintln!("rift-files: could not answer as {NAME}: {why}"),
                }
                thread::sleep(RETRY);
                if app.is_closed() {
                    return;
                }
            }
        });
        receiver
    })
}

/// The session bus with the object served and the name taken.
fn connect(server: Server) -> zbus::Result<zbus::blocking::Connection> {
    zbus::blocking::connection::Builder::session()?
        .name(NAME)?
        .serve_at(PATH, server)?
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_names_its_files_by_address() {
        let paths = Server::paths(vec![
            "file:///home/rift/Documents/notes%20of%20mine.txt".to_string(),
            "file://localhost/home/rift/Pictures".to_string(),
            // an address of something that is not a file on this machine is not a path
            "https://example.org/page".to_string(),
        ]);
        assert_eq!(
            paths,
            [
                PathBuf::from("/home/rift/Documents/notes of mine.txt"),
                PathBuf::from("/home/rift/Pictures"),
            ]
        );
    }
}
