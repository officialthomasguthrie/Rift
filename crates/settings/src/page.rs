//! The pages of Settings, in the order the sidebar lists them. Each one has a name, the word the
//! control socket takes, and a symbolic icon.

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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
