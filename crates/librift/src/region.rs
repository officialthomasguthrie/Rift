//! The language, the formats and the keyboard from a client's side: what systemd's localed says
//! the system is set to, and what a locale means in words, out of the C library's own data about
//! it. The Region and language page and the Keyboard page ask it here; [`crate::keyboard`] names
//! the layouts and changes them.

use std::process::Command;

#[cfg(feature = "bus")]
use crate::bus;

/// localed's name on the system bus, which is also the name of its interface.
pub const SERVICE: &str = "org.freedesktop.locale1";
/// Its one object.
#[cfg(feature = "bus")]
const OBJECT: &str = "/org/freedesktop/locale1";
/// The name a person knows it by, for the sentences.
#[cfg(feature = "bus")]
const NAME: &str = "localed";

/// What localed says the system is set to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Region {
    /// The locale, as the variables it sets: `LANG=en_GB.UTF-8`, and any `LC_` beside it.
    pub locale: Vec<String>,
    /// The keymap of the text console, where the passphrase is typed at the start: `us`.
    pub keymap: String,
    /// The layout the desktop is told to use, `gb`. Empty when nothing names one.
    pub layout: String,
    /// Its variant, `dvorak`. Usually empty.
    pub variant: String,
    /// The keyboard model the desktop is told about, `pc105`. Usually empty.
    pub model: String,
    /// The XKB options, `ctrl:nocaps`. Usually empty.
    pub options: String,
}

impl Region {
    /// The value one variable of the locale is set to.
    #[must_use]
    pub fn value(&self, name: &str) -> Option<&str> {
        self.locale
            .iter()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
            .filter(|value| !value.is_empty())
    }

    /// The locale messages are in: `LC_MESSAGES` when it is set, else `LANG`.
    #[must_use]
    pub fn language(&self) -> Option<&str> {
        self.value("LC_MESSAGES").or_else(|| self.value("LANG"))
    }

    /// The locale dates and times are written in: `LC_TIME` when it is set, else `LANG`.
    #[must_use]
    pub fn formats(&self) -> Option<&str> {
        self.value("LC_TIME").or_else(|| self.value("LANG"))
    }
}

/// What localed says the system is set to.
///
/// # Errors
///
/// A sentence when the bus or localed does not answer.
#[cfg(feature = "bus")]
pub fn read() -> Result<Region, String> {
    let connection = bus::connect(bus::PROPERTY_TIMEOUT)?;
    let mut said = bus::properties(&connection, SERVICE, OBJECT, SERVICE)
        .map_err(|e| bus::sentence_for(NAME, e))?;
    let mut text = |key: &str| {
        said.remove(key)
            .and_then(|value| String::try_from(value).ok())
            .unwrap_or_default()
    };
    let keymap = text("VConsoleKeymap");
    let layout = text("X11Layout");
    let variant = text("X11Variant");
    let model = text("X11Model");
    let options = text("X11Options");
    let locale = said
        .remove("Locale")
        .and_then(|value| Vec::<String>::try_from(value).ok())
        .unwrap_or_default();
    Ok(Region {
        locale,
        keymap,
        layout,
        variant,
        model,
        options,
    })
}

/// What the C library knows about one locale, out of `locale -k`, and a date and a time written
/// the way it writes them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Described {
    /// The language, `English`. Empty for a locale that names none, like C.
    pub language: String,
    /// The country, `United Kingdom`. Empty the same way.
    pub country: String,
    /// Today's date, `21/09/26`.
    pub date: String,
    /// The time now, `14:05:00`.
    pub time: String,
    /// The width and the height of the paper, in millimetres.
    pub paper: (u32, u32),
    /// Whether measurements are metric.
    pub metric: bool,
    /// The weekday a week starts on, 0 for Sunday.
    pub first: u8,
}

/// The keywords `locale -k` is asked for.
const KEYWORDS: [&str; 7] = [
    "lang_name",
    "country_name",
    "width",
    "height",
    "measurement",
    "week-1stday",
    "first_weekday",
];

/// The names of the weekdays, from Sunday.
const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

impl Described {
    /// What `locale -k` printed for [`KEYWORDS`], and the two lines `date '+%x%n%X'` printed. `None`
    /// when either is not what they print.
    #[must_use]
    pub fn read(keywords: &str, example: &str) -> Option<Self> {
        let value = |key: &str| {
            keywords.lines().find_map(|line| {
                let (name, value) = line.trim().split_once('=')?;
                (name == key).then(|| value.trim().trim_matches('"').to_string())
            })
        };
        let number = |key: &str| value(key)?.parse::<u32>().ok();
        let mut lines = example.lines().map(str::trim);
        Some(Self {
            language: value("lang_name")?,
            country: value("country_name")?,
            date: lines.next()?.to_string(),
            time: lines.next()?.to_string(),
            paper: (number("width")?, number("height")?),
            // 1 is metric, 2 is the inches and pounds of the United States
            metric: number("measurement")? != 2,
            first: first_weekday(&value("week-1stday")?, number("first_weekday")?)?,
        })
    }

    /// The language and the country the way a person says them: `English (United Kingdom)`.
    /// `None` for a locale that names neither.
    #[must_use]
    pub fn name(&self) -> Option<String> {
        match (self.language.as_str(), self.country.as_str()) {
            ("", "") => None,
            (language, "") => Some(language.to_string()),
            ("", country) => Some(country.to_string()),
            (language, country) => Some(format!("{language} ({country})")),
        }
    }

    /// The paper by the name it is sold under, or its size.
    #[must_use]
    pub fn paper_name(&self) -> String {
        match self.paper {
            (210, 297) => "A4".to_string(),
            (216, 279) => "Letter".to_string(),
            (width, height) => format!("{width} by {height} mm"),
        }
    }

    /// The weekday a week starts on.
    #[must_use]
    pub fn first_day(&self) -> &'static str {
        WEEKDAYS[usize::from(self.first % 7)]
    }

    /// What the formats mean, in two sentences.
    #[must_use]
    pub fn sentence(&self) -> String {
        let measures = if self.metric {
            "measurements are metric"
        } else {
            "measurements are in inches and pounds"
        };
        format!(
            "Dates are written {} and times {}. Weeks start on {}, paper is {} and {measures}.",
            self.date,
            self.time,
            self.first_day(),
            self.paper_name()
        )
    }
}

/// The weekday a locale starts its weeks on: `locale` names a day it counts from, `19971130`, and
/// which day after it is the first, where 1 is that day itself.
fn first_weekday(counted: &str, which: u32) -> Option<u8> {
    if counted.len() != 8 || !counted.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if !(1..=7).contains(&which) {
        return None;
    }
    let year: i64 = counted[..4].parse().ok()?;
    let month: i64 = counted[4..6].parse().ok()?;
    let day: i64 = counted[6..].parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    u8::try_from((weekday(year, month, day) + i64::from(which) - 1).rem_euclid(7)).ok()
}

/// The weekday of a date, 0 for Sunday, by Sakamoto's method.
fn weekday(year: i64, month: i64, day: i64) -> i64 {
    const OFFSETS: [i64; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let year = if month < 3 { year - 1 } else { year };
    let at = usize::try_from(month - 1).unwrap_or(0);
    (year + year / 4 - year / 100 + year / 400 + OFFSETS[at] + day).rem_euclid(7)
}

/// What the C library knows about one locale. `None` where `locale` or `date` is not there, and
/// for a locale this system cannot load, which the two would describe as C.
#[must_use]
pub fn describe(locale: &str) -> Option<Described> {
    let ask = |program: &str, args: &[&str]| {
        Command::new(program)
            .args(args)
            .env("LC_ALL", locale)
            .output()
            .ok()
            .filter(|output| output.status.success() && output.stderr.is_empty())
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
    };
    let keywords = ask("locale", &[&["-k"][..], &KEYWORDS[..]].concat())?;
    let example = ask("date", &["+%x%n%X"])?;
    Described::read(&keywords, &example)
}

/// A keymap of the text console the way a person says it.
#[must_use]
pub fn keymap_words(keymap: &str) -> String {
    match keymap {
        "" | "us" => "English (US)".to_string(),
        "uk" => "English (UK)".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `locale -k` prints for the keywords under en_GB.UTF-8.
    const BRITISH: &str = "lang_name=\"English\"\n\
        country_name=\"United Kingdom\"\n\
        width=210\n\
        height=297\n\
        measurement=1\n\
        week-1stday=19971130\n\
        first_weekday=2\n";
    /// And under en_US.UTF-8.
    const AMERICAN: &str = "lang_name=\"English\"\n\
        country_name=\"United States\"\n\
        width=216\n\
        height=279\n\
        measurement=2\n\
        week-1stday=19971130\n\
        first_weekday=1\n";
    /// And under C.UTF-8, which names no language and no country.
    const PLAIN: &str = "lang_name=\"\"\n\
        country_name=\"\"\n\
        width=210\n\
        height=297\n\
        measurement=1\n\
        week-1stday=19971130\n\
        first_weekday=2\n";

    #[test]
    fn british_english_is_day_first_a_monday_a4_and_metric() {
        let british = Described::read(BRITISH, "21/09/26\n14:05:00\n").expect("described");
        assert_eq!(british.name().as_deref(), Some("English (United Kingdom)"));
        assert_eq!(british.paper_name(), "A4");
        assert_eq!(british.first_day(), "Monday");
        assert!(british.metric);
        assert_eq!(
            british.sentence(),
            "Dates are written 21/09/26 and times 14:05:00. Weeks start on Monday, paper is A4 and \
             measurements are metric."
        );
    }

    #[test]
    fn american_english_is_a_sunday_letter_and_inches() {
        let american = Described::read(AMERICAN, "09/21/2026\n02:05:00 PM\n").expect("described");
        assert_eq!(american.name().as_deref(), Some("English (United States)"));
        assert_eq!(american.paper_name(), "Letter");
        assert_eq!(american.first_day(), "Sunday");
        assert!(
            american
                .sentence()
                .ends_with("measurements are in inches and pounds.")
        );
    }

    #[test]
    fn the_c_locale_has_no_name() {
        let plain = Described::read(PLAIN, "09/21/26\n14:05:00\n").expect("described");
        assert_eq!(plain.name(), None);
        assert_eq!(
            Described::read("lang_name=\"English\"\n", "21/09/26\n14:05:00\n"),
            None
        );
        assert_eq!(Described::read(BRITISH, "21/09/26\n"), None);
    }

    #[test]
    fn a_week_starts_on_the_day_after_the_one_counted_from() {
        // 30 November 1997 was a Sunday, and 1 December 1997 a Monday
        assert_eq!(first_weekday("19971130", 1), Some(0));
        assert_eq!(first_weekday("19971130", 2), Some(1));
        assert_eq!(first_weekday("19971201", 1), Some(1));
        assert_eq!(first_weekday("19971130", 7), Some(6));
        assert_eq!(first_weekday("1997113", 1), None);
        assert_eq!(first_weekday("19971130", 0), None);
        assert_eq!(first_weekday("19971330", 1), None);
        // 21 September 2026, the day this was written, is a Monday
        assert_eq!(weekday(2026, 9, 21), 1);
        assert_eq!(weekday(2000, 1, 1), 6);
    }

    #[test]
    fn the_locale_is_read_one_variable_at_a_time() {
        let region = Region {
            locale: vec![
                "LANG=en_GB.UTF-8".to_string(),
                "LC_TIME=en_US.UTF-8".to_string(),
                "LC_PAPER=".to_string(),
            ],
            ..Region::default()
        };
        assert_eq!(region.value("LANG"), Some("en_GB.UTF-8"));
        assert_eq!(region.language(), Some("en_GB.UTF-8"));
        assert_eq!(region.formats(), Some("en_US.UTF-8"));
        // a variable set to nothing, or not set, is not a value
        assert_eq!(region.value("LC_PAPER"), None);
        assert_eq!(region.value("LC_NUMERIC"), None);
        // and a variable whose name starts with another's is not that one
        assert_eq!(region.value("LAN"), None);
        assert_eq!(Region::default().language(), None);
    }

    #[test]
    fn a_keymap_reads_as_words() {
        assert_eq!(keymap_words(""), "English (US)");
        assert_eq!(keymap_words("us"), "English (US)");
        assert_eq!(keymap_words("uk"), "English (UK)");
        assert_eq!(keymap_words("fr"), "fr");
    }
}
