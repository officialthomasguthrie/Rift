//! The month the clock menu shows: which weekday a day falls on, how long a month is, and the six
//! weeks of days a month is laid out in, from the day the locale starts its weeks on. Plain
//! arithmetic on the proleptic Gregorian calendar, the one `date` uses.

// only the clock menu lays a month out, and the menu is linux only
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

/// The weekdays, Monday first, as the calendar heads its columns.
const LETTERS: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];

/// The months, as the calendar heads a page.
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// A weekday, Monday first: 0 is Monday and 6 is Sunday.
pub type Weekday = u8;

/// Monday, the day a week starts on when the locale does not say.
pub const MONDAY: Weekday = 0;

/// One day of the calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Day {
    /// The year.
    pub year: i32,
    /// The month, 1 to 12.
    pub month: u8,
    /// The day of the month, from 1.
    pub date: u8,
}

impl Day {
    /// Read `2026 09 17`, which is how `date +'%Y %m %d'` prints a day. Anything that is not a day
    /// of the calendar is `None`.
    #[must_use]
    pub fn parse(printed: &str) -> Option<Self> {
        let mut words = printed.split_whitespace();
        let year = words.next()?.parse().ok()?;
        let month = words.next()?.parse().ok()?;
        let date = words.next()?.parse().ok()?;
        if words.next().is_some() {
            return None;
        }
        let found = Self { year, month, date };
        let month_days = Month { year, month }.days();
        (month_days > 0 && (1..=month_days).contains(&date)).then_some(found)
    }

    /// How many days this day is after the first of January 1970, or before it when negative.
    #[must_use]
    pub fn number(self) -> i64 {
        // Howard Hinnant's days from civil: the year starts in March, so the leap day is the last
        // day of the year before
        let year = i64::from(self.year) - i64::from(self.month <= 2);
        let era = year.div_euclid(400);
        let of_era = year.rem_euclid(400);
        let from_march = (i64::from(self.month) + 9) % 12;
        let of_year = (153 * from_march + 2) / 5 + i64::from(self.date) - 1;
        let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
        era * 146_097 + of_cycle - 719_468
    }

    /// The day this many days after the first of January 1970.
    #[must_use]
    pub fn from_number(number: i64) -> Self {
        let shifted = number + 719_468;
        let era = shifted.div_euclid(146_097);
        let of_cycle = shifted.rem_euclid(146_097);
        let of_era = (of_cycle - of_cycle / 1460 + of_cycle / 36_524 - of_cycle / 146_096) / 365;
        let of_year = of_cycle - (365 * of_era + of_era / 4 - of_era / 100);
        let from_march = (5 * of_year + 2) / 153;
        let day = of_year - (153 * from_march + 2) / 5 + 1;
        let month = if from_march < 10 {
            from_march + 3
        } else {
            from_march - 9
        };
        let year = of_era + era * 400 + i64::from(month <= 2);
        Self {
            year: i32::try_from(year).unwrap_or(i32::MAX),
            month: u8::try_from(month).unwrap_or(1),
            date: u8::try_from(day).unwrap_or(1),
        }
    }

    /// The weekday this day falls on. The first of January 1970 was a Thursday.
    #[must_use]
    pub fn weekday(self) -> Weekday {
        u8::try_from((self.number() + 3).rem_euclid(7)).unwrap_or(MONDAY)
    }
}

/// One month of one year: a page of the calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Month {
    /// The year.
    pub year: i32,
    /// The month, 1 to 12.
    pub month: u8,
}

impl Month {
    /// The month a day is in.
    #[must_use]
    pub const fn of(day: Day) -> Self {
        Self {
            year: day.year,
            month: day.month,
        }
    }

    /// The month after this one.
    #[must_use]
    pub const fn next(self) -> Self {
        if self.month >= 12 {
            Self {
                year: self.year.saturating_add(1),
                month: 1,
            }
        } else {
            Self {
                year: self.year,
                month: self.month + 1,
            }
        }
    }

    /// The month before this one.
    #[must_use]
    pub const fn previous(self) -> Self {
        if self.month <= 1 {
            Self {
                year: self.year.saturating_sub(1),
                month: 12,
            }
        } else {
            Self {
                year: self.year,
                month: self.month - 1,
            }
        }
    }

    /// How many days the month has, or none when it is not a month.
    #[must_use]
    pub const fn days(self) -> u8 {
        match self.month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if self.year % 4 == 0 && (self.year % 100 != 0 || self.year % 400 == 0) => 29,
            2 => 28,
            _ => 0,
        }
    }

    /// The name over the page: `September 2026`.
    #[must_use]
    pub fn name(self) -> String {
        let name = MONTHS
            .get(usize::from(self.month.max(1) - 1))
            .copied()
            .unwrap_or_default();
        format!("{name} {}", self.year)
    }

    /// The six weeks of days the page lays out, row by row, starting on the weekday `first`: the
    /// end of the month before, the month itself, and the start of the one after. Six weeks is the
    /// most a month needs, and every page has them all so the menu does not change its height.
    #[must_use]
    pub fn weeks(self, first: Weekday) -> [Day; 42] {
        let start = Day {
            year: self.year,
            month: self.month,
            date: 1,
        };
        let before = i64::from((start.weekday() + 7 - first % 7) % 7);
        let from = start.number() - before;
        let mut days = [start; 42];
        for (offset, day) in (0_i64..).zip(days.iter_mut()) {
            *day = Day::from_number(from + offset);
        }
        days
    }
}

/// The letters over the columns, from the weekday the weeks start on.
#[must_use]
pub fn letters(first: Weekday) -> [&'static str; 7] {
    let mut found = LETTERS;
    found.rotate_left(usize::from(first % 7));
    found
}

/// The weekday the locale starts its weeks on, from what `locale week-1stday first_weekday`
/// prints: a day the locale counts from, then which day after it is the first, where 1 is that
/// day itself. `en_US` counts from a Sunday and says 1, `en_GB` from the same Sunday and says 2,
/// `de_DE` from a Monday and says 1.
#[must_use]
pub fn first_weekday(printed: &str) -> Option<Weekday> {
    let mut lines = printed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let counted = lines.next()?;
    let which: i64 = lines.next()?.parse().ok()?;
    if counted.len() != 8 || !counted.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let from = Day::parse(&format!(
        "{} {} {}",
        &counted[..4],
        &counted[4..6],
        &counted[6..]
    ))?;
    if !(1..=7).contains(&which) {
        return None;
    }
    let weekday = (i64::from(from.weekday()) + which - 1).rem_euclid(7);
    u8::try_from(weekday).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn day(year: i32, month: u8, date: u8) -> Day {
        Day { year, month, date }
    }

    #[test]
    fn days_count_from_the_start_of_1970() {
        assert_eq!(day(1970, 1, 1).number(), 0);
        assert_eq!(day(1970, 1, 2).number(), 1);
        assert_eq!(day(1969, 12, 31).number(), -1);
        assert_eq!(day(2000, 3, 1).number(), 11_017);
        assert_eq!(day(2026, 9, 17).number(), 20_713);
        for number in [-800_000, -1, 0, 59, 60, 11_016, 20_713, 2_932_896] {
            assert_eq!(Day::from_number(number).number(), number, "{number}");
        }
        assert_eq!(Day::from_number(11_016), day(2000, 2, 29));
    }

    #[test]
    fn a_day_knows_its_weekday() {
        // a Thursday, a Thursday, a Sunday and a Monday
        assert_eq!(day(1970, 1, 1).weekday(), 3);
        assert_eq!(day(2026, 9, 17).weekday(), 3);
        assert_eq!(day(1997, 11, 30).weekday(), 6);
        assert_eq!(day(1997, 12, 1).weekday(), 0);
        assert_eq!(day(1900, 1, 1).weekday(), 0);
    }

    #[test]
    fn a_printed_day_reads_back_only_when_it_is_one() {
        assert_eq!(Day::parse("2026 09 17\n"), Some(day(2026, 9, 17)));
        assert_eq!(Day::parse("2024 02 29"), Some(day(2024, 2, 29)));
        assert_eq!(Day::parse("2026 02 29"), None);
        assert_eq!(Day::parse("2026 13 01"), None);
        assert_eq!(Day::parse("2026 09 00"), None);
        assert_eq!(Day::parse("2026 09"), None);
        assert_eq!(Day::parse("2026 09 17 20"), None);
        assert_eq!(Day::parse("Thu 17 Sep"), None);
    }

    #[test]
    fn months_have_their_lengths_and_turn_over_at_the_year() {
        assert_eq!(
            Month {
                year: 2026,
                month: 9
            }
            .days(),
            30
        );
        assert_eq!(
            Month {
                year: 2024,
                month: 2
            }
            .days(),
            29
        );
        assert_eq!(
            Month {
                year: 1900,
                month: 2
            }
            .days(),
            28
        );
        assert_eq!(
            Month {
                year: 2000,
                month: 2
            }
            .days(),
            29
        );
        assert_eq!(
            Month {
                year: 2026,
                month: 0
            }
            .days(),
            0
        );
        let december = Month {
            year: 2026,
            month: 12,
        };
        assert_eq!(
            december.next(),
            Month {
                year: 2027,
                month: 1
            }
        );
        assert_eq!(december.next().previous(), december);
        assert_eq!(Month::of(day(2026, 9, 17)).name(), "September 2026");
    }

    #[test]
    fn a_page_starts_on_the_first_day_of_the_week() {
        let september = Month {
            year: 2026,
            month: 9,
        };
        // the first of September 2026 is a Tuesday
        let monday_first = september.weeks(MONDAY);
        assert_eq!(monday_first[0], day(2026, 8, 31));
        assert_eq!(monday_first[1], day(2026, 9, 1));
        assert_eq!(monday_first[30], day(2026, 9, 30));
        assert_eq!(monday_first[41], day(2026, 10, 11));
        let sunday_first = september.weeks(6);
        assert_eq!(sunday_first[0], day(2026, 8, 30));
        assert_eq!(sunday_first[2], day(2026, 9, 1));
        // a month that starts on the first day of the week starts the page
        let june = Month {
            year: 2026,
            month: 6,
        };
        assert_eq!(june.weeks(MONDAY)[0], day(2026, 6, 1));
        for (at, pair) in monday_first.windows(2).enumerate() {
            assert_eq!(pair[1].number() - pair[0].number(), 1, "at {at}");
        }
        assert!(
            monday_first
                .iter()
                .step_by(7)
                .all(|day| day.weekday() == MONDAY)
        );
    }

    #[test]
    fn the_letters_follow_the_first_day() {
        assert_eq!(letters(MONDAY), ["M", "T", "W", "T", "F", "S", "S"]);
        assert_eq!(letters(6), ["S", "M", "T", "W", "T", "F", "S"]);
    }

    #[test]
    fn the_locale_says_which_day_starts_the_week() {
        assert_eq!(first_weekday("19971130\n1\n"), Some(6));
        assert_eq!(first_weekday("19971130\n2\n"), Some(MONDAY));
        assert_eq!(first_weekday("19971201\n1\n"), Some(MONDAY));
        assert_eq!(first_weekday("19971130\n7\n"), Some(5));
        assert_eq!(first_weekday("19971130\n"), None);
        assert_eq!(first_weekday("19971130\n9\n"), None);
        assert_eq!(first_weekday("locale: not found\n1\n"), None);
    }
}
