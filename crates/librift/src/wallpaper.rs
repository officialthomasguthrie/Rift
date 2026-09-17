//! The wallpaper, which the owner picks: one of the photographs Rift ships, any JPEG or PNG
//! picture, or a flat colour. `~/.config/rift/wallpaper` holds a path or a colour until Settings
//! writes it, and `/etc/rift/wallpaper` the system's photograph. Horizon draws a picture itself,
//! scaled to fill each screen, from the part of its config [`crate::appearance`] writes: the path of
//! the picture, or no picture and the colour as the desktop's background.

use std::fmt;
use std::fs::{self, File};
use std::io::Read as _;
use std::path::{Path, PathBuf};

use crate::appearance::{self, Theme};

/// Where the owner's choice lives, under home.
pub const SETTING: &str = ".config/rift/wallpaper";
/// The system's choice, which the image writes.
pub const SYSTEM: &str = "/etc/rift/wallpaper";
/// The photographs Rift ships, each a JPEG with a text file of the same name that says what it
/// shows, who took it, where it came from and on what terms.
pub const SHIPPED: &str = "/run/current-system/sw/share/backgrounds/rift";
/// The flat grays: the desktop's own colours on dark and on light.
pub const GRAYS: [(&str, &str); 2] = [("#242424", "Dark gray"), ("#f2f1f0", "Light gray")];
/// What the desktop has when neither the owner nor the system has chosen.
const UNCHOSEN: &str = "#242424";

/// A wallpaper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wallpaper {
    /// A picture Horizon reads, by its full path.
    Picture(PathBuf),
    /// A flat colour, `#rrggbb` in lower case.
    Color(String),
}

impl Wallpaper {
    /// The wallpaper a setting holds: a colour, or a full path. Nothing for anything else.
    #[must_use]
    pub fn from_setting(text: &str) -> Option<Self> {
        let text = text.trim();
        if let Some(color) = color(text) {
            Some(Self::Color(color))
        } else if text.starts_with('/') && !text.contains('\n') {
            Some(Self::Picture(PathBuf::from(text)))
        } else {
            None
        }
    }

    /// The owner's wallpaper, or the system's when home has no setting, or the dark gray.
    #[must_use]
    pub fn read() -> Self {
        let owner = appearance::home().map(|home| home.join(SETTING));
        owner
            .iter()
            .map(PathBuf::as_path)
            .chain([Path::new(SYSTEM)])
            .find_map(|path| {
                fs::read_to_string(path)
                    .ok()
                    .and_then(|text| Self::from_setting(&text))
            })
            .unwrap_or_else(|| Self::Color(UNCHOSEN.to_string()))
    }

    /// The line the setting holds.
    #[must_use]
    pub fn setting(&self) -> String {
        match self {
            Self::Picture(path) => path.display().to_string(),
            Self::Color(color) => color.clone(),
        }
    }

    /// The name of the photograph among the ones Rift ships, when it is one of them.
    #[must_use]
    pub fn shipped_name(&self) -> Option<&str> {
        let Self::Picture(path) = self else {
            return None;
        };
        if path.parent()? != Path::new(SHIPPED) || path.extension()? != "jpg" {
            return None;
        }
        path.file_stem()?.to_str()
    }
}

impl fmt::Display for Wallpaper {
    /// A shipped photograph by its name, any other picture by its path, a colour as itself.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.shipped_name(), self) {
            (Some(name), _) => f.write_str(name),
            (None, Self::Picture(path)) => write!(f, "{}", path.display()),
            (None, Self::Color(color)) => f.write_str(color),
        }
    }
}

/// A photograph Rift ships.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shipped {
    /// The file's name without `.jpg`, which `rift wallpaper set` takes.
    pub name: String,
    /// What it shows, from its text file.
    pub title: String,
    /// Who took it, from its text file.
    pub credit: String,
    /// The picture.
    pub path: PathBuf,
}

/// The photographs Rift ships, by name.
#[must_use]
pub fn shipped() -> Vec<Shipped> {
    shipped_in(Path::new(SHIPPED))
}

/// The photographs in a folder, by name: each JPEG, with the title and the credit of the text file
/// next to it.
#[must_use]
pub fn shipped_in(folder: &Path) -> Vec<Shipped> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<Shipped> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jpg"))
        .filter_map(|path| {
            let name = path.file_stem()?.to_str()?.to_string();
            let about = fs::read_to_string(path.with_extension("txt")).unwrap_or_default();
            Some(Shipped {
                title: field(&about, "Title").unwrap_or_else(|| name.clone()),
                credit: field(&about, "Credit").unwrap_or_default(),
                name,
                path,
            })
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// What `rift wallpaper set` and Settings take, as a wallpaper: a colour, the name of a photograph
/// Rift ships, or a JPEG or PNG picture by its path, relative to `cwd` or full.
///
/// # Errors
///
/// A sentence that says why the word is no wallpaper.
pub fn choose(word: &str, cwd: &Path) -> Result<Wallpaper, String> {
    choose_in(word, cwd, Path::new(SHIPPED))
}

/// [`choose`] with the shipped photographs in `folder`.
///
/// # Errors
///
/// A sentence that says why the word is no wallpaper.
pub fn choose_in(word: &str, cwd: &Path, folder: &Path) -> Result<Wallpaper, String> {
    let word = word.trim();
    if word.starts_with('#') {
        return color(word).map(Wallpaper::Color).ok_or_else(|| {
            format!("{word} is not a colour. A colour is # and six hex digits, like #242424.")
        });
    }
    if word.is_empty() {
        return Err("The name of a wallpaper, a picture or a colour is needed.".to_string());
    }
    let named = folder.join(format!("{word}.jpg"));
    if !word.contains('/') && named.is_file() {
        return Ok(Wallpaper::Picture(named));
    }
    let path = cwd.join(word);
    if !path.is_file() {
        return Err(format!(
            "There is no wallpaper named {word} and no file at {}. rift wallpaper list shows the names.",
            path.display()
        ));
    }
    let path = path
        .canonicalize()
        .map_err(|e| format!("Could not read {}: {e}.", path.display()))?;
    if path.to_str().is_none_or(|text| text.contains('\n')) {
        return Err(format!(
            "The path {} has characters the compositor cannot read. Rename the file or move it.",
            path.display()
        ));
    }
    if !is_picture(&path) {
        return Err(format!("{} is not a JPEG or PNG picture.", path.display()));
    }
    Ok(Wallpaper::Picture(path))
}

/// Make this the owner's wallpaper and hand it to Horizon, which changes the desktop at once.
///
/// # Errors
///
/// A sentence when the setting or the compositor's part could not be written.
pub fn set(wallpaper: &Wallpaper) -> Result<(), String> {
    let home = appearance::home().ok_or("There is no home to keep the wallpaper in.")?;
    appearance::write_beside(&home.join(SETTING), &format!("{}\n", wallpaper.setting()))?;
    appearance::write_horizon(Theme::read(), wallpaper)
}

/// `#rrggbb` in lower case, when the text is a colour.
fn color(text: &str) -> Option<String> {
    let hex = text.strip_prefix('#')?;
    (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| format!("#{}", hex.to_ascii_lowercase()))
}

/// The value of a `Name: value` line.
fn field(text: &str, name: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.trim() == name && !value.trim().is_empty()).then(|| value.trim().to_string())
    })
}

/// Whether the file starts the way a JPEG or a PNG does, the two kinds Horizon reads.
fn is_picture(path: &Path) -> bool {
    let mut head = [0u8; 8];
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let Ok(read) = file.read(&mut head) else {
        return false;
    };
    let head = &head[..read];
    head.starts_with(&[0xff, 0xd8, 0xff]) || head.starts_with(b"\x89PNG\r\n\x1a\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own under the system's temporary folder, empty.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("librift-wallpaper-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_setting_is_a_colour_or_a_full_path() {
        assert_eq!(
            Wallpaper::from_setting("#242424\n"),
            Some(Wallpaper::Color("#242424".into()))
        );
        assert_eq!(
            Wallpaper::from_setting(" #F2F1F0 "),
            Some(Wallpaper::Color("#f2f1f0".into()))
        );
        assert_eq!(
            Wallpaper::from_setting("/home/rift/Pictures/lake.jpg\n"),
            Some(Wallpaper::Picture("/home/rift/Pictures/lake.jpg".into()))
        );
        assert_eq!(Wallpaper::from_setting("Pictures/lake.jpg"), None);
        assert_eq!(Wallpaper::from_setting("#24242"), None);
        assert_eq!(Wallpaper::from_setting("#24242g"), None);
        assert_eq!(Wallpaper::from_setting(""), None);
    }

    #[test]
    fn a_shipped_photograph_goes_by_its_name() {
        let shipped = Wallpaper::Picture(PathBuf::from(format!("{SHIPPED}/earthset.jpg")));
        assert_eq!(shipped.shipped_name(), Some("earthset"));
        assert_eq!(shipped.to_string(), "earthset");
        assert_eq!(shipped.setting(), format!("{SHIPPED}/earthset.jpg"));
        let own = Wallpaper::Picture("/home/rift/lake.jpg".into());
        assert_eq!(own.shipped_name(), None);
        assert_eq!(own.to_string(), "/home/rift/lake.jpg");
        assert_eq!(Wallpaper::Color("#242424".into()).to_string(), "#242424");
    }

    #[test]
    fn the_list_has_each_jpeg_with_its_title_and_credit() {
        let dir = scratch("list");
        fs::write(dir.join("earthset.jpg"), b"\xff\xd8\xff").unwrap();
        fs::write(
            dir.join("earthset.txt"),
            "Title: Earthset over the Moon\nCredit: NASA\nSource: https://images.nasa.gov/details/art002e009284\n",
        )
        .unwrap();
        fs::write(dir.join("aurora.jpg"), b"\xff\xd8\xff").unwrap();
        fs::write(dir.join("notes.txt"), "Title: not a picture\n").unwrap();
        let found = shipped_in(&dir);
        let names: Vec<_> = found.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["aurora", "earthset"]);
        assert_eq!(found[0].title, "aurora");
        assert_eq!(found[0].credit, "");
        assert_eq!(found[1].title, "Earthset over the Moon");
        assert_eq!(found[1].credit, "NASA");
        assert_eq!(found[1].path, dir.join("earthset.jpg"));
        assert!(shipped_in(&dir.join("missing")).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_choice_is_a_colour_a_name_or_a_picture() {
        let dir = scratch("choose");
        let folder = dir.join("rift");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("earthset.jpg"), b"\xff\xd8\xff\xe0").unwrap();
        fs::write(dir.join("lake.png"), b"\x89PNG\r\n\x1a\nrest").unwrap();
        fs::write(dir.join("notes.txt"), "not a picture").unwrap();

        assert_eq!(
            choose_in("#ABCDEF", &dir, &folder),
            Ok(Wallpaper::Color("#abcdef".into()))
        );
        assert_eq!(
            choose_in("earthset", &dir, &folder),
            Ok(Wallpaper::Picture(folder.join("earthset.jpg")))
        );
        let lake = choose_in("lake.png", &dir, &folder).unwrap();
        assert_eq!(
            lake,
            Wallpaper::Picture(dir.join("lake.png").canonicalize().unwrap())
        );

        let bad = choose_in("#12345", &dir, &folder).unwrap_err();
        assert!(bad.starts_with("#12345 is not a colour."), "{bad}");
        let text = choose_in("notes.txt", &dir, &folder).unwrap_err();
        assert!(
            text.ends_with("notes.txt is not a JPEG or PNG picture."),
            "{text}"
        );
        let missing = choose_in("moon", &dir, &folder).unwrap_err();
        assert!(
            missing.starts_with("There is no wallpaper named moon"),
            "{missing}"
        );
        assert!(choose_in(" ", &dir, &folder).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn fields_are_read_by_name() {
        let text = "Title: Crescent Earth\nCredit:\nSource: https://images.nasa.gov/details/x\n";
        assert_eq!(field(text, "Title").as_deref(), Some("Crescent Earth"));
        assert_eq!(field(text, "Credit"), None);
        assert_eq!(
            field(text, "Source").as_deref(),
            Some("https://images.nasa.gov/details/x")
        );
        assert_eq!(field(text, "License"), None);
    }
}
