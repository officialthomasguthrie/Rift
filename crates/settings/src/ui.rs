//! The window: a header bar along the top, a sidebar of pages down the left, and one page beside
//! it, the way GNOME Settings and Windows Settings are laid out. Drawn with iced in software, in
//! the dark or light colours the owner has chosen.

use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use iced::futures::channel::mpsc;
use iced::widget::{button, column, container, row, space, text};
use iced::{
    Border, Center, Color, Element, Fill, Length, Size, Subscription, Task, Theme, theme, window,
};
use librift::appearance::{Accent, Look, Scheme, Theme as Mode};
use librift::bluetooth as bluetooth_picture;
use librift::boot::Style;
use librift::network;
use librift::orbit::Host;

use crate::control::{self, Command};
use crate::net::Joining;
use crate::page::Page;
use crate::theme::{Colors, colors};
use crate::widgets::{BOLD, FONT, TEXT_SIZE, TITLE_SIZE, scroll};
use crate::{about, appearance, bluetooth, displays, icons, net, watch};

/// What the window calls itself: the name of its desktop entry, which the dock, the compositor and
/// the boot test all know it by.
const APP_ID: &str = "dev.rift.Settings";

/// How wide the sidebar is.
const SIDEBAR: f32 = 208.0;
/// How tall the header bar is. It is the title bar of the window, which the app draws itself.
const HEADER: f32 = 45.0;
/// How tall a row of the sidebar is.
const ROW: f32 = 34.0;

/// What the app is started with.
#[derive(Debug, Default)]
pub struct Start {
    /// The page to open on, when one was named.
    pub page: Option<Page>,
    /// Where to save a picture of the window once it has drawn, and then quit.
    pub screenshot: Option<PathBuf>,
}

/// The window's state.
pub struct Settings {
    /// The page that is up.
    pub page: Page,
    /// Dark or light, the accent, the gaps, the radius and the wallpaper.
    pub look: Look,
    /// Whether the terminal greets the first shell of a session.
    pub greeting: bool,
    /// How the next boot of this drive looks, once Vault has said. It is on the esp, not in home,
    /// so Vault is the one that reads and writes it.
    pub boot: Option<Result<Style, String>>,
    /// The wallpapers to choose from: the photographs Rift ships, then the flat colours.
    pub choices: Vec<appearance::Choice>,
    /// What Orbit says about this machine, once it has answered.
    pub host: Option<Result<Host, String>>,
    /// What `NetworkManager` says about the cable and the wireless networks, once it has answered
    /// and after every change it makes while the window is open.
    pub network: Option<Result<network::Picture, String>>,
    /// What `BlueZ` says, the same way. `Ok(None)` is a machine with no adapter.
    pub bluetooth: Option<Result<Option<bluetooth_picture::Picture>, String>>,
    /// The network being joined that asks for a password, and what has been typed for it.
    pub joining: Option<Joining>,
    /// What is happening: a join, or a device being connected.
    pub doing: Option<String>,
    /// Whether the card has been asked to sweep since the Wi-Fi page came up.
    swept: bool,
    /// What the system calls itself, from os-release.
    pub release: String,
    /// The last setting that could not be written.
    pub problem: Option<String>,
    /// Where `--screenshot` saves a picture of the window.
    screenshot: Option<PathBuf>,
}

/// What a press, a drag or a line on the socket asks for.
#[derive(Debug, Clone)]
pub enum Message {
    /// Show this page.
    Show(Page),
    /// Dark or light.
    Mode(Mode),
    /// One of the nine accents.
    Accent(Accent),
    /// The wallpaper at this place in the list.
    Wallpaper(usize),
    /// The gap slider moved, and where it was let go.
    Gaps(u32),
    /// The text size slider moved.
    Text(u32),
    /// One of the terminal colour schemes.
    Terminal(Scheme),
    /// One of the two boot styles.
    Boot(Style),
    /// The size one screen is drawn at, from the Displays page.
    Scale(String, u32),
    /// What `NetworkManager` says now. It is the biggest thing a message carries, so it travels
    /// behind a pointer.
    Network(Box<Result<network::Picture, String>>),
    /// What `BlueZ` says now.
    Bluetooth(Result<Option<bluetooth_picture::Picture>, String>),
    /// The Wi-Fi switch.
    Wifi(bool),
    /// The network at this place in the list was pressed.
    Join(usize),
    /// The password field was typed into.
    Password(String),
    /// The password was given.
    Joined,
    /// The card was asked to sweep for networks again.
    Scanned,
    /// The Bluetooth switch.
    Power(bool),
    /// The paired device at this place in the list was pressed.
    Device(usize),
    /// How a join, a switch or a connect went.
    Acted(Result<(), String>),
    /// What Orbit answered after a screen's size was written: the machine again, or why not.
    Scaled(Result<Host, String>),
    /// What Vault answered about the boot style, after reading it or after writing it.
    BootStyle(Result<Style, String>),
    /// The corner radius slider moved.
    Radius(u32),
    /// A slider was let go, so the look is written.
    Wrote,
    /// The terminal greeting.
    Greeting(bool),
    /// What Orbit answered about this machine.
    Host(Result<Host, String>),
    /// A line from the socket.
    Said(Command),
    /// The picture of the window `--screenshot` asked for.
    Shot(window::Screenshot),
    /// The window was asked to close.
    Close,
}

/// Open the window and run until it is closed.
///
/// # Errors
///
/// When the window cannot be opened.
pub fn run(start: Start) -> iced::Result {
    let whole_page = start.screenshot.is_some();
    iced::application(move || boot(&start), update, view)
        .title("Settings")
        .theme(|state: &Settings| {
            let look = state.colors();
            Theme::custom(
                "Rift",
                theme::Palette {
                    background: look.page,
                    text: look.text,
                    primary: look.accent,
                    success: look.accent,
                    warning: look.accent,
                    danger: look.error,
                },
            )
        })
        .subscription(subscription)
        // the window follows the interface text size the way a GTK app does, since it is not one
        .scale_factor(|state: &Settings| {
            f32::from(u16::try_from(state.look.text).unwrap_or(100)) / 100.0
        })
        .default_font(FONT)
        .settings(iced::Settings {
            id: Some(APP_ID.to_string()),
            default_font: FONT,
            default_text_size: TEXT_SIZE.into(),
            ..iced::Settings::default()
        })
        .window(window(whole_page))
        .run()
}

/// The window itself. On Wayland the app id comes from the platform settings and nowhere else, and
/// it is the name of the desktop entry, which is how the dock and the compositor know the window.
/// A window that is only there to have its picture taken is as tall as a whole page, so the
/// picture holds one without anyone scrolling it.
fn window(screenshot: bool) -> window::Settings {
    #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
    let mut settings = window::Settings {
        size: Size::new(920.0, if screenshot { 1200.0 } else { 660.0 }),
        min_size: Some(Size::new(600.0, 420.0)),
        exit_on_close_request: false,
        // the app draws its own title bar, the way the GTK apps of the session do. left to itself
        // winit draws an Adwaita frame of its own around the window, in its own colours
        decorations: false,
        ..window::Settings::default()
    };
    #[cfg(target_os = "linux")]
    {
        settings.platform_specific.application_id = APP_ID.to_string();
    }
    settings
}

fn boot(start: &Start) -> (Settings, Task<Message>) {
    let state = Settings {
        page: start.page.unwrap_or(Page::FIRST),
        look: Look::read(),
        greeting: librift::appearance::greeting(),
        boot: None,
        choices: appearance::choices(),
        host: None,
        release: about::release(),
        problem: None,
        screenshot: start.screenshot.clone(),
        ..Settings::bare()
    };
    let mut work = vec![about::ask_orbit(), appearance::ask_vault()];
    if start.screenshot.is_some() {
        work.push(shoot());
    }
    (state, Task::batch(work))
}

/// For `--screenshot`: wait a moment for the window to draw itself, then take the picture.
fn shoot() -> Task<Message> {
    Task::perform(
        async { std::thread::sleep(Duration::from_millis(800)) },
        |()| (),
    )
    .then(|()| window::oldest())
    .and_then(window::screenshot)
    .map(Message::Shot)
}

/// Write a picture of the window as a png.
fn save(path: &Path, shot: &window::Screenshot) -> Result<(), String> {
    let (wide, tall) = (shot.size.width, shot.size.height);
    image::RgbaImage::from_raw(wide, tall, shot.rgba.to_vec())
        .ok_or_else(|| format!("the picture is not {wide} by {tall}"))?
        .save(path)
        .map_err(|e| format!("Could not write {}: {e}", path.display()))
}

impl Settings {
    /// A window that has asked for nothing yet, which the pages' own tests build on.
    #[must_use]
    pub fn bare() -> Self {
        Self {
            page: Page::FIRST,
            look: Look::default(),
            greeting: false,
            boot: None,
            choices: Vec::new(),
            host: None,
            network: None,
            bluetooth: None,
            joining: None,
            doing: None,
            swept: false,
            release: String::new(),
            problem: None,
            screenshot: None,
        }
    }

    /// The colours the window is drawn in.
    pub fn colors(&self) -> Colors {
        colors(self.look.theme, self.look.accent)
    }

    /// The line `rift-settings --state` prints for each setting.
    fn state(&self) -> String {
        let look = &self.look;
        [
            format!("page {}", self.page.word()),
            format!("theme {}", look.theme.word()),
            format!("accent {}", look.accent.word()),
            format!("wallpaper {}", look.wallpaper),
            format!("gaps {}", look.gaps),
            format!("radius {}", look.radius),
            format!("text {}", look.text),
            format!("terminal {}", look.terminal.word()),
            format!("greeting {}", if self.greeting { "on" } else { "off" }),
        ]
        .into_iter()
        // the boot style is Vault's to answer, and the screens are Orbit's, so each is printed
        // once it has
        .chain(
            self.boot
                .as_ref()
                .and_then(|answered| answered.as_ref().ok())
                .map(|style| format!("boot {}", style.word())),
        )
        .chain(
            self.host
                .as_ref()
                .and_then(|answered| answered.as_ref().ok())
                .into_iter()
                .flat_map(|host| {
                    host.outputs.iter().map(|output| {
                        format!(
                            "screen {} {}x{} scale {}",
                            output.connector, output.width, output.height, output.scale
                        )
                    })
                }),
        )
        // and the same for the network and Bluetooth, which the window follows while it is open
        .chain(net::state(self))
        .chain(bluetooth::state(self))
        .collect::<Vec<_>>()
        .join("\n")
            + "\n"
    }

    /// Write the look the owner has just changed, hand it to the apps and the desktop, and tell
    /// the shell to draw with it.
    fn wrote(&mut self) {
        self.problem = self.look.save().err();
        poke_the_shell();
    }
}

/// Tell the shell that is running to read the appearance settings again. It draws the bar, the
/// dock and the menus, so the accent and the theme reach it this way rather than through a file it
/// would have to watch.
fn poke_the_shell() {
    let _ = std::process::Command::new("lens")
        .arg("--look")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

fn update(state: &mut Settings, message: Message) -> Task<Message> {
    match message {
        Message::Show(page) => return show(state, page),
        Message::Mode(mode) => {
            state.look.theme = mode;
            state.wrote();
        }
        Message::Accent(accent) => {
            state.look.accent = accent;
            state.wrote();
        }
        Message::Wallpaper(at) => {
            if let Some(choice) = state.choices.get(at) {
                state.look.wallpaper = choice.wallpaper.clone();
                state.wrote();
            }
        }
        Message::Gaps(gaps) => state.look.gaps = gaps,
        Message::Radius(radius) => state.look.radius = radius,
        Message::Text(text) => state.look.text = text,
        Message::Terminal(scheme) => {
            state.look.terminal = scheme;
            state.wrote();
        }
        Message::Boot(style) => return appearance::write_style(style),
        Message::Scale(connector, scale) => return displays::set_scale(connector, scale),
        Message::Network(picture) => {
            state.network = Some(*picture);
            // a network that has come up is no longer one waiting for a password
            if state
                .joining
                .as_ref()
                .is_some_and(|asked| on_network(state, asked.network.ssid.as_slice()))
            {
                state.joining = None;
                state.doing = None;
            }
            if state.page == Page::Wifi && !state.swept {
                state.swept = true;
                return net::scan(state);
            }
        }
        Message::Bluetooth(answered) => state.bluetooth = Some(answered),
        Message::Wifi(on) => {
            state.problem = None;
            return net::set_wifi(state, on);
        }
        Message::Join(at) => return net::join(state, at),
        Message::Password(typed) => {
            if let Some(asked) = state.joining.as_mut() {
                asked.password = typed;
                state.problem = None;
            }
        }
        Message::Joined => return net::joined(state),
        Message::Scanned => {}
        Message::Power(on) => {
            state.problem = None;
            return bluetooth::set_powered(state, on);
        }
        Message::Device(at) => return bluetooth::connect(state, at),
        Message::Acted(Ok(())) => {
            state.problem = None;
            state.doing = None;
            if let Some(asked) = state.joining.as_mut() {
                asked.busy = false;
            }
        }
        Message::Acted(Err(why)) => {
            state.doing = None;
            state.problem = Some(why);
            if let Some(asked) = state.joining.as_mut() {
                asked.busy = false;
                return crate::widgets::focus(net::FIELD);
            }
        }
        Message::Scaled(Ok(host)) => {
            state.problem = None;
            state.host = Some(Ok(host));
        }
        Message::Scaled(Err(why)) => state.problem = Some(why),
        Message::BootStyle(answered) => state.boot = Some(answered),
        Message::Wrote => state.wrote(),
        Message::Greeting(on) => {
            state.greeting = on;
            state.problem = librift::appearance::set_greeting(on).err();
        }
        Message::Host(host) => state.host = Some(host),
        Message::Said(command) => return said(state, command),
        Message::Shot(shot) => {
            if let Some(path) = &state.screenshot
                && let Err(why) = save(path, &shot)
            {
                eprintln!("rift-settings: {why}");
            }
            return iced::exit();
        }
        Message::Close => return iced::exit(),
    }
    Task::none()
}

/// Show a page. The Wi-Fi page asks the card to sweep as it comes up, so the list is what is
/// around now rather than what was around when the window opened.
fn show(state: &mut Settings, page: Page) -> Task<Message> {
    if state.page == page {
        return Task::none();
    }
    state.page = page;
    state.problem = None;
    if page == Page::Wifi {
        state.swept = true;
        return net::scan(state);
    }
    state.swept = false;
    Task::none()
}

/// Whether the machine is on the network with this name as the radio sends it.
fn on_network(state: &Settings, ssid: &[u8]) -> bool {
    state
        .network
        .as_ref()
        .and_then(|answered| answered.as_ref().ok())
        .and_then(|picture| picture.wireless.as_ref())
        .and_then(librift::network::Wireless::active)
        .is_some_and(|network| network.ssid == ssid)
}

/// A line from the socket. Setting something over it does what pressing it on the page does.
fn said(state: &mut Settings, command: Command) -> Task<Message> {
    match command {
        Command::Page(word) => {
            if let Some(page) = Page::from_word(&word) {
                return show(state, page);
            }
        }
        Command::Set(name, value) => return set(state, &name, &value),
        // answered on the socket's own thread
        Command::State => {}
    }
    Task::none()
}

fn set(state: &mut Settings, name: &str, value: &str) -> Task<Message> {
    let number = |most: u32| value.trim().parse::<u32>().ok().map(|n| n.min(most));
    let on = |value: &str| !value.trim().eq_ignore_ascii_case("off");
    match name {
        "theme" => Task::done(Message::Mode(Mode::from_setting(value))),
        "accent" => Task::done(Message::Accent(Accent::from_setting(value))),
        "gaps" => number(librift::appearance::GAPS_MOST).map_or_else(Task::none, |gaps| {
            state.look.gaps = gaps;
            Task::done(Message::Wrote)
        }),
        "radius" => number(librift::appearance::RADIUS_MOST).map_or_else(Task::none, |radius| {
            state.look.radius = radius;
            Task::done(Message::Wrote)
        }),
        "text" => number(librift::appearance::TEXT_MOST).map_or_else(Task::none, |text| {
            state.look.text = text.max(librift::appearance::TEXT_LEAST);
            Task::done(Message::Wrote)
        }),
        "terminal" => Task::done(Message::Terminal(Scheme::from_setting(value))),
        "boot" => Task::done(Message::Boot(Style::from_setting(value))),
        // the screen is named first, then the size: `--set scale eDP-1 2`
        "scale" => value
            .trim()
            .rsplit_once(' ')
            .and_then(|(screen, size)| Some((screen.trim().to_string(), size.trim().parse().ok()?)))
            .map_or_else(Task::none, |(screen, scale)| {
                Task::done(Message::Scale(screen, scale))
            }),
        "greeting" => Task::done(Message::Greeting(!value.trim().eq_ignore_ascii_case("off"))),
        "wifi" => Task::done(Message::Wifi(on(value))),
        "bluetooth" => Task::done(Message::Power(on(value))),
        // a network and a paired device are named, since neither list is in an order anyone typed
        "join" => {
            net::named(state, value).map_or_else(Task::none, |at| Task::done(Message::Join(at)))
        }
        "connect" => bluetooth::named(state, value)
            .map_or_else(Task::none, |at| Task::done(Message::Device(at))),
        "password" => Task::batch([
            Task::done(Message::Password(value.to_string())),
            Task::done(Message::Joined),
        ]),
        "wallpaper" => state
            .choices
            .iter()
            .position(|choice| choice.names(value))
            .map_or_else(Task::none, |at| Task::done(Message::Wallpaper(at))),
        _ => Task::none(),
    }
}

fn subscription(_: &Settings) -> Subscription<Message> {
    Subscription::batch([
        window::close_requests().map(|_| Message::Close),
        terminal(),
        watch::network(),
        watch::bluetooth(),
    ])
}

/// The socket in the runtime directory, read on a thread of its own. The state query is answered
/// there, from the lines the window keeps up to date. The subscription is named, because iced tells
/// two of them apart by the type of the stream and the address of the function that makes it.
fn terminal() -> Subscription<Message> {
    Subscription::run_with("terminal", |_| {
        let (sender, receiver) = mpsc::unbounded();
        thread::spawn(move || {
            if let Err(why) = control::serve(|command| {
                if command == Command::State {
                    return Some(kept());
                }
                let _ = sender.unbounded_send(Message::Said(command));
                None
            }) {
                eprintln!("rift-settings: {why}");
            }
        });
        receiver
    })
}

/// What the socket answers a state query with. The view writes it on every frame, so the thread
/// that answers never touches the window's own state.
fn kept() -> String {
    STATE.lock().map_or_else(
        |_| "the window is busy\n".to_string(),
        |state| state.clone(),
    )
}

static STATE: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

fn view(state: &Settings) -> Element<'_, Message> {
    if let Ok(mut kept) = STATE.lock() {
        *kept = state.state();
    }
    let look = state.colors();
    let body = row![
        sidebar(state, look),
        container(page(state, look))
            .width(Fill)
            .height(Fill)
            .style(move |_: &Theme| fill(look.page)),
    ]
    .height(Fill);
    column![header(state, look), body].into()
}

/// The title bar the app draws for itself: the name of the app over the sidebar, the name of the
/// page over the page, and the close button at the right end, the way every GTK app of the session
/// draws its own.
fn header(state: &Settings, look: Colors) -> Element<'_, Message> {
    let name = container(text("Settings").size(TEXT_SIZE).font(BOLD).color(look.text))
        .width(Length::Fixed(SIDEBAR))
        .padding([0, 14])
        .center_y(Fill);
    let title = container(
        text(state.page.label())
            .size(TEXT_SIZE)
            .font(BOLD)
            .color(look.text),
    )
    .width(Fill)
    .center_x(Fill)
    .center_y(Fill);
    let close = button(icons::symbolic(look.text, "window-close-symbolic", 16.0))
        .padding(7)
        .on_press(Message::Close)
        .style(move |_: &Theme, status| button::Style {
            background: Some(
                match status {
                    button::Status::Hovered | button::Status::Pressed => look.hover,
                    _ => look.button,
                }
                .into(),
            ),
            text_color: look.text,
            border: Border {
                radius: 15.0.into(),
                ..Border::default()
            },
            ..button::Style::default()
        });
    let line = container(space().height(1.0).width(Fill)).style(move |_: &Theme| fill(look.line));
    column![
        container(row![name, title, close].align_y(Center).padding([0, 8]))
            .width(Fill)
            .height(Length::Fixed(HEADER))
            .style(move |_: &Theme| fill(look.header)),
        line,
    ]
    .into()
}

/// The pages, one row each, the one that is up in the accent.
fn sidebar(state: &Settings, look: Colors) -> Element<'_, Message> {
    let mut rows = column![].width(Fill).spacing(2).padding([8, 8]);
    for page in Page::ALL {
        let here = page == state.page;
        let colour = if here { look.on_accent } else { look.text };
        rows = rows.push(
            button(
                row![
                    icons::symbolic(colour, page.icon(), 16.0),
                    text(page.label()).size(TEXT_SIZE).color(colour),
                ]
                .align_y(Center)
                .spacing(10),
            )
            .width(Fill)
            .height(Length::Fixed(ROW))
            .padding([0, 10])
            .on_press(Message::Show(page))
            .style(move |_: &Theme, status| button::Style {
                background: Some(
                    match (here, status) {
                        (true, _) => look.accent,
                        (false, button::Status::Hovered | button::Status::Pressed) => look.hover,
                        _ => Color::TRANSPARENT,
                    }
                    .into(),
                ),
                text_color: colour,
                border: Border {
                    radius: 4.0.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }),
        );
    }
    row![
        container(scroll(look, rows).height(Fill))
            .width(Length::Fixed(SIDEBAR))
            .height(Fill)
            .style(move |_: &Theme| fill(look.side)),
        container(space().width(1.0).height(Fill)).style(move |_: &Theme| fill(look.line)),
    ]
    .into()
}

/// The page that is up.
fn page(state: &Settings, look: Colors) -> Element<'_, Message> {
    let inside = match state.page {
        Page::Wifi => net::wifi(state, look),
        Page::Network => net::wired(state, look),
        Page::Bluetooth => bluetooth::view(state, look),
        Page::Appearance => appearance::view(state, look),
        Page::Displays => displays::view(state, look),
        Page::About => about::view(state, look),
        other => nothing_yet(look, other),
    };
    scroll(
        look,
        container(inside).width(Fill).padding(crate::widgets::PAD),
    )
    .height(Fill)
    .into()
}

/// A page for something the system cannot do from here yet: its name and one sentence.
fn nothing_yet<'a>(look: Colors, page: Page) -> Element<'a, Message> {
    column![
        text(page.label())
            .size(TITLE_SIZE)
            .font(BOLD)
            .color(look.text),
        text(page.note()).size(TEXT_SIZE).color(look.dim),
    ]
    .spacing(10)
    .into()
}

/// A container filled with one colour and nothing else.
pub fn fill(colour: Color) -> container::Style {
    container::Style {
        background: Some(colour.into()),
        ..container::Style::default()
    }
}
