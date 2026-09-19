//! What the desktop looks like: dark or light, the accent colour, the gaps between windows, the
//! corner radius of a window, and the wallpaper. Each one is a line in a file of its own under
//! `~/.config/rift`, written by Settings and by the rift command. The shell and the lock screen
//! read the theme themselves; whoever writes a setting hands the whole look on as well, to GTK and
//! libadwaita through the owner's dconf database and to Horizon through a part of its config that
//! the system config includes from the owner's state directory. That part carries the
//! [`crate::wallpaper`] too, so whoever writes it writes both.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

use crate::wallpaper::{self, Wallpaper};

/// Where the theme lives, under home.
pub const SETTING: &str = ".config/rift/theme";
/// Where the accent lives, under home.
pub const ACCENT: &str = ".config/rift/accent";
/// Where the gap between windows lives, under home.
pub const GAPS: &str = ".config/rift/gaps";
/// Where the corner radius of a window lives, under home.
pub const RADIUS: &str = ".config/rift/radius";
/// Where the interface text size lives, under home, as a whole number of per cent.
pub const TEXT: &str = ".config/rift/text";
/// Where the terminal colour scheme lives, under home, as one word.
pub const TERMINAL: &str = ".config/rift/terminal";
/// Where the Ghostty configuration written from that scheme goes. The Ghostty config in the image
/// includes this file, and one that is not there is no error to Ghostty.
pub const TERMINAL_CONFIG: &str = ".config/rift/terminal.ghostty";
/// Where the terminal greeting lives, under home. The fish function in the image reads it.
pub const GREETING: &str = ".config/rift/greeting";
/// Where the part of Horizon's config goes, under home. The system config includes it, and Horizon
/// reads its config again when the file changes.
pub const HORIZON_PART: &str = ".local/state/rift/horizon.kdl";

/// The desktop's background on light, from the light column of the shell's colours. Dark is the
/// system config's own.
const LIGHT_BACKGROUND: &str = "#f2f1f0";

/// The gap between windows the system config has, in pixels, and the widest the page offers.
pub const GAPS_DEFAULT: u32 = 8;
/// The widest gap Settings offers.
pub const GAPS_MOST: u32 = 32;
/// The corner radius of a window the system config has: square, as a tiled window is.
pub const RADIUS_DEFAULT: u32 = 0;
/// The largest corner radius Settings offers, from the design rules.
pub const RADIUS_MOST: u32 = 12;

/// The interface text size everything is drawn at by default, in per cent.
pub const TEXT_DEFAULT: u32 = 100;
/// The smallest interface text size Settings offers.
pub const TEXT_LEAST: u32 = 100;
/// The largest one.
pub const TEXT_MOST: u32 = 200;
/// The step between two sizes on the page.
pub const TEXT_STEP: u32 = 5;

/// The dconf keys the look sets, all in `org.gnome.desktop.interface`. libadwaita and GTK 4 follow
/// the colour scheme and the accent; GTK 3 has no scheme and goes dark by the theme's name.
const COLOR_SCHEME: &str = "/org/gnome/desktop/interface/color-scheme";
const GTK_THEME: &str = "/org/gnome/desktop/interface/gtk-theme";
const ACCENT_KEY: &str = "/org/gnome/desktop/interface/accent-color";
/// The size of the text apps draw, as a factor of the usual. It is the key GNOME's large text
/// switch writes, and GTK turns it into the dots per inch its text is laid out at.
const TEXT_KEY: &str = "/org/gnome/desktop/interface/text-scaling-factor";

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

    /// The name of it on the page.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }

    /// The owner's theme, or dark when home has no setting.
    #[must_use]
    pub fn read() -> Self {
        read_setting(SETTING).map_or(Self::Dark, |text| Self::from_setting(&text))
    }

    /// The name of the GTK theme for it. GTK 3 takes the dark variant by its name, so the image
    /// carries an `Adwaita-dark` theme for GTK 3 to find.
    const fn gtk_theme(self) -> &'static str {
        match self {
            Self::Dark => "Adwaita-dark",
            Self::Light => "Adwaita",
        }
    }

    const fn scheme(self) -> &'static str {
        match self {
            Self::Dark => "prefer-dark",
            Self::Light => "default",
        }
    }
}

/// The nine accent colours GNOME offers, in GNOME's order. Blue is Rift's default and the one its
/// screenshots, docs and website use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Accent {
    /// The default, and the one Rift's own screenshots use.
    #[default]
    Blue,
    /// Teal.
    Teal,
    /// Green.
    Green,
    /// Yellow.
    Yellow,
    /// Orange.
    Orange,
    /// Red.
    Red,
    /// Pink.
    Pink,
    /// Purple.
    Purple,
    /// Slate, a gray blue.
    Slate,
}

impl Accent {
    /// Every accent, in the order the page shows them.
    pub const ALL: [Accent; 9] = [
        Self::Blue,
        Self::Teal,
        Self::Green,
        Self::Yellow,
        Self::Orange,
        Self::Red,
        Self::Pink,
        Self::Purple,
        Self::Slate,
    ];

    /// The word for it, as the setting holds it and as dconf takes it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Blue => "blue",
            Self::Teal => "teal",
            Self::Green => "green",
            Self::Yellow => "yellow",
            Self::Orange => "orange",
            Self::Red => "red",
            Self::Pink => "pink",
            Self::Purple => "purple",
            Self::Slate => "slate",
        }
    }

    /// The name of it on the page.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Blue => "Blue",
            Self::Teal => "Teal",
            Self::Green => "Green",
            Self::Yellow => "Yellow",
            Self::Orange => "Orange",
            Self::Red => "Red",
            Self::Pink => "Pink",
            Self::Purple => "Purple",
            Self::Slate => "Slate",
        }
    }

    /// The colour itself, `#rrggbb`. On light it is GNOME's own value; on dark it is that value
    /// mixed a third of the way to white, so it reads on a dark gray the way blue's #78aeed does.
    #[must_use]
    pub const fn hex(self, theme: Theme) -> &'static str {
        match (self, theme) {
            (Self::Blue, Theme::Light) => "#3584e4",
            (Self::Blue, Theme::Dark) => "#78aeed",
            (Self::Teal, Theme::Light) => "#2190a4",
            (Self::Teal, Theme::Dark) => "#68b4c1",
            (Self::Green, Theme::Light) => "#3a944a",
            (Self::Green, Theme::Dark) => "#79b684",
            (Self::Yellow, Theme::Light) => "#c88800",
            (Self::Yellow, Theme::Dark) => "#daae52",
            (Self::Orange, Theme::Light) => "#ed5b00",
            (Self::Orange, Theme::Dark) => "#f38f52",
            (Self::Red, Theme::Light) => "#e62d42",
            (Self::Red, Theme::Dark) => "#ee707f",
            (Self::Pink, Theme::Light) => "#d56199",
            (Self::Pink, Theme::Dark) => "#e294ba",
            (Self::Purple, Theme::Light) => "#9141ac",
            (Self::Purple, Theme::Dark) => "#b47ec7",
            (Self::Slate, Theme::Light) => "#6f8396",
            (Self::Slate, Theme::Dark) => "#9dabb8",
        }
    }

    /// The accent a setting names, or blue for anything else.
    #[must_use]
    pub fn from_setting(text: &str) -> Self {
        let word = text.trim();
        Self::ALL
            .into_iter()
            .find(|accent| word.eq_ignore_ascii_case(accent.word()))
            .unwrap_or_default()
    }

    /// The owner's accent, or blue when home has no setting.
    #[must_use]
    pub fn read() -> Self {
        read_setting(ACCENT).map_or_else(Self::default, |text| Self::from_setting(&text))
    }
}

/// The colours a terminal is drawn in: Rift's own and a few well known ones. Each is a background,
/// a foreground and, for all but Rift's, the sixteen colours a terminal program asks for by number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scheme {
    /// Rift's own: the near black the logo was drawn on, with light gray text.
    #[default]
    Rift,
    /// The same in reverse, for a light desktop.
    RiftLight,
    /// Solarized dark.
    SolarizedDark,
    /// Solarized light.
    SolarizedLight,
    /// Nord.
    Nord,
    /// Gruvbox dark.
    GruvboxDark,
}

/// The palette GNOME Terminal draws with on a light background.
const TANGO: [&str; 16] = [
    "#2e3436", "#cc0000", "#4e9a06", "#c4a000", "#3465a4", "#75507b", "#06989a", "#d3d7cf",
    "#555753", "#ef2929", "#8ae234", "#fce94f", "#729fcf", "#ad7fa8", "#34e2e2", "#eeeeec",
];
/// Both Solarized schemes have the same sixteen colours; only the two they are drawn on differ.
const SOLARIZED: [&str; 16] = [
    "#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#eee8d5",
    "#002b36", "#cb4b16", "#586e75", "#657b83", "#839496", "#6c71c4", "#93a1a1", "#fdf6e3",
];
const NORD: [&str; 16] = [
    "#3b4252", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#88c0d0", "#e5e9f0",
    "#4c566a", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#8fbcbb", "#eceff4",
];
const GRUVBOX: [&str; 16] = [
    "#282828", "#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#a89984",
    "#928374", "#fb4934", "#b8bb26", "#fabd2f", "#83a598", "#d3869b", "#8ec07c", "#ebdbb2",
];

impl Scheme {
    /// Every scheme, in the order the page shows them.
    pub const ALL: [Scheme; 6] = [
        Self::Rift,
        Self::RiftLight,
        Self::SolarizedDark,
        Self::SolarizedLight,
        Self::Nord,
        Self::GruvboxDark,
    ];

    /// The word for it, as the setting holds it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Rift => "rift",
            Self::RiftLight => "rift-light",
            Self::SolarizedDark => "solarized-dark",
            Self::SolarizedLight => "solarized-light",
            Self::Nord => "nord",
            Self::GruvboxDark => "gruvbox-dark",
        }
    }

    /// The name of it on the page.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rift => "Rift",
            Self::RiftLight => "Rift light",
            Self::SolarizedDark => "Solarized dark",
            Self::SolarizedLight => "Solarized light",
            Self::Nord => "Nord",
            Self::GruvboxDark => "Gruvbox dark",
        }
    }

    /// What the terminal is drawn on.
    #[must_use]
    pub const fn background(self) -> &'static str {
        match self {
            Self::Rift => "#040406",
            Self::RiftLight => "#ffffff",
            Self::SolarizedDark => "#002b36",
            Self::SolarizedLight => "#fdf6e3",
            Self::Nord => "#2e3440",
            Self::GruvboxDark => "#282828",
        }
    }

    /// What it writes in.
    #[must_use]
    pub const fn foreground(self) -> &'static str {
        match self {
            Self::Rift => "#d4d4d4",
            Self::RiftLight => "#1d1d1d",
            Self::SolarizedDark => "#839496",
            Self::SolarizedLight => "#657b83",
            Self::Nord => "#d8dee9",
            Self::GruvboxDark => "#ebdbb2",
        }
    }

    /// The sixteen colours a terminal program asks for by number. Rift's own has none of its own,
    /// so a terminal keeps the ones it ships with.
    #[must_use]
    pub const fn palette(self) -> Option<[&'static str; 16]> {
        match self {
            Self::Rift => None,
            Self::RiftLight => Some(TANGO),
            Self::SolarizedDark | Self::SolarizedLight => Some(SOLARIZED),
            Self::Nord => Some(NORD),
            Self::GruvboxDark => Some(GRUVBOX),
        }
    }

    /// The scheme a setting names, or Rift's own for anything else.
    #[must_use]
    pub fn from_setting(text: &str) -> Self {
        let word = text.trim();
        Self::ALL
            .into_iter()
            .find(|scheme| word.eq_ignore_ascii_case(scheme.word()))
            .unwrap_or_default()
    }

    /// The owner's scheme, or Rift's own when home has no setting.
    #[must_use]
    pub fn read() -> Self {
        read_setting(TERMINAL).map_or_else(Self::default, |text| Self::from_setting(&text))
    }

    /// The Ghostty configuration for it. The config in the image reads this file after itself, so
    /// these lines take the place of the ones it has.
    #[must_use]
    pub fn config(self) -> String {
        let mut config = format!(
            "# written from ~/{TERMINAL}: {}\nbackground = {}\nforeground = {}\n",
            self.word(),
            self.background(),
            self.foreground()
        );
        if let Some(palette) = self.palette() {
            for (at, colour) in palette.iter().enumerate() {
                let _ = writeln!(config, "palette = {at}={colour}");
            }
        }
        config
    }
}

/// Tell every terminal that is open to read its configuration again. Ghostty reloads it when it is
/// sent SIGUSR2. The name the kernel keeps for a running program is the file it was started from,
/// cut to fifteen characters, and the one in the image is the file the GTK wrapper starts, so the
/// name to look for is `ghostty` or `.ghostty-wrapp`.
fn reload_terminals() {
    let _ = Command::new("pkill")
        .args(["-USR2", "-x", r"\.?ghostty.*"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// The owner's interface text size in per cent, or a hundred when home has no setting.
#[must_use]
pub fn text() -> u32 {
    number(TEXT, TEXT_DEFAULT, TEXT_MOST).max(TEXT_LEAST)
}

/// An interface text size as dconf takes it: a factor with a point in it, since a whole number
/// would go into the database as a whole number and the key holds a double. The shortest form is
/// the one dconf prints back, so a key that already says it is left alone.
#[must_use]
pub fn text_factor(per_cent: u32) -> String {
    let (whole, rest) = (per_cent / 100, per_cent % 100);
    if rest == 0 {
        format!("{whole}.0")
    } else if rest % 10 == 0 {
        format!("{whole}.{}", rest / 10)
    } else {
        format!("{whole}.{rest:02}")
    }
}

/// Everything the desktop is drawn from, as the owner has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Look {
    /// Dark or light.
    pub theme: Theme,
    /// The accent colour.
    pub accent: Accent,
    /// The gap between windows, in pixels.
    pub gaps: u32,
    /// The corner radius of a window, in pixels.
    pub radius: u32,
    /// The size of the text of the interface, in per cent of the usual.
    pub text: u32,
    /// The colours a terminal is drawn in.
    pub terminal: Scheme,
    /// The picture or the colour behind the windows.
    pub wallpaper: Wallpaper,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            accent: Accent::default(),
            gaps: GAPS_DEFAULT,
            radius: RADIUS_DEFAULT,
            text: TEXT_DEFAULT,
            terminal: Scheme::default(),
            wallpaper: Wallpaper::read(),
        }
    }
}

impl Look {
    /// The look as the owner's files have it, each setting falling back to the default.
    #[must_use]
    pub fn read() -> Self {
        Self {
            theme: Theme::read(),
            accent: Accent::read(),
            gaps: number(GAPS, GAPS_DEFAULT, GAPS_MOST),
            radius: number(RADIUS, RADIUS_DEFAULT, RADIUS_MOST),
            text: text(),
            terminal: Scheme::read(),
            wallpaper: Wallpaper::read(),
        }
    }

    /// The accent as it is drawn in this theme.
    #[must_use]
    pub const fn accent_hex(&self) -> &'static str {
        self.accent.hex(self.theme)
    }

    /// The dconf keys for GTK and libadwaita apps, each with its value as dconf writes it.
    #[must_use]
    pub fn gtk(&self) -> [(&'static str, String); 4] {
        [
            (COLOR_SCHEME, format!("'{}'", self.theme.scheme())),
            (GTK_THEME, format!("'{}'", self.theme.gtk_theme())),
            (ACCENT_KEY, format!("'{}'", self.accent.word())),
            (TEXT_KEY, text_factor(self.text)),
        ]
    }

    /// Write the Ghostty configuration for the terminal colour scheme, and tell the terminals that
    /// are open to read it again. Ghostty reloads its configuration when it is sent SIGUSR2; a
    /// window opened after this reads the file anyway.
    ///
    /// # Errors
    ///
    /// A sentence when there is no home or the file could not be written.
    pub fn write_terminal(&self) -> Result<(), String> {
        let path = home()
            .ok_or("There is no home to write the terminal colours into.")?
            .join(TERMINAL_CONFIG);
        if write_beside(&path, &self.terminal.config())? {
            reload_terminals();
        }
        Ok(())
    }

    /// The part of Horizon's config for this look. The system config is dark with the blue accent
    /// and the gap it ships, so the part says what differs: the focus ring in the accent, the gap
    /// and the corner radius whatever they are, the light desktop under a picture on light, and a
    /// colour in the desktop's place when the wallpaper is one.
    #[must_use]
    pub fn horizon(&self) -> String {
        let mut part = format!(
            "// written from ~/.config/rift: {}, {}, gaps {}, radius {}, {}\n",
            self.theme.word(),
            self.accent.word(),
            self.gaps,
            self.radius,
            self.wallpaper.setting().replace('\n', " ")
        );
        let background = match &self.wallpaper {
            Wallpaper::Color(color) => Some(color.as_str()),
            Wallpaper::Picture(_) => (self.theme == Theme::Light).then_some(LIGHT_BACKGROUND),
        };
        part.push_str("layout {\n");
        let _ = writeln!(part, "    gaps {}", self.gaps);
        if let Some(background) = background {
            let _ = writeln!(part, "    background-color \"{background}\"");
        }
        let _ = writeln!(
            part,
            "    focus-ring {{\n        active-color \"{}\"\n    }}",
            self.accent_hex()
        );
        part.push_str("}\n");
        if self.radius > 0 {
            let _ = writeln!(
                part,
                "window-rule {{\n    geometry-corner-radius {}\n    clip-to-geometry true\n}}",
                self.radius
            );
        }
        match &self.wallpaper {
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

    /// Write the part of Horizon's config. Horizon reads its config again when the file changes,
    /// so the desktop follows at once.
    ///
    /// # Errors
    ///
    /// A sentence when there is no home or the file could not be written.
    pub fn write_horizon(&self) -> Result<(), String> {
        let path = home()
            .ok_or("There is no home to write the compositor's part into.")?
            .join(HORIZON_PART);
        write_beside(&path, &self.horizon()).map(|_| ())
    }

    /// Hand the look on to GTK and to Horizon. A key or a file that already says it is left alone,
    /// so running apps get no change signal for nothing.
    ///
    /// # Errors
    ///
    /// A sentence for each part that could not be written. The other parts are still written.
    pub fn apply(&self) -> Result<(), String> {
        let mut failed = Vec::new();
        if let Err(why) = self.write_horizon() {
            failed.push(why);
        }
        if let Err(why) = self.write_terminal() {
            failed.push(why);
        }
        for (key, value) in self.gtk() {
            if let Err(why) = write_key(key, &value) {
                failed.push(why);
            }
        }
        if failed.is_empty() {
            Ok(())
        } else {
            Err(failed.join(" "))
        }
    }

    /// Write every setting of the look into the owner's files, then hand it on.
    ///
    /// # Errors
    ///
    /// A sentence for each part that could not be written.
    pub fn save(&self) -> Result<(), String> {
        let mut failed = Vec::new();
        for (setting, value) in [
            (SETTING, self.theme.word().to_string()),
            (ACCENT, self.accent.word().to_string()),
            (GAPS, self.gaps.to_string()),
            (RADIUS, self.radius.to_string()),
            (TEXT, self.text.to_string()),
            (TERMINAL, self.terminal.word().to_string()),
            (wallpaper::SETTING, self.wallpaper.setting()),
        ] {
            if let Err(why) = write_home(setting, &format!("{value}\n")) {
                failed.push(why);
            }
        }
        if let Err(why) = self.apply() {
            failed.push(why);
        }
        if failed.is_empty() {
            Ok(())
        } else {
            Err(failed.join(" "))
        }
    }
}

/// Whether the terminal greets the first shell of a session with fastfetch. The fish function in
/// the image reads the same file.
#[must_use]
pub fn greeting() -> bool {
    read_setting(GREETING).is_none_or(|text| !text.trim().eq_ignore_ascii_case("off"))
}

/// Turn the terminal greeting on or off.
///
/// # Errors
///
/// A sentence when there is no home or the file could not be written.
pub fn set_greeting(on: bool) -> Result<(), String> {
    write_home(GREETING, if on { "on\n" } else { "off\n" })
}

/// Hand the owner's look on to GTK and to Horizon, with this theme in place of the one on file.
/// The shell calls this when it starts, since dconf may only be reachable once the session is up.
///
/// # Errors
///
/// A sentence for each part that could not be written.
pub fn apply(theme: Theme) -> Result<(), String> {
    Look {
        theme,
        ..Look::read()
    }
    .apply()
}

pub(crate) fn home() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

/// The text of a setting under home, when there is one.
fn read_setting(setting: &str) -> Option<String> {
    fs::read_to_string(home()?.join(setting)).ok()
}

/// A whole number a setting holds, kept inside its range, or the default.
fn number(setting: &str, default: u32, most: u32) -> u32 {
    read_setting(setting)
        .and_then(|text| text.trim().parse::<u32>().ok())
        .map_or(default, |value| value.min(most))
}

/// Write a setting under home.
fn write_home(setting: &str, text: &str) -> Result<(), String> {
    let path = home()
        .ok_or_else(|| format!("There is no home to keep {setting} in."))?
        .join(setting);
    write_beside(&path, text).map(|_| ())
}

/// Write the part of Horizon's config for a theme and a wallpaper, with the rest of the look as
/// the owner has it.
///
/// # Errors
///
/// A sentence when there is no home or the file could not be written.
pub fn write_horizon(theme: Theme, wallpaper: &Wallpaper) -> Result<(), String> {
    Look {
        theme,
        wallpaper: wallpaper.clone(),
        ..Look::read()
    }
    .write_horizon()
}

/// Write a file beside itself and rename it over the old one, so nothing ever reads half of it. A
/// file that already says it is left alone, and that answers false: nothing has to be told.
pub(crate) fn write_beside(path: &Path, text: &str) -> Result<bool, String> {
    if fs::read_to_string(path).is_ok_and(|old| old == text) {
        return Ok(false);
    }
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)
            .map_err(|e| format!("Could not make {}: {e}.", folder.display()))?;
    }
    let mut fresh = path.as_os_str().to_owned();
    fresh.push(".new");
    let fresh = PathBuf::from(fresh);
    fs::write(&fresh, text).map_err(|e| format!("Could not write {}: {e}.", fresh.display()))?;
    fs::rename(&fresh, path).map_err(|e| format!("Could not write {}: {e}.", path.display()))?;
    Ok(true)
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
    fn a_text_size_is_a_factor_with_a_point_in_it() {
        assert_eq!(text_factor(100), "1.0");
        assert_eq!(text_factor(105), "1.05");
        assert_eq!(text_factor(125), "1.25");
        assert_eq!(text_factor(150), "1.5");
        assert_eq!(text_factor(200), "2.0");
        // the page's slider starts at the size everything is drawn at and its steps reach the end
        assert_eq!(TEXT_LEAST, TEXT_DEFAULT);
        assert_eq!(TEXT_MOST % TEXT_STEP, TEXT_LEAST % TEXT_STEP);
        for per_cent in (TEXT_LEAST..=TEXT_MOST).step_by(TEXT_STEP as usize) {
            let factor = text_factor(per_cent);
            let (whole, rest) = factor.split_once('.').unwrap();
            let rest: u32 = rest.parse::<u32>().unwrap() * if rest.len() == 1 { 10 } else { 1 };
            assert_eq!(whole.parse::<u32>().unwrap() * 100 + rest, per_cent);
        }
    }

    #[test]
    fn a_scheme_goes_by_its_word_and_rift_is_the_default() {
        for scheme in Scheme::ALL {
            assert_eq!(Scheme::from_setting(scheme.word()), scheme);
            let config = scheme.config();
            assert!(config.contains(&format!("background = {}\n", scheme.background())));
            assert!(config.contains(&format!("foreground = {}\n", scheme.foreground())));
            match scheme.palette() {
                None => assert!(!config.contains("palette")),
                Some(palette) => {
                    for (at, colour) in palette.iter().enumerate() {
                        assert!(
                            config.contains(&format!("palette = {at}={colour}\n")),
                            "{at}"
                        );
                    }
                }
            }
        }
        assert_eq!(Scheme::from_setting(" Nord \n"), Scheme::Nord);
        assert_eq!(Scheme::from_setting("amber"), Scheme::Rift);
        assert_eq!(Scheme::from_setting(""), Scheme::Rift);
        assert_eq!(Scheme::default(), Scheme::Rift);
        // rift's own is what the image writes into ghostty's config, so the default changes nothing
        assert_eq!(Scheme::Rift.background(), "#040406");
        assert_eq!(Scheme::Rift.foreground(), "#d4d4d4");
    }

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
    fn an_accent_goes_by_its_word_and_blue_is_the_default() {
        for accent in Accent::ALL {
            assert_eq!(Accent::from_setting(accent.word()), accent);
            assert_eq!(accent.label().to_ascii_lowercase(), accent.word());
        }
        assert_eq!(Accent::from_setting(" Teal \n"), Accent::Teal);
        assert_eq!(Accent::from_setting("chartreuse"), Accent::Blue);
        assert_eq!(Accent::from_setting(""), Accent::Blue);
        assert_eq!(Accent::default(), Accent::Blue);
    }

    #[test]
    fn every_accent_has_two_colours_and_the_dark_one_is_lighter() {
        let channels = |hex: &str| {
            let hex = hex.strip_prefix('#').unwrap();
            assert_eq!(hex.len(), 6, "{hex}");
            (0..3)
                .map(|at| u32::from_str_radix(&hex[at * 2..at * 2 + 2], 16).unwrap())
                .collect::<Vec<_>>()
        };
        let mut seen = Vec::new();
        for accent in Accent::ALL {
            let light = channels(accent.hex(Theme::Light));
            let dark = channels(accent.hex(Theme::Dark));
            let lighter: u32 = dark.iter().sum();
            assert!(lighter > light.iter().sum::<u32>(), "{accent:?}");
            seen.push(accent.hex(Theme::Light));
            seen.push(accent.hex(Theme::Dark));
        }
        // the design rules name these two by hand, so they may never drift
        assert_eq!(Accent::Blue.hex(Theme::Light), "#3584e4");
        assert_eq!(Accent::Blue.hex(Theme::Dark), "#78aeed");
        seen.sort_unstable();
        let mut once = seen.clone();
        once.dedup();
        assert_eq!(seen, once, "two accents share a colour");
    }

    #[test]
    fn gtk_gets_a_scheme_a_theme_name_an_accent_and_the_text_size() {
        let dark = Look {
            theme: Theme::Dark,
            accent: Accent::Blue,
            ..plain(Wallpaper::Color("#242424".into()))
        };
        assert_eq!(
            dark.gtk(),
            [
                (
                    "/org/gnome/desktop/interface/color-scheme",
                    "'prefer-dark'".to_string()
                ),
                (
                    "/org/gnome/desktop/interface/gtk-theme",
                    "'Adwaita-dark'".to_string()
                ),
                (
                    "/org/gnome/desktop/interface/accent-color",
                    "'blue'".to_string()
                ),
                (
                    "/org/gnome/desktop/interface/text-scaling-factor",
                    "1.0".to_string()
                ),
            ]
        );
        let light = Look {
            theme: Theme::Light,
            accent: Accent::Teal,
            ..dark
        };
        assert_eq!(light.gtk()[0].1, "'default'");
        assert_eq!(light.gtk()[1].1, "'Adwaita'");
        assert_eq!(light.gtk()[2].1, "'teal'");
    }

    /// A look with the defaults and this wallpaper, without reading anything from home.
    fn plain(wallpaper: Wallpaper) -> Look {
        Look {
            theme: Theme::Dark,
            accent: Accent::Blue,
            gaps: GAPS_DEFAULT,
            radius: RADIUS_DEFAULT,
            text: TEXT_DEFAULT,
            terminal: Scheme::Rift,
            wallpaper,
        }
    }

    fn photo() -> Wallpaper {
        Wallpaper::Picture(PathBuf::from(
            "/run/current-system/sw/share/backgrounds/rift/earthset.jpg",
        ))
    }

    #[test]
    fn dark_with_a_picture_has_the_gap_the_ring_and_the_picture() {
        let dark = plain(photo()).horizon();
        assert_eq!(
            dark,
            "// written from ~/.config/rift: dark, blue, gaps 8, radius 0, \
             /run/current-system/sw/share/backgrounds/rift/earthset.jpg\n\
             layout {\n    gaps 8\n    focus-ring {\n        active-color \"#78aeed\"\n    }\n}\n\
             wallpaper \"/run/current-system/sw/share/backgrounds/rift/earthset.jpg\"\n"
        );
    }

    #[test]
    fn light_has_its_desktop_and_accent_under_the_picture() {
        let light = Look {
            theme: Theme::Light,
            ..plain(photo())
        }
        .horizon();
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
        let dark = plain(gray.clone()).horizon();
        assert!(dark.lines().next().unwrap().ends_with(", #242424"));
        assert!(dark.contains("background-color \"#242424\""));
        assert!(dark.ends_with("wallpaper null\n"));
        let light = Look {
            theme: Theme::Light,
            ..plain(gray)
        }
        .horizon();
        assert_eq!(light.matches("background-color").count(), 1);
        assert!(light.contains("background-color \"#242424\""));
        assert!(light.contains("active-color \"#3584e4\""));
        assert!(light.ends_with("wallpaper null\n"));
    }

    #[test]
    fn the_accent_the_gap_and_the_radius_go_into_the_part() {
        let part = Look {
            accent: Accent::Teal,
            gaps: 20,
            radius: 12,
            ..plain(photo())
        }
        .horizon();
        assert!(part.contains("    gaps 20\n"), "{part}");
        assert!(part.contains("active-color \"#68b4c1\""), "{part}");
        assert!(
            part.contains(
                "window-rule {\n    geometry-corner-radius 12\n    clip-to-geometry true\n}\n"
            ),
            "{part}"
        );
        assert_eq!(part.matches('{').count(), 3);
        assert_eq!(part.matches('}').count(), 3);
        // a square window is the system's own, so nothing is written for it
        let square = Look {
            radius: 0,
            ..plain(photo())
        }
        .horizon();
        assert!(!square.contains("window-rule"), "{square}");
    }

    #[test]
    fn a_path_is_escaped_inside_its_quotes() {
        assert_eq!(
            kdl_string(r#"/home/rift/a "b"\c.jpg"#),
            r#"/home/rift/a \"b\"\\c.jpg"#
        );
        let odd = Wallpaper::Picture(PathBuf::from("/home/rift/it's \"here\".png"));
        let part = plain(odd).horizon();
        assert!(
            part.ends_with("wallpaper \"/home/rift/it's \\\"here\\\".png\"\n"),
            "{part}"
        );
    }
}
