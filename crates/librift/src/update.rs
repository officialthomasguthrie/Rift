//! The two slots of the drive: which version is in which, what each version's uki on the esp has
//! left, and where updates come from.
//!
//! A slot is a store partition and the verity tree beside it. systemd-sysupdate writes a new
//! version into the slot that is not running and puts its uki on the esp with three tries.
//! systemd-boot takes a try off the file name each time it starts that uki, and systemd-bless-boot
//! drops the counter once the boot is good, so three boots that never come up start the version in
//! the other slot again. Vault reads all of this as root and the Updates page draws it.

use std::cmp::Ordering;

/// One slot of the drive.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Slot {
    /// Which slot it is, `a` or `b`.
    pub slot: String,
    /// The version its partitions hold, empty when the slot is empty.
    pub version: String,
    /// The name of that version's uki on the esp, empty when there is none.
    pub uki: String,
}

impl Slot {
    /// How many more times systemd-boot will start this version before it gives up on it. `None`
    /// when the uki has no counter: a boot of it was marked good, or it never had one.
    #[must_use]
    pub fn tries(&self) -> Option<u32> {
        counter(&self.uki).map(|(left, _)| left)
    }

    /// Whether the slot holds a version.
    #[must_use]
    pub fn filled(&self) -> bool {
        !self.version.is_empty()
    }
}

/// What the drive holds and what is waiting to go onto it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Slots {
    /// The version running now.
    pub running: String,
    /// The slots, slot a first.
    pub slots: Vec<Slot>,
    /// Where updates come from, the way the transfer files have it.
    pub source: String,
    /// The versions in that folder, oldest first. Empty when updates come from somewhere this
    /// machine cannot look in.
    pub waiting: Vec<String>,
}

impl Slots {
    /// The slot the running version is in, when one of them holds it.
    #[must_use]
    pub fn running_slot(&self) -> Option<&str> {
        self.slots
            .iter()
            .find(|slot| slot.version == self.running && !self.running.is_empty())
            .map(|slot| slot.slot.as_str())
    }

    /// What Vault answers on the bus.
    #[must_use]
    pub fn answer(&self) -> Answer {
        (
            self.running.clone(),
            self.slots
                .iter()
                .map(|slot| (slot.slot.clone(), slot.version.clone(), slot.uki.clone()))
                .collect(),
            self.source.clone(),
            self.waiting.clone(),
        )
    }

    /// The picture in what Vault answered.
    #[must_use]
    pub fn from_answer(answer: Answer) -> Self {
        let (running, slots, source, waiting) = answer;
        Self {
            running,
            slots: slots
                .into_iter()
                .map(|(slot, version, uki)| Slot { slot, version, uki })
                .collect(),
            source,
            waiting,
        }
    }

    /// The newest version waiting where updates come from, when it is newer than every version on
    /// the drive.
    #[must_use]
    pub fn newer(&self) -> Option<&str> {
        let newest = self.waiting.iter().max_by(|a, b| compare(a, b))?;
        let on_the_drive = self
            .slots
            .iter()
            .filter(|slot| slot.filled())
            .map(|slot| slot.version.as_str())
            .chain(Some(self.running.as_str()).filter(|running| !running.is_empty()))
            .max_by(|a, b| compare(a, b));
        match on_the_drive {
            Some(held) if compare(newest, held) != Ordering::Greater => None,
            _ => Some(newest.as_str()),
        }
    }
}

/// What Vault's `Slots` method carries on the bus: the version running, the slot, version and uki
/// name of each slot, where updates come from, and the versions waiting there.
pub type Answer = (String, Vec<(String, String, String)>, String, Vec<String>);

/// The boot counter in a uki's name: `rift_0.2.0+3-0.efi` has three tries left and none done,
/// `rift_0.2.0+2.efi` has two left, and `rift_0.2.0.efi` has no counter at all.
#[must_use]
pub fn counter(name: &str) -> Option<(u32, u32)> {
    let rest = name.strip_suffix(".efi")?;
    counted(rest.rsplit_once('+')?.1)
}

/// The two numbers of a counter, `3-0` or `2`, and nothing else.
fn counted(text: &str) -> Option<(u32, u32)> {
    let (left, done) = text.split_once('-').unwrap_or((text, "0"));
    Some((number(left)?, number(done)?))
}

/// A whole number of digits and nothing else.
fn number(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// Whether `name` is the uki of `version` of the image `id`, with or without a boot counter.
#[must_use]
pub fn names_uki(name: &str, id: &str, version: &str) -> bool {
    let Some(rest) = name
        .strip_prefix(&format!("{id}_{version}"))
        .and_then(|rest| rest.strip_suffix(".efi"))
    else {
        return false;
    };
    rest.is_empty()
        || rest
            .strip_prefix('+')
            .is_some_and(|left| counted(left).is_some())
}

/// The uki of `version` among the file names in the esp's `EFI/Linux`: `rift_0.2.0.efi`, or one
/// with a boot counter, `rift_0.2.0+2.efi` or `rift_0.2.0+1-2.efi`. The one without a counter
/// comes first, since a version that has booted well keeps that name.
#[must_use]
pub fn uki_of(names: &[String], id: &str, version: &str) -> Option<String> {
    let mut found: Vec<&String> = names
        .iter()
        .filter(|name| names_uki(name, id, version))
        .collect();
    found.sort_by_key(|name| (name.len(), name.as_str()));
    found.first().map(|name| (*name).clone())
}

/// The version an update file in the source folder is of: `rift_0.3.0.efi` is version 0.3.0. The
/// store and its verity tree have the partition uuid in the name as well, so only the uki counts.
#[must_use]
pub fn version_of<'a>(name: &'a str, id: &str) -> Option<&'a str> {
    let version = name.strip_prefix(&format!("{id}_"))?.strip_suffix(".efi")?;
    (!version.is_empty() && !version.contains('_') && counter(name).is_none()).then_some(version)
}

/// Two versions in the order the drive puts them in: by their numbers where they are numbers, and
/// as text where they are not.
#[must_use]
pub fn compare(one: &str, other: &str) -> Ordering {
    let mut ours = one.split('.');
    let mut theirs = other.split('.');
    loop {
        match (ours.next(), theirs.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ours), Some(theirs)) => {
                let order = match (number(ours), number(theirs)) {
                    (Some(ours), Some(theirs)) => ours.cmp(&theirs),
                    _ => ours.cmp(theirs),
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

/// The `Path=` of the `[Source]` section of a systemd-sysupdate transfer file, which says where
/// the version's files come from.
#[must_use]
pub fn source_of(text: &str) -> Option<&str> {
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            inside = section.trim().eq_ignore_ascii_case("source");
        } else if inside
            && let Some((key, value)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("path")
            && !value.trim().is_empty()
        {
            return Some(value.trim());
        }
    }
    None
}

/// The folder a source names when it is one on this machine: `file:///var/lib/rift/updates/` is
/// `/var/lib/rift/updates`. `None` when updates come from somewhere else.
#[must_use]
pub fn folder(source: &str) -> Option<&str> {
    let path = source.trim().strip_prefix("file://")?;
    let trimmed = path.trim_end_matches('/');
    Some(if trimmed.is_empty() { "/" } else { trimmed })
}

/// Where updates come from, in the words the page shows: the folder of a source on this machine,
/// or the source as it stands.
#[must_use]
pub fn where_from(source: &str) -> &str {
    folder(source).unwrap_or_else(|| source.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(of: &[&str]) -> Vec<String> {
        of.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn a_uki_carries_the_tries_it_has_left() {
        assert_eq!(counter("rift_0.2.0+3-0.efi"), Some((3, 0)));
        assert_eq!(counter("rift_0.2.0+1-2.efi"), Some((1, 2)));
        assert_eq!(counter("rift_0.2.0+2.efi"), Some((2, 0)));
        assert_eq!(counter("rift_0.2.0.efi"), None);
        assert_eq!(counter("rift_0.2.0+.efi"), None);
        assert_eq!(counter("rift_0.2.0+a-1.efi"), None);
        assert_eq!(counter("rift_0.2.0+1-.efi"), None);
        assert_eq!(counter("rift_0.2.0+3-0.efi.bak"), None);
    }

    #[test]
    fn a_uki_of_another_version_is_never_this_version() {
        assert_eq!(
            uki_of(
                &names(&["rift_0.2.0.efi", "rift_0.3.0+0-3.efi"]),
                "rift",
                "0.2.0"
            )
            .as_deref(),
            Some("rift_0.2.0.efi")
        );
        assert_eq!(uki_of(&names(&["rift_0.10.0.efi"]), "rift", "0.1"), None);
        assert_eq!(
            uki_of(&names(&["rift_0.1.0+2+3.efi"]), "rift", "0.1.0"),
            None
        );
    }

    #[test]
    fn a_slot_says_what_is_in_it() {
        let blessed = Slot {
            slot: "a".to_string(),
            version: "0.1.0".to_string(),
            uki: "rift_0.1.0.efi".to_string(),
        };
        assert!(blessed.filled());
        assert_eq!(blessed.tries(), None);
        let counted = Slot {
            uki: "rift_0.2.0+3-0.efi".to_string(),
            ..blessed.clone()
        };
        assert_eq!(counted.tries(), Some(3));
        assert!(!Slot::default().filled());
        assert_eq!(Slot::default().tries(), None);
    }

    #[test]
    fn the_uki_of_a_version_is_found_by_its_name() {
        assert_eq!(
            uki_of(
                &names(&["rift_0.2.0.efi", "rift_0.1.0.efi"]),
                "rift",
                "0.1.0"
            )
            .as_deref(),
            Some("rift_0.1.0.efi")
        );
        assert_eq!(
            uki_of(&names(&["rift_0.1.0+2-1.efi"]), "rift", "0.1.0").as_deref(),
            Some("rift_0.1.0+2-1.efi")
        );
        assert_eq!(
            uki_of(&names(&["rift_0.1.0+3.efi"]), "rift", "0.1.0").as_deref(),
            Some("rift_0.1.0+3.efi")
        );
        // the one without a counter is the one a good boot left behind
        assert_eq!(
            uki_of(
                &names(&["rift_0.1.0+3-0.efi", "rift_0.1.0.efi"]),
                "rift",
                "0.1.0"
            )
            .as_deref(),
            Some("rift_0.1.0.efi")
        );
        for other in [
            "rift_0.1.0.1.efi",
            "rift_0.1.0+.efi",
            "rift_0.1.0+a-1.efi",
            "rift_0.1.0+1-.efi",
            "rift_0.1.0.efi.bak",
            "other_0.1.0.efi",
        ] {
            assert_eq!(uki_of(&names(&[other]), "rift", "0.1.0"), None, "{other}");
        }
    }

    #[test]
    fn an_update_file_says_which_version_it_is_of() {
        assert_eq!(version_of("rift_0.3.0.efi", "rift"), Some("0.3.0"));
        for other in [
            "SHA256SUMS",
            "rift_0.3.0_8e4b1c2d.store.zst",
            "rift_0.3.0+3-0.efi",
            "other_0.3.0.efi",
            "rift_.efi",
        ] {
            assert_eq!(version_of(other, "rift"), None, "{other}");
        }
    }

    #[test]
    fn versions_are_ordered_by_their_numbers() {
        assert_eq!(compare("0.1.0", "0.2.0"), Ordering::Less);
        assert_eq!(compare("0.10.0", "0.9.0"), Ordering::Greater);
        assert_eq!(compare("0.2.0", "0.2.0"), Ordering::Equal);
        assert_eq!(compare("0.2", "0.2.0"), Ordering::Less);
        assert_eq!(compare("0.2.0-rc1", "0.2.0"), Ordering::Greater);
    }

    #[test]
    fn the_source_comes_out_of_the_transfer_file() {
        let transfer = "[Transfer]\nProtectVersion=%A\n\n[Source]\nType=url-file\n\
                        Path=file:///var/lib/rift/updates/\nMatchPattern=rift_@v.efi\n\n\
                        [Target]\nType=regular-file\nPath=/EFI/Linux\n";
        assert_eq!(source_of(transfer), Some("file:///var/lib/rift/updates/"));
        assert_eq!(
            folder("file:///var/lib/rift/updates/"),
            Some("/var/lib/rift/updates")
        );
        assert_eq!(
            where_from("file:///var/lib/rift/updates/"),
            "/var/lib/rift/updates"
        );
        // a target path is not the source, and a file with no source says nothing
        assert_eq!(source_of("[Target]\nPath=/EFI/Linux\n"), None);
        assert_eq!(source_of(""), None);
        assert_eq!(folder("https://rift.example/updates/"), None);
        assert_eq!(
            where_from("https://rift.example/updates/ "),
            "https://rift.example/updates/"
        );
    }

    #[test]
    fn the_drive_says_what_is_running_and_what_is_waiting() {
        let slots = Slots {
            running: "0.1.0".to_string(),
            slots: vec![
                Slot {
                    slot: "a".to_string(),
                    version: "0.1.0".to_string(),
                    uki: "rift_0.1.0.efi".to_string(),
                },
                Slot {
                    slot: "b".to_string(),
                    version: "0.2.0".to_string(),
                    uki: "rift_0.2.0+3-0.efi".to_string(),
                },
            ],
            source: "file:///var/lib/rift/updates/".to_string(),
            waiting: vec!["0.2.0".to_string()],
        };
        assert_eq!(slots.running_slot(), Some("a"));
        assert_eq!(Slots::from_answer(slots.answer()), slots);
        // what is waiting is already on the drive
        assert_eq!(slots.newer(), None);
        let newer = Slots {
            waiting: vec!["0.2.0".to_string(), "0.3.0".to_string()],
            ..slots.clone()
        };
        assert_eq!(newer.newer(), Some("0.3.0"));
        let empty_b = Slots {
            slots: vec![
                slots.slots[0].clone(),
                Slot {
                    slot: "b".to_string(),
                    ..Slot::default()
                },
            ],
            ..slots.clone()
        };
        assert_eq!(empty_b.newer(), Some("0.2.0"));
        assert_eq!(empty_b.slots[1].tries(), None);
        // a version no slot holds is running from nowhere the drive knows
        let stray = Slots {
            running: "0.9.0".to_string(),
            ..slots
        };
        assert_eq!(stray.running_slot(), None);
        assert_eq!(Slots::default().running_slot(), None);
        assert_eq!(Slots::default().newer(), None);
    }
}
