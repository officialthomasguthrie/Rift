//! Small pictures of files, the way every desktop keeps them: a png under
//! `~/.cache/thumbnails/normal` or `large`, named by the md5 of the file's address, made once by
//! one of the programs the system has for the kind of file and read from the cache after that.
//!
//! The programs are the freedesktop ones: a `.thumbnailer` file under `share/thumbnailers` names
//! the kinds of file it makes a picture of and the command that makes it. The image has the one
//! that comes with gdk-pixbuf, which covers the picture formats, and the one that comes with
//! Papers, which covers pdf, djvu and comic books. A kind no program names has no picture, and the
//! file is drawn with the icon of its kind instead.
//!
//! A picture is only ever written into the owner's own cache. The specification also allows one
//! beside the file, at the top of the disk it is on, for a disk carried between machines; Rift
//! never writes that, so a disk that is only there to be read is never written to.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// How wide and tall a small picture is, at most.
pub const NORMAL: u32 = 128;
/// How wide and tall a large one is.
pub const LARGE: u32 = 256;

/// Where the pictures of one size are kept, under the owner's cache.
#[must_use]
pub fn folder(large: bool) -> Option<PathBuf> {
    let cache = match std::env::var_os("XDG_CACHE_HOME").filter(|path| !path.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => crate::appearance::home()?.join(".cache"),
    };
    Some(
        cache
            .join("thumbnails")
            .join(if large { "large" } else { "normal" }),
    )
}

/// What the picture of an address is called: the md5 of the address, written in hex, with `.png`
/// after it.
#[must_use]
pub fn name_of(uri: &str) -> String {
    use std::fmt::Write as _;

    let mut name = String::with_capacity(36);
    for byte in md5(uri.as_bytes()) {
        let _ = write!(name, "{byte:02x}");
    }
    name.push_str(".png");
    name
}

/// Where the picture of a file would lie.
#[must_use]
pub fn place_of(path: &Path, large: bool) -> Option<PathBuf> {
    Some(folder(large)?.join(name_of(&super::uri(path))))
}

/// The picture of a file that has been made already, when there is one and it is still of the file
/// as it is now: the address and the time written into the png are the file's own.
#[must_use]
pub fn made(path: &Path, modified: Option<i64>, large: bool) -> Option<PathBuf> {
    let place = place_of(path, large)?;
    let png = fs::read(&place).ok()?;
    let held = keys(&png);
    (held.get("Thumb::URI")? == &super::uri(path)
        && held.get("Thumb::MTime")?.parse::<i64>().ok() == modified)
        .then_some(place)
}

/// One program that makes pictures of some kinds of file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Maker {
    /// The command, with the places for the size, the address and the file to write left in it.
    pub exec: String,
    /// The kinds of file it makes a picture of.
    pub kinds: Vec<String>,
}

/// Every program the system has, in the order the folders are looked in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Makers {
    /// The programs, the first that names a kind being the one used for it.
    pub makers: Vec<Maker>,
}

impl Makers {
    /// What `share/thumbnailers` holds, in every folder the system looks in for shared files. A
    /// program whose `TryExec` is not there is left out.
    #[must_use]
    pub fn load() -> Self {
        let mut makers = Vec::new();
        for folder in super::mime::data_dirs() {
            let Ok(entries) = fs::read_dir(folder.join("thumbnailers")) else {
                continue;
            };
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.extension().is_some_and(|end| end == "thumbnailer")
                    && let Ok(text) = fs::read_to_string(&path)
                    && let Some(maker) = Maker::parse(&text)
                {
                    makers.push(maker);
                }
            }
        }
        Self { makers }
    }

    /// The program that makes a picture of this kind of file, when the system has one.
    #[must_use]
    pub fn for_kind(&self, kind: &str) -> Option<&Maker> {
        self.makers
            .iter()
            .find(|maker| maker.kinds.iter().any(|named| named == kind))
    }

    /// Whether any program makes a picture of this kind.
    #[must_use]
    pub fn covers(&self, kind: &str) -> bool {
        self.for_kind(kind).is_some()
    }

    /// Make the picture of a file and say where it went. It is written beside the place it belongs
    /// under a name of its own, has the address of the file and the file's time written into it,
    /// and is moved there once it is whole, so a picture half written is never read.
    ///
    /// # Errors
    ///
    /// A sentence when no program makes a picture of that kind, when the cache cannot be written,
    /// or when the program fails.
    pub fn make(&self, path: &Path, kind: &str, large: bool) -> Result<PathBuf, String> {
        let maker = self
            .for_kind(kind)
            .ok_or_else(|| format!("Nothing here makes a picture of {kind}."))?;
        let place = place_of(path, large).ok_or("There is no cache to keep pictures in.")?;
        let folder = place
            .parent()
            .ok_or("There is no cache to keep pictures in.")?;
        make_folder(folder)?;
        let being_made = folder.join(format!(
            "{}.{}",
            std::process::id(),
            place.file_name().unwrap_or_default().to_string_lossy()
        ));
        let size = if large { LARGE } else { NORMAL };
        let words = maker.command(path, &being_made, size);
        let (program, rest) = words.split_first().ok_or("That program has no command.")?;
        let ran = Command::new(program).args(rest).status();
        let done = match ran {
            Ok(status) if status.success() => being_made.is_file(),
            _ => false,
        };
        if !done {
            let _ = fs::remove_file(&being_made);
            return Err(format!("{program} made no picture of {}.", path.display()));
        }
        // the programs write a picture and nothing else: gdk-pixbuf's writes how wide and tall the
        // picture is, and the specification leaves the address and the time to whoever keeps the
        // cache, which is this
        let whole = fs::read(&being_made)
            .ok()
            .and_then(|png| with_keys(&png, &super::uri(path), modified_of(path)?));
        let Some(whole) = whole else {
            let _ = fs::remove_file(&being_made);
            return Err(format!("{program} made no png of {}.", path.display()));
        };
        fs::write(&being_made, whole).map_err(|e| {
            let _ = fs::remove_file(&being_made);
            format!("Could not keep the picture of {}: {e}", path.display())
        })?;
        own_only(&being_made);
        fs::rename(&being_made, &place).map_err(|e| {
            let _ = fs::remove_file(&being_made);
            format!("Could not keep the picture of {}: {e}", path.display())
        })?;
        Ok(place)
    }
}

impl Maker {
    /// One `.thumbnailer` file: the command and the kinds it names. Nothing when it names neither,
    /// or when the program it names is not on this system.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut exec = String::new();
        let mut tried = String::new();
        let mut kinds = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "Exec" => exec = value.trim().to_string(),
                "TryExec" => tried = value.trim().to_string(),
                "MimeType" => {
                    kinds = value
                        .split(';')
                        .map(str::trim)
                        .filter(|kind| !kind.is_empty())
                        .map(str::to_string)
                        .collect();
                }
                _ => {}
            }
        }
        if exec.is_empty() || kinds.is_empty() {
            return None;
        }
        if !tried.is_empty() && !there(&tried) {
            return None;
        }
        Some(Self { exec, kinds })
    }

    /// The command as words, with the file, the place to write and the size in it. `%i` is the
    /// file, `%u` its address, `%o` the picture to write and `%s` how big it may be.
    #[must_use]
    pub fn command(&self, path: &Path, into: &Path, size: u32) -> Vec<String> {
        self.exec
            .split_whitespace()
            .map(|word| match word {
                "%i" => path.display().to_string(),
                "%u" => super::uri(path),
                "%o" => into.display().to_string(),
                "%s" => size.to_string(),
                other => other.to_string(),
            })
            .collect()
    }
}

/// Whether a program is there to run: a path that is a file, or a name on the path.
fn there(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).is_file();
    }
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|folder| folder.join(program).is_file())
    })
}

/// Make the cache folder, and the folder over it, readable by the owner alone: what a person has
/// looked at is their own.
fn make_folder(folder: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    fs::create_dir_all(folder).map_err(|e| format!("Could not make {}: {e}", folder.display()))?;
    for each in [folder.parent(), Some(folder)].into_iter().flatten() {
        let _ = fs::set_permissions(each, fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

/// A picture only the owner can read, which is what the specification asks for.
fn own_only(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

/// The words a png carries beside its picture, which is where the address of the file and the time
/// it was last changed are kept. Only the plain ones are read, which is what a thumbnailer writes.
#[must_use]
pub fn keys(png: &[u8]) -> HashMap<String, String> {
    let mut found = HashMap::new();
    let mut at = 8;
    while at + 8 <= png.len() {
        let length = u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]) as usize;
        let kind = &png[at + 4..at + 8];
        let from = at + 8;
        let Some(to) = from.checked_add(length).filter(|to| *to <= png.len()) else {
            break;
        };
        if kind == b"tEXt"
            && let Some(split) = png[from..to].iter().position(|byte| *byte == 0)
        {
            let key = String::from_utf8_lossy(&png[from..from + split]).into_owned();
            let value = String::from_utf8_lossy(&png[from + split + 1..to]).into_owned();
            found.insert(key, value);
        }
        if kind == b"IDAT" || kind == b"IEND" {
            break;
        }
        at = to + 4;
    }
    found
}

/// When a file was last changed, in seconds, which is what a picture of it carries.
fn modified_of(path: &Path) -> Option<i64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_secs()).ok())
}

/// The same png with the address of the file and the time it was last changed written into it,
/// after the header, and any it already carried taken out. Nothing when it is not a png.
#[must_use]
pub fn with_keys(png: &[u8], uri: &str, modified: i64) -> Option<Vec<u8>> {
    if !png.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        return None;
    }
    let mut out = png.get(..8)?.to_vec();
    let mut first = true;
    let mut at = 8;
    while at + 12 <= png.len() {
        let length = u32::from_be_bytes(png.get(at..at + 4)?.try_into().ok()?) as usize;
        let kind = png.get(at + 4..at + 8)?;
        let end = at.checked_add(12)?.checked_add(length)?;
        if end > png.len() {
            return None;
        }
        // ours take the place of any the program wrote, so there is one of each
        let named = |name: &[u8]| {
            kind == b"tEXt"
                && png[at + 8..]
                    .iter()
                    .take_while(|byte| **byte != 0)
                    .eq(name.iter())
        };
        if !named(b"Thumb::URI") && !named(b"Thumb::MTime") {
            out.extend_from_slice(&png[at..end]);
        }
        if first {
            out.extend_from_slice(&chunk(*b"tEXt", &text_of("Thumb::URI", uri)));
            out.extend_from_slice(&chunk(
                *b"tEXt",
                &text_of("Thumb::MTime", &modified.to_string()),
            ));
            first = false;
        }
        if kind == b"IEND" {
            return Some(out);
        }
        at = end;
    }
    None
}

/// The body of a `tEXt` chunk: the key, nothing, and what it says.
fn text_of(key: &str, value: &str) -> Vec<u8> {
    let mut body = key.as_bytes().to_vec();
    body.push(0);
    body.extend_from_slice(value.as_bytes());
    body
}

/// One png chunk: how long it is, what it is, what it holds, and the check of the last two.
fn chunk(kind: [u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = u32::try_from(body.len())
        .unwrap_or_default()
        .to_be_bytes()
        .to_vec();
    out.extend_from_slice(&kind);
    out.extend_from_slice(body);
    out.extend_from_slice(&check(&out[4..]).to_be_bytes());
    out
}

/// The check every png chunk ends with.
fn check(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// The md5 of some bytes, which is what names a picture in the cache. It is the name the
/// specification gives and nothing is kept safe with it.
#[must_use]
pub fn md5(bytes: &[u8]) -> [u8; 16] {
    let mut state: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    let mut message = bytes.to_vec();
    let length = (bytes.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&length.to_le_bytes());
    for block in message.chunks_exact(64) {
        let mut words = [0u32; 16];
        for (word, four) in words.iter_mut().zip(block.chunks_exact(4)) {
            *word = u32::from_le_bytes([four[0], four[1], four[2], four[3]]);
        }
        round(&mut state, &words);
    }
    let mut out = [0u8; 16];
    for (four, word) in out.chunks_exact_mut(4).zip(state) {
        four.copy_from_slice(&word.to_le_bytes());
    }
    out
}

/// One block of the md5, the four passes of sixteen steps the algorithm is made of.
fn round(state: &mut [u32; 4], words: &[u32; 16]) {
    let [mut a, mut b, mut c, mut d] = *state;
    for step in 0..64 {
        let (mixed, which) = match step / 16 {
            0 => ((b & c) | (!b & d), step),
            1 => ((d & b) | (!d & c), (5 * step + 1) % 16),
            2 => (b ^ c ^ d, (3 * step + 5) % 16),
            _ => (c ^ (b | !d), (7 * step) % 16),
        };
        let sum = a
            .wrapping_add(mixed)
            .wrapping_add(ADD[step])
            .wrapping_add(words[which]);
        a = d;
        d = c;
        c = b;
        b = b.wrapping_add(sum.rotate_left(SHIFT[step]));
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
}

/// How far each step turns its word.
const SHIFT: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

/// What each step adds, the table the algorithm is defined with.
const ADD: [u32; 64] = [
    0xd76a_a478,
    0xe8c7_b756,
    0x2420_70db,
    0xc1bd_ceee,
    0xf57c_0faf,
    0x4787_c62a,
    0xa830_4613,
    0xfd46_9501,
    0x6980_98d8,
    0x8b44_f7af,
    0xffff_5bb1,
    0x895c_d7be,
    0x6b90_1122,
    0xfd98_7193,
    0xa679_438e,
    0x49b4_0821,
    0xf61e_2562,
    0xc040_b340,
    0x265e_5a51,
    0xe9b6_c7aa,
    0xd62f_105d,
    0x0244_1453,
    0xd8a1_e681,
    0xe7d3_fbc8,
    0x21e1_cde6,
    0xc337_07d6,
    0xf4d5_0d87,
    0x455a_14ed,
    0xa9e3_e905,
    0xfcef_a3f8,
    0x676f_02d9,
    0x8d2a_4c8a,
    0xfffa_3942,
    0x8771_f681,
    0x6d9d_6122,
    0xfde5_380c,
    0xa4be_ea44,
    0x4bde_cfa9,
    0xf6bb_4b60,
    0xbebf_bc70,
    0x289b_7ec6,
    0xeaa1_27fa,
    0xd4ef_3085,
    0x0488_1d05,
    0xd9d4_d039,
    0xe6db_99e5,
    0x1fa2_7cf8,
    0xc4ac_5665,
    0xf429_2244,
    0x432a_ff97,
    0xab94_23a7,
    0xfc93_a039,
    0x655b_59c3,
    0x8f0c_cc92,
    0xffef_f47d,
    0x8584_5dd1,
    0x6fa8_7e4f,
    0xfe2c_e6e0,
    0xa301_4314,
    0x4e08_11a1,
    0xf753_7e82,
    0xbd3a_f235,
    0x2ad7_d2bb,
    0xeb86_d391,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        use std::fmt::Write as _;

        bytes.iter().fold(String::new(), |mut written, byte| {
            let _ = write!(written, "{byte:02x}");
            written
        })
    }

    #[test]
    fn the_md5_of_the_known_answers() {
        assert_eq!(hex(&md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(&md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            hex(&md5(b"The quick brown fox jumps over the lazy dog")),
            "9e107d9d372bb6826bd81d3542a419d6"
        );
        assert_eq!(
            hex(&md5(&b"1234567890".repeat(8))),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
        // the example in the thumbnail specification itself
        assert_eq!(
            name_of("file:///home/jens/photos/me.png"),
            "c6ee772d9e49320e97ec29a7eb5b1697.png"
        );
    }

    #[test]
    fn a_thumbnailer_file_is_read_the_way_the_image_writes_one() {
        let text = "[Thumbnailer Entry]\n\
                    TryExec=/usr/bin/gdk-pixbuf-thumbnailer\n\
                    Exec=/usr/bin/gdk-pixbuf-thumbnailer -s %s %u %o\n\
                    MimeType=image/png;image/jpeg;\n";
        // the program named is not on this machine, so it is left out
        assert_eq!(Maker::parse(text), None);
        let maker = Maker::parse(
            "[Thumbnailer Entry]\nExec=picture -s %s %u %o\nMimeType=image/png;image/jpeg;\n",
        )
        .expect("a thumbnailer");
        assert_eq!(maker.kinds, ["image/png", "image/jpeg"]);
        let makers = Makers {
            makers: vec![maker.clone()],
        };
        assert!(makers.covers("image/png") && !makers.covers("video/mp4"));
        assert_eq!(
            maker.command(
                Path::new("/home/rift/a b.png"),
                Path::new("/tmp/out.png"),
                128
            ),
            [
                "picture",
                "-s",
                "128",
                "file:///home/rift/a%20b.png",
                "/tmp/out.png"
            ]
        );
        assert_eq!(Maker::parse("[Thumbnailer Entry]\nExec=picture %o\n"), None);
    }

    #[test]
    fn the_address_and_the_time_are_written_into_the_picture() {
        // what a thumbnailer writes: a png with the size of the picture in it and nothing else
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        png.extend_from_slice(&chunk(*b"IHDR", &[0; 13]));
        png.extend_from_slice(&chunk(*b"tEXt", &text_of("Thumb::Image::Width", "1920")));
        png.extend_from_slice(&chunk(
            *b"tEXt",
            &text_of("Thumb::URI", "file:///the/old/one"),
        ));
        png.extend_from_slice(&chunk(*b"IDAT", b"a picture"));
        png.extend_from_slice(&chunk(*b"IEND", b""));
        let whole = with_keys(&png, "file:///home/rift/a.png", 1_700_000_000).expect("a png");
        let held = keys(&whole);
        assert_eq!(
            held.get("Thumb::URI").map(String::as_str),
            Some("file:///home/rift/a.png"),
            "the one the program wrote is gone and ours is there"
        );
        assert_eq!(
            held.get("Thumb::MTime").map(String::as_str),
            Some("1700000000")
        );
        assert_eq!(
            held.get("Thumb::Image::Width").map(String::as_str),
            Some("1920"),
            "what the program wrote about the picture is kept"
        );
        assert!(whole.ends_with(&chunk(*b"IEND", b"")));
        assert_eq!(with_keys(b"not a png", "file:///a", 1), None);
    }

    #[test]
    fn the_words_a_png_carries_are_read() {
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        let mut chunk = |kind: &[u8], body: &[u8]| {
            png.extend_from_slice(&u32::try_from(body.len()).expect("a length").to_be_bytes());
            png.extend_from_slice(kind);
            png.extend_from_slice(body);
            png.extend_from_slice(&[0, 0, 0, 0]);
        };
        chunk(b"IHDR", &[0; 13]);
        chunk(b"tEXt", b"Thumb::URI\0file:///home/rift/a.png");
        chunk(b"tEXt", b"Thumb::MTime\x001700000000");
        chunk(b"IDAT", b"not really");
        let held = keys(&png);
        assert_eq!(
            held.get("Thumb::URI").map(String::as_str),
            Some("file:///home/rift/a.png")
        );
        assert_eq!(
            held.get("Thumb::MTime").map(String::as_str),
            Some("1700000000")
        );
        assert!(keys(&[]).is_empty());
    }
}
