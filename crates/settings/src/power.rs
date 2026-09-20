//! The Power page: the battery, and what the machine does when it is left alone.
//!
//! `UPower` adds every battery of the machine into one display device, which is what the bar's
//! icon and this page both read. Nothing watches for an idle session yet, so the second half of
//! the page says so rather than offering a setting that would do nothing.

use iced::widget::{column, text};
use iced::{Element, Fill};
use librift::battery::{Battery, Charge};

use crate::theme::Colors;
use crate::ui::{Message, Settings};
use crate::widgets::{GAP, TEXT_SIZE, group, heading, note, setting};

/// The line `rift-settings --state` prints about the battery, once `UPower` has answered.
#[must_use]
pub fn state(state: &Settings) -> Vec<String> {
    let Some(Ok(answered)) = state.battery.as_ref() else {
        return Vec::new();
    };
    vec![answered.map_or_else(
        || "battery none".to_string(),
        |battery| format!("battery {} {}", battery.level, battery.charge.word()),
    )]
}

/// The page.
pub fn view(state: &Settings, look: Colors) -> Element<'_, Message> {
    let mut page = column![].spacing(GAP).width(Fill);
    match &state.battery {
        None => page = page.push(note(look, "Asking UPower about the battery.")),
        Some(Err(why)) => {
            page = page.push(note(
                look,
                "UPower is not answering, so the battery is not here.",
            ));
            page = page.push(note(look, why));
        }
        Some(Ok(None)) => {
            page = page.push(note(
                look,
                "This machine has no battery. It runs on the mains.",
            ));
        }
        Some(Ok(Some(battery))) => page = page.push(charge(look, *battery)),
    }
    if let Some(why) = &state.problem {
        page = page.push(text(why).size(TEXT_SIZE).color(look.error));
    }
    page.push(
        column![
            heading(look, "When the machine is left alone"),
            note(
                look,
                "Nothing blanks the screen, locks it or suspends the machine after a while yet: \
                 that takes a service watching the session, and Rift has none. Mod+L locks the \
                 screen now, and the system menu suspends the machine.",
            ),
        ]
        .spacing(8),
    )
    .into()
}

/// The battery: how full it is, what it is doing, and how long it has.
fn charge<'a>(look: Colors, battery: Battery) -> Element<'a, Message> {
    let mut rows = vec![
        setting(
            look,
            "Charge",
            None,
            said(look, format!("{}%", battery.level)),
        ),
        setting(look, "State", None, said(look, doing(battery).to_string())),
    ];
    if let Some(time) = battery.time() {
        rows.push(setting(look, "Time", None, said(look, time)));
    }
    column![heading(look, "Battery"), group(look, rows)]
        .spacing(8)
        .into()
}

/// What the battery is doing, in the words a page says it in.
const fn doing(battery: Battery) -> &'static str {
    match battery.charge {
        Charge::Charging => "Charging",
        Charge::Discharging => "On battery",
        Charge::Full => "Fully charged",
        Charge::Idle => "On the charger",
    }
}

/// What a row is set to, at the right of it.
fn said<'a>(look: Colors, value: String) -> Element<'a, Message> {
    text(value).size(TEXT_SIZE).color(look.dim).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(answered: Option<Result<Option<Battery>, String>>) -> Settings {
        let mut state = Settings::bare();
        state.battery = answered;
        state
    }

    fn battery(level: u8, charge: Charge, seconds: i64) -> Battery {
        Battery {
            level,
            charge,
            seconds,
        }
    }

    #[test]
    fn a_machine_with_no_battery_says_so_in_one_line() {
        assert_eq!(state(&settings(Some(Ok(None)))), ["battery none"]);
        // nothing has answered yet, and a failure is on the page, not in the state
        assert!(state(&settings(None)).is_empty());
        assert!(state(&settings(Some(Err("UPower is not running.".into())))).is_empty());
    }

    #[test]
    fn a_battery_prints_how_full_it_is_and_what_it_is_doing() {
        let kept = settings(Some(Ok(Some(battery(72, Charge::Discharging, 12_000)))));
        assert_eq!(state(&kept), ["battery 72 discharging"]);
        let full = settings(Some(Ok(Some(battery(100, Charge::Full, 0)))));
        assert_eq!(state(&full), ["battery 100 charged"]);
    }

    #[test]
    fn what_the_battery_is_doing_reads_as_words() {
        assert_eq!(doing(battery(40, Charge::Charging, 2_700)), "Charging");
        assert_eq!(doing(battery(40, Charge::Discharging, 0)), "On battery");
        assert_eq!(doing(battery(100, Charge::Full, 0)), "Fully charged");
        assert_eq!(doing(battery(80, Charge::Idle, 0)), "On the charger");
    }
}
