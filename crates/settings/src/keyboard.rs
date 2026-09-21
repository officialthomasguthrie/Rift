//! The Keyboard page: the layouts the desktop types with, the text console's keymap, and the
//! shortcuts.
//!
//! systemd's localed keeps the layouts, and Horizon takes them from it and follows every change, so
//! the page hands localed a new list and Horizon switches without being told. The image keeps
//! localed's file on persist, so the layouts go with the drive. The page reads localed as it comes
//! up and again after every change it makes.

use std::process::Command;
use std::thread;

use iced::futures::channel::oneshot;
use iced::widget::{column, container, text};
use iced::{Element, Fill, Task};
use librift::keyboard::{self as xkb, Layout};
use librift::region::{self, Region};

use crate::ai::said;
use crate::theme::Colors;
use crate::ui::{Message, Settings};
use crate::widgets::{
    GAP, TEXT_SIZE, action, choice, field, group, heading, note, pressable, setting,
};

/// The field a layout is searched for in.
pub const FIELD: &str = "layout";
/// How many layouts a search lists at most.
const LISTED: usize = 8;

/// What the owner asked for on the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// Add the layout with this word at the end of the list.
    Add(String),
    /// Take the layout with this word off the list.
    Remove(String),
    /// Put the layout with this word first, where the desktop starts.
    First(String),
    /// Show every shortcut the desktop has.
    Shortcuts,
}

/// Ask localed now.
fn reading() -> Message {
    Message::Layouts(region::read())
}

/// Ask localed once, on a thread of its own.
pub fn read() -> Task<Message> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let _ = sender.send(reading());
    });
    Task::perform(receiver, |answered| {
        answered.unwrap_or_else(|_| Message::Layouts(Err("localed did not answer.".to_string())))
    })
}

/// Do something on a thread of its own and say how it went.
fn done(work: impl FnOnce() -> Result<(), String> + Send + 'static) -> Task<Result<(), String>> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let _ = sender.send(work());
    });
    Task::perform(receiver, |said| {
        said.unwrap_or_else(|_| Err("It stopped before it finished.".to_string()))
    })
}

/// Do what the owner asked. A new list of layouts goes to localed, and the page reads localed again
/// afterwards, so it shows what localed took rather than what was asked for. The reading is asked
/// for inside the closure, so it starts once the change has finished.
pub fn asked(state: &mut Settings, asked: &Asked) -> Task<Message> {
    state.problem = None;
    if *asked == Asked::Shortcuts {
        return done(show_shortcuts).map(Message::Acted);
    }
    let Some(Ok(set)) = state.keyboard.as_ref() else {
        return Task::none();
    };
    let before = xkb::chosen(&state.layouts, &set.layout, &set.variant);
    let Some(after) = changed(&state.layouts, &before, asked) else {
        return Task::none();
    };
    if matches!(asked, Asked::Add(_)) {
        state.finding.clear();
    }
    let (model, options) = (set.model.clone(), set.options.clone());
    done(move || xkb::set(&after, &model, &options))
        .then(|said| Task::done(Message::Acted(said)).chain(read()))
}

/// The list of layouts after what was asked, or nothing when it asks for no change: a layout the
/// list of layouts there are does not have, one already there, a fifth, taking off the last one, or
/// putting first the one that is first.
fn changed(list: &[Layout], before: &[Layout], asked: &Asked) -> Option<Vec<Layout>> {
    let at = |word: &str| before.iter().position(|layout| layout.is(word));
    let mut after = before.to_vec();
    match asked {
        Asked::Add(word) => {
            let layout = xkb::by_word(list, word)?;
            if at(word).is_some() || before.len() >= xkb::MOST {
                return None;
            }
            after.push(layout.clone());
        }
        Asked::Remove(word) => {
            if before.len() < 2 {
                return None;
            }
            after.remove(at(word)?);
        }
        Asked::First(word) => {
            let found = at(word).filter(|found| *found > 0)?;
            let layout = after.remove(found);
            after.insert(0, layout);
        }
        Asked::Shortcuts => return None,
    }
    Some(after)
}

/// Show the list of shortcuts Mod+Shift+Slash shows, the compositor's own.
fn show_shortcuts() -> Result<(), String> {
    let output = Command::new("horizon")
        .args(["msg", "action", "show-hotkey-overlay"])
        .output()
        .map_err(|e| format!("Could not ask Horizon for the shortcuts: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if said.is_empty() {
        "Horizon did not show the shortcuts.".to_string()
    } else {
        said
    })
}

/// The lines `rift-settings --state` prints, once localed has answered on this page: how many
/// layouts the desktop has, a line for each in order with its word and its name, and the text
/// console's keymap.
#[must_use]
pub fn state(state: &Settings) -> Vec<String> {
    let Some(Ok(set)) = state.keyboard.as_ref() else {
        return Vec::new();
    };
    let chosen = xkb::chosen(&state.layouts, &set.layout, &set.variant);
    let mut lines = vec![format!("layouts {}", chosen.len())];
    for layout in &chosen {
        lines.push(format!("keyboard-layout {} {}", layout.word(), layout.name));
    }
    lines.push(format!(
        "console-keymap {}",
        if set.keymap.is_empty() {
            "none"
        } else {
            set.keymap.as_str()
        }
    ));
    lines
}

/// The page.
pub fn view(state: &Settings, look: Colors) -> Element<'_, Message> {
    let mut page = column![].spacing(GAP).width(Fill);
    match state.keyboard.as_ref() {
        None => return note(look, "Asking localed about the keyboard."),
        Some(Err(why)) => {
            page = page.push(note(
                look,
                "The layouts are not here, since localed is not answering.",
            ));
            page = page.push(note(look, why));
        }
        Some(Ok(set)) => {
            page = page.push(the_layouts(state, look, set));
            page = page.push(the_console(look, set));
        }
    }
    page = page.push(the_shortcuts(look));
    if let Some(why) = &state.problem {
        page = page.push(text(why).size(TEXT_SIZE).color(look.error));
    }
    page.into()
}

/// The layouts, the first with the mark, and the search that finds another.
fn the_layouts<'a>(state: &'a Settings, look: Colors, set: &Region) -> Element<'a, Message> {
    let chosen = xkb::chosen(&state.layouts, &set.layout, &set.variant);
    let more = chosen.len() > 1;
    let mut rows: Vec<Element<'a, Message>> = chosen
        .iter()
        .enumerate()
        .map(|(at, layout)| listed(look, layout, at == 0, more))
        .collect();
    if chosen.len() >= xkb::MOST {
        rows.push(container(note(look, xkb::TOO_MANY)).padding([8, 12]).into());
    } else if state.layouts.is_empty() {
        rows.push(
            container(note(
                look,
                "The list of layouts to choose from is not on this system.",
            ))
            .padding([8, 12])
            .into(),
        );
    } else {
        let found = xkb::find(&state.layouts, &state.finding, &chosen);
        let entered = found.first().map_or_else(
            || Message::Find(state.finding.clone()),
            |layout| Message::Keyboard(Asked::Add(layout.word())),
        );
        rows.push(setting(
            look,
            "Add a layout",
            None,
            field(
                look,
                "Language or country",
                &state.finding,
                false,
                FIELD,
                Message::Find,
                entered,
            ),
        ));
        for layout in found.iter().take(LISTED) {
            rows.push(choice(
                look,
                &layout.name,
                None,
                Some(said(look, &layout.word())),
                false,
                Message::Keyboard(Asked::Add(layout.word())),
            ));
        }
        if found.len() > LISTED {
            rows.push(
                container(note(look, "Type more of the name to find the rest."))
                    .padding([8, 12])
                    .into(),
            );
        } else if found.is_empty() && !state.finding.trim().is_empty() {
            rows.push(
                container(note(look, "No layout has that name."))
                    .padding([8, 12])
                    .into(),
            );
        }
    }
    column![
        heading(look, "Layouts"),
        group(look, rows),
        note(look, LAYOUTS)
    ]
    .spacing(8)
    .into()
}

/// One layout the desktop has: its name, pressed to put it first, with the mark on the first and a
/// button that takes it off the list while there is another.
fn listed<'a>(look: Colors, layout: &Layout, first: bool, more: bool) -> Element<'a, Message> {
    let beside: Option<Element<'a, Message>> = more.then(|| {
        action(
            look,
            "Remove",
            Some(Message::Keyboard(Asked::Remove(layout.word()))),
        )
    });
    pressable(
        look,
        text(layout.name.clone())
            .size(TEXT_SIZE)
            .color(look.text)
            .into(),
        beside,
        first,
        Message::Keyboard(Asked::First(layout.word())),
    )
}

/// The text console's keymap, which the passphrase is typed in.
fn the_console<'a>(look: Colors, set: &Region) -> Element<'a, Message> {
    column![
        heading(look, "Text console"),
        group(
            look,
            vec![setting(
                look,
                "Keymap",
                None,
                said(look, &region::keymap_words(&set.keymap)),
            )],
        ),
        note(look, CONSOLE)
    ]
    .spacing(8)
    .into()
}

/// The shortcuts: a button that shows them, and a sentence.
fn the_shortcuts<'a>(look: Colors) -> Element<'a, Message> {
    column![
        heading(look, "Shortcuts"),
        group(
            look,
            vec![setting(
                look,
                "Keyboard shortcuts",
                Some("Mod+Shift+Slash shows them too."),
                action(look, "Show", Some(Message::Keyboard(Asked::Shortcuts))),
            )],
        ),
        note(look, SHORTCUTS)
    ]
    .spacing(8)
    .into()
}

/// How the layouts are used.
const LAYOUTS: &str = "The desktop starts in the layout with the mark, and Mod+Shift+Space \
                       switches to the next one. Pressing a layout puts it first.";
/// What the console's keymap is for, and why the layouts leave it alone.
const CONSOLE: &str = "The drive asks for its passphrase in this keymap as it starts, before the \
                       desktop and its layouts. It stays the same whatever layouts the desktop \
                       has, so the passphrase types the way it did when it was chosen.";
/// What the page cannot do yet.
const SHORTCUTS: &str = "Changing a shortcut is not in Settings yet.";

#[cfg(test)]
mod tests {
    use super::*;

    /// A few of the layouts there are, the way xkeyboard-config names them.
    fn list() -> Vec<Layout> {
        xkb::parse(
            "! layout\n  us  English (US)\n  gb  English (UK)\n  fr  French\n  de  German\n  \
             es  Spanish\n! variant\n  dvorak  us: English (Dvorak)\n",
        )
    }

    fn words(layouts: &[Layout]) -> Vec<String> {
        layouts.iter().map(Layout::word).collect()
    }

    fn after(before: &[&str], asked: &Asked) -> Option<Vec<String>> {
        let list = list();
        let before: Vec<Layout> = before
            .iter()
            .map(|word| xkb::by_word(&list, word).expect("in the list").clone())
            .collect();
        changed(&list, &before, asked).map(|after| words(&after))
    }

    #[test]
    fn a_press_changes_the_list_the_way_it_says() {
        let add = |word: &str| Asked::Add(word.to_string());
        assert_eq!(
            after(&["us"], &add("gb")),
            Some(vec!["us".into(), "gb".into()])
        );
        assert_eq!(
            after(&["us"], &add("us(dvorak)")),
            Some(vec!["us".into(), "us(dvorak)".into()])
        );
        assert_eq!(
            after(&["us", "gb"], &Asked::First("gb".to_string())),
            Some(vec!["gb".into(), "us".into()])
        );
        assert_eq!(
            after(&["us", "gb", "fr"], &Asked::Remove("gb".to_string())),
            Some(vec!["us".into(), "fr".into()])
        );
    }

    #[test]
    fn a_press_that_changes_nothing_writes_nothing() {
        let add = |word: &str| Asked::Add(word.to_string());
        // one already there, one the list does not have, and a fifth
        assert_eq!(after(&["us", "gb"], &add("gb")), None);
        assert_eq!(after(&["us"], &add("klingon")), None);
        assert_eq!(after(&["us", "gb", "fr", "de"], &add("es")), None);
        // the last one, and one that is not there
        assert_eq!(after(&["us"], &Asked::Remove("us".to_string())), None);
        assert_eq!(after(&["us", "gb"], &Asked::Remove("fr".to_string())), None);
        // the first one first, and the shortcuts, which are not a change to the list
        assert_eq!(after(&["us", "gb"], &Asked::First("us".to_string())), None);
        assert_eq!(after(&["us"], &Asked::Shortcuts), None);
    }

    #[test]
    fn the_state_says_every_layout_in_order_and_the_keymap() {
        assert!(state(&Settings::bare()).is_empty());
        let mut kept = Settings::bare();
        kept.layouts = list();
        kept.keyboard = Some(Ok(Region {
            layout: "gb,us".to_string(),
            variant: ",dvorak".to_string(),
            ..Region::default()
        }));
        assert_eq!(
            state(&kept),
            [
                "layouts 2",
                "keyboard-layout gb English (UK)",
                "keyboard-layout us(dvorak) English (Dvorak)",
                "console-keymap none",
            ]
        );
        // a desktop where none was chosen types with the layout every keyboard starts in
        kept.keyboard = Some(Ok(Region {
            keymap: "us".to_string(),
            ..Region::default()
        }));
        assert_eq!(
            state(&kept),
            [
                "layouts 1",
                "keyboard-layout us English (US)",
                "console-keymap us"
            ]
        );
        kept.keyboard = Some(Err("localed is not running.".to_string()));
        assert!(state(&kept).is_empty());
    }

    #[test]
    fn the_sentences_are_sentences() {
        for sentence in [LAYOUTS, CONSOLE, SHORTCUTS] {
            assert!(sentence.ends_with('.'), "{sentence}");
            assert!(sentence.is_ascii(), "{sentence}");
        }
    }
}
