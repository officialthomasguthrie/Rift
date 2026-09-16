//! Icons by name, the way the XDG icon theme specification says: through the data directories,
//! the Adwaita theme and then hicolor. A status icon is the symbolic drawing the bar paints in
//! one colour, so those directories come first; an app icon is the app's own drawing in its own
//! colours, so that lookup starts at the app directories instead.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use iced::widget::{image, space, svg};
use iced::{Color, Element, Theme};

/// The themes to look in, in order. hicolor is where an app that ships one icon puts it.
const THEMES: [&str; 2] = ["Adwaita", "hicolor"];

/// The context directories inside a theme, in the order a 16 px bar icon wants them: the
/// symbolic ones, then the scalable ones, then the fixed sizes from the smallest that is still
/// sharp at 32 px.
const DIRECTORIES: [&str; 26] = [
    "symbolic/status",
    "symbolic/devices",
    "symbolic/actions",
    "symbolic/apps",
    "symbolic/categories",
    "symbolic/places",
    "symbolic/mimetypes",
    "symbolic/ui",
    "symbolic/legacy",
    "scalable/status",
    "scalable/devices",
    "scalable/apps",
    "scalable/actions",
    "scalable/categories",
    "scalable/places",
    "scalable/mimetypes",
    "scalable/legacy",
    "48x48/apps",
    "48x48/devices",
    "64x64/apps",
    "32x32/apps",
    "32x32/devices",
    "256x256/apps",
    "128x128/apps",
    "16x16/devices",
    "16x16/places",
];

/// The directories an app's own drawing comes from, in the order a 16 px row wants them: the
/// vector one, then the sizes from the one that scales down best. No symbolic directory: a theme
/// is searched before the directories are, so one symbolic drawing in the first theme would beat
/// every colour one in the next.
const APP_DIRECTORIES: [&str; 10] = [
    "scalable/apps",
    "48x48/apps",
    "32x32/apps",
    "64x64/apps",
    "24x24/apps",
    "16x16/apps",
    "128x128/apps",
    "256x256/apps",
    "512x512/apps",
    "scalable/mimetypes",
];

/// The file endings, vector first.
const ENDINGS: [&str; 2] = ["svg", "png"];

/// What a desktop entry with no icon of its own, or with one no theme has, is drawn with.
pub const UNKNOWN_APP: &str = "application-x-executable";

/// Where a named icon is, or `None` when no theme on this machine has it. The answer is
/// remembered: a view asks for the same few names on every frame.
#[must_use]
pub fn find(name: &str) -> Option<PathBuf> {
    static FOUND: Found = OnceLock::new();
    remembered(&FOUND, name, &DIRECTORIES)
}

/// The same lookup for an app icon: the app's own drawing in its own colours first, and only when
/// no theme has one, whatever the symbolic lookup finds, which the caller then paints in one
/// colour.
#[must_use]
pub fn app(name: &str) -> Option<PathBuf> {
    static FOUND: Found = OnceLock::new();
    remembered(&FOUND, name, &APP_DIRECTORIES).or_else(|| find(name))
}

/// What a lookup remembers: a name to where it was found, or to nothing.
type Found = OnceLock<Mutex<HashMap<String, Option<PathBuf>>>>;

/// One name in one order, remembered in the cache the caller keeps.
fn remembered(cache: &Found, name: &str, directories: &[&str]) -> Option<PathBuf> {
    let cache = cache.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(cache) = cache.lock() {
        if let Some(found) = cache.get(name) {
            return found.clone();
        }
    }
    let found = look(&bases(), name, directories);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(name.to_string(), found.clone());
    }
    found
}

/// The icon directories to search, most specific first: the owner's own, then the data
/// directories of the session, then the system profile, which is where the image's themes are
/// even when the environment is thin.
fn bases() -> Vec<PathBuf> {
    let mut bases = Vec::new();
    let mut push = |dir: PathBuf| {
        if !bases.contains(&dir) {
            bases.push(dir);
        }
    };
    if let Some(home) = std::env::var_os("XDG_DATA_HOME").filter(|dir| !dir.is_empty()) {
        push(PathBuf::from(home).join("icons"));
    } else if let Some(home) = std::env::var_os("HOME") {
        push(PathBuf::from(home).join(".local/share/icons"));
    }
    let dirs = std::env::var_os("XDG_DATA_DIRS")
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| OsString::from("/usr/local/share:/usr/share"));
    for dir in std::env::split_paths(&dirs) {
        push(dir.join("icons"));
    }
    push(PathBuf::from("/run/current-system/sw/share/icons"));
    bases
}

fn look(bases: &[PathBuf], name: &str, directories: &[&str]) -> Option<PathBuf> {
    // a name with a slash or a name that walks up is not an icon name
    if name.is_empty() || name.contains('/') || name.contains("..") {
        return None;
    }
    // a desktop entry may name an icon by its path
    let given = Path::new(name);
    if given.is_absolute() && given.is_file() {
        return Some(given.to_path_buf());
    }
    for base in bases {
        for theme in THEMES {
            for directory in directories {
                for ending in ENDINGS {
                    let path = base
                        .join(theme)
                        .join(directory)
                        .join(format!("{name}.{ending}"));
                    if path.is_file() {
                        return Some(path);
                    }
                }
            }
        }
    }
    None
}

/// An app's own drawing at this size, for a menu row or a dock item. A symbolic drawing has no
/// colours of its own, so it is painted in `text`, the way the status icons in the bar are, and a
/// name no theme has falls back to the drawing for a program with nothing of its own.
#[must_use]
pub fn draw<'a, Message: 'a>(text: Color, name: Option<&str>, size: f32) -> Element<'a, Message> {
    let found = name.and_then(app).or_else(|| app(UNKNOWN_APP));
    let Some(path) = found else {
        return space().width(size).height(size).into();
    };
    let colour = path.to_string_lossy().contains("symbolic").then_some(text);
    if path.extension().is_some_and(|ending| ending == "svg") {
        svg(svg::Handle::from_path(path))
            .width(size)
            .height(size)
            .style(move |_: &Theme, _| svg::Style { color: colour })
            .into()
    } else {
        image(image::Handle::from_path(path))
            .width(size)
            .height(size)
            .into()
    }
}

/// A symbolic icon by name at this size, painted in one colour, or the same space left empty when no
/// theme on the machine has it.
#[must_use]
pub fn symbolic<'a, Message: 'a>(colour: Color, name: &str, size: f32) -> Element<'a, Message> {
    let Some(path) = find(name) else {
        return space().width(size).height(size).into();
    };
    svg(svg::Handle::from_path(path))
        .width(size)
        .height(size)
        .style(move |_: &Theme, _| svg::Style {
            color: Some(colour),
        })
        .into()
}

/// The icon for a strength between 0 and 100, the way GNOME steps them.
#[must_use]
pub fn signal(kind: &str, strength: u8) -> String {
    let step = match strength {
        0..=4 => "none",
        5..=29 => "weak",
        30..=54 => "ok",
        55..=79 => "good",
        _ => "excellent",
    };
    format!("network-{kind}-signal-{step}-symbolic")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn theme(root: &Path, theme: &str, directory: &str, file: &str) {
        let dir = root.join(theme).join(directory);
        fs::create_dir_all(&dir).expect("the icon directory");
        fs::write(dir.join(file), "<svg/>").expect("the icon file");
    }

    fn temporary(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("lens-icons-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("the test directory");
        root
    }

    #[test]
    fn the_symbolic_directory_comes_first() {
        let root = temporary("order");
        theme(
            &root,
            "Adwaita",
            "symbolic/status",
            "audio-volume-high-symbolic.svg",
        );
        theme(
            &root,
            "Adwaita",
            "scalable/status",
            "audio-volume-high-symbolic.svg",
        );
        let found = look(
            std::slice::from_ref(&root),
            "audio-volume-high-symbolic",
            &DIRECTORIES,
        )
        .expect("the icon");
        assert!(found.to_string_lossy().contains("symbolic/status"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn adwaita_comes_before_hicolor_and_a_base_before_the_next() {
        let first = temporary("first");
        let second = temporary("second");
        theme(
            &second,
            "Adwaita",
            "symbolic/devices",
            "network-wired-symbolic.svg",
        );
        theme(
            &first,
            "hicolor",
            "symbolic/devices",
            "network-wired-symbolic.svg",
        );
        // the first base wins over the theme order
        let found = look(
            &[first.clone(), second.clone()],
            "network-wired-symbolic",
            &DIRECTORIES,
        )
        .expect("the icon");
        assert!(found.starts_with(&first));
        // and inside one base, Adwaita wins
        theme(
            &second,
            "hicolor",
            "symbolic/devices",
            "network-offline-symbolic.svg",
        );
        theme(
            &second,
            "Adwaita",
            "symbolic/devices",
            "network-offline-symbolic.svg",
        );
        let found = look(
            std::slice::from_ref(&second),
            "network-offline-symbolic",
            &DIRECTORIES,
        )
        .expect("the icon");
        assert!(found.to_string_lossy().contains("Adwaita"));
        let _ = fs::remove_dir_all(&first);
        let _ = fs::remove_dir_all(&second);
    }

    #[test]
    fn png_is_taken_when_there_is_no_svg() {
        let root = temporary("png");
        theme(&root, "hicolor", "48x48/apps", "firefox.png");
        let found = look(std::slice::from_ref(&root), "firefox", &DIRECTORIES).expect("the icon");
        assert!(found.to_string_lossy().ends_with("firefox.png"));
        assert!(
            look(
                std::slice::from_ref(&root),
                "nothing-like-this",
                &DIRECTORIES
            )
            .is_none()
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_app_icon_comes_in_colour_before_a_symbolic_one() {
        let root = temporary("apps");
        // the first theme has a symbolic drawing of it, the second the app's own in colour
        theme(&root, "Adwaita", "symbolic/apps", "firefox.svg");
        theme(&root, "hicolor", "48x48/apps", "firefox.png");
        let found =
            look(std::slice::from_ref(&root), "firefox", &APP_DIRECTORIES).expect("the icon");
        assert!(found.to_string_lossy().ends_with("48x48/apps/firefox.png"));
        // and the bar, which paints one colour over what it draws, takes the symbolic one
        let found = look(std::slice::from_ref(&root), "firefox", &DIRECTORIES).expect("the icon");
        assert!(found.to_string_lossy().contains("symbolic/apps"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_name_that_is_not_a_name_finds_nothing() {
        let root = temporary("names");
        theme(
            &root,
            "Adwaita",
            "symbolic/status",
            "battery-level-50-symbolic.svg",
        );
        assert!(look(std::slice::from_ref(&root), "", &DIRECTORIES).is_none());
        assert!(
            look(
                std::slice::from_ref(&root),
                "../../etc/passwd",
                &DIRECTORIES
            )
            .is_none()
        );
        assert!(
            look(
                std::slice::from_ref(&root),
                "symbolic/status/battery-level-50-symbolic",
                &DIRECTORIES
            )
            .is_none()
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_strength_steps_follow_the_signal() {
        assert_eq!(
            signal("wireless", 0),
            "network-wireless-signal-none-symbolic"
        );
        assert_eq!(
            signal("wireless", 20),
            "network-wireless-signal-weak-symbolic"
        );
        assert_eq!(
            signal("wireless", 40),
            "network-wireless-signal-ok-symbolic"
        );
        assert_eq!(
            signal("wireless", 70),
            "network-wireless-signal-good-symbolic"
        );
        assert_eq!(
            signal("wireless", 95),
            "network-wireless-signal-excellent-symbolic"
        );
        assert_eq!(
            signal("cellular", 60),
            "network-cellular-signal-good-symbolic"
        );
    }
}
