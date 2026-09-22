//! Welcome's pages. The first is the start page, or the page that says there is no network when
//! there is none; the steps after it are in the sidebar, each with its label, the word the control
//! socket takes, and a symbolic icon.

/// One page of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    /// The mark, a few sentences and the way on.
    Start,
    /// The start page when there is no network: it says so and offers to open later.
    Offline,
    /// Dark or light, the accent and the wallpaper.
    Appearance,
    /// The apps Rift suggests, from Flathub.
    Apps,
    /// The languages on the drive, and how to run the others.
    Developer,
    /// What is still installing, and the button that closes Welcome.
    Done,
}

impl Page {
    /// Every page, in the order the words are listed.
    pub const ALL: [Self; 6] = [
        Self::Start,
        Self::Offline,
        Self::Appearance,
        Self::Apps,
        Self::Developer,
        Self::Done,
    ];

    /// The steps the sidebar lists, in order.
    pub const STEPS: [Self; 4] = [Self::Appearance, Self::Apps, Self::Developer, Self::Done];

    /// The page's name in the sidebar and over the page.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Start | Self::Offline => "Welcome",
            Self::Appearance => "Appearance",
            Self::Apps => "Apps",
            Self::Developer => "Developer",
            Self::Done => "Done",
        }
    }

    /// The word the control socket takes and `--state` prints.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Offline => "offline",
            Self::Appearance => "appearance",
            Self::Apps => "apps",
            Self::Developer => "developer",
            Self::Done => "done",
        }
    }

    /// The page a word names.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        let word = word.trim();
        Self::ALL
            .into_iter()
            .find(|page| page.word().eq_ignore_ascii_case(word))
    }

    /// The symbolic icon of the page's row in the sidebar, from the Adwaita theme.
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Start | Self::Offline => "go-home-symbolic",
            Self::Appearance => "applications-graphics-symbolic",
            Self::Apps => "system-software-install-symbolic",
            Self::Developer => "utilities-terminal-symbolic",
            Self::Done => "object-select-symbolic",
        }
    }

    /// Whether the page is one of the steps, drawn beside the sidebar. The start page and the page
    /// with no network fill the window by themselves.
    #[must_use]
    pub fn is_step(self) -> bool {
        Self::STEPS.contains(&self)
    }

    /// The step after this one.
    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self {
            Self::Start | Self::Offline => Some(Self::Appearance),
            Self::Appearance => Some(Self::Apps),
            Self::Apps => Some(Self::Developer),
            Self::Developer => Some(Self::Done),
            Self::Done => None,
        }
    }

    /// The step before this one. Before the first step is the start page.
    #[must_use]
    pub const fn back(self) -> Option<Self> {
        match self {
            Self::Start | Self::Offline => None,
            Self::Appearance => Some(Self::Start),
            Self::Apps => Some(Self::Appearance),
            Self::Developer => Some(Self::Apps),
            Self::Done => Some(Self::Developer),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_reads_back_from_its_word() {
        for page in Page::ALL {
            assert_eq!(Page::from_word(page.word()), Some(page));
            assert_eq!(
                Page::from_word(&page.word().to_ascii_uppercase()),
                Some(page)
            );
        }
        assert_eq!(Page::from_word("settings"), None);
    }

    #[test]
    fn next_and_back_walk_the_steps_in_order() {
        let mut walked = vec![Page::Start];
        while let Some(next) = walked.last().and_then(|page| page.next()) {
            walked.push(next);
        }
        assert_eq!(
            walked,
            [
                Page::Start,
                Page::Appearance,
                Page::Apps,
                Page::Developer,
                Page::Done
            ]
        );
        for pair in walked.windows(2) {
            assert_eq!(pair[1].back(), Some(pair[0]));
        }
        assert_eq!(Page::Offline.next(), Some(Page::Appearance));
        assert!(Page::STEPS.iter().all(|page| page.is_step()));
        assert!(!Page::Start.is_step() && !Page::Offline.is_step());
    }
}
