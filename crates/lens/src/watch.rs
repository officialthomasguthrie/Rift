//! The status sources, followed as they change. `NetworkManager`, `UPower` and `BlueZ` send signals
//! on the system bus and `pw-mon` prints what `PipeWire` does; each is read on a thread of its own,
//! and a burst of signals, which a scan or a sink coming up sends, turns into one reading once it
//! has settled. A source that goes away is waited for. The waiting itself is `librift::bus`, which
//! Settings follows the same services with.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

use iced::Subscription;
use librift::{battery, bluetooth, bus, network};

use crate::status::{self, Monitor};
use crate::ui::Message;

/// How long to wait before starting `pw-mon` again when it went away.
const RETRY: Duration = Duration::from_secs(5);

/// `NetworkManager`'s picture, now and after every change.
pub fn network() -> Subscription<Message> {
    Subscription::run_with("network", |_| {
        follow(
            |poke| listen(poke, |each| bus::signals(network::SERVICE, each)),
            || Message::Network(Box::new(network::read())),
        )
    })
}

/// The battery, now and after every change.
pub fn battery() -> Subscription<Message> {
    Subscription::run_with("battery", |_| {
        follow(
            |poke| listen(poke, |each| bus::signals(battery::SERVICE, each)),
            || Message::Battery(battery::read().ok().flatten()),
        )
    })
}

/// Bluetooth, now and after every change, and when `BlueZ` starts or stops, which it does when an
/// adapter is plugged in or taken out.
pub fn bluetooth() -> Subscription<Message> {
    Subscription::run_with("bluetooth", |_| {
        follow(
            |poke| {
                let owner = poke.clone();
                listen(owner, |each| bus::owner_changes(bluetooth::SERVICE, each));
                listen(poke, |each| bus::signals(bluetooth::SERVICE, each));
            },
            || Message::Bluetooth(bluetooth::read().ok().flatten()),
        )
    })
}

/// The volume, now and whenever a sink or a card changes.
pub fn sound() -> Subscription<Message> {
    Subscription::run_with("sound", |_| {
        follow(monitor, || Message::Sound(status::volume()))
    })
}

/// Read a source now, and again after every change `start` pokes about, on threads of their own.
/// The stream ends when the shell is gone.
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

/// Follow a signal watch, saying what went wrong in the shell's own words.
fn listen<W>(poke: Sender<()>, watch: W)
where
    W: Fn(&mut dyn FnMut() -> bool) -> Result<(), String> + Send + 'static,
{
    bus::listen(poke, watch, |why| eprintln!("lens: {why}"));
}

/// `pw-mon` prints every change `PipeWire` makes; a poke for each one that is about a sink or a
/// card. Without `pw-mon` the volume is still read once a minute with the clock.
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
                Err(why) => eprintln!("lens: could not run pw-mon: {why}"),
            }
            // PipeWire restarted, or pw-mon is not there
            thread::sleep(RETRY);
            if poke.send(()).is_err() {
                return;
            }
        }
    });
}

/// Hands values to a thread that applies them, the newest first: a slider being dragged sends a
/// value for every pixel it moves, and only the last one that arrived while the program ran is
/// worth running it again for.
#[derive(Debug)]
pub struct Latest<T> {
    sender: Sender<T>,
}

impl<T: Send + 'static> Latest<T> {
    /// A thread that applies every value it is given with `apply`, skipping the ones a newer one
    /// replaced while it was busy.
    pub fn new<F: Fn(T) + Send + 'static>(apply: F) -> Self {
        let (sender, receiver) = mpsc::channel::<T>();
        thread::spawn(move || {
            while let Ok(mut value) = receiver.recv() {
                while let Ok(newer) = receiver.try_recv() {
                    value = newer;
                }
                apply(value);
            }
        });
        Self { sender }
    }

    /// Apply this value, once the thread gets to it.
    pub fn send(&self, value: T) {
        let _ = self.sender.send(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    #[test]
    fn the_newest_value_is_the_last_one_applied() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&seen);
        let latest = Latest::new(move |value: u8| {
            thread::sleep(Duration::from_millis(30));
            kept.lock().expect("the list").push(value);
        });
        for value in 0..=20 {
            latest.send(value);
        }
        let until = Instant::now() + Duration::from_secs(5);
        while seen.lock().expect("the list").last() != Some(&20) {
            assert!(Instant::now() < until, "20 was never applied");
            thread::sleep(Duration::from_millis(10));
        }
        // values that were replaced while it was busy were skipped
        assert!(seen.lock().expect("the list").len() < 21);
    }
}
