//! The kind of a file, the way shared-mime-info's database says: by its name first, through the
//! globs every package installs, and by its first bytes when no name fits. The same database
//! says which kinds are a kind of another, `text/x-python` of `text/plain`, and which generic icon
//! draws each one. `update-mime-database` writes the files this reads into `share/mime` of each
//! data directory; the image has them in the system profile.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};

/// How many bytes of a file are read to tell its kind when its name does not.
const HEAD: usize = 512;

/// One pattern of the database.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Glob {
    /// How strongly the pattern says the kind, out of a hundred.
    weight: u32,
    /// The kind it says.
    mime: String,
    /// The pattern, in lower case unless it is matched with case.
    pattern: String,
    /// Whether case counts.
    sensitive: bool,
}

/// The database of kinds.
#[derive(Debug, Clone, Default)]
pub struct Database {
    /// Whole names, `makefile`, by the name in lower case.
    literal: HashMap<String, Vec<Glob>>,
    /// Endings, `*.txt`, by what follows the last dot, in lower case.
    endings: HashMap<String, Vec<Glob>>,
    /// Every other pattern.
    other: Vec<Glob>,
    /// The kinds each kind is a kind of.
    parents: HashMap<String, Vec<String>>,
    /// The kind an old name stands for.
    aliases: HashMap<String, String>,
    /// The generic icon of each kind.
    icons: HashMap<String, String>,
}

impl Database {
    /// The database of every data directory, the owner's first.
    #[must_use]
    pub fn load() -> Self {
        let mut database = Self::default();
        for dir in data_dirs() {
            let read =
                |file: &str| fs::read_to_string(dir.join("mime").join(file)).unwrap_or_default();
            let globs = read("globs2");
            if globs.is_empty() {
                continue;
            }
            database.add(
                &globs,
                &read("subclasses"),
                &read("aliases"),
                &read("generic-icons"),
            );
        }
        database
    }

    /// A database from the text of the four files, for a test or a single directory.
    #[must_use]
    pub fn parse(globs2: &str, subclasses: &str, aliases: &str, icons: &str) -> Self {
        let mut database = Self::default();
        database.add(globs2, subclasses, aliases, icons);
        database
    }

    /// Add one directory's files. What an earlier directory said about a kind stays.
    fn add(&mut self, globs2: &str, subclasses: &str, aliases: &str, icons: &str) {
        for line in globs2.lines() {
            if line.starts_with('#') {
                continue;
            }
            let mut fields = line.split(':');
            let (Some(weight), Some(mime), Some(pattern)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let Ok(weight) = weight.parse::<u32>() else {
                continue;
            };
            if pattern.is_empty() || pattern == "__NOGLOBS__" {
                continue;
            }
            let sensitive = fields.any(|flag| flag.split(',').any(|flag| flag == "cs"));
            let glob = Glob {
                weight,
                mime: mime.to_string(),
                pattern: if sensitive {
                    pattern.to_string()
                } else {
                    pattern.to_lowercase()
                },
                sensitive,
            };
            if !pattern.contains(['*', '?', '[']) {
                self.literal
                    .entry(glob.pattern.clone())
                    .or_default()
                    .push(glob);
            } else if let Some(ending) = simple_ending(&glob.pattern) {
                self.endings
                    .entry(ending.to_lowercase())
                    .or_default()
                    .push(glob);
            } else {
                self.other.push(glob);
            }
        }
        for line in subclasses.lines() {
            if let Some((child, parent)) = line.split_once(' ') {
                let parents = self.parents.entry(child.to_string()).or_default();
                if !parents.iter().any(|known| known == parent) {
                    parents.push(parent.trim().to_string());
                }
            }
        }
        for line in aliases.lines() {
            if let Some((alias, kind)) = line.split_once(' ') {
                self.aliases
                    .entry(alias.to_string())
                    .or_insert_with(|| kind.trim().to_string());
            }
        }
        for line in icons.lines() {
            if let Some((kind, icon)) = line.split_once(':') {
                self.icons
                    .entry(kind.to_string())
                    .or_insert_with(|| icon.trim().to_string());
            }
        }
    }

    /// The kind a file's name says, when a pattern fits it: a whole name first, then the pattern
    /// with the greatest weight, and of those the longest, and of those one where case counts.
    #[must_use]
    pub fn by_name(&self, name: &str) -> Option<&str> {
        let lower = name.to_lowercase();
        let fits = |glob: &&Glob| {
            let against = if glob.sensitive { name } else { &lower };
            glob_match(&glob.pattern, against)
        };
        let whole: Vec<&Glob> = [name, lower.as_str()]
            .iter()
            .filter_map(|key| self.literal.get(*key))
            .flatten()
            .filter(fits)
            .collect();
        if let Some(found) = best(whole) {
            return Some(found);
        }
        let ending = lower.rsplit_once('.').map(|(_, ending)| ending);
        let candidates: Vec<&Glob> = ending
            .and_then(|ending| self.endings.get(ending))
            .into_iter()
            .flatten()
            .chain(self.other.iter())
            .filter(fits)
            .collect();
        best(candidates)
    }

    /// The kind of a file: a folder, a device, what its name says, or what its first bytes look
    /// like. `meta` is what the file itself says, after a link is followed; `link` says the path is
    /// a link, which with no `meta` is a link to nothing.
    #[must_use]
    pub fn guess(&self, path: &Path, meta: Option<&fs::Metadata>, link: bool) -> String {
        let Some(meta) = meta else {
            return if link {
                "inode/symlink"
            } else {
                "application/octet-stream"
            }
            .to_string();
        };
        let kind = meta.file_type();
        let special = if kind.is_dir() {
            Some("inode/directory")
        } else if kind.is_char_device() {
            Some("inode/chardevice")
        } else if kind.is_block_device() {
            Some("inode/blockdevice")
        } else if kind.is_fifo() {
            Some("inode/fifo")
        } else if kind.is_socket() {
            Some("inode/socket")
        } else {
            None
        };
        if let Some(special) = special {
            return special.to_string();
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Some(found) = self.by_name(&name) {
            return found.to_string();
        }
        if meta.len() == 0 {
            return "application/x-zerosize".to_string();
        }
        let mut head = Vec::with_capacity(HEAD);
        match File::open(path) {
            Ok(file) => {
                let _ = file.take(HEAD as u64).read_to_end(&mut head);
            }
            Err(_) => return "application/octet-stream".to_string(),
        }
        sniff(&head).to_string()
    }

    /// The kind an old name stands for, or the name itself.
    #[must_use]
    pub fn canonical<'a>(&'a self, mime: &'a str) -> &'a str {
        self.aliases.get(mime).map_or(mime, String::as_str)
    }

    /// The kinds a kind is a kind of, nearest first: what the database says, then plain text for
    /// any text and for an empty file, which an editor opens.
    #[must_use]
    pub fn parents(&self, mime: &str) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();
        let mut waiting = vec![self.canonical(mime).to_string()];
        while !waiting.is_empty() {
            let mut next = Vec::new();
            for kind in waiting {
                for parent in self.parents.get(&kind).into_iter().flatten() {
                    let parent = self.canonical(parent).to_string();
                    if parent != mime && !found.contains(&parent) {
                        found.push(parent.clone());
                        next.push(parent);
                    }
                }
            }
            waiting = next;
        }
        let plain = "text/plain".to_string();
        if (mime.starts_with("text/") || mime == "application/x-zerosize")
            && mime != plain
            && !found.contains(&plain)
        {
            found.push(plain);
        }
        found
    }

    /// The names of the icons that can draw a kind, the one that fits best first: its own, the
    /// generic one the database gives it or a kind it is a kind of, then the generic one of its
    /// media type.
    #[must_use]
    pub fn icons(&self, mime: &str) -> Vec<String> {
        if mime == "inode/directory" {
            return vec!["folder".to_string()];
        }
        let mime = self.canonical(mime);
        let mut names = vec![mime.replace('/', "-")];
        let generic = std::iter::once(mime.to_string())
            .chain(self.parents(mime))
            .find_map(|kind| self.icons.get(&kind).cloned());
        names.extend(generic);
        let media = mime.split('/').next().unwrap_or("application");
        names.push(format!("{media}-x-generic"));
        names.push(
            if media == "application" || media == "inode" {
                "application-x-generic"
            } else {
                "text-x-generic"
            }
            .to_string(),
        );
        names.dedup();
        names
    }
}

/// The kind the strongest of the patterns that fit says: the greatest weight, and of those the
/// longest pattern, and of those one where case counts.
fn best(globs: Vec<&Glob>) -> Option<&str> {
    globs
        .into_iter()
        .max_by_key(|glob| (glob.weight, glob.pattern.len(), glob.sensitive))
        .map(|glob| glob.mime.as_str())
}

/// What a pattern is when it is only an ending, `*.tar.gz`: the text after its last dot.
fn simple_ending(pattern: &str) -> Option<&str> {
    let rest = pattern.strip_prefix("*.")?;
    if rest.contains(['*', '?', '[']) {
        return None;
    }
    Some(rest.rsplit_once('.').map_or(rest, |(_, last)| last))
}

/// The kind the first bytes of a file say when its name says nothing: a few formats by the bytes
/// they always start with, a script by its first line, text when the bytes are text, and bytes
/// otherwise.
#[must_use]
pub fn sniff(head: &[u8]) -> &'static str {
    const SIGNATURES: [(&[u8], &str); 7] = [
        (b"\x7fELF", "application/x-executable"),
        (b"%PDF-", "application/pdf"),
        (b"\x89PNG\r\n\x1a\n", "image/png"),
        (b"\xff\xd8\xff", "image/jpeg"),
        (b"GIF8", "image/gif"),
        (b"PK\x03\x04", "application/zip"),
        (b"\x1f\x8b", "application/gzip"),
    ];
    if let Some((_, kind)) = SIGNATURES
        .iter()
        .find(|(signature, _)| head.starts_with(signature))
    {
        return kind;
    }
    if head.contains(&0) {
        return "application/octet-stream";
    }
    // a character cut off by the end of what was read is still text
    let text = match std::str::from_utf8(head) {
        Ok(text) => text,
        Err(e) if e.error_len().is_none() => {
            std::str::from_utf8(&head[..e.valid_up_to()]).unwrap_or("")
        }
        Err(_) => return "application/octet-stream",
    };
    if let Some(first) = text.strip_prefix("#!").and_then(|rest| rest.lines().next()) {
        if first.contains("python") {
            return "text/x-python";
        }
        if first.contains("sh") {
            return "application/x-shellscript";
        }
    }
    "text/plain"
}

/// Whether a name fits a pattern with `*`, `?` and `[...]` in it, the way fnmatch does it. A `[`
/// with no `]` after it is a plain character.
#[must_use]
pub fn glob_match(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    let (mut at, mut on) = (0, 0);
    // where the last star was, and the character of the name it has taken up to
    let mut star: Option<(usize, usize)> = None;
    while on < name.len() {
        if at < pattern.len() {
            match pattern[at] {
                '*' => {
                    star = Some((at, on));
                    at += 1;
                    continue;
                }
                '?' => {
                    at += 1;
                    on += 1;
                    continue;
                }
                '[' => match class(&pattern, at, name[on]) {
                    Some((true, next)) => {
                        at = next;
                        on += 1;
                        continue;
                    }
                    None if name[on] == '[' => {
                        at += 1;
                        on += 1;
                        continue;
                    }
                    Some((false, _)) | None => {}
                },
                wanted if wanted == name[on] => {
                    at += 1;
                    on += 1;
                    continue;
                }
                _ => {}
            }
        }
        match star {
            Some((star_at, taken)) => {
                at = star_at + 1;
                on = taken + 1;
                star = Some((star_at, taken + 1));
            }
            None => return false,
        }
    }
    pattern[at..].iter().all(|&c| c == '*')
}

/// Whether a character is in the class that starts at `at`, and where the pattern goes on after
/// it. `None` when the class never closes.
fn class(pattern: &[char], at: usize, wanted: char) -> Option<(bool, usize)> {
    let mut on = at + 1;
    let negated = matches!(pattern.get(on), Some('!' | '^'));
    if negated {
        on += 1;
    }
    let mut matched = false;
    let mut first = true;
    while on < pattern.len() && (pattern[on] != ']' || first) {
        first = false;
        if on + 2 < pattern.len() && pattern[on + 1] == '-' && pattern[on + 2] != ']' {
            matched |= pattern[on] <= wanted && wanted <= pattern[on + 2];
            on += 3;
        } else {
            matched |= pattern[on] == wanted;
            on += 1;
        }
    }
    (on < pattern.len()).then_some((matched != negated, on + 1))
}

/// The data directories, the owner's first, then the session's, then the system profile, where the
/// image's database is even when the environment is thin.
fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut push = |dir: PathBuf| {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    };
    match std::env::var_os("XDG_DATA_HOME").filter(|dir| !dir.is_empty()) {
        Some(dir) => push(PathBuf::from(dir)),
        None => {
            if let Some(home) = std::env::var_os("HOME").filter(|home| !home.is_empty()) {
                push(PathBuf::from(home).join(".local/share"));
            }
        }
    }
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    for dir in system.split(':').filter(|dir| !dir.is_empty()) {
        push(PathBuf::from(dir));
    }
    push(PathBuf::from("/run/current-system/sw/share"));
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    const GLOBS: &str = "# a comment\n\
        80:text/html:*.html\n\
        50:text/plain:*.txt\n\
        50:application/x-compressed-tar:*.tar.gz\n\
        20:application/gzip:*.gz\n\
        50:text/x-makefile:makefile\n\
        10:text/x-makefile:makefile.*\n\
        50:application/x-sharedlib:*.so.[0-9]*\n\
        50:text/x-csrc:*.c\n\
        50:text/x-c++src:*.C:cs\n\
        50:application/x-trash:*~\n\
        50:text/x-readme:readme*\n";

    fn database() -> Database {
        Database::parse(
            GLOBS,
            "text/x-csrc text/plain\ntext/x-makefile text/plain\napplication/x-compressed-tar application/x-tar\n",
            "text/x-c text/x-csrc\n",
            "application/x-compressed-tar:package-x-generic\ntext/x-csrc:text-x-script\n",
        )
    }

    #[test]
    fn a_name_says_its_kind() {
        let types = database();
        assert_eq!(types.by_name("index.HTML"), Some("text/html"));
        assert_eq!(types.by_name("notes.txt"), Some("text/plain"));
        // the longer ending wins over the shorter one
        assert_eq!(
            types.by_name("a.tar.gz"),
            Some("application/x-compressed-tar")
        );
        assert_eq!(types.by_name("a.gz"), Some("application/gzip"));
        // a whole name before any pattern, whatever its case
        assert_eq!(types.by_name("Makefile"), Some("text/x-makefile"));
        assert_eq!(types.by_name("makefile.am"), Some("text/x-makefile"));
        assert_eq!(types.by_name("libc.so.6"), Some("application/x-sharedlib"));
        // case counts where the database says it does
        assert_eq!(types.by_name("main.c"), Some("text/x-csrc"));
        assert_eq!(types.by_name("main.C"), Some("text/x-c++src"));
        assert_eq!(types.by_name("notes~"), Some("application/x-trash"));
        assert_eq!(types.by_name("README.md"), Some("text/x-readme"));
        assert_eq!(types.by_name("photo"), None);
    }

    #[test]
    fn a_kind_has_parents_and_icons() {
        let types = database();
        assert_eq!(types.parents("text/x-csrc"), ["text/plain"]);
        assert_eq!(types.parents("text/x-c"), ["text/plain"]);
        assert_eq!(types.parents("text/markdown"), ["text/plain"]);
        assert_eq!(types.parents("application/x-zerosize"), ["text/plain"]);
        assert_eq!(
            types.parents("application/x-compressed-tar"),
            ["application/x-tar"]
        );
        assert_eq!(
            types.icons("application/x-compressed-tar"),
            [
                "application-x-compressed-tar",
                "package-x-generic",
                "application-x-generic"
            ]
        );
        assert_eq!(
            types.icons("text/x-c"),
            ["text-x-csrc", "text-x-script", "text-x-generic"]
        );
        assert_eq!(
            types.icons("image/png"),
            ["image-png", "image-x-generic", "text-x-generic"]
        );
        assert_eq!(types.icons("inode/directory"), ["folder"]);
    }

    #[test]
    fn the_first_bytes_say_the_kind_when_the_name_does_not() {
        assert_eq!(sniff(b"\x7fELF\x02\x01"), "application/x-executable");
        assert_eq!(sniff(b"%PDF-1.7"), "application/pdf");
        assert_eq!(
            sniff(b"#!/usr/bin/env python3\nprint(1)\n"),
            "text/x-python"
        );
        assert_eq!(sniff(b"#!/bin/sh\necho\n"), "application/x-shellscript");
        assert_eq!(sniff("plain words, and caf\u{e9}".as_bytes()), "text/plain");
        // a character cut in two by the end of what was read
        assert_eq!(sniff(&"caf\u{e9}".as_bytes()[..4]), "text/plain");
        assert_eq!(sniff(b"\x00\x01\x02"), "application/octet-stream");
        assert_eq!(sniff(b"\xff\xfe\xfd"), "application/octet-stream");
    }

    #[test]
    fn a_pattern_matches_the_way_fnmatch_does() {
        assert!(glob_match("*.txt", "a.txt"));
        assert!(!glob_match("*.txt", "a.txt.bak"));
        assert!(glob_match("*.[1-9]", "ls.1"));
        assert!(!glob_match("*.[1-9]", "ls.0"));
        assert!(glob_match("[0-9][0-9][0-9].vdr", "001.vdr"));
        assert!(glob_match("*.anim[1-9j]", "a.animj"));
        assert!(glob_match("[!a]*", "b"));
        assert!(!glob_match("[!a]*", "a"));
        assert!(glob_match("a?c", "abc"));
        assert!(glob_match("*", ""));
        assert!(glob_match("a[b", "a[b"));
        assert!(glob_match("*,v", "file,v"));
    }

    #[test]
    fn a_file_is_guessed_from_its_name_then_its_bytes() {
        let folder = std::env::temp_dir().join(format!("librift-mime-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        let types = database();
        let guess = |name: &str, bytes: &[u8]| {
            let path = folder.join(name);
            fs::write(&path, bytes).unwrap();
            types.guess(&path, fs::metadata(&path).ok().as_ref(), false)
        };
        assert_eq!(guess("notes.txt", b"hello"), "text/plain");
        assert_eq!(guess("notes", b"hello"), "text/plain");
        assert_eq!(guess("empty", b""), "application/x-zerosize");
        assert_eq!(guess("run", b"#!/bin/sh\n"), "application/x-shellscript");
        assert_eq!(
            types.guess(&folder, fs::metadata(&folder).ok().as_ref(), false),
            "inode/directory"
        );
        assert_eq!(
            types.guess(&folder.join("gone"), None, true),
            "inode/symlink"
        );
        let _ = fs::remove_dir_all(&folder);
    }
}
