//! The Notifications page: Do not disturb, and a switch for the banners of each app that has sent a
//! notification.
//!
//! Do not disturb is the same one line in the owner's settings as the switch in the clock menu, so
//! the page reads it as it comes up and every second while it is up, and the switch in the menu
//! moves the page's. The shell remembers every app that sends one, which is the list the page
//! offers a switch for. What the page changes it writes, then tells the shell to read it again.

use std::thread;
use std::time::Duration;

use iced::futures::channel::mpsc;
use iced::widget::{column, container, row, text};
use iced::{Center, Element, Fill, Subscription, Task};
use librift::notifications::{self as settings, Sender};

use crate::icons;
use crate::theme::Colors;
use crate::ui::{Message, Settings};
use crate::widgets::{GAP, TEXT_SIZE, group, heading, note, setting, switch};

/// How often the page reads the files again while it is up.
const EVERY: Duration = Duration::from_secs(1);
/// How big an app's icon is in a row.
const ICON: f32 = 24.0;

/// What the files say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    /// Do not disturb.
    pub quiet: bool,
    /// The apps that have sent a notification, the first one first.
    pub senders: Vec<Sender>,
    /// The ones whose banners stay off the screen.
    pub muted: Vec<String>,
}

impl Picture {
    /// Whether this app's banners show.
    #[must_use]
    pub fn shows(&self, app: &str) -> bool {
        !self.muted.iter().any(|muted| muted == app)
    }
}

/// Read the files now. They are three short files, so the window reads them itself as the page
/// comes up.
#[must_use]
pub fn reading() -> Picture {
    Picture {
        quiet: settings::quiet(),
        senders: settings::senders(),
        muted: settings::quiet_apps(),
    }
}

/// The files while the page is up: read every second, on a thread that ends at the first send after
/// the page has gone.
pub fn following() -> Subscription<Message> {
    Subscription::run_with("notifications", |_| {
        let (sender, receiver) = mpsc::unbounded();
        thread::spawn(move || {
            while sender.unbounded_send(Message::Noticed(reading())).is_ok() {
                thread::sleep(EVERY);
            }
        });
        receiver
    })
}

/// What the owner asked for on the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// Do not disturb on or off.
    Quiet(bool),
    /// This app's banners shown or kept off the screen.
    Banners(String, bool),
}

/// Do what the owner asked: write it, and tell the shell.
pub fn asked(state: &mut Settings, asked: &Asked) -> Task<Message> {
    state.problem = match asked {
        Asked::Quiet(on) => settings::keep_quiet(*on).err(),
        Asked::Banners(app, on) => settings::set_banners(app, *on).err(),
    };
    state.notices = Some(reading());
    poke_the_shell();
    Task::none()
}

/// Tell the shell that is running to read Do not disturb and the apps kept quiet again.
fn poke_the_shell() {
    let _ = std::process::Command::new("lens")
        .arg("--notifications")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// What `rift-settings --set` asks of this page, the way pressing it would: `do-not-disturb on`, or
/// `app-banners <app> off` for an app that has sent a notification. Nothing for anything else.
#[must_use]
pub fn named(state: &Settings, name: &str, value: &str) -> Option<Asked> {
    let on = |word: &str| match word.trim() {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    };
    match name {
        "do-not-disturb" => on(value).map(Asked::Quiet),
        "app-banners" => {
            let (app, word) = value.trim().rsplit_once(' ')?;
            let app = app.trim();
            let known = state
                .notices
                .as_ref()
                .is_some_and(|now| now.senders.iter().any(|sender| sender.app == app));
            known.then_some(Asked::Banners(app.to_string(), on(word)?))
        }
        _ => None,
    }
}

/// The lines `rift-settings --state` prints, once the page has read the files: Do not disturb, how
/// many apps have sent a notification, and a line for each with whether its banners show.
#[must_use]
pub fn state(state: &Settings) -> Vec<String> {
    let Some(now) = state.notices.as_ref() else {
        return Vec::new();
    };
    let word = |on: bool| if on { "on" } else { "off" };
    let mut lines = vec![
        format!("do-not-disturb {}", word(now.quiet)),
        format!("notifiers {}", now.senders.len()),
    ];
    for sender in &now.senders {
        lines.push(format!(
            "app-banners {} {}",
            word(now.shows(&sender.app)),
            sender.app
        ));
    }
    lines
}

/// The page.
pub fn view(state: &Settings, look: Colors) -> Element<'_, Message> {
    let Some(now) = state.notices.as_ref() else {
        return note(look, "Reading the notification settings.");
    };
    let quiet = vec![setting(
        look,
        "Do not disturb",
        Some(QUIET),
        switch(look, now.quiet, |on| Message::Notices(Asked::Quiet(on))),
    )];
    let mut apps: Vec<Element<'_, Message>> = now
        .senders
        .iter()
        .map(|sender| app_row(state, look, now, sender))
        .collect();
    if apps.is_empty() {
        apps.push(
            container(note(look, "No app has sent a notification yet."))
                .padding([8, 12])
                .into(),
        );
    }
    let mut page = column![
        group(look, quiet),
        column![heading(look, "Apps"), group(look, apps), note(look, APPS)].spacing(8),
        column![heading(look, "Lock screen"), note(look, LOCKED)].spacing(8),
    ]
    .spacing(GAP)
    .width(Fill);
    if let Some(why) = &state.problem {
        page = page.push(text(why).size(TEXT_SIZE).color(look.error));
    }
    page.into()
}

/// One app that has sent a notification: its icon and name, from the desktop entry it said it was
/// when it said one, and the switch for its banners.
fn app_row<'a>(
    state: &'a Settings,
    look: Colors,
    now: &Picture,
    sender: &'a Sender,
) -> Element<'a, Message> {
    let entry = sender
        .entry
        .as_deref()
        .and_then(|entry| state.apps.iter().find(|app| app.id == entry));
    let name = entry.map_or(sender.app.as_str(), |app| app.name.as_str());
    let app = sender.app.clone();
    container(
        row![
            icons::of_app(look.text, entry.and_then(|app| app.icon.as_deref()), ICON),
            text(name.to_string())
                .size(TEXT_SIZE)
                .color(look.text)
                .width(Fill),
            switch(look, now.shows(&sender.app), move |on| {
                Message::Notices(Asked::Banners(app.clone(), on))
            }),
        ]
        .spacing(12)
        .align_y(Center),
    )
    .width(Fill)
    .padding([8, 12])
    .into()
}

/// Under Do not disturb.
const QUIET: &str = "Notifications go into the list in the clock menu without showing. A critical \
                     one still shows.";
/// Under the apps.
const APPS: &str = "An app is listed once it has sent a notification. With its switch off, what it \
                    sends goes into the list without showing, the way Do not disturb keeps it.";
/// What the lock screen does.
const LOCKED: &str = "The lock screen shows no notifications.";

#[cfg(test)]
mod tests {
    use super::*;

    fn sender(app: &str) -> Sender {
        Sender {
            app: app.to_string(),
            entry: None,
        }
    }

    fn settings(quiet: bool, muted: &[&str]) -> Settings {
        let mut state = Settings::bare();
        state.notices = Some(Picture {
            quiet,
            senders: vec![sender("notify-send"), sender("Rift boot test")],
            muted: muted.iter().map(|&app| app.to_string()).collect(),
        });
        state
    }

    #[test]
    fn the_state_says_do_not_disturb_and_every_app() {
        assert!(state(&Settings::bare()).is_empty());
        assert_eq!(
            state(&settings(true, &["Rift boot test"])),
            [
                "do-not-disturb on",
                "notifiers 2",
                "app-banners on notify-send",
                "app-banners off Rift boot test",
            ]
        );
    }

    #[test]
    fn a_setting_from_a_terminal_is_what_the_page_would_press() {
        let kept = settings(false, &[]);
        assert_eq!(
            named(&kept, "do-not-disturb", "on"),
            Some(Asked::Quiet(true))
        );
        assert_eq!(named(&kept, "do-not-disturb", "loud"), None);
        // an app by its name, spaces and all, and only one that has sent a notification
        assert_eq!(
            named(&kept, "app-banners", "Rift boot test off"),
            Some(Asked::Banners("Rift boot test".into(), false))
        );
        assert_eq!(named(&kept, "app-banners", "Firefox off"), None);
        assert_eq!(named(&kept, "app-banners", "notify-send"), None);
    }

    #[test]
    fn the_sentences_are_sentences() {
        for sentence in [QUIET, APPS, LOCKED] {
            assert!(sentence.ends_with('.'), "{sentence}");
            assert!(sentence.is_ascii(), "{sentence}");
        }
    }
}
