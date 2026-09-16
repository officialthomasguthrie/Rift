//! `UPower` from a client's side: the battery, as the bar's icon and the system menu show it. `UPower`
//! adds every battery of the machine up into one display device, which is what a desktop shows.

#[cfg(feature = "bus")]
use crate::bus;

/// `UPower`'s name on the system bus.
pub const SERVICE: &str = "org.freedesktop.UPower";

/// What the battery is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Charge {
    /// On the charger and filling up.
    Charging,
    /// Running the machine.
    Discharging,
    /// On the charger and full.
    Full,
    /// Neither: on the charger and holding, or `UPower` does not know yet.
    Idle,
}

/// The battery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Battery {
    /// Percent of full.
    pub level: u8,
    /// What it is doing.
    pub charge: Charge,
    /// Seconds until it is empty while it discharges, or until it is full while it charges. 0 when
    /// `UPower` has no estimate yet.
    pub seconds: i64,
}

impl Battery {
    /// The display device's properties as `UPower` gives them: `Type`, `IsPresent`, `Percentage`,
    /// `State`, `TimeToEmpty` and `TimeToFull`. `None` when it is not a battery that is there.
    #[must_use]
    pub fn from_device(
        kind: u32,
        present: bool,
        percentage: f64,
        state: u32,
        to_empty: i64,
        to_full: i64,
    ) -> Option<Self> {
        // 2 is a battery that runs the machine; 0 is what the display device is with no battery
        if kind != 2 || !present || !percentage.is_finite() {
            return None;
        }
        let charge = match state {
            1 => Charge::Charging,
            2 => Charge::Discharging,
            4 => Charge::Full,
            _ => Charge::Idle,
        };
        let seconds = match charge {
            Charge::Charging => to_full,
            Charge::Discharging => to_empty,
            Charge::Full | Charge::Idle => 0,
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let level = percentage.clamp(0.0, 100.0).round() as u8;
        Some(Self {
            level,
            charge,
            seconds: seconds.max(0),
        })
    }

    /// How long it has, in words: `3 h 20 min left`, `45 min until full`, or `Fully charged`.
    /// `None` when there is nothing to say yet.
    #[must_use]
    pub fn time(&self) -> Option<String> {
        match self.charge {
            Charge::Full => Some("Fully charged".to_string()),
            Charge::Idle => None,
            Charge::Charging | Charge::Discharging if self.seconds < 60 => None,
            Charge::Charging => Some(format!("{} until full", span(self.seconds))),
            Charge::Discharging => Some(format!("{} left", span(self.seconds))),
        }
    }
}

/// A span of time the way a battery estimate is written: hours and minutes, rounded to the minute.
#[must_use]
pub fn span(seconds: i64) -> String {
    let minutes = (seconds.max(0) + 30) / 60;
    match (minutes / 60, minutes % 60) {
        (0, minutes) => format!("{minutes} min"),
        (hours, 0) => format!("{hours} h"),
        (hours, minutes) => format!("{hours} h {minutes} min"),
    }
}

/// The battery, or `None` when the machine runs without one.
///
/// # Errors
///
/// A sentence when `UPower` is not there or does not answer.
#[cfg(feature = "bus")]
pub fn read() -> Result<Option<Battery>, String> {
    use zbus::zvariant::OwnedValue;

    let connection = bus::connect(bus::PROPERTY_TIMEOUT)?;
    let mut device = bus::properties(
        &connection,
        SERVICE,
        "/org/freedesktop/UPower/devices/DisplayDevice",
        "org.freedesktop.UPower.Device",
    )
    .map_err(|e| bus::sentence_for("UPower", e))?;
    let mut take = |key: &str| device.remove(key);
    let kind = take("Type").and_then(|value| u32::try_from(value).ok());
    let present = take("IsPresent").and_then(|value| bool::try_from(value).ok());
    let percentage = take("Percentage").and_then(|value| f64::try_from(value).ok());
    let state = take("State").and_then(|value| u32::try_from(value).ok());
    let seconds = |value: Option<OwnedValue>| value.and_then(|value| i64::try_from(value).ok());
    let to_empty = seconds(take("TimeToEmpty"));
    let to_full = seconds(take("TimeToFull"));
    Ok(Battery::from_device(
        kind.unwrap_or(0),
        present.unwrap_or(false),
        percentage.unwrap_or(0.0),
        state.unwrap_or(0),
        to_empty.unwrap_or(0),
        to_full.unwrap_or(0),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_display_device_is_a_battery_only_when_one_is_there() {
        assert_eq!(Battery::from_device(0, false, 0.0, 0, 0, 0), None);
        assert_eq!(Battery::from_device(2, false, 50.0, 2, 3600, 0), None);
        // a mouse or a phone is not the machine's battery
        assert_eq!(Battery::from_device(5, true, 50.0, 2, 3600, 0), None);
        assert_eq!(
            Battery::from_device(2, true, 71.6, 2, 12_000, 0),
            Some(Battery {
                level: 72,
                charge: Charge::Discharging,
                seconds: 12_000
            })
        );
        let charging = Battery::from_device(2, true, 40.0, 1, 0, 2_700).expect("the battery");
        assert_eq!(charging.charge, Charge::Charging);
        assert_eq!(charging.seconds, 2_700);
    }

    #[test]
    fn the_time_left_reads_as_words() {
        let at = |charge, seconds| Battery {
            level: 50,
            charge,
            seconds,
        };
        assert_eq!(
            at(Charge::Discharging, 12_000).time().as_deref(),
            Some("3 h 20 min left")
        );
        assert_eq!(
            at(Charge::Charging, 2_700).time().as_deref(),
            Some("45 min until full")
        );
        assert_eq!(at(Charge::Full, 0).time().as_deref(), Some("Fully charged"));
        // no estimate yet
        assert_eq!(at(Charge::Discharging, 0).time(), None);
        assert_eq!(at(Charge::Idle, 5_000).time(), None);
    }

    #[test]
    fn a_span_is_hours_and_minutes() {
        assert_eq!(span(60), "1 min");
        assert_eq!(span(3_600), "1 h");
        assert_eq!(span(3_629), "1 h");
        assert_eq!(span(3_630), "1 h 1 min");
        assert_eq!(span(-5), "0 min");
    }
}
