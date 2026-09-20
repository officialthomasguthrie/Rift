//! The boot style on the esp of the drive this system started from.
//!
//! The esp is root's, mounted at /boot for root alone, so the owner cannot write the word that says
//! how the next boot looks. Vault owns the drive and runs as root, so it does it for them: the two
//! methods on the bus mount the esp under the service's own runtime directory, read or write the
//! one file, and let it go again. The initrd reads the same file before plymouth starts.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use librift::boot::Style;
use librift::disk::run::Mounted;

/// Numbers the mount points, so two calls at once never pick the same folder.
static NEXT: AtomicUsize = AtomicUsize::new(0);

/// The name udev gives the esp of the drive the running system is on. It names no host disk's.
const ESP: &str = "esp";

/// Where the esp is, and where it is mounted while Vault reads or writes it.
pub struct Esp {
    /// Where udev names the running drive's partitions, `/dev/disk/by-designator`.
    designators: PathBuf,
    /// Where the esp is mounted while Vault uses it, `/run/vault`.
    run: PathBuf,
}

impl Esp {
    /// The esp named under `designators`, mounted under `run` while it is read or written.
    #[must_use]
    pub fn new(designators: PathBuf, run: PathBuf) -> Self {
        Self { designators, run }
    }

    /// The boot style the drive holds, text when it holds none.
    ///
    /// # Errors
    ///
    /// A sentence when the esp is not there or could not be mounted.
    pub fn style(&self) -> Result<Style, String> {
        let esp = self.mount()?;
        let style = Style::read(esp.path());
        esp.unmount()?;
        Ok(style)
    }

    /// Writes the boot style onto the drive.
    ///
    /// # Errors
    ///
    /// A sentence when the esp is not there, could not be mounted, or could not be written.
    pub fn set_style(&self, style: Style) -> Result<(), String> {
        let esp = self.mount()?;
        style.write(esp.path())?;
        esp.unmount()
    }

    /// Mounts the esp under the runtime directory. It is mounted at /boot as well, on an automount
    /// that comes and goes, and the same vfat mounted twice is one file system either way.
    fn mount(&self) -> Result<Mounted, String> {
        let device = self.designators.join(ESP);
        if !device.exists() {
            return Err("The drive this system started from has no boot partition.".into());
        }
        let number = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = self
            .run
            .join(format!("esp-{}-{number}", std::process::id()));
        Mounted::new(&device, path, "vfat", None)
    }
}
