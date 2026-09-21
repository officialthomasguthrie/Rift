//! What the owner has set about notifications: Do not disturb, and the apps whose banners stay off
//! the screen. The shell reads both when it starts and when Settings tells it to, and keeps the
//! list of apps that have sent a notification, which is what the Notifications page offers a
//! switch for.
//!
//! Each file has one writer. Do not disturb is written by the clock menu's switch and by the page,
//! which is the same one line either way; the apps that have sent one are the shell's; the apps
//! whose banners are off are Settings'.

use std::fs;

use crate::appearance::{home, write_beside};

/// Do not disturb, under home: `on`, or anything else for off.
pub const QUIET: &str = ".config/rift/do-not-disturb";
/// The apps that have sent a notification, under home: one a line, the name the app gives itself,
/// then a tab and the desktop entry it says it is when it said one.
pub const SENDERS: &str = ".local/state/rift/notified";
/// The apps whose banners stay off the screen, under home: one name a line.
pub const QUIET_APPS: &str = ".config/rift/quiet-apps";
/// The most apps the shell remembers. Past that the one that sent first is forgotten.
pub const REMEMBERED: usize = 64;

/// Whether the owner left Do not disturb on.
#[must_use]
pub fn quiet() -> bool {
    home()
        .and_then(|home| fs::read_to_string(home.join(QUIET)).ok())
        .is_some_and(|text| text.trim() == "on")
}

/// Keep Do not disturb as the owner set it.
///
/// # Errors
///
/// A sentence when there is no home to keep it in, or the file cannot be written.
pub fn keep_quiet(on: bool) -> Result<(), String> {
    let home = home().ok_or("There is no home folder to keep Do not disturb in.")?;
    write_beside(&home.join(QUIET), if on { "on\n" } else { "off\n" }).map(|_| ())
}

/// An app that has sent a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sender {
    /// The name it gives itself, which is what its banners are turned off by.
    pub app: String,
    /// The desktop entry it says it is, whose name and icon the page shows.
    pub entry: Option<String>,
}

/// The apps that have sent a notification, the first one first.
#[must_use]
pub fn senders() -> Vec<Sender> {
    home()
        .and_then(|home| fs::read_to_string(home.join(SENDERS)).ok())
        .map_or_else(Vec::new, |text| parse_senders(&text))
}

/// The apps a file of them holds.
#[must_use]
pub fn parse_senders(text: &str) -> Vec<Sender> {
    let mut found: Vec<Sender> = Vec::new();
    for line in text.lines() {
        let (app, entry) = line.split_once('\t').unwrap_or((line, ""));
        let app = app.trim();
        if app.is_empty() || found.iter().any(|sender| sender.app == app) {
            continue;
        }
        found.push(Sender {
            app: app.to_string(),
            entry: Some(entry.trim().to_string()).filter(|entry| !entry.is_empty()),
        });
    }
    found
}

/// The list with this app in it, or nothing when it is there already as it is. An app that named
/// its entry this time and did not before keeps its place and gains the entry.
#[must_use]
pub fn with_sender(list: &[Sender], app: &str, entry: Option<&str>) -> Option<Vec<Sender>> {
    // a name is one line with no tab in it, so the file keeps its shape whatever an app sends
    let app = one_line(app);
    if app.is_empty() {
        return None;
    }
    let entry = entry.map(one_line).filter(|entry| !entry.is_empty());
    let mut after = list.to_vec();
    match after.iter_mut().find(|sender| sender.app == app) {
        Some(known) if known.entry.is_some() || entry.is_none() => return None,
        Some(known) => known.entry = entry,
        None => {
            after.push(Sender { app, entry });
            if after.len() > REMEMBERED {
                after.remove(0);
            }
        }
    }
    Some(after)
}

/// Write the apps that have sent a notification.
///
/// # Errors
///
/// A sentence when there is no home or the file cannot be written.
pub fn save_senders(list: &[Sender]) -> Result<(), String> {
    let home = home().ok_or("There is no home folder to keep the apps in.")?;
    let mut text = String::new();
    for sender in list {
        text.push_str(&sender.app);
        if let Some(entry) = &sender.entry {
            text.push('\t');
            text.push_str(entry);
        }
        text.push('\n');
    }
    write_beside(&home.join(SENDERS), &text).map(|_| ())
}

/// The apps whose banners stay off the screen.
#[must_use]
pub fn quiet_apps() -> Vec<String> {
    home()
        .and_then(|home| fs::read_to_string(home.join(QUIET_APPS)).ok())
        .map_or_else(Vec::new, |text| parse_quiet_apps(&text))
}

/// The apps a file of them holds.
#[must_use]
pub fn parse_quiet_apps(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for line in text.lines().map(str::trim) {
        if !line.is_empty() && !found.iter().any(|app| app == line) {
            found.push(line.to_string());
        }
    }
    found
}

/// Show an app's banners, or keep them off the screen.
///
/// # Errors
///
/// A sentence when there is no home or the file cannot be written.
pub fn set_banners(app: &str, on: bool) -> Result<(), String> {
    let home = home().ok_or("There is no home folder to keep the apps in.")?;
    let app = one_line(app);
    let mut quiet = quiet_apps();
    quiet.retain(|other| *other != app);
    if !on {
        quiet.push(app);
    }
    let mut text = String::new();
    for app in &quiet {
        text.push_str(app);
        text.push('\n');
    }
    write_beside(&home.join(QUIET_APPS), &text).map(|_| ())
}

/// Text as one line with no tab in it.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sender(app: &str, entry: Option<&str>) -> Sender {
        Sender {
            app: app.to_string(),
            entry: entry.map(str::to_string),
        }
    }

    #[test]
    fn the_apps_that_sent_one_are_a_line_each() {
        let text = "notify-send\nFirefox\tfirefox\n\nnotify-send\n  Loupe \t org.gnome.Loupe \n";
        assert_eq!(
            parse_senders(text),
            [
                sender("notify-send", None),
                sender("Firefox", Some("firefox")),
                sender("Loupe", Some("org.gnome.Loupe")),
            ]
        );
    }

    #[test]
    fn an_app_is_remembered_once() {
        let known = vec![sender("notify-send", None)];
        assert_eq!(with_sender(&known, "notify-send", None), None);
        let grown = with_sender(&known, "Firefox", Some("firefox")).expect("a new one");
        assert_eq!(grown[1], sender("Firefox", Some("firefox")));
        // an entry named later is kept, and a name is one line
        let named =
            with_sender(&known, "notify-send", Some("org.example.Tool")).expect("the entry");
        assert_eq!(named, [sender("notify-send", Some("org.example.Tool"))]);
        assert_eq!(
            with_sender(&known, "Rift\tboot\ntest", None).expect("a new one")[1].app,
            "Rift boot test"
        );
        assert_eq!(with_sender(&known, " ", None), None);
    }

    #[test]
    fn the_list_forgets_the_first_past_its_length() {
        let mut known = Vec::new();
        for number in 0..REMEMBERED {
            known = with_sender(&known, &format!("app {number}"), None).expect("a new one");
        }
        let past = with_sender(&known, "one more", None).expect("a new one");
        assert_eq!(past.len(), REMEMBERED);
        assert_eq!(past[0].app, "app 1");
        assert_eq!(past[REMEMBERED - 1].app, "one more");
    }

    #[test]
    fn the_quiet_apps_are_a_name_a_line() {
        assert_eq!(
            parse_quiet_apps("notify-send\n\nRift boot test\nnotify-send\n"),
            ["notify-send", "Rift boot test"]
        );
    }
}
