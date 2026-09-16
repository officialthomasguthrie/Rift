//! The clock in the middle of the bar: the weekday, the day, the month and 24 hour time, in the
//! machine's own zone. `date` reads /etc/localtime and knows the locale, and it is asked once a
//! minute, on the minute, so nothing in the bar redraws between ticks.

use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How the bar writes the time: `Mon 14 Sep 20:41`.
const FORMAT: &str = "+%a %-d %b %H:%M";

/// What the clock says now, or an empty line when `date` is not there to ask.
#[must_use]
pub fn now() -> String {
    let Ok(output) = Command::new("date").arg(FORMAT).output() else {
        return String::new();
    };
    if !output.status.success() {
        return String::new();
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// How long until the next minute starts. A tick lands a little after the turn, so `date` never
/// reads the minute that just ended.
#[must_use]
pub fn until_next_minute() -> Duration {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    until(seconds)
}

fn until(seconds: u64) -> Duration {
    Duration::from_secs(60 - seconds % 60) + Duration::from_millis(200)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tick_lands_just_after_the_turn_of_the_minute() {
        assert_eq!(until(0), Duration::from_millis(60_200));
        assert_eq!(until(59), Duration::from_millis(1_200));
        assert_eq!(until(1_789_221_603), Duration::from_millis(57_200));
        // never zero, so the thread cannot spin
        for second in 0..120 {
            assert!(until(second) >= Duration::from_millis(1_200));
            assert!(until(second) <= Duration::from_millis(60_200));
        }
    }
}
