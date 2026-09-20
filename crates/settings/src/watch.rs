//! The services a page follows while the window is open. `NetworkManager` and `BlueZ` send signals
//! on the system bus whenever something changes; each is read on a thread of its own, and a burst
//! of signals, which a sweep for networks sends, turns into one reading once it has settled. The
//! waiting itself is `librift::bus`, which the shell follows the same two services with.
//!
//! Every subscription is named. Without a name of its own iced tells two apart by the type of the
//! stream and the address of the function that makes it, and folds these two into one.

use iced::Subscription;
use librift::{bluetooth, bus, network};

use crate::ui::Message;

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

/// Read a source now, and again after every change `start` pokes about. The stream ends when the
/// window is gone.
fn follow<S, R>(start: S, read: R) -> iced::futures::channel::mpsc::UnboundedReceiver<Message>
where
    S: FnOnce(std::sync::mpsc::Sender<()>) + Send + 'static,
    R: Fn() -> Message + Send + 'static,
{
    let (sender, receiver) = iced::futures::channel::mpsc::unbounded();
    bus::follow(start, read, move |message| {
        sender.unbounded_send(message).is_ok()
    });
    receiver
}

/// Follow a signal watch, saying what went wrong in the app's own words.
fn listen<W>(poke: std::sync::mpsc::Sender<()>, watch: W)
where
    W: Fn(&mut dyn FnMut() -> bool) -> Result<(), String> + Send + 'static,
{
    bus::listen(poke, watch, |why| eprintln!("rift-settings: {why}"));
}
