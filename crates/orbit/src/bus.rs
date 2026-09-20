//! Orbit on the system bus: `dev.rift.Orbit` at `/dev/rift/Orbit`.
//!
//! The properties are the effective profile, worked out once before the name is taken, so anything
//! that waits for the name has the answers the moment it arrives. `Set` and `SetDisplayScale`
//! write the `[set]` layer of that profile, which is what Settings and `rift host set` change;
//! `[detected]` and every other line of the file stay as they were. Anyone on the machine may read
//! the properties, and root and the people who log in may write: Rift's own services run as system
//! accounts and have no business saying what this machine is.

use std::path::PathBuf;

use librift::Component;
use zbus::fdo;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;

use crate::profile::{self, Profile, Settings};

/// One output, as the bus reports it: connector, width and height in pixels, width and height in
/// centimetres, scale. A width of 0 means the output gave no EDID, which is what a KVM switch and
/// a cheap adapter look like.
pub type BusDisplay = (String, u32, u32, u32, u32, u32);

/// The first uid a person gets. Everything under it is root and the system's own accounts.
const FIRST_ACCOUNT: u32 = 1000;

/// The object that answers on the bus.
pub struct Orbit {
    /// Where the profiles live, so a write lands in the file Orbit wrote at the start.
    hosts: PathBuf,
    /// What Orbit worked out about this machine. A write never changes it.
    detected: Settings,
    /// The machine, and the settings the detection and the file make between them.
    profile: Profile,
}

#[zbus::interface(name = "dev.rift.Orbit")]
impl Orbit {
    /// SHA-256 of the machine's DMI strings and PCI ids.
    #[zbus(property)]
    fn fingerprint(&self) -> String {
        self.profile.identity.fingerprint.clone()
    }

    /// `owned`, `trusted` or `borrowed`.
    #[zbus(property)]
    fn class(&self) -> String {
        self.profile.settings.class.clone()
    }

    /// Every connected output.
    #[zbus(property)]
    fn displays(&self) -> Vec<BusDisplay> {
        self.profile
            .settings
            .displays
            .iter()
            .map(|d| {
                (
                    d.connector.clone(),
                    d.mode.0,
                    d.mode.1,
                    d.size_cm.0,
                    d.size_cm.1,
                    d.scale,
                )
            })
            .collect()
    }

    /// `mesa`, `nvk` or `none`.
    #[zbus(property)]
    fn gpu_path(&self) -> String {
        self.profile.settings.gpu_path.clone()
    }

    /// Which Quasar model tier this machine can carry.
    #[zbus(property)]
    fn ai_tier(&self) -> String {
        self.profile.settings.ai_tier.clone()
    }

    /// Writes one setting of this machine into the profile: `class`, `tier` or `gpu`.
    async fn set(
        &mut self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        name: String,
        value: String,
    ) -> fdo::Result<()> {
        let uid = allowed(&header, connection).await?;
        let (key, value) = profile::settable(&name, &value).map_err(fdo::Error::InvalidArgs)?;
        let before = self.write(|set| profile::put(set, key, &value))?;
        println!("orbit: {key} is {value} for this machine, set by uid {uid}");
        self.announce(&before, &emitter).await;
        Ok(())
    }

    /// Writes the size one screen is drawn at, by its connector: 1 or 2.
    async fn set_display_scale(
        &mut self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        connector: String,
        scale: u32,
    ) -> fdo::Result<()> {
        let uid = allowed(&header, connection).await?;
        if !profile::SCALES.contains(&scale) {
            return Err(fdo::Error::InvalidArgs(format!(
                "{scale} is not a size a screen is drawn at. It is 1 or 2."
            )));
        }
        let screens: Vec<String> = self
            .profile
            .settings
            .displays
            .iter()
            .map(|display| display.connector.clone())
            .collect();
        if !screens.contains(&connector) {
            return Err(fdo::Error::InvalidArgs(match screens.len() {
                0 => {
                    format!("There is no screen called \"{connector}\", and none on this machine.")
                }
                _ => format!(
                    "There is no screen called \"{connector}\". There is {}.",
                    screens.join(", ")
                ),
            }));
        }
        let before = self.write(|set| profile::put_scale(set, &connector, scale))?;
        println!("orbit: {connector} is drawn at scale {scale}, set by uid {uid}");
        self.announce(&before, &emitter).await;
        Ok(())
    }
}

impl Orbit {
    /// Write a change into the `[set]` layer and keep what it makes. Answers with the settings as
    /// they were, so the properties that moved can say so.
    fn write(&mut self, change: impl FnOnce(&str) -> String) -> fdo::Result<Settings> {
        let settings =
            profile::write_set(&self.hosts, &self.profile.identity, &self.detected, change)
                .map_err(|e| {
                    fdo::Error::Failed(format!("Could not write the profile for this machine: {e}"))
                })?;
        Ok(std::mem::replace(&mut self.profile.settings, settings))
    }

    /// Tell the bus which properties the write moved.
    async fn announce(&self, before: &Settings, emitter: &SignalEmitter<'_>) {
        let now = &self.profile.settings;
        if before.class != now.class {
            let _ = self.class_changed(emitter).await;
        }
        if before.ai_tier != now.ai_tier {
            let _ = self.ai_tier_changed(emitter).await;
        }
        if before.gpu_path != now.gpu_path {
            let _ = self.gpu_path_changed(emitter).await;
        }
        if before.displays != now.displays {
            let _ = self.displays_changed(emitter).await;
        }
    }
}

/// Who may write. Anyone on the machine may send to the name under the bus policy, the way
/// Vault's does, since the account a person logs in with is not known until first boot setup ran.
/// Orbit is the one that knows what a caller is: root and the people who log in may change what
/// this machine is, and the system accounts Rift's own services run as may not. One of those
/// services runs a model that came off the network.
async fn allowed(header: &Header<'_>, connection: &zbus::Connection) -> fdo::Result<u32> {
    let sender = header
        .sender()
        .ok_or_else(|| fdo::Error::Failed("The request came without a sender.".into()))?
        .to_owned();
    let uid = fdo::DBusProxy::new(connection)
        .await?
        .get_connection_unix_user(sender.into())
        .await?;
    if uid != 0 && uid < FIRST_ACCOUNT {
        return Err(fdo::Error::AccessDenied(
            "Only you or the system itself can change what this machine is.".into(),
        ));
    }
    Ok(uid)
}

/// Takes the name and answers until the process is stopped.
///
/// # Errors
///
/// When the system bus is not there, or another process already owns the name.
pub fn serve(hosts: PathBuf, detected: Settings, profile: Profile) -> zbus::Result<()> {
    let component = Component::Orbit;
    let orbit = Orbit {
        hosts,
        detected,
        profile,
    };
    let _connection = zbus::blocking::connection::Builder::system()?
        .name(component.dbus_name())?
        .serve_at(component.dbus_path(), orbit)?
        .build()?;
    // the connection runs on its own threads; this one has nothing left to do
    loop {
        std::thread::park();
    }
}
