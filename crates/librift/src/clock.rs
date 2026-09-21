//! The clock and the time zone from a client's side: what systemd's timedated says about them, the
//! time zone the owner sets through it, the zones there are to choose from, out of the time zone
//! database the image carries, and what the clock reads now. The Date and time page asks all of it
//! here.

use std::collections::{HashMap, HashSet};

#[cfg(feature = "bus")]
use crate::bus;

/// timedated's name on the system bus, which is also the name of its interface.
pub const SERVICE: &str = "org.freedesktop.timedate1";
/// Its one object.
#[cfg(feature = "bus")]
const OBJECT: &str = "/org/freedesktop/timedate1";
/// The name a person knows it by, for the sentences.
#[cfg(feature = "bus")]
const NAME: &str = "timedated";
/// Where the image keeps the time zone database.
pub const ZONEINFO: &str = "/etc/zoneinfo";
/// The zone of a drive where no other was chosen.
pub const UTC: &str = "UTC";

/// Whether the network sets the clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    /// Nothing on this machine sets the clock from the network.
    Missing,
    /// Something could, and it is off.
    Off,
    /// It is on. Synchronized is whether a time server has set the clock since the machine started.
    On {
        /// Whether a time server has answered.
        synchronized: bool,
    },
}

impl Network {
    /// Out of timedated's three properties: `CanNTP`, `NTP` and `NTPSynchronized`.
    #[must_use]
    pub const fn from_properties(can: bool, on: bool, synchronized: bool) -> Self {
        match (can, on) {
            (false, _) => Self::Missing,
            (true, false) => Self::Off,
            (true, true) => Self::On { synchronized },
        }
    }
}

/// What timedated says about the clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clock {
    /// The time zone, `Pacific/Auckland`. Empty when timedated could not tell.
    pub zone: String,
    /// Whether the hardware clock keeps local time rather than UTC.
    pub local_rtc: bool,
    /// Whether the network sets the clock.
    pub network: Network,
}

/// What timedated says about the clock.
///
/// # Errors
///
/// A sentence when the bus or timedated does not answer.
#[cfg(feature = "bus")]
pub fn read() -> Result<Clock, String> {
    let connection = bus::connect(bus::PROPERTY_TIMEOUT)?;
    let mut said = bus::properties(&connection, SERVICE, OBJECT, SERVICE)
        .map_err(|e| bus::sentence_for(NAME, e))?;
    let mut flag = |key: &str| {
        said.remove(key)
            .and_then(|value| bool::try_from(value).ok())
            .unwrap_or(false)
    };
    let local_rtc = flag("LocalRTC");
    let network = Network::from_properties(flag("CanNTP"), flag("NTP"), flag("NTPSynchronized"));
    let zone = said
        .remove("Timezone")
        .and_then(|value| String::try_from(value).ok())
        .unwrap_or_default();
    Ok(Clock {
        zone,
        local_rtc,
        network,
    })
}

/// Set the time zone. timedated writes the link to it, which the image keeps on persist, and
/// everything that reads the time follows.
///
/// # Errors
///
/// A sentence when timedated does not know the zone, or will not let this account change it.
#[cfg(feature = "bus")]
pub fn set_zone(zone: &str) -> Result<(), String> {
    let connection = bus::connect(bus::PROPERTY_TIMEOUT)?;
    bus::object(&connection, SERVICE, OBJECT, SERVICE)
        .and_then(|proxy| proxy.call::<_, _, ()>("SetTimezone", &(zone, false)))
        .map_err(|e| {
            if bus::refused(&e) {
                "This account may not change the time zone without an administrator's password."
                    .to_string()
            } else {
                bus::sentence_for(NAME, e)
            }
        })
}

/// One time zone, with the words a person finds it by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone {
    /// Its name in the database, `Pacific/Auckland`.
    pub name: String,
    /// The place it is named after, `Auckland`.
    pub city: String,
    /// The country it is in, `New Zealand`, or nothing for a zone that is in none.
    pub country: String,
}

impl Zone {
    /// A zone known by its name alone: one the database lists in no country, like UTC.
    #[must_use]
    pub fn bare(name: &str) -> Self {
        Self {
            name: name.to_string(),
            city: city(name),
            country: String::new(),
        }
    }

    /// The zone the way a person says it: `Auckland, New Zealand`, or `UTC`.
    #[must_use]
    pub fn words(&self) -> String {
        if self.country.is_empty() {
            self.city.clone()
        } else {
            format!("{}, {}", self.city, self.country)
        }
    }

    /// Whether every word typed is in its name, its place or its country, whatever the case.
    #[must_use]
    pub fn matches(&self, typed: &str) -> bool {
        let known = format!(
            "{} {} {}",
            self.name.replace(['/', '_'], " "),
            self.city,
            self.country
        )
        .to_lowercase();
        let typed = typed.to_lowercase();
        let mut words = typed.split_whitespace().peekable();
        words.peek().is_some() && words.all(|word| known.contains(word))
    }
}

/// The place a zone is named after: the last part of its name, with spaces for the underscores.
fn city(name: &str) -> String {
    name.rsplit('/').next().unwrap_or(name).replace('_', " ")
}

/// The zones there are to choose from, in the order of their words: every zone `zone.tab` lists,
/// with its country out of `iso3166.tab`, and UTC. A zone listed for two countries is one zone,
/// under the first.
#[must_use]
pub fn zones(zone_tab: &str, iso3166: &str) -> Vec<Zone> {
    let countries: HashMap<String, String> = listed(iso3166)
        .into_iter()
        .filter_map(|fields| Some((fields.first()?.to_string(), fields.get(1)?.to_string())))
        .collect();
    let mut seen = HashSet::new();
    let mut found = vec![Zone::bare(UTC)];
    for fields in listed(zone_tab) {
        let (Some(code), Some(name)) = (fields.first(), fields.get(2)) else {
            continue;
        };
        if name.is_empty() || !seen.insert(name.to_string()) {
            continue;
        }
        found.push(Zone {
            country: countries.get(*code).cloned().unwrap_or_default(),
            ..Zone::bare(name)
        });
    }
    found.sort_by_cached_key(|zone| zone.words().to_lowercase());
    found
}

/// The rows of one of the database's tables, each split at its tabs, without the comments.
fn listed(text: &str) -> Vec<Vec<&str>> {
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| line.split('\t').map(str::trim).collect())
        .collect()
}

/// The zones there are to choose from, out of the database the image carries. Only UTC where it
/// cannot be read.
#[must_use]
pub fn installed() -> Vec<Zone> {
    let read = |name: &str| {
        std::fs::read_to_string(std::path::Path::new(ZONEINFO).join(name)).unwrap_or_default()
    };
    zones(&read("zone.tab"), &read("iso3166.tab"))
}

/// The zones a search finds: every one that matches, a place whose name starts with what was typed
/// first. Nothing when nothing was typed.
#[must_use]
pub fn find<'a>(zones: &'a [Zone], typed: &str) -> Vec<&'a Zone> {
    let typed = typed.trim().to_lowercase();
    if typed.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<&Zone> = zones.iter().filter(|zone| zone.matches(&typed)).collect();
    // the sort is stable, so each half keeps the order of the words
    found.sort_by_key(|zone| !zone.city.to_lowercase().starts_with(&typed));
    found
}

/// The zone with this name out of the list, or one made from the name alone, for a zone set
/// somewhere else that the list does not have.
#[must_use]
pub fn named(zones: &[Zone], name: &str) -> Zone {
    zones
        .iter()
        .find(|zone| zone.name == name)
        .cloned()
        .unwrap_or_else(|| Zone::bare(name))
}

/// How `date` is asked what the clock reads, a line each: the time, the date in words, the date
/// in digits, what the zone is called now and how far it is from UTC.
const NOW: &str = "+%H:%M%n%A %-d %B %Y%n%F%n%Z%n%:z";

/// What the clock reads now, in the zone that is set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Now {
    /// The time, `14:05`.
    pub time: String,
    /// The date the way a person reads it, `Monday 21 September 2026`.
    pub date: String,
    /// The same date in digits, `2026-09-21`.
    pub day: String,
    /// What the zone is called at this time of year, `NZST`.
    pub abbreviation: String,
    /// How far it is from UTC, `+12:00`.
    pub offset: String,
}

impl Now {
    /// The five lines `date` printed.
    #[must_use]
    pub fn read(printed: &str) -> Option<Self> {
        let mut lines = printed.lines().map(|line| line.trim().to_string());
        let now = Self {
            time: lines.next()?,
            date: lines.next()?,
            day: lines.next()?,
            abbreviation: lines.next()?,
            offset: lines.next()?,
        };
        (!now.time.is_empty() && !now.day.is_empty()).then_some(now)
    }

    /// What the zone is called now and how far it is from UTC: `NZST, UTC+12:00`. A zone with no
    /// name of its own for the time of year, which `date` prints as its offset, is only the
    /// offset, and UTC is only UTC.
    #[must_use]
    pub fn zone_words(&self) -> String {
        let offset = format!("UTC{}", self.offset);
        let abbreviation = self.abbreviation.as_str();
        if abbreviation == UTC && self.offset == "+00:00" {
            UTC.to_string()
        } else if abbreviation.is_empty() || abbreviation.starts_with(['+', '-']) {
            offset
        } else {
            format!("{abbreviation}, {offset}")
        }
    }
}

/// What the clock reads now, from `date`, which reads the zone that is set every time it runs.
/// `None` where `date` is not there to ask.
#[must_use]
pub fn now() -> Option<Now> {
    crate::os::ask("date", &[NOW]).and_then(|printed| Now::read(&printed))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZONE_TAB: &str = "# tzdb timezone descriptions (deprecated version)\n\
        #\n\
        GB\t+513030-0000731\tEurope/London\n\
        NZ\t-3652+17446\tPacific/Auckland\tmost of New Zealand\n\
        NZ\t-4357-17633\tPacific/Chatham\tChatham Islands\n\
        US\t+404251-0740023\tAmerica/New_York\tEastern (most areas)\n\
        AR\t-3436-05827\tAmerica/Argentina/Buenos_Aires\tBuenos Aires (BA, CF)\n\
        GG\t+492717-0023210\tEurope/Guernsey\n\
        IM\t+5409-00428\tEurope/Isle_of_Man\n\
        BR\t-0308-06001\tAmerica/Manaus\tAmazonas (east)\n\
        XX\t+0000+00000\tEurope/London\n";
    const ISO3166: &str = "# ISO 3166 alpha-2 country codes\n\
        AR\tArgentina\n\
        BR\tBrazil\n\
        GB\tBritain (UK)\n\
        GG\tGuernsey\n\
        IM\tIsle of Man\n\
        NZ\tNew Zealand\n\
        US\tUnited States\n";

    fn listed() -> Vec<Zone> {
        zones(ZONE_TAB, ISO3166)
    }

    fn names(found: &[&Zone]) -> Vec<String> {
        found.iter().map(|zone| zone.name.clone()).collect()
    }

    #[test]
    fn a_zone_reads_as_its_place_and_its_country() {
        let zones = listed();
        let auckland = named(&zones, "Pacific/Auckland");
        assert_eq!(auckland.words(), "Auckland, New Zealand");
        assert_eq!(
            named(&zones, "America/Argentina/Buenos_Aires").words(),
            "Buenos Aires, Argentina"
        );
        assert_eq!(named(&zones, UTC).words(), "UTC");
        // a zone the list does not have is still a zone, known by its name
        let bare = named(&zones, "Etc/GMT+5");
        assert_eq!((bare.city.as_str(), bare.country.as_str()), ("GMT+5", ""));
    }

    #[test]
    fn every_zone_is_listed_once_with_utc_among_them() {
        let zones = listed();
        assert_eq!(zones.len(), 9, "{zones:?}");
        assert!(zones.iter().any(|zone| zone.name == UTC));
        // the second line for London is not a second London
        assert_eq!(
            zones
                .iter()
                .filter(|zone| zone.name == "Europe/London")
                .count(),
            1
        );
        assert_eq!(
            named(&zones, "Europe/London").country,
            "Britain (UK)",
            "the first country listed for a zone is the one it is under"
        );
        let words: Vec<String> = zones.iter().map(Zone::words).collect();
        let mut sorted = words.clone();
        sorted.sort_by_key(|said| said.to_lowercase());
        assert_eq!(words, sorted);
    }

    #[test]
    fn a_search_finds_a_place_a_country_or_a_name() {
        let zones = listed();
        assert_eq!(names(&find(&zones, "auck")), ["Pacific/Auckland"]);
        assert_eq!(
            names(&find(&zones, "new zealand")),
            ["Pacific/Auckland", "Pacific/Chatham"]
        );
        assert_eq!(names(&find(&zones, "New York")), ["America/New_York"]);
        assert_eq!(names(&find(&zones, "pacific")).len(), 2);
        assert_eq!(names(&find(&zones, "utc")), [UTC]);
        assert!(find(&zones, "").is_empty());
        assert!(find(&zones, "   ").is_empty());
        assert!(find(&zones, "atlantis").is_empty());
    }

    #[test]
    fn a_place_that_starts_with_what_was_typed_comes_first() {
        let zones = listed();
        // Isle of Man comes before Manaus in the list, and has "man" only at the end of its name
        assert_eq!(
            names(&find(&zones, "man")),
            ["America/Manaus", "Europe/Isle_of_Man"]
        );
        let found = names(&find(&zones, "l"));
        assert_eq!(found.first().map(String::as_str), Some("Europe/London"));
        assert!(found.len() > 2, "{found:?}");
    }

    #[test]
    fn date_says_the_time_the_day_and_the_zone() {
        let now = Now::read("14:05\nMonday 21 September 2026\n2026-09-21\nNZST\n+12:00\n")
            .expect("five lines");
        assert_eq!(now.time, "14:05");
        assert_eq!(now.date, "Monday 21 September 2026");
        assert_eq!(now.day, "2026-09-21");
        assert_eq!(now.zone_words(), "NZST, UTC+12:00");
        let utc = Now::read("02:05\nMonday 21 September 2026\n2026-09-21\nUTC\n+00:00\n")
            .expect("five lines");
        assert_eq!(utc.zone_words(), "UTC");
        let winter = Now {
            abbreviation: "GMT".to_string(),
            offset: "+00:00".to_string(),
            ..Now::default()
        };
        assert_eq!(winter.zone_words(), "GMT, UTC+00:00");
        // a zone with no name for the time of year is written as its offset
        let numbered = Now {
            abbreviation: "-03".to_string(),
            offset: "-03:00".to_string(),
            ..Now::default()
        };
        assert_eq!(numbered.zone_words(), "UTC-03:00");
        assert_eq!(Now::read("14:05\n"), None);
        assert_eq!(Now::read(""), None);
    }

    #[test]
    fn the_network_is_one_of_three_things() {
        assert_eq!(
            Network::from_properties(false, true, true),
            Network::Missing
        );
        assert_eq!(Network::from_properties(true, false, true), Network::Off);
        assert_eq!(
            Network::from_properties(true, true, false),
            Network::On {
                synchronized: false
            }
        );
    }
}
