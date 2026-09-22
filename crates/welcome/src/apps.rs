//! The Apps page: the apps Rift suggests, in their groups, nothing ticked, each with how much room
//! it takes on the drive and whether it needs an account. What flatpak knows is asked on a thread
//! when the page comes up: the remotes the system installation has, the sizes on each remote the
//! list names, which needs the network, and the apps already installed. Install puts the ticked
//! apps in the queue, and a row for each says how it is going.

use std::thread;

use iced::futures::channel::oneshot;
use iced::widget::{button, column, container, row, space, text};
use iced::{Border, Center, Color, Element, Fill, Task, Theme};
use librift::flatpak;
use librift::suggested::{self, App};

use crate::done;
use crate::install::{self, Install, Job};
use crate::theme::Colors;
use crate::ui::{Message, Welcome};
use crate::widgets::{GAP, TEXT_SIZE, action, group, heading, line, note};

/// What one remote said about the size of every app on it, or why it could not say.
pub type Sizes = Result<Vec<(String, String)>, String>;

/// What flatpak said about the apps on the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    /// The system installation's remotes, or why flatpak could not say.
    pub remotes: Result<Vec<String>, String>,
    /// The size of every app on each remote the list names, or why the remote did not say. Left
    /// empty with no network.
    pub sizes: Vec<(String, Sizes)>,
    /// The apps installed, in either installation.
    pub installed: Vec<String>,
}

impl Catalog {
    /// A question to flatpak that got no answer at all.
    fn failed(why: &str) -> Self {
        Self {
            remotes: Err(why.to_string()),
            sizes: Vec::new(),
            installed: Vec::new(),
        }
    }

    /// The remotes the system installation has, or none when flatpak could not say.
    fn remotes(&self) -> &[String] {
        self.remotes.as_deref().unwrap_or_default()
    }

    /// How much room an app takes, when its remote said.
    #[must_use]
    pub fn size(&self, app: &App) -> Option<&str> {
        self.sizes
            .iter()
            .find(|(remote, _)| *remote == app.remote)
            .and_then(|(_, answer)| answer.as_ref().ok())
            .and_then(|sizes| sizes.iter().find(|(id, _)| *id == app.id))
            .map(|(_, size)| size.as_str())
    }

    /// Why Flathub did not say how big its apps are, when it did not, or why flatpak could not
    /// list the remotes.
    fn problem(&self) -> Option<&str> {
        if let Err(why) = &self.remotes {
            return Some(why);
        }
        self.sizes
            .iter()
            .find_map(|(_, answer)| answer.as_ref().err())
            .map(String::as_str)
    }
}

/// Ask flatpak about the apps on a thread of its own: listing a remote reads its summary over the
/// network, which takes a moment.
pub fn ask(state: &mut Welcome) -> Task<Message> {
    let Ok(apps) = &state.apps else {
        return Task::none();
    };
    if state.asking {
        return Task::none();
    }
    state.asking = true;
    let apps = apps.clone();
    let online = state.online;
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let _ = sender.send(read(&apps, online));
    });
    Task::perform(receiver, |answered| {
        Message::Catalog(Box::new(answered.unwrap_or_else(|_| {
            Catalog::failed("The question to flatpak stopped before it was answered.")
        })))
    })
}

/// Ask as the page comes up, unless flatpak has answered it all already.
pub fn ask_once(state: &mut Welcome) -> Task<Message> {
    let answered = state.catalog.as_ref().is_some_and(|catalog| {
        catalog.problem().is_none() && (!state.online || !catalog.sizes.is_empty())
    });
    if answered { Task::none() } else { ask(state) }
}

/// What flatpak says: the remotes, the sizes on each remote an app on the list comes from when
/// there is a network, and what is installed.
fn read(apps: &[App], online: bool) -> Catalog {
    let remotes = flatpak::remotes();
    let mut wanted: Vec<String> = Vec::new();
    for app in suggested::offered(apps, remotes.as_deref().unwrap_or_default()) {
        if !wanted.contains(&app.remote) {
            wanted.push(app.remote.clone());
        }
    }
    // each remote on a thread of its own, so a slow one does not hold back the others
    let sizes = if online {
        thread::scope(|scope| {
            let asked: Vec<_> = wanted
                .into_iter()
                .map(|remote| {
                    scope.spawn(move || {
                        let answer = flatpak::sizes(&remote);
                        (remote, answer)
                    })
                })
                .collect();
            asked
                .into_iter()
                .filter_map(|one| one.join().ok())
                .collect()
        })
    } else {
        Vec::new()
    };
    Catalog {
        remotes,
        sizes,
        installed: flatpak::installed().unwrap_or_default(),
    }
}

/// The apps to list here: every app from Flathub, and one from another remote where the system
/// installation has that remote.
fn offered(state: &Welcome) -> Vec<&App> {
    let remotes = state
        .catalog
        .as_ref()
        .map(|catalog| catalog.remotes())
        .unwrap_or_default();
    state
        .apps
        .as_deref()
        .map(|apps| suggested::offered(apps, remotes))
        .unwrap_or_default()
}

/// Whether an app is installed already.
fn is_installed(state: &Welcome, id: &str) -> bool {
    state
        .catalog
        .as_ref()
        .is_some_and(|catalog| catalog.installed.iter().any(|one| one == id))
}

/// Whether an app is in the queue or installing now.
fn is_pending(state: &Welcome, id: &str) -> bool {
    state
        .installs
        .iter()
        .any(|install| install.id == id && install.doing.pending())
}

/// Whether an app can be ticked: there is a network, it is on the list here, and it is neither
/// installed nor on its way.
fn can_tick(state: &Welcome, id: &str) -> bool {
    state.online
        && offered(state).iter().any(|app| app.id == id)
        && !is_installed(state, id)
        && !is_pending(state, id)
}

/// Tick an app, or take the tick away.
pub fn tick(state: &mut Welcome, id: &str, on: bool) {
    if !on {
        state.ticked.retain(|ticked| ticked != id);
    } else if can_tick(state, id) && !state.ticked.iter().any(|ticked| ticked == id) {
        state.ticked.push(id.to_string());
    }
}

/// Whether Install has something to do.
#[must_use]
pub fn can_install(state: &Welcome) -> bool {
    state.online && !state.ticked.is_empty()
}

/// Put the ticked apps in the queue, in the order the list has them.
pub fn install(state: &mut Welcome) -> Task<Message> {
    if !can_install(state) {
        return Task::none();
    }
    let ticked = std::mem::take(&mut state.ticked);
    let chosen: Vec<(String, String, String)> = offered(state)
        .into_iter()
        .filter(|app| ticked.contains(&app.id))
        .map(|app| (app.id.clone(), app.name.clone(), app.remote.clone()))
        .collect();
    let mut jobs = Vec::new();
    for (id, name, remote) in chosen {
        if is_installed(state, &id) || is_pending(state, &id) {
            continue;
        }
        state.installs.push(Install {
            id: id.clone(),
            name,
            doing: install::Doing::Waiting,
        });
        jobs.push(Job { id, remote });
    }
    install::start(state.queue(), jobs)
}

/// An app has finished installing: it is installed, and no longer ticked.
pub fn installed(state: &mut Welcome, id: &str) {
    state.ticked.retain(|ticked| ticked != id);
    if let Some(catalog) = state.catalog.as_mut()
        && !catalog.installed.iter().any(|one| one == id)
    {
        catalog.installed.push(id.to_string());
    }
}

/// The lines `--state` prints: the remotes, what each remote said, every app listed with its size
/// or that it is installed, and what is ticked.
#[must_use]
pub fn state(state: &Welcome) -> Vec<String> {
    let mut lines = Vec::new();
    if let Err(why) = &state.apps {
        lines.push(format!("apps-problem {why}"));
    }
    match state.catalog.as_ref() {
        None => lines.push(format!(
            "remotes {}",
            if state.asking { "asking" } else { "unknown" }
        )),
        Some(catalog) => {
            match &catalog.remotes {
                Ok(remotes) if remotes.is_empty() => lines.push("remotes none".to_string()),
                Ok(remotes) => lines.push(format!("remotes {}", remotes.join(","))),
                Err(why) => lines.push(format!("remotes-problem {why}")),
            }
            for (remote, answer) in &catalog.sizes {
                lines.push(match answer {
                    Ok(sizes) => format!("sizes {remote} {}", sizes.len()),
                    Err(why) => format!("sizes {remote} problem {why}"),
                });
            }
        }
    }
    for app in offered(state) {
        let size = if is_installed(state, &app.id) {
            "installed".to_string()
        } else {
            state
                .catalog
                .as_ref()
                .and_then(|catalog| catalog.size(app))
                .unwrap_or("unknown")
                .to_string()
        };
        lines.push(format!("app {} {size}", app.id));
    }
    lines.push(format!(
        "ticked {}",
        if state.ticked.is_empty() {
            "none".to_string()
        } else {
            state.ticked.join(",")
        }
    ));
    lines
}

/// The page.
pub fn view(state: &Welcome, look: Colors) -> Element<'_, Message> {
    let mut page = column![note(
        look,
        "These come from Flathub, and each runs in a sandbox of its own. Tick the ones you want and \
         press Install. They go on installing if Welcome is closed.",
    )]
    .spacing(GAP)
    .width(Fill);
    let problem = state.catalog.as_ref().and_then(|catalog| catalog.problem());
    if !state.online {
        page = page.push(note(
            look,
            "There is no network, so nothing can be installed now. Connect from the menu at the \
             right end of the bar, and the sizes come by themselves.",
        ));
    } else if state.asking {
        page = page.push(note(look, "Asking Flathub how big each app is."));
    } else if let Some(why) = problem {
        page = page.push(
            row![
                text(format!("Flathub did not answer. {why}"))
                    .size(TEXT_SIZE)
                    .color(look.error)
                    .width(Fill),
                action(look, "Try again", Some(Message::Ask)),
            ]
            .align_y(Center)
            .spacing(GAP),
        );
    }
    if !state.installs.is_empty() {
        page = page.push(done::installs(state, look));
    }
    if let Err(why) = &state.apps {
        page = page.push(text(why).size(TEXT_SIZE).color(look.error));
    }
    let listed = offered(state);
    for name in suggested::groups(&listed) {
        let rows = listed
            .iter()
            .filter(|app| app.group == name)
            .map(|app| app_row(state, look, app))
            .collect();
        page = page.push(column![heading(look, name), group(look, rows)].spacing(8));
    }
    if let Some(why) = &state.problem {
        page = page.push(text(why).size(TEXT_SIZE).color(look.error));
    }
    page.push(note(
        look,
        "A size is what the app itself takes on the drive. Most apps also need a runtime, which \
         apps share and which comes with the first app that needs it.",
    ))
    .into()
}

/// One app: the box, its name with what it is for under it, and how big it is at the right. The
/// whole row ticks it, the way a row of a list does in GNOME.
fn app_row<'a>(state: &'a Welcome, look: Colors, app: &'a App) -> Element<'a, Message> {
    let installed = is_installed(state, &app.id);
    let pending = is_pending(state, &app.id);
    let ticked = state.ticked.contains(&app.id);
    let can = can_tick(state, &app.id);
    let id = app.id.clone();
    let toggle: Option<Box<dyn Fn(bool) -> Message>> = can
        .then(|| Box::new(move |on| Message::Tick(id.clone(), on)) as Box<dyn Fn(bool) -> Message>);
    let mut left = column![line(look, &app.name), note(look, &app.about)].spacing(2);
    if app.account {
        left = left.push(note(look, "Needs an account"));
    }
    let right: Element<'a, Message> = if installed {
        note(look, "Installed")
    } else if pending {
        note(look, "Installing")
    } else if let Some(size) = state.catalog.as_ref().and_then(|catalog| catalog.size(app)) {
        note(look, size)
    } else {
        space().into()
    };
    let inside = row![
        crate::widgets::tick(look, ticked || installed || pending, toggle),
        left.width(Fill),
        right,
    ]
    .align_y(Center)
    .spacing(GAP);
    let mut pressable =
        button(inside)
            .width(Fill)
            .padding([8, 12])
            .style(move |_: &Theme, status| button::Style {
                background: Some(
                    match status {
                        button::Status::Hovered | button::Status::Pressed => look.hover,
                        _ => Color::TRANSPARENT,
                    }
                    .into(),
                ),
                text_color: look.text,
                border: Border {
                    radius: 4.0.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            });
    if can {
        pressable = pressable.on_press(Message::Tick(app.id.clone(), !ticked));
    }
    container(pressable).width(Fill).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use librift::flatpak::FLATHUB;

    fn listed() -> Vec<App> {
        librift::suggested::parse(
            "[[app]]\nid = \"org.videolan.VLC\"\nname = \"VLC\"\nabout = \"Video\"\ngroup = \"Media\"\n\n\
             [[app]]\nid = \"org.gimp.GIMP\"\nname = \"GIMP\"\nabout = \"Pictures\"\ngroup = \"Creative\"\n\n\
             [[app]]\nid = \"dev.rift.TestApp\"\nname = \"Rift test app\"\nabout = \"The test's\"\n\
             group = \"Testing\"\nremote = \"rift-test\"\n",
        )
        .unwrap()
    }

    fn answered(remotes: &[&str], installed: &[&str]) -> Catalog {
        Catalog {
            remotes: Ok(remotes.iter().map(|name| (*name).to_string()).collect()),
            sizes: vec![(
                FLATHUB.to_string(),
                Ok(vec![(
                    "org.videolan.VLC".to_string(),
                    "139.4 MB".to_string(),
                )]),
            )],
            installed: installed.iter().map(|id| (*id).to_string()).collect(),
        }
    }

    #[test]
    fn an_app_from_the_tests_remote_is_listed_once_the_remote_is_there() {
        let mut state = Welcome::bare();
        state.apps = Ok(listed());
        state.catalog = Some(Box::new(answered(&["flathub"], &[])));
        let ids = |state: &Welcome| -> Vec<String> {
            offered(state).iter().map(|app| app.id.clone()).collect()
        };
        assert_eq!(ids(&state), ["org.videolan.VLC", "org.gimp.GIMP"]);
        state.catalog = Some(Box::new(answered(&["flathub", "rift-test"], &[])));
        assert_eq!(
            ids(&state),
            ["org.videolan.VLC", "org.gimp.GIMP", "dev.rift.TestApp"]
        );
        let printed = self::state(&state);
        assert!(printed.contains(&"remotes flathub,rift-test".to_string()));
        assert!(printed.contains(&"sizes flathub 1".to_string()));
        assert!(printed.contains(&"app org.videolan.VLC 139.4 MB".to_string()));
        assert!(printed.contains(&"app dev.rift.TestApp unknown".to_string()));
        assert!(printed.contains(&"ticked none".to_string()));
    }

    #[test]
    fn only_what_can_be_installed_is_ticked() {
        let mut state = Welcome::bare();
        state.apps = Ok(listed());
        state.catalog = Some(Box::new(answered(&["flathub"], &["org.gimp.GIMP"])));
        tick(&mut state, "org.videolan.VLC", true);
        // installed already, not on the list here, and not on the list at all
        tick(&mut state, "org.gimp.GIMP", true);
        tick(&mut state, "dev.rift.TestApp", true);
        tick(&mut state, "org.example.Nothing", true);
        assert_eq!(state.ticked, ["org.videolan.VLC"]);
        assert!(can_install(&state));
        tick(&mut state, "org.videolan.VLC", false);
        assert!(state.ticked.is_empty() && !can_install(&state));
        // with no network nothing is
        state.online = false;
        tick(&mut state, "org.videolan.VLC", true);
        assert!(state.ticked.is_empty());
        let printed = self::state(&state);
        assert!(printed.contains(&"app org.gimp.GIMP installed".to_string()));
    }

    #[test]
    fn a_finished_install_is_installed_and_no_longer_ticked() {
        let mut state = Welcome::bare();
        state.apps = Ok(listed());
        state.catalog = Some(Box::new(answered(&["flathub"], &[])));
        state.ticked = vec!["org.videolan.VLC".to_string()];
        installed(&mut state, "org.videolan.VLC");
        assert!(state.ticked.is_empty());
        assert!(is_installed(&state, "org.videolan.VLC"));
        assert!(!can_tick(&state, "org.videolan.VLC"));
    }

    #[test]
    fn a_problem_is_what_flatpak_said() {
        let catalog = Catalog {
            remotes: Ok(vec![FLATHUB.to_string()]),
            sizes: vec![(
                FLATHUB.to_string(),
                Err("Unable to load summary from remote flathub.".to_string()),
            )],
            installed: Vec::new(),
        };
        assert_eq!(
            catalog.problem(),
            Some("Unable to load summary from remote flathub.")
        );
        assert_eq!(
            Catalog::failed("No flatpak.").problem(),
            Some("No flatpak.")
        );
        assert_eq!(answered(&["flathub"], &[]).problem(), None);
    }
}
