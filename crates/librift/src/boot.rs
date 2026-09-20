//! The boot style: whether a drive shows the text boot or the graphical one while it starts.
//!
//! It is the one setting of the Appearance page that cannot live in home. The image is read only,
//! and plymouth has drawn the passphrase prompt before persist is open, so neither is there to read
//! from. The word lives on the esp instead, the writable part of the drive the boot can reach:
//! Vault writes it, since the esp is root's, and `liftoff-style` reads it in the initrd and gives
//! plymouth the theme for it before the splash comes up.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

/// Where the word lives on the esp, under its root.
pub const ON_ESP: &str = "rift/boot-style";

/// The variable systemd-boot leaves with the partition uuid of the esp it was started from, so a
/// host disk's esp is never the one read.
pub const LOADER_PARTITION: &str =
    "/sys/firmware/efi/efivars/LoaderDevicePartUUID-4a67b082-0a4c-41cf-b6c7-440b29bb8c4f";

/// Where udev names partitions by their uuids.
pub const PARTITIONS: &str = "/dev/disk/by-partuuid";

/// The two ways the boot looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Style {
    /// The logo in characters with the system's own status lines under it, the default.
    #[default]
    Text,
    /// The mark alone, in the middle of the screen.
    Graphical,
}

impl Style {
    /// Both styles, in the order the page shows them.
    pub const ALL: [Style; 2] = [Self::Text, Self::Graphical];

    /// The style a word names: `graphical` is the graphical one, anything else is text.
    #[must_use]
    pub fn from_setting(text: &str) -> Self {
        if text.trim().eq_ignore_ascii_case("graphical") {
            Self::Graphical
        } else {
            Self::Text
        }
    }

    /// The word for it, as the esp holds it and as Settings prints it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Graphical => "graphical",
        }
    }

    /// The name of it on the page.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Graphical => "Graphical",
        }
    }

    /// The sentence under that name on the page.
    #[must_use]
    pub const fn note(self) -> &'static str {
        match self {
            Self::Text => "The logo, with what the system is doing under it.",
            Self::Graphical => "The mark alone, in the middle of the screen.",
        }
    }

    /// The plymouth theme that draws it. Both are in the initrd.
    #[must_use]
    pub const fn theme(self) -> &'static str {
        match self {
            Self::Text => "liftoff-text",
            Self::Graphical => "liftoff-graphical",
        }
    }

    /// The style a mounted esp names, or text when it names none. A drive whose owner has never
    /// chosen has no file, and so does one written by an older version.
    #[must_use]
    pub fn read(esp: &Path) -> Self {
        fs::read_to_string(esp.join(ON_ESP)).map_or(Self::Text, |text| Self::from_setting(&text))
    }

    /// Write the style onto a mounted esp, and wait for the drive to have it: the next boot reads
    /// this file before anything else of the system is up, so it has to be on the stick and not in
    /// a cache when the power goes.
    ///
    /// # Errors
    ///
    /// A sentence when the folder or the file could not be written.
    pub fn write(self, esp: &Path) -> Result<(), String> {
        let path = esp.join(ON_ESP);
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder)
                .map_err(|e| format!("Could not make {}: {e}", folder.display()))?;
        }
        let mut file =
            File::create(&path).map_err(|e| format!("Could not write {}: {e}", path.display()))?;
        file.write_all(format!("{}\n", self.word()).as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|e| format!("Could not write {}: {e}", path.display()))
    }
}

/// The partition uuid in `LoaderDevicePartUUID`: four bytes of attributes, then the uuid in UTF-16
/// ending in a zero. In lower case, the way udev names partitions.
#[must_use]
pub fn loader_partition(variable: &[u8]) -> Option<String> {
    let units: Vec<u16> = variable
        .get(4..)?
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|&unit| unit != 0)
        .collect();
    let uuid = String::from_utf16(&units).ok()?.to_ascii_lowercase();
    (uuid.len() == 36 && uuid.chars().all(|c| c.is_ascii_hexdigit() || c == '-')).then_some(uuid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_names_a_style_and_a_style_names_a_theme() {
        assert_eq!(Style::from_setting("graphical"), Style::Graphical);
        assert_eq!(Style::from_setting(" Graphical\n"), Style::Graphical);
        assert_eq!(Style::from_setting("text"), Style::Text);
        assert_eq!(Style::from_setting(""), Style::Text);
        assert_eq!(Style::from_setting("anything"), Style::Text);
        assert_eq!(Style::default(), Style::Text);
        for style in Style::ALL {
            assert_eq!(Style::from_setting(style.word()), style);
            assert!(style.theme().starts_with("liftoff-"));
            assert!(!style.note().is_empty());
        }
    }

    #[test]
    fn an_esp_gives_back_the_style_it_was_written() {
        let esp = std::env::temp_dir().join(format!("rift-boot-{}", std::process::id()));
        let _ = fs::remove_dir_all(&esp);
        fs::create_dir_all(&esp).unwrap();
        // nothing written yet, and an esp of a drive nobody has chosen on
        assert_eq!(Style::read(&esp), Style::Text);
        for style in [Style::Graphical, Style::Text, Style::Graphical] {
            style.write(&esp).unwrap();
            assert_eq!(Style::read(&esp), style);
            assert_eq!(
                fs::read_to_string(esp.join(ON_ESP)).unwrap(),
                format!("{}\n", style.word())
            );
        }
        let _ = fs::remove_dir_all(&esp);
    }

    #[test]
    fn the_loader_variable_holds_a_partition_uuid() {
        let uuid = "b9e0d4f2-1c3a-4e5b-8f70-2a6c9d1e4b83";
        let mut variable = vec![7, 0, 0, 0];
        for unit in uuid.to_ascii_uppercase().encode_utf16() {
            variable.extend_from_slice(&unit.to_le_bytes());
        }
        variable.extend_from_slice(&[0, 0]);
        assert_eq!(loader_partition(&variable).as_deref(), Some(uuid));
        assert_eq!(loader_partition(&[1, 2, 3]), None);
        assert_eq!(loader_partition(&[1, 2, 3, 4]), None);
        // a value that is not a uuid is not one, however long it is
        let mut wrong = vec![7, 0, 0, 0];
        for unit in "not a partition uuid at all, no".encode_utf16() {
            wrong.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(loader_partition(&wrong), None);
    }
}
