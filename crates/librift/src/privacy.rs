//! What the Privacy and security page reads and writes. The camera portal asks the owner before an
//! app takes the camera and keeps each app's answer in the permission store on the session bus,
//! which is where the answers are read and changed. GTK remembers the files opened lately while a
//! dconf key says so. The firewall is a table of nftables rules the image loads at boot, and fwupd
//! says how well the firmware protects the machine.

#[cfg(feature = "bus")]
use std::collections::HashMap;
#[cfg(feature = "bus")]
use std::time::Duration;

#[cfg(feature = "bus")]
use crate::bus;

/// The permission store's name on the session bus, which is also its interface.
#[cfg(feature = "bus")]
const STORE: &str = "org.freedesktop.impl.portal.PermissionStore";
/// Where it answers.
#[cfg(feature = "bus")]
const STORE_PATH: &str = "/org/freedesktop/impl/portal/PermissionStore";
/// The table the camera portal keeps its answers in.
pub const CAMERA_TABLE: &str = "devices";
/// The entry of that table for the camera, one answer per app.
pub const CAMERA_ENTRY: &str = "camera";
/// The dconf key GTK reads to know whether to remember the files opened lately.
pub const RECENT_FILES: &str = "/org/gnome/desktop/privacy/remember-recent-files";
/// The unit that loads the image's nftables rules, the firewall's among them.
pub const FIREWALL_UNIT: &str = "nftables.service";
/// How long the permission store, systemd and fwupd may take. fwupd is started by the question, and
/// looks at the machine before it answers.
#[cfg(feature = "bus")]
const TIMEOUT: Duration = Duration::from_secs(60);

/// What one app was told about the camera.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    /// The app's id, the one the portal knows it by. Empty for a program the portal cannot name,
    /// one started from a terminal.
    pub app: String,
    /// Whether it may take the camera. `None` when it is to be asked again, or the store holds
    /// something the portal does not read.
    pub allowed: Option<bool>,
}

impl Answer {
    /// Read one entry of the table the way the portal reads it: one word, yes, no or ask.
    #[must_use]
    pub fn from_words(app: &str, words: &[String]) -> Self {
        let allowed = match words {
            [word] if word == "yes" => Some(true),
            [word] if word == "no" => Some(false),
            _ => None,
        };
        Self {
            app: app.to_string(),
            allowed,
        }
    }

    /// The word the store keeps for it, and `--state` prints.
    #[must_use]
    pub const fn word(&self) -> &'static str {
        match self.allowed {
            Some(true) => "yes",
            Some(false) => "no",
            None => "ask",
        }
    }
}

/// Every answer the portal keeps about the camera, by app id. None when no app has asked yet.
///
/// # Errors
///
/// A sentence when the session bus or the permission store cannot be reached.
#[cfg(feature = "bus")]
pub fn camera() -> Result<Vec<Answer>, String> {
    let connection = session()?;
    let store = bus::object(&connection, STORE, STORE_PATH, STORE)
        .map_err(|e| bus::sentence_for("The permission store", e))?;
    let answered: zbus::Result<(HashMap<String, Vec<String>>, zbus::zvariant::OwnedValue)> =
        store.call("Lookup", &(CAMERA_TABLE, CAMERA_ENTRY));
    match answered {
        Ok((kept, _)) => {
            let mut answers: Vec<Answer> = kept
                .iter()
                .map(|(app, words)| Answer::from_words(app, words))
                .collect();
            answers.sort_by(|one, other| one.app.cmp(&other.app));
            Ok(answers)
        }
        // the portal makes the entry the first time an app asks
        Err(zbus::Error::MethodError(name, _, _))
            if name.as_str() == "org.freedesktop.portal.Error.NotFound" =>
        {
            Ok(Vec::new())
        }
        Err(e) => Err(bus::sentence_for("The permission store", e)),
    }
}

/// Let an app take the camera without asking, or refuse it without asking, the way the portal
/// keeps an answer the owner gave.
///
/// # Errors
///
/// A sentence when the session bus or the permission store cannot be reached, or the store refused.
#[cfg(feature = "bus")]
pub fn set_camera(app: &str, allowed: bool) -> Result<(), String> {
    let connection = session()?;
    let store = bus::object(&connection, STORE, STORE_PATH, STORE)
        .map_err(|e| bus::sentence_for("The permission store", e))?;
    let word = if allowed { "yes" } else { "no" };
    store
        .call::<_, _, ()>(
            "SetPermission",
            &(CAMERA_TABLE, true, CAMERA_ENTRY, app, vec![word]),
        )
        .map_err(|e| bus::sentence_for("The permission store", e))
}

/// A connection to the owner's session bus.
#[cfg(feature = "bus")]
fn session() -> Result<zbus::blocking::Connection, String> {
    zbus::blocking::connection::Builder::session()
        .map(|builder| builder.method_timeout(TIMEOUT))
        .and_then(zbus::blocking::connection::Builder::build)
        .map_err(|e| format!("Could not reach the session bus: {e}"))
}

/// Whether GTK apps remember the files opened lately. The key is on until something turns it off.
#[must_use]
pub fn recent_files() -> bool {
    std::process::Command::new("dconf")
        .args(["read", RECENT_FILES])
        .output()
        .map_or(true, |read| {
            String::from_utf8_lossy(&read.stdout).trim() != "false"
        })
}

/// Turn remembering the files opened lately on or off, for every GTK app.
///
/// # Errors
///
/// A sentence when dconf could not be run or could not write the key.
pub fn set_recent_files(on: bool) -> Result<(), String> {
    crate::appearance::write_key(RECENT_FILES, if on { "true" } else { "false" })
}

/// Whether the firewall's rules are loaded: the unit that loads them has run and is active.
///
/// # Errors
///
/// A sentence when the system bus or systemd cannot be reached.
#[cfg(feature = "bus")]
pub fn firewall() -> Result<bool, String> {
    const SYSTEMD: &str = "org.freedesktop.systemd1";
    let connection = bus::connect(TIMEOUT)?;
    let manager = bus::object(
        &connection,
        SYSTEMD,
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .map_err(|e| bus::sentence_for("systemd", e))?;
    let unit: zbus::zvariant::OwnedObjectPath = match manager.call("GetUnit", &(FIREWALL_UNIT,)) {
        Ok(path) => path,
        // a unit that is not loaded has loaded nothing
        Err(zbus::Error::MethodError(name, _, _))
            if name.as_str() == "org.freedesktop.systemd1.NoSuchUnit" =>
        {
            return Ok(false);
        }
        Err(e) => return Err(bus::sentence_for("systemd", e)),
    };
    let state: String = bus::object(
        &connection,
        SYSTEMD,
        unit.as_str(),
        "org.freedesktop.systemd1.Unit",
    )
    .and_then(|proxy| proxy.get_property("ActiveState"))
    .map_err(|e| bus::sentence_for("systemd", e))?;
    Ok(state == "active")
}

/// What fwupd says about how well the firmware protects the machine: its Host Security ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Security {
    /// The id as fwupd gives it, `HSI:1 (v2.1.6)`.
    pub id: String,
    /// The level, 0 and up, or `None` when fwupd does not measure this kind of machine.
    pub level: Option<u32>,
    /// Whether it found something wrong with the running system on top of the firmware's settings.
    pub issues: bool,
}

impl Security {
    /// Read an id: `HSI:<level>`, a `!` after it when there are runtime issues, then the version in
    /// brackets. `HSI:INVALID:chassis[other]` is a machine fwupd does not measure.
    #[must_use]
    pub fn from_id(id: &str) -> Self {
        let id = id.trim();
        let bare = id.split(" (").next().unwrap_or(id);
        let rest = bare.strip_prefix("HSI:").unwrap_or("");
        let issues = rest.ends_with('!');
        let level = rest.trim_end_matches('!').parse::<u32>().ok();
        Self {
            id: id.to_string(),
            level,
            issues: issues && level.is_some(),
        }
    }

    /// The level in words, as GNOME Settings says it: checks failed, checks passed, protected.
    #[must_use]
    pub const fn words(&self) -> &'static str {
        match self.level {
            None => "Not measured",
            Some(0) => "Checks failed",
            Some(1) => "Checks passed",
            Some(_) => "Protected",
        }
    }
}

/// Ask fwupd how well the firmware protects the machine. fwupd starts for the question.
///
/// # Errors
///
/// A sentence when the system bus or fwupd cannot be reached.
#[cfg(feature = "bus")]
pub fn security() -> Result<Security, String> {
    let connection = bus::connect(TIMEOUT)?;
    let id: String = bus::object(
        &connection,
        "org.freedesktop.fwupd",
        "/",
        "org.freedesktop.fwupd",
    )
    .and_then(|proxy| proxy.get_property("HostSecurityId"))
    .map_err(|e| bus::sentence_for("fwupd", e))?;
    Ok(Security::from_id(&id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_is_one_word_the_portal_reads() {
        let words = |list: &[&str]| {
            list.iter()
                .map(|&word| word.to_string())
                .collect::<Vec<_>>()
        };
        let yes = Answer::from_words("org.gnome.Snapshot", &words(&["yes"]));
        assert_eq!((yes.allowed, yes.word()), (Some(true), "yes"));
        let no = Answer::from_words("org.gnome.Snapshot", &words(&["no"]));
        assert_eq!((no.allowed, no.word()), (Some(false), "no"));
        // ask, and anything the portal would ignore, mean the app is asked again
        for odd in [&["ask"][..], &["yes", "no"], &[], &["maybe"]] {
            assert_eq!(
                Answer::from_words("x", &words(odd)).allowed,
                None,
                "{odd:?}"
            );
        }
    }

    #[test]
    fn a_security_id_says_its_level() {
        let one = Security::from_id("HSI:1 (v2.1.6)");
        assert_eq!(
            (one.level, one.issues, one.words()),
            (Some(1), false, "Checks passed")
        );
        let failed = Security::from_id("HSI:0! (v2.1.6)");
        assert_eq!(
            (failed.level, failed.issues, failed.words()),
            (Some(0), true, "Checks failed")
        );
        assert_eq!(Security::from_id("HSI:3").words(), "Protected");
        // a virtual machine says what kind of machine it is instead of a level
        let vm = Security::from_id("HSI:INVALID:chassis[other] (v2.1.6)");
        assert_eq!(
            (vm.level, vm.issues, vm.words()),
            (None, false, "Not measured")
        );
        assert_eq!(vm.id, "HSI:INVALID:chassis[other] (v2.1.6)");
        assert_eq!(Security::from_id("").level, None);
    }
}
