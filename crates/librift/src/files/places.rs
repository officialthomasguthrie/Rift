//! Where the owner keeps things: home, and the folders in it that `~/.config/user-dirs.dirs` names
//! for documents, downloads, music, pictures and videos, which GTK's file chooser and every app
//! that saves a download or a photograph read too. The image writes that file with those five, and
//! a folder the file names as home itself is one the owner does not want.

use std::fs;
use std::path::{Path, PathBuf};

use super::home;

/// Where the file that names the folders is, under home.
pub const USER_DIRS: &str = ".config/user-dirs.dirs";

/// A place the sidebar lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// The word `--set place` takes.
    pub word: &'static str,
    /// What the sidebar calls it: Home, or the folder's own name.
    pub name: String,
    /// The folder.
    pub path: PathBuf,
    /// The symbolic icon the sidebar draws it with.
    pub icon: &'static str,
    /// The icon in colour a list of files draws the folder with.
    pub colour: &'static str,
}

/// The folders the file can name, in the order the sidebar lists them: the variable, the word, the
/// folder's usual name, the symbolic icon and the icon in colour.
const FOLDERS: [(&str, &str, &str, &str, &str); 8] = [
    (
        "XDG_DOCUMENTS_DIR",
        "documents",
        "Documents",
        "folder-documents-symbolic",
        "folder-documents",
    ),
    (
        "XDG_DOWNLOAD_DIR",
        "downloads",
        "Downloads",
        "folder-download-symbolic",
        "folder-download",
    ),
    (
        "XDG_MUSIC_DIR",
        "music",
        "Music",
        "folder-music-symbolic",
        "folder-music",
    ),
    (
        "XDG_PICTURES_DIR",
        "pictures",
        "Pictures",
        "folder-pictures-symbolic",
        "folder-pictures",
    ),
    (
        "XDG_VIDEOS_DIR",
        "videos",
        "Videos",
        "folder-videos-symbolic",
        "folder-videos",
    ),
    (
        "XDG_DESKTOP_DIR",
        "desktop",
        "Desktop",
        "user-desktop-symbolic",
        "user-desktop",
    ),
    (
        "XDG_TEMPLATES_DIR",
        "templates",
        "Templates",
        "folder-templates-symbolic",
        "folder-templates",
    ),
    (
        "XDG_PUBLICSHARE_DIR",
        "public",
        "Public",
        "folder-publicshare-symbolic",
        "folder-publicshare",
    ),
];

/// Home, then each folder the file names that is there, in the order of [`FOLDERS`]. Without the
/// file, the five usual folders that are there.
#[must_use]
pub fn places() -> Vec<Place> {
    let Some(home) = home() else {
        return Vec::new();
    };
    let text = fs::read_to_string(home.join(USER_DIRS)).ok();
    let mut places = vec![Place {
        word: "home",
        name: "Home".to_string(),
        path: home.clone(),
        icon: "user-home-symbolic",
        colour: "user-home",
    }];
    places.extend(
        folders(&home, text.as_deref())
            .into_iter()
            .filter(|place| place.path.is_dir()),
    );
    places
}

/// The drive's own exchange partition as a place, when the drive has one and the system has
/// mounted it. It is a folder like the others, not a disk to mount and eject, and the sidebar and
/// the Applications menu both list it after the folders of home.
#[must_use]
pub fn exchange() -> Option<Place> {
    crate::drives::exchange().map(|path| Place {
        word: "exchange",
        name: crate::drives::EXCHANGE_NAME.to_string(),
        path,
        icon: "drive-harddisk-symbolic",
        colour: "drive-harddisk",
    })
}

/// The folders a `user-dirs.dirs` names, whether they are there or not. A folder named as home
/// itself is left out, and so is one named twice. With no file, the five usual folders in home.
#[must_use]
pub fn folders(home: &Path, text: Option<&str>) -> Vec<Place> {
    let named: Vec<(String, PathBuf)> = match text {
        Some(text) => text
            .lines()
            .filter_map(|line| assignment(home, line))
            .collect(),
        None => FOLDERS[..5]
            .iter()
            .map(|(variable, _, name, _, _)| ((*variable).to_string(), home.join(name)))
            .collect(),
    };
    let mut places: Vec<Place> = Vec::new();
    for (variable, word, _, icon, colour) in FOLDERS {
        let Some((_, path)) = named.iter().find(|(name, _)| name == variable) else {
            continue;
        };
        if path == home || places.iter().any(|place| &place.path == path) {
            continue;
        }
        places.push(Place {
            word,
            name: path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
            path: path.clone(),
            icon,
            colour,
        });
    }
    places
}

/// One line of the file, `XDG_MUSIC_DIR="$HOME/Music"`: the variable and the folder.
fn assignment(home: &Path, line: &str) -> Option<(String, PathBuf)> {
    let line = line.trim();
    if line.starts_with('#') {
        return None;
    }
    let (variable, value) = line.split_once('=')?;
    let value = value.trim().strip_prefix('"')?.strip_suffix('"')?;
    let mut unquoted = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            unquoted.extend(chars.next());
        } else {
            unquoted.push(c);
        }
    }
    let path = if unquoted == "$HOME" {
        home.to_path_buf()
    } else if let Some(rest) = unquoted.strip_prefix("$HOME/") {
        home.join(rest)
    } else if unquoted.starts_with('/') {
        PathBuf::from(unquoted)
    } else {
        return None;
    };
    Some((variable.trim().to_string(), path))
}

/// The icon in colour a list draws a folder with: home's, or a named folder's, or none for a
/// folder like any other.
#[must_use]
pub fn colour_of<'a>(places: &'a [Place], folder: &Path) -> Option<&'a str> {
    places
        .iter()
        .find(|place| place.path == folder)
        .map(|place| place.colour)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "# This file is written by xdg-user-dirs-update\n\
        XDG_DESKTOP_DIR=\"$HOME\"\n\
        XDG_DOWNLOAD_DIR=\"$HOME/Downloads\"\n\
        XDG_DOCUMENTS_DIR=\"$HOME/Documents\"\n\
        XDG_MUSIC_DIR=\"/media/music\"\n\
        XDG_PICTURES_DIR=\"$HOME/My \\\"pictures\\\"\"\n\
        XDG_VIDEOS_DIR=\"$HOME/Documents\"\n\
        XDG_TEMPLATES_DIR=\"relative\"\n";

    #[test]
    fn the_file_names_the_folders() {
        let home = Path::new("/home/rift");
        let places = folders(home, Some(FILE));
        let seen: Vec<(&str, String, &Path)> = places
            .iter()
            .map(|place| (place.word, place.name.clone(), place.path.as_path()))
            .collect();
        assert_eq!(
            seen,
            [
                (
                    "documents",
                    "Documents".to_string(),
                    Path::new("/home/rift/Documents")
                ),
                (
                    "downloads",
                    "Downloads".to_string(),
                    Path::new("/home/rift/Downloads")
                ),
                ("music", "music".to_string(), Path::new("/media/music")),
                (
                    "pictures",
                    "My \"pictures\"".to_string(),
                    Path::new("/home/rift/My \"pictures\"")
                ),
            ]
        );
        assert_eq!(
            colour_of(&places, Path::new("/home/rift/Downloads")),
            Some("folder-download")
        );
        assert_eq!(colour_of(&places, Path::new("/home/rift/Other")), None);
    }

    #[test]
    fn with_no_file_the_usual_five() {
        let home = Path::new("/home/rift");
        let words: Vec<&str> = folders(home, None).iter().map(|place| place.word).collect();
        assert_eq!(
            words,
            ["documents", "downloads", "music", "pictures", "videos"]
        );
    }
}
