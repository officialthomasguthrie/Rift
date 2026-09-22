//! The apps Rift suggests: too big for the image, or needing a network to be of use, and installed
//! from Flathub only when the owner ticks them in Welcome. The image keeps the list in one file.
//! An app from a remote other than Flathub is only listed on a machine that has that remote.

use serde::Deserialize;

use crate::flatpak::FLATHUB;

/// One app the list suggests.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct App {
    /// The app's id on its remote, `org.videolan.VLC`.
    pub id: String,
    /// The name people know it by.
    pub name: String,
    /// What it is for, in a few plain words.
    pub about: String,
    /// The heading it is listed under.
    pub group: String,
    /// Whether it is of no use without an account with the company that makes it.
    #[serde(default)]
    pub account: bool,
    /// The remote it comes from.
    #[serde(default = "flathub")]
    pub remote: String,
}

fn flathub() -> String {
    FLATHUB.to_string()
}

/// The file: an `[[app]]` table for each app, in the order they are listed.
#[derive(Deserialize)]
struct List {
    #[serde(default)]
    app: Vec<App>,
}

/// The list in a file's text.
///
/// # Errors
///
/// A sentence saying what is wrong with it.
pub fn parse(text: &str) -> Result<Vec<App>, String> {
    toml::from_str::<List>(text)
        .map(|list| list.app)
        .map_err(|e| format!("The list of suggested apps does not read: {e}"))
}

/// The list the image keeps.
///
/// # Errors
///
/// A sentence when the file is missing or does not read.
pub fn read() -> Result<Vec<App>, String> {
    let text = std::fs::read_to_string(crate::paths::SUGGESTED_APPS).map_err(|e| {
        format!(
            "Could not read the list of suggested apps in {}: {e}",
            crate::paths::SUGGESTED_APPS
        )
    })?;
    parse(&text)
}

/// The apps to list on a machine with these remotes: every app from Flathub, and an app from
/// another remote only where that remote is set up.
#[must_use]
pub fn offered<'a>(apps: &'a [App], remotes: &[String]) -> Vec<&'a App> {
    apps.iter()
        .filter(|app| app.remote == FLATHUB || remotes.contains(&app.remote))
        .collect()
}

/// The headings, in the order the list first names them.
#[must_use]
pub fn groups<'a>(apps: &[&'a App]) -> Vec<&'a str> {
    let mut found: Vec<&str> = Vec::new();
    for app in apps {
        if !found.contains(&app.group.as_str()) {
            found.push(&app.group);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list the image installs.
    const SHIPPED: &str = include_str!("../../../nix/welcome/apps.toml");

    #[test]
    fn the_shipped_list_reads() {
        let apps = parse(SHIPPED).unwrap();
        let flathub: Vec<&App> = apps.iter().filter(|app| app.remote == FLATHUB).collect();
        assert_eq!(flathub.len(), 21);
        // nothing twice, and every id is a flatpak id: three parts or more, no spaces
        for (at, app) in apps.iter().enumerate() {
            assert!(
                apps[..at].iter().all(|other| other.id != app.id),
                "{} twice",
                app.id
            );
            assert!(app.id.split('.').count() >= 3, "{}", app.id);
            assert!(!app.id.contains(char::is_whitespace), "{}", app.id);
            assert!(!app.name.is_empty() && !app.about.is_empty(), "{}", app.id);
            // plain words: a sentence case line with no full stop at its end
            assert!(!app.about.ends_with('.'), "{}", app.id);
        }
        // the three chat apps are the ones that need an account
        let accounts: Vec<&str> = apps
            .iter()
            .filter(|app| app.account)
            .map(|app| app.name.as_str())
            .collect();
        assert_eq!(accounts, ["Signal", "Telegram", "Discord"]);
        // the headings, in the order the page shows them, on a machine with Flathub alone
        assert_eq!(
            groups(&offered(&apps, &[FLATHUB.to_string()])),
            [
                "Internet and privacy",
                "Chat",
                "Office and notes",
                "Media and downloads",
                "Network",
                "System",
                "Editors",
                "Creative",
            ]
        );
    }

    #[test]
    fn an_app_from_another_remote_needs_that_remote() {
        let apps = parse(
            "[[app]]\nid = \"org.example.One\"\nname = \"One\"\nabout = \"The first\"\ngroup = \"Tools\"\n\n\
             [[app]]\nid = \"org.example.Two\"\nname = \"Two\"\nabout = \"The second\"\ngroup = \"Other\"\n\
             remote = \"elsewhere\"\naccount = true\n",
        )
        .unwrap();
        assert_eq!(apps[0].remote, FLATHUB);
        assert!(!apps[0].account);
        assert!(apps[1].account);
        let names = |remotes: &[String]| -> Vec<String> {
            offered(&apps, remotes)
                .iter()
                .map(|app| app.name.clone())
                .collect()
        };
        assert_eq!(names(&[]), ["One"]);
        assert_eq!(names(&[FLATHUB.to_string()]), ["One"]);
        assert_eq!(
            names(&[FLATHUB.to_string(), "elsewhere".to_string()]),
            ["One", "Two"]
        );
        assert_eq!(
            groups(&offered(&apps, &["elsewhere".to_string()])),
            ["Tools", "Other"]
        );
        assert!(parse("[[app]]\nid = 3\n").is_err());
        assert!(parse("").unwrap().is_empty());
    }
}
