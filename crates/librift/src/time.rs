//! Times the way a person says them. `rift snapshot` says how long ago the last snapshot was
//! taken and the Search page how long ago the index was written, in the same words.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since 1970, now. 0 on a clock that is before then.
#[must_use]
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_secs()).ok())
        .unwrap_or(0)
}

/// A span of seconds the way a person says it about the past. A span that has not happened yet,
/// which a clock that was put back gives, is just now.
#[must_use]
pub fn ago(seconds: i64) -> String {
    let (count, unit) = match seconds {
        ..60 => return "just now".to_string(),
        60..3600 => (seconds / 60, "minute"),
        3600..172_800 => (seconds / 3600, "hour"),
        _ => (seconds / 86_400, "day"),
    };
    let plural = if count == 1 { "" } else { "s" };
    format!("{count} {unit}{plural} ago")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_span_reads_in_the_largest_unit_that_fits() {
        assert_eq!(ago(0), "just now");
        assert_eq!(ago(59), "just now");
        assert_eq!(ago(60), "1 minute ago");
        assert_eq!(ago(3599), "59 minutes ago");
        assert_eq!(ago(3600), "1 hour ago");
        assert_eq!(ago(172_799), "47 hours ago");
        assert_eq!(ago(172_800), "2 days ago");
        assert_eq!(ago(86_400 * 30), "30 days ago");
    }

    #[test]
    fn a_clock_that_went_back_says_just_now() {
        assert_eq!(ago(-1), "just now");
        assert_eq!(ago(-86_400), "just now");
    }

    #[test]
    fn the_clock_is_past_the_day_this_was_written() {
        // 2026-09-21, the day the module was written, in seconds since 1970
        assert!(now() > 1_789_000_000);
    }
}
