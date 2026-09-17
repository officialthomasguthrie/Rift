//! Dark or light, which the owner picks. `~/.config/rift/theme` holds the word until Settings writes
//! it. The shell and the lock screen read it themselves; the shell also hands it on when it starts, to
//! GTK and libadwaita through the owner's dconf database and to Horizon through a part of its config
//! that the system config includes from the owner's state directory. That part carries the
//! [`crate::wallpaper`] too, so whoever writes it writes both.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

use crate::wallpaper::{self, Wallpaper};

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

    /// The part of Horizon's config for this theme and this wallpaper. The system config is dark,
    /// so dark adds nothing of its own; light has the light desktop and the light accent around the
    /// focused window. A picture is named for Horizon to draw, with the theme's gray under it while
    /// it is read; a colour takes the desktop's place and no picture is drawn.
    #[must_use]
    pub fn horizon(self, wallpaper: &Wallpaper) -> String {
        let mut part = format!(
            "// written from ~/{SETTING} and ~/{}: {}, {}\n",
            wallpaper::SETTING,
            self.word(),
            wallpaper.setting().replace('\n', " ")
        );
        let background = match wallpaper {
            Wallpaper::Color(color) => Some(color.as_str()),
            Wallpaper::Picture(_) => (self == Self::Light).then_some(LIGHT_BACKGROUND),
        };
        let accent = (self == Self::Light).then_some(LIGHT_ACCENT);
        if background.is_some() || accent.is_some() {
            part.push_str("layout {\n");
            if let Some(background) = background {
                let _ = writeln!(part, "    background-color \"{background}\"");
            }
            if let Some(accent) = accent {
                let _ = writeln!(
                    part,
                    "    focus-ring {{\n        active-color \"{accent}\"\n    }}"
                );
            }
            part.push_str("}\n");
        }
        match wallpaper {
            Wallpaper::Picture(path) => {
                let _ = writeln!(
                    part,
                    "wallpaper \"{}\"",
                    kdl_string(&path.display().to_string())
                );
            }
            Wallpaper::Color(_) => part.push_str("wallpaper null\n"),
        }
        part
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
    if let Err(why) = write_horizon(theme, &Wallpaper::read()) {
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

pub(crate) fn home() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

/// Write the part of Horizon's config for a theme and a wallpaper. Horizon reads its config again
/// when the file changes, so the desktop follows at once.
///
/// # Errors
///
/// A sentence when there is no home or the file could not be written.
pub fn write_horizon(theme: Theme, wallpaper: &Wallpaper) -> Result<(), String> {
    let path = home()
        .ok_or("There is no home to write the compositor's part into.")?
        .join(HORIZON_PART);
    write_beside(&path, &theme.horizon(wallpaper))
}

/// Write a file beside itself and rename it over the old one, so nothing ever reads half of it. A
/// file that already says it is left alone.
pub(crate) fn write_beside(path: &Path, text: &str) -> Result<(), String> {
    if fs::read_to_string(path).is_ok_and(|old| old == text) {
        return Ok(());
    }
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)
            .map_err(|e| format!("Could not make {}: {e}.", folder.display()))?;
    }
    let mut fresh = path.as_os_str().to_owned();
    fresh.push(".new");
    let fresh = PathBuf::from(fresh);
    fs::write(&fresh, text).map_err(|e| format!("Could not write {}: {e}.", fresh.display()))?;
    fs::rename(&fresh, path).map_err(|e| format!("Could not write {}: {e}.", path.display()))
}

/// Text as it goes between the quotes of a KDL string.
fn kdl_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
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

    fn photo() -> Wallpaper {
        Wallpaper::Picture(PathBuf::from(
            "/run/current-system/sw/share/backgrounds/rift/earthset.jpg",
        ))
    }

    #[test]
    fn dark_with_a_picture_names_the_picture_alone() {
        let dark = Theme::Dark.horizon(&photo());
        assert_eq!(
            dark,
            "// written from ~/.config/rift/theme and ~/.config/rift/wallpaper: dark, \
             /run/current-system/sw/share/backgrounds/rift/earthset.jpg\n\
             wallpaper \"/run/current-system/sw/share/backgrounds/rift/earthset.jpg\"\n"
        );
    }

    #[test]
    fn light_has_its_desktop_and_accent_under_the_picture() {
        let light = Theme::Light.horizon(&photo());
        assert!(light.contains("background-color \"#f2f1f0\""));
        assert!(light.contains("active-color \"#3584e4\""));
        assert!(light.ends_with(
            "wallpaper \"/run/current-system/sw/share/backgrounds/rift/earthset.jpg\"\n"
        ));
        // every brace it opens it closes, or Horizon refuses the whole config
        assert_eq!(light.matches('{').count(), 2);
        assert_eq!(light.matches('}').count(), 2);
        assert!(light.is_ascii());
    }

    #[test]
    fn a_colour_takes_the_desktops_place_and_no_picture_is_drawn() {
        let gray = Wallpaper::Color("#242424".into());
        let dark = Theme::Dark.horizon(&gray);
        assert!(dark.lines().next().unwrap().ends_with(": dark, #242424"));
        assert!(dark.contains("layout {\n    background-color \"#242424\"\n}\n"));
        assert!(dark.ends_with("wallpaper null\n"));
        let light = Theme::Light.horizon(&gray);
        assert_eq!(light.matches("background-color").count(), 1);
        assert!(light.contains("background-color \"#242424\""));
        assert!(light.contains("active-color \"#3584e4\""));
        assert!(light.ends_with("wallpaper null\n"));
    }

    #[test]
    fn a_path_is_escaped_inside_its_quotes() {
        assert_eq!(
            kdl_string(r#"/home/rift/a "b"\c.jpg"#),
            r#"/home/rift/a \"b\"\\c.jpg"#
        );
        let odd = Wallpaper::Picture(PathBuf::from("/home/rift/it's \"here\".png"));
        let part = Theme::Dark.horizon(&odd);
        assert!(
            part.ends_with("wallpaper \"/home/rift/it's \\\"here\\\".png\"\n"),
            "{part}"
        );
    }
}
