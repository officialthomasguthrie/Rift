//! Orbit from a client's side: what it remembers about this machine, read off the system bus, and
//! the two settings a person changes there.
//!
//! The scale a screen is drawn at lives in the host profile, which is root's, so Orbit writes it.
//! The compositor reads it from a part of its config under the owner's state directory, the way it
//! reads the theme and the wallpaper: the session writes that part when it starts, and Settings
//! writes it again whenever the scale changes, so a screen changes size where it stands.

use std::fmt::Write as _;

use crate::appearance::{home, write_beside};
#[cfg(feature = "bus")]
use crate::{Component, bus};

/// Where the part of Horizon's config that says how big each screen is drawn goes, under home.
pub const HORIZON_PART: &str = ".local/state/rift/displays.kdl";

/// Centimetres in an inch.
const CM_PER_INCH: f64 = 2.54;

/// One connected output, as Orbit reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// The connector, for example `eDP-1`.
    pub connector: String,
    /// Width of the preferred mode in pixels. 0 when the output gave no EDID.
    pub width: u32,
    /// Height of the preferred mode in pixels.
    pub height: u32,
    /// Physical width in centimetres. 0 when the output does not say.
    pub width_cm: u32,
    /// Physical height in centimetres.
    pub height_cm: u32,
    /// The scale the session draws it at.
    pub scale: u32,
}

impl Output {
    /// Dots per inch along the diagonal, `None` when the output does not say enough.
    #[must_use]
    pub fn dpi(&self) -> Option<f64> {
        dpi((self.width, self.height), (self.width_cm, self.height_cm))
    }
}

/// Dots per inch along the diagonal of a screen that many pixels across and that many centimetres
/// wide, `None` when either is unknown.
#[must_use]
pub fn dpi(mode: (u32, u32), size_cm: (u32, u32)) -> Option<f64> {
    if mode.0 == 0 || mode.1 == 0 || size_cm.0 == 0 || size_cm.1 == 0 {
        return None;
    }
    let pixels = f64::from(mode.0).hypot(f64::from(mode.1));
    let inches = f64::from(size_cm.0).hypot(f64::from(size_cm.1)) / CM_PER_INCH;
    Some(pixels / inches)
}

/// The settings Orbit works with for this machine: its defaults, then what it detected, then
/// what a person set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    /// SHA-256 of the machine's DMI strings and PCI ids, in hex.
    pub fingerprint: String,
    /// `owned`, `trusted` or `borrowed`.
    pub class: String,
    /// Every connected output.
    pub outputs: Vec<Output>,
    /// `mesa`, `nvk` or `none`.
    pub gpu_path: String,
    /// Which Quasar model tier the machine can carry.
    pub ai_tier: String,
}

/// Reads Orbit's properties.
///
/// # Errors
///
/// A sentence when the bus or Orbit is not there, or a property could not be read.
#[cfg(feature = "bus")]
pub fn host() -> Result<Host, String> {
    let orbit = Component::Orbit;
    let connection = bus::connect(bus::PROPERTY_TIMEOUT)?;
    let proxy = bus::proxy(&connection, orbit)?;
    let text = |name: &str| {
        proxy
            .get_property::<String>(name)
            .map_err(|e| bus::sentence(orbit, e))
    };
    let outputs: Vec<(String, u32, u32, u32, u32, u32)> = proxy
        .get_property("Displays")
        .map_err(|e| bus::sentence(orbit, e))?;
    Ok(Host {
        fingerprint: text("Fingerprint")?,
        class: text("Class")?,
        outputs: outputs
            .into_iter()
            .map(
                |(connector, width, height, width_cm, height_cm, scale)| Output {
                    connector,
                    width,
                    height,
                    width_cm,
                    height_cm,
                    scale,
                },
            )
            .collect(),
        gpu_path: text("GpuPath")?,
        ai_tier: text("AiTier")?,
    })
}

/// Writes one setting of this machine into the host profile: `class`, `tier` or `gpu`.
///
/// # Errors
///
/// A sentence when the bus or Orbit is not there, or the setting is not one Orbit takes.
#[cfg(feature = "bus")]
pub fn set(name: &str, value: &str) -> Result<(), String> {
    let orbit = Component::Orbit;
    let connection = bus::connect(bus::PROPERTY_TIMEOUT)?;
    bus::proxy(&connection, orbit)?
        .call("Set", &(name, value))
        .map_err(|e| bus::sentence(orbit, e))
}

/// Writes the size one screen is drawn at, by its connector.
///
/// # Errors
///
/// A sentence when the bus or Orbit is not there, or there is no such screen.
#[cfg(feature = "bus")]
pub fn set_display_scale(connector: &str, scale: u32) -> Result<(), String> {
    let orbit = Component::Orbit;
    let connection = bus::connect(bus::PROPERTY_TIMEOUT)?;
    bus::proxy(&connection, orbit)?
        .call("SetDisplayScale", &(connector, scale))
        .map_err(|e| bus::sentence(orbit, e))
}

/// Ask Orbit what the screens are and tell the compositor how big to draw them. The session does
/// this when it starts, and Settings again whenever the scale changes.
///
/// # Errors
///
/// A sentence when Orbit did not answer or the part could not be written.
#[cfg(feature = "bus")]
pub fn follow() -> Result<(), String> {
    write_horizon(&host()?.outputs)
}

/// The part of Horizon's config for these screens: one output block each, at the size the host
/// profile says it is drawn at. The compositor works a size out for itself for a screen the part
/// does not name, which is what every screen gets until Orbit has answered.
#[must_use]
pub fn horizon(outputs: &[Output]) -> String {
    let mut part = String::from("// written from the host profile Orbit keeps\n");
    for output in outputs {
        let _ = write!(
            part,
            "output \"{}\" {{\n    scale {}\n}}\n",
            output.connector.replace('"', ""),
            output.scale.clamp(1, 3)
        );
    }
    part
}

/// Write that part. Horizon reads its config again when the file changes, so the screens follow
/// at once.
///
/// # Errors
///
/// A sentence when there is no home or the file could not be written.
pub fn write_horizon(outputs: &[Output]) -> Result<(), String> {
    let path = home()
        .ok_or("There is no home to write the screens into.")?
        .join(HORIZON_PART);
    write_beside(&path, &horizon(outputs)).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(connector: &str, scale: u32) -> Output {
        Output {
            connector: connector.to_string(),
            width: 1280,
            height: 800,
            width_cm: 32,
            height_cm: 20,
            scale,
        }
    }

    #[test]
    fn the_dots_per_inch_of_a_screen() {
        // the boot test's virtual screen, and a dense laptop panel
        assert_eq!(screen("Virtual-1", 1).dpi().map(f64::round), Some(102.0));
        let panel = Output {
            width: 2880,
            height: 1800,
            width_cm: 30,
            height_cm: 19,
            ..screen("eDP-1", 2)
        };
        assert_eq!(panel.dpi().map(f64::round), Some(243.0));
        // an output that gives no EDID says nothing about either
        let bare = Output {
            width: 0,
            height: 0,
            width_cm: 0,
            height_cm: 0,
            ..screen("HDMI-A-1", 1)
        };
        assert_eq!(bare.dpi(), None);
    }

    #[test]
    fn every_screen_is_an_output_block_at_its_scale() {
        let part = horizon(&[screen("eDP-1", 2), screen("HDMI-A-1", 1)]);
        assert!(part.starts_with("// "), "{part}");
        assert!(
            part.contains("output \"eDP-1\" {\n    scale 2\n}\n"),
            "{part}"
        );
        assert!(
            part.contains("output \"HDMI-A-1\" {\n    scale 1\n}\n"),
            "{part}"
        );
        // no screens is a part with nothing in it, which is what the compositor already does
        assert_eq!(horizon(&[]).lines().count(), 1);
    }

    #[test]
    fn a_scale_outside_what_a_screen_is_drawn_at_is_brought_back() {
        assert!(horizon(&[screen("eDP-1", 0)]).contains("scale 1"));
        assert!(horizon(&[screen("eDP-1", 40)]).contains("scale 3"));
    }
}
