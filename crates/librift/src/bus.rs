//! The system bus from a client's side. [`reason`] turns an error reply into the sentence a person
//! reads. With the `bus` feature this module also opens the connection and the proxies that
//! [`crate::quasar`] and [`crate::orbit`] ask through, the ones for the services Rift did not
//! write (`NetworkManager`, `BlueZ`, `UPower`, logind), and the signal watches that tell the shell one of
//! those services has something new to say.

#[cfg(feature = "bus")]
use std::sync::mpsc::Sender;
#[cfg(feature = "bus")]
use std::time::Duration;

#[cfg(feature = "bus")]
use zbus::DBusError as _;

use crate::Component;

/// How long reading a property may take. The services answer those from memory.
#[cfg(feature = "bus")]
pub(crate) const PROPERTY_TIMEOUT: Duration = Duration::from_secs(10);

/// The sentence for an error reply from `component`, from the D-Bus error name and the message
/// that came with it.
#[must_use]
pub fn reason(component: Component, error: &str, detail: Option<&str>) -> String {
    said(component.display_name(), error, detail)
}

/// The same sentence for a service by the name a person knows it by, `NetworkManager` or `BlueZ`.
#[must_use]
pub fn said(name: &str, error: &str, detail: Option<&str>) -> String {
    match error {
        "org.freedesktop.DBus.Error.ServiceUnknown"
        | "org.freedesktop.DBus.Error.NameHasNoOwner" => format!("{name} is not running."),
        "org.freedesktop.DBus.Error.NoReply" | "org.freedesktop.DBus.Error.Timeout" => {
            format!("{name} took too long to answer.")
        }
        _ => detail
            .map(str::trim)
            .filter(|detail| !detail.is_empty())
            .map_or_else(|| format!("{name} could not answer."), ToString::to_string),
    }
}

/// A connection to the system bus whose method calls give up after `timeout`.
#[cfg(feature = "bus")]
pub(crate) fn connect(timeout: Duration) -> Result<zbus::blocking::Connection, String> {
    zbus::blocking::connection::Builder::system()
        .map(|builder| builder.method_timeout(timeout))
        .and_then(zbus::blocking::connection::Builder::build)
        .map_err(|e| format!("Could not reach the system bus: {e}"))
}

/// A proxy for the component's own interface. Properties are read fresh every time, never from
/// a cache that could be out of date.
#[cfg(feature = "bus")]
pub(crate) fn proxy(
    connection: &zbus::blocking::Connection,
    component: Component,
) -> Result<zbus::blocking::Proxy<'static>, String> {
    build(connection, component).map_err(|e| sentence(component, e))
}

#[cfg(feature = "bus")]
fn build(
    connection: &zbus::blocking::Connection,
    component: Component,
) -> zbus::Result<zbus::blocking::Proxy<'static>> {
    let name = component.dbus_name();
    zbus::blocking::proxy::Builder::new(connection)
        .destination(name.clone())?
        .path(component.dbus_path())?
        .interface(name)?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
}

/// The sentence for anything that went wrong while talking to `component`.
#[cfg(feature = "bus")]
pub(crate) fn sentence(component: Component, error: zbus::Error) -> String {
    sentence_for(component.display_name(), error)
}

/// The sentence for anything that went wrong while talking to the service a person knows as `name`.
#[cfg(feature = "bus")]
pub(crate) fn sentence_for(name: &str, error: zbus::Error) -> String {
    match error {
        zbus::Error::MethodError(error, detail, _) => said(name, error.as_str(), detail.as_deref()),
        zbus::Error::FDO(error) => said(name, error.name().as_str(), error.description()),
        zbus::Error::InputOutput(error) if error.kind() == std::io::ErrorKind::TimedOut => {
            format!("{name} took too long to answer.")
        }
        other => format!("Could not talk to {name}: {other}"),
    }
}

/// A proxy for one interface of one object of a service Rift did not write. Nothing is cached, so
/// a property is read fresh every time.
#[cfg(feature = "bus")]
pub(crate) fn object(
    connection: &zbus::blocking::Connection,
    service: &'static str,
    path: &str,
    interface: &'static str,
) -> zbus::Result<zbus::blocking::Proxy<'static>> {
    zbus::blocking::proxy::Builder::new(connection)
        .destination(service)?
        .path(path.to_string())?
        .interface(interface)?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
}

/// Every property of one interface of one object, in one call.
#[cfg(feature = "bus")]
pub(crate) fn properties(
    connection: &zbus::blocking::Connection,
    service: &'static str,
    path: &str,
    interface: &'static str,
) -> zbus::Result<std::collections::HashMap<String, zbus::zvariant::OwnedValue>> {
    object(connection, service, path, "org.freedesktop.DBus.Properties")?
        .call("GetAll", &(interface,))
}

/// Whether a service is running now. Asking this first keeps a call from starting a service that
/// has nothing to do on this machine, like `BlueZ` with no adapter.
#[cfg(feature = "bus")]
pub(crate) fn running(connection: &zbus::blocking::Connection, service: &str) -> bool {
    zbus::blocking::fdo::DBusProxy::new(connection)
        .ok()
        .zip(zbus::names::BusName::try_from(service).ok())
        .is_some_and(|(proxy, name)| proxy.name_has_owner(name).unwrap_or(false))
}

/// Call `each` for every signal `service` sends, until `each` says it wants no more. Blocks, so the
/// caller runs it on a thread of its own. The bus delivers a well-known name's signals from
/// whoever owns the name at the time, so a service that restarts is followed without asking again.
///
/// # Errors
///
/// When the bus cannot be reached or closes the connection.
#[cfg(feature = "bus")]
pub fn signals<F: FnMut() -> bool>(service: &str, each: F) -> Result<(), String> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(service)
        .map(zbus::match_rule::Builder::build);
    watch(rule, each)
}

/// Call `each` whenever `service` starts or stops, until `each` says it wants no more.
///
/// # Errors
///
/// When the bus cannot be reached or closes the connection.
#[cfg(feature = "bus")]
pub fn owner_changes<F: FnMut() -> bool>(service: &str, each: F) -> Result<(), String> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.DBus")
        .and_then(|builder| builder.interface("org.freedesktop.DBus"))
        .and_then(|builder| builder.member("NameOwnerChanged"))
        .and_then(|builder| builder.add_arg(service))
        .map(zbus::match_rule::Builder::build);
    watch(rule, each)
}

/// How long a burst of signals has to be quiet before the source is read again.
#[cfg(feature = "bus")]
pub const SETTLE: Duration = Duration::from_millis(150);
/// The longest a reading waits for a burst to end, so a scan that never stops still shows.
#[cfg(feature = "bus")]
const LONGEST: Duration = Duration::from_secs(1);
/// How long to wait before listening again when the bus went away.
#[cfg(feature = "bus")]
const RETRY: Duration = Duration::from_secs(5);

/// Read a source now, and again after every change `start` pokes about, on a thread of its own.
/// Every reading goes to `send`, which says whether anyone is still listening; the thread stops
/// when nobody is. A burst of signals, which a scan or a service starting sends, turns into one
/// reading once it has settled. The shell follows its status sources this way and Settings its
/// pages.
#[cfg(feature = "bus")]
pub fn follow<T, R, S>(start: impl FnOnce(Sender<()>) + Send + 'static, read: R, mut send: S)
where
    T: Send + 'static,
    R: Fn() -> T + Send + 'static,
    S: FnMut(T) -> bool + Send + 'static,
{
    std::thread::spawn(move || {
        let (poke, pokes) = std::sync::mpsc::channel();
        start(poke);
        loop {
            if !send(read()) || pokes.recv().is_err() {
                return;
            }
            settle(&pokes);
        }
    });
}

/// Wait until the pokes stop for a moment, or a second has gone by.
#[cfg(feature = "bus")]
fn settle(pokes: &std::sync::mpsc::Receiver<()>) {
    let until = std::time::Instant::now() + LONGEST;
    loop {
        let left = until.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return;
        }
        if pokes.recv_timeout(SETTLE.min(left)).is_err() {
            return;
        }
    }
}

/// Run a signal watch on a thread of its own and poke for every signal. A bus that failed is
/// listened to again after a while, with a poke, since whatever happened in between went unseen.
/// `said` is called with the sentence for a failure, which the caller prints in its own words.
#[cfg(feature = "bus")]
pub fn listen<W, S>(poke: Sender<()>, watch: W, said: S)
where
    W: Fn(&mut dyn FnMut() -> bool) -> Result<(), String> + Send + 'static,
    S: Fn(&str) + Send + 'static,
{
    std::thread::spawn(move || {
        loop {
            let mut each = || poke.send(()).is_ok();
            match watch(&mut each) {
                Ok(()) => return,
                Err(why) => said(&why),
            }
            std::thread::sleep(RETRY);
            if poke.send(()).is_err() {
                return;
            }
        }
    });
}

#[cfg(feature = "bus")]
fn watch<F: FnMut() -> bool>(
    rule: zbus::Result<zbus::MatchRule<'_>>,
    mut each: F,
) -> Result<(), String> {
    let rule = rule.map_err(|e| format!("Could not make a match rule: {e}"))?;
    let connection = zbus::blocking::Connection::system()
        .map_err(|e| format!("Could not reach the system bus: {e}"))?;
    // a burst of signals only has to wake the watcher once, so a short queue is plenty
    let messages = zbus::blocking::MessageIterator::for_match_rule(rule, &connection, Some(64))
        .map_err(|e| format!("Could not listen on the system bus: {e}"))?;
    for message in messages {
        message.map_err(|e| format!("The system bus failed: {e}"))?;
        if !each() {
            return Ok(());
        }
    }
    Err("The system bus closed the connection.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "bus")]
    #[test]
    fn a_burst_of_pokes_settles_into_one_reading() {
        let (poke, pokes) = std::sync::mpsc::channel();
        for _ in 0..50 {
            poke.send(()).expect("the channel");
        }
        let started = std::time::Instant::now();
        settle(&pokes);
        // everything queued is taken in, then the quiet ends the wait
        assert!(pokes.try_recv().is_err());
        assert!(started.elapsed() >= SETTLE);
        assert!(started.elapsed() < LONGEST + SETTLE);
    }

    #[cfg(feature = "bus")]
    #[test]
    fn pokes_that_never_stop_still_end_the_wait() {
        let (poke, pokes) = std::sync::mpsc::channel();
        let keep = std::thread::spawn(move || {
            while poke.send(()).is_ok() {
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        let started = std::time::Instant::now();
        settle(&pokes);
        assert!(started.elapsed() >= LONGEST);
        assert!(started.elapsed() < LONGEST * 2);
        drop(pokes);
        keep.join().expect("the poking thread");
    }

    #[test]
    fn errors_on_the_bus_read_as_sentences() {
        assert_eq!(
            reason(
                Component::Quasar,
                "org.freedesktop.DBus.Error.ServiceUnknown",
                None
            ),
            "Quasar is not running."
        );
        assert_eq!(
            reason(
                Component::Orbit,
                "org.freedesktop.DBus.Error.NameHasNoOwner",
                Some("The name is not activatable")
            ),
            "Orbit is not running."
        );
        assert_eq!(
            reason(
                Component::Quasar,
                "org.freedesktop.DBus.Error.NoReply",
                Some("Did not receive a reply")
            ),
            "Quasar took too long to answer."
        );
        assert_eq!(
            reason(
                Component::Quasar,
                "org.freedesktop.DBus.Error.Failed",
                Some("The model is still loading. Try again in a moment.")
            ),
            "The model is still loading. Try again in a moment."
        );
        assert_eq!(
            reason(
                Component::Quasar,
                "org.freedesktop.DBus.Error.Failed",
                Some(" ")
            ),
            "Quasar could not answer."
        );
    }
}
