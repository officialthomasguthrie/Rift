//! Symbolic icons by name, the way the XDG icon theme specification says: through the data
//! directories, the Adwaita theme and then hicolor. Every icon here is a symbolic drawing painted
//! in one colour, so the lookup is the short one. The shell has the fuller version of this, for
//! app icons as well; P3.10 joins the two.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use iced::widget::{space, svg};
use iced::{Color, Element, Theme};

/// The themes to look in, in order.
const THEMES: [&str; 2] = ["Adwaita", "hicolor"];

/// The context directories inside a theme, in the order a 16 px row wants them.
const DIRECTORIES: [&str; 9] = [
    "symbolic/status",
    "symbolic/devices",
    "symbolic/actions",
    "symbolic/apps",
    "symbolic/categories",
    "symbolic/places",
    "symbolic/mimetypes",
    "symbolic/ui",
    "symbolic/legacy",
];

/// Where a named icon is, or `None` when no theme on this machine has it. The answer is
/// remembered: a view asks for the same few names on every frame.
#[must_use]
pub fn find(name: &str) -> Option<PathBuf> {
    static FOUND: OnceLock<Mutex<HashMap<String, Option<PathBuf>>>> = OnceLock::new();
    let cache = FOUND.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(cache) = cache.lock() {
        if let Some(found) = cache.get(name) {
            return found.clone();
        }
    }
    let found = look(&bases(), name);
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

fn look(bases: &[PathBuf], name: &str) -> Option<PathBuf> {
    // a name with a slash or a name that walks up is not an icon name
    if name.is_empty() || name.contains('/') || name.contains("..") {
        return None;
    }
    for base in bases {
        for theme in THEMES {
            for directory in DIRECTORIES {
                let path = base.join(theme).join(directory).join(format!("{name}.svg"));
                if path.is_file() {
                    return Some(path);
                }
            }
        }
    }
    None
}

/// A symbolic icon by name at this size, painted in one colour, or the same space left empty when
/// no theme on the machine has it.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_that_is_a_path_is_no_icon_name() {
        let bases = vec![PathBuf::from("/nowhere")];
        assert_eq!(look(&bases, ""), None);
        assert_eq!(look(&bases, "../../etc/passwd"), None);
        assert_eq!(look(&bases, "/etc/passwd"), None);
    }

    #[test]
    fn the_search_starts_at_home_and_ends_at_the_system_profile() {
        let bases = bases();
        assert_eq!(
            bases.last(),
            Some(&PathBuf::from("/run/current-system/sw/share/icons"))
        );
        assert!(bases.iter().all(|base| base.ends_with("icons")));
    }

    #[test]
    fn an_icon_in_a_theme_is_found_by_its_name() {
        let dir = std::env::temp_dir().join(format!("rift-settings-icons-{}", std::process::id()));
        let folder = dir.join("Adwaita/symbolic/status");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("battery-symbolic.svg"), "<svg/>").unwrap();
        let bases = vec![dir.clone()];
        assert_eq!(
            look(&bases, "battery-symbolic"),
            Some(folder.join("battery-symbolic.svg"))
        );
        assert_eq!(look(&bases, "battery"), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
