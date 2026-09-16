//! The shell: the top bar along the top of the screen, the dock along the bottom, and the menus
//! that come and go over them. One process with a layer surface per part, drawn with iced on the
//! software renderer and placed by the layer-shell protocol, so it works on any machine the drive
//! meets.

use std::sync::{Mutex, OnceLock};

use iced::widget::container;
use iced::{
    Element, Font, Length, Subscription, Task, Theme, event, font, keyboard, theme, window,
};
use iced_layershell::actions::{LayerShellCustomAction, LayerShellCustomActionWithId};
use iced_layershell::reexport::{
    Anchor, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption,
};
use iced_layershell::settings::{LayerShellSettings, Settings};
use librift::os::{self, Action};
use librift::quasar;

use crate::answer;
use crate::bar;
use crate::clock;
use crate::control::{self, Command};
use crate::dock::{self, Dock};
use crate::horizon::{self, Open};
use crate::launcher::{self, App};
use crate::menu::{self, Menu, Results};
use crate::nu;
use crate::route::{self, Interpretation};
use crate::status::{Battery, Status, Volume};
use crate::theme::Palette;

/// The interface font.
pub const FONT: Font = Font {
    family: font::Family::Name("Noto Sans"),
    ..Font::DEFAULT
};
/// What a command printed is terminal output, and a table only lines up in a fixed width.
pub const MONO: Font = Font {
    family: font::Family::Name("DejaVu Sans Mono"),
    ..Font::DEFAULT
};
/// The name over a group of rows, the same size as the rows and in bold, the way a settings page
/// heads a section.
pub const HEADING: Font = Font {
    weight: font::Weight::Bold,
    ..FONT
};

/// What `lens --state` prints. The shell writes it after every message and the thread that
/// answers the socket reads it, so a query never waits for the one that draws.
fn kept() -> &'static Mutex<String> {
    static KEPT: OnceLock<Mutex<String>> = OnceLock::new();
    KEPT.get_or_init(|| Mutex::new(String::new()))
}

/// The shell's state. The bar and the dock are always there; a menu comes and goes with its
/// surface.
struct Lens {
    look: Palette,
    apps: Vec<App>,
    clock: String,
    status: Status,
    menu: Option<Menu>,
    dock: Dock,
}

/// What happens to the shell.
#[derive(Debug, Clone)]
pub enum Message {
    /// The minute turned, and this is the clock's line now.
    Tick(String),
    /// What the status sources say now.
    Status(Status),
    /// The Applications button, or Mod+Space.
    ToggleMenu,
    /// New words in the field.
    Input(String),
    /// Enter in the field.
    Submit,
    /// Up or down the list.
    Move(isize),
    /// A click on a row of the app list: that app starts.
    Pick(usize),
    /// Escape: clear the field, or close the menu when it is already empty.
    Escape,
    /// Close the menu, whatever surface it is on.
    Dismiss,
    /// A command or a pipeline finished.
    Done(Result<String, String>),
    /// Quasar answered.
    Answered(Result<(String, String), String>),
    /// A line came in on the socket.
    Typed(Command),
    /// Horizon opened, closed or focused something.
    Windows(Open),
    /// A click on a dock item: its app starts, or its window comes forward.
    Dock(String),
    /// A middle click on one: another window of that app.
    DockNew(String),
    /// A right click on one: the menu of what can be done with it.
    DockMenu(String),
    /// A row of that menu.
    DockRow(dock::Row),
    /// A click on a workspace button.
    Space(u8),
    /// Open the dock's surface.
    OpenDock(window::Id),
    /// Open the menu of a dock item, this tall, with its left edge here.
    OpenItemMenu(window::Id, u32, i32),
    /// Open the menu's surface, this tall.
    Open(window::Id, u32),
    /// The menu's surface has to grow or shrink.
    Resize(window::Id, u32),
    /// Close a surface.
    Close(window::Id),
    /// A surface took or lost the keyboard.
    Focus(window::Id, bool),
}

// the layer-shell runtime asks every message whether it is one of its own actions. the three that
// are carry a surface: opening the menu, resizing it as the list grows, and closing it. an action
// that makes a new surface must not name it as the target, or the runtime waits for a surface
// that does not exist yet
impl TryFrom<Message> for LayerShellCustomActionWithId {
    type Error = Message;

    fn try_from(message: Message) -> Result<Self, Message> {
        match message {
            Message::Open(id, height) => Ok(Self::new(
                None,
                LayerShellCustomAction::NewLayerShell {
                    settings: menu_surface(height),
                    id,
                },
            )),
            Message::OpenDock(id) => Ok(Self::new(
                None,
                LayerShellCustomAction::NewLayerShell {
                    settings: dock_surface(),
                    id,
                },
            )),
            Message::OpenItemMenu(id, height, left) => Ok(Self::new(
                None,
                LayerShellCustomAction::NewLayerShell {
                    settings: item_menu_surface(height, left),
                    id,
                },
            )),
            Message::Resize(id, height) => Ok(Self::new(
                Some(id),
                LayerShellCustomAction::SizeChange((menu::WIDTH, height)),
            )),
            Message::Close(id) => Ok(Self::new(Some(id), LayerShellCustomAction::RemoveWindow)),
            other => Err(other),
        }
    }
}

/// The menu's surface: on the overlay layer, hanging under the bar inside the working area, its
/// left edge under the Applications button. It takes the keyboard on demand, which the compositor
/// gives it as it appears and takes away as soon as anything else is clicked.
fn menu_surface(height: u32) -> NewLayerShellSettings {
    NewLayerShellSettings {
        size: Some((menu::WIDTH, height)),
        layer: Layer::Overlay,
        anchor: Anchor::Top | Anchor::Left,
        exclusive_zone: Some(0),
        margin: Some((0, 0, 0, i32::try_from(menu::PAD).unwrap_or(0))),
        keyboard_interactivity: KeyboardInteractivity::OnDemand,
        output_option: OutputOption::Active,
        events_transparent: false,
        namespace: Some("lens-menu".to_string()),
    }
}

/// The dock's surface: along the bottom edge, full width, with its own height reserved so that a
/// window sits over it and nothing is ever hidden behind it. A bar never takes the keyboard.
fn dock_surface() -> NewLayerShellSettings {
    NewLayerShellSettings {
        size: Some((0, dock::HEIGHT)),
        layer: Layer::Top,
        anchor: Anchor::Bottom | Anchor::Left | Anchor::Right,
        exclusive_zone: Some(i32::try_from(dock::HEIGHT).unwrap_or(0)),
        margin: None,
        keyboard_interactivity: KeyboardInteractivity::None,
        output_option: OutputOption::Active,
        events_transparent: false,
        namespace: Some("lens-dock".to_string()),
    }
}

/// The menu a right click on a dock item opens: standing on the dock, its left edge where the item
/// is. A surface that reserves nothing is placed inside the working area, so the dock's own height
/// is already taken off and the margin under it is nothing. It takes the keyboard the same way the
/// Applications menu does, so a click anywhere else closes it.
fn item_menu_surface(height: u32, left: i32) -> NewLayerShellSettings {
    NewLayerShellSettings {
        size: Some((dock::MENU_WIDTH, height)),
        layer: Layer::Overlay,
        anchor: Anchor::Bottom | Anchor::Left,
        exclusive_zone: Some(0),
        margin: Some((0, 0, 0, left)),
        keyboard_interactivity: KeyboardInteractivity::OnDemand,
        output_option: OutputOption::Active,
        events_transparent: false,
        namespace: Some("lens-menu".to_string()),
    }
}

/// Open the shell on the session's Wayland display and run until it is closed.
///
/// # Errors
///
/// When there is no display or the compositor has no layer-shell.
pub fn run(apps: Vec<App>) -> Result<(), iced_layershell::Error> {
    let look = crate::theme::load();
    iced_layershell::daemon(move || boot(look, apps.clone()), "lens", update, view)
        .theme(|state: &Lens, _| Theme::custom("Rift", palette(state.look)))
        .style(|state: &Lens, _: &Theme| theme::Style {
            // every surface paints its own background over all of itself; this is what shows if
            // one ever does not, and a software-rendered surface has no transparency
            background_color: state.look.bar,
            text_color: state.look.text,
        })
        .subscription(subscription)
        .settings(Settings {
            id: Some("dev.rift.Lens".to_string()),
            default_font: FONT,
            default_text_size: bar::TEXT_SIZE.into(),
            layer_settings: LayerShellSettings {
                anchor: Anchor::Top | Anchor::Left | Anchor::Right,
                layer: Layer::Top,
                exclusive_zone: i32::try_from(bar::HEIGHT).unwrap_or(0),
                size: Some((0, bar::HEIGHT)),
                // a bar never takes the keyboard away from a window
                keyboard_interactivity: KeyboardInteractivity::None,
                ..LayerShellSettings::default()
            },
            ..Settings::default()
        })
        .run()
}

fn palette(look: Palette) -> theme::Palette {
    theme::Palette {
        background: look.bar,
        text: look.text,
        primary: look.accent,
        success: look.ok,
        warning: look.warn,
        danger: look.error,
    }
}

fn boot(look: Palette, apps: Vec<App>) -> (Lens, Task<Message>) {
    // the dock is made here, not when something opens it: it is a part of the shell like the bar,
    // and it takes its own height from the screen before the first window is placed
    let dock = Dock::new(window::Id::unique(), &apps);
    let opening = Task::done(Message::OpenDock(dock.id));
    let state = Lens {
        look,
        apps,
        clock: clock::now(),
        status: Status::default(),
        menu: None,
        dock,
    };
    remember(&state);
    (state, opening)
}

fn subscription(_: &Lens) -> Subscription<Message> {
    Subscription::batch([keys(), focus(), terminal(), ticker(), windows()])
}

// the field takes the printable keys for itself, so these come from every event, not only the
// ones no widget wanted
fn keys() -> Subscription<Message> {
    event::listen_with(|event, _, _| match event {
        iced::Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => match key {
            keyboard::Key::Named(keyboard::key::Named::Escape) => Some(Message::Escape),
            keyboard::Key::Named(keyboard::key::Named::ArrowDown) => Some(Message::Move(1)),
            keyboard::Key::Named(keyboard::key::Named::ArrowUp) => Some(Message::Move(-1)),
            _ => None,
        },
        _ => None,
    })
}

// the compositor gives an on-demand surface the keyboard as it appears and takes it back when
// something else is clicked, which is how a click outside closes the menu. the surface is drawn
// before the keyboard reaches it, so the cursor goes in the field on either event
fn focus() -> Subscription<Message> {
    event::listen_with(|event, _, id| match event {
        iced::Event::Window(window::Event::Opened { .. } | window::Event::Focused) => {
            Some(Message::Focus(id, true))
        }
        iced::Event::Window(window::Event::Unfocused) => Some(Message::Focus(id, false)),
        _ => None,
    })
}

// the socket in the runtime directory, read on a thread of its own. the state query is answered
// there, from the lines the shell keeps up to date
fn terminal() -> Subscription<Message> {
    Subscription::run(|| {
        let (sender, receiver) = iced::futures::channel::mpsc::unbounded();
        std::thread::spawn(move || {
            if let Err(why) =
                control::serve(|command| {
                    if command == Command::State {
                        return Some(kept().lock().map_or_else(
                            |_| "the shell is busy".to_string(),
                            |lines| lines.clone(),
                        ));
                    }
                    let _ = sender.unbounded_send(Message::Typed(command));
                    None
                })
            {
                eprintln!("lens: {why}");
            }
        });
        receiver
    })
}

// the clock and the status, once a minute on the minute, read on a thread of its own because it
// runs child processes. the clock goes first: asking the status sources takes a moment, and the
// time on the bar should turn with the minute
fn ticker() -> Subscription<Message> {
    Subscription::run(|| {
        let (sender, receiver) = iced::futures::channel::mpsc::unbounded();
        std::thread::spawn(move || {
            loop {
                if sender.unbounded_send(Message::Tick(clock::now())).is_err() {
                    return;
                }
                if sender
                    .unbounded_send(Message::Status(Status::read()))
                    .is_err()
                {
                    return;
                }
                std::thread::sleep(clock::until_next_minute());
            }
        });
        receiver
    })
}

// horizon's windows and workspaces, read on a thread of its own because the stream blocks until
// the compositor has something to say. every event the dock draws from turns into one message
fn windows() -> Subscription<Message> {
    Subscription::run(|| {
        let (sender, receiver) = iced::futures::channel::mpsc::unbounded();
        std::thread::spawn(move || {
            horizon::watch(|open| {
                sender
                    .unbounded_send(Message::Windows(open.clone()))
                    .is_ok()
            });
        });
        receiver
    })
}

fn update(state: &mut Lens, message: Message) -> Task<Message> {
    let task = match message {
        Message::Tick(now) => {
            state.clock = now;
            Task::none()
        }
        Message::Status(status) => {
            state.status = status;
            Task::none()
        }
        Message::ToggleMenu => toggle(state),
        Message::Input(value) => write(state, value),
        Message::Submit => submit(state),
        Message::Move(step) => state.menu.as_mut().map_or_else(Task::none, |menu| {
            menu.step(step);
            menu.scroll()
        }),
        Message::Pick(at) => {
            if let Some(menu) = state.menu.as_mut() {
                menu.selected = Some(at);
            }
            submit(state)
        }
        Message::Escape => escape(state),
        Message::Dismiss => close(state),
        Message::Done(result) => {
            if let Some(menu) = state.menu.as_mut() {
                finish(menu, result);
            }
            Task::none()
        }
        Message::Answered(result) => state
            .menu
            .as_mut()
            .map_or_else(Task::none, |menu| answered(menu, result)),
        Message::Typed(command) => typed(state, command),
        Message::Windows(open) => {
            state.dock.changed(&state.apps, open);
            Task::none()
        }
        Message::Dock(key) => dock_click(state, &key),
        Message::DockNew(key) => {
            new_window(state, &key);
            Task::none()
        }
        Message::DockMenu(key) => dock_menu(state, &key),
        Message::DockRow(row) => dock_row(state, &row),
        Message::Space(number) => {
            report(horizon::activate(number));
            Task::none()
        }
        Message::Focus(id, has) => focused(state, id, has),
        // the runtime takes these before update ever sees them
        Message::Open(..)
        | Message::OpenDock(_)
        | Message::OpenItemMenu(..)
        | Message::Resize(..)
        | Message::Close(..) => Task::none(),
    };
    let grow = resize(state);
    remember(state);
    Task::batch([task, grow])
}

/// Open the menu when it is closed, close it when it is open.
fn toggle(state: &mut Lens) -> Task<Message> {
    if state.menu.is_some() {
        Task::done(Message::Dismiss)
    } else {
        open(state)
    }
}

fn open(state: &mut Lens) -> Task<Message> {
    if state.menu.is_some() {
        return Task::none();
    }
    // the entries are read again here, so an app installed since the session started is in the
    // list without a restart. it is a walk of a few directories, once per opening
    state.apps = launcher::load();
    let id = window::Id::unique();
    let menu = Menu::new(id, &state.apps);
    let height = menu.height;
    state.menu = Some(menu);
    Task::done(Message::Open(id, height))
}

fn close(state: &mut Lens) -> Task<Message> {
    state
        .menu
        .take()
        .map_or_else(Task::none, |menu| Task::done(Message::Close(menu.id)))
}

/// Escape clears the field first, the way a search entry does, and closes the menu when there is
/// nothing left to clear.
fn escape(state: &mut Lens) -> Task<Message> {
    let Lens { apps, menu, .. } = state;
    match menu.as_mut() {
        None => Task::none(),
        Some(menu) if menu.has_anything() => {
            menu.clear(apps);
            Task::batch([menu::focus_field(), menu.scroll()])
        }
        Some(_) => Task::done(Message::Dismiss),
    }
}

/// A click on a dock item: the app starts when it is not running, its window comes forward when
/// it is, and the next of its windows when one of them is the one being used.
fn dock_click(state: &mut Lens, key: &str) -> Task<Message> {
    let closing = close_item_menu(state);
    let Some(item) = state.dock.item(key) else {
        return closing;
    };
    if let Some(window) = item.next() {
        report(horizon::focus(window));
    } else if let Some(app) = item.app.as_ref() {
        open_app(app);
    }
    closing
}

/// A middle click on an item, and New window in its menu: one more window of that app.
fn new_window(state: &Lens, key: &str) {
    if let Some(app) = state.dock.item(key).and_then(|item| item.app.as_ref()) {
        open_app(app);
    }
}

/// A right click on an item: the menu of what can be done with it, on a surface of its own where
/// the item is. A second right click on the same item closes it again.
fn dock_menu(state: &mut Lens, key: &str) -> Task<Message> {
    let same = state.dock.menu.as_ref().is_some_and(|menu| menu.key == key);
    let closing = close_item_menu(state);
    if same {
        return closing;
    }
    let Some(rows) = state.dock.item(key).map(dock::Item::rows) else {
        return closing;
    };
    if rows.is_empty() {
        return closing;
    }
    let left = state.dock.left_of(key);
    let height = dock::menu_height(rows.len());
    let id = window::Id::unique();
    state.dock.menu = Some(dock::Menu {
        id,
        key: key.to_string(),
        rows,
    });
    Task::batch([closing, Task::done(Message::OpenItemMenu(id, height, left))])
}

/// A row of that menu. Every one of them closes it.
fn dock_row(state: &mut Lens, row: &dock::Row) -> Task<Message> {
    let Some(key) = state.dock.menu.as_ref().map(|menu| menu.key.clone()) else {
        return Task::none();
    };
    let closing = close_item_menu(state);
    match row {
        dock::Row::Window(window, _) => report(horizon::focus(*window)),
        dock::Row::New => new_window(state, &key),
        dock::Row::Pin(_) => state.dock.pin(&key, &state.apps),
        dock::Row::Close => {
            let windows: Vec<u64> = state
                .dock
                .item(&key)
                .map(|item| item.windows.iter().map(|(id, _)| *id).collect())
                .unwrap_or_default();
            for window in windows {
                report(horizon::close(window));
            }
        }
    }
    closing
}

/// Close the menu a right click opened, when one is open.
fn close_item_menu(state: &mut Lens) -> Task<Message> {
    state
        .dock
        .menu
        .take()
        .map_or_else(Task::none, |menu| Task::done(Message::Close(menu.id)))
}

/// Start an app from the dock. What went wrong goes in the journal: the dock has no line to say
/// it on, and the app either opens a window or it does not.
fn open_app(app: &App) {
    report(launcher::launch(app));
}

fn report(done: Result<(), String>) {
    if let Err(why) = done {
        eprintln!("lens: {why}");
    }
}

/// A surface took or lost the keyboard. A menu closes when it loses it, which is what happens
/// when anything outside it is clicked; when the Applications menu takes it, the cursor goes in
/// the field.
fn focused(state: &mut Lens, id: window::Id, has: bool) -> Task<Message> {
    if state.dock.menu.as_ref().is_some_and(|menu| menu.id == id) {
        return if has {
            Task::none()
        } else {
            close_item_menu(state)
        };
    }
    if state.menu.as_ref().is_none_or(|menu| menu.id != id) {
        return Task::none();
    }
    if has {
        menu::focus_field()
    } else {
        Task::done(Message::Dismiss)
    }
}

/// A line from the socket. Typing into the field opens the menu when it is closed, because the
/// boot test and Quasar's step reach the field that way.
fn typed(state: &mut Lens, command: Command) -> Task<Message> {
    match command {
        Command::Type(words) => {
            let opening = open(state);
            Task::batch([opening, write(state, words)])
        }
        Command::Enter(words) => {
            let opening = open(state);
            let writing = if words.is_empty() {
                Task::none()
            } else {
                write(state, words)
            };
            Task::batch([opening, writing, submit(state)])
        }
        Command::Escape => escape(state),
        Command::Menu => toggle(state),
        // answered on the socket's own thread, from the lines remember() keeps
        Command::State => Task::none(),
    }
}

/// New words in the field. The list is shorter or longer for them, so it goes back to its top,
/// which the widget itself does not do when its contents change.
fn write(state: &mut Lens, words: String) -> Task<Message> {
    let Lens { apps, menu, .. } = state;
    menu.as_mut().map_or_else(Task::none, |menu| {
        menu.typed(apps, words);
        menu.scroll()
    })
}

/// Ask the compositor for a taller or shorter menu when it changed shape.
fn resize(state: &mut Lens) -> Task<Message> {
    let Some(menu) = state.menu.as_mut() else {
        return Task::none();
    };
    let wanted = menu.wanted_height();
    if wanted == menu.height {
        return Task::none();
    }
    menu.height = wanted;
    Task::done(Message::Resize(menu.id, wanted))
}

/// What `lens --state` prints: one line per thing the bar shows.
fn remember(state: &Lens) {
    let mut lines = String::new();
    let mut line = |key: &str, value: &str| {
        lines.push_str(key);
        lines.push(' ');
        lines.push_str(value);
        lines.push('\n');
    };
    line("clock", &state.clock);
    line("apps", &state.apps.len().to_string());
    line("network", &state.status.network.word());
    line(
        "volume",
        &state
            .status
            .volume
            .map_or_else(|| "none".to_string(), Volume::word),
    );
    line(
        "battery",
        &state
            .status
            .battery
            .map_or_else(|| "none".to_string(), Battery::word),
    );
    match &state.menu {
        None => line("menu", "closed"),
        Some(menu) => {
            line("menu", "open");
            line("field", &menu.input);
            line("rows", &menu.results.shown().to_string());
            if let Some((text, wrong)) = menu.line() {
                line(if wrong { "error" } else { "notice" }, text);
            }
        }
    }
    line("dock", &state.dock.line());
    line("workspaces", &state.dock.spaces_line());
    if let Some(menu) = &state.dock.menu {
        line("item", &format!("{} {}", menu.key, menu.rows.len()));
    }
    if let Ok(mut kept) = kept().lock() {
        *kept = lines;
    }
}

fn submit(state: &mut Lens) -> Task<Message> {
    let Lens { apps, menu, .. } = state;
    let Some(menu) = menu.as_mut() else {
        return Task::none();
    };
    if let Some(action) = menu.pending.take() {
        menu.input.clear();
        menu.notice = None;
        return start(action);
    }
    // the list is a menu: Enter takes the row that is selected, not always the first
    if let Some(app) = menu.selected_app().cloned() {
        launch(menu, &app);
        return Task::done(Message::Dismiss);
    }
    let reading = route::route(&menu.input, apps);
    eprintln!("lens: {:?} -> {reading:?}", menu.input);
    match reading {
        Interpretation::Nothing => {}
        Interpretation::Launch(app) => {
            launch(menu, &app);
            return Task::done(Message::Dismiss);
        }
        Interpretation::Os(action) => return propose(menu, action),
        Interpretation::Usage(usage) => {
            menu.results = Results::None;
            menu.error = Some(usage.to_string());
        }
        Interpretation::Shell(line) => {
            menu.input.clear();
            menu.results = Results::None;
            menu.error = None;
            return Task::perform(async move { nu::run(&line) }, Message::Done);
        }
        Interpretation::Ask(question) => {
            menu.input.clear();
            menu.results = Results::None;
            menu.error = None;
            menu.notice = Some("Asking Quasar".to_string());
            return ask(question);
        }
    }
    Task::none()
}

/// An OS command, typed or proposed by Quasar. One that changes something waits for a second
/// Enter, the rest runs at once.
fn propose(menu: &mut Menu, action: Action) -> Task<Message> {
    if action.mutating {
        menu.notice = Some(format!(
            "{}? Press Enter to confirm or Escape to cancel.",
            action.summary
        ));
        menu.pending = Some(action);
        return Task::none();
    }
    menu.input.clear();
    menu.notice = None;
    start(action)
}

/// The question goes to quasard on a thread of its own. An answer can take a minute, and the
/// executor's few threads also carry the socket the terminal types on.
fn ask(question: String) -> Task<Message> {
    Task::perform(
        async move {
            let (sender, receiver) = iced::futures::channel::oneshot::channel();
            std::thread::spawn(move || {
                let _ = sender.send(quasar::ask(&question));
            });
            receiver
                .await
                .unwrap_or_else(|_| Err("Quasar stopped before it answered.".to_string()))
        },
        Message::Answered,
    )
}

/// Quasar's reply: an answer goes in the list, a command is handled like a typed one, anything
/// else goes on the line under it.
fn answered(menu: &mut Menu, result: Result<(String, String), String>) -> Task<Message> {
    let reply = match result {
        Ok((kind, text)) => quasar::read(&kind, &text),
        Err(why) => quasar::Reply::Refused(why),
    };
    eprintln!("lens: quasar -> {reply:?}");
    menu.notice = None;
    menu.error = None;
    menu.results = Results::None;
    match reply {
        quasar::Reply::Answer(answer) => {
            menu.results = Results::Answer(answer::rows(&answer, menu::ROWS));
            Task::none()
        }
        quasar::Reply::Action(action) => propose(menu, action),
        quasar::Reply::Refused(why) => {
            menu.error = Some(why);
            Task::none()
        }
    }
}

fn launch(menu: &mut Menu, app: &App) {
    match launcher::launch(app) {
        Ok(()) => {
            menu.notice = Some(format!("Starting {}", app.name));
            menu.error = None;
        }
        Err(why) => menu.error = Some(why),
    }
    menu.input.clear();
    menu.results = Results::None;
    menu.selected = None;
}

fn start(action: Action) -> Task<Message> {
    eprintln!("lens: running {} {}", action.program, action.args.join(" "));
    Task::perform(async move { os::run(&action) }, Message::Done)
}

/// What a command or a pipeline printed goes in the list, what it complained about goes on the
/// line under it.
fn finish(menu: &mut Menu, result: Result<String, String>) {
    match result {
        Ok(output) => {
            let rows = nu::rows(&output, menu::ROWS);
            menu.error = None;
            menu.results = if rows.is_empty() {
                menu.notice = Some("Done".to_string());
                Results::None
            } else {
                menu.notice = None;
                Results::Output(rows)
            };
        }
        Err(why) => {
            menu.notice = None;
            menu.results = Results::None;
            menu.error = Some(why);
        }
    }
}

fn view(state: &Lens, id: window::Id) -> Element<'_, Message> {
    if let Some(menu) = state.menu.as_ref().filter(|menu| menu.id == id) {
        return menu::view(state.look, menu);
    }
    if let Some(menu) = state.dock.menu.as_ref().filter(|menu| menu.id == id) {
        return dock::menu_view(state.look, menu);
    }
    if id == state.dock.id {
        return dock::view(state.look, &state.dock);
    }
    container(bar::view(
        state.look,
        &state.clock,
        &state.status,
        state.menu.is_some(),
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
