//! `BlueZ` from a client's side: whether Bluetooth is on and the devices paired with this machine, as
//! the system menu shows them, and what the menu asks of `BlueZ` when the owner flips the switch or
//! picks a device. Pairing a new device is a job for Settings.

#[cfg(feature = "bus")]
use crate::bus;

/// `BlueZ`'s name on the system bus.
pub const SERVICE: &str = "org.bluez";

/// The name a person knows it by, for the sentences.
#[cfg(feature = "bus")]
const NAME: &str = "Bluetooth";

/// One device paired with this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// Its object on the bus, which a connect names.
    pub path: String,
    /// The name the owner gave it, or the name it gave itself.
    pub name: String,
    /// The icon `BlueZ` picked from its class, like `audio-headphones`, when it picked one.
    pub icon: Option<String>,
    /// Whether it is connected now.
    pub connected: bool,
}

/// Bluetooth on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    /// The adapter's object on the bus.
    pub adapter: String,
    /// Whether the adapter is on.
    pub powered: bool,
    /// The paired devices, the connected ones first.
    pub devices: Vec<Device>,
}

/// An object from `BlueZ`'s list, reduced to what the menu needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Object {
    /// An adapter, and whether it is on.
    Adapter {
        /// Its object on the bus.
        path: String,
        /// `Powered`.
        powered: bool,
    },
    /// A device `BlueZ` knows of.
    Device {
        /// Its object on the bus.
        path: String,
        /// The adapter it belongs to.
        adapter: String,
        /// `Alias`, which is the name when the owner gave it none.
        name: String,
        /// `Icon`.
        icon: Option<String>,
        /// `Paired`.
        paired: bool,
        /// `Connected`.
        connected: bool,
    },
}

/// The picture from `BlueZ`'s objects: the first adapter and the devices paired through it. `None`
/// when the machine has no adapter.
#[must_use]
pub fn picture(objects: &[Object]) -> Option<Picture> {
    let (adapter, powered) = objects
        .iter()
        .filter_map(|object| match object {
            Object::Adapter { path, powered } => Some((path, *powered)),
            Object::Device { .. } => None,
        })
        .min_by(|one, other| one.0.cmp(other.0))?;
    let mut devices: Vec<Device> = objects
        .iter()
        .filter_map(|object| match object {
            Object::Device {
                path,
                adapter: owner,
                name,
                icon,
                paired: true,
                connected,
            } if owner == adapter => Some(Device {
                path: path.clone(),
                name: name.clone(),
                icon: icon.clone(),
                connected: *connected,
            }),
            _ => None,
        })
        .collect();
    devices.sort_by(|one, other| {
        other
            .connected
            .cmp(&one.connected)
            .then(one.name.cmp(&other.name))
    });
    Some(Picture {
        adapter: adapter.clone(),
        powered,
        devices,
    })
}

/// Bluetooth as `BlueZ` has it now, or `None` when `BlueZ` is not running, which is what it does on a
/// machine with no adapter.
///
/// # Errors
///
/// A sentence when `BlueZ` is running and does not answer.
#[cfg(feature = "bus")]
pub fn read() -> Result<Option<Picture>, String> {
    use std::collections::HashMap;

    use zbus::zvariant::{OwnedObjectPath, OwnedValue};

    type Interfaces = HashMap<String, HashMap<String, OwnedValue>>;

    let connection = bus::connect(bus::PROPERTY_TIMEOUT)?;
    // asking a service that is not running starts it, and BlueZ's unit refuses to start with no
    // adapter, which would be one more line in the journal every time the menu looked
    if !bus::running(&connection, SERVICE) {
        return Ok(None);
    }
    let listed: HashMap<OwnedObjectPath, Interfaces> = bus::object(
        &connection,
        SERVICE,
        "/",
        "org.freedesktop.DBus.ObjectManager",
    )
    .and_then(|proxy| proxy.call("GetManagedObjects", &()))
    .map_err(|e| bus::sentence_for(NAME, e))?;
    let mut objects = Vec::new();
    for (path, mut interfaces) in listed {
        let path = path.to_string();
        if let Some(mut adapter) = interfaces.remove("org.bluez.Adapter1") {
            let powered = adapter
                .remove("Powered")
                .and_then(|value| bool::try_from(value).ok())
                .unwrap_or(false);
            objects.push(Object::Adapter { path, powered });
        } else if let Some(mut device) = interfaces.remove("org.bluez.Device1") {
            let mut text = |key: &str| {
                device
                    .remove(key)
                    .and_then(|value| String::try_from(value).ok())
            };
            let name = text("Alias").or_else(|| text("Name")).unwrap_or_default();
            let icon = text("Icon");
            let adapter = device
                .remove("Adapter")
                .and_then(|value| OwnedObjectPath::try_from(value).ok())
                .map(|adapter| adapter.to_string())
                .unwrap_or_default();
            let mut flag = |key: &str| {
                device
                    .remove(key)
                    .and_then(|value| bool::try_from(value).ok())
                    .unwrap_or(false)
            };
            let paired = flag("Paired");
            let connected = flag("Connected");
            objects.push(Object::Device {
                path,
                adapter,
                name,
                icon,
                paired,
                connected,
            });
        }
    }
    Ok(picture(&objects))
}

/// Turn the adapter on or off.
///
/// # Errors
///
/// A sentence when `BlueZ` refuses, which it does when a switch on the machine blocks the radio.
#[cfg(feature = "bus")]
pub fn set_powered(adapter: &str, on: bool) -> Result<(), String> {
    let connection = bus::connect(bus::PROPERTY_TIMEOUT)?;
    bus::object(&connection, SERVICE, adapter, "org.bluez.Adapter1")
        .map_err(|e| bus::sentence_for(NAME, e))?
        .set_property("Powered", on)
        .map_err(|e| bus::sentence_for(NAME, e.into()))
}

/// Connect a paired device, or disconnect it. Connecting waits for the device to answer, which can
/// take a while.
///
/// # Errors
///
/// A sentence when the device does not answer or `BlueZ` refuses.
#[cfg(feature = "bus")]
pub fn connect(device: &str, on: bool) -> Result<(), String> {
    let connection = bus::connect(std::time::Duration::from_secs(40))?;
    bus::object(&connection, SERVICE, device, "org.bluez.Device1")
        .and_then(|proxy| proxy.call::<_, _, ()>(if on { "Connect" } else { "Disconnect" }, &()))
        .map_err(|e| bus::sentence_for(NAME, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(path: &str, adapter: &str, name: &str, paired: bool, connected: bool) -> Object {
        Object::Device {
            path: path.to_string(),
            adapter: adapter.to_string(),
            name: name.to_string(),
            icon: Some("audio-headphones".to_string()),
            paired,
            connected,
        }
    }

    #[test]
    fn no_adapter_no_bluetooth() {
        assert_eq!(picture(&[]), None);
        assert_eq!(
            picture(&[device(
                "/org/bluez/hci0/dev_1",
                "/org/bluez/hci0",
                "Phone",
                true,
                false
            )]),
            None
        );
    }

    #[test]
    fn the_paired_devices_of_the_first_adapter_the_connected_ones_first() {
        let objects = [
            Object::Adapter {
                path: "/org/bluez/hci1".into(),
                powered: false,
            },
            device(
                "/org/bluez/hci0/dev_1",
                "/org/bluez/hci0",
                "Speaker",
                true,
                false,
            ),
            Object::Adapter {
                path: "/org/bluez/hci0".into(),
                powered: true,
            },
            device(
                "/org/bluez/hci0/dev_2",
                "/org/bluez/hci0",
                "Headphones",
                true,
                true,
            ),
            // seen nearby, never paired
            device(
                "/org/bluez/hci0/dev_3",
                "/org/bluez/hci0",
                "Television",
                false,
                false,
            ),
            device(
                "/org/bluez/hci1/dev_4",
                "/org/bluez/hci1",
                "Mouse",
                true,
                true,
            ),
        ];
        let found = picture(&objects).expect("an adapter");
        assert_eq!(found.adapter, "/org/bluez/hci0");
        assert!(found.powered);
        let names: Vec<&str> = found
            .devices
            .iter()
            .map(|device| device.name.as_str())
            .collect();
        assert_eq!(names, ["Headphones", "Speaker"]);
    }
}
