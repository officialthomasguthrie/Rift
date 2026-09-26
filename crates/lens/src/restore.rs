//! Opening again what was open, which is the other half of teleport. The shell writes down what is
//! open as it changes; at the first login after a boot it reads that back and starts those apps
//! again, each on the workspace it was on and in the column it stood in.
//!
//! The compositor has no action that says "open this app on workspace 2 in column 3", and it does
//! not need one: a new window opens as a column of its own beside the one that has the keyboard, so
//! starting the apps one at a time, workspace by workspace and left to right, puts the columns back
//! in the order they were in. Each window is waited for before the next app is started, and the one
//! that arrived is given the keyboard, so nothing the owner does in the meantime moves the next one
//! somewhere else. A window that shared a column is put back into it, and one that floated is let
//! go of the layout after the ones that stood in it.
//!
//! It runs once a login. A crash of the shell is not a login: the shell leaves a note in the
//! session's runtime directory, which a boot clears, and a shell that finds the note brings nothing
//! back. Nothing comes back when the owner has turned it off on the Owner page, and nothing is
//! written down or brought back in Ghost mode at all, where home is a tmpfs and persist stays
//! locked.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use librift::apps::{self, App};
use librift::session::{self, Passed, Step};

use crate::horizon::{Open, Win};

/// How long one window is waited for before the next app is started. An app that draws nothing in
/// that time is given up on: it may be one that keeps its own windows, like a browser, and the rest
/// of the session should not wait on it. Long enough for the slowest app in the image to draw on the
/// slowest machine it runs on, since giving up early puts the windows after it in the wrong columns.
pub const PATIENCE: Duration = Duration::from_secs(90);

/// The note in the session's runtime directory that says this login has already had its session
/// back. The runtime directory is made at login and gone at the end of it, so a boot clears it and
/// a shell that starts again inside one login does not.
const NOTE: &str = "lens-restored";

/// What is left of bringing the last session back.
#[derive(Debug)]
pub struct Restore {
    /// The apps still to open, the next one last, so they come off the end.
    left: Vec<Step>,
    /// The app whose window is being waited for.
    waiting: Option<Waiting>,
    /// The windows the journal named that this machine has nothing to open.
    passed: Vec<Passed>,
    /// How many windows have come back.
    back: usize,
    /// Counts the apps started, so only the last one's patience gives up on it.
    turn: u64,
    /// Every window open at the last picture, which is what says a window is new.
    ids: Vec<u64>,
    /// The workspace the last app was started on, 0 before the first one.
    here: u8,
    /// The workspace the oldest window was on, which the screen goes back to at the end.
    first: u8,
}

/// The app a window is being waited for from.
#[derive(Debug)]
struct Waiting {
    /// What was started.
    app: App,
    /// What the journal said about where its window goes.
    step: Step,
    /// Which start this was.
    turn: u64,
    /// When the wait began.
    since: Instant,
}

/// What the shell has to do next.
#[derive(Debug, PartialEq, Eq)]
pub enum Next {
    /// Nothing: the window that was waited for has not arrived yet.
    Wait,
    /// Start the next app and wait for its window until this many seconds have gone by.
    Started(u64),
    /// It is over. These are the windows nothing here could open again.
    Done(Vec<Passed>),
}

impl Restore {
    /// How many windows have come back, for `lens --state`.
    #[must_use]
    pub const fn back(&self) -> usize {
        self.back
    }

    /// How many windows nothing here could open again.
    #[must_use]
    pub fn passed(&self) -> usize {
        self.passed.len()
    }
}

/// The session to bring back, or `None` when there is nothing to bring back: this login has had its
/// session back already, the owner turned it off, the journal names no window, or nothing in it can
/// be opened here. The note that says this login has had its session is left before any of that is
/// decided, so it is the shell that starts first that brings a session back and no other.
#[must_use]
pub fn begin(apps: &[App]) -> Option<Restore> {
    let note = note()?;
    if note.exists() {
        return None;
    }
    // a shell that started again beside the windows of the session it is in would open every one of
    // them a second time, so the note comes before the switch and before the journal is read at all
    if let Err(why) = std::fs::write(&note, "") {
        eprintln!("lens: the note that this login has had its session back: {why}");
        return None;
    }
    if !session::restores() {
        return None;
    }
    let windows = session::kept()?;
    let (mut steps, passed) = session::to_open(&windows, apps);
    if steps.is_empty() && passed.is_empty() {
        return None;
    }
    let first = steps.first().map_or(0, |step| step.workspace);
    steps.reverse();
    Some(Restore {
        left: steps,
        waiting: None,
        passed,
        back: 0,
        turn: 0,
        ids: Vec::new(),
        here: 0,
        first,
    })
}

/// Where the note that this login has had its session back lives.
fn note() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|dir| PathBuf::from(dir).join(NOTE))
}

/// What the compositor has open now. The window that was waited for is put where the journal said
/// it stood, and the next app is started; a window that has not come by the patience is given up on.
pub fn saw(restore: &mut Restore, open: &Open, apps: &[App]) -> Next {
    let waiting = restore.waiting.take();
    let found = waiting
        .as_ref()
        .and_then(|waiting| arrived(waiting, open, &restore.ids));
    restore.ids = open.windows.iter().map(|win| win.id).collect();
    match (waiting, found) {
        (Some(waiting), Some(win)) => {
            place(&waiting.step, win, open);
            restore.back += 1;
        }
        (Some(waiting), None) if waiting.since.elapsed() < PATIENCE => {
            restore.waiting = Some(waiting);
            return Next::Wait;
        }
        (Some(waiting), None) => gave_up(restore, &waiting),
        (None, _) => {}
    }
    start(restore, apps)
}

/// The patience for one app ran out, with no picture of what is open since. The window may still
/// arrive, so the app is not started again, but the session carries on without it.
pub fn waited(restore: &mut Restore, turn: u64, apps: &[App]) -> Next {
    match restore.waiting.take() {
        Some(waiting) if waiting.turn == turn => {
            gave_up(restore, &waiting);
            start(restore, apps)
        }
        other => {
            restore.waiting = other;
            Next::Wait
        }
    }
}

/// An app that was started and drew nothing. It may be one that keeps its own windows, or a second
/// start of one that will only ever draw once, and the rest of the session should not wait on it.
fn gave_up(restore: &mut Restore, waiting: &Waiting) {
    eprintln!(
        "lens: {} opened no window, so the rest of the session comes back without it",
        waiting.app.name
    );
    restore
        .passed
        .push(Passed::Silent(waiting.app.name.clone()));
}

/// Start the next app that can be started, and say how long to wait for its window. The screen goes
/// to a workspace before an app is started on it, so the window opens there rather than being moved
/// afterwards.
fn start(restore: &mut Restore, apps: &[App]) -> Next {
    while let Some(step) = restore.left.pop() {
        let Some(app) = apps.iter().find(|app| app.id == step.app) else {
            restore.passed.push(Passed::Entry(step.app.clone()));
            continue;
        };
        if step.workspace > 0 && step.workspace != restore.here {
            if let Err(why) = crate::horizon::activate(step.workspace) {
                eprintln!("lens: workspace {}: {why}", step.workspace);
            }
            restore.here = step.workspace;
        }
        if let Err(why) = apps::launch(app, &[]) {
            eprintln!("lens: {} did not start: {why}", app.name);
            restore.passed.push(Passed::Silent(app.name.clone()));
            continue;
        }
        restore.turn += 1;
        let turn = restore.turn;
        restore.waiting = Some(Waiting {
            app: app.clone(),
            step,
            turn,
            since: Instant::now(),
        });
        return Next::Started(turn);
    }
    // the screen goes back to where the session started, so it is not left on the last workspace
    // anything came back on
    if restore.first > 0 {
        if let Err(why) = crate::horizon::activate(restore.first) {
            eprintln!("lens: workspace {}: {why}", restore.first);
        }
    }
    Next::Done(std::mem::take(&mut restore.passed))
}

/// The window this app opened, when it is one that was not there before the app was started.
fn arrived<'a>(waiting: &Waiting, open: &'a Open, before: &[u64]) -> Option<&'a Win> {
    open.windows
        .iter()
        .find(|win| !before.contains(&win.id) && apps::belongs(&waiting.app, &win.app_id))
}

/// Put a window where the journal said it stood. A window opens on the workspace the screen is on,
/// so the move is only for one that landed somewhere else, which an app that places its own windows
/// can do. The window is given the keyboard last, so the next one opens beside it.
fn place(step: &Step, win: &Win, open: &Open) {
    let on = win
        .space
        .and_then(|id| open.all.iter().find(|space| space.id == id))
        .map_or(0, |space| space.idx);
    if step.workspace > 0 && on != step.workspace {
        if let Err(why) = crate::horizon::to_workspace(win.id, step.workspace) {
            eprintln!("lens: {} to workspace {}: {why}", step.app, step.workspace);
        }
    }
    if let Err(why) = crate::horizon::focus(win.id) {
        eprintln!("lens: the keyboard to {}: {why}", step.app);
    }
    if step.stack {
        if let Err(why) = crate::horizon::stack(win.id) {
            eprintln!("lens: {} into the column on its left: {why}", step.app);
        }
    }
    if step.floating {
        if let Err(why) = crate::horizon::float(win.id) {
            eprintln!("lens: {} out of the layout: {why}", step.app);
        }
    }
}
