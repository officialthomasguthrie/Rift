//! The services a page follows while the window is open. `NetworkManager`, `BlueZ` and `UPower`
//! send signals on the system bus whenever something changes, and `pw-mon` prints what `PipeWire`
//! does; each is read on a thread of its own, and a burst of signals, which a sweep for networks
//! sends, turns into one reading once it has settled. The waiting itself is `librift::bus`, which
//! the shell follows the same services with.
//!
//! Every subscription is named. Without a name of its own iced tells two apart by the type of the
//! stream and the address of the function that makes it, and folds these two into one.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use iced::Subscription;
use librift::sound::Monitor;
use librift::{Component, battery, bluetooth, bus, network, sound};

use crate::ai;
use crate::ui::Message;

/// How long to wait before starting `pw-mon` again when it went away.
const RETRY: Duration = Duration::from_secs(5);

/// What `NetworkManager` says, now and after every change.
pub fn network() -> Subscription<Message> {
    Subscription::run_with("network", |_| {
        follow(
            |poke| listen(poke, |each| bus::signals(network::SERVICE, each)),
            || Message::Network(Box::new(network::read())),
        )
    })
}

/// What `BlueZ` says, now and after every change, and when it starts or stops, which it does when
/// an adapter is plugged in or taken out.
pub fn bluetooth() -> Subscription<Message> {
    Subscription::run_with("bluetooth", |_| {
        follow(
            |poke| {
                let owner = poke.clone();
                listen(owner, |each| bus::owner_changes(bluetooth::SERVICE, each));
                listen(poke, |each| bus::signals(bluetooth::SERVICE, each));
            },
            || Message::Bluetooth(bluetooth::read()),
        )
    })
}

/// What `PipeWire` says, now and whenever a sink, a source or a card changes.
pub fn sound() -> Subscription<Message> {
    Subscription::run_with("sound", |_| {
        follow(monitor, || Message::Sound(Box::new(sound::read())))
    })
}

/// What Quasar says about the models it runs, now and after every change it announces: a model that
/// has finished loading, or one that could not.
pub fn quasar() -> Subscription<Message> {
    Subscription::run_with("quasar", |_| {
        follow(
            |poke| {
                let name = Component::Quasar.dbus_name();
                listen(poke, move |each| bus::signals(&name, each));
            },
            ai::reading,
        )
    })
}

/// The battery, now and after every change.
pub fn battery() -> Subscription<Message> {
    Subscription::run_with("battery", |_| {
        follow(
            |poke| listen(poke, |each| bus::signals(battery::SERVICE, each)),
            || Message::Battery(battery::read()),
        )
    })
}

/// `pw-mon` prints every change `PipeWire` makes; a poke for each one that is about a sink, a
/// source or a card. Without it the page still shows what `PipeWire` said when it came up, and
/// what the page itself wrote.
fn monitor(poke: Sender<()>) {
    thread::spawn(move || {
        loop {
            let started = Command::new("pw-mon")
                .args(["--no-colors", "--hide-props", "--hide-params"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            match started {
                Ok(mut child) => {
                    let mut monitor = Monitor::default();
                    let wanted = child.stdout.take().is_none_or(|out| {
                        BufReader::new(out)
                            .lines()
                            .map_while(Result::ok)
                            .all(|line| !monitor.line(&line) || poke.send(()).is_ok())
                    });
                    let _ = child.kill();
                    let _ = child.wait();
                    if !wanted {
                        return;
                    }
                }
                Err(why) => eprintln!("rift-settings: could not run pw-mon: {why}"),
            }
            // PipeWire restarted, or pw-mon is not there
            thread::sleep(RETRY);
            if poke.send(()).is_err() {
                return;
            }
        }
    });
}

/// Read a source now, and again after every change `start` pokes about. The stream ends when the
/// window is gone.
fn follow<S, R>(start: S, read: R) -> iced::futures::channel::mpsc::UnboundedReceiver<Message>
where
    S: FnOnce(Sender<()>) + Send + 'static,
    R: Fn() -> Message + Send + 'static,
{
    let (sender, receiver) = iced::futures::channel::mpsc::unbounded();
    bus::follow(start, read, move |message| {
        sender.unbounded_send(message).is_ok()
    });
    receiver
}

/// Follow a signal watch, saying what went wrong in the app's own words.
fn listen<W>(poke: Sender<()>, watch: W)
where
    W: Fn(&mut dyn FnMut() -> bool) -> Result<(), String> + Send + 'static,
{
    bus::listen(poke, watch, |why| eprintln!("rift-settings: {why}"));
}
