//! The keys for the volume and the backlight. Horizon runs `lens --volume up` and the rest for
//! them: the change is made here, with the same programs the system menu runs, and the level it left
//! behind goes to the shell, which shows it in the key popup. When the shell is not running the key
//! still does what it says.

use librift::sound::{self, Side};

use crate::control::Level;
use crate::status;

/// What `--volume` and `--brightness` take after them.
pub const USAGE: &str = "lens --volume up|down|mute, lens --brightness up|down";

/// Turn the volume up or down a step, or mute and unmute it, and read back where it is.
///
/// # Errors
///
/// When the word is not one of the three, or `wpctl` refuses or reads nothing.
pub fn volume(word: &str) -> Result<Level, String> {
    match word {
        "up" => sound::step_volume(true)?,
        "down" => sound::step_volume(false)?,
        "mute" => sound::toggle_mute(Side::Output)?,
        _ => return Err(format!("Try {USAGE}.")),
    }
    let volume =
        sound::volume(Side::Output).ok_or("wpctl reads no volume for the default sink.")?;
    Ok(Level::Volume {
        level: u8::try_from(volume.level.min(100)).unwrap_or(100),
        muted: volume.muted,
    })
}

/// Turn the backlight up or down a step, and read back where it is.
///
/// # Errors
///
/// When the word is not one of the two, or the machine has no backlight.
pub fn brightness(word: &str) -> Result<Level, String> {
    match word {
        "up" => status::step_brightness(true)?,
        "down" => status::step_brightness(false)?,
        _ => return Err(format!("Try {USAGE}.")),
    }
    status::brightness()
        .map(Level::Brightness)
        .ok_or_else(|| "This machine has no backlight.".to_string())
}
