//! The pages of Settings, in the order the sidebar lists them. Each one has a name, the word the
//! control socket takes, a symbolic icon, and a sentence for the pages the system cannot do from
//! here yet.

/// One page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// Wireless networks.
    Wifi,
    /// Wired networks and VPN.
    Network,
    /// Bluetooth devices.
    Bluetooth,
    /// Screens, their resolution and their arrangement.
    Displays,
    /// Output and input devices.
    Sound,
    /// The battery and what the machine does when it is idle.
    Power,
    /// What the desktop looks like.
    Appearance,
    /// The dock along the bottom.
    Dock,
    /// Default apps and app permissions.
    Apps,
    /// Which notifications come through.
    Notifications,
    /// What search looks at.
    Search,
    /// Layouts and shortcuts.
    Keyboard,
    /// The mouse and the touchpad.
    Pointer,
    /// Printers and scanners.
    Printers,
    /// Larger text, contrast, zoom, the screen reader, the on-screen keyboard.
    Accessibility,
    /// The camera, the microphone, the screen lock and the firewall.
    Privacy,
    /// The person the drive belongs to.
    Owner,
    /// The clock and the time zone.
    DateTime,
    /// The language and the formats.
    Region,
    /// The two slots, rollback and firmware.
    Updates,
    /// Snapshots and backups, which Vault keeps.
    Backups,
    /// Quasar's tier and its models.
    Ai,
    /// Rift, the machine, and the host class.
    About,
}

impl Page {
    /// Every page, in the order the sidebar lists them, which is the order GNOME Settings uses.
    pub const ALL: [Page; 23] = [
        Self::Wifi,
        Self::Network,
        Self::Bluetooth,
        Self::Displays,
        Self::Sound,
        Self::Power,
        Self::Appearance,
        Self::Dock,
        Self::Apps,
        Self::Notifications,
        Self::Search,
        Self::Keyboard,
        Self::Pointer,
        Self::Printers,
        Self::Accessibility,
        Self::Privacy,
        Self::Owner,
        Self::DateTime,
        Self::Region,
        Self::Updates,
        Self::Backups,
        Self::Ai,
        Self::About,
    ];

    /// The page the app opens on. Appearance, until the rest of them do something.
    pub const FIRST: Page = Self::Appearance;

    /// The name in the sidebar and at the top of the page.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Wifi => "Wi-Fi",
            Self::Network => "Network",
            Self::Bluetooth => "Bluetooth",
            Self::Displays => "Displays",
            Self::Sound => "Sound",
            Self::Power => "Power",
            Self::Appearance => "Appearance",
            Self::Dock => "Dock",
            Self::Apps => "Apps",
            Self::Notifications => "Notifications",
            Self::Search => "Search",
            Self::Keyboard => "Keyboard",
            Self::Pointer => "Mouse and touchpad",
            Self::Printers => "Printers",
            Self::Accessibility => "Accessibility",
            Self::Privacy => "Privacy and security",
            Self::Owner => "Owner",
            Self::DateTime => "Date and time",
            Self::Region => "Region and language",
            Self::Updates => "Updates",
            Self::Backups => "Backups",
            Self::Ai => "AI",
            Self::About => "About",
        }
    }

    /// The word `rift-settings --page` takes and `--state` prints.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Wifi => "wifi",
            Self::Network => "network",
            Self::Bluetooth => "bluetooth",
            Self::Displays => "displays",
            Self::Sound => "sound",
            Self::Power => "power",
            Self::Appearance => "appearance",
            Self::Dock => "dock",
            Self::Apps => "apps",
            Self::Notifications => "notifications",
            Self::Search => "search",
            Self::Keyboard => "keyboard",
            Self::Pointer => "pointer",
            Self::Printers => "printers",
            Self::Accessibility => "accessibility",
            Self::Privacy => "privacy",
            Self::Owner => "owner",
            Self::DateTime => "datetime",
            Self::Region => "region",
            Self::Updates => "updates",
            Self::Backups => "backups",
            Self::Ai => "ai",
            Self::About => "about",
        }
    }

    /// The symbolic icon of the row, from the Adwaita theme.
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Wifi => "network-wireless-symbolic",
            Self::Network => "network-wired-symbolic",
            Self::Bluetooth => "bluetooth-symbolic",
            Self::Displays => "video-display-symbolic",
            Self::Sound => "audio-speakers-symbolic",
            Self::Power => "battery-symbolic",
            Self::Appearance => "applications-graphics-symbolic",
            Self::Dock => "view-grid-symbolic",
            Self::Apps => "application-x-executable-symbolic",
            Self::Notifications => "preferences-system-notifications-symbolic",
            Self::Search => "system-search-symbolic",
            Self::Keyboard => "input-keyboard-symbolic",
            Self::Pointer => "input-mouse-symbolic",
            Self::Printers => "printer-symbolic",
            Self::Accessibility => "preferences-desktop-accessibility-symbolic",
            Self::Privacy => "channel-secure-symbolic",
            Self::Owner => "avatar-default-symbolic",
            Self::DateTime => "preferences-system-time-symbolic",
            Self::Region => "preferences-desktop-locale-symbolic",
            Self::Updates => "software-update-available-symbolic",
            Self::Backups => "drive-multidisk-symbolic",
            Self::Ai => "starred-symbolic",
            Self::About => "help-about-symbolic",
        }
    }

    /// The page a word names, for the control socket.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        let word = word.trim();
        Self::ALL
            .into_iter()
            .find(|page| word.eq_ignore_ascii_case(page.word()))
    }

    /// What a page that Settings cannot do yet says, in one sentence, with a second one where
    /// there is another way to do it today. A page with one of its own says nothing here.
    #[must_use]
    pub const fn note(self) -> &'static str {
        match self {
            Self::Wifi
            | Self::Network
            | Self::Bluetooth
            | Self::Displays
            | Self::Sound
            | Self::Power
            | Self::Appearance
            | Self::Search
            | Self::Updates
            | Self::Backups
            | Self::Ai
            | Self::About => "",
            Self::Dock => {
                "The dock is not in Settings yet. A right click on an app in the dock pins it or \
                 takes it off."
            }
            Self::Apps => {
                "Default apps and app permissions are not in Settings yet. xdg-mime picks the app \
                 for a kind of file, and rift run puts an app in a sandbox."
            }
            Self::Notifications => {
                "Notification settings are not in Settings yet. The clock menu keeps the ones that \
                 have come in."
            }
            Self::Keyboard => {
                "Keyboard layouts and shortcuts are not in Settings yet. Mod+Shift+Slash shows \
                 every shortcut the desktop has."
            }
            Self::Pointer => "The mouse and the touchpad are not in Settings yet.",
            Self::Printers => {
                "Printers are not in Settings yet. The print dialog of an app finds a printer on \
                 the network, and Document Scanner finds a scanner."
            }
            Self::Accessibility => {
                "Accessibility is not in Settings yet. Mod+Alt+S starts the screen reader and \
                 Mod+Alt+K the on-screen keyboard."
            }
            Self::Privacy => {
                "Privacy and security are not in Settings yet. Mod+L locks the screen, and an app \
                 asks before it takes the camera."
            }
            Self::Owner => {
                "The owner is not in Settings yet. rift host says what this machine is to the drive."
            }
            Self::DateTime => {
                "The clock and the time zone are not in Settings yet. timedatectl sets them from a \
                 terminal."
            }
            Self::Region => {
                "Region and language are not in Settings yet. The system is in British English."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pages that have a page of their own, which `ui::page` draws. The rest draw their name
    /// and one sentence.
    const REAL: [Page; 12] = [
        Page::Wifi,
        Page::Network,
        Page::Bluetooth,
        Page::Displays,
        Page::Sound,
        Page::Power,
        Page::Appearance,
        Page::Search,
        Page::Updates,
        Page::Backups,
        Page::Ai,
        Page::About,
    ];

    #[test]
    fn every_page_has_a_name_a_word_and_an_icon_of_its_own() {
        let mut words: Vec<&str> = Page::ALL.iter().map(|page| page.word()).collect();
        let mut labels: Vec<&str> = Page::ALL.iter().map(|page| page.label()).collect();
        let mut icons: Vec<&str> = Page::ALL.iter().map(|page| page.icon()).collect();
        for list in [&mut words, &mut labels, &mut icons] {
            let before = list.len();
            list.sort_unstable();
            list.dedup();
            assert_eq!(list.len(), before, "two pages share one of these");
        }
        for page in Page::ALL {
            assert_eq!(Page::from_word(page.word()), Some(page));
            assert!(page.icon().ends_with("-symbolic"), "{page:?}");
            assert!(page.label().is_ascii() && page.word().is_ascii());
        }
        assert_eq!(Page::from_word(" About \n"), Some(Page::About));
        assert_eq!(Page::from_word("printer"), None);
    }

    #[test]
    fn a_page_with_nothing_on_it_says_so_in_a_sentence() {
        for page in Page::ALL {
            let note = page.note();
            if REAL.contains(&page) {
                assert!(note.is_empty(), "{page:?}: {note}");
                continue;
            }
            assert!(note.ends_with('.'), "{page:?}: {note}");
            assert!(note.contains("not in Settings yet"), "{page:?}: {note}");
            assert!(note.is_ascii(), "{page:?}: {note}");
        }
    }
}
