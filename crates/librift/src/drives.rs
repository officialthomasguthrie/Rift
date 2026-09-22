//! The disks a person plugs in, and the exchange partition of the drive itself.
//!
//! udisks answers on the system bus for every block device the kernel knows, and mounts the ones
//! polkit lets the owner mount. Rift's polkit rule refuses every udisks action whose id ends in
//! `-system`, which is the one udisks asks for when the device is internal to the machine, so a
//! host's own disks can be looked at and never mounted. What is left is removable disks and disks
//! on USB, which are the ones a person brought with them: those are what this lists.
//!
//! The drive Rift itself runs from is left out whole. On a real stick it is removable like any
//! other, so its esp, its two slots and its locked persist would all be rows to mount; they are
//! the drive's own, not disks to open. The exchange partition of the drive is mounted by the
//! system at [`EXCHANGE`] and is a folder, not a disk to mount and eject.
//!
//! Nothing here mounts anything by itself. A disk that is plugged in appears, and it is mounted
//! when the owner asks for it.

use std::path::{Path, PathBuf};

/// udisks on the system bus.
pub const SERVICE: &str = "org.freedesktop.UDisks2";
/// Where udev names the partitions of the drive the system started from.
const DESIGNATORS: &str = "/dev/disk/by-designator";
/// What the kernel says about a block device.
const SYSFS: &str = "/sys/class/block";
/// Where the system mounts the exchange partition of the drive, when the drive has one.
pub const EXCHANGE: &str = "/exchange";
/// What the sidebar calls that folder.
pub const EXCHANGE_NAME: &str = "Exchange";

/// A disk, or a partition of one, that the sidebar lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    /// What names it on the bus, and what `--set drive` takes.
    pub id: String,
    /// What the sidebar calls it: the name of its file system, the disk's own model, or how big
    /// it is.
    pub name: String,
    /// The device the kernel gave it, `/dev/sda1`.
    pub device: String,
    /// The kind of file system on it, `exfat` or `ext4`, or `crypto_LUKS` for a locked one.
    pub fs: String,
    /// How big it is, in bytes.
    pub size: u64,
    /// Where it is mounted, when it is.
    pub mount: Option<PathBuf>,
    /// Whether it holds an encrypted volume that has not been unlocked.
    pub locked: bool,
    /// The disk it is a part of, which is what is ejected.
    pub drive: String,
    /// Whether that disk can be ejected or switched off.
    pub eject: bool,
    /// The symbolic icon the sidebar draws it with.
    pub icon: &'static str,
}

impl Volume {
    /// Whether it is mounted now.
    #[must_use]
    pub const fn mounted(&self) -> bool {
        self.mount.is_some()
    }
}

/// The exchange partition of the drive, when the drive has one and the system has mounted it.
#[must_use]
pub fn exchange() -> Option<PathBuf> {
    let top = crate::files::top_of(Path::new(EXCHANGE))?;
    (top == Path::new(EXCHANGE)).then_some(top)
}

/// The disk the system started from: the one the esp is a partition of. Everything on it belongs
/// to the drive and is never a row to mount.
#[must_use]
pub fn own_disk() -> Option<PathBuf> {
    let esp = std::fs::canonicalize(Path::new(DESIGNATORS).join("esp")).ok()?;
    let known = std::fs::canonicalize(Path::new(SYSFS).join(esp.file_name()?)).ok()?;
    Some(Path::new("/dev").join(known.parent()?.file_name()?))
}

/// The icon for a disk, by the way it is attached.
#[cfg(any(feature = "bus", test))]
#[must_use]
fn icon_of(bus: &str, locked: bool) -> &'static str {
    if locked {
        "changes-prevent-symbolic"
    } else if bus == "usb" {
        "drive-removable-media-symbolic"
    } else {
        "drive-harddisk-symbolic"
    }
}

#[cfg(feature = "bus")]
mod asking {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use zbus::zvariant::{Array, OwnedObjectPath, OwnedValue};

    use super::{SERVICE, Volume, icon_of, own_disk};
    use crate::bus;

    /// What a person calls udisks in a sentence about something going wrong.
    const NAME: &str = "the disk service";
    /// Where it hangs its objects.
    const ROOT: &str = "/org/freedesktop/UDisks2";

    /// How long a call may take. Mounting a file system reads it first, and an unmount writes
    /// everything that is waiting, so these are not as quick as reading a property.
    const TIMEOUT: Duration = Duration::from_secs(120);

    /// What `GetManagedObjects` answers: an object, its interfaces, and each one's properties.
    type Interfaces = HashMap<String, HashMap<String, OwnedValue>>;
    type Objects = HashMap<OwnedObjectPath, Interfaces>;

    const BLOCK: &str = "org.freedesktop.UDisks2.Block";
    const FILESYSTEM: &str = "org.freedesktop.UDisks2.Filesystem";
    const DRIVE: &str = "org.freedesktop.UDisks2.Drive";
    const ENCRYPTED: &str = "org.freedesktop.UDisks2.Encrypted";

    /// Everything udisks knows, in one call.
    fn managed(connection: &zbus::blocking::Connection) -> Result<Objects, String> {
        bus::object(
            connection,
            SERVICE,
            ROOT,
            "org.freedesktop.DBus.ObjectManager",
        )
        .and_then(|proxy| proxy.call("GetManagedObjects", &()))
        .map_err(|e| bus::sentence_for(NAME, e))
    }

    fn text(properties: &Interfaces, interface: &str, key: &str) -> String {
        properties
            .get(interface)
            .and_then(|found| found.get(key))
            .and_then(|value| String::try_from(value.try_clone().ok()?).ok())
            .unwrap_or_default()
    }

    fn flag(properties: &Interfaces, interface: &str, key: &str) -> bool {
        properties
            .get(interface)
            .and_then(|found| found.get(key))
            .and_then(|value| bool::try_from(value.try_clone().ok()?).ok())
            .unwrap_or(false)
    }

    fn number(properties: &Interfaces, interface: &str, key: &str) -> u64 {
        properties
            .get(interface)
            .and_then(|found| found.get(key))
            .and_then(|value| u64::try_from(value.try_clone().ok()?).ok())
            .unwrap_or(0)
    }

    fn object(properties: &Interfaces, interface: &str, key: &str) -> String {
        properties
            .get(interface)
            .and_then(|found| found.get(key))
            .and_then(|value| OwnedObjectPath::try_from(value.try_clone().ok()?).ok())
            .map(|path| path.to_string())
            .unwrap_or_default()
    }

    /// A string of bytes with the nothing at its end taken off, which is how udisks gives a device
    /// name and a mount point.
    fn path_of(bytes: &zbus::zvariant::Value<'_>) -> Option<PathBuf> {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let array = Array::try_from(bytes.try_clone().ok()?).ok()?;
        let mut found = Vec::<u8>::try_from(array).ok()?;
        while found.last() == Some(&0) {
            found.pop();
        }
        (!found.is_empty()).then(|| PathBuf::from(OsString::from_vec(found)))
    }

    fn device(properties: &Interfaces) -> String {
        properties
            .get(BLOCK)
            .and_then(|block| block.get("Device"))
            .and_then(|device| path_of(device))
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    }

    /// Where a file system is mounted, the first place when it is mounted in more than one.
    fn mount_of(properties: &Interfaces) -> Option<PathBuf> {
        let points = properties.get(FILESYSTEM)?.get("MountPoints")?;
        let array = Array::try_from(points.try_clone().ok()?).ok()?;
        array.iter().find_map(path_of)
    }

    /// The disks a person plugged in, in the order the sidebar lists them.
    ///
    /// # Errors
    ///
    /// A sentence when udisks cannot be reached or does not answer.
    pub fn volumes() -> Result<Vec<Volume>, String> {
        let connection = bus::connect(TIMEOUT)?;
        let objects = managed(&connection)?;
        Ok(listed(&objects, own_disk().as_deref()))
    }

    /// What the sidebar lists, out of everything udisks knows.
    fn listed(objects: &Objects, own: Option<&Path>) -> Vec<Volume> {
        let ours = own.and_then(|disk| {
            objects
                .iter()
                .find(|(_, properties)| device(properties) == disk.display().to_string())
                .map(|(_, properties)| object(properties, BLOCK, "Drive"))
        });
        // a locked volume that has been unlocked shows as the file system inside it, not twice
        let unlocked: Vec<String> = objects
            .values()
            .map(|properties| object(properties, BLOCK, "CryptoBackingDevice"))
            .filter(|backing| !backing.is_empty() && backing != "/")
            .collect();
        let mut found: Vec<Volume> = objects
            .iter()
            .filter_map(|(path, properties)| {
                let drive = object(properties, BLOCK, "Drive");
                let disk = objects.get(&OwnedObjectPath::try_from(drive.as_str()).ok()?)?;
                let volume = one(path.as_str(), properties, disk, &drive)?;
                let mine = ours.as_ref() == Some(&drive);
                let shown = !mine && !unlocked.contains(&volume.id);
                shown.then_some(volume)
            })
            .collect();
        found.sort_by(|one, other| {
            crate::files::natural(&one.name, &other.name).then_with(|| one.id.cmp(&other.id))
        });
        found
    }

    /// One row, when this object is a volume the sidebar lists: a file system or a locked volume,
    /// on a disk that is removable or on USB, that udisks does not say to hide.
    fn one(path: &str, properties: &Interfaces, disk: &Interfaces, drive: &str) -> Option<Volume> {
        if !properties.contains_key(BLOCK) || flag(properties, BLOCK, "HintIgnore") {
            return None;
        }
        let encrypted = properties.contains_key(ENCRYPTED);
        if !properties.contains_key(FILESYSTEM) && !encrypted {
            return None;
        }
        let bus = text(disk, DRIVE, "ConnectionBus");
        if !flag(disk, DRIVE, "Removable") && bus != "usb" {
            return None;
        }
        let size = number(properties, BLOCK, "Size");
        if size == 0 {
            return None;
        }
        let mount = mount_of(properties);
        let locked = encrypted && mount.is_none();
        let label = text(properties, BLOCK, "IdLabel");
        let model = [text(disk, DRIVE, "Vendor"), text(disk, DRIVE, "Model")]
            .join(" ")
            .trim()
            .to_string();
        let name = if label.is_empty() {
            if model.is_empty() {
                format!("{} volume", crate::files::size_words(size))
            } else {
                model
            }
        } else {
            label
        };
        Some(Volume {
            id: path.to_string(),
            name,
            device: device(properties),
            fs: text(properties, BLOCK, "IdType"),
            size,
            mount,
            locked,
            drive: drive.to_string(),
            eject: flag(disk, DRIVE, "Ejectable") || flag(disk, DRIVE, "CanPowerOff"),
            icon: icon_of(&bus, locked),
        })
    }

    /// A proxy for one interface of one udisks object.
    fn on(
        connection: &zbus::blocking::Connection,
        id: &str,
        interface: &'static str,
    ) -> Result<zbus::blocking::Proxy<'static>, String> {
        bus::object(connection, SERVICE, id, interface).map_err(|e| bus::sentence_for(NAME, e))
    }

    /// Mount a volume, and say where it went. udisks picks the folder, `/run/media/<account>/<the
    /// volume's name>`, and gives a file system with no owners of its own to the account that
    /// asked for it.
    ///
    /// # Errors
    ///
    /// A sentence when udisks refuses or the file system cannot be read.
    pub fn mount(id: &str) -> Result<PathBuf, String> {
        let connection = bus::connect(TIMEOUT)?;
        let options: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
        let where_it_went: String = on(&connection, id, FILESYSTEM)?
            .call("Mount", &(options,))
            .map_err(|e| refusal(e, "mount"))?;
        Ok(PathBuf::from(where_it_went))
    }

    /// Unmount a volume, writing out everything that was waiting.
    ///
    /// # Errors
    ///
    /// A sentence when udisks refuses, or something still has the volume open.
    pub fn unmount(id: &str) -> Result<(), String> {
        let connection = bus::connect(TIMEOUT)?;
        unmount_on(&connection, id)
    }

    fn unmount_on(connection: &zbus::blocking::Connection, id: &str) -> Result<(), String> {
        let options: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
        on(connection, id, FILESYSTEM)?
            .call::<_, _, ()>("Unmount", &(options,))
            .map_err(|e| refusal(e, "unmount"))
    }

    /// Unmount everything on the disk this volume is on, then eject it or switch it off, so the
    /// stick can be pulled out.
    ///
    /// # Errors
    ///
    /// A sentence when something still has a file system on it open, or udisks refuses.
    pub fn eject(id: &str) -> Result<(), String> {
        let connection = bus::connect(TIMEOUT)?;
        let objects = managed(&connection)?;
        let drive = objects
            .get(
                &OwnedObjectPath::try_from(id)
                    .map_err(|_| "That disk is not there.".to_string())?,
            )
            .map(|properties| object(properties, BLOCK, "Drive"))
            .filter(|drive| !drive.is_empty())
            .ok_or_else(|| "That disk is not there.".to_string())?;
        for (path, properties) in &objects {
            if object(properties, BLOCK, "Drive") == drive && mount_of(properties).is_some() {
                unmount_on(&connection, path.as_str())?;
            }
        }
        let disk = objects
            .get(
                &OwnedObjectPath::try_from(drive.as_str())
                    .map_err(|_| "That disk is not there.")?,
            )
            .ok_or_else(|| "That disk is not there.".to_string())?;
        let options: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
        if flag(disk, DRIVE, "Ejectable") {
            on(&connection, &drive, DRIVE)?
                .call::<_, _, ()>("Eject", &(options.clone(),))
                .map_err(|e| refusal(e, "eject"))?;
        }
        if flag(disk, DRIVE, "CanPowerOff") {
            on(&connection, &drive, DRIVE)?
                .call::<_, _, ()>("PowerOff", &(options,))
                .map_err(|e| refusal(e, "switch off"))?;
        }
        Ok(())
    }

    /// The sentence for a call udisks would not do. A refusal from polkit is the rule that keeps
    /// the disks of the machine as they are, and it is worth saying so.
    fn refusal(error: zbus::Error, doing: &str) -> String {
        if bus::refused(&error) {
            return format!(
                "Rift does not {doing} a disk that belongs to this computer, only one you plugged in."
            );
        }
        bus::sentence_for(NAME, error)
    }

    /// Call `each` whenever udisks has something new to say: a disk plugged in or pulled out, a
    /// volume mounted or unmounted. Blocks, so the caller runs it on a thread of its own.
    ///
    /// # Errors
    ///
    /// When the bus cannot be reached or closes the connection.
    pub fn watch<F: FnMut() -> bool>(each: F) -> Result<(), String> {
        bus::signals(SERVICE, each)
    }
}

#[cfg(feature = "bus")]
pub use asking::{eject, mount, unmount, volumes, watch};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_disk_is_drawn_by_the_way_it_is_attached() {
        assert_eq!(icon_of("usb", false), "drive-removable-media-symbolic");
        assert_eq!(icon_of("sdio", false), "drive-harddisk-symbolic");
        assert_eq!(icon_of("usb", true), "changes-prevent-symbolic");
    }

    #[test]
    fn the_exchange_partition_is_a_folder_of_its_own_or_nothing() {
        // the folder is not mounted here, and a folder on the same file system as its parent is
        // not a partition
        assert_eq!(exchange(), None);
    }
}
