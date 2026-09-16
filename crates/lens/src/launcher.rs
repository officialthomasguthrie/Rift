//! The app launcher: desktop entries from the XDG data directories and the Flatpak exports,
//! matched by name in the routing module and started here as detached processes.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{env, fs};

/// An app from a desktop entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    /// The entry's file name without the ending, which is also the app id most apps give their
    /// windows.
    pub id: String,
    /// The Name field.
    pub name: String,
    /// The Exec field split into words, field codes removed.
    pub exec: Vec<String>,
    /// The Terminal field: the app wants a terminal around it.
    pub terminal: bool,
    /// The Icon field, a name to look up or a path.
    pub icon: Option<String>,
    /// The `StartupWMClass` field: the app id this app gives its windows when it is not the
    /// entry's own name.
    pub wm_class: Option<String>,
    /// The section of the menu the Categories field puts it in.
    pub category: Category,
}

/// The section of the Applications menu an app is listed under. The six the menu of GNOME
/// Classic shows on Tails, and Accessories takes whatever does not say where it belongs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    /// Anything else, and an entry with no category at all.
    Accessories,
    /// Network.
    Internet,
    /// Office.
    Office,
    /// Development.
    Programming,
    /// System and Settings.
    System,
    /// Utility: the small tools.
    Utilities,
}

// the menu is what lists the sections, and the menu is linux only
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl Category {
    /// The sections in the order the menu lists them.
    pub const ALL: [Self; 6] = [
        Self::Accessories,
        Self::Internet,
        Self::Office,
        Self::Programming,
        Self::System,
        Self::Utilities,
    ];

    /// The header over the section.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Accessories => "Accessories",
            Self::Internet => "Internet",
            Self::Office => "Office",
            Self::Programming => "Programming",
            Self::System => "System",
            Self::Utilities => "Utilities",
        }
    }

    /// Where a Categories field belongs. The words are tried in this order, so a terminal that
    /// says System;TerminalEmulator;Utility is a system app and an editor that says
    /// Development;TextEditor;Utility is programming.
    #[must_use]
    pub fn of(categories: &str) -> Self {
        let words: Vec<&str> = categories
            .split(';')
            .map(str::trim)
            .filter(|word| !word.is_empty())
            .collect();
        let has = |wanted: &str| words.contains(&wanted);
        if has("Network") {
            Self::Internet
        } else if has("Office") {
            Self::Office
        } else if has("Development") {
            Self::Programming
        } else if has("System") || has("Settings") {
            Self::System
        } else if has("Utility") {
            Self::Utilities
        } else {
            Self::Accessories
        }
    }
}

/// The terminal that wraps apps with `Terminal=true`. It is given the app's own class, so the
/// window belongs to that app and not to the terminal, and the dock has one item per app.
const TERMINAL: &str = "ghostty";

impl App {
    /// The class the terminal takes when it wraps this app. Ghostty reads a class as a GTK
    /// application id, which has to be dotted parts that each start with a letter: an entry named
    /// the way Flatpak names them already is one, and anything else goes under Rift's own name,
    /// the way the console's window does.
    #[must_use]
    pub fn class(&self) -> String {
        let plain: String = self
            .id
            .chars()
            .map(|letter| {
                if letter.is_ascii_alphanumeric() || letter == '-' || letter == '_' || letter == '.'
                {
                    letter
                } else {
                    '-'
                }
            })
            .collect();
        let parts: Vec<&str> = plain.split('.').collect();
        if parts.len() > 1
            && parts
                .iter()
                .all(|part| part.starts_with(|letter: char| letter.is_ascii_alphabetic()))
        {
            plain
        } else {
            format!("dev.rift.{}", plain.replace('.', "-"))
        }
    }
}

/// Every usable app in the data directories, sorted by name, one per entry id.
#[must_use]
pub fn load() -> Vec<App> {
    let mut apps = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in data_dirs() {
        let Ok(entries) = fs::read_dir(dir.join("applications")) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "desktop") {
                continue;
            }
            let Some(id) = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
            else {
                continue;
            };
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(app) = fs::read_to_string(&path)
                .ok()
                .as_deref()
                .and_then(|text| parse(&id, text))
            {
                apps.push(app);
            }
        }
    }
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps
}

/// The user's data directory first, then the system ones, then the two the Flatpak exports are
/// in. A session that never sourced the profile has neither of those in its data directories, and
/// an installed Flatpak belongs in the menu either way.
fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let home = env::var_os("HOME");
    match env::var_os("XDG_DATA_HOME") {
        Some(data) => dirs.push(PathBuf::from(data)),
        None => {
            if let Some(home) = home.as_ref() {
                dirs.push(Path::new(home).join(".local/share"));
            }
        }
    }
    let system = env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    dirs.extend(
        system
            .split(':')
            .filter(|d| !d.is_empty())
            .map(PathBuf::from),
    );
    if let Some(home) = home.as_ref() {
        dirs.push(Path::new(home).join(".local/share/flatpak/exports/share"));
    }
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share"));
    dirs.dedup();
    dirs
}

/// Read a desktop entry, whose file name without the ending is its id. `None` for anything that
/// is not an app to show: hidden entries, entries without a command, other types.
#[must_use]
pub fn parse(id: &str, text: &str) -> Option<App> {
    let mut in_entry = false;
    let mut name = None;
    let mut exec = None;
    let mut terminal = false;
    let mut kind = None;
    let mut icon = None;
    let mut wm_class = None;
    let mut categories = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Name" => name = Some(value.trim().to_string()),
            "Exec" => exec = Some(value.trim().to_string()),
            "Type" => kind = Some(value.trim().to_string()),
            "Terminal" => terminal = value.trim() == "true",
            "Icon" if !value.trim().is_empty() => icon = Some(value.trim().to_string()),
            "StartupWMClass" if !value.trim().is_empty() => {
                wm_class = Some(value.trim().to_string());
            }
            "Categories" => categories = value.trim().to_string(),
            "NoDisplay" | "Hidden" if value.trim() == "true" => return None,
            _ => {}
        }
    }
    if kind.as_deref() != Some("Application") {
        return None;
    }
    let exec = split_exec(&exec?);
    if exec.is_empty() {
        return None;
    }
    Some(App {
        id: id.to_string(),
        name: name?,
        exec,
        terminal,
        icon,
        wm_class,
        category: Category::of(&categories),
    })
}

/// Split an Exec value the way the desktop entry spec says: quoted words, backslash escapes
/// inside quotes, and the field codes (`%f`, `%U` and friends) dropped.
#[must_use]
pub fn split_exec(value: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quoted = false;
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                in_word = true;
            }
            '\\' if quoted => {
                if let Some(next) = chars.next() {
                    word.push(next);
                }
            }
            '%' if !quoted => {
                if chars.next() == Some('%') {
                    word.push('%');
                }
            }
            c if c.is_whitespace() && !quoted => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            c => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(word);
    }
    words.retain(|w| !w.is_empty());
    words
}

/// Start the app and let it go.
///
/// # Errors
///
/// When the program cannot be started.
pub fn launch(app: &App) -> Result<(), String> {
    let class = app.terminal.then(|| format!("--class={}", app.class()));
    let mut words: Vec<&str> = Vec::new();
    if let Some(class) = class.as_deref() {
        words.extend([TERMINAL, class, "-e"]);
    }
    words.extend(app.exec.iter().map(String::as_str));
    let (program, args) = words
        .split_first()
        .ok_or_else(|| "Nothing to run".to_string())?;
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| format!("Could not start {}: {e}", app.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_entry() {
        let text = "[Desktop Entry]\nType=Application\nName=Firefox\nExec=firefox %u\nIcon=firefox\nCategories=Network;WebBrowser;\n\n[Desktop Action new-window]\nName=New window\nExec=firefox --new-window\n";
        let app = parse("firefox", text).unwrap();
        assert_eq!(app.id, "firefox");
        assert_eq!(app.name, "Firefox");
        assert_eq!(app.exec, ["firefox"]);
        assert!(!app.terminal);
        assert_eq!(app.icon.as_deref(), Some("firefox"));
        assert_eq!(app.category, Category::Internet);
        // with no StartupWMClass the entry's id is what its windows are called
        assert!(app.wm_class.is_none());
    }

    #[test]
    fn an_entry_that_says_what_its_windows_are_called() {
        let app = parse(
            "code",
            "[Desktop Entry]\nType=Application\nName=Code\nExec=code\nStartupWMClass=Code\n",
        )
        .unwrap();
        assert_eq!(app.wm_class.as_deref(), Some("Code"));
    }

    #[test]
    fn an_entry_without_an_icon_or_a_category() {
        let app = parse(
            "thing",
            "[Desktop Entry]\nType=Application\nName=Thing\nExec=thing\nIcon=\n",
        )
        .unwrap();
        assert!(app.icon.is_none());
        assert_eq!(app.category, Category::Accessories);
    }

    #[test]
    fn the_class_the_terminal_takes_reads_as_an_application_id() {
        let of = |id: &str| {
            parse(
                id,
                "[Desktop Entry]\nType=Application\nName=X\nExec=x\nTerminal=true\n",
            )
            .unwrap()
            .class()
        };
        // an entry named the way flatpak names them is already one
        assert_eq!(of("com.mitchellh.ghostty"), "com.mitchellh.ghostty");
        // a plain name is not: one part, and a part that starts with a digit is not a part
        assert_eq!(of("Helix"), "dev.rift.Helix");
        assert_eq!(of("btop"), "dev.rift.btop");
        assert_eq!(of("7zip.gui"), "dev.rift.7zip-gui");
        assert_eq!(of("my app+"), "dev.rift.my-app-");
    }

    #[test]
    fn the_categories_the_menu_knows() {
        assert_eq!(Category::of("Network;WebBrowser;"), Category::Internet);
        assert_eq!(Category::of("Office;WordProcessor;"), Category::Office);
        assert_eq!(
            Category::of("Development;TextEditor;Utility;"),
            Category::Programming
        );
        assert_eq!(
            Category::of("System;TerminalEmulator;Utility;"),
            Category::System
        );
        assert_eq!(Category::of("GTK;Settings;"), Category::System);
        assert_eq!(Category::of("Utility;Calculator;"), Category::Utilities);
        assert_eq!(
            Category::of("Graphics;RasterGraphics;"),
            Category::Accessories
        );
        assert_eq!(Category::of(""), Category::Accessories);
        // the words come in any order and the list may be padded
        assert_eq!(Category::of(" GTK ; Network "), Category::Internet);
        // a word that only starts with one the menu knows is not that word
        assert_eq!(Category::of("Networking;"), Category::Accessories);
    }

    #[test]
    fn skips_what_is_not_an_app() {
        assert!(parse("docs", "[Desktop Entry]\nType=Link\nName=Docs\nURL=x\n").is_none());
        assert!(
            parse(
                "hidden",
                "[Desktop Entry]\nType=Application\nName=Hidden\nExec=x\nNoDisplay=true\n"
            )
            .is_none()
        );
        assert!(
            parse(
                "none",
                "[Desktop Entry]\nType=Application\nName=No command\n"
            )
            .is_none()
        );
    }

    #[test]
    fn terminal_apps_are_marked() {
        let app = parse(
            "Helix",
            "[Desktop Entry]\nType=Application\nName=Helix\nExec=hx %F\nTerminal=true\n",
        )
        .unwrap();
        assert!(app.terminal);
        assert_eq!(app.exec, ["hx"]);
    }

    #[test]
    fn exec_splitting() {
        assert_eq!(split_exec("firefox %u"), ["firefox"]);
        assert_eq!(
            split_exec("/usr/bin/env FOO=1 app --flag %F"),
            ["/usr/bin/env", "FOO=1", "app", "--flag"]
        );
        assert_eq!(
            split_exec("\"/opt/My App/run\" --name \"a b\" 100%%"),
            ["/opt/My App/run", "--name", "a b", "100%"]
        );
        assert_eq!(
            split_exec("\"quote \\\"inside\\\"\" x"),
            ["quote \"inside\"", "x"]
        );
    }
}
