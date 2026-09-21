//! The Date and time page: the time and the date as they read now, whether a time server sets the
//! clock, how the hardware clock is read, and the time zone, which the owner chooses here.
//!
//! systemd's timedated answers for the clock and sets the zone. The image keeps the link it writes
//! on persist, so a zone chosen here goes with the drive to every machine it starts. The page reads
//! timedated and `date` as it comes up and again as every minute turns while it is up, so the time
//! on it turns with the bar's.

use std::thread;

use iced::futures::channel::{mpsc, oneshot};
use iced::widget::{column, container, row, text};
use iced::{Center, Element, Fill, Subscription, Task};
use librift::clock::{self, Clock, Network, Now, Zone};

use crate::ai::said;
use crate::theme::Colors;
use crate::ui::{Message, Settings};
use crate::widgets::{GAP, TEXT_SIZE, choice, field, group, heading, line, note, setting};

/// The field a zone is searched for in.
pub const FIELD: &str = "zone";
/// How many zones a search lists at most.
const LISTED: usize = 8;

/// What the page reads: timedated's answer and what `date` says the time is.
#[derive(Debug, Clone)]
pub struct Reading {
    /// What timedated says about the clock, or why it did not.
    pub clock: Result<Clock, String>,
    /// The time and the date in the zone that is set, when `date` was there to ask.
    pub now: Option<Now>,
}

/// Read the clock now.
fn reading() -> Reading {
    Reading {
        clock: clock::read(),
        now: clock::now(),
    }
}

/// Read the clock once, on a thread of its own.
pub fn read() -> Task<Message> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let _ = sender.send(reading());
    });
    Task::perform(receiver, |answered| {
        Message::Clock(Box::new(answered.unwrap_or_else(|_| Reading {
            clock: Err("timedated did not answer.".to_string()),
            now: None,
        })))
    })
}

/// The clock while the page is up: read as the page comes up and again as every minute turns, on a
/// thread that ends at the first tick after the page has gone.
pub fn ticking() -> Subscription<Message> {
    Subscription::run_with("clock", |_| {
        let (sender, receiver) = mpsc::unbounded();
        thread::spawn(move || {
            loop {
                if sender
                    .unbounded_send(Message::Clock(Box::new(reading())))
                    .is_err()
                {
                    return;
                }
                thread::sleep(librift::time::until_next_minute());
            }
        });
        receiver
    })
}

/// Choose a time zone from the page or the socket. The search is done with, and what went wrong
/// before is behind it.
pub fn choose(state: &mut Settings, zone: String) -> Task<Message> {
    state.finding.clear();
    state.problem = None;
    set_zone(zone)
}

/// Set the time zone on a thread of its own, then read the clock again, so the page says the zone
/// that is set rather than the one that was asked for.
///
/// The reading is asked for inside the closure: a task built beside the setting would start its
/// thread at once and read the zone before it had changed.
fn set_zone(zone: String) -> Task<Message> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let _ = sender.send(clock::set_zone(&zone));
    });
    Task::perform(receiver, |said| {
        said.unwrap_or_else(|_| Err("It stopped before it finished.".to_string()))
    })
    .then(|said| Task::done(Message::Acted(said)).chain(read()))
}

/// The lines `rift-settings --state` prints about the clock, once timedated has answered: the
/// zone, whether a time server sets the clock and has answered, how the hardware clock is read,
/// and the time and the date as the page shows them.
#[must_use]
pub fn state(state: &Settings) -> Vec<String> {
    let Some(reading) = state.clock.as_ref() else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    if let Ok(clock) = &reading.clock {
        let zone = if clock.zone.is_empty() {
            "none"
        } else {
            clock.zone.as_str()
        };
        let (ntp, synchronized) = match clock.network {
            Network::Missing => ("none", "no"),
            Network::Off => ("off", "no"),
            Network::On { synchronized } => ("on", if synchronized { "yes" } else { "no" }),
        };
        lines.push(format!("timezone {zone}"));
        lines.push(format!("ntp {ntp}"));
        lines.push(format!("synchronized {synchronized}"));
        lines.push(format!(
            "rtc {}",
            if clock.local_rtc { "local" } else { "utc" }
        ));
    }
    if let Some(now) = &reading.now {
        lines.push(format!("time {}", now.time));
        lines.push(format!("date {}", now.day));
    }
    lines
}

/// The page.
pub fn view(state: &Settings, look: Colors) -> Element<'_, Message> {
    let mut page = column![].spacing(GAP).width(Fill);
    let Some(reading) = state.clock.as_ref() else {
        return note(look, "Asking timedated about the clock.");
    };
    match &reading.clock {
        Err(why) => {
            page = page.push(note(
                look,
                "The clock and the time zone are not here, since timedated is not answering.",
            ));
            page = page.push(note(look, why));
        }
        Ok(clock) => {
            page = page.push(the_clock(look, clock, reading.now.as_ref()));
            page = page.push(the_zone(state, look, clock, reading.now.as_ref()));
        }
    }
    if let Some(why) = &state.problem {
        page = page.push(text(why).size(TEXT_SIZE).color(look.error));
    }
    page.into()
}

/// The time and the date, and what sets them.
fn the_clock<'a>(look: Colors, clock: &Clock, now: Option<&Now>) -> Element<'a, Message> {
    let mut rows = Vec::new();
    if let Some(now) = now {
        rows.push(setting(look, "Time", None, said(look, &now.time)));
        rows.push(setting(look, "Date", None, said(look, &now.date)));
    }
    let (network, under) = match clock.network {
        Network::Missing => (
            "No",
            Some("Nothing on this machine sets the clock from a time server."),
        ),
        Network::Off => ("Off", None),
        Network::On { synchronized: true } => ("On", Some("A time server has set the clock.")),
        Network::On {
            synchronized: false,
        } => ("On", Some("No time server has answered yet.")),
    };
    rows.push(setting(
        look,
        "Set from the network",
        under,
        said(look, network),
    ));
    rows.push(setting(
        look,
        "Hardware clock",
        None,
        said(look, if clock.local_rtc { "Local time" } else { "UTC" }),
    ));
    let said = if matches!(clock.network, Network::On { .. }) {
        CLOCK
    } else {
        BY_HAND
    };
    column![heading(look, "Clock"), group(look, rows), note(look, said)]
        .spacing(8)
        .into()
}

/// The time zone that is set, and the search that finds another.
fn the_zone<'a>(
    state: &'a Settings,
    look: Colors,
    clock: &Clock,
    now: Option<&Now>,
) -> Element<'a, Message> {
    let words = if clock.zone.is_empty() {
        "Not known".to_string()
    } else {
        clock::named(&state.zones, &clock.zone).words()
    };
    // what the zone is called at this time of year and how far it is from UTC, where that says
    // more than the zone's own name does
    let offset = now.map(Now::zone_words).filter(|offset| *offset != words);
    let found = clock::find(&state.zones, &state.finding);
    let entered = found.first().map_or_else(
        || Message::Find(state.finding.clone()),
        |zone| Message::Zone(zone.name.clone()),
    );
    let mut rows = vec![
        current(look, &words, offset),
        setting(
            look,
            "Find another",
            None,
            field(
                look,
                "City or country",
                &state.finding,
                false,
                FIELD,
                Message::Find,
                entered,
            ),
        ),
    ];
    for zone in found.iter().take(LISTED) {
        rows.push(listed(look, zone, zone.name == clock.zone));
    }
    if found.len() > LISTED {
        rows.push(
            container(note(look, "Type more of the name to find the rest."))
                .padding([8, 12])
                .into(),
        );
    } else if found.is_empty() && !state.finding.trim().is_empty() {
        rows.push(
            container(note(look, "No time zone has that name."))
                .padding([8, 12])
                .into(),
        );
    }
    column![
        heading(look, "Time zone"),
        group(look, rows),
        note(look, ZONE)
    ]
    .spacing(8)
    .into()
}

/// The row of the zone that is set: its words at the right, and under its label what the zone is
/// called now and how far it is from UTC.
fn current<'a>(look: Colors, words: &str, offset: Option<String>) -> Element<'a, Message> {
    let mut left = column![line(look, "Time zone")].spacing(2);
    if let Some(offset) = offset {
        left = left.push(text(offset).size(TEXT_SIZE).color(look.dim));
    }
    container(
        row![left.width(Fill), said(look, words)]
            .align_y(Center)
            .spacing(GAP),
    )
    .width(Fill)
    .padding([8, 12])
    .into()
}

/// A zone the search found, pressed to choose it: its place, its country under it, and its name
/// in the database at the right.
fn listed(look: Colors, zone: &Zone, chosen: bool) -> Element<'_, Message> {
    let country = (!zone.country.is_empty()).then_some(zone.country.as_str());
    choice(
        look,
        &zone.city,
        country,
        Some(said(look, &zone.name)),
        chosen,
        Message::Zone(zone.name.clone()),
    )
}

/// What sets the clock, and what a machine that keeps local time does to it.
const CLOCK: &str = "A time server sets the clock whenever there is a network, so it is not set \
                     by hand here. A machine that keeps local time in its hardware clock, as \
                     Windows does, shows the wrong time until a time server has answered.";
/// The same, where no time server sets it.
const BY_HAND: &str = "Setting the clock by hand is not in Settings yet. A machine that keeps local \
                       time in its hardware clock, as Windows does, shows the wrong time here.";
/// Where the zone is kept.
const ZONE: &str = "The time zone is kept on this drive, so it goes with the drive to every \
                    machine it starts.";

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Now {
        Now {
            time: "14:05".to_string(),
            date: "Monday 21 September 2026".to_string(),
            day: "2026-09-21".to_string(),
            abbreviation: "NZST".to_string(),
            offset: "+12:00".to_string(),
        }
    }

    fn settings(clock: Result<Clock, String>, now: Option<Now>) -> Settings {
        let mut state = Settings::bare();
        state.clock = Some(Reading { clock, now });
        state
    }

    fn clock(zone: &str, network: Network) -> Clock {
        Clock {
            zone: zone.to_string(),
            local_rtc: false,
            network,
        }
    }

    #[test]
    fn the_state_says_the_zone_the_network_and_the_time() {
        let kept = settings(
            Ok(clock(
                "Pacific/Auckland",
                Network::On { synchronized: true },
            )),
            Some(now()),
        );
        assert_eq!(
            state(&kept),
            [
                "timezone Pacific/Auckland",
                "ntp on",
                "synchronized yes",
                "rtc utc",
                "time 14:05",
                "date 2026-09-21",
            ]
        );
    }

    #[test]
    fn a_machine_with_no_time_server_and_no_date_says_so() {
        let kept = settings(Ok(clock("", Network::Missing)), None);
        assert_eq!(
            state(&kept),
            ["timezone none", "ntp none", "synchronized no", "rtc utc"]
        );
        let off = settings(Ok(clock("UTC", Network::Off)), None);
        assert_eq!(state(&off)[1..3], ["ntp off", "synchronized no"]);
    }

    #[test]
    fn nothing_is_said_about_the_clock_until_timedated_has_answered() {
        assert!(state(&Settings::bare()).is_empty());
        // the time is still the time when timedated is not answering
        let failed = settings(Err("timedated is not running.".to_string()), Some(now()));
        assert_eq!(state(&failed), ["time 14:05", "date 2026-09-21"]);
    }

    #[test]
    fn the_sentences_are_sentences() {
        for sentence in [CLOCK, BY_HAND, ZONE] {
            assert!(sentence.ends_with('.'), "{sentence}");
            assert!(sentence.is_ascii(), "{sentence}");
        }
    }
}
