//! The Dock page: the apps the dock keeps and their order, the edge it stands on, whether it runs
//! from one side of the screen to the other, whether it hides until the pointer reaches the bottom
//! edge, and how big its icons are.
//!
//! The shell keeps the dock's apps in a file of the owner's and writes it when an app is pinned from
//! its menu in the dock, so the page reads the file as it comes up and every second while it is
//! up, and a right click on the dock moves the page too. What the page changes it writes, then
//! tells the shell to read both files again and stand the dock where they say.

use std::thread;
use std::time::Duration;

use iced::futures::channel::mpsc;
use iced::widget::{column, container, row, text};
use iced::{Center, Element, Fill, Subscription, Task};
use librift::apps::App;
use librift::dock::{self as settings, Change, Edge, Options, Size};

use crate::icons;
use crate::theme::Colors;
use crate::ui::{Message, Settings};
use crate::widgets::{
    GAP, TEXT_SIZE, action, choice, field, group, heading, note, pressable, setting, still, switch,
};

/// The field an app to pin is searched for in.
pub const FIELD: &str = "app";
/// How many apps a search lists at most.
const LISTED: usize = 8;
/// How often the page reads the files again while it is up.
const EVERY: Duration = Duration::from_secs(1);
/// How big an app's icon is in a row.
const ICON: f32 = 24.0;
/// The names `rift-settings --set` takes for this page: the four settings, then the four changes
/// to the list, each with an app's id.
pub const NAMES: [&str; 8] = [
    "dock-position",
    "dock-extend",
    "dock-icons",
    "dock-hide",
    "pin",
    "unpin",
    "move-up",
    "move-down",
];

/// What the two files say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    /// The apps the dock keeps, in order.
    pub pinned: Vec<String>,
    /// Where it stands and how big its icons are.
    pub options: Options,
}

/// Read both files now. They are two short files, so the window reads them itself as the page
/// comes up.
#[must_use]
pub fn reading() -> Picture {
    Picture {
        pinned: settings::pinned(),
        options: Options::read(),
    }
}

/// The files while the page is up: read every second, on a thread that ends at the first send after
/// the page has gone.
pub fn following() -> Subscription<Message> {
    Subscription::run_with("dock", |_| {
        let (sender, receiver) = mpsc::unbounded();
        thread::spawn(move || {
            while sender.unbounded_send(Message::Docked(reading())).is_ok() {
                thread::sleep(EVERY);
            }
        });
        receiver
    })
}

/// What the owner asked for on the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// A change to the list of apps.
    Change(Change),
    /// The settings, with one of them changed.
    Options(Options),
}

/// Do what the owner asked: write the list or the settings, and tell the shell. A change is made to
/// the list as the file has it now, which a right click on the dock may have changed a moment ago.
pub fn asked(state: &mut Settings, asked: Asked) -> Task<Message> {
    state.problem = None;
    let mut now = reading();
    match asked {
        Asked::Change(change) => {
            if matches!(change, Change::Pin(_)) {
                state.finding.clear();
            }
            let Some(after) = change.apply(&now.pinned) else {
                state.dock = Some(now);
                return Task::none();
            };
            state.problem = settings::save(&after).err();
            now.pinned = after;
        }
        Asked::Options(options) => {
            state.problem = options.save().err();
            now.options = options;
        }
    }
    state.dock = Some(now);
    poke_the_shell();
    Task::none()
}

/// Tell the shell that is running to read the dock's files again and stand the dock where they say.
fn poke_the_shell() {
    let _ = std::process::Command::new("lens")
        .arg("--dock")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// What `rift-settings --set` asks of this page, the way pressing it would: a setting and its word,
/// or a change to the list and an app's id. Nothing for a word that is not one, or an app to pin
/// that no desktop entry has.
#[must_use]
pub fn named(state: &Settings, name: &str, value: &str) -> Option<Asked> {
    let value = value.trim().to_string();
    let change = match name {
        "pin" => {
            if !state.apps.iter().any(|app| app.id == value) {
                return None;
            }
            Change::Pin(value)
        }
        "unpin" => Change::Unpin(value),
        "move-up" => Change::Up(value),
        "move-down" => Change::Down(value),
        _ => {
            let mut options = state
                .dock
                .as_ref()
                .map_or_else(Options::read, |now| now.options);
            return options.set(name, &value).then_some(Asked::Options(options));
        }
    };
    Some(Asked::Change(change))
}

/// The name an app goes by on the page: its desktop entry's, or its id when it has none here.
fn name_of<'a>(apps: &'a [App], key: &'a str) -> &'a str {
    apps.iter()
        .find(|app| app.id == key)
        .map_or(key, |app| app.name.as_str())
}

/// The lines `rift-settings --state` prints, once the page has read the files: how many apps the
/// dock keeps with a line for each in order, and every setting.
#[must_use]
pub fn state(state: &Settings) -> Vec<String> {
    let Some(now) = state.dock.as_ref() else {
        return Vec::new();
    };
    let mut lines = vec![format!("pinned {}", now.pinned.len())];
    for key in &now.pinned {
        lines.push(format!("pinned-app {key} {}", name_of(&state.apps, key)));
    }
    lines.extend(now.options.lines());
    lines
}

/// The apps a search finds, leaving out the ones the dock keeps: a name that starts with what was
/// typed first, then one with a word that does, then one that has it anywhere. Nothing when nothing
/// was typed.
fn find<'a>(apps: &'a [App], typed: &str, pinned: &[String]) -> Vec<&'a App> {
    let typed = typed.trim().to_lowercase();
    if typed.is_empty() {
        return Vec::new();
    }
    let rank = |app: &App| {
        let name = app.name.to_lowercase();
        if name.starts_with(&typed) {
            Some(0)
        } else if name.split_whitespace().any(|word| word.starts_with(&typed)) {
            Some(1)
        } else if name.contains(&typed) {
            Some(2)
        } else {
            None
        }
    };
    let mut found: Vec<(u8, &App)> = apps
        .iter()
        .filter(|app| !pinned.contains(&app.id))
        .filter_map(|app| rank(app).map(|at| (at, app)))
        .collect();
    // the list of apps is in the order of their names, and the sort keeps it inside each rank
    found.sort_by_key(|(at, _)| *at);
    found.into_iter().map(|(_, app)| app).collect()
}

/// The page.
pub fn view(state: &Settings, look: Colors) -> Element<'_, Message> {
    let Some(now) = state.dock.as_ref() else {
        return note(look, "Reading the dock's settings.");
    };
    let mut page = column![
        the_apps(state, look, now),
        the_position(look, now.options),
        the_size(look, now.options),
    ]
    .spacing(GAP)
    .width(Fill);
    if let Some(why) = &state.problem {
        page = page.push(text(why).size(TEXT_SIZE).color(look.error));
    }
    page.into()
}

/// The apps the dock keeps, in order, each with the buttons that move it and take it off, and the
/// search that finds another to pin.
fn the_apps<'a>(state: &'a Settings, look: Colors, now: &'a Picture) -> Element<'a, Message> {
    let last = now.pinned.len().saturating_sub(1);
    let mut rows: Vec<Element<'a, Message>> = now
        .pinned
        .iter()
        .enumerate()
        .map(|(at, key)| kept(state, look, key, at > 0, at < last))
        .collect();
    if rows.is_empty() {
        rows.push(
            container(note(look, "The dock keeps no apps."))
                .padding([8, 12])
                .into(),
        );
    }
    let found = find(&state.apps, &state.finding, &now.pinned);
    let entered = found.first().map_or_else(
        || Message::Find(state.finding.clone()),
        |app| Message::Dock(Asked::Change(Change::Pin(app.id.clone()))),
    );
    rows.push(setting(
        look,
        "Pin an app",
        None,
        field(
            look,
            "Name of the app",
            &state.finding,
            false,
            FIELD,
            Message::Find,
            entered,
        ),
    ));
    for app in found.iter().take(LISTED) {
        rows.push(pressable(
            look,
            named_row(look, app.icon.as_deref(), &app.name),
            None,
            false,
            Message::Dock(Asked::Change(Change::Pin(app.id.clone()))),
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
            container(note(look, "No app that is not in the dock has that name."))
                .padding([8, 12])
                .into(),
        );
    }
    column![
        heading(look, "Apps in the dock"),
        group(look, rows),
        note(look, APPS)
    ]
    .spacing(8)
    .into()
}

/// An app's icon in its own colours and its name, the way the dock draws the one and a menu the
/// other.
fn named_row<'a>(look: Colors, icon: Option<&str>, name: &str) -> Element<'a, Message> {
    row![
        icons::of_app(look.text, icon, ICON),
        text(name.to_string()).size(TEXT_SIZE).color(look.text),
    ]
    .spacing(12)
    .align_y(Center)
    .into()
}

/// One app the dock keeps, with Move up while it is not the first, Move down while it is not the
/// last, and Unpin.
fn kept<'a>(
    state: &'a Settings,
    look: Colors,
    key: &'a str,
    up: bool,
    down: bool,
) -> Element<'a, Message> {
    let app = state.apps.iter().find(|app| app.id == key);
    let change = |change: fn(String) -> Change| Message::Dock(Asked::Change(change(key.into())));
    let buttons = row![
        action(look, "Move up", up.then(|| change(Change::Up))),
        action(look, "Move down", down.then(|| change(Change::Down))),
        action(look, "Unpin", Some(change(Change::Unpin))),
    ]
    .spacing(8);
    container(
        row![
            container(named_row(
                look,
                app.and_then(|app| app.icon.as_deref()),
                name_of(&state.apps, key),
            ))
            .width(Fill),
            buttons,
        ]
        .align_y(Center)
        .spacing(GAP),
    )
    .width(Fill)
    .padding([8, 12])
    .into()
}

/// The edge the dock stands on, whether it runs from one side to the other, and whether it hides.
fn the_position<'a>(look: Colors, options: Options) -> Element<'a, Message> {
    let edges = [(Edge::Bottom, "Bottom"), (Edge::Top, "Top")]
        .into_iter()
        .map(|(edge, label)| {
            choice(
                look,
                label,
                None,
                None,
                options.edge == edge,
                Message::Dock(Asked::Options(Options { edge, ..options })),
            )
        })
        .collect();
    // along the top the dock stays, since the pointer crosses that edge on its way to the bar, so
    // the switch stands still there and keeps what it says for the bottom
    let hiding = if options.edge == Edge::Bottom {
        setting(
            look,
            "Hide automatically",
            Some(HIDE),
            switch(look, options.hide, move |hide| {
                Message::Dock(Asked::Options(Options { hide, ..options }))
            }),
        )
    } else {
        setting(
            look,
            "Hide automatically",
            Some(HIDE_TOP),
            still(look, options.hide),
        )
    };
    let switches = vec![
        setting(
            look,
            "Extend to the edges",
            Some(EXTEND),
            switch(look, options.extend, move |extend| {
                Message::Dock(Asked::Options(Options { extend, ..options }))
            }),
        ),
        hiding,
    ];
    column![
        heading(look, "Position"),
        group(look, edges),
        group(look, switches),
    ]
    .spacing(8)
    .into()
}

/// How big the icons are.
fn the_size<'a>(look: Colors, options: Options) -> Element<'a, Message> {
    let sizes = Size::ALL
        .into_iter()
        .map(|size| {
            choice(
                look,
                size.label(),
                None,
                None,
                options.size == size,
                Message::Dock(Asked::Options(Options { size, ..options })),
            )
        })
        .collect();
    column![heading(look, "Icon size"), group(look, sizes)]
        .spacing(8)
        .into()
}

/// Under the list of apps.
const APPS: &str = "The dock shows these in this order, then the apps that are running. A right \
                    click on an app in the dock pins it or takes it off too.";
/// Under the switch that makes the dock run from side to side.
const EXTEND: &str = "Off, the dock is only as wide as its apps and stands in the middle of its \
                      edge, a little way off it.";
/// Under the switch that makes the dock hide.
const HIDE: &str = "Windows have the room it stood in, and it comes back over them when the \
                    pointer reaches the bottom edge of the screen.";
/// The same, with the dock along the top.
const HIDE_TOP: &str = "Along the top the dock stays, since the pointer crosses it on the way to \
                        the bar.";

#[cfg(test)]
mod tests {
    use super::*;
    use librift::apps::Category;

    fn app(id: &str, name: &str) -> App {
        App {
            id: id.to_string(),
            name: name.to_string(),
            exec: vec![id.to_string()],
            terminal: false,
            icon: None,
            wm_class: None,
            category: Category::Accessories,
            types: Vec::new(),
        }
    }

    fn kept_apps() -> Vec<App> {
        vec![
            app("org.gnome.Calculator", "Calculator"),
            app("firefox", "Firefox"),
            app("Helix", "Helix"),
            app("com.mitchellh.ghostty", "Ghostty"),
            app("org.gnome.Loupe", "Image Viewer"),
        ]
    }

    fn settings(pinned: &[&str]) -> Settings {
        let mut state = Settings::bare();
        state.apps = kept_apps();
        state.dock = Some(Picture {
            pinned: pinned.iter().map(|&key| key.to_string()).collect(),
            options: Options::default(),
        });
        state
    }

    #[test]
    fn the_state_says_the_apps_in_order_and_every_setting() {
        assert!(state(&Settings::bare()).is_empty());
        let kept = settings(&["firefox", "org.example.Gone"]);
        assert_eq!(
            state(&kept),
            [
                "pinned 2",
                "pinned-app firefox Firefox",
                // an app with no entry here goes by its id
                "pinned-app org.example.Gone org.example.Gone",
                "dock-position bottom",
                "dock-extend on",
                "dock-icons small",
                "dock-hide off",
            ]
        );
    }

    #[test]
    fn a_setting_from_a_terminal_is_what_the_page_would_press() {
        let kept = settings(&["firefox"]);
        assert_eq!(
            named(&kept, "dock-position", "top"),
            Some(Asked::Options(Options {
                edge: Edge::Top,
                ..Options::default()
            }))
        );
        assert_eq!(
            named(&kept, "move-up", " firefox "),
            Some(Asked::Change(Change::Up("firefox".into())))
        );
        // an app is pinned by the id of an entry there is
        assert_eq!(
            named(&kept, "pin", "Helix"),
            Some(Asked::Change(Change::Pin("Helix".into())))
        );
        assert_eq!(named(&kept, "pin", "klingon"), None);
        assert_eq!(named(&kept, "dock-icons", "huge"), None);
        assert_eq!(
            named(&kept, "dock-hide", "on"),
            Some(Asked::Options(Options {
                hide: true,
                ..Options::default()
            }))
        );
        assert_eq!(named(&kept, "autohide", "on"), None);
    }

    #[test]
    fn a_search_finds_the_apps_that_are_not_kept() {
        let apps = kept_apps();
        let names = |typed: &str, pinned: &[&str]| {
            let pinned: Vec<String> = pinned.iter().map(|&key| key.to_string()).collect();
            find(&apps, typed, &pinned)
                .iter()
                .map(|app| app.name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names("", &[]), Vec::<String>::new());
        // a name that starts with it, then a word that does, then one that has it anywhere
        assert_eq!(names("i", &[]), ["Image Viewer", "Firefox", "Helix"]);
        assert_eq!(names("view", &[]), ["Image Viewer"]);
        assert_eq!(names("fire", &["firefox"]), Vec::<String>::new());
    }

    #[test]
    fn the_sentences_are_sentences() {
        for sentence in [APPS, EXTEND, HIDE, HIDE_TOP] {
            assert!(sentence.ends_with('.'), "{sentence}");
            assert!(sentence.is_ascii(), "{sentence}");
        }
    }
}
