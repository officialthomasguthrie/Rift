//! The clock in the middle of the bar: the weekday, the day, the month and 24 hour time, in the
//! machine's own zone. `date` reads /etc/localtime and knows the locale, and it is asked once a
//! minute, on the minute, so nothing in the bar redraws between ticks. The same call says which
//! day it is, for the calendar in the clock menu.

use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::calendar::{self, Day, Weekday};

/// How the bar writes the time, `Mon 14 Sep 20:41`, and on a line of its own the day for the
/// calendar, `2026 09 14`.
const FORMAT: &str = "+%a %-d %b %H:%M%n%Y %m %d";

/// What the clock says now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Now {
    /// The bar's line, or an empty line when `date` is not there to ask.
    pub line: String,
    /// Today, when `date` said.
    pub today: Option<Day>,
}

/// What the clock says now.
#[must_use]
pub fn now() -> Now {
    let Ok(output) = Command::new("date").arg(FORMAT).output() else {
        return Now::default();
    };
    if !output.status.success() {
        return Now::default();
    }
    read(&String::from_utf8_lossy(&output.stdout))
}

/// The two lines `date` printed.
fn read(printed: &str) -> Now {
    let mut lines = printed.lines();
    Now {
        line: lines.next().unwrap_or_default().trim().to_string(),
        today: lines.next().and_then(Day::parse),
    }
}

/// The minute out of the bar's line, `20:41`, for the time a notification came in.
#[must_use]
pub fn minute(line: &str) -> &str {
    line.rsplit(' ').next().unwrap_or_default()
}

/// The day the locale starts its weeks on, which `locale` says, or Monday when it cannot.
#[must_use]
pub fn first_weekday() -> Weekday {
    Command::new("locale")
        .args(["week-1stday", "first_weekday"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| calendar::first_weekday(&String::from_utf8_lossy(&output.stdout)))
        .unwrap_or(calendar::MONDAY)
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

    #[test]
    fn date_says_the_time_and_the_day() {
        let now = read("Thu 17 Sep 07:41\n2026 09 17\n");
        assert_eq!(now.line, "Thu 17 Sep 07:41");
        assert_eq!(
            now.today,
            Some(Day {
                year: 2026,
                month: 9,
                date: 17
            })
        );
        assert_eq!(minute(&now.line), "07:41");
        // a date that printed one line still gives the bar its line
        let older = read("Thu 17 Sep 07:41\n");
        assert_eq!(older.line, "Thu 17 Sep 07:41");
        assert_eq!(older.today, None);
        assert_eq!(minute(""), "");
    }
}
