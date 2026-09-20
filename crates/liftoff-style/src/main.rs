//! liftoff-style: gives plymouth the boot style the owner chose, in the initrd of every boot.
//!
//! The style is a word on the esp, which Settings writes through Vault. Nothing else of the drive
//! can be read this early: the image is not mounted yet and plymouth draws the passphrase prompt
//! before persist is open. This runs before plymouth starts, mounts that esp, reads the word and
//! writes the theme for it into the initrd's copy of plymouth's config, which is on a tmpfs and so
//! is the one file this boot can change. A drive with no word, an esp that does not show up, or
//! anything else that goes wrong leaves the style the image was built with.

use std::fs;
use std::process::ExitCode;

/// Plymouth's config in the initrd. It names the theme plymouthd loads when the kernel command
/// line names none.
const CONF: &str = "/etc/plymouth/plymouthd.conf";
/// Where the new one is written before it takes the old one's place.
const CONF_BESIDE: &str = "/etc/plymouth/plymouthd.conf.new";

fn main() -> ExitCode {
    match run() {
        Ok(said) => println!("liftoff-style: {said}"),
        Err(why) => eprintln!("liftoff-style: {why}"),
    }
    // the boot goes on either way. the splash is not worth stopping a machine from starting
    ExitCode::SUCCESS
}

fn run() -> Result<String, String> {
    let style = drive::style()?;
    let conf = fs::read_to_string(CONF).map_err(|e| format!("Could not read {CONF}: {e}"))?;
    let written = with_theme(&conf, style.theme())
        .ok_or_else(|| format!("{CONF} names no theme, so the boot draws what it always did."))?;
    if written == conf {
        return Ok(format!(
            "this drive starts with the {} boot, which is the one it was built with",
            style.word()
        ));
    }
    fs::write(CONF_BESIDE, &written).map_err(|e| format!("Could not write {CONF}: {e}"))?;
    // the file in the initrd is a link into the read-only store, and a rename takes its place
    fs::rename(CONF_BESIDE, CONF).map_err(|e| format!("Could not put {CONF} in place: {e}"))?;
    Ok(format!("this drive starts with the {} boot", style.word()))
}

/// Plymouth's config with the theme it names changed, or `None` when it names none.
fn with_theme(conf: &str, theme: &str) -> Option<String> {
    let mut written = String::with_capacity(conf.len() + theme.len());
    let mut found = false;
    for line in conf.lines() {
        match line.split_once('=') {
            Some((key, _)) if key.trim() == "Theme" => {
                written.push_str("Theme=");
                written.push_str(theme);
                found = true;
            }
            _ => written.push_str(line),
        }
        written.push('\n');
    }
    found.then_some(written)
}

/// The esp of the drive this system started from, and the word on it.
#[cfg(target_os = "linux")]
mod drive {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};
    use std::{fs, thread};

    use librift::boot::{LOADER_PARTITION, PARTITIONS, Style, loader_partition};
    use rustix::mount::{MountFlags, UnmountFlags, mount, unmount};

    /// Where the esp is mounted while the word is read.
    const AT: &str = "/run/liftoff-style";
    /// How long the esp gets to show up. Plymouth waits for this, so it is bounded: a drive slower
    /// than this draws the style the image has rather than holding the boot up for a splash. Ten
    /// seconds is room for a usb stick to be enumerated, which the rest of the boot waits for
    /// anyway, since nothing of the system can be read until it is.
    const WAIT: Duration = Duration::from_secs(10);

    /// The boot style on that esp, text when it holds none.
    pub fn style() -> Result<Style, String> {
        let node = esp()?;
        let at = Path::new(AT);
        fs::create_dir_all(at).map_err(|e| format!("Could not make {AT}: {e}"))?;
        mount(
            &node,
            at,
            "vfat",
            MountFlags::RDONLY | MountFlags::NOSUID | MountFlags::NODEV | MountFlags::NOEXEC,
            None,
        )
        .map_err(|e| format!("Could not mount {}: {e}", node.display()))?;
        let style = Style::read(at);
        let _ = unmount(at, UnmountFlags::DETACH);
        let _ = fs::remove_dir(at);
        Ok(style)
    }

    /// The esp systemd-boot started this system from, once udev has named it. Never a host disk's.
    fn esp() -> Result<PathBuf, String> {
        let variable = fs::read(LOADER_PARTITION).map_err(|e| {
            format!("Could not read which partition systemd-boot started from: {e}")
        })?;
        let uuid = loader_partition(&variable)
            .ok_or("systemd-boot left a partition uuid that cannot be read.")?;
        let node = Path::new(PARTITIONS).join(&uuid);
        let start = Instant::now();
        while !node.exists() {
            if start.elapsed() > WAIT {
                return Err(format!(
                    "The partition systemd-boot started from, {uuid}, did not show up in {} s.",
                    WAIT.as_secs()
                ));
            }
            thread::sleep(Duration::from_millis(100));
        }
        Ok(node)
    }
}

/// Nothing to read anywhere else: the word is on a Rift drive, which only Linux mounts.
#[cfg(not(target_os = "linux"))]
mod drive {
    use librift::boot::Style;

    /// Always a sentence, on a system that is not a drive.
    pub fn style() -> Result<Style, String> {
        Err("The boot style is on the esp of a Rift drive, which only Linux mounts.".into())
    }
}

#[cfg(test)]
mod tests {
    use librift::boot::Style;

    use super::*;

    /// What the image's plymouth config holds, from nix/modules/liftoff.nix and the nixos module
    /// that writes it.
    const CONFIG: &str =
        "[Daemon]\nShowDelay=0\nDeviceTimeout=8\nTheme=liftoff-text\nUseSimpledrm=1\n";

    #[test]
    fn the_theme_changes_and_nothing_else_does() {
        let written = with_theme(CONFIG, "liftoff-graphical").unwrap();
        assert_eq!(
            written,
            "[Daemon]\nShowDelay=0\nDeviceTimeout=8\nTheme=liftoff-graphical\nUseSimpledrm=1\n"
        );
        // the same theme again is the same file, so nothing is written
        assert_eq!(with_theme(CONFIG, "liftoff-text").unwrap(), CONFIG);
        for style in Style::ALL {
            let written = with_theme(CONFIG, style.theme()).unwrap();
            assert!(written.contains(&format!("Theme={}\n", style.theme())));
            assert!(written.contains("UseSimpledrm=1"));
            assert_eq!(written.lines().count(), CONFIG.lines().count());
        }
    }

    #[test]
    fn a_config_that_names_no_theme_is_left_alone() {
        assert_eq!(
            with_theme("[Daemon]\nShowDelay=0\n", "liftoff-graphical"),
            None
        );
        assert_eq!(with_theme("", "liftoff-graphical"), None);
        // a key that only starts with the word is not the key
        assert_eq!(with_theme("ThemeDir=/themes\n", "liftoff-graphical"), None);
    }

    #[test]
    fn spaces_around_the_key_still_name_it() {
        let written =
            with_theme("[Daemon]\n Theme = liftoff-text \n", "liftoff-graphical").unwrap();
        assert_eq!(written, "[Daemon]\nTheme=liftoff-graphical\n");
    }
}
