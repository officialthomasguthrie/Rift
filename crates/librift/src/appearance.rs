//! Dark or light, which the owner picks. `~/.config/rift/theme` holds the word until Settings writes
//! it. The shell and the lock screen read it themselves; the shell also hands it on when it starts, to
//! GTK and libadwaita through the owner's dconf database and to Horizon through a part of its config
//! that the system config includes from the owner's state directory.

use std::path::PathBuf;
use std::process::Command;
use std::{env, fs};

/// Where the setting lives, under home.
pub const SETTING: &str = ".config/rift/theme";
/// Where the part of Horizon's config goes, under home. The system config includes it, and Horizon
/// reads its config again when the file changes.
pub const HORIZON_PART: &str = ".local/state/rift/horizon.kdl";

/// The desktop's background and focus ring on light, from the light column of the shell's colours.
/// Dark is the system config's own.
const LIGHT_BACKGROUND: &str = "#f2f1f0";
const LIGHT_ACCENT: &str = "#3584e4";

/// The dconf keys that differ between the two, both in `org.gnome.desktop.interface`. libadwaita
/// and GTK 4 follow the colour scheme; GTK 3 has no scheme and takes the dark variant by its name.
const COLOR_SCHEME: &str = "/org/gnome/desktop/interface/color-scheme";
const GTK_THEME: &str = "/org/gnome/desktop/interface/gtk-theme";

/// The two themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    /// Neutral dark grays, the default.
    #[default]
    Dark,
    /// White and light grays.
    Light,
}

impl Theme {
    /// The theme a setting names: `light` is light, and anything else is dark.
    #[must_use]
    pub fn from_setting(text: &str) -> Self {
        if text.trim().eq_ignore_ascii_case("light") {
            Self::Light
        } else {
            Self::Dark
        }
    }

    /// The word for it, as the setting holds it and `lens --state` prints it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    /// The owner's theme, or dark when home has no setting.
    #[must_use]
    pub fn read() -> Self {
        home()
            .and_then(|home| fs::read_to_string(home.join(SETTING)).ok())
            .map_or(Self::Dark, |text| Self::from_setting(&text))
    }

    /// The dconf keys for GTK and libadwaita apps, each with its value as dconf writes it.
    #[must_use]
    pub const fn gtk(self) -> [(&'static str, &'static str); 2] {
        match self {
            Self::Dark => [
                (COLOR_SCHEME, "'prefer-dark'"),
                (GTK_THEME, "'Adwaita-dark'"),
            ],
            Self::Light => [(COLOR_SCHEME, "'default'"), (GTK_THEME, "'Adwaita'")],
        }
    }

    /// The part of Horizon's config for this theme. Dark says nothing, since the system config is
    /// dark; light has the light desktop and the light accent around the focused window.
    #[must_use]
    pub fn horizon(self) -> String {
        let head = format!(
            "// written by lens when the session starts, from ~/{SETTING}: {}\n",
            self.word()
        );
        match self {
            Self::Dark => head,
            Self::Light => format!(
                "{head}layout {{\n    background-color \"{LIGHT_BACKGROUND}\"\n    \
                 focus-ring {{\n        active-color \"{LIGHT_ACCENT}\"\n    }}\n}}\n"
            ),
        }
    }
}

/// Hand the theme on to GTK and to Horizon. A key or a file that already says it is left alone, so
/// running apps get no change signal for nothing.
///
/// # Errors
///
/// A sentence for each part that could not be written. The other part is still written.
pub fn apply(theme: Theme) -> Result<(), String> {
    let mut failed = Vec::new();
    if let Err(why) = write_horizon(theme) {
        failed.push(why);
    }
    for (key, value) in theme.gtk() {
        if let Err(why) = write_key(key, value) {
            failed.push(why);
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(failed.join(" "))
    }
}

fn home() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

fn write_horizon(theme: Theme) -> Result<(), String> {
    let path = home()
        .ok_or("There is no home to write the compositor's part into.")?
        .join(HORIZON_PART);
    let text = theme.horizon();
    if fs::read_to_string(&path).is_ok_and(|old| old == text) {
        return Ok(());
    }
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)
            .map_err(|e| format!("Could not make {}: {e}.", folder.display()))?;
    }
    // written beside it and renamed over it, so Horizon never reads half a file
    let fresh = path.with_extension("kdl.new");
    fs::write(&fresh, text).map_err(|e| format!("Could not write {}: {e}.", fresh.display()))?;
    fs::rename(&fresh, &path).map_err(|e| format!("Could not write {}: {e}.", path.display()))
}

fn write_key(key: &str, value: &str) -> Result<(), String> {
    let read = Command::new("dconf")
        .args(["read", key])
        .output()
        .map_err(|e| format!("Could not run dconf: {e}."))?;
    if String::from_utf8_lossy(&read.stdout).trim() == value {
        return Ok(());
    }
    let status = Command::new("dconf")
        .args(["write", key, value])
        .status()
        .map_err(|e| format!("Could not run dconf: {e}."))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("dconf could not set {key} to {value}."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_is_the_only_other_word() {
        assert_eq!(Theme::from_setting("light\n"), Theme::Light);
        assert_eq!(Theme::from_setting(" Light "), Theme::Light);
        assert_eq!(Theme::from_setting("dark"), Theme::Dark);
        assert_eq!(Theme::from_setting(""), Theme::Dark);
        assert_eq!(Theme::from_setting("blue"), Theme::Dark);
        assert_eq!(Theme::default(), Theme::Dark);
        assert_eq!(Theme::from_setting(Theme::Light.word()), Theme::Light);
    }

    #[test]
    fn gtk_gets_a_scheme_and_a_theme_name_for_each() {
        assert_eq!(
            Theme::Dark.gtk(),
            [
                ("/org/gnome/desktop/interface/color-scheme", "'prefer-dark'"),
                ("/org/gnome/desktop/interface/gtk-theme", "'Adwaita-dark'"),
            ]
        );
        assert_eq!(Theme::Light.gtk()[0].1, "'default'");
        assert_eq!(Theme::Light.gtk()[1].1, "'Adwaita'");
    }

    #[test]
    fn the_compositor_part_is_a_comment_on_dark_and_the_light_colours_on_light() {
        let dark = Theme::Dark.horizon();
        assert_eq!(dark.lines().count(), 1);
        assert!(dark.starts_with("// ") && dark.ends_with(": dark\n"));
        let light = Theme::Light.horizon();
        assert!(light.contains("background-color \"#f2f1f0\""));
        assert!(light.contains("active-color \"#3584e4\""));
        // every brace it opens it closes, or Horizon refuses the whole config
        assert_eq!(light.matches('{').count(), 2);
        assert_eq!(light.matches('}').count(), 2);
        assert!(light.is_ascii());
    }
}
