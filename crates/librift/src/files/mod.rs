//! Files and folders the way a file manager shows them: what is in a folder, with the kind, size
//! and age of each entry, the order a list puts them in, the words a list says them in, the names
//! a new file may have and the name a copy gets, and where the owner keeps things. The kinds of
//! file come from shared-mime-info's database, the trash is the freedesktop one every GTK app
//! uses too, and the places are the folders `~/.config/user-dirs.dirs` names.

pub mod mime;
pub mod places;
pub mod trash;

use std::cmp::Ordering;
use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::fs;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

use crate::appearance::{home, write_beside};

/// Where the file manager keeps whether hidden files show and the order of a list, under home.
pub const OPTIONS: &str = ".config/rift/files";

/// A folder with more entries than this is listed without counting what is in each of its
/// folders, which would be a folder read for every one of them.
const COUNTED: usize = 1000;

/// The longest name a file system here takes, in bytes.
const LONGEST_NAME: usize = 255;

/// What an entry of a folder is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A folder, or a link to one.
    Folder,
    /// A regular file, or a link to one.
    File,
    /// A link that points at nothing.
    Broken,
    /// A device, a socket or a pipe.
    Other,
}

impl Kind {
    /// The word `--state` prints for it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::File => "file",
            Self::Broken => "link",
            Self::Other => "other",
        }
    }
}

/// One entry of a folder, with what a list shows about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Its name in the folder, exactly as the file system has it.
    pub name: OsString,
    /// The same name as it is drawn. A name that is not UTF-8 gets a stand-in for the bytes that
    /// are not.
    pub label: String,
    /// What it is. A link counts as what it points at.
    pub kind: Kind,
    /// Whether it is a symbolic link.
    pub link: bool,
    /// The size of a file in bytes. Nothing for a folder.
    pub size: u64,
    /// When it was last changed, in seconds since 1970.
    pub modified: Option<i64>,
    /// Its type, `text/plain` or `inode/directory`.
    pub mime: String,
    /// Whether a list leaves it out while hidden files do not show: a name with a dot in front, a
    /// backup copy with a tilde at the end, or one the folder's `.hidden` file names.
    pub hidden: bool,
    /// How many entries a folder holds, when they were counted.
    pub items: Option<u64>,
}

/// The column a list is in the order of.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Sort {
    /// By name, the way a person reads numbers in a name.
    #[default]
    Name,
    /// By size, folders by how much they hold.
    Size,
    /// By the time each was last changed.
    Modified,
}

impl Sort {
    /// Every order, as the menu lists them.
    pub const ALL: [Self; 3] = [Self::Name, Self::Size, Self::Modified];

    /// The word `--set sort` takes and the options file keeps.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Size => "size",
            Self::Modified => "modified",
        }
    }

    /// The heading of its column.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Size => "Size",
            Self::Modified => "Modified",
        }
    }

    /// The order a word names.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|sort| word.trim().eq_ignore_ascii_case(sort.word()))
    }

    /// Whether a list in this order runs from the least to the most. Names run from A, and sizes
    /// and times from the biggest and the newest, the way a person looks for them, unless the
    /// order is reversed.
    #[must_use]
    pub const fn ascending(self, reversed: bool) -> bool {
        matches!(self, Self::Name) != reversed
    }
}

/// How the owner likes a folder shown. Every window starts this way.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    /// Whether hidden files show.
    pub hidden: bool,
    /// The column the list is in the order of.
    pub sort: Sort,
    /// Whether it runs from the other end of that order.
    pub reversed: bool,
}

impl Options {
    /// What the options file says, or how a list starts when there is none.
    #[must_use]
    pub fn read() -> Self {
        home()
            .and_then(|home| fs::read_to_string(home.join(OPTIONS)).ok())
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// The options a file holds: `hidden on` and `sort size reversed`, a line each. A line that is
    /// not one of them is left out.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut options = Self::default();
        for line in text.lines() {
            let mut words = line.split_whitespace();
            match (words.next(), words.next(), words.next()) {
                (Some("hidden"), Some(value), None) => options.hidden = value == "on",
                (Some("sort"), Some(word), rest) => {
                    if let Some(sort) = Sort::from_word(word) {
                        options.sort = sort;
                        options.reversed = rest == Some("reversed");
                    }
                }
                _ => {}
            }
        }
        options
    }

    /// The text of the options file.
    #[must_use]
    pub fn text(self) -> String {
        format!(
            "hidden {}\nsort {}{}\n",
            if self.hidden { "on" } else { "off" },
            self.sort.word(),
            if self.reversed { " reversed" } else { "" }
        )
    }

    /// Keep them for the next window.
    ///
    /// # Errors
    ///
    /// A sentence when there is no home or the file could not be written.
    pub fn save(self) -> Result<(), String> {
        let home = home().ok_or("There is no home to keep the file manager's options in.")?;
        write_beside(&home.join(OPTIONS), &self.text()).map(|_| ())
    }
}

/// Read a folder: every entry in it, with its kind, size, age and type. Hidden entries are read
/// too and marked, so showing them is up to the list. The order is the folder's own; [`sort`] puts
/// them in a list's.
///
/// # Errors
///
/// A sentence when the folder cannot be read.
pub fn read(folder: &Path, types: &mime::Database) -> Result<Vec<Entry>, String> {
    let entries: Vec<fs::DirEntry> = fs::read_dir(folder)
        .map_err(|e| cannot_open(folder, &e))?
        .filter_map(Result::ok)
        .collect();
    let listed = fs::read_to_string(folder.join(".hidden")).unwrap_or_default();
    let count = entries.len() <= COUNTED;
    Ok(entries
        .iter()
        .map(|found| entry(&found.path(), found.file_name(), &listed, count, types))
        .collect())
}

/// One entry, from its path. A link is followed for its kind, its size and its time, and a link
/// that points at nothing is still listed.
fn entry(path: &Path, name: OsString, listed: &str, count: bool, types: &mime::Database) -> Entry {
    let own = fs::symlink_metadata(path).ok();
    let link = own
        .as_ref()
        .is_some_and(|meta| meta.file_type().is_symlink());
    let meta = if link { fs::metadata(path).ok() } else { own };
    let kind = match &meta {
        None if link => Kind::Broken,
        Some(meta) if meta.is_dir() => Kind::Folder,
        Some(meta) if meta.is_file() => Kind::File,
        None | Some(_) => Kind::Other,
    };
    let label = name.to_string_lossy().into_owned();
    let hidden = is_hidden(&label, listed);
    let modified = meta
        .as_ref()
        .and_then(|meta| meta.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|since| i64::try_from(since.as_secs()).ok());
    let items = (kind == Kind::Folder && count)
        .then(|| fs::read_dir(path).ok().map(|inside| inside.count() as u64))
        .flatten();
    Entry {
        mime: types.guess(path, meta.as_ref(), link),
        size: meta
            .as_ref()
            .filter(|meta| meta.is_file())
            .map_or(0, fs::Metadata::len),
        name,
        label,
        kind,
        link,
        modified,
        hidden,
        items,
    }
}

/// Whether a name is one a list hides: a dot in front, a backup's tilde at the end, or a line of
/// the folder's `.hidden` file.
#[must_use]
pub fn is_hidden(name: &str, listed: &str) -> bool {
    name.starts_with('.')
        || (name.ends_with('~') && name.len() > 1)
        || listed.lines().any(|line| line.trim_end() == name)
}

/// The sentence for a folder that could not be read.
fn cannot_open(folder: &Path, error: &std::io::Error) -> String {
    let name = shown(folder);
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => {
            format!("{name} cannot be opened: you are not allowed to see what is in it.")
        }
        std::io::ErrorKind::NotFound => format!("{name} is not there any more."),
        _ => format!("{name} cannot be opened: {error}."),
    }
}

/// A folder's name the way a person says it: its own name, Home for home, and a slash for the
/// root of the file system.
#[must_use]
pub fn shown(folder: &Path) -> String {
    if home().is_some_and(|home| home == folder) {
        return "Home".to_string();
    }
    folder.file_name().map_or_else(
        || folder.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Put entries in the order of a list: folders first, then the rest, each in the order the
/// options name.
pub fn sort(entries: &mut [Entry], options: Options) {
    entries.sort_by(|one, other| {
        let folders = (other.kind == Kind::Folder).cmp(&(one.kind == Kind::Folder));
        folders.then_with(|| {
            let by = match options.sort {
                Sort::Name => natural(&one.label, &other.label),
                Sort::Size => one
                    .size
                    .cmp(&other.size)
                    .then(one.items.cmp(&other.items))
                    .reverse(),
                Sort::Modified => one.modified.cmp(&other.modified).reverse(),
            };
            let by = by.then_with(|| natural(&one.label, &other.label));
            if options.reversed { by.reverse() } else { by }
        })
    });
}

/// Two names in the order a person expects: letters without regard to case, and a run of digits
/// by the number it is, so `file 9` comes before `file 10`. The dot in front of a hidden name does
/// not count, so `.config` stands beside `config`.
#[must_use]
pub fn natural(one: &str, other: &str) -> Ordering {
    let mut left = one.strip_prefix('.').unwrap_or(one).chars().peekable();
    let mut right = other.strip_prefix('.').unwrap_or(other).chars().peekable();
    loop {
        match (left.peek().copied(), right.peek().copied()) {
            (None, None) => return one.cmp(other),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(a), Some(b)) if a.is_ascii_digit() && b.is_ascii_digit() => {
                let first = digits(&mut left);
                let second = digits(&mut right);
                let by = first
                    .trim_start_matches('0')
                    .len()
                    .cmp(&second.trim_start_matches('0').len())
                    .then_with(|| {
                        first
                            .trim_start_matches('0')
                            .cmp(second.trim_start_matches('0'))
                    });
                if by != Ordering::Equal {
                    return by;
                }
            }
            (Some(a), Some(b)) => {
                let by = a.to_lowercase().cmp(b.to_lowercase());
                if by != Ordering::Equal {
                    return by;
                }
                left.next();
                right.next();
            }
        }
    }
}

fn digits(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut run = String::new();
    while let Some(digit) = chars.next_if(char::is_ascii_digit) {
        run.push(digit);
    }
    run
}

/// A size the way GNOME writes one, in powers of ten: `1 byte`, `12 bytes`, `12.3 kB`, `4.0 GB`.
#[must_use]
pub fn size_words(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["kB", "MB", "GB", "TB", "PB"];
    match bytes {
        1 => "1 byte".to_string(),
        0..1000 => format!("{bytes} bytes"),
        _ => {
            let mut unit = 0;
            let mut scale: u64 = 1000;
            while unit + 1 < UNITS.len() && bytes >= scale * 1000 - scale / 20 {
                unit += 1;
                scale *= 1000;
            }
            let tenths = (u128::from(bytes) * 10 + u128::from(scale) / 2) / u128::from(scale);
            format!("{}.{} {}", tenths / 10, tenths % 10, UNITS[unit])
        }
    }
}

/// How much a folder holds, the way its row says it.
#[must_use]
pub fn items_words(items: u64) -> String {
    match items {
        0 => "Empty".to_string(),
        1 => "1 item".to_string(),
        _ => format!("{items} items"),
    }
}

/// The abbreviations of the months, the way the bar's clock writes them.
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// When something happened, the way a list says it: the time for today, Yesterday, the day and the
/// month for this year, and the year too for an older one. `offset` is the local zone's distance
/// from UTC in seconds, and both times are seconds since 1970.
#[must_use]
pub fn when_words(seconds: i64, now: i64, offset: i32) -> String {
    const DAY: i64 = 86_400;
    let local = seconds + i64::from(offset);
    let today = (now + i64::from(offset)).div_euclid(DAY);
    let day = local.div_euclid(DAY);
    let (year, month, date) = crate::vault::civil(day);
    let (this_year, _, _) = crate::vault::civil(today);
    let month = usize::try_from(month - 1).map_or("", |at| MONTHS[at % 12]);
    if day == today {
        let minutes = local.rem_euclid(DAY) / 60;
        format!("{:02}:{:02}", minutes / 60, minutes % 60)
    } else if day + 1 == today {
        "Yesterday".to_string()
    } else if year == this_year {
        format!("{date} {month}")
    } else {
        format!("{date} {month} {year}")
    }
}

/// The local zone's distance from UTC now, in seconds, as `date +%z` says it. UTC when it cannot
/// say, which is what a drive with no zone chosen is in.
#[must_use]
pub fn utc_offset() -> i32 {
    Command::new("date")
        .arg("+%z")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| offset_of(String::from_utf8_lossy(&output.stdout).trim()))
        .unwrap_or(0)
}

/// The seconds of an offset `date` writes, `+1200` or `-0330`.
#[must_use]
pub fn offset_of(written: &str) -> Option<i32> {
    let (sign, digits) = match written.as_bytes().first()? {
        b'+' => (1, &written[1..]),
        b'-' => (-1, &written[1..]),
        _ => return None,
    };
    if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let hours: i32 = digits[..2].parse().ok()?;
    let minutes: i32 = digits[2..].parse().ok()?;
    Some(sign * (hours * 3600 + minutes * 60))
}

/// A local time the way the trash writes one, `2026-09-22T10:11:12`, for seconds since 1970 and the
/// zone's offset.
#[must_use]
pub fn local_stamp(seconds: i64, offset: i32) -> String {
    let local = seconds + i64::from(offset);
    let (year, month, date) = crate::vault::civil(local.div_euclid(86_400));
    let within = local.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{date:02}T{:02}:{:02}:{:02}",
        within / 3600,
        within % 3600 / 60,
        within % 60
    )
}

/// The seconds since 1970 of a local time the trash wrote, taking the zone's offset off. Seconds
/// after the minute and anything after them may be missing.
#[must_use]
pub fn stamp_seconds(written: &str, offset: i32) -> Option<i64> {
    let (date, time) = written.trim().split_once('T')?;
    let mut day = date.split('-').map(|part| part.parse::<i64>().ok());
    let (year, month, date) = (day.next()??, day.next()??, day.next()??);
    let time = time.get(..8).unwrap_or(time);
    let mut clock = time.split(':').map(|part| part.parse::<i64>().ok());
    let hour = clock.next()??;
    let minute = clock.next()??;
    let second = clock.next().flatten().unwrap_or(0);
    if !(1..=12).contains(&month) || !(1..=31).contains(&date) || hour > 23 || minute > 59 {
        return None;
    }
    let days = crate::vault::days_from_civil(year, month, date);
    Some(days * 86_400 + hour * 3600 + minute * 60 + second - i64::from(offset))
}

/// What is wrong with a name a person typed for a new file or folder, or for a rename, as the
/// sentence under the field.
///
/// # Errors
///
/// The sentence, when the name cannot be used.
pub fn check_name(name: &str) -> Result<(), &'static str> {
    if name.trim().is_empty() {
        return Err("Type a name.");
    }
    if name.contains('/') {
        return Err("A name cannot have a slash in it.");
    }
    if name == "." || name == ".." {
        return Err("A name cannot be a dot or two dots.");
    }
    if name.len() > LONGEST_NAME {
        return Err("The name is too long.");
    }
    Ok(())
}

/// Delete a file, a link or a whole folder for good. A link is deleted, never what it points at. A
/// folder with read-only folders inside, which some build tools leave behind, is made writable on
/// the way, as far as the owner may.
///
/// # Errors
///
/// A sentence when it is not there or could not be deleted.
pub fn remove(path: &Path) -> Result<(), String> {
    let shown = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let meta = fs::symlink_metadata(path).map_err(|_| format!("{shown} is not there any more."))?;
    let first = if meta.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    match first {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && meta.is_dir() => {
            writable(path);
            fs::remove_dir_all(path).map_err(|e| format!("{shown} could not be deleted: {e}."))
        }
        Err(e) => Err(format!("{shown} could not be deleted: {e}.")),
    }
}

/// Give the owner write access to a folder and every folder in it, without following links.
fn writable(folder: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(meta) = fs::symlink_metadata(folder) else {
        return;
    };
    if !meta.is_dir() {
        return;
    }
    let mode = meta.permissions().mode();
    let _ = fs::set_permissions(folder, fs::Permissions::from_mode(mode | 0o700));
    if let Ok(entries) = fs::read_dir(folder) {
        for entry in entries.filter_map(Result::ok) {
            writable(&entry.path());
        }
    }
}

/// Endings kept whole when a copy is named, so `a.tar.gz` becomes `a (copy).tar.gz`.
const DOUBLE_ENDINGS: [&str; 5] = [".tar.gz", ".tar.xz", ".tar.bz2", ".tar.zst", ".tar.lz"];

/// A name in `folder` for `name` that nothing there has yet: the name itself when it is free,
/// otherwise the name with ` (copy)`, ` (copy 2)` and so on before its ending for a copy, or with
/// ` (2)`, ` (3)` for anything else. A name that already ends in one of those counts on from it.
#[must_use]
pub fn free_name(folder: &Path, name: &OsStr, copy: bool) -> OsString {
    if fs::symlink_metadata(folder.join(name)).is_err() {
        return name.to_owned();
    }
    let text = name.to_string_lossy();
    let (stem, ending) = split_ending(&text);
    let (stem, mut number) = counted(stem, copy);
    loop {
        let tried = match (copy, number) {
            (true, 1) => format!("{stem} (copy){ending}"),
            (true, _) => format!("{stem} (copy {number}){ending}"),
            (false, _) => format!("{stem} ({}){ending}", number.max(2)),
        };
        if fs::symlink_metadata(folder.join(&tried)).is_err() {
            return OsString::from(tried);
        }
        number = number.max(if copy { 1 } else { 2 }) + 1;
    }
}

/// A name's stem and its ending, the dot included. A name that starts with its only dot has no
/// ending, and neither has one whose ending is long or has a space in it.
fn split_ending(name: &str) -> (&str, &str) {
    if let Some(double) = DOUBLE_ENDINGS.iter().find(|ending| {
        let at = name.len().saturating_sub(ending.len());
        at > 0 && name.is_char_boundary(at) && name[at..].eq_ignore_ascii_case(ending)
    }) {
        return name.split_at(name.len() - double.len());
    }
    match name.rfind('.') {
        Some(at) if at > 0 && name.len() - at <= 8 && !name[at..].contains(' ') => {
            name.split_at(at)
        }
        _ => (name, ""),
    }
}

/// A stem without the count a name already has, and the number to go on from: `a (copy 2)` is `a`
/// and 3 for a copy, `a (4)` is `a` and 5 otherwise.
fn counted(stem: &str, copy: bool) -> (&str, u32) {
    let first = u32::from(!copy) + 1;
    let Some(open) = stem.strip_suffix(')').and_then(|rest| rest.rfind(" (")) else {
        return (stem, first);
    };
    let inside = &stem[open + 2..stem.len() - 1];
    let number = if copy {
        match inside.strip_prefix("copy") {
            Some("") => Some(2),
            Some(rest) => rest.trim().parse::<u32>().ok().map(|n| n + 1),
            None => None,
        }
    } else {
        inside.parse::<u32>().ok().map(|n| n + 1)
    };
    number.map_or((stem, first), |number| (&stem[..open], number))
}

/// A path as a `file://` address, each byte outside the unreserved ones and the slash written as
/// `%XX`, the way `GLib` writes a file's uri.
#[must_use]
pub fn uri(path: &Path) -> String {
    format!("file://{}", escaped(path))
}

/// A path with each byte outside the unreserved ones and the slash written as `%XX`, the way the
/// trash keeps where a file was.
#[must_use]
pub fn escaped(path: &Path) -> String {
    let mut out = String::new();
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// The path an escaped one stands for. A `%` without two hex digits after it is kept as it is.
#[must_use]
pub fn unescaped(text: &str) -> PathBuf {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%'
            && let Some(byte) = text
                .get(at + 1..at + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(byte);
            at += 3;
            continue;
        }
        out.push(bytes[at]);
        at += 1;
    }
    PathBuf::from(OsString::from_vec(out))
}

/// The path a `file://` address or a plain path stands for, as a desktop entry's `%U` or a
/// person's typing gives it. `~` at the start is home.
#[must_use]
pub fn path_of(given: &str) -> PathBuf {
    let given = given.trim();
    if let Some(rest) = given.strip_prefix("file://") {
        // a host between the slashes, which is this machine or nothing
        let rest = rest.find('/').map_or(rest, |at| &rest[at..]);
        return unescaped(rest);
    }
    if given == "~" {
        return home().unwrap_or_else(|| PathBuf::from("/"));
    }
    if let Some(rest) = given.strip_prefix("~/")
        && let Some(home) = home()
    {
        return home.join(rest);
    }
    PathBuf::from(given)
}

/// The top of the file system a path is on: the folder over it that something is mounted at,
/// found by walking up while the file system stays the same. `/` for anything on the root file
/// system. The trash specification calls it the top directory, and a drive keeps its own trash
/// there.
#[must_use]
pub fn top_of(path: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let start = if fs::symlink_metadata(path).ok()?.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };
    let mut top = fs::canonicalize(&start).ok()?;
    let device = fs::metadata(&top).ok()?.dev();
    while let Some(parent) = top.parent() {
        match fs::metadata(parent) {
            Ok(meta) if meta.dev() == device => top = parent.to_path_buf(),
            _ => break,
        }
    }
    Some(top)
}

/// The account this is running as, which names the trash on a drive. The kernel says so where
/// there is a /proc, and home's own owner says it everywhere else.
#[must_use]
pub fn uid() -> u32 {
    use std::os::unix::fs::MetadataExt;
    fs::metadata("/proc/self")
        .ok()
        .or_else(|| home().and_then(|home| fs::metadata(home).ok()))
        .map_or(0, |meta| meta.uid())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("librift-files-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("the test folder");
        root
    }

    #[test]
    fn names_are_in_the_order_a_person_reads_them() {
        let mut names = vec![
            "file 10", "File 9", "file 1", "apple", "Banana", "file 01b", "10",
        ];
        names.sort_by(|one, other| natural(one, other));
        assert_eq!(
            names,
            [
                "10", "apple", "Banana", "file 1", "file 01b", "File 9", "file 10"
            ]
        );
        assert_eq!(natural("a", "a"), Ordering::Equal);
        assert_eq!(natural("a2", "a10"), Ordering::Less);
    }

    #[test]
    fn a_list_has_its_folders_first() {
        let entry = |name: &str, kind, size, modified| Entry {
            name: OsString::from(name),
            label: name.to_string(),
            kind,
            link: false,
            size,
            modified: Some(modified),
            mime: String::new(),
            hidden: false,
            items: None,
        };
        let mut entries = vec![
            entry("b.txt", Kind::File, 10, 3),
            entry("Zeta", Kind::Folder, 0, 1),
            entry("a.txt", Kind::File, 300, 2),
            entry("alpha", Kind::Folder, 0, 4),
        ];
        let names = |entries: &[Entry]| {
            entries
                .iter()
                .map(|entry| entry.label.clone())
                .collect::<Vec<_>>()
        };
        sort(&mut entries, Options::default());
        assert_eq!(names(&entries), ["alpha", "Zeta", "a.txt", "b.txt"]);
        let by_size = Options {
            sort: Sort::Size,
            ..Options::default()
        };
        sort(&mut entries, by_size);
        assert_eq!(names(&entries), ["alpha", "Zeta", "a.txt", "b.txt"]);
        let newest = Options {
            sort: Sort::Modified,
            ..Options::default()
        };
        sort(&mut entries, newest);
        assert_eq!(names(&entries), ["alpha", "Zeta", "b.txt", "a.txt"]);
        sort(
            &mut entries,
            Options {
                reversed: true,
                ..Options::default()
            },
        );
        assert_eq!(names(&entries), ["Zeta", "alpha", "b.txt", "a.txt"]);
    }

    #[test]
    fn sizes_read_the_way_gnome_writes_them() {
        assert_eq!(size_words(0), "0 bytes");
        assert_eq!(size_words(1), "1 byte");
        assert_eq!(size_words(999), "999 bytes");
        assert_eq!(size_words(1000), "1.0 kB");
        assert_eq!(size_words(12_345), "12.3 kB");
        assert_eq!(size_words(999_949), "999.9 kB");
        assert_eq!(size_words(999_999), "1.0 MB");
        assert_eq!(size_words(4_000_000_000), "4.0 GB");
        assert_eq!(items_words(0), "Empty");
        assert_eq!(items_words(1), "1 item");
        assert_eq!(items_words(12), "12 items");
    }

    #[test]
    fn a_time_reads_as_close_as_it_is() {
        // 2026-09-22 10:42:00 UTC
        let now = 1_790_073_720;
        assert_eq!(when_words(now - 60, now, 0), "10:41");
        assert_eq!(when_words(now - 86_400, now, 0), "Yesterday");
        assert_eq!(when_words(now - 5 * 86_400, now, 0), "17 Sep");
        assert_eq!(when_words(now - 400 * 86_400, now, 0), "18 Aug 2025");
        // twelve hours east it is already 22:42, and the minute before is still today
        assert_eq!(when_words(now - 60, now, 12 * 3600), "22:41");
        assert_eq!(offset_of("+1200"), Some(43_200));
        assert_eq!(offset_of("-0330"), Some(-12_600));
        assert_eq!(offset_of("UTC"), None);
    }

    #[test]
    fn a_stamp_goes_there_and_back() {
        let now = 1_790_073_720;
        assert_eq!(local_stamp(now, 0), "2026-09-22T10:42:00");
        assert_eq!(local_stamp(now, 43_200), "2026-09-22T22:42:00");
        assert_eq!(stamp_seconds("2026-09-22T22:42:00", 43_200), Some(now));
        assert_eq!(stamp_seconds("2026-09-22T10:42", 0), Some(now));
        assert_eq!(stamp_seconds("yesterday", 0), None);
    }

    #[test]
    fn a_name_has_to_be_one_a_file_can_have() {
        assert_eq!(check_name("Notes"), Ok(()));
        assert_eq!(check_name(".config"), Ok(()));
        assert_eq!(check_name("  "), Err("Type a name."));
        assert_eq!(check_name("a/b"), Err("A name cannot have a slash in it."));
        assert_eq!(check_name(".."), Err("A name cannot be a dot or two dots."));
        assert_eq!(check_name(&"x".repeat(256)), Err("The name is too long."));
    }

    #[test]
    fn a_copy_gets_a_name_of_its_own() {
        let folder = temporary("names");
        let free = |name: &str, copy| {
            free_name(&folder, OsStr::new(name), copy)
                .to_string_lossy()
                .into_owned()
        };
        assert_eq!(free("report.pdf", true), "report.pdf");
        fs::write(folder.join("report.pdf"), "").unwrap();
        assert_eq!(free("report.pdf", true), "report (copy).pdf");
        fs::write(folder.join("report (copy).pdf"), "").unwrap();
        assert_eq!(free("report.pdf", true), "report (copy 2).pdf");
        assert_eq!(free("report (copy).pdf", true), "report (copy 2).pdf");
        assert_eq!(free("report.pdf", false), "report (2).pdf");
        fs::write(folder.join("a.tar.gz"), "").unwrap();
        assert_eq!(free("a.tar.gz", true), "a (copy).tar.gz");
        fs::create_dir(folder.join("New folder")).unwrap();
        assert_eq!(free("New folder", false), "New folder (2)");
        fs::write(folder.join(".bashrc"), "").unwrap();
        assert_eq!(free(".bashrc", true), ".bashrc (copy)");
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_path_is_escaped_the_way_glib_escapes_it() {
        let path = Path::new("/home/rift/My notes/b(1).txt");
        assert_eq!(escaped(path), "/home/rift/My%20notes/b%281%29.txt");
        assert_eq!(uri(path), "file:///home/rift/My%20notes/b%281%29.txt");
        assert_eq!(unescaped(&escaped(path)), path);
        assert_eq!(
            path_of("file:///home/rift/My%20notes"),
            Path::new("/home/rift/My notes")
        );
        assert_eq!(path_of("file://localhost/tmp"), Path::new("/tmp"));
        assert_eq!(path_of("/tmp/x"), Path::new("/tmp/x"));
        // a name that is not UTF-8 goes there and back as bytes
        let odd = PathBuf::from(OsString::from_vec(b"/tmp/\xff".to_vec()));
        assert_eq!(escaped(&odd), "/tmp/%FF");
        assert_eq!(unescaped("/tmp/%FF"), odd);
        assert_eq!(unescaped("100%"), Path::new("100%"));
    }

    #[test]
    fn a_folder_with_read_only_folders_is_deleted_all_the_same() {
        use std::os::unix::fs::PermissionsExt;
        let folder = temporary("remove");
        fs::create_dir_all(folder.join("cache/locked")).unwrap();
        fs::write(folder.join("cache/locked/a"), "a").unwrap();
        fs::set_permissions(
            folder.join("cache/locked"),
            fs::Permissions::from_mode(0o555),
        )
        .unwrap();
        std::os::unix::fs::symlink(folder.join("cache"), folder.join("link")).unwrap();
        // a link goes, and what it points at stays
        remove(&folder.join("link")).unwrap();
        assert!(folder.join("cache/locked/a").exists());
        remove(&folder.join("cache")).unwrap();
        assert!(!folder.join("cache").exists());
        assert_eq!(
            remove(&folder.join("cache")).unwrap_err(),
            "cache is not there any more."
        );
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_options_read_back_from_their_file() {
        let options = Options {
            hidden: true,
            sort: Sort::Modified,
            reversed: true,
        };
        assert_eq!(options.text(), "hidden on\nsort modified reversed\n");
        assert_eq!(Options::parse(&options.text()), options);
        assert_eq!(
            Options::parse("sort nowhere\nhidden maybe\n"),
            Options::default()
        );
    }

    #[test]
    fn a_folder_reads_with_what_a_list_shows() {
        let folder = temporary("read");
        fs::create_dir(folder.join("Notes")).unwrap();
        fs::write(folder.join("Notes/a.txt"), "a").unwrap();
        fs::write(folder.join("report.txt"), "twelve bytes").unwrap();
        fs::write(folder.join(".secret"), "").unwrap();
        fs::write(folder.join("listed"), "").unwrap();
        fs::write(folder.join(".hidden"), "listed\n").unwrap();
        std::os::unix::fs::symlink(folder.join("nowhere"), folder.join("broken")).unwrap();
        std::os::unix::fs::symlink(folder.join("Notes"), folder.join("shortcut")).unwrap();
        let types = mime::Database::parse("50:text/plain:*.txt\n", "", "", "");
        let mut entries = read(&folder, &types).unwrap();
        sort(&mut entries, Options::default());
        let seen: Vec<(String, Kind, bool, bool)> = entries
            .iter()
            .map(|entry| (entry.label.clone(), entry.kind, entry.link, entry.hidden))
            .collect();
        assert_eq!(
            seen,
            [
                ("Notes".to_string(), Kind::Folder, false, false),
                ("shortcut".to_string(), Kind::Folder, true, false),
                ("broken".to_string(), Kind::Broken, true, false),
                (".hidden".to_string(), Kind::File, false, true),
                ("listed".to_string(), Kind::File, false, true),
                ("report.txt".to_string(), Kind::File, false, false),
                (".secret".to_string(), Kind::File, false, true),
            ]
        );
        let notes = &entries[0];
        assert_eq!(
            (notes.items, notes.mime.as_str()),
            (Some(1), "inode/directory")
        );
        let report = entries
            .iter()
            .find(|entry| entry.label == "report.txt")
            .unwrap();
        assert_eq!((report.size, report.mime.as_str()), (12, "text/plain"));
        assert!(
            read(&folder.join("nowhere"), &types)
                .unwrap_err()
                .ends_with("is not there any more.")
        );
        let _ = fs::remove_dir_all(&folder);
    }
}
