//! The owner's session: what was open in it, and locking it.
//!
//! The journal is the first half of teleport. The drive is unplugged from one machine and plugged
//! into another, and the session has to come back, so something has to have written down what was
//! open: for each window the app that opened it, the workspace and the screen it was on, and where
//! it stood in the scrolling layout. The shell writes it as windows open, move and close, `rift
//! session` prints it, and the file lives under the owner's state directory, on persist, because
//! it follows the drive and not the machine.
//!
//! At the first login after a boot the shell reads the journal back and opens those apps again,
//! each on the workspace it was on and in the column it stood in, unless the owner has turned that
//! off. That is the second half of teleport, and [`to_open`] is the whole of what it has to work
//! out: which windows can be opened again here, in what order, and which ones this machine has
//! nothing to open.
//!
//! An app id is not a desktop entry, so the one thing the compositor cannot say is what to start
//! again. Every app on Rift starts in a scope of its own: [`crate::apps::start`] names it
//! `app-rift-<entry>-<number>`, and Horizon names what a key starts `app-niri-<program>-<pid>`, so
//! the control group of the process that drew a window says which entry opened it. A window whose
//! scope says nothing falls back to the entry whose windows carry its app id, which is how the
//! dock has always found an app.
//!
//! Locking goes through logind, the way `loginctl lock-session` does: logind signals the session
//! and the lock screen that listens for it comes up, so the lock is the same whether a key, a
//! command or a menu asked for it.

use std::fmt::Write as _;
use std::fs;

use crate::appearance::{home, write_beside};
use crate::apps::{App, owner};

/// Where the journal of what was open lives, under home.
pub const JOURNAL: &str = ".local/state/rift/session";

/// Where the owner says whether the apps come back at the next login, under home. A drive that has
/// never been asked brings them back: teleport is the reason the drive is the shape it is.
pub const RESTORE: &str = ".config/rift/session-restore";

/// What the name of a systemd scope ends in.
const SCOPE: &str = ".scope";

/// The lines at the top of the journal, for whoever opens the file.
const HEADING: &str = "# The windows that were open, oldest first. The shell writes this down as\n\
                       # windows open, move and close, so a session comes back on another machine.\n";

/// One window the journal remembers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Window {
    /// The desktop entry that would open it again, by its id. Empty when nothing says which one.
    pub app: String,
    /// The app id the window carried, empty when it had none.
    pub window: String,
    /// Its title, empty when it had none.
    pub title: String,
    /// The workspace it was on, counting from one on its screen. 0 when it was on none.
    pub workspace: u8,
    /// The screen that workspace was on, empty when nothing says.
    pub screen: String,
    /// Its column in the scrolling layout, counting from one from the left.
    pub column: Option<usize>,
    /// Its place in that column, counting from one from the top.
    pub tile: Option<usize>,
    /// Whether it floated over the layout instead of standing in it.
    pub floating: bool,
}

/// One open window as the compositor describes it, which is everything the journal is built from
/// except the entry that opened it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seen {
    /// The app id it carries, empty when it has none.
    pub app_id: String,
    /// Its title, empty when it has none.
    pub title: String,
    /// The process that drew it, when the compositor knows which one.
    pub pid: Option<i32>,
    /// The compositor's id for the workspace it is on.
    pub space: Option<u64>,
    /// Whether it floats.
    pub floating: bool,
    /// Its column and its place in that column, both counting from one.
    pub place: Option<(usize, usize)>,
}

/// One workspace, as the compositor numbers it on its screen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Space {
    /// The compositor's id for it, which a window names. It means nothing after a restart.
    pub id: u64,
    /// Its place on its screen, counting from one.
    pub idx: u8,
    /// The screen it is on, empty when none is connected.
    pub screen: String,
}

/// What the name of the scope a window's process runs in says about what opened it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Started {
    /// A desktop entry by its id: the shell, the file manager, or anything else that goes through
    /// [`crate::apps::start`].
    Entry(String),
    /// A program by the name of its file: a key in the compositor started it.
    Program(String),
}

/// The journal of these windows, in the order the compositor gave them, with the entry of each one
/// worked out: the one its scope names when `started` can say, and otherwise the one whose windows
/// carry its app id.
#[must_use]
pub fn of(
    seen: &[Seen],
    spaces: &[Space],
    apps: &[App],
    started: impl Fn(i32) -> Option<Started>,
) -> Vec<Window> {
    seen.iter()
        .map(|win| {
            let space = win
                .space
                .and_then(|id| spaces.iter().find(|space| space.id == id));
            let place = if win.floating { None } else { win.place };
            Window {
                app: entry(apps, win.pid.and_then(&started).as_ref(), &win.app_id),
                window: win.app_id.clone(),
                title: win.title.clone(),
                workspace: space.map_or(0, |space| space.idx),
                screen: space.map_or_else(String::new, |space| space.screen.clone()),
                column: place.map(|at| at.0),
                tile: place.map(|at| at.1),
                floating: win.floating,
            }
        })
        .collect()
}

/// The desktop entry that opened a window. The scope is the exact answer when there is one, since
/// it names the entry the app was started from; a program name and an app id are both looked up
/// among the entries the way the dock looks one up. A Flatpak app puts itself in a scope of
/// Flatpak's own, so it comes out of the app id, which for a Flatpak app is the id of the entry it
/// exports anyway.
fn entry(apps: &[App], started: Option<&Started>, app_id: &str) -> String {
    match started {
        Some(Started::Entry(id)) => return id.clone(),
        Some(Started::Program(program)) => {
            if let Some(app) = owner(apps, program) {
                return app.id.clone();
            }
        }
        None => {}
    }
    owner(apps, app_id).map_or_else(String::new, |app| app.id.clone())
}

/// What a scope's name says opened the app in it, when it is a scope a Rift session makes.
#[must_use]
pub fn started_of_scope(unit: &str) -> Option<Started> {
    let (name, number) = unit.strip_suffix(SCOPE)?.rsplit_once('-')?;
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if let Some(id) = name.strip_prefix("app-rift-") {
        return unescaped(id).map(Started::Entry);
    }
    name.strip_prefix("app-niri-")
        .and_then(unescaped)
        .map(Started::Program)
}

/// A name with the escapes a unit name needs taken back out: `\x2d` is the dash a unit name cannot
/// hold. [`crate::apps::scope`] and Horizon's own spawn both write them.
fn unescaped(name: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(name.len());
    let mut rest = name;
    while let Some(at) = rest.find("\\x") {
        bytes.extend_from_slice(&rest.as_bytes()[..at]);
        bytes.push(u8::from_str_radix(rest.get(at + 2..at + 4)?, 16).ok()?);
        rest = &rest[at + 4..];
    }
    bytes.extend_from_slice(rest.as_bytes());
    String::from_utf8(bytes).ok()
}

/// The scope a process runs in, from the text of its `/proc/<pid>/cgroup`: the deepest part of the
/// control group path that is a scope, since an app can make control groups of its own inside it.
#[must_use]
pub fn scope_of_cgroup(cgroup: &str) -> Option<&str> {
    cgroup
        .lines()
        .find_map(|line| line.strip_prefix("0::"))?
        .rsplit('/')
        .find(|part| part.ends_with(SCOPE))
}

/// What opened the process with this id, read from its control group. `None` on a machine with no
/// `/proc`, for a process that has gone, and for one in no scope a session made.
#[must_use]
pub fn started_by(pid: i32) -> Option<Started> {
    let cgroup = fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    started_of_scope(scope_of_cgroup(&cgroup)?)
}

/// The text of the journal: a paragraph for each window, a line for each thing known about it,
/// and no line at all for what is not known. A window that says nothing whatever about itself is
/// left out, because there would be nothing to bring back.
#[must_use]
pub fn write(windows: &[Window]) -> String {
    let mut text = String::from(HEADING);
    for win in windows {
        let number = |value: Option<usize>| value.map_or_else(String::new, |at| at.to_string());
        let mut paragraph = String::new();
        for (key, value) in [
            ("app", win.app.clone()),
            ("window", win.window.clone()),
            ("title", win.title.replace('\n', " ")),
            (
                "workspace",
                number((win.workspace > 0).then_some(win.workspace.into())),
            ),
            ("screen", win.screen.clone()),
            ("column", number(win.column)),
            ("tile", number(win.tile)),
        ] {
            if !value.is_empty() {
                let _ = writeln!(paragraph, "{key} {value}");
            }
        }
        if win.floating {
            paragraph.push_str("floating\n");
        }
        if !paragraph.is_empty() {
            text.push('\n');
            text.push_str(&paragraph);
        }
    }
    text
}

/// The windows a journal names. A word it does not know is passed over, so an older journal and a
/// newer one both read.
#[must_use]
pub fn read(text: &str) -> Vec<Window> {
    let mut windows = Vec::new();
    let mut win: Option<Window> = None;
    for line in text.lines() {
        let line = line.trim_end();
        if line.starts_with('#') {
            continue;
        }
        if line.is_empty() {
            windows.extend(win.take());
            continue;
        }
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        if !matches!(
            key,
            "app" | "window" | "title" | "workspace" | "screen" | "column" | "tile" | "floating"
        ) {
            continue;
        }
        let known = win.get_or_insert_with(Window::default);
        match key {
            "app" => known.app = value.to_string(),
            "window" => known.window = value.to_string(),
            "title" => known.title = value.to_string(),
            "workspace" => known.workspace = value.parse().unwrap_or(0),
            "screen" => known.screen = value.to_string(),
            "column" => known.column = value.parse().ok(),
            "tile" => known.tile = value.parse().ok(),
            _ => known.floating = true,
        }
    }
    windows.extend(win);
    windows
}

/// What the shell wrote down, or `None` when there is no home or it has written nothing yet.
#[must_use]
pub fn kept() -> Option<Vec<Window>> {
    let text = fs::read_to_string(home()?.join(JOURNAL)).ok()?;
    Some(read(&text))
}

/// Write the journal under home, beside itself and then over the old one, so nothing ever reads
/// half of it. `Ok(false)` when it already said exactly this.
///
/// # Errors
///
/// A sentence when there is no home or the file could not be written.
pub fn keep(windows: &[Window]) -> Result<bool, String> {
    let path = home()
        .ok_or_else(|| "There is no home to write the session down in.".to_string())?
        .join(JOURNAL);
    write_beside(&path, &write(windows))
}

/// One window to open again. The order these come in is the order the apps have to be started in,
/// because a new window opens as a column of its own beside the one that has the keyboard, which is
/// how the columns come back in the order they were in without a word about columns being said to
/// the compositor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// The desktop entry to start, by its id.
    pub app: String,
    /// The workspace to start it on, counting from one. 0 when the journal named none.
    pub workspace: u8,
    /// Whether it stood in the column the window before it is in, rather than a column of its own.
    pub stack: bool,
    /// Whether it floated over the layout instead of standing in it.
    pub floating: bool,
}

/// A window the journal names that this machine has nothing to open again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Passed {
    /// The entry it came from is not installed here. A drive that has met another machine, or an
    /// app the owner has since removed.
    Entry(String),
    /// Nothing names it: it carried this app id and no entry here claims it. The drop-down terminal
    /// is one, since the compositor draws it and no entry starts it.
    Window(String),
}

impl Passed {
    /// One line for the owner, which is what the notification after a restore is made of.
    #[must_use]
    pub fn line(&self) -> String {
        match self {
            Self::Entry(id) => format!("{id} is not installed here"),
            Self::Window(app_id) => format!("nothing here opens {app_id}"),
        }
    }
}

/// The apps to open again, in the order to open them, and the windows this machine has nothing to
/// open. Workspaces come back in their own order, lowest first, and inside a workspace the columns
/// come back from left to right, with a window that shared a column marked so it can be put back
/// into it; a window that floated comes last on its workspace, since it takes no column and would
/// push the ones after it along.
#[must_use]
pub fn to_open(windows: &[Window], apps: &[App]) -> (Vec<Step>, Vec<Passed>) {
    let mut order: Vec<&Window> = windows.iter().collect();
    // a workspace the journal said nothing about goes last, on whichever one the session starts on
    order.sort_by_key(|win| {
        (
            if win.workspace == 0 {
                u8::MAX
            } else {
                win.workspace
            },
            win.floating,
            win.column.unwrap_or(usize::MAX),
            win.tile.unwrap_or(usize::MAX),
        )
    });
    let mut steps: Vec<Step> = Vec::new();
    let mut passed = Vec::new();
    let mut last: Option<&Window> = None;
    for win in order {
        let entry = if win.app.is_empty() {
            owner(apps, &win.window).map(|app| app.id.clone())
        } else {
            apps.iter()
                .find(|app| app.id == win.app)
                .map(|app| app.id.clone())
        };
        let Some(app) = entry else {
            if !win.app.is_empty() {
                passed.push(Passed::Entry(win.app.clone()));
            } else if !win.window.is_empty() {
                passed.push(Passed::Window(win.window.clone()));
            }
            continue;
        };
        let stack = !win.floating
            && win.column.is_some()
            && last.is_some_and(|before| {
                before.workspace == win.workspace && !before.floating && before.column == win.column
            });
        steps.push(Step {
            app,
            workspace: win.workspace,
            stack,
            floating: win.floating,
        });
        last = Some(win);
    }
    (steps, passed)
}

/// Whether the apps that were open come back at the next login. A drive nobody has asked brings
/// them back.
#[must_use]
pub fn restores() -> bool {
    home()
        .and_then(|home| fs::read_to_string(home.join(RESTORE)).ok())
        .is_none_or(|text| text.trim() != "off")
}

/// Keep the owner's answer about opening the apps again.
///
/// # Errors
///
/// A sentence when there is no home to keep it in, or the file cannot be written.
pub fn keep_restores(on: bool) -> Result<(), String> {
    let home = home().ok_or("There is no home folder to keep the session setting in.")?;
    write_beside(&home.join(RESTORE), if on { "on\n" } else { "off\n" }).map(|_| ())
}

/// Lock the session the owner is using. Asked from a process outside any session, like a user unit,
/// logind takes the owner's graphical session.
///
/// # Errors
///
/// A sentence when logind does not know the session or refuses.
#[cfg(feature = "bus")]
pub fn lock() -> Result<(), String> {
    use zbus::zvariant::OwnedObjectPath;

    use crate::bus;

    const SERVICE: &str = "org.freedesktop.login1";
    let failed = |e| bus::sentence_for("logind", e);
    let connection = bus::connect(bus::PROPERTY_TIMEOUT)?;
    let session: OwnedObjectPath = bus::object(
        &connection,
        SERVICE,
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
    .and_then(|manager| manager.call("GetSession", &("auto",)))
    .map_err(failed)?;
    bus::object(
        &connection,
        SERVICE,
        session.as_str(),
        "org.freedesktop.login1.Session",
    )
    .and_then(|proxy| proxy.call::<_, _, ()>("Lock", &()))
    .map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::Category;

    fn app(id: &str, program: &str) -> App {
        App {
            id: id.to_string(),
            name: id.to_string(),
            exec: vec![program.to_string()],
            terminal: false,
            icon: None,
            wm_class: None,
            category: Category::Accessories,
            types: Vec::new(),
            line: String::new(),
        }
    }

    #[test]
    fn a_scope_says_which_entry_opened_an_app() {
        assert_eq!(
            started_of_scope("app-rift-org.gnome.Nautilus-1758780000000000000.scope"),
            Some(Started::Entry("org.gnome.Nautilus".to_string()))
        );
        // the dash the id had, which a unit name cannot hold, comes back out of its escape
        assert_eq!(
            started_of_scope("app-rift-virt\\x2dmanager-17.scope"),
            Some(Started::Entry("virt-manager".to_string()))
        );
        assert_eq!(
            started_of_scope("app-rift-my\\x20app-17.scope"),
            Some(Started::Entry("my app".to_string()))
        );
        // what a key in the compositor started is named after the program, not the entry
        assert_eq!(
            started_of_scope("app-niri-ghostty-4711.scope"),
            Some(Started::Program("ghostty".to_string()))
        );
        // and nothing else is a scope this reads
        for unit in [
            "app-flatpak-org.gnome.Loupe-4711.scope",
            "session-2.scope",
            "app-rift-org.gnome.Nautilus.scope",
            "app-rift-org.gnome.Nautilus-.scope",
            "app-rift-org.gnome.Nautilus-12a.scope",
            "app-rift-org.gnome.Nautilus-12.service",
        ] {
            assert_eq!(started_of_scope(unit), None, "{unit}");
        }
    }

    #[test]
    fn the_scope_is_the_deepest_one_in_the_control_group() {
        let cgroup = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/\
                      app-rift-com.mitchellh.ghostty-1758780000000000000.scope\n";
        assert_eq!(
            scope_of_cgroup(cgroup),
            Some("app-rift-com.mitchellh.ghostty-1758780000000000000.scope")
        );
        // an app with control groups of its own inside the scope is still in that scope
        assert_eq!(
            scope_of_cgroup("0::/user.slice/app.slice/app-niri-ghostty-9.scope/tab\n"),
            Some("app-niri-ghostty-9.scope")
        );
        assert_eq!(scope_of_cgroup("0::/user.slice/user-1000.slice\n"), None);
        assert_eq!(scope_of_cgroup(""), None);
    }

    #[test]
    fn a_window_gets_the_entry_its_scope_names_and_falls_back_to_its_app_id() {
        let apps = [
            app("com.mitchellh.ghostty", "ghostty"),
            app("org.gnome.Nautilus", "nautilus"),
            app("dev.rift.Settings", "rift-settings"),
        ];
        let spaces = [
            Space {
                id: 7,
                idx: 1,
                screen: "eDP-1".to_string(),
            },
            Space {
                id: 8,
                idx: 2,
                screen: "eDP-1".to_string(),
            },
        ];
        let seen = [
            // the shell started it, so the scope names the entry
            Seen {
                app_id: "nautilus".to_string(),
                title: "Home".to_string(),
                pid: Some(1),
                space: Some(7),
                floating: false,
                place: Some((2, 1)),
            },
            // a key started it, so the scope names the program and the entry comes from that
            Seen {
                app_id: "com.mitchellh.ghostty".to_string(),
                title: "fish".to_string(),
                pid: Some(2),
                space: Some(8),
                floating: true,
                place: Some((1, 1)),
            },
            // nothing is in a scope, so the app id is all there is
            Seen {
                app_id: "dev.rift.Settings".to_string(),
                title: "Settings".to_string(),
                pid: None,
                space: Some(7),
                floating: false,
                place: Some((1, 2)),
            },
        ];
        let started = |pid: i32| match pid {
            1 => started_of_scope("app-rift-org.gnome.Nautilus-17.scope"),
            2 => started_of_scope("app-niri-ghostty-4711.scope"),
            _ => None,
        };
        let windows = of(&seen, &spaces, &apps, started);
        assert_eq!(
            windows
                .iter()
                .map(|win| win.app.as_str())
                .collect::<Vec<_>>(),
            [
                "org.gnome.Nautilus",
                "com.mitchellh.ghostty",
                "dev.rift.Settings"
            ]
        );
        assert_eq!(windows[0].workspace, 1);
        assert_eq!(windows[0].screen, "eDP-1");
        assert_eq!((windows[0].column, windows[0].tile), (Some(2), Some(1)));
        // a window that floats stands nowhere in the layout, whatever the compositor last said
        assert!(windows[1].floating);
        assert_eq!((windows[1].column, windows[1].tile), (None, None));
        assert_eq!(windows[1].workspace, 2);
        assert_eq!((windows[2].column, windows[2].tile), (Some(1), Some(2)));

        // a window on a workspace nobody knows, of an app nobody knows
        let lost = of(
            &[Seen {
                app_id: "xterm".to_string(),
                space: Some(99),
                ..Seen::default()
            }],
            &spaces,
            &apps,
            |_| None,
        );
        assert_eq!(lost[0].app, "");
        assert_eq!(lost[0].workspace, 0);
        assert_eq!(lost[0].screen, "");
    }

    #[test]
    fn the_journal_reads_back_exactly_what_was_written() {
        let windows = vec![
            Window {
                app: "org.gnome.Nautilus".to_string(),
                window: "nautilus".to_string(),
                title: "Home".to_string(),
                workspace: 1,
                screen: "eDP-1".to_string(),
                column: Some(2),
                tile: Some(1),
                floating: false,
            },
            Window {
                app: "com.mitchellh.ghostty".to_string(),
                window: "com.mitchellh.ghostty".to_string(),
                title: String::new(),
                workspace: 2,
                screen: "HDMI-1".to_string(),
                column: None,
                tile: None,
                floating: true,
            },
        ];
        let text = write(&windows);
        assert!(text.starts_with('#'));
        assert!(text.contains(
            "\napp org.gnome.Nautilus\nwindow nautilus\ntitle Home\n\
                               workspace 1\nscreen eDP-1\ncolumn 2\ntile 1\n"
        ));
        assert!(text.contains("\nfloating\n"));
        // what is not known has no line of its own
        assert!(!text.contains("title \n"));
        assert_eq!(read(&text), windows);
        assert_eq!(read(""), []);
        assert_eq!(read(HEADING), []);
        // and a window that says nothing at all about itself is not written down, because there
        // would be nothing to bring back
        assert_eq!(write(&[Window::default()]), HEADING);
    }

    #[test]
    fn a_journal_from_another_version_still_reads() {
        // a word this version does not know is passed over, and a paragraph of nothing else is
        // no window
        let windows = read(
            "# a journal\n\nfrom 1\napp org.gnome.Nautilus\nworkspace 3\n\nfrom 2\n\n\
             app com.mitchellh.ghostty\nfloating\ncolumn nine\n",
        );
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].app, "org.gnome.Nautilus");
        assert_eq!(windows[0].workspace, 3);
        assert_eq!(windows[1].app, "com.mitchellh.ghostty");
        assert!(windows[1].floating);
        assert_eq!(windows[1].column, None);
    }

    #[test]
    fn the_apps_come_back_workspace_by_workspace_and_column_by_column() {
        let apps = [
            app("com.mitchellh.ghostty", "ghostty"),
            app("org.gnome.Nautilus", "nautilus"),
            app("org.gnome.Calculator", "gnome-calculator"),
        ];
        let win = |entry: &str, workspace, column, tile| Window {
            app: entry.to_string(),
            workspace,
            column: Some(column),
            tile: Some(tile),
            ..Window::default()
        };
        // out of order on purpose: the second workspace before the first, and the third column
        // before the first two, one of which held two windows
        let windows = [
            win("org.gnome.Calculator", 2, 1, 1),
            win("com.mitchellh.ghostty", 1, 2, 1),
            win("org.gnome.Nautilus", 1, 1, 1),
            win("com.mitchellh.ghostty", 1, 1, 2),
            Window {
                app: "org.gnome.Nautilus".to_string(),
                workspace: 1,
                floating: true,
                ..Window::default()
            },
        ];
        let (steps, passed) = to_open(&windows, &apps);
        assert!(passed.is_empty());
        assert_eq!(
            steps
                .iter()
                .map(|step| (step.app.as_str(), step.workspace, step.stack, step.floating))
                .collect::<Vec<_>>(),
            [
                // the first workspace, left to right, the second window of the first column put
                // back into it, and the one that floated last of all
                ("org.gnome.Nautilus", 1, false, false),
                ("com.mitchellh.ghostty", 1, true, false),
                ("com.mitchellh.ghostty", 1, false, false),
                ("org.gnome.Nautilus", 1, false, true),
                ("org.gnome.Calculator", 2, false, false),
            ]
        );
    }

    #[test]
    fn a_window_this_machine_cannot_open_is_passed_over_and_said_so() {
        let apps = [app("com.mitchellh.ghostty", "ghostty")];
        let windows = [
            Window {
                app: "org.gnome.Loupe".to_string(),
                window: "org.gnome.Loupe".to_string(),
                workspace: 1,
                column: Some(1),
                tile: Some(1),
                ..Window::default()
            },
            // nothing named it when it was written down, and nothing here names it either
            Window {
                window: "dev.rift.Console".to_string(),
                workspace: 1,
                column: Some(2),
                tile: Some(1),
                ..Window::default()
            },
            // and one whose entry was not known there but is an app here
            Window {
                window: "ghostty".to_string(),
                workspace: 1,
                column: Some(3),
                tile: Some(1),
                ..Window::default()
            },
        ];
        let (steps, passed) = to_open(&windows, &apps);
        assert_eq!(
            steps
                .iter()
                .map(|step| step.app.as_str())
                .collect::<Vec<_>>(),
            ["com.mitchellh.ghostty"]
        );
        assert_eq!(
            passed,
            [
                Passed::Entry("org.gnome.Loupe".to_string()),
                Passed::Window("dev.rift.Console".to_string())
            ]
        );
        assert_eq!(passed[0].line(), "org.gnome.Loupe is not installed here");
        assert_eq!(passed[1].line(), "nothing here opens dev.rift.Console");
        // and a journal of nothing asks for nothing
        assert_eq!(to_open(&[], &apps), (Vec::new(), Vec::new()));
    }

    #[test]
    fn a_title_never_runs_over_more_than_its_own_line() {
        let windows = vec![Window {
            app: "org.gnome.TextEditor".to_string(),
            title: "two\nlines".to_string(),
            ..Window::default()
        }];
        let text = write(&windows);
        assert!(text.contains("title two lines\n"));
        assert_eq!(read(&text)[0].title, "two lines");
    }
}
