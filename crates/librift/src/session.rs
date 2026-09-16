//! The owner's session from a client's side. Locking goes through logind, the way `loginctl
//! lock-session` does: logind signals the session and the lock screen that listens for it comes
//! up, so the lock is the same whether a key, a command or a menu asked for it.

/// Lock the session the owner is using. Asked from a process outside any session, like a user unit,
/// logind takes the owner's graphical session.
///
/// # Errors
///
/// A sentence when logind does not know the session or refuses.
#[cfg(feature = "bus")]
pub fn lock() -> Result<(), String> {
    use zbus::zvariant::OwnedObjectPath;

    use crate::bus;

    const SERVICE: &str = "org.freedesktop.login1";
    let failed = |e| bus::sentence_for("logind", e);
    let connection = bus::connect(bus::PROPERTY_TIMEOUT)?;
    let session: OwnedObjectPath = bus::object(
        &connection,
        SERVICE,
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
    .and_then(|manager| manager.call("GetSession", &("auto",)))
    .map_err(failed)?;
    bus::object(
        &connection,
        SERVICE,
        session.as_str(),
        "org.freedesktop.login1.Session",
    )
    .and_then(|proxy| proxy.call::<_, _, ()>("Lock", &()))
    .map_err(failed)
}
