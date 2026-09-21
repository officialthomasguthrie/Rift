//! What Horizon has open: the windows and the workspaces the dock draws and the keyboard layouts
//! the bar names, read from the compositor's event stream, and the actions a click on the dock or
//! the bar sends back. The stream is read on a thread of its own, like the clock's, and every event
//! turns into a new picture of what is open.

// the dock is what draws this, and the dock is linux only
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::collections::HashMap;
use std::io;
use std::time::Duration;

use niri_ipc::socket::Socket;
use niri_ipc::{
    Action, Event, KeyboardLayouts, LayoutSwitchTarget, Reply, Request, Response, Window,
    Workspace, WorkspaceReferenceArg,
};

use crate::launcher::App;

/// How long to wait before looking for the compositor's socket again. Lens is started with the
/// session, so the socket is normally there before the first try.
const RETRY: Duration = Duration::from_secs(2);

/// One open window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Win {
    /// The compositor's id for it, which the actions name.
    pub id: u64,
    /// The app id it carries, empty when it has none.
    pub app_id: String,
    /// Its title, empty when it has none.
    pub title: String,
    /// Whether it has the keyboard focus.
    pub focused: bool,
}

/// One workspace, numbered the way Horizon numbers it on its output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Space {
    /// Its place on its output, counting from one, which is what a click names.
    pub idx: u8,
    /// Whether it is the one on screen.
    pub active: bool,
}

/// What Horizon has open now: the windows in the order they opened, the workspaces of the screen
/// the owner is looking at, and the keyboard layouts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Open {
    /// Every window, oldest first, so the dock walks an app's windows in the same order twice.
    pub windows: Vec<Win>,
    /// The workspaces of the focused output, in their order.
    pub spaces: Vec<Space>,
    /// The keyboard layouts by the names Horizon gives them, "English (UK)", in their order.
    pub layouts: Vec<String>,
    /// The place in that list of the one in use.
    pub layout: usize,
}

impl Open {
    /// The windows of one app, in the order they opened.
    pub fn of<'a>(&'a self, app_id: &'a str) -> impl Iterator<Item = &'a Win> {
        self.windows.iter().filter(move |win| win.app_id == app_id)
    }
}

/// The event stream, applied to a picture of what is open. Only the events the dock draws from
/// are read; the rest go by. Nothing here panics on an event that does not fit, because this runs
/// on a thread of the shell and the workspace aborts on a panic.
#[derive(Debug, Default)]
struct Tracked {
    windows: HashMap<u64, Window>,
    spaces: HashMap<u64, Workspace>,
    layouts: Option<KeyboardLayouts>,
}

impl Tracked {
    /// Take one event in. `false` when it was not one the dock or the bar draws from.
    fn apply(&mut self, event: Event) -> bool {
        match event {
            Event::WindowsChanged { windows } => {
                self.windows = windows.into_iter().map(|win| (win.id, win)).collect();
            }
            Event::WindowOpenedOrChanged { window } => {
                let (id, focused) = (window.id, window.is_focused);
                self.windows.insert(id, window);
                if focused {
                    self.focus(Some(id));
                }
            }
            Event::WindowClosed { id } => {
                self.windows.remove(&id);
            }
            Event::WindowFocusChanged { id } => self.focus(id),
            Event::WorkspacesChanged { workspaces } => {
                self.spaces = workspaces
                    .into_iter()
                    .map(|space| (space.id, space))
                    .collect();
            }
            Event::WorkspaceActivated { id, focused } => self.activate(id, focused),
            Event::KeyboardLayoutsChanged { keyboard_layouts } => {
                self.layouts = Some(keyboard_layouts);
            }
            Event::KeyboardLayoutSwitched { idx } => {
                if let Some(layouts) = self.layouts.as_mut() {
                    layouts.current_idx = idx;
                }
            }
            _ => return false,
        }
        true
    }

    fn focus(&mut self, id: Option<u64>) {
        for win in self.windows.values_mut() {
            win.is_focused = Some(win.id) == id;
        }
    }

    /// One workspace became the one on screen on its output, and with it the focused one.
    fn activate(&mut self, id: u64, focused: bool) {
        let Some(output) = self.spaces.get(&id).map(|space| space.output.clone()) else {
            return;
        };
        for space in self.spaces.values_mut() {
            if space.output == output {
                space.is_active = space.id == id;
            }
            if focused {
                space.is_focused = space.id == id;
            }
        }
    }

    /// What the dock and the bar draw: the windows oldest first, the workspaces of the screen the
    /// owner is looking at, which is the only screen the shell draws on, and the layouts.
    fn picture(&self) -> Open {
        let mut windows: Vec<&Window> = self.windows.values().collect();
        windows.sort_by_key(|win| win.id);
        let here = self
            .spaces
            .values()
            .find(|space| space.is_focused)
            .and_then(|space| space.output.clone());
        let mut spaces: Vec<&Workspace> = self
            .spaces
            .values()
            .filter(|space| here.is_none() || space.output == here)
            .collect();
        spaces.sort_by_key(|space| space.idx);
        Open {
            windows: windows
                .into_iter()
                .map(|win| Win {
                    id: win.id,
                    app_id: win.app_id.clone().unwrap_or_default(),
                    title: win.title.clone().unwrap_or_default(),
                    focused: win.is_focused,
                })
                .collect(),
            spaces: spaces
                .into_iter()
                .map(|space| Space {
                    idx: space.idx,
                    active: space.is_active,
                })
                .collect(),
            layouts: self
                .layouts
                .as_ref()
                .map(|layouts| layouts.names.clone())
                .unwrap_or_default(),
            layout: self
                .layouts
                .as_ref()
                .map_or(0, |layouts| usize::from(layouts.current_idx)),
        }
    }
}

/// Read Horizon's event stream and hand `each` a new picture after every event it knows. Blocks
/// forever, so the shell calls it on a thread of its own; when `each` says the shell is gone it
/// returns. A compositor that is not there yet, or that went away, is waited for.
pub fn watch<F: Fn(&Open) -> bool>(each: F) {
    loop {
        match stream(&each) {
            Ok(()) => return,
            Err(why) => eprintln!("lens: horizon's event stream: {why}"),
        }
        // the compositor went away or was never there. say that nothing is open, so the dock does
        // not keep drawing marks for windows that are gone
        if !each(&Open::default()) {
            return;
        }
        std::thread::sleep(RETRY);
    }
}

/// One connection's worth of events. `Ok(())` when the shell is gone and nothing else is wanted.
fn stream<F: Fn(&Open) -> bool>(each: &F) -> io::Result<()> {
    let mut socket = Socket::connect()?;
    match socket.send(Request::EventStream)? {
        Ok(Response::Handled) => {}
        other => {
            return Err(io::Error::other(format!(
                "Horizon refused the event stream: {other:?}"
            )));
        }
    }
    let mut tracked = Tracked::default();
    let mut read = socket.read_events();
    loop {
        let event = read()?;
        if tracked.apply(event) && !each(&tracked.picture()) {
            return Ok(());
        }
    }
}

/// Give this window the keyboard.
///
/// # Errors
///
/// When the compositor is not there or refuses.
pub fn focus(window: u64) -> Result<(), String> {
    act(Action::FocusWindow { id: window })
}

/// Close this window.
///
/// # Errors
///
/// When the compositor is not there or refuses.
pub fn close(window: u64) -> Result<(), String> {
    act(Action::CloseWindow { id: Some(window) })
}

/// Go to this workspace.
///
/// # Errors
///
/// When the compositor is not there or refuses.
pub fn activate(space: u8) -> Result<(), String> {
    act(Action::FocusWorkspace {
        reference: WorkspaceReferenceArg::Index(space),
    })
}

/// Switch to the next keyboard layout, what Mod+Shift+Space does.
///
/// # Errors
///
/// When the compositor is not there or refuses.
pub fn next_layout() -> Result<(), String> {
    act(Action::SwitchLayout {
        layout: LayoutSwitchTarget::Next,
    })
}

/// One action over a connection of its own. The socket is in the session's runtime directory and
/// the answer comes back at once, so this is a round trip of a few microseconds.
fn act(action: Action) -> Result<(), String> {
    let done: io::Result<Reply> =
        Socket::connect().and_then(|mut socket| socket.send(Request::Action(action)));
    match done {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(why)) => Err(why),
        Err(why) => Err(format!("Horizon did not answer: {why}")),
    }
}

/// Whether a window with this app id belongs to this app. An app names its windows after its
/// entry, or after the class the entry says it uses, or after the program; a terminal app is
/// named after the class Lens starts it with.
#[must_use]
pub fn belongs(app: &App, app_id: &str) -> bool {
    if app_id.is_empty() {
        return false;
    }
    let same = |other: &str| other.eq_ignore_ascii_case(app_id);
    let tail = app.id.rsplit('.').next().unwrap_or(&app.id);
    let program = app
        .exec
        .first()
        .and_then(|word| word.rsplit('/').next())
        .unwrap_or_default();
    same(&app.id)
        || app.wm_class.as_deref().is_some_and(same)
        || same(tail)
        || same(program)
        || (app.terminal && same(&app.class()))
}

/// The app a window belongs to, when one of them does.
#[must_use]
pub fn owner<'a>(apps: &'a [App], app_id: &str) -> Option<&'a App> {
    apps.iter().find(|app| belongs(app, app_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use librift::apps::Category;

    fn app(id: &str, program: &str) -> App {
        App {
            id: id.to_string(),
            name: id.to_string(),
            exec: vec![program.to_string()],
            terminal: false,
            icon: None,
            wm_class: None,
            category: Category::Accessories,
        }
    }

    #[test]
    fn a_window_is_matched_to_the_entry_it_came_from() {
        let firefox = app("firefox", "/run/current-system/sw/bin/firefox");
        assert!(belongs(&firefox, "firefox"));
        assert!(belongs(&firefox, "Firefox"), "the case is not the app");
        assert!(!belongs(&firefox, "com.mitchellh.ghostty"));
        // a window with no app id at all belongs to nobody
        assert!(!belongs(&firefox, ""));

        // an entry named the way flatpak names them, and a window named after the program
        let ghostty = app("com.mitchellh.ghostty", "ghostty");
        assert!(belongs(&ghostty, "com.mitchellh.ghostty"));
        assert!(belongs(&ghostty, "ghostty"));

        // and one that says what its windows are called
        let mut code = app("code", "code");
        code.wm_class = Some("Code".to_string());
        assert!(belongs(&code, "Code"));
        assert!(belongs(&code, "code"));
    }

    #[test]
    fn a_terminal_app_is_matched_to_the_class_it_was_started_with() {
        let mut helix = app("Helix", "hx");
        helix.terminal = true;
        assert!(belongs(&helix, "dev.rift.Helix"));
        assert!(belongs(&helix, "Helix"));
        // the terminal's own windows are the terminal's
        assert!(!belongs(&helix, "com.mitchellh.ghostty"));
    }

    #[test]
    fn the_first_entry_that_fits_owns_the_window() {
        let apps = vec![app("firefox", "firefox"), app("btop", "btop")];
        assert_eq!(
            owner(&apps, "btop").map(|app| app.id.as_str()),
            Some("btop")
        );
        assert!(owner(&apps, "org.gnome.Nautilus").is_none());
    }

    fn window(id: u64, app_id: &str, focused: bool) -> Window {
        Window {
            id,
            title: Some(format!("window {id}")),
            app_id: Some(app_id.to_string()),
            pid: None,
            workspace_id: Some(1),
            is_focused: focused,
            is_floating: false,
            is_urgent: false,
            layout: niri_ipc::WindowLayout {
                pos_in_scrolling_layout: None,
                tile_size: (0.0, 0.0),
                window_size: (0, 0),
                tile_pos_in_workspace_view: None,
                window_offset_in_tile: (0.0, 0.0),
            },
            focus_timestamp: None,
        }
    }

    fn space(id: u64, idx: u8, output: &str, active: bool) -> Workspace {
        Workspace {
            id,
            idx,
            name: None,
            output: Some(output.to_string()),
            is_urgent: false,
            is_active: active,
            is_focused: active,
            active_window_id: None,
        }
    }

    #[test]
    fn the_event_stream_builds_the_picture_the_dock_draws() {
        let mut tracked = Tracked::default();
        assert!(tracked.apply(Event::WindowsChanged {
            windows: vec![window(2, "firefox", false), window(1, "ghostty", true)],
        }));
        let open = tracked.picture();
        assert_eq!(
            open.windows.iter().map(|win| win.id).collect::<Vec<_>>(),
            [1, 2],
            "oldest first"
        );
        assert!(open.windows[0].focused);

        // a new window that took the focus takes it from the one that had it
        assert!(tracked.apply(Event::WindowOpenedOrChanged {
            window: window(3, "firefox", true),
        }));
        let open = tracked.picture();
        assert_eq!(open.of("firefox").count(), 2);
        assert_eq!(
            open.windows
                .iter()
                .filter(|win| win.focused)
                .map(|win| win.id)
                .collect::<Vec<_>>(),
            [3]
        );

        assert!(tracked.apply(Event::WindowClosed { id: 3 }));
        assert!(tracked.apply(Event::WindowFocusChanged { id: Some(2) }));
        let open = tracked.picture();
        assert_eq!(open.windows.len(), 2);
        assert!(open.windows[1].focused);

        // an event the dock does not draw from is not one of its own
        assert!(!tracked.apply(Event::OverviewOpenedOrClosed { is_open: true }));
    }

    #[test]
    fn the_layouts_follow_what_horizon_says() {
        let mut tracked = Tracked::default();
        assert!(tracked.picture().layouts.is_empty());
        assert!(tracked.apply(Event::KeyboardLayoutsChanged {
            keyboard_layouts: KeyboardLayouts {
                names: vec!["English (US)".to_string(), "English (UK)".to_string()],
                current_idx: 0,
            },
        }));
        assert!(tracked.apply(Event::KeyboardLayoutSwitched { idx: 1 }));
        let open = tracked.picture();
        assert_eq!(open.layouts, ["English (US)", "English (UK)"]);
        assert_eq!(open.layout, 1);
    }

    #[test]
    fn the_workspaces_are_the_ones_of_the_screen_in_front_of_the_owner() {
        let mut tracked = Tracked::default();
        tracked.apply(Event::WorkspacesChanged {
            workspaces: vec![
                space(2, 2, "eDP-1", false),
                space(1, 1, "eDP-1", true),
                space(3, 1, "HDMI-1", false),
            ],
        });
        let open = tracked.picture();
        assert_eq!(
            open.spaces
                .iter()
                .map(|space| (space.idx, space.active))
                .collect::<Vec<_>>(),
            [(1, true), (2, false)]
        );

        // the second workspace of the same screen becomes the one on screen
        tracked.apply(Event::WorkspaceActivated {
            id: 2,
            focused: true,
        });
        let open = tracked.picture();
        assert_eq!(
            open.spaces
                .iter()
                .map(|space| (space.idx, space.active))
                .collect::<Vec<_>>(),
            [(1, false), (2, true)]
        );

        // and one that is not there changes nothing
        tracked.apply(Event::WorkspaceActivated {
            id: 9,
            focused: true,
        });
        assert_eq!(tracked.picture().spaces.len(), 2);
    }
}
