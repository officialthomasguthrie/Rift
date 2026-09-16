//! The shell: the top bar along the top of the screen, and the Applications menu that hangs under
//! it with the field in it. One process with a layer surface per part, drawn with iced on the
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

/// What `lens --state` prints. The shell writes it after every message and the thread that
/// answers the socket reads it, so a query never waits for the one that draws.
fn kept() -> &'static Mutex<String> {
    static KEPT: OnceLock<Mutex<String>> = OnceLock::new();
    KEPT.get_or_init(|| Mutex::new(String::new()))
}

/// The shell's state. The bar is always there; the menu comes and goes with its surface.
struct Lens {
    look: Palette,
    apps: Vec<App>,
    clock: String,
    status: Status,
    menu: Option<Menu>,
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
    let state = Lens {
        look,
        apps,
        clock: clock::now(),
        status: Status::default(),
        menu: None,
    };
    remember(&state);
    (state, Task::none())
}

fn subscription(_: &Lens) -> Subscription<Message> {
    Subscription::batch([keys(), focus(), terminal(), ticker()])
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
        Message::Input(value) => {
            let Lens { apps, menu, .. } = state;
            if let Some(menu) = menu.as_mut() {
                menu.typed(apps, value);
            }
            Task::none()
        }
        Message::Submit => submit(state),
        Message::Move(step) => {
            if let Some(menu) = state.menu.as_mut() {
                menu.step(step);
            }
            Task::none()
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
        Message::Focus(id, has) => focused(state, id, has),
        // the runtime takes these before update ever sees them
        Message::Open(..) | Message::Resize(..) | Message::Close(..) => Task::none(),
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
    let id = window::Id::unique();
    let menu = Menu::new(id);
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
    match state.menu.as_mut() {
        None => Task::none(),
        Some(menu) if menu.has_anything() => {
            menu.clear();
            menu::focus_field()
        }
        Some(_) => Task::done(Message::Dismiss),
    }
}

/// A surface took or lost the keyboard. The menu closes when it loses it, which is what happens
/// when anything outside it is clicked; when it takes it, the cursor goes in the field.
fn focused(state: &mut Lens, id: window::Id, has: bool) -> Task<Message> {
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
            write(state, words);
            opening
        }
        Command::Enter(words) => {
            let opening = open(state);
            if !words.is_empty() {
                write(state, words);
            }
            Task::batch([opening, submit(state)])
        }
        Command::Escape => escape(state),
        Command::Menu => toggle(state),
        // answered on the socket's own thread, from the lines remember() keeps
        Command::State => Task::none(),
    }
}

fn write(state: &mut Lens, words: String) {
    let Lens { apps, menu, .. } = state;
    if let Some(menu) = menu.as_mut() {
        menu.typed(apps, words);
    }
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
            line("rows", &menu.results.len().to_string());
            if let Some((text, wrong)) = menu.line() {
                line(if wrong { "error" } else { "notice" }, text);
            }
        }
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
    if let Results::Matches(matched) = &menu.results {
        if let Some(app) = matched.get(menu.selected).cloned() {
            launch(menu, &app);
            return Task::done(Message::Dismiss);
        }
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
    menu.selected = 0;
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
    match &state.menu {
        Some(menu) if menu.id == id => menu::view(state.look, menu),
        _ => container(bar::view(
            state.look,
            &state.clock,
            &state.status,
            state.menu.is_some(),
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .into(),
    }
}
