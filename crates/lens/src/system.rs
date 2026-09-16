//! The system menu, which the status icons at the right of the bar open. The sound at the top, then
//! the network, Bluetooth and the battery, and the session at the bottom, the way GNOME's menu and
//! the one on Tails put them: a list of rows on the menu's gray inside a border, like every other
//! menu of the shell. A section the machine has nothing for is not there.

use iced::widget::{button, column, container, row, slider, space, text, toggler};
use iced::{Background, Border, Color, Element, Length, Padding, Shadow, Theme, window};
use librift::battery::Battery;
use librift::network::{Link, Network};

use crate::bar;
use crate::icons;
use crate::status::{self, Status};
use crate::theme::Palette;
use crate::ui::Message;

/// How wide the menu is in logical pixels.
pub const WIDTH: u32 = 340;
/// The padding inside the menu, and its margin from the right edge of the screen.
pub const PAD: u32 = 8;
/// The same padding where a widget wants it.
const INSIDE: u16 = 8;
/// One row.
pub const ROW: u32 = 32;
/// The same height where a length is wanted.
const TALL: f32 = 32.0;
/// A slider inside its row.
const SLIDER: f32 = 24.0;
/// The line between two sections, with the space above and under it.
pub const SEPARATOR: u32 = 9;
/// A symbolic icon at the left of a row.
const ICON: f32 = 16.0;
/// The gap between the things in a row.
const GAP: f32 = 8.0;
/// The space at each end of a row, so the icons line up with the mute button's.
const INSET: u16 = 8;
/// How tall a switch is. It is twice as wide.
const SWITCH: f32 = 20.0;
/// The corner of a row under the pointer.
const RADIUS: f32 = 4.0;
/// The most networks the menu lists; the rest are the weakest and Settings has them all.
pub const NETWORKS: usize = 5;
/// The most paired devices it lists.
pub const DEVICES: usize = 3;

/// What a row, a switch or a slider of the menu asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// The volume slider moved.
    Volume(u8),
    /// The volume slider was let go.
    VolumeSet,
    /// The button at the left of the volume slider.
    Mute,
    /// The brightness slider moved.
    Brightness(u8),
    /// The brightness slider was let go.
    BrightnessSet,
    /// The Wi-Fi switch.
    Wifi(bool),
    /// A network in the list.
    Join(usize),
    /// The Bluetooth switch.
    Bluetooth(bool),
    /// A paired device in the list.
    Device(usize),
    /// Lock.
    Lock,
    /// Log out.
    LogOut,
    /// Restart.
    Restart,
    /// Shut down.
    ShutDown,
}

/// One row of the menu, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// The volume slider with the mute button.
    Volume,
    /// The brightness slider.
    Brightness,
    /// The line between two sections.
    Separator,
    /// The cable and its state.
    Wired,
    /// Wi-Fi and its switch.
    Wifi,
    /// A network around, by its place in the list.
    Network(usize),
    /// Wi-Fi is on and sees nothing.
    NoNetworks,
    /// Bluetooth and its switch.
    Bluetooth,
    /// A paired device, by its place in the list.
    Device(usize),
    /// The battery.
    Battery,
    /// What went wrong, or what is happening.
    Line,
    /// Lock.
    Lock,
    /// Log out.
    LogOut,
    /// Restart.
    Restart,
    /// Shut down.
    ShutDown,
}

impl Part {
    /// How tall the row is.
    #[must_use]
    pub const fn height(self) -> u32 {
        match self {
            Self::Separator => SEPARATOR,
            _ => ROW,
        }
    }
}

/// The menu while it is open.
#[derive(Debug)]
pub struct Menu {
    /// The surface it draws on.
    pub id: window::Id,
    /// The height the surface has been told to be.
    pub height: u32,
    /// The volume while the slider is held. A reading from `PipeWire` that arrives meanwhile is of
    /// a value the slider has already left, so it does not move the slider.
    pub volume: Option<u8>,
    /// The brightness while its slider is held.
    pub brightness: Option<u8>,
    /// What went wrong, on the line over the session.
    pub error: Option<String>,
    /// What is happening, on the same line when nothing went wrong.
    pub notice: Option<String>,
}

impl Menu {
    /// A menu that has just opened.
    #[must_use]
    pub const fn new(id: window::Id, height: u32) -> Self {
        Self {
            id,
            height,
            volume: None,
            brightness: None,
            error: None,
            notice: None,
        }
    }

    /// The line over the session: what went wrong, or what is happening.
    #[must_use]
    pub fn line(&self) -> Option<(&str, bool)> {
        self.error
            .as_deref()
            .map(|why| (why, true))
            .or_else(|| self.notice.as_deref().map(|notice| (notice, false)))
    }
}

/// The rows the menu has for what the machine has now, the sections apart by a line.
#[must_use]
pub fn parts(status: &Status, menu: Option<&Menu>) -> Vec<Part> {
    let mut sound = Vec::new();
    if status.volume.is_some() {
        sound.push(Part::Volume);
    }
    if status.brightness.is_some() {
        sound.push(Part::Brightness);
    }
    let mut network = Vec::new();
    if let Some(picture) = &status.network {
        if picture.wired.is_some() {
            network.push(Part::Wired);
        }
        if let Some(wireless) = &picture.wireless {
            network.push(Part::Wifi);
            if picture.wifi && picture.radio {
                if wireless.networks.is_empty() {
                    network.push(Part::NoNetworks);
                }
                network.extend((0..wireless.networks.len().min(NETWORKS)).map(Part::Network));
            }
        }
    }
    let mut bluetooth = Vec::new();
    if let Some(picture) = &status.bluetooth {
        bluetooth.push(Part::Bluetooth);
        if picture.powered {
            bluetooth.extend((0..picture.devices.len().min(DEVICES)).map(Part::Device));
        }
    }
    let battery = if status.battery.is_some() {
        vec![Part::Battery]
    } else {
        Vec::new()
    };
    let line = if menu.and_then(Menu::line).is_some() {
        vec![Part::Line]
    } else {
        Vec::new()
    };
    let session = vec![Part::Lock, Part::LogOut, Part::Restart, Part::ShutDown];
    let mut parts = Vec::new();
    for section in [sound, network, bluetooth, battery, line, session] {
        if section.is_empty() {
            continue;
        }
        if !parts.is_empty() {
            parts.push(Part::Separator);
        }
        parts.extend(section);
    }
    parts
}

/// How tall the menu is with these rows.
#[must_use]
pub fn height(parts: &[Part]) -> u32 {
    2 * PAD + parts.iter().map(|part| part.height()).sum::<u32>()
}

/// What the wired row says about the cable.
#[must_use]
pub const fn wired_word(link: Link) -> &'static str {
    match link {
        Link::Connected => "Connected",
        Link::Connecting => "Connecting",
        Link::Disconnected => "Disconnected",
        Link::Unavailable => "Cable unplugged",
    }
}

/// What the battery row says at its right: the level, and how long it has when that is known.
#[must_use]
pub fn battery_words(battery: &Battery) -> String {
    match battery.time() {
        Some(time) => format!("{}%, {time}", battery.level),
        None => format!("{}%", battery.level),
    }
}

/// The volume as the slider shows it: a sink turned up past full sits at the end.
#[must_use]
pub fn volume_level(status: &Status, menu: &Menu) -> u8 {
    menu.volume.unwrap_or_else(|| {
        status.volume.map_or(0, |volume| {
            u8::try_from(volume.level.min(100)).unwrap_or(100)
        })
    })
}

/// The menu: its rows on the menu's gray inside the border.
pub fn view<'a>(look: Palette, status: &'a Status, menu: &'a Menu) -> Element<'a, Message> {
    let mut rows = column![];
    for part in parts(status, Some(menu)) {
        rows = rows.push(part_view(look, status, menu, part));
    }
    container(rows)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(INSIDE)
        .style(move |_: &Theme| container::Style {
            background: Some(look.menu.into()),
            text_color: Some(look.text),
            border: Border {
                color: look.edge,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn part_view<'a>(
    look: Palette,
    status: &'a Status,
    menu: &'a Menu,
    part: Part,
) -> Element<'a, Message> {
    match part {
        Part::Volume => {
            let level = volume_level(status, menu);
            let muted = status.volume.is_some_and(|volume| volume.muted);
            let icon = status::Volume {
                level: u16::from(level),
                muted,
            }
            .icon();
            slider_row(
                look,
                icon,
                Some(Message::System(Event::Mute)),
                level,
                Event::Volume,
                Event::VolumeSet,
            )
        }
        Part::Brightness => slider_row(
            look,
            "display-brightness-symbolic",
            None,
            menu.brightness.or(status.brightness).unwrap_or(0),
            Event::Brightness,
            Event::BrightnessSet,
        ),
        Part::Separator => separator(look),
        Part::Wired => wired_row(look, status),
        Part::Wifi => wifi_row(look, status),
        Part::Network(at) => status
            .network
            .as_ref()
            .and_then(|picture| picture.wireless.as_ref())
            .and_then(|wireless| wireless.networks.get(at))
            .map_or_else(|| plain(space()), |network| network_row(look, network, at)),
        Part::NoNetworks => plain(
            text("No networks found")
                .size(bar::TEXT_SIZE)
                .color(look.dim),
        ),
        Part::Bluetooth => bluetooth_row(look, status),
        Part::Device(at) => device_row(look, status, at),
        Part::Battery => status.battery.map_or_else(
            || plain(space()),
            |battery| {
                let icon = status::battery_icon(battery);
                plain(line(
                    look,
                    &icon,
                    "Battery",
                    dim(look, &battery_words(&battery)),
                ))
            },
        ),
        Part::Line => {
            let (said, wrong) = menu.line().unwrap_or(("", false));
            let colour = if wrong { look.error } else { look.dim };
            plain(
                container(
                    text(said.to_string())
                        .size(bar::TEXT_SIZE)
                        .color(colour)
                        .wrapping(iced::widget::text::Wrapping::None),
                )
                .width(Length::Fill)
                .clip(true),
            )
        }
        Part::Lock => session_row(look, "system-lock-screen-symbolic", "Lock", Event::Lock),
        Part::LogOut => session_row(look, "system-log-out-symbolic", "Log out", Event::LogOut),
        Part::Restart => session_row(look, "system-reboot-symbolic", "Restart", Event::Restart),
        Part::ShutDown => session_row(
            look,
            "system-shutdown-symbolic",
            "Shut down",
            Event::ShutDown,
        ),
    }
}

/// The cable, and what state it is in.
fn wired_row(look: Palette, status: &Status) -> Element<'static, Message> {
    let link = status
        .network
        .as_ref()
        .and_then(|picture| picture.wired.as_ref())
        .map_or(Link::Unavailable, |wired| wired.link);
    let icon = match link {
        Link::Connected => "network-wired-symbolic",
        Link::Connecting => "network-wired-acquiring-symbolic",
        Link::Disconnected | Link::Unavailable => "network-wired-disconnected-symbolic",
    };
    plain(line(look, icon, "Wired", dim(look, wired_word(link))))
}

/// Wi-Fi and its switch, which a switch on the machine that blocks the radio leaves off and greyed.
fn wifi_row(look: Palette, status: &Status) -> Element<'static, Message> {
    let picture = status.network.as_ref();
    let (on, radio) = picture.map_or((false, false), |picture| (picture.wifi, picture.radio));
    let active = picture
        .and_then(|picture| picture.wireless.as_ref())
        .and_then(|wireless| wireless.active());
    let icon = match active {
        _ if !(on && radio) => "network-wireless-disabled-symbolic".to_string(),
        Some(network) => icons::signal("wireless", network.strength),
        None => "network-wireless-signal-none-symbolic".to_string(),
    };
    let switch = switch(
        look,
        on && radio,
        radio.then_some(|on| Message::System(Event::Wifi(on))),
    );
    plain(line(look, &icon, "Wi-Fi", switch))
}

/// Bluetooth and its switch.
fn bluetooth_row(look: Palette, status: &Status) -> Element<'static, Message> {
    let on = status
        .bluetooth
        .as_ref()
        .is_some_and(|bluetooth| bluetooth.powered);
    let icon = if on {
        "bluetooth-active-symbolic"
    } else {
        "bluetooth-disabled-symbolic"
    };
    let switch = switch(look, on, Some(|on| Message::System(Event::Bluetooth(on))));
    plain(line(look, icon, "Bluetooth", switch))
}

/// A paired device: its kind of icon, its name, and whether it is connected. A press connects it
/// or disconnects it.
fn device_row(look: Palette, status: &Status, at: usize) -> Element<'static, Message> {
    let found = status
        .bluetooth
        .as_ref()
        .and_then(|bluetooth| bluetooth.devices.get(at));
    let Some(device) = found else {
        return plain(space());
    };
    let icon = device
        .icon
        .as_ref()
        .map(|icon| format!("{icon}-symbolic"))
        .filter(|icon| icons::find(icon).is_some())
        .unwrap_or_else(|| "bluetooth-symbolic".to_string());
    let right = dim(look, if device.connected { "Connected" } else { "" });
    pressable(
        look,
        line(look, &icon, &device.name, right),
        Some(Message::System(Event::Device(at))),
    )
}

/// A row with nothing to press, its contents centred on its height.
fn plain<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .width(Length::Fill)
        .height(TALL)
        .padding([0, INSET])
        .align_y(iced::Center)
        .clip(true)
        .into()
}

/// A row that does something when it is pressed, with the hover fill under the pointer.
fn pressable<'a>(
    look: Palette,
    content: impl Into<Element<'a, Message>>,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    // a button lays its content out at the top of its box, so the row is centred by hand
    let inside = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(iced::Center)
        .clip(true);
    button(inside)
        .width(Length::Fill)
        .height(TALL)
        .padding([0, INSET])
        .on_press_maybe(on_press)
        .style(move |_: &Theme, state| fill(look, state))
        .into()
}

/// The icon at the left, the label, and whatever the row has at its right.
fn line<'a>(
    look: Palette,
    icon: &str,
    label: &str,
    right: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    row![
        icons::symbolic(look.text, icon, ICON),
        container(
            text(label.to_string())
                .size(bar::TEXT_SIZE)
                .color(look.text)
                .wrapping(iced::widget::text::Wrapping::None),
        )
        .width(Length::Fill)
        .clip(true),
        right.into(),
    ]
    .spacing(GAP)
    .align_y(iced::Center)
    .into()
}

/// Words that say less, at the right of a row.
fn dim(look: Palette, words: &str) -> Element<'static, Message> {
    text(words.to_string())
        .size(bar::TEXT_SIZE)
        .color(look.dim)
        .wrapping(iced::widget::text::Wrapping::None)
        .into()
}

/// One network: its signal, its name, a lock when it asks for a password, and a check on the one in
/// use. The one in use is not a button; any other is, and joins it.
fn network_row(look: Palette, network: &Network, at: usize) -> Element<'_, Message> {
    let mut marks = row![].spacing(GAP).align_y(iced::Center);
    if network.security.secured() {
        marks = marks.push(icons::symbolic(
            look.dim,
            "network-wireless-encrypted-symbolic",
            ICON,
        ));
    }
    if network.active {
        marks = marks.push(icons::symbolic(look.text, "object-select-symbolic", ICON));
    }
    let content = line(
        look,
        &icons::signal("wireless", network.strength),
        &network.name,
        marks,
    );
    if network.active {
        plain(content)
    } else {
        pressable(look, content, Some(Message::System(Event::Join(at))))
    }
}

/// A slider with its icon at the left: a button for the volume, which mutes, and a plain icon for
/// the brightness.
fn slider_row<'a>(
    look: Palette,
    icon: &str,
    on_icon: Option<Message>,
    value: u8,
    change: fn(u8) -> Event,
    done: Event,
) -> Element<'a, Message> {
    let symbol = container(icons::symbolic(look.text, icon, ICON)).center(Length::Fill);
    let lead: Element<'a, Message> = if on_icon.is_some() {
        button(symbol)
            .width(TALL)
            .height(TALL)
            .padding(0)
            .on_press_maybe(on_icon)
            .style(move |_: &Theme, state| fill(look, state))
            .into()
    } else {
        container(symbol).width(TALL).height(TALL).into()
    };
    let bar = slider(0..=100_u8, value, move |value| {
        Message::System(change(value))
    })
    .on_release(Message::System(done))
    .height(SLIDER)
    .width(Length::Fill)
    .style(move |_: &Theme, state| slider_style(look, state));
    container(
        row![lead, bar]
            .spacing(GAP)
            .align_y(iced::Center)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(TALL)
    .padding(Padding {
        right: f32::from(INSET),
        ..Padding::ZERO
    })
    .into()
}

/// A row of the session: its icon and its word, pressed to do it.
fn session_row(look: Palette, icon: &str, label: &str, event: Event) -> Element<'static, Message> {
    pressable(
        look,
        line(look, icon, label, space().width(0)),
        Some(Message::System(event)),
    )
}

/// The line between two sections.
pub fn separator(look: Palette) -> Element<'static, Message> {
    let rule =
        container(space().width(Length::Fill).height(1)).style(move |_: &Theme| container::Style {
            background: Some(look.edge.into()),
            ..container::Style::default()
        });
    container(rule)
        .width(Length::Fill)
        .height(SEPARATOR)
        .align_y(iced::Center)
        .into()
}

/// An on and off switch: the accent when it is on, the track gray when it is off, and the text
/// colour for the knob.
pub fn switch<'a, F>(look: Palette, on: bool, toggle: Option<F>) -> Element<'a, Message>
where
    F: Fn(bool) -> Message + 'a,
{
    toggler(on)
        .on_toggle_maybe(toggle)
        .size(SWITCH)
        .style(move |_: &Theme, state| {
            let (track, knob) = match state {
                toggler::Status::Active { is_toggled }
                | toggler::Status::Hovered { is_toggled } => {
                    (if is_toggled { look.accent } else { look.track }, look.text)
                }
                toggler::Status::Disabled { .. } => (look.track, look.dim),
            };
            toggler::Style {
                background: track.into(),
                background_border_width: 0.0,
                background_border_color: Color::TRANSPARENT,
                foreground: knob.into(),
                foreground_border_width: 0.0,
                foreground_border_color: Color::TRANSPARENT,
                text_color: None,
                border_radius: None,
                padding_ratio: 0.15,
            }
        })
        .into()
}

/// The slider: the part up to the handle in the accent, the rest in the track gray, and a round
/// handle in the text colour.
fn slider_style(look: Palette, _state: slider::Status) -> slider::Style {
    slider::Style {
        rail: slider::Rail {
            backgrounds: (look.accent.into(), look.track.into()),
            width: 4.0,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 2.0.into(),
            },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 8.0 },
            background: look.text.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    }
}

/// The fill under a row or a button: the hover gray under the pointer, the pressed one while it is
/// held.
pub fn fill(look: Palette, state: button::Status) -> button::Style {
    let background = match state {
        button::Status::Hovered => Some(look.hover),
        button::Status::Pressed => Some(look.press),
        button::Status::Active | button::Status::Disabled => None,
    };
    button::Style {
        background: background.map(Background::from),
        text_color: look.text,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: RADIUS.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::Volume;
    use librift::battery::Charge;
    use librift::network::{Picture, Security, Wired, Wireless};

    fn network(name: &str, active: bool) -> Network {
        Network {
            name: name.to_string(),
            ssid: name.as_bytes().to_vec(),
            strength: 60,
            security: Security::Password,
            point: format!("/ap/{name}"),
            active,
            saved: None,
        }
    }

    /// What the boot test's machine has: a sink, a cable, nothing else.
    fn vm() -> Status {
        Status {
            network: Some(Picture {
                wifi: true,
                radio: true,
                wired: Some(Wired {
                    path: "/devices/2".into(),
                    link: Link::Connected,
                }),
                wireless: None,
            }),
            volume: Some(Volume {
                level: 40,
                muted: false,
            }),
            battery: None,
            bluetooth: None,
            brightness: None,
        }
    }

    #[test]
    fn a_machine_with_a_sink_and_a_cable_has_three_sections() {
        let parts = parts(&vm(), None);
        assert_eq!(
            parts,
            [
                Part::Volume,
                Part::Separator,
                Part::Wired,
                Part::Separator,
                Part::Lock,
                Part::LogOut,
                Part::Restart,
                Part::ShutDown
            ]
        );
        assert_eq!(height(&parts), 2 * PAD + 6 * ROW + 2 * SEPARATOR);
    }

    #[test]
    fn a_laptop_has_every_section_and_the_lists_stop_at_their_most() {
        let mut status = vm();
        status.brightness = Some(80);
        status.battery = Some(Battery {
            level: 72,
            charge: Charge::Discharging,
            seconds: 12_000,
        });
        status.bluetooth = Some(librift::bluetooth::Picture {
            adapter: "/org/bluez/hci0".into(),
            powered: true,
            devices: (0..5)
                .map(|at| librift::bluetooth::Device {
                    path: format!("/org/bluez/hci0/dev_{at}"),
                    name: format!("Device {at}"),
                    icon: None,
                    connected: false,
                })
                .collect(),
        });
        if let Some(picture) = status.network.as_mut() {
            picture.wireless = Some(Wireless {
                path: "/devices/3".into(),
                link: Link::Connected,
                networks: (0..9)
                    .map(|at| network(&format!("Network {at}"), at == 0))
                    .collect(),
            });
        }
        let found = parts(&status, None);
        let count = |wanted: fn(&Part) -> bool| found.iter().filter(|part| wanted(part)).count();
        assert_eq!(count(|part| matches!(part, Part::Network(_))), NETWORKS);
        assert_eq!(count(|part| matches!(part, Part::Device(_))), DEVICES);
        assert_eq!(count(|part| matches!(part, Part::Separator)), 4);
        // the tallest menu still fits between the bar and the dock of a 768 px screen
        assert!(
            height(&found) + ROW + SEPARATOR <= 768 - 32 - 44,
            "{}",
            height(&found)
        );

        // with the radio off the networks go, and with Bluetooth off the devices do
        if let Some(picture) = status.network.as_mut() {
            picture.wifi = false;
        }
        if let Some(bluetooth) = status.bluetooth.as_mut() {
            bluetooth.powered = false;
        }
        let found = parts(&status, None);
        assert!(
            !found
                .iter()
                .any(|part| matches!(part, Part::Network(_) | Part::Device(_)))
        );
        assert!(found.contains(&Part::Wifi));
        assert!(found.contains(&Part::Bluetooth));
    }

    #[test]
    fn a_line_over_the_session_says_what_happened() {
        let mut menu = Menu::new(window::Id::unique(), 0);
        assert!(!parts(&vm(), Some(&menu)).contains(&Part::Line));
        menu.notice = Some("Connecting to Home".into());
        let found = parts(&vm(), Some(&menu));
        let at = found
            .iter()
            .position(|part| *part == Part::Line)
            .expect("the line");
        assert_eq!(found[at + 1], Part::Separator);
        assert_eq!(found[at + 2], Part::Lock);
        menu.error = Some("The network did not come up.".into());
        assert_eq!(menu.line(), Some(("The network did not come up.", true)));
    }

    #[test]
    fn the_rows_say_what_they_see() {
        assert_eq!(wired_word(Link::Connected), "Connected");
        assert_eq!(wired_word(Link::Unavailable), "Cable unplugged");
        let battery = Battery {
            level: 72,
            charge: Charge::Discharging,
            seconds: 12_000,
        };
        assert_eq!(battery_words(&battery), "72%, 3 h 20 min left");
        let unknown = Battery {
            seconds: 0,
            ..battery
        };
        assert_eq!(battery_words(&unknown), "72%");
        // a sink past full sits at the end of the slider, and a held slider wins over a reading
        let mut status = vm();
        status.volume = Some(Volume {
            level: 130,
            muted: false,
        });
        let mut menu = Menu::new(window::Id::unique(), 0);
        assert_eq!(volume_level(&status, &menu), 100);
        menu.volume = Some(25);
        assert_eq!(volume_level(&status, &menu), 25);
    }

    #[test]
    fn the_sizes_add_up() {
        const {
            assert!(SLIDER < TALL);
            assert!(
                2.0 * SWITCH < 85.0,
                "a switch takes less than a quarter of the menu"
            );
        }
        assert!((f64::from(TALL) - f64::from(ROW)).abs() < f64::EPSILON);
        assert_eq!(PAD, u32::from(INSIDE));
    }
}
