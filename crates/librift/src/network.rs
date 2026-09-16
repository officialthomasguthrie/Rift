//! `NetworkManager` from a client's side: what the system menu shows about the cable and the wireless
//! networks around, and what it asks `NetworkManager` to do when the owner turns Wi-Fi on or off or
//! picks a network. The picture is read off the system bus and built here from plain values, so the
//! rules for which device counts, how a network is secured and in what order the networks come are
//! tested without a bus.

#[cfg(feature = "bus")]
use std::collections::HashMap;
#[cfg(feature = "bus")]
use std::time::{Duration, Instant};

#[cfg(feature = "bus")]
use zbus::zvariant::{Array, ObjectPath, OwnedObjectPath, OwnedValue, Value};

#[cfg(feature = "bus")]
use crate::bus;

/// `NetworkManager`'s name on the system bus.
pub const SERVICE: &str = "org.freedesktop.NetworkManager";

/// The name a person knows it by, for the sentences.
#[cfg(feature = "bus")]
const NAME: &str = "NetworkManager";

/// `DeviceType` of a cable.
const ETHERNET: u32 = 1;
/// `DeviceType` of a wireless card.
const WIFI: u32 = 2;

/// The state of a device, the way the menu says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// Up and in use.
    Connected,
    /// On its way up: preparing, configuring, waiting for a password or an address.
    Connecting,
    /// Could be used and is not.
    Disconnected,
    /// Cannot be used now: no cable in it, or the radio is off.
    Unavailable,
}

impl Link {
    /// From a device's `State` property.
    #[must_use]
    pub const fn from_state(state: u32) -> Self {
        match state {
            100 => Self::Connected,
            40..=90 => Self::Connecting,
            // unknown, unmanaged and unavailable
            0..=20 => Self::Unavailable,
            // disconnected, deactivating and failed
            _ => Self::Disconnected,
        }
    }

    /// How much it says about the machine being online, for picking the device that matters.
    const fn rank(self) -> u8 {
        match self {
            Self::Connected => 0,
            Self::Connecting => 1,
            Self::Disconnected => 2,
            Self::Unavailable => 3,
        }
    }
}

/// How a network is secured, which decides what joining it asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Security {
    /// Anyone can join.
    Open,
    /// Anyone can join, and the traffic is encrypted all the same (OWE).
    Enhanced,
    /// A password: WPA and WPA2 personal, and WPA3 networks that still take WPA2.
    Password,
    /// A password, on a WPA3 network that takes nothing else (SAE).
    Sae,
    /// An old WEP key.
    Wep,
    /// A user name as well as a password or a certificate: WPA enterprise.
    Enterprise,
}

impl Security {
    /// From an access point's `Flags`, `WpaFlags` and `RsnFlags`.
    #[must_use]
    pub const fn from_flags(flags: u32, wpa: u32, rsn: u32) -> Self {
        // NM80211ApSecurityFlags: the key management bits
        const PSK: u32 = 0x100;
        const EAP: u32 = 0x200;
        const SAE: u32 = 0x400;
        const OWE: u32 = 0x800 | 0x1000;
        const SUITE_B: u32 = 0x2000;
        // NM80211ApFlags: the network asks for a key of some kind
        const PRIVACY: u32 = 0x1;
        let both = wpa | rsn;
        if both & (EAP | SUITE_B) != 0 {
            Self::Enterprise
        } else if both & PSK != 0 {
            Self::Password
        } else if both & SAE != 0 {
            Self::Sae
        } else if both & OWE != 0 {
            Self::Enhanced
        } else if flags & PRIVACY != 0 {
            Self::Wep
        } else {
            Self::Open
        }
    }

    /// Whether joining it asks the owner for something.
    #[must_use]
    pub const fn secured(self) -> bool {
        !matches!(self, Self::Open | Self::Enhanced)
    }

    /// Whether the menu can ask for what joining it needs: a password in a dialog, or nothing.
    #[must_use]
    pub const fn joinable(self) -> bool {
        !matches!(self, Self::Enterprise)
    }

    /// The shortest password it takes, in characters.
    #[must_use]
    pub const fn shortest(self) -> usize {
        match self {
            Self::Open | Self::Enhanced => 0,
            Self::Password | Self::Sae => 8,
            Self::Wep | Self::Enterprise => 1,
        }
    }
}

/// One access point, as `NetworkManager` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessPoint {
    /// Its object on the bus, which a join names.
    pub path: String,
    /// The network's name as the radio sends it. Empty for a hidden network.
    pub ssid: Vec<u8>,
    /// Out of a hundred.
    pub strength: u8,
    /// How it is secured.
    pub security: Security,
}

/// One network the menu lists: every access point that sends the same name is one network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Network {
    /// Its name, as the menu writes it.
    pub name: String,
    /// Its name as the radio sends it, which a new connection is made for.
    pub ssid: Vec<u8>,
    /// The strongest of its access points, out of a hundred.
    pub strength: u8,
    /// How that access point is secured.
    pub security: Security,
    /// That access point's object, which a join names.
    pub point: String,
    /// Whether the machine is on this network now.
    pub active: bool,
    /// The saved connection for it, when it has been joined before.
    pub saved: Option<String>,
}

/// A device as `NetworkManager` lists it, before the picture is made from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// Its object on the bus.
    pub path: String,
    /// `DeviceType`: 1 a cable, 2 a wireless card, and many more the menu does not show.
    pub kind: u32,
    /// `State`.
    pub state: u32,
    /// Whether `NetworkManager` looks after it. One it does not is someone else's business.
    pub managed: bool,
    /// For a wireless card, the access point it is on.
    pub active: Option<String>,
    /// For a wireless card, every access point it sees.
    pub points: Vec<AccessPoint>,
}

/// The cable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wired {
    /// The device's object on the bus.
    pub path: String,
    /// Its state.
    pub link: Link,
}

/// The wireless card and the networks around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wireless {
    /// The device's object on the bus, which a scan and a join name.
    pub path: String,
    /// Its state.
    pub link: Link,
    /// The networks it sees, the one in use first, then the ones joined before, then the
    /// strongest.
    pub networks: Vec<Network>,
}

impl Wireless {
    /// The network the machine is on, when it is on one.
    #[must_use]
    pub fn active(&self) -> Option<&Network> {
        self.networks.iter().find(|network| network.active)
    }
}

/// Everything the menu and the bar show about the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    /// Wi-Fi is switched on, as `NetworkManager` keeps it.
    pub wifi: bool,
    /// No switch on the machine and nothing in the firmware keeps the radio off.
    pub radio: bool,
    /// The cable, when the machine has a port for one.
    pub wired: Option<Wired>,
    /// The wireless card, when the machine has one.
    pub wireless: Option<Wireless>,
}

/// The picture from what `NetworkManager` listed. When the machine has more than one device of a
/// kind, the one that is most online is the one that counts.
#[must_use]
pub fn picture(
    wifi: bool,
    radio: bool,
    devices: &[Device],
    saved: &[(Vec<u8>, String)],
) -> Picture {
    let pick = |kind: u32| {
        devices
            .iter()
            .filter(|device| device.kind == kind && device.managed)
            .min_by_key(|device| Link::from_state(device.state).rank())
    };
    Picture {
        wifi,
        radio,
        wired: pick(ETHERNET).map(|device| Wired {
            path: device.path.clone(),
            link: Link::from_state(device.state),
        }),
        wireless: pick(WIFI).map(|device| Wireless {
            path: device.path.clone(),
            link: Link::from_state(device.state),
            networks: networks(&device.points, device.active.as_deref(), saved),
        }),
    }
}

/// The networks a card sees: one per name, from its strongest access point, the one in use first,
/// then the ones joined before, then by strength. A hidden network has no name to show, so it is
/// left out.
#[must_use]
pub fn networks(
    points: &[AccessPoint],
    active: Option<&str>,
    saved: &[(Vec<u8>, String)],
) -> Vec<Network> {
    let mut found: Vec<Network> = Vec::new();
    for point in points.iter().filter(|point| !point.ssid.is_empty()) {
        let in_use = active == Some(point.path.as_str());
        if let Some(network) = found.iter_mut().find(|network| network.ssid == point.ssid) {
            network.active |= in_use;
            if point.strength > network.strength {
                network.strength = point.strength;
                network.security = point.security;
                network.point.clone_from(&point.path);
            }
            continue;
        }
        found.push(Network {
            name: name(&point.ssid),
            ssid: point.ssid.clone(),
            strength: point.strength,
            security: point.security,
            point: point.path.clone(),
            active: in_use,
            saved: saved
                .iter()
                .find(|(ssid, _)| *ssid == point.ssid)
                .map(|(_, path)| path.clone()),
        });
    }
    found.sort_by(|one, other| {
        other
            .active
            .cmp(&one.active)
            .then(other.saved.is_some().cmp(&one.saved.is_some()))
            .then(other.strength.cmp(&one.strength))
            .then(one.name.cmp(&other.name))
    });
    found
}

/// A network's name as text. The radio sends bytes, nearly always UTF-8; whatever is not, and any
/// control character, is not drawn.
#[must_use]
pub fn name(ssid: &[u8]) -> String {
    String::from_utf8_lossy(ssid)
        .chars()
        .filter(|character| !character.is_control())
        .collect()
}

/// What `NetworkManager` says now.
///
/// # Errors
///
/// A sentence when `NetworkManager` is not running or does not answer.
#[cfg(feature = "bus")]
pub fn read() -> Result<Picture, String> {
    let connection = bus::connect(bus::PROPERTY_TIMEOUT)?;
    let failed = |e| bus::sentence_for(NAME, e);
    let mut manager = bus::properties(&connection, SERVICE, ROOT, MANAGER).map_err(failed)?;
    let wifi = take::<bool>(&mut manager, "WirelessEnabled").unwrap_or(false);
    let radio = take::<bool>(&mut manager, "WirelessHardwareEnabled").unwrap_or(true);
    let mut devices = Vec::new();
    for path in paths(&mut manager, "Devices") {
        // a device that went away between the two calls is not an error
        if let Ok(device) = device(&connection, &path) {
            devices.push(device);
        }
    }
    Ok(picture(wifi, radio, &devices, &saved(&connection)))
}

/// Turn Wi-Fi on or off.
///
/// # Errors
///
/// A sentence when `NetworkManager` refuses or is not there.
#[cfg(feature = "bus")]
pub fn set_wifi(on: bool) -> Result<(), String> {
    let connection = bus::connect(bus::PROPERTY_TIMEOUT)?;
    bus::object(&connection, SERVICE, ROOT, MANAGER)
        .map_err(|e| bus::sentence_for(NAME, e))?
        .set_property("WirelessEnabled", on)
        .map_err(|e| bus::sentence_for(NAME, e.into()))
}

/// Ask the card to look for networks again, so the list is fresh when the menu opens. The card
/// refuses when it looked a moment ago, which is not worth saying.
#[cfg(feature = "bus")]
pub fn scan(device: &str) {
    let Ok(connection) = bus::connect(bus::PROPERTY_TIMEOUT) else {
        return;
    };
    if let Ok(proxy) = bus::object(&connection, SERVICE, device, WIRELESS) {
        let options: HashMap<&str, Value<'_>> = HashMap::new();
        let _: zbus::Result<()> = proxy.call("RequestScan", &(options,));
    }
}

/// A join that `NetworkManager` has started.
#[cfg(feature = "bus")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Joining {
    /// The active connection it is bringing up.
    pub active: String,
    /// The connection made for this join, when the network had none. It is deleted again when the
    /// join fails, so a wrong password is asked for again instead of being tried forever.
    pub made: Option<String>,
}

/// Start joining a network: the saved connection when it has one, or a new one with the password
/// it needs. [`wait`] says how it went.
///
/// # Errors
///
/// A sentence when the network cannot be joined from here or `NetworkManager` refuses.
#[cfg(feature = "bus")]
pub fn join(device: &str, network: &Network, password: Option<&str>) -> Result<Joining, String> {
    let connection = bus::connect(bus::PROPERTY_TIMEOUT)?;
    let failed = |e| bus::sentence_for(NAME, e);
    let manager = bus::object(&connection, SERVICE, ROOT, MANAGER).map_err(failed)?;
    let device_path = ObjectPath::try_from(device).map_err(|e| failed(e.into()))?;
    let point = ObjectPath::try_from(network.point.as_str()).map_err(|e| failed(e.into()))?;
    if let Some(saved) = &network.saved {
        let saved = ObjectPath::try_from(saved.as_str()).map_err(|e| failed(e.into()))?;
        let active: OwnedObjectPath = manager
            .call("ActivateConnection", &(saved, device_path, point))
            .map_err(failed)?;
        return Ok(Joining {
            active: active.to_string(),
            made: None,
        });
    }
    let settings = settings(network, password)?;
    let (made, active): (OwnedObjectPath, OwnedObjectPath) = manager
        .call("AddAndActivateConnection", &(settings, device_path, point))
        .map_err(failed)?;
    Ok(Joining {
        active: active.to_string(),
        made: Some(made.to_string()),
    })
}

/// What a new connection to this network holds. `NetworkManager` fills in the rest from the access
/// point.
#[cfg(feature = "bus")]
fn settings<'a>(
    network: &Network,
    password: Option<&'a str>,
) -> Result<HashMap<&'static str, HashMap<&'static str, Value<'a>>>, String> {
    let password = password.unwrap_or_default();
    let mut security: HashMap<&'static str, Value<'a>> = HashMap::new();
    match network.security {
        Security::Open => {}
        Security::Enhanced => {
            security.insert("key-mgmt", Value::from("owe"));
        }
        Security::Password => {
            security.insert("key-mgmt", Value::from("wpa-psk"));
            security.insert("psk", Value::from(password));
        }
        Security::Sae => {
            security.insert("key-mgmt", Value::from("sae"));
            security.insert("psk", Value::from(password));
        }
        Security::Wep => {
            security.insert("key-mgmt", Value::from("none"));
            security.insert("wep-key0", Value::from(password));
            security.insert("wep-key-type", Value::from(wep_kind(password)));
        }
        Security::Enterprise => {
            return Err(format!(
                "{} asks for a user name, and joining a network like that is not supported yet.",
                network.name
            ));
        }
    }
    let mut settings = HashMap::new();
    if !security.is_empty() {
        settings.insert("802-11-wireless-security", security);
    }
    Ok(settings)
}

/// Whether a WEP password is the key itself (1) or a passphrase the key is made from (2): a key is
/// 5 or 13 characters, or 10 or 26 hex digits.
#[must_use]
pub fn wep_kind(password: &str) -> u32 {
    let hex = password
        .chars()
        .all(|character| character.is_ascii_hexdigit());
    match password.len() {
        5 | 13 => 1,
        10 | 26 if hex => 1,
        _ => 2,
    }
}

/// Wait until a join is up, or has failed. A connection made for a join that failed is deleted.
///
/// # Errors
///
/// A sentence when the network did not come up within `timeout`.
#[cfg(feature = "bus")]
pub fn wait(joining: &Joining, timeout: Duration) -> Result<(), String> {
    let connection = bus::connect(bus::PROPERTY_TIMEOUT)?;
    let until = Instant::now() + timeout;
    let outcome = loop {
        // 1 activating, 2 activated, 3 deactivating, 4 deactivated. once the connection is down
        // its object goes away, and reading it fails
        let state = bus::object(&connection, SERVICE, &joining.active, ACTIVE)
            .and_then(|proxy| proxy.get_property::<u32>("State"));
        match state {
            Ok(2) => break Ok(()),
            Ok(0 | 1) if Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(250));
            }
            _ => break Err(()),
        }
    };
    if outcome.is_err() {
        if let Some(made) = &joining.made {
            if let Ok(proxy) = bus::object(&connection, SERVICE, made, SAVED) {
                let _: zbus::Result<()> = proxy.call("Delete", &());
            }
        }
    }
    outcome.map_err(|()| "The network did not come up.".to_string())
}

#[cfg(feature = "bus")]
const ROOT: &str = "/org/freedesktop/NetworkManager";
#[cfg(feature = "bus")]
const SETTINGS: &str = "/org/freedesktop/NetworkManager/Settings";
#[cfg(feature = "bus")]
const MANAGER: &str = "org.freedesktop.NetworkManager";
#[cfg(feature = "bus")]
const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
#[cfg(feature = "bus")]
const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
#[cfg(feature = "bus")]
const POINT: &str = "org.freedesktop.NetworkManager.AccessPoint";
#[cfg(feature = "bus")]
const ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";
#[cfg(feature = "bus")]
const SAVED: &str = "org.freedesktop.NetworkManager.Settings.Connection";
#[cfg(feature = "bus")]
const SAVED_LIST: &str = "org.freedesktop.NetworkManager.Settings";

/// One device and, for a wireless card, the access points it sees.
#[cfg(feature = "bus")]
fn device(connection: &zbus::blocking::Connection, path: &str) -> zbus::Result<Device> {
    let mut properties = bus::properties(connection, SERVICE, path, DEVICE)?;
    let kind = take::<u32>(&mut properties, "DeviceType").unwrap_or(0);
    let mut device = Device {
        path: path.to_string(),
        kind,
        state: take::<u32>(&mut properties, "State").unwrap_or(0),
        managed: take::<bool>(&mut properties, "Managed").unwrap_or(false),
        active: None,
        points: Vec::new(),
    };
    if kind == WIFI && device.managed {
        let mut wireless = bus::properties(connection, SERVICE, path, WIRELESS)?;
        device.active = take::<OwnedObjectPath>(&mut wireless, "ActiveAccessPoint")
            .map(|point| point.to_string())
            .filter(|point| point != "/");
        for point in paths(&mut wireless, "AccessPoints") {
            if let Ok(mut found) = bus::properties(connection, SERVICE, &point, POINT) {
                device.points.push(AccessPoint {
                    ssid: bytes(&mut found, "Ssid"),
                    strength: take::<u8>(&mut found, "Strength").unwrap_or(0),
                    security: Security::from_flags(
                        take::<u32>(&mut found, "Flags").unwrap_or(0),
                        take::<u32>(&mut found, "WpaFlags").unwrap_or(0),
                        take::<u32>(&mut found, "RsnFlags").unwrap_or(0),
                    ),
                    path: point,
                });
            }
        }
    }
    Ok(device)
}

/// The saved wireless connections, by the name of their network.
#[cfg(feature = "bus")]
fn saved(connection: &zbus::blocking::Connection) -> Vec<(Vec<u8>, String)> {
    let Ok(list) = bus::object(connection, SERVICE, SETTINGS, SAVED_LIST) else {
        return Vec::new();
    };
    let Ok(paths) = list.call::<_, _, Vec<OwnedObjectPath>>("ListConnections", &()) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for path in paths {
        let Ok(proxy) = bus::object(connection, SERVICE, path.as_str(), SAVED) else {
            continue;
        };
        let Ok(mut settings) =
            proxy.call::<_, _, HashMap<String, HashMap<String, OwnedValue>>>("GetSettings", &())
        else {
            continue;
        };
        if let Some(mut wireless) = settings.remove("802-11-wireless") {
            let ssid = bytes(&mut wireless, "ssid");
            if !ssid.is_empty() {
                found.push((ssid, path.to_string()));
            }
        }
    }
    found
}

/// A property of one type, taken out of what `GetAll` returned.
#[cfg(feature = "bus")]
fn take<T: TryFrom<OwnedValue>>(
    properties: &mut HashMap<String, OwnedValue>,
    key: &str,
) -> Option<T> {
    properties
        .remove(key)
        .and_then(|value| T::try_from(value).ok())
}

/// A property that is a list of objects.
#[cfg(feature = "bus")]
fn paths(properties: &mut HashMap<String, OwnedValue>, key: &str) -> Vec<String> {
    take::<Array<'static>>(properties, key)
        .and_then(|array| Vec::<OwnedObjectPath>::try_from(array).ok())
        .map(|paths| paths.iter().map(ToString::to_string).collect())
        .unwrap_or_default()
}

/// A property that is a string of bytes.
#[cfg(feature = "bus")]
fn bytes(properties: &mut HashMap<String, OwnedValue>, key: &str) -> Vec<u8> {
    take::<Array<'static>>(properties, key)
        .and_then(|array| Vec::<u8>::try_from(array).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(path: &str, ssid: &str, strength: u8, security: Security) -> AccessPoint {
        AccessPoint {
            path: path.to_string(),
            ssid: ssid.as_bytes().to_vec(),
            strength,
            security,
        }
    }

    #[test]
    fn the_state_of_a_device_reads_as_a_link() {
        assert_eq!(Link::from_state(100), Link::Connected);
        assert_eq!(Link::from_state(70), Link::Connecting);
        assert_eq!(Link::from_state(60), Link::Connecting);
        assert_eq!(Link::from_state(30), Link::Disconnected);
        assert_eq!(Link::from_state(120), Link::Disconnected);
        assert_eq!(Link::from_state(20), Link::Unavailable);
        assert_eq!(Link::from_state(0), Link::Unavailable);
    }

    #[test]
    fn the_flags_say_how_a_network_is_secured() {
        assert_eq!(Security::from_flags(0, 0, 0), Security::Open);
        // WPA2 personal, CCMP
        assert_eq!(Security::from_flags(1, 0, 0x188), Security::Password);
        // WPA3 in transition mode takes a WPA2 password
        assert_eq!(Security::from_flags(1, 0, 0x588), Security::Password);
        assert_eq!(Security::from_flags(1, 0, 0x488), Security::Sae);
        assert_eq!(Security::from_flags(1, 0x288, 0x288), Security::Enterprise);
        assert_eq!(Security::from_flags(0, 0, 0x888), Security::Enhanced);
        assert_eq!(Security::from_flags(1, 0, 0), Security::Wep);
        assert!(!Security::Open.secured());
        assert!(!Security::Enhanced.secured());
        assert!(Security::Wep.secured());
        assert!(!Security::Enterprise.joinable());
        assert_eq!(Security::Password.shortest(), 8);
    }

    #[test]
    fn one_network_per_name_from_its_strongest_access_point() {
        let points = [
            point("/ap/1", "Home", 40, Security::Password),
            point("/ap/2", "Cafe", 90, Security::Open),
            point("/ap/3", "Home", 75, Security::Password),
            point("/ap/4", "", 99, Security::Open),
            point("/ap/5", "Library", 60, Security::Password),
        ];
        let saved = [(b"Library".to_vec(), "/settings/7".to_string())];
        let found = networks(&points, Some("/ap/1"), &saved);
        let names: Vec<&str> = found.iter().map(|network| network.name.as_str()).collect();
        // the one in use, the one joined before, then the strongest; the hidden one is not listed
        assert_eq!(names, ["Home", "Library", "Cafe"]);
        assert!(found[0].active, "the weaker access point is the one in use");
        assert_eq!(found[0].strength, 75);
        assert_eq!(found[0].point, "/ap/3");
        assert_eq!(found[1].saved.as_deref(), Some("/settings/7"));
        assert!(found[2].saved.is_none());
    }

    #[test]
    fn the_device_that_is_most_online_is_the_one_that_counts() {
        let devices = [
            Device {
                path: "/devices/1".into(),
                kind: ETHERNET,
                state: 20,
                managed: true,
                active: None,
                points: Vec::new(),
            },
            Device {
                path: "/devices/2".into(),
                kind: ETHERNET,
                state: 100,
                managed: true,
                active: None,
                points: Vec::new(),
            },
            // a bridge or a loopback is not a cable
            Device {
                path: "/devices/3".into(),
                kind: 32,
                state: 100,
                managed: true,
                active: None,
                points: Vec::new(),
            },
        ];
        let found = picture(true, true, &devices, &[]);
        assert_eq!(
            found.wired,
            Some(Wired {
                path: "/devices/2".into(),
                link: Link::Connected
            })
        );
        assert!(found.wireless.is_none());

        // a card NetworkManager does not look after is not shown
        let unmanaged = [Device {
            path: "/devices/4".into(),
            kind: WIFI,
            state: 10,
            managed: false,
            active: None,
            points: Vec::new(),
        }];
        assert!(picture(true, true, &unmanaged, &[]).wireless.is_none());
    }

    #[test]
    fn a_network_name_is_text_without_control_characters() {
        assert_eq!(name(b"Home"), "Home");
        assert_eq!(name(b"Caf\xc3\xa9 5G"), "Caf\u{e9} 5G");
        assert_eq!(name(b"bad\x00\x1bname"), "badname");
    }

    #[test]
    fn a_wep_password_is_a_key_or_a_passphrase() {
        assert_eq!(wep_kind("abcde"), 1);
        assert_eq!(wep_kind("0123456789"), 1);
        assert_eq!(wep_kind("0123456789abcdef0123456789"), 1);
        assert_eq!(wep_kind("not a key at all"), 2);
        assert_eq!(wep_kind("zzzzzzzzzz"), 2);
    }
}
