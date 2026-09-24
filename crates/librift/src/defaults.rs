//! Which app opens each kind of file, and each kind of link. The choice is kept the way the
//! freedesktop specification for default apps keeps it, in `mimeapps.list` files that xdg-mime and
//! `GLib` both read: the image sets its defaults in /etc/xdg, and the owner's choices go into
//! ~/.config/mimeapps.list, which is read first.
//!
//! A kind is the types the image sets a default for, the way basics.nix lists them. Its first type
//! is the one whose app the kind is said to open with, and choosing an app makes it the default for
//! every type of the kind at once, the way GNOME Settings does for the types of a row. The default
//! for a type is found the way xdg-mime finds it, so a page and a terminal always agree.

use std::env;
use std::fs;
use std::path::PathBuf;

use crate::apps::App;
use crate::files::mime;

/// A kind of file, or of link, that one app opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Pages, and the links an app hands on.
    Web,
    /// A mail address an app hands on.
    Mail,
    /// Sound files.
    Music,
    /// Video files.
    Video,
    /// Photographs and other pictures.
    Pictures,
    /// Documents to read: pdf, djvu and comic books.
    Documents,
    /// Archives and compressed files.
    Archives,
    /// Plain text and source code.
    Text,
}

impl Kind {
    /// Every kind, in the order a page lists them.
    pub const ALL: [Self; 8] = [
        Self::Web,
        Self::Mail,
        Self::Music,
        Self::Video,
        Self::Pictures,
        Self::Documents,
        Self::Archives,
        Self::Text,
    ];

    /// The word `rift-settings --set` takes and `--state` prints.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Mail => "mail",
            Self::Music => "music",
            Self::Video => "video",
            Self::Pictures => "pictures",
            Self::Documents => "documents",
            Self::Archives => "archives",
            Self::Text => "text",
        }
    }

    /// The name at the left of its row.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Web => "Web",
            Self::Mail => "Mail",
            Self::Music => "Music",
            Self::Video => "Video",
            Self::Pictures => "Pictures",
            Self::Documents => "Documents",
            Self::Archives => "Archives",
            Self::Text => "Text",
        }
    }

    /// The types of the kind, the one whose app the kind opens with first. The lists are the ones
    /// nix/profiles/basics.nix sets the image's defaults for.
    #[must_use]
    pub const fn types(self) -> &'static [&'static str] {
        match self {
            Self::Web => &[
                "x-scheme-handler/http",
                "x-scheme-handler/https",
                "text/html",
                "application/xhtml+xml",
            ],
            Self::Mail => &["x-scheme-handler/mailto"],
            Self::Music => &[
                "audio/mpeg",
                "audio/flac",
                "audio/x-vorbis+ogg",
                "audio/ogg",
                "audio/x-wav",
                "audio/mp4",
                "audio/x-opus+ogg",
                "audio/x-m4b",
            ],
            Self::Video => &[
                "video/mp4",
                "video/x-matroska",
                "video/webm",
                "video/quicktime",
                "video/mpeg",
                "video/x-msvideo",
                "video/ogg",
            ],
            Self::Pictures => &[
                "image/jpeg",
                "image/png",
                "image/gif",
                "image/webp",
                "image/tiff",
                "image/bmp",
                "image/avif",
                "image/heic",
                "image/jxl",
                "image/svg+xml",
                "image/vnd.microsoft.icon",
            ],
            Self::Documents => &[
                "application/pdf",
                "image/vnd.djvu",
                "application/vnd.comicbook+zip",
                "application/x-cbz",
                "application/x-cbr",
            ],
            Self::Archives => &[
                "application/zip",
                "application/x-tar",
                "application/x-compressed-tar",
                "application/gzip",
                "application/x-xz",
                "application/zstd",
                "application/x-bzip2",
                "application/x-7z-compressed",
            ],
            // plain text, and every kind of source the editors in the image say they open, so the
            // one chosen opens all of them and not only the files that say they are plain text
            Self::Text => &[
                "text/plain",
                "text/markdown",
                "text/x-c",
                "text/x-csrc",
                "text/x-chdr",
                "text/x-c++",
                "text/x-c++src",
                "text/x-c++hdr",
                "text/x-java",
                "text/x-makefile",
                "text/x-pascal",
                "text/x-tcl",
                "text/x-tex",
                "text/x-moc",
                "application/x-shellscript",
            ],
        }
    }

    /// The type whose app the kind opens with.
    #[must_use]
    pub const fn main_type(self) -> &'static str {
        self.types()[0]
    }

    /// The kind a word names.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        let word = word.trim();
        Self::ALL
            .into_iter()
            .find(|kind| word.eq_ignore_ascii_case(kind.word()))
    }
}

/// What opens one kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opens {
    /// The kind.
    pub kind: Kind,
    /// The desktop id of the app its first type opens with, `firefox.desktop`, as xdg-mime prints
    /// it. `None` when nothing does.
    pub default: Option<String>,
    /// The ids of the apps that say they open its first type, by name, and the default's too when
    /// it is installed and does not say so.
    pub apps: Vec<String>,
}

impl Opens {
    /// The id of the entry the default is, `firefox` for `firefox.desktop`.
    #[must_use]
    pub fn default_id(&self) -> Option<&str> {
        self.default.as_deref().map(entry_id)
    }

    /// Whether there is another app to choose: one that is not the default.
    #[must_use]
    pub fn choosable(&self) -> bool {
        self.apps
            .iter()
            .any(|id| Some(id.as_str()) != self.default_id())
    }
}

/// The id of an entry from its desktop id: the file name without `.desktop`.
#[must_use]
pub fn entry_id(desktop: &str) -> &str {
    desktop.strip_suffix(".desktop").unwrap_or(desktop)
}

/// The kind a folder is, which the app that opens folders says it opens.
pub const FOLDER: &str = "inode/directory";

/// The app that opens a folder, which is the file manager: the default for [`FOLDER`], found the
/// way xdg-mime finds one. The shell opens a place with it. `None` when there is no such app.
#[must_use]
pub fn manager(apps: &[App]) -> Option<App> {
    let desktop = Found::read().default_for(FOLDER, apps)?;
    apps.iter()
        .find(|app| app.id == entry_id(&desktop))
        .cloned()
}

/// The lists and the caches as they are now, read once and asked about every type.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Found {
    /// The text of each `mimeapps.list` there is, in the order they count.
    pub lists: Vec<String>,
    /// The text of each `defaults.list` and `mimeinfo.cache` there is, in the order xdg-mime
    /// looks at them.
    pub caches: Vec<String>,
}

impl Found {
    /// Read every list and cache there is now. They are small files.
    #[must_use]
    pub fn read() -> Self {
        let text = |paths: Vec<PathBuf>| -> Vec<String> {
            paths
                .iter()
                .filter_map(|path| fs::read_to_string(path).ok())
                .collect()
        };
        Self {
            lists: text(list_paths()),
            caches: text(cache_paths()),
        }
    }

    /// The desktop id of the app that opens `mime`, the way xdg-mime finds it: the first app a list
    /// names for it that is installed, in the order the lists count; then the first app a cache
    /// names for it; then the first installed app, by id, whose entry says it opens it. `None` when
    /// nothing does.
    #[must_use]
    pub fn default_for(&self, mime: &str, apps: &[App]) -> Option<String> {
        let installed = |desktop: &str| apps.iter().any(|app| app.id == entry_id(desktop));
        let listed = self
            .lists
            .iter()
            .filter_map(|text| listed(text, mime))
            .find_map(|named| named.into_iter().find(|desktop| installed(desktop)));
        if listed.is_some() {
            return listed;
        }
        let cached = self.caches.iter().find_map(|text| cached(text, mime));
        if cached.is_some() {
            return cached;
        }
        let mut saying: Vec<&App> = apps
            .iter()
            .filter(|app| app.types.iter().any(|kind| kind == mime))
            .collect();
        saying.sort_by(|one, other| one.id.cmp(&other.id));
        saying.first().map(|app| format!("{}.desktop", app.id))
    }

    /// The app that opens a kind of file: the default for the kind itself, or else for a kind it
    /// is a kind of, the way xdg-mime looks. `besides` is an app that never counts as the answer,
    /// which is how the file manager keeps from opening a file with itself. `None` when no app
    /// installed here opens it.
    #[must_use]
    pub fn opener(
        &self,
        kind: &str,
        apps: &[App],
        types: &mime::Database,
        besides: Option<&str>,
    ) -> Option<App> {
        std::iter::once(types.canonical(kind).to_string())
            .chain(types.parents(kind))
            .find_map(|mime| self.default_for(&mime, apps))
            .and_then(|desktop| {
                apps.iter()
                    .find(|app| app.id == entry_id(&desktop) && Some(app.id.as_str()) != besides)
                    .cloned()
            })
    }

    /// What opens each kind, with the apps that say they open it.
    #[must_use]
    pub fn opens(&self, apps: &[App]) -> Vec<Opens> {
        Kind::ALL
            .into_iter()
            .map(|kind| {
                let default = self.default_for(kind.main_type(), apps);
                let mut ids: Vec<String> = apps
                    .iter()
                    .filter(|app| app.types.iter().any(|mime| mime == kind.main_type()))
                    .map(|app| app.id.clone())
                    .collect();
                if let Some(id) = default.as_deref().map(entry_id)
                    && !ids.iter().any(|known| known == id)
                    && apps.iter().any(|app| app.id == id)
                {
                    ids.push(id.to_string());
                }
                Opens {
                    kind,
                    default,
                    apps: ids,
                }
            })
            .collect()
    }
}

/// The apps the first line for `mime` in the Default Applications group of one list names, in its
/// order. xdg-mime reads the first such line of a file and no other, and so does this.
#[must_use]
pub fn listed(text: &str, mime: &str) -> Option<Vec<String>> {
    let key = format!("{mime}=");
    let mut in_defaults = false;
    for line in text.lines() {
        if line.starts_with("[Default Applications]") {
            in_defaults = true;
        } else if line.starts_with('[') {
            in_defaults = false;
        } else if in_defaults && let Some(value) = line.strip_prefix(&key) {
            return Some(
                value
                    .split(';')
                    .map(str::trim)
                    .filter(|desktop| !desktop.is_empty())
                    .map(ToString::to_string)
                    .collect(),
            );
        }
    }
    None
}

/// The first app the first line for `mime` in a `defaults.list` or a `mimeinfo.cache` names.
#[must_use]
pub fn cached(text: &str, mime: &str) -> Option<String> {
    let key = format!("{mime}=");
    text.lines()
        .find_map(|line| line.strip_prefix(&key))
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .filter(|desktop| !desktop.is_empty())
        .map(ToString::to_string)
}

/// Make `app` the default for every type of `kind` in the owner's `mimeapps.list`, the one
/// `xdg-mime default` writes to. `app` is an entry's id or its desktop id.
///
/// # Errors
///
/// A sentence when there is no home or the file could not be read or written.
pub fn choose(kind: Kind, app: &str) -> Result<(), String> {
    let path = owner_list().ok_or("There is no home to keep the default apps in.")?;
    // xdg-mime writes through a link to where it points, and so does this
    let path = if path.is_symlink() {
        fs::canonicalize(&path).map_err(|e| format!("Could not follow {}: {e}.", path.display()))?
    } else {
        path
    };
    let old = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("Could not read {}: {e}.", path.display())),
    };
    let desktop = format!("{}.desktop", entry_id(app.trim()));
    crate::appearance::write_beside(&path, &with_defaults(&old, kind.types(), &desktop)).map(|_| ())
}

/// The text of a `mimeapps.list` with `desktop` as the default for each of `types`: in the Default
/// Applications group the first line for a type takes the new value and any later line for it goes,
/// and a type the group lacks is added after its last line. A file without the group gets one at
/// its end. Every other line stays as it was.
#[must_use]
pub fn with_defaults(text: &str, types: &[&str], desktop: &str) -> String {
    let line_of = |mime: &str| format!("{mime}={desktop}");
    let mut out: Vec<String> = Vec::new();
    let mut written: Vec<&str> = Vec::new();
    let mut in_defaults = false;
    // how many Default Applications groups have begun, and where a type the first one lacks goes:
    // after its last line that is not blank
    let mut groups = 0;
    let mut insert_at: Option<usize> = None;
    for line in text.lines() {
        if line.starts_with('[') {
            in_defaults = line.starts_with("[Default Applications]");
            out.push(line.to_string());
            if in_defaults {
                groups += 1;
                if groups == 1 {
                    insert_at = Some(out.len());
                }
            }
            continue;
        }
        let named = if in_defaults {
            types
                .iter()
                .copied()
                .find(|mime| line.starts_with(&format!("{mime}=")))
        } else {
            None
        };
        match named {
            Some(mime) if written.contains(&mime) => continue,
            Some(mime) => {
                written.push(mime);
                out.push(line_of(mime));
            }
            None => out.push(line.to_string()),
        }
        if in_defaults && groups == 1 && !line.trim().is_empty() {
            insert_at = Some(out.len());
        }
    }
    let missing: Vec<String> = types
        .iter()
        .filter(|mime| !written.contains(mime))
        .map(|mime| line_of(mime))
        .collect();
    if let Some(at) = insert_at {
        out.splice(at..at, missing);
    } else {
        if out.iter().any(|line| !line.trim().is_empty()) {
            out.push(String::new());
        } else {
            out.clear();
        }
        out.push("[Default Applications]".to_string());
        out.extend(missing);
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

/// The owner's list, the first one read and the one a choice is written to.
fn owner_list() -> Option<PathBuf> {
    config_home().map(|folder| folder.join("mimeapps.list"))
}

/// Every `mimeapps.list` in the order they count: the owner's configuration, the system's, the
/// owner's data and the system's, each with a list for the desktop that is running before the
/// plain one.
fn list_paths() -> Vec<PathBuf> {
    let desktops = current_desktops();
    let mut folders: Vec<PathBuf> = config_home().into_iter().collect();
    folders.extend(config_dirs());
    folders.extend(data_home().map(|folder| folder.join("applications")));
    folders.extend(
        data_dirs()
            .into_iter()
            .map(|folder| folder.join("applications")),
    );
    folders
        .iter()
        .flat_map(|folder| {
            desktops
                .iter()
                .map(|desktop| folder.join(format!("{desktop}-mimeapps.list")))
                .chain(std::iter::once(folder.join("mimeapps.list")))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Every `defaults.list` and `mimeinfo.cache` in the order xdg-mime looks at them: the owner's data
/// first, then the system's, with the menu prefix's own files before the plain ones.
fn cache_paths() -> Vec<PathBuf> {
    let prefix = env::var("XDG_MENU_PREFIX").unwrap_or_default();
    let mut prefixes = vec![String::new()];
    if !prefix.is_empty() {
        prefixes.insert(0, prefix);
    }
    let mut folders: Vec<PathBuf> = data_home().into_iter().collect();
    folders.extend(data_dirs());
    let mut paths = Vec::new();
    for folder in &folders {
        let apps = folder.join("applications");
        for prefix in &prefixes {
            paths.push(apps.join(format!("{prefix}defaults.list")));
            paths.push(apps.join(format!("{prefix}mimeinfo.cache")));
        }
    }
    paths
}

/// The desktops the session says it is, in lower case, for their own lists.
fn current_desktops() -> Vec<String> {
    env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .map(str::trim)
        .filter(|desktop| !desktop.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// A variable that names a folder, when it is set and not empty.
fn folder_in(variable: &str) -> Option<PathBuf> {
    env::var_os(variable)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Folders a variable lists, or the ones the specification gives when it is not set.
fn folders_in(variable: &str, otherwise: &str) -> Vec<PathBuf> {
    let listed = env::var(variable)
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| otherwise.to_string());
    listed
        .split(':')
        .filter(|folder| !folder.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn home() -> Option<PathBuf> {
    folder_in("HOME")
}

fn config_home() -> Option<PathBuf> {
    folder_in("XDG_CONFIG_HOME").or_else(|| home().map(|home| home.join(".config")))
}

fn config_dirs() -> Vec<PathBuf> {
    folders_in("XDG_CONFIG_DIRS", "/etc/xdg")
}

fn data_home() -> Option<PathBuf> {
    folder_in("XDG_DATA_HOME").or_else(|| home().map(|home| home.join(".local/share")))
}

fn data_dirs() -> Vec<PathBuf> {
    folders_in("XDG_DATA_DIRS", "/usr/local/share/:/usr/share/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::Category;

    fn app(id: &str, types: &[&str]) -> App {
        App {
            id: id.to_string(),
            name: id.to_string(),
            exec: vec![id.to_lowercase()],
            terminal: false,
            icon: None,
            wm_class: None,
            category: Category::Accessories,
            types: types.iter().map(|&mime| mime.to_string()).collect(),
            line: String::new(),
        }
    }

    fn installed() -> Vec<App> {
        vec![
            app("dev.zed.Zed", &["text/plain", "application/x-zerosize"]),
            app("firefox", &["text/html", "x-scheme-handler/http"]),
            app("Helix", &["text/plain", "text/x-csrc"]),
            app("nvim", &["text/plain", "text/x-csrc"]),
            app("org.gnome.Loupe", &["image/jpeg", "image/png"]),
        ]
    }

    #[test]
    fn every_kind_has_a_word_a_name_and_types_of_its_own() {
        let mut every: Vec<&str> = Kind::ALL
            .iter()
            .flat_map(|kind| kind.types())
            .copied()
            .collect();
        let before = every.len();
        every.sort_unstable();
        every.dedup();
        assert_eq!(every.len(), before, "two kinds share a type");
        for kind in Kind::ALL {
            assert_eq!(Kind::from_word(kind.word()), Some(kind));
            assert_eq!(kind.main_type(), kind.types()[0]);
            assert!(kind.label().is_ascii());
        }
        assert_eq!(Kind::from_word(" Text\n"), Some(Kind::Text));
        assert_eq!(Kind::from_word("calendar"), None);
    }

    #[test]
    fn a_list_names_the_first_line_of_its_default_group_and_no_other() {
        let text = "[Added Associations]\ntext/plain=nvim.desktop;\n\n[Default Applications]\n\
                    text/plain=Helix.desktop;dev.zed.Zed.desktop;\ntext/plain=nvim.desktop\n\
                    image/png = org.gnome.Loupe.desktop\n[Removed Associations]\n\
                    image/jpeg=org.gnome.Loupe.desktop\n";
        assert_eq!(
            listed(text, "text/plain"),
            Some(vec![
                "Helix.desktop".to_string(),
                "dev.zed.Zed.desktop".to_string()
            ])
        );
        // a key with a space before its equals sign is not the key, as xdg-mime reads it
        assert_eq!(listed(text, "image/png"), None);
        // and a line outside the group is not a default
        assert_eq!(listed(text, "image/jpeg"), None);
        assert_eq!(listed("", "text/plain"), None);
    }

    #[test]
    fn the_default_is_the_first_installed_app_the_lists_name() {
        let found = Found {
            lists: vec![
                // the owner's list names an app that is not installed, then one that is
                "[Default Applications]\ntext/plain=gone.desktop;Helix.desktop\n".to_string(),
                "[Default Applications]\ntext/plain=dev.zed.Zed.desktop\n\
                 x-scheme-handler/http=firefox.desktop\n"
                    .to_string(),
            ],
            caches: vec!["[MIME Cache]\ntext/x-csrc=nvim.desktop;Helix.desktop;\n".to_string()],
        };
        let apps = installed();
        assert_eq!(
            found.default_for("text/plain", &apps).as_deref(),
            Some("Helix.desktop")
        );
        assert_eq!(
            found.default_for("x-scheme-handler/http", &apps).as_deref(),
            Some("firefox.desktop")
        );
        // no list names it, so the first app the cache names
        assert_eq!(
            found.default_for("text/x-csrc", &apps).as_deref(),
            Some("nvim.desktop")
        );
        // nothing names it, so the first app by id whose entry says it opens it
        assert_eq!(
            found.default_for("image/png", &apps).as_deref(),
            Some("org.gnome.Loupe.desktop")
        );
        assert_eq!(found.default_for("x-scheme-handler/mailto", &apps), None);
    }

    #[test]
    fn each_kind_lists_the_apps_that_open_its_first_type() {
        let found = Found {
            lists: vec!["[Default Applications]\ntext/plain=dev.zed.Zed.desktop\n".to_string()],
            caches: Vec::new(),
        };
        let opens = found.opens(&installed());
        let text = opens.iter().find(|one| one.kind == Kind::Text).unwrap();
        assert_eq!(text.default.as_deref(), Some("dev.zed.Zed.desktop"));
        assert_eq!(text.default_id(), Some("dev.zed.Zed"));
        assert_eq!(text.apps, ["dev.zed.Zed", "Helix", "nvim"]);
        assert!(text.choosable());
        let mail = opens.iter().find(|one| one.kind == Kind::Mail).unwrap();
        assert_eq!((mail.default.as_deref(), mail.apps.len()), (None, 0));
        assert!(!mail.choosable());
        let web = opens.iter().find(|one| one.kind == Kind::Web).unwrap();
        assert_eq!(web.apps, ["firefox"]);
        assert!(!web.choosable());
    }

    #[test]
    fn a_choice_is_written_into_the_default_group_and_nothing_else_changes() {
        // an empty file gets the group and nothing before it
        assert_eq!(
            with_defaults("", &["text/plain", "text/x-csrc"], "Helix.desktop"),
            "[Default Applications]\ntext/plain=Helix.desktop\ntext/x-csrc=Helix.desktop\n"
        );
        // a line for a type takes the new value where it stands, a second line for it goes, and a
        // type the group lacks goes in after its last line, before the blank line and the next group
        let before = "[Added Associations]\ntext/plain=nvim.desktop;\n\n[Default Applications]\n\
                      text/plain=dev.zed.Zed.desktop\nimage/png=org.gnome.Loupe.desktop\n\
                      text/plain=nvim.desktop\n\n[Removed Associations]\ntext/x-csrc=a.desktop\n";
        assert_eq!(
            with_defaults(before, &["text/plain", "text/x-csrc"], "Helix.desktop"),
            "[Added Associations]\ntext/plain=nvim.desktop;\n\n[Default Applications]\n\
             text/plain=Helix.desktop\nimage/png=org.gnome.Loupe.desktop\n\
             text/x-csrc=Helix.desktop\n\n[Removed Associations]\ntext/x-csrc=a.desktop\n"
        );
        // a file with other groups and none of these gets the group at its end
        assert_eq!(
            with_defaults(
                "[Added Associations]\nimage/png=x.desktop\n",
                &["text/plain"],
                "a.desktop"
            ),
            "[Added Associations]\nimage/png=x.desktop\n\n[Default Applications]\n\
             text/plain=a.desktop\n"
        );
        // writing the same choice again changes nothing
        let once = with_defaults(before, &["text/plain", "text/x-csrc"], "Helix.desktop");
        assert_eq!(
            with_defaults(&once, &["text/plain", "text/x-csrc"], "Helix.desktop"),
            once
        );
        // and what is written is what a list reads back
        let back = with_defaults(
            before,
            &["text/plain", "text/x-csrc"],
            "dev.zed.Zed.desktop",
        );
        assert_eq!(
            listed(&back, "text/x-csrc"),
            Some(vec!["dev.zed.Zed.desktop".to_string()])
        );
        assert_eq!(
            listed(&back, "text/plain"),
            Some(vec!["dev.zed.Zed.desktop".to_string()])
        );
    }

    #[test]
    fn a_cache_names_the_first_app_of_its_line() {
        let text = "[MIME Cache]\ntext/plain=Helix.desktop;nvim.desktop;\ntext/x-c=;\n";
        assert_eq!(cached(text, "text/plain").as_deref(), Some("Helix.desktop"));
        assert_eq!(cached(text, "text/x-c"), None);
        assert_eq!(cached(text, "text/plai"), None);
    }
}
