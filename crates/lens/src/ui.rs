//! The shell: the top bar along the top of the screen, the dock along the bottom, and the menus,
//! notifications and the key popup that come and go over them. One process with a layer surface per
//! part, drawn with iced on the software renderer and placed by the layer-shell protocol, so it
//! works on any machine the drive meets.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use iced::widget::container;
use iced::{
    Element, Font, Length, Subscription, Task, Theme, event, font, keyboard, theme, window,
};
use iced_layershell::actions::{LayerShellCustomAction, LayerShellCustomActionWithId};
use iced_layershell::reexport::{
    Anchor, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption,
};
use iced_layershell::settings::{LayerShellSettings, Settings};
use librift::appearance;
use librift::battery::Battery;
use librift::os::{self, Action};
use librift::sound::{self, Side, Volume};
use librift::{bluetooth, network, quasar, session};

use crate::access;
use crate::answer;
use crate::banner;
use crate::bar;
use crate::calendar::{Day, Month, Weekday};
use crate::clock;
use crate::control::{self, Command, Level, Recording};
use crate::datemenu;
use crate::dialog::{self, Ask, Dialog};
use crate::dock::{self, Dock};
use crate::horizon::{self, Open};
use crate::launcher::{self, App};
use crate::menu::{self, Menu, Results};
use crate::notice::{self, Effect, Fitted, Notices, Notification, Outbox};
use crate::nu;
use crate::popup::{self, Popup};
use crate::route::{self, Interpretation};
use crate::status::{self, Status};
use crate::system;
use crate::theme::Palette;
use crate::watch::{self, Latest};

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

/// How much bigger than its own sizes the shell draws itself, in per cent, from the owner's
/// interface text size. Every surface is asked for at this much of its size in the pixels the
/// compositor places it in, and iced is told the same factor, so the bar, the dock and the menus
/// grow with the text in the apps and everything inside them keeps the numbers it is written with.
/// It is one number for the whole process because the step that turns a message into a layer-shell
/// action has no state to read.
static SCALE: AtomicU32 = AtomicU32::new(appearance::TEXT_DEFAULT);

/// A size of the shell's own, in the pixels the compositor places surfaces in.
fn scaled(size: u32) -> u32 {
    (size.saturating_mul(SCALE.load(Ordering::Relaxed)) + 50) / 100
}

/// The same for a margin, which the protocol takes as a whole number that may be negative.
fn margin(size: u32) -> i32 {
    i32::try_from(scaled(size)).unwrap_or(0)
}

/// What iced draws a surface at, so a bar asked for at half again its height holds the same rows
/// half again as big.
fn factor() -> f32 {
    f32::from(u16::try_from(SCALE.load(Ordering::Relaxed)).unwrap_or(100)) / 100.0
}

/// How long after the compositor closed a menu a press of that menu's own button is the click that
/// closed it, and not a click to open it again.
const REOPEN: Duration = Duration::from_millis(400);

/// How long a network may take to come up after it was picked.
const JOIN_WAIT: Duration = Duration::from_secs(45);

/// The shell's state. The bar and the dock are always there; a menu, a dialog, a notification or the
/// key popup comes and goes with its surface.
struct Lens {
    /// Dark or light, from the owner's setting.
    theme: appearance::Theme,
    /// Which of GNOME's nine the accent is, from the owner's setting.
    accent: appearance::Accent,
    look: Palette,
    apps: Vec<App>,
    clock: String,
    /// Today, for the calendar.
    today: Option<Day>,
    /// The day the locale starts its weeks on.
    first: Weekday,
    status: Status,
    menu: Option<Menu>,
    system: Option<system::Menu>,
    /// The clock menu.
    datemenu: Option<datemenu::Menu>,
    dialog: Option<Dialog>,
    dock: Dock,
    /// The bar's own surface, once the compositor has opened it. It is the one surface the shell
    /// does not open itself, so it has no id until then.
    bar: Option<window::Id>,
    /// The notifications on screen and the ones kept.
    notices: Notices,
    /// Where the signals about them go out on the bus, once the server has the name.
    outbox: Option<Outbox>,
    /// The key popup, while it is up.
    popup: Option<Popup>,
    /// The file the screen recorder is writing, while it is running. The bar is marked for it.
    recording: Option<String>,
    /// Counts the keys the popup showed, so only the last one's second closes it.
    keys: u64,
    /// The menu the compositor closed by taking the keyboard away, and when.
    dismissed: Option<(Closed, Instant)>,
    /// Sets the volume the slider asks for, on a thread of its own.
    volume: Latest<u8>,
    /// Sets the brightness the slider asks for.
    brightness: Latest<u8>,
}

/// Which of the bar's menus closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Closed {
    Applications,
    Clock,
    System,
}

/// What happens to the shell.
#[derive(Debug, Clone)]
pub enum Message {
    /// The minute turned, and this is what the clock says now.
    Tick(clock::Now),
    /// What `NetworkManager` says now, or why it did not answer. It is the biggest thing a
    /// message carries, so it travels behind a pointer.
    Network(Box<Result<network::Picture, String>>),
    /// What `UPower` says about the battery now.
    Battery(Option<Battery>),
    /// What `BlueZ` says now.
    Bluetooth(Option<bluetooth::Picture>),
    /// The default sink's volume now.
    Sound(Option<Volume>),
    /// The backlight now.
    Brightness(Option<u8>),
    /// The Applications button, or Mod+Space.
    ToggleMenu,
    /// The clock in the bar.
    ToggleClock,
    /// A button or the switch of the clock menu.
    Clock(datemenu::Event),
    /// An app sent a notification.
    Notified(Notification),
    /// An app closed the notification it sent.
    Recalled(u32),
    /// The notification server has the name, and takes the signals the shell sends through this.
    Outbox(Outbox),
    /// A notification on screen was pressed, closed, or the pointer came or went.
    Banner(banner::Event),
    /// A notification's time on screen ran out, for this run of it.
    Expire(u32, u64),
    /// The key popup's second is over, for this key.
    PopupDone(u64),
    /// The status icons in the bar.
    ToggleSystem,
    /// A row, a switch or a slider of the system menu.
    System(system::Event),
    /// Something the system menu asked for finished, or what went wrong.
    Acted(Result<(), String>),
    /// A dialog's field or buttons.
    Dialog(dialog::Event),
    /// What a dialog asked for finished, or what went wrong. The id is the dialog's.
    DialogDone(window::Id, Result<(), String>),
    /// Enter, which a dialog with no field takes as its default button.
    Enter,
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
    /// Open the system menu's surface, this tall.
    OpenSystem(window::Id, u32),
    /// Open a dialog's surface, this tall.
    OpenDialog(window::Id, u32),
    /// Open the clock menu's surface, this tall.
    OpenClock(window::Id, u32),
    /// Open a notification's surface, this tall and this far under the bar.
    OpenBanner(window::Id, u32, u32),
    /// Open the key popup's surface.
    OpenPopup(window::Id),
    /// Put a notification's surface this far under the bar.
    Place(window::Id, u32),
    /// A menu's surface has to grow or shrink to this width and height.
    Resize(window::Id, u32, u32),
    /// A bar has to keep this much of the screen for itself.
    Reserve(window::Id, u32),
    /// A surface opened.
    Opened(window::Id),
    /// Close a surface.
    Close(window::Id),
    /// A surface took or lost the keyboard.
    Focus(window::Id, bool),
}

// the layer-shell runtime asks every message whether it is one of its own actions. the ones that
// are carry a surface: opening a menu, a dialog or the dock, resizing a menu as it grows, and
// closing one. an action that makes a new surface must not name it as the target, or the runtime
// waits for a surface that does not exist yet
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
            Message::OpenSystem(id, height) => Ok(Self::new(
                None,
                LayerShellCustomAction::NewLayerShell {
                    settings: system_surface(height),
                    id,
                },
            )),
            Message::OpenDialog(id, height) => Ok(Self::new(
                None,
                LayerShellCustomAction::NewLayerShell {
                    settings: dialog_surface(height),
                    id,
                },
            )),
            Message::OpenClock(id, height) => Ok(Self::new(
                None,
                LayerShellCustomAction::NewLayerShell {
                    settings: clock_surface(height),
                    id,
                },
            )),
            Message::OpenBanner(id, height, top) => Ok(Self::new(
                None,
                LayerShellCustomAction::NewLayerShell {
                    settings: banner_surface(height, top),
                    id,
                },
            )),
            Message::OpenPopup(id) => Ok(Self::new(
                None,
                LayerShellCustomAction::NewLayerShell {
                    settings: popup_surface(),
                    id,
                },
            )),
            Message::Place(id, top) => Ok(Self::new(
                Some(id),
                LayerShellCustomAction::MarginChange(banner_margin(top)),
            )),
            Message::Resize(id, width, height) => Ok(Self::new(
                Some(id),
                LayerShellCustomAction::SizeChange((scaled(width), scaled(height))),
            )),
            Message::Reserve(id, height) => Ok(Self::new(
                Some(id),
                LayerShellCustomAction::ExclusiveZoneChange(margin(height)),
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
        size: Some((scaled(menu::WIDTH), scaled(height))),
        layer: Layer::Overlay,
        anchor: Anchor::Top | Anchor::Left,
        exclusive_zone: Some(0),
        margin: Some((0, 0, 0, margin(menu::PAD))),
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
        size: Some((0, scaled(dock::HEIGHT))),
        layer: Layer::Top,
        anchor: Anchor::Bottom | Anchor::Left | Anchor::Right,
        exclusive_zone: Some(margin(dock::HEIGHT)),
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
        size: Some((scaled(dock::MENU_WIDTH), scaled(height))),
        layer: Layer::Overlay,
        anchor: Anchor::Bottom | Anchor::Left,
        exclusive_zone: Some(0),
        margin: Some((0, 0, 0, margin(u32::try_from(left).unwrap_or(0)))),
        keyboard_interactivity: KeyboardInteractivity::OnDemand,
        output_option: OutputOption::Active,
        events_transparent: false,
        namespace: Some("lens-menu".to_string()),
    }
}

/// The system menu's surface: on the overlay layer, hanging under the bar inside the working area,
/// its right edge under the status icons. It takes the keyboard the way the Applications menu
/// does, so a click anywhere else closes it.
fn system_surface(height: u32) -> NewLayerShellSettings {
    NewLayerShellSettings {
        size: Some((scaled(system::WIDTH), scaled(height))),
        layer: Layer::Overlay,
        anchor: Anchor::Top | Anchor::Right,
        exclusive_zone: Some(0),
        margin: Some((0, margin(system::PAD), 0, 0)),
        keyboard_interactivity: KeyboardInteractivity::OnDemand,
        output_option: OutputOption::Active,
        events_transparent: false,
        namespace: Some("lens-menu".to_string()),
    }
}

/// A dialog's surface: on the overlay layer in the middle of the screen, anchored to no edge. It
/// holds the keyboard until it is answered, so a click on a window does not throw away what was
/// typed into it.
fn dialog_surface(height: u32) -> NewLayerShellSettings {
    NewLayerShellSettings {
        size: Some((scaled(dialog::WIDTH), scaled(height))),
        layer: Layer::Overlay,
        anchor: Anchor::empty(),
        exclusive_zone: Some(0),
        margin: None,
        keyboard_interactivity: KeyboardInteractivity::Exclusive,
        output_option: OutputOption::Active,
        events_transparent: false,
        namespace: Some("lens-dialog".to_string()),
    }
}

/// The clock menu's surface: on the overlay layer, hanging under the bar inside the working area and
/// anchored to no side, so it is in the middle of the screen under the clock. It takes the keyboard
/// the way the other menus do, so a click anywhere else closes it.
fn clock_surface(height: u32) -> NewLayerShellSettings {
    NewLayerShellSettings {
        size: Some((scaled(datemenu::WIDTH), scaled(height))),
        layer: Layer::Overlay,
        anchor: Anchor::Top,
        exclusive_zone: Some(0),
        margin: None,
        keyboard_interactivity: KeyboardInteractivity::OnDemand,
        output_option: OutputOption::Active,
        events_transparent: false,
        namespace: Some("lens-menu".to_string()),
    }
}

/// A notification's surface: on the overlay layer at the top right of the working area, this far
/// under the bar. It never takes the keyboard, so it does not take it away from a window.
fn banner_surface(height: u32, top: u32) -> NewLayerShellSettings {
    NewLayerShellSettings {
        size: Some((scaled(banner::WIDTH), scaled(height))),
        layer: Layer::Overlay,
        anchor: Anchor::Top | Anchor::Right,
        exclusive_zone: Some(0),
        margin: Some(banner_margin(top)),
        keyboard_interactivity: KeyboardInteractivity::None,
        output_option: OutputOption::Active,
        events_transparent: false,
        namespace: Some("lens-notify".to_string()),
    }
}

/// The margins of a notification this far under the bar: top, right, bottom and left.
fn banner_margin(top: u32) -> (i32, i32, i32, i32) {
    (margin(top), margin(notice::GAP), 0, 0)
}

/// The key popup's surface: on the overlay layer, anchored to the bottom alone so it is in the
/// middle, standing above the dock. Clicks go through it to whatever is under it.
fn popup_surface() -> NewLayerShellSettings {
    NewLayerShellSettings {
        size: Some((scaled(popup::WIDTH), scaled(popup::HEIGHT))),
        layer: Layer::Overlay,
        anchor: Anchor::Bottom,
        exclusive_zone: Some(0),
        margin: Some((0, 0, margin(popup::ABOVE), 0)),
        keyboard_interactivity: KeyboardInteractivity::None,
        output_option: OutputOption::Active,
        events_transparent: true,
        namespace: Some("lens-popup".to_string()),
    }
}

/// Open the shell on the session's Wayland display and run until it is closed.
///
/// # Errors
///
/// When there is no display or the compositor has no layer-shell.
pub fn run(apps: Vec<App>) -> Result<(), iced_layershell::Error> {
    let chosen = appearance::Theme::read();
    // the interface text size before the first surface is asked for, since the bar is asked for at
    // its size as the daemon starts
    SCALE.store(appearance::text(), Ordering::Relaxed);
    // apps and the compositor follow the same setting. dconf may have to be started on the bus
    // first, which the bar does not wait for
    thread::spawn(move || {
        if let Err(why) = appearance::apply(chosen) {
            eprintln!("lens: {why}");
        }
        // and how big each screen is drawn, which Orbit keeps in the host profile. a machine with
        // no answer from Orbit is drawn the way the compositor works it out for itself
        if let Err(why) = librift::orbit::follow() {
            eprintln!("lens: the screens: {why}");
        }
        // and the mouse and the touchpad, written the way this image writes them
        if let Err(why) = librift::pointer::apply() {
            eprintln!("lens: the mouse and the touchpad: {why}");
        }
    });
    iced_layershell::daemon(move || boot(chosen, apps.clone()), "lens", update, view)
        .theme(|state: &Lens, _| Theme::custom("Rift", palette(state.look)))
        .scale_factor(|_: &Lens, _| factor())
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
                exclusive_zone: margin(bar::HEIGHT),
                size: Some((0, scaled(bar::HEIGHT))),
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

fn boot(chosen: appearance::Theme, apps: Vec<App>) -> (Lens, Task<Message>) {
    // the dock is made here, not when something opens it: it is a part of the shell like the bar,
    // and it takes its own height from the screen before the first window is placed
    let dock = Dock::new(window::Id::unique(), &apps);
    let opening = Task::done(Message::OpenDock(dock.id));
    let now = clock::now();
    let accent = appearance::Accent::read();
    let state = Lens {
        theme: chosen,
        accent,
        look: crate::theme::palette(chosen, accent),
        apps,
        clock: now.line,
        today: now.today,
        first: clock::first_weekday(),
        status: Status::default(),
        menu: None,
        system: None,
        datemenu: None,
        dialog: None,
        dock,
        bar: None,
        notices: Notices::new(notice::quiet()),
        outbox: None,
        popup: None,
        recording: access::running(access::Tool::Recorder).and_then(|(_, file)| file),
        keys: 0,
        dismissed: None,
        volume: Latest::new(|level| report(sound::set_volume(Side::Output, level))),
        brightness: Latest::new(|level| report(status::set_brightness(level))),
    };
    remember(&state);
    (state, opening)
}

fn subscription(_: &Lens) -> Subscription<Message> {
    Subscription::batch([
        keys(),
        focus(),
        terminal(),
        ticker(),
        windows(),
        watch::network(),
        watch::battery(),
        watch::bluetooth(),
        watch::sound(),
        watch::zone(),
        notice::serve(),
    ])
}

// the field takes the printable keys for itself, so these come from every event, not only the
// ones no widget wanted
fn keys() -> Subscription<Message> {
    event::listen_with(|event, _, _| match event {
        iced::Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => match key {
            keyboard::Key::Named(keyboard::key::Named::Escape) => Some(Message::Escape),
            keyboard::Key::Named(keyboard::key::Named::ArrowDown) => Some(Message::Move(1)),
            keyboard::Key::Named(keyboard::key::Named::ArrowUp) => Some(Message::Move(-1)),
            keyboard::Key::Named(keyboard::key::Named::Enter) => Some(Message::Enter),
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
        iced::Event::Window(window::Event::Opened { .. }) => Some(Message::Opened(id)),
        iced::Event::Window(window::Event::Focused) => Some(Message::Focus(id, true)),
        iced::Event::Window(window::Event::Unfocused) => Some(Message::Focus(id, false)),
        _ => None,
    })
}

// the socket in the runtime directory, read on a thread of its own. the state query is answered
// there, from the lines the shell keeps up to date.
//
// every one of these is named: iced tells two subscriptions apart by the type of the stream they
// make and the address of the function that makes it, and every one of ours makes the same kind of
// stream, so two of them whose code the optimiser folds together would be one subscription and the
// second would never be polled. the name is what tells them apart
fn terminal() -> Subscription<Message> {
    Subscription::run_with("terminal", |_| {
        let (sender, receiver) = iced::futures::channel::mpsc::unbounded();
        std::thread::spawn(move || {
            if let Err(why) = control::serve(|command| {
                if command == Command::State {
                    return Some(kept().lock().map_or_else(
                        |_| "the shell is busy".to_string(),
                        |lines| lines.clone() + &access::lines(),
                    ));
                }
                let _ = sender.unbounded_send(Message::Typed(command));
                None
            }) {
                eprintln!("lens: {why}");
            }
        });
        receiver
    })
}

// the clock, once a minute on the minute, read on a thread of its own because it runs a child
// process. the volume is read in the same tick: pw-mon does not say when the default sink becomes
// another one, and a minute is soon enough for that
fn ticker() -> Subscription<Message> {
    Subscription::run_with("ticker", |_| {
        let (sender, receiver) = iced::futures::channel::mpsc::unbounded();
        std::thread::spawn(move || {
            loop {
                if sender.unbounded_send(Message::Tick(clock::now())).is_err() {
                    return;
                }
                if sender
                    .unbounded_send(Message::Sound(sound::volume(Side::Output)))
                    .is_err()
                {
                    return;
                }
                std::thread::sleep(librift::time::until_next_minute());
            }
        });
        receiver
    })
}

// horizon's windows and workspaces, read on a thread of its own because the stream blocks until
// the compositor has something to say. every event the dock draws from turns into one message
fn windows() -> Subscription<Message> {
    Subscription::run_with("windows", |_| {
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
        Message::Tick(_)
        | Message::Network(_)
        | Message::Battery(_)
        | Message::Bluetooth(_)
        | Message::Sound(_)
        | Message::Brightness(_) => {
            heard(state, message);
            Task::none()
        }
        Message::ToggleMenu => toggle(state),
        Message::ToggleClock
        | Message::Clock(_)
        | Message::Notified(_)
        | Message::Recalled(_)
        | Message::Outbox(_)
        | Message::Banner(_)
        | Message::Expire(..)
        | Message::PopupDone(_) => notices(state, message),
        Message::ToggleSystem => toggle_system(state),
        Message::System(event) => system_event(state, event),
        Message::Acted(result) => {
            match state.system.as_mut() {
                Some(menu) => {
                    menu.notice = None;
                    menu.error = result.err();
                }
                None => report(result),
            }
            Task::none()
        }
        Message::Dialog(event) => dialog_event(state, event),
        Message::DialogDone(id, result) => dialog_done(state, id, result),
        Message::Enter => match state.dialog.as_ref().map(|dialog| &dialog.ask) {
            Some(Ask::Command(_)) => dialog_event(state, dialog::Event::Confirm),
            _ => Task::none(),
        },
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
        // the bar is the one surface the shell does not open itself: the runtime makes it from the
        // settings and names it when it maps, which is before anything can open a menu. its id is
        // what a new interface text size is sent to
        Message::Opened(id) => {
            if state.bar.is_none() && id != state.dock.id {
                state.bar = Some(id);
            }
            focused(state, id, true)
        }
        // the runtime takes these before update ever sees them
        Message::Open(..)
        | Message::OpenDock(_)
        | Message::OpenItemMenu(..)
        | Message::OpenSystem(..)
        | Message::OpenDialog(..)
        | Message::OpenClock(..)
        | Message::OpenBanner(..)
        | Message::OpenPopup(_)
        | Message::Place(..)
        | Message::Resize(..)
        | Message::Reserve(..)
        | Message::Close(..) => Task::none(),
    };
    let grow = resize(state);
    remember(state);
    Task::batch([task, grow])
}

/// What the clock and the status sources said: the bar and the menus draw from it.
fn heard(state: &mut Lens, message: Message) {
    match message {
        Message::Tick(now) => {
            state.clock = now.line;
            if now.today.is_some() {
                state.today = now.today;
            }
        }
        Message::Network(picture) => {
            if let Err(why) = &*picture {
                eprintln!("lens: {why}");
            }
            state.status.network = (*picture).ok();
        }
        Message::Battery(battery) => state.status.battery = battery,
        Message::Bluetooth(bluetooth) => state.status.bluetooth = bluetooth,
        Message::Sound(volume) => state.status.volume = volume,
        Message::Brightness(brightness) => state.status.brightness = brightness,
        _ => {}
    }
}

/// The clock menu, notifications and the key popup.
fn notices(state: &mut Lens, message: Message) -> Task<Message> {
    match message {
        Message::ToggleClock => toggle_clock(state),
        Message::Clock(event) => clock_event(state, event),
        Message::Notified(notification) => notified(state, notification),
        Message::Recalled(id) => {
            let effects = state.notices.recall(id);
            apply(state, effects)
        }
        Message::Outbox(outbox) => {
            state.outbox = Some(outbox);
            Task::none()
        }
        Message::Banner(event) => banner_event(state, event),
        Message::Expire(id, epoch) => {
            let effects = state.notices.expire(id, epoch);
            apply(state, effects)
        }
        Message::PopupDone(key) => match state.popup {
            Some(shown) if shown.epoch == key => {
                state.popup = None;
                Task::done(Message::Close(shown.id))
            }
            _ => Task::none(),
        },
        _ => Task::none(),
    }
}

/// Open the menu when it is closed, close it when it is open.
fn toggle(state: &mut Lens) -> Task<Message> {
    if state.menu.is_some() {
        Task::done(Message::Dismiss)
    } else if state.dialog.is_some() || recently(state, Closed::Applications) {
        Task::none()
    } else {
        open(state)
    }
}

/// Open the clock menu when it is closed, close it when it is open. It opens on the month today is
/// in.
fn toggle_clock(state: &mut Lens) -> Task<Message> {
    if state.datemenu.is_some() {
        return close_clock(state);
    }
    if state.dialog.is_some() || recently(state, Closed::Clock) {
        return Task::none();
    }
    let id = window::Id::unique();
    let height = datemenu::height(state.notices.kept.len());
    let month = state.today.map_or(
        Month {
            year: 1970,
            month: 1,
        },
        Month::of,
    );
    state.datemenu = Some(datemenu::Menu { id, height, month });
    Task::done(Message::OpenClock(id, height))
}

fn close_clock(state: &mut Lens) -> Task<Message> {
    state
        .datemenu
        .take()
        .map_or_else(Task::none, |menu| Task::done(Message::Close(menu.id)))
}

/// A button or the switch of the clock menu.
fn clock_event(state: &mut Lens, event: datemenu::Event) -> Task<Message> {
    match event {
        datemenu::Event::Previous | datemenu::Event::Next => {
            if let Some(menu) = state.datemenu.as_mut() {
                menu.month = if event == datemenu::Event::Next {
                    menu.month.next()
                } else {
                    menu.month.previous()
                };
            }
            Task::none()
        }
        datemenu::Event::Clear => {
            let effects = state.notices.clear();
            apply(state, effects)
        }
        datemenu::Event::Quiet(on) => {
            state.notices.quiet = on;
            report(notice::keep_quiet(on));
            Task::none()
        }
    }
}

/// A notification came in: its icon is found and its words are cut to fit here, once, and then it
/// shows under the bar and goes in the list.
fn notified(state: &mut Lens, notification: Notification) -> Task<Message> {
    let icon = banner::icon(&state.apps, &notification);
    let (summary, body, lines) = banner::texts(&notification, icon.is_some());
    let (row_summary, row_body) = datemenu::texts(&notification, icon.is_some());
    let height = banner::height(lines, notification.actions.len());
    let fitted = Fitted {
        icon,
        summary,
        body,
        lines,
        row_summary,
        row_body,
    };
    let time = clock::minute(&state.clock).to_string();
    let effects = state.notices.arrive(notification, fitted, height, &time);
    apply(state, effects)
}

/// A press, a button or the pointer on a notification on screen.
fn banner_event(state: &mut Lens, event: banner::Event) -> Task<Message> {
    let effects = match event {
        banner::Event::Press(id) => state.notices.act(id, None),
        banner::Event::Close(id) => state.notices.dismiss(id),
        banner::Event::Action(id, key) => state.notices.act(id, Some(&key)),
        banner::Event::Hover(id, over) => state.notices.hover(id, over),
    };
    apply(state, effects)
}

/// Do what the notifications changing asks for: surfaces to open, move, resize or close, signals
/// for the bus, and the time a notification stays.
fn apply(state: &Lens, effects: Vec<Effect>) -> Task<Message> {
    let mut tasks = Vec::new();
    for effect in effects {
        match effect {
            Effect::Open(id, height, top) => {
                tasks.push(Task::done(Message::OpenBanner(id, height, top)));
            }
            Effect::Move(id, top) => tasks.push(Task::done(Message::Place(id, top))),
            Effect::Resize(id, height) => {
                tasks.push(Task::done(Message::Resize(id, banner::WIDTH, height)));
            }
            Effect::Close(id) => tasks.push(Task::done(Message::Close(id))),
            Effect::Signal(signal) => {
                if let Some(outbox) = &state.outbox {
                    outbox.send(signal);
                }
            }
            Effect::Time(id, epoch) => {
                tasks.push(later(notice::SHOWN, Message::Expire(id, epoch)));
            }
        }
    }
    Task::batch(tasks)
}

/// A message after a while, from a thread that sleeps: the executor has no timer of its own.
fn later(delay: Duration, message: Message) -> Task<Message> {
    Task::perform(
        async move {
            let (sender, receiver) = iced::futures::channel::oneshot::channel();
            std::thread::spawn(move || {
                std::thread::sleep(delay);
                let _ = sender.send(());
            });
            let _ = receiver.await;
        },
        move |()| message,
    )
}

/// A volume or a brightness key was pressed: the popup shows the level for a second, or for a
/// second more when it is already up, and the bar follows at once.
fn show_popup(state: &mut Lens, level: Level) -> Task<Message> {
    match level {
        Level::Volume { level, muted } => {
            state.status.volume = Some(Volume {
                level: u16::from(level),
                muted,
            });
        }
        Level::Brightness(level) => state.status.brightness = Some(level),
    }
    state.keys += 1;
    let key = state.keys;
    let timer = later(popup::SHOWN, Message::PopupDone(key));
    if let Some(shown) = state.popup.as_mut() {
        shown.level = level;
        shown.epoch = key;
        return timer;
    }
    let id = window::Id::unique();
    state.popup = Some(Popup {
        id,
        level,
        epoch: key,
    });
    Task::batch([Task::done(Message::OpenPopup(id)), timer])
}

/// Whether the compositor closed this menu a moment ago. The press that took the keyboard away
/// from it may be on its own button, whose click then arrives after it closed.
fn recently(state: &Lens, which: Closed) -> bool {
    state
        .dismissed
        .is_some_and(|(closed, when)| closed == which && when.elapsed() < REOPEN)
}

/// Open the system menu when it is closed, close it when it is open. A dialog holds the keyboard,
/// so nothing opens over it.
fn toggle_system(state: &mut Lens) -> Task<Message> {
    if state.system.is_some() {
        return close_system(state);
    }
    if state.dialog.is_some() || recently(state, Closed::System) {
        return Task::none();
    }
    let id = window::Id::unique();
    let height = system::height(&system::parts(&state.status, None));
    state.system = Some(system::Menu::new(id, height));
    // the backlight is read as the menu opens, and the card is asked to look for networks, so the
    // list is fresh by the time the owner reads it
    let reading = Task::perform(async { status::brightness() }, Message::Brightness);
    let device = state
        .status
        .network
        .as_ref()
        .and_then(|picture| picture.wireless.as_ref())
        .map(|wireless| wireless.path.clone());
    if let Some(device) = device {
        // what the scan finds comes back through NetworkManager's signals
        std::thread::spawn(move || network::scan(&device));
    }
    Task::batch([Task::done(Message::OpenSystem(id, height)), reading])
}

fn close_system(state: &mut Lens) -> Task<Message> {
    state
        .system
        .take()
        .map_or_else(Task::none, |menu| Task::done(Message::Close(menu.id)))
}

/// A row, a switch or a slider of the system menu.
fn system_event(state: &mut Lens, event: system::Event) -> Task<Message> {
    use system::Event;
    match event {
        Event::Volume(level) => {
            if let Some(menu) = state.system.as_mut() {
                menu.volume = Some(level);
            }
            state.status.volume = Some(Volume {
                level: u16::from(level),
                muted: false,
            });
            state.volume.send(level);
            Task::none()
        }
        Event::VolumeSet => {
            if let Some(menu) = state.system.as_mut() {
                menu.volume = None;
            }
            Task::none()
        }
        Event::Mute => off_thread(|| sound::toggle_mute(Side::Output), Message::Acted),
        Event::Brightness(level) => {
            if let Some(menu) = state.system.as_mut() {
                menu.brightness = Some(level);
            }
            state.status.brightness = Some(level);
            state.brightness.send(level);
            Task::none()
        }
        Event::BrightnessSet => {
            if let Some(menu) = state.system.as_mut() {
                menu.brightness = None;
            }
            Task::none()
        }
        Event::Wifi(on) => off_thread(move || network::set_wifi(on), Message::Acted),
        Event::Join(at) => join(state, at),
        Event::Bluetooth(on) => {
            let Some(adapter) = state
                .status
                .bluetooth
                .as_ref()
                .map(|bluetooth| bluetooth.adapter.clone())
            else {
                return Task::none();
            };
            off_thread(move || bluetooth::set_powered(&adapter, on), Message::Acted)
        }
        Event::Device(at) => {
            let Some(device) = state
                .status
                .bluetooth
                .as_ref()
                .and_then(|bluetooth| bluetooth.devices.get(at))
                .cloned()
            else {
                return Task::none();
            };
            if let Some(menu) = state.system.as_mut() {
                let doing = if device.connected {
                    "Disconnecting"
                } else {
                    "Connecting to"
                };
                menu.error = None;
                menu.notice = Some(format!("{doing} {}", device.name));
            }
            off_thread(
                move || bluetooth::connect(&device.path, !device.connected),
                Message::Acted,
            )
        }
        Event::Lock => {
            // the lock screen covers everything, the menu included, and comes back to a desktop
            // with the menu closed
            let closing = close_system(state);
            Task::batch([closing, off_thread(session::lock, Message::Acted)])
        }
        Event::LogOut => ask_first(state, &["power", "logout"]),
        Event::Restart => ask_first(state, &["power", "reboot"]),
        Event::ShutDown => ask_first(state, &["power", "off"]),
    }
}

/// A network in the system menu was picked. One that needs a password nobody saved asks for it in
/// a dialog; any other is joined at once.
fn join(state: &mut Lens, at: usize) -> Task<Message> {
    let picked = state
        .status
        .network
        .as_ref()
        .and_then(|picture| picture.wireless.as_ref())
        .and_then(|wireless| {
            wireless
                .networks
                .get(at)
                .map(|network| (wireless.path.clone(), network.clone()))
        });
    let Some((device, network)) = picked else {
        return Task::none();
    };
    if network.active {
        return Task::none();
    }
    if network.security.joinable() && network.security.secured() && network.saved.is_none() {
        return open_dialog(state, Ask::Password { network, device });
    }
    let Some(menu) = state.system.as_mut() else {
        return Task::none();
    };
    if !network.security.joinable() {
        menu.notice = None;
        menu.error = Some(format!(
            "{} asks for a user name, which the menu cannot do yet.",
            network.name
        ));
        return Task::none();
    }
    menu.error = None;
    menu.notice = Some(format!("Connecting to {}", network.name));
    off_thread(
        move || {
            let joining = network::join(&device, &network, None)?;
            network::wait(&joining, JOIN_WAIT)
                .map_err(|_| format!("Could not connect to {}.", network.name))
        },
        Message::Acted,
    )
}

/// Ask before a command that ends the session or the machine's run.
fn ask_first(state: &mut Lens, words: &[&str]) -> Task<Message> {
    match os::parse(words) {
        Some(Ok(action)) => open_dialog(state, Ask::Command(action)),
        _ => Task::none(),
    }
}

/// Open a dialog in the middle of the screen. The system menu closes: the dialog takes the
/// keyboard, and the question is the owner's whole attention.
fn open_dialog(state: &mut Lens, ask: Ask) -> Task<Message> {
    let closing = close_system(state);
    if state.dialog.is_some() {
        return closing;
    }
    let found = Dialog::new(window::Id::unique(), ask);
    let opening = Task::done(Message::OpenDialog(found.id, found.height()));
    state.dialog = Some(found);
    Task::batch([closing, opening])
}

fn close_dialog(state: &mut Lens) -> Task<Message> {
    state
        .dialog
        .take()
        .map_or_else(Task::none, |found| Task::done(Message::Close(found.id)))
}

/// A dialog's field or buttons.
fn dialog_event(state: &mut Lens, event: dialog::Event) -> Task<Message> {
    let Some(found) = state.dialog.as_mut() else {
        return Task::none();
    };
    match event {
        dialog::Event::Input(value) => {
            found.input = value;
            found.error = None;
            Task::none()
        }
        dialog::Event::Cancel => close_dialog(state),
        dialog::Event::Confirm => {
            if !found.ready() {
                return Task::none();
            }
            found.busy = true;
            found.error = None;
            let id = found.id;
            match found.ask.clone() {
                Ask::Command(action) => off_thread(
                    move || os::run(&action).map(|_| ()),
                    move |result| Message::DialogDone(id, result),
                ),
                Ask::Password { network, device } => {
                    let password = found.input.clone();
                    off_thread(
                        move || {
                            let joining = network::join(&device, &network, Some(&password))?;
                            network::wait(&joining, JOIN_WAIT).map_err(|_| {
                                format!(
                                    "Could not connect to {}. Check the password and try again.",
                                    network.name
                                )
                            })
                        },
                        move |result| Message::DialogDone(id, result),
                    )
                }
            }
        }
    }
}

/// What a dialog asked for finished. It closes when it worked, and says what went wrong when it
/// did not, with the cursor back in its field.
fn dialog_done(state: &mut Lens, id: window::Id, result: Result<(), String>) -> Task<Message> {
    let Some(found) = state.dialog.as_mut().filter(|found| found.id == id) else {
        return Task::none();
    };
    match result {
        Ok(()) => close_dialog(state),
        Err(why) => {
            found.busy = false;
            found.error = Some(why);
            dialog::focus_field()
        }
    }
}

/// Run something that asks a service or a program on a thread of its own, so the shell keeps
/// drawing while it waits, and hand back how it went.
fn off_thread<W, D>(work: W, done: D) -> Task<Message>
where
    W: FnOnce() -> Result<(), String> + Send + 'static,
    D: Fn(Result<(), String>) -> Message + Send + 'static,
{
    Task::perform(
        async move {
            let (sender, receiver) = iced::futures::channel::oneshot::channel();
            std::thread::spawn(move || {
                let _ = sender.send(work());
            });
            receiver
                .await
                .unwrap_or_else(|_| Err("It stopped before it finished.".to_string()))
        },
        done,
    )
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

/// Escape cancels a dialog and closes the system menu. In the Applications menu it clears the field
/// first, the way a search entry does, and closes the menu when there is nothing left to clear.
fn escape(state: &mut Lens) -> Task<Message> {
    if state.dialog.is_some() {
        return close_dialog(state);
    }
    if state.system.is_some() {
        return close_system(state);
    }
    if state.datemenu.is_some() {
        return close_clock(state);
    }
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
/// when anything outside it is clicked; when the Applications menu or a password dialog takes it,
/// the cursor goes in the field.
fn focused(state: &mut Lens, id: window::Id, has: bool) -> Task<Message> {
    if let Some(found) = state.dialog.as_ref().filter(|found| found.id == id) {
        return if has && matches!(found.ask, Ask::Password { .. }) {
            dialog::focus_field()
        } else {
            Task::none()
        };
    }
    if state.system.as_ref().is_some_and(|menu| menu.id == id) {
        if has {
            return Task::none();
        }
        state.dismissed = Some((Closed::System, Instant::now()));
        return close_system(state);
    }
    if state.datemenu.as_ref().is_some_and(|menu| menu.id == id) {
        if has {
            return Task::none();
        }
        state.dismissed = Some((Closed::Clock, Instant::now()));
        return close_clock(state);
    }
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
        state.dismissed = Some((Closed::Applications, Instant::now()));
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
        Command::Popup(level) => show_popup(state, level),
        Command::Record(recording) => record(state, &recording),
        Command::Look => look(state),
        // answered on the socket's own thread, from the lines remember() keeps
        Command::State => Task::none(),
    }
}

/// The appearance settings changed. The shell reads the theme and the accent again and draws with
/// them; the desktop, the apps and the lock screen follow through the files Settings wrote.
fn look(state: &mut Lens) -> Task<Message> {
    state.theme = appearance::Theme::read();
    state.accent = appearance::Accent::read();
    state.look = crate::theme::palette(state.theme, state.accent);
    remember(state);
    let text = appearance::text();
    if SCALE.swap(text, Ordering::Relaxed) == text {
        return Task::none();
    }
    // a menu is made when it opens and is asked for at the new size then; the bar and the dock are
    // there all session, so each is told its height and how much of the screen it keeps
    let mut work = vec![
        Task::done(Message::Resize(state.dock.id, 0, dock::HEIGHT)),
        Task::done(Message::Reserve(state.dock.id, dock::HEIGHT)),
    ];
    if let Some(id) = state.bar {
        work.push(Task::done(Message::Resize(id, 0, bar::HEIGHT)));
        work.push(Task::done(Message::Reserve(id, bar::HEIGHT)));
    }
    Task::batch(work)
}

/// The screen recorder started or stopped. While it runs the bar carries the mark every desktop
/// puts up for it; when it stops, a notification names the file it left behind, and a second
/// recording's notification takes the place of the first.
fn record(state: &mut Lens, recording: &Recording) -> Task<Message> {
    match recording {
        Recording::On(file) => {
            state.recording = Some(file.clone());
            Task::none()
        }
        Recording::Off(file) => {
            state.recording = None;
            Task::done(Message::Notified(Notification {
                id: u32::MAX,
                app: "Lens".to_string(),
                icon: Some(bar::RECORDING.to_string()),
                entry: None,
                summary: "Screen recording saved".to_string(),
                body: file.clone(),
                actions: Vec::new(),
                default: false,
                urgency: notice::Urgency::Normal,
                transient: false,
            }))
        }
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

/// Ask the compositor for a taller or shorter menu when one changed shape: the Applications menu as
/// its list grows, the system menu as networks and devices come and go, the clock menu as
/// notifications come in and are cleared.
fn resize(state: &mut Lens) -> Task<Message> {
    let mut tasks = Vec::new();
    if let Some(menu) = state.menu.as_mut() {
        let wanted = menu.wanted_height();
        if wanted != menu.height {
            menu.height = wanted;
            tasks.push(Task::done(Message::Resize(menu.id, menu::WIDTH, wanted)));
        }
    }
    if let Some(menu) = state.system.as_mut() {
        let wanted = system::height(&system::parts(&state.status, Some(&*menu)));
        if wanted != menu.height {
            menu.height = wanted;
            tasks.push(Task::done(Message::Resize(menu.id, system::WIDTH, wanted)));
        }
    }
    if let Some(menu) = state.datemenu.as_mut() {
        let wanted = datemenu::height(state.notices.kept.len());
        if wanted != menu.height {
            menu.height = wanted;
            tasks.push(Task::done(Message::Resize(
                menu.id,
                datemenu::WIDTH,
                wanted,
            )));
        }
    }
    Task::batch(tasks)
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
    let status = &state.status;
    line("clock", &state.clock);
    line("theme", state.theme.word());
    line("accent", state.accent.word());
    line("text", &SCALE.load(Ordering::Relaxed).to_string());
    line("apps", &state.apps.len().to_string());
    line("network", &status::network_word(status.network.as_ref()));
    line(
        "volume",
        &status
            .volume
            .map_or_else(|| "none".to_string(), Volume::word),
    );
    line(
        "battery",
        &status
            .battery
            .map_or_else(|| "none".to_string(), status::battery_word),
    );
    line(
        "brightness",
        &status
            .brightness
            .map_or_else(|| "none".to_string(), |level| level.to_string()),
    );
    line("wired", &status::wired_word(status.network.as_ref()));
    line("wifi", &status::wifi_word(status.network.as_ref()));
    line(
        "bluetooth",
        &status::bluetooth_word(status.bluetooth.as_ref()),
    );
    match &state.system {
        None => line("system", "closed"),
        Some(menu) => line("system", &format!("open {}x{}", system::WIDTH, menu.height)),
    }
    match &state.dialog {
        None => line("dialog", "closed"),
        Some(found) => line("dialog", &found.title()),
    }
    notices_lines(state, &mut line);
    match &state.datemenu {
        None => line("clock-menu", "closed"),
        Some(menu) => line(
            "clock-menu",
            &format!("open {}x{}", datemenu::WIDTH, menu.height),
        ),
    }
    match &state.popup {
        None => line("popup", "closed"),
        Some(shown) => line("popup", &shown.level.words()),
    }
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

/// The lines about notifications: how many are on screen and how many are kept, the size of each
/// on screen from the top, the summary of the newest kept, and Do not disturb.
fn notices_lines(state: &Lens, line: &mut impl FnMut(&str, &str)) {
    let notices = &state.notices;
    line(
        "notifications",
        &format!("{} {}", notices.banners.len(), notices.kept.len()),
    );
    let sizes: Vec<String> = notices
        .banners
        .iter()
        .map(|shown| format!("{}x{}", banner::WIDTH, shown.height))
        .collect();
    line(
        "banners",
        &if sizes.is_empty() {
            "none".to_string()
        } else {
            sizes.join(" ")
        },
    );
    line(
        "latest",
        notices
            .kept
            .first()
            .map_or("none", |kept| kept.notification.summary.as_str()),
    );
    line("do-not-disturb", if notices.quiet { "on" } else { "off" });
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
    if let Some(found) = state.dialog.as_ref().filter(|found| found.id == id) {
        return dialog::view(state.look, found);
    }
    if let Some(menu) = state.system.as_ref().filter(|menu| menu.id == id) {
        return system::view(state.look, &state.status, menu);
    }
    if let Some(menu) = state.datemenu.as_ref().filter(|menu| menu.id == id) {
        return datemenu::view(state.look, menu, state.today, state.first, &state.notices);
    }
    if let Some(shown) = state.notices.banners.iter().find(|shown| shown.id == id) {
        return banner::view(state.look, shown);
    }
    if let Some(shown) = state.popup.as_ref().filter(|shown| shown.id == id) {
        return popup::view(state.look, shown);
    }
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
        bar::Open {
            applications: state.menu.is_some(),
            clock: state.datemenu.is_some(),
            system: state.system.is_some(),
        },
        state.notices.quiet,
        state.recording.is_some(),
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
