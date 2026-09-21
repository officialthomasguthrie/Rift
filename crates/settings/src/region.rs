//! The Region and language page: the language the system is in, the way it writes dates, times
//! and measures, and the keyboard layouts, from systemd's localed, with what each means in words
//! out of the C library's own data about the locale.
//!
//! The language and the formats are part of the image for now, so the page says what they are and
//! has nothing to press; the Keyboard page chooses the layouts. It asks localed as it comes up.

use std::thread;

use iced::futures::channel::oneshot;
use iced::widget::{column, text};
use iced::{Element, Fill, Task};
use librift::keyboard::{self, Layout};
use librift::region::{self, Described, Region};

use crate::ai::said;
use crate::theme::Colors;
use crate::ui::{Message, Settings};
use crate::widgets::{GAP, TEXT_SIZE, group, heading, note, setting};

/// What the page reads.
#[derive(Debug, Clone)]
pub struct Picture {
    /// What localed says the system is set to, or why it did not.
    pub said: Result<Region, String>,
    /// What the C library knows about the locale messages are in, when it could be asked.
    pub language: Option<Described>,
    /// And about the one dates and times are written in, which is usually the same one.
    pub formats: Option<Described>,
}

/// Ask localed and the C library, on a thread of its own: the second runs two programs.
pub fn read() -> Task<Message> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let _ = sender.send(picture());
    });
    Task::perform(receiver, |answered| {
        Message::Region(Box::new(answered.unwrap_or_else(|_| Picture {
            said: Err("localed did not answer.".to_string()),
            language: None,
            formats: None,
        })))
    })
}

/// What the system is set to, and what that means.
fn picture() -> Picture {
    let said = region::read();
    let (language, formats) = match &said {
        Ok(set) => {
            let language = set.language().and_then(region::describe);
            let formats = if set.formats() == set.language() {
                language.clone()
            } else {
                set.formats().and_then(region::describe)
            };
            (language, formats)
        }
        Err(_) => (None, None),
    };
    Picture {
        said,
        language,
        formats,
    }
}

/// The lines `rift-settings --state` prints, once localed has answered: the locale as localed has
/// it, the language in words, the locale dates are written in and the paper it uses, the console's
/// keymap and the desktop's layout.
#[must_use]
pub fn state(state: &Settings) -> Vec<String> {
    let Some(picture) = state.region.as_deref() else {
        return Vec::new();
    };
    let Ok(set) = &picture.said else {
        return Vec::new();
    };
    let or_none = |value: &str| {
        if value.is_empty() {
            "none".to_string()
        } else {
            value.to_string()
        }
    };
    vec![
        format!("locale {}", or_none(&set.locale.join(" "))),
        format!("language {}", or_none(&language(picture, set))),
        format!("formats {}", or_none(set.formats().unwrap_or_default())),
        format!(
            "paper {}",
            picture
                .formats
                .as_ref()
                .map_or_else(|| "none".to_string(), Described::paper_name)
        ),
        format!("keymap {}", or_none(&set.keymap)),
        format!("layout {}", or_none(&set.layout)),
    ]
}

/// The language in words, `English (United Kingdom)`, or the locale's own name where the C library
/// has none for it.
fn language(picture: &Picture, set: &Region) -> String {
    picture
        .language
        .as_ref()
        .and_then(Described::name)
        .unwrap_or_else(|| set.language().unwrap_or_default().to_string())
}

/// The page.
pub fn view(state: &Settings, look: Colors) -> Element<'_, Message> {
    let mut page = column![].spacing(GAP).width(Fill);
    match state.region.as_deref() {
        None => return note(look, "Asking localed what the system is set to."),
        Some(Picture { said: Err(why), .. }) => {
            page = page.push(note(
                look,
                "The language and the keyboard layout are not here, since localed is not \
                 answering.",
            ));
            page = page.push(note(look, why));
        }
        Some(picture @ Picture { said: Ok(set), .. }) => {
            page = page.push(the_language(look, picture, set));
            page = page.push(the_keyboard(look, &state.layouts, set));
        }
    }
    page.push(note(look, NOT_YET)).into()
}

/// The language, the formats, and what the formats mean.
fn the_language<'a>(look: Colors, picture: &Picture, set: &Region) -> Element<'a, Message> {
    let formats = picture
        .formats
        .as_ref()
        .and_then(|described| (!described.country.is_empty()).then(|| described.country.clone()))
        .unwrap_or_else(|| set.formats().unwrap_or("Not set").to_string());
    let mut part = column![
        heading(look, "Language"),
        group(
            look,
            vec![
                setting(look, "Language", None, said(look, &language(picture, set))),
                setting(look, "Formats", None, said(look, &formats)),
            ],
        ),
    ]
    .spacing(8);
    if let Some(described) = &picture.formats {
        part = part.push(text(described.sentence()).size(TEXT_SIZE).color(look.dim));
    }
    part.into()
}

/// The layouts the desktop types with, and the console's keymap, which is the one the passphrase is
/// typed in when the drive starts.
fn the_keyboard<'a>(look: Colors, list: &[Layout], set: &Region) -> Element<'a, Message> {
    column![
        heading(look, "Keyboard"),
        group(
            look,
            vec![
                setting(
                    look,
                    "Layouts",
                    None,
                    said(
                        look,
                        &keyboard::said(&keyboard::chosen(list, &set.layout, &set.variant)),
                    ),
                ),
                setting(
                    look,
                    "Text console",
                    Some("Where the passphrase is typed when the drive starts."),
                    said(look, &region::keymap_words(&set.keymap)),
                ),
            ],
        ),
    ]
    .spacing(8)
    .into()
}

/// What the page cannot do yet, and where the layouts are chosen.
const NOT_YET: &str = "Choosing another language or other formats is not in Settings yet. The \
                       Keyboard page chooses the layouts.";

#[cfg(test)]
mod tests {
    use super::*;

    fn british() -> Described {
        Described {
            language: "English".to_string(),
            country: "United Kingdom".to_string(),
            date: "21/09/26".to_string(),
            time: "14:05:00".to_string(),
            paper: (210, 297),
            metric: true,
            first: 1,
        }
    }

    fn set(locale: &[&str], keymap: &str, layout: &str) -> Region {
        Region {
            locale: locale.iter().map(ToString::to_string).collect(),
            keymap: keymap.to_string(),
            layout: layout.to_string(),
            ..Region::default()
        }
    }

    fn settings(picture: Picture) -> Settings {
        let mut state = Settings::bare();
        state.region = Some(Box::new(picture));
        state
    }

    #[test]
    fn the_state_says_the_locale_the_language_and_the_keyboard() {
        let kept = settings(Picture {
            said: Ok(set(&["LANG=en_GB.UTF-8"], "us", "")),
            language: Some(british()),
            formats: Some(british()),
        });
        assert_eq!(
            state(&kept),
            [
                "locale LANG=en_GB.UTF-8",
                "language English (United Kingdom)",
                "formats en_GB.UTF-8",
                "paper A4",
                "keymap us",
                "layout none",
            ]
        );
    }

    #[test]
    fn a_locale_the_c_library_cannot_describe_is_its_own_name() {
        let kept = settings(Picture {
            said: Ok(set(&["LANG=xx_XX.UTF-8"], "", "gb")),
            language: None,
            formats: None,
        });
        assert_eq!(
            state(&kept),
            [
                "locale LANG=xx_XX.UTF-8",
                "language xx_XX.UTF-8",
                "formats xx_XX.UTF-8",
                "paper none",
                "keymap none",
                "layout gb",
            ]
        );
        let nothing = settings(Picture {
            said: Ok(Region::default()),
            language: None,
            formats: None,
        });
        assert_eq!(
            state(&nothing)[..3],
            ["locale none", "language none", "formats none"]
        );
    }

    #[test]
    fn nothing_is_said_until_localed_has_answered() {
        assert!(state(&Settings::bare()).is_empty());
        let failed = settings(Picture {
            said: Err("localed is not running.".to_string()),
            language: None,
            formats: None,
        });
        assert!(state(&failed).is_empty());
    }

    #[test]
    fn the_sentence_is_a_sentence() {
        assert!(NOT_YET.ends_with('.') && NOT_YET.is_ascii());
        assert!(NOT_YET.contains("not in Settings yet"));
    }
}
