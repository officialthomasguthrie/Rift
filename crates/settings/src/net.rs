//! The Wi-Fi and Network pages. Both are views of the one picture `NetworkManager` gives: the
//! wireless card with the networks around it, and the cable with what it has been given. The
//! window follows the service while it is open, so a network that comes and goes comes and goes on
//! the page.
//!
//! Joining a network that asks for a password asks for it in the row under that network, rather
//! than in a window of its own: a page is already the owner's whole attention, which a menu is not.

use std::thread;
use std::time::Duration;

use iced::futures::channel::oneshot;
use iced::widget::{column, row, text};
use iced::{Center, Element, Fill, Task};
use librift::network::{self, Network, Picture, Security};

use crate::icons;
use crate::theme::Colors;
use crate::ui::{Message, Settings};
use crate::widgets::{GAP, TEXT_SIZE, choice, field, group, heading, note, setting};

/// How long a join is given before the page says the network did not come up.
const JOIN_WAIT: Duration = Duration::from_secs(45);
/// The password field's widget id, for the operation that puts the cursor in it.
const FIELD: &str = "password";
/// How big a signal icon is drawn.
const ICON: f32 = 16.0;

/// A network being joined that asks for a password, and what has been typed for it.
#[derive(Debug, Clone)]
pub struct Joining {
    /// The network itself.
    pub network: Network,
    /// The card it is joined through.
    pub device: String,
    /// What has been typed so far.
    pub password: String,
    /// While `NetworkManager` is bringing it up.
    pub busy: bool,
}

/// Turn the radio on or off. A machine with no card has no switch on the page, so it has none
/// from a terminal either. What it did shows up in the next reading.
pub fn set_wifi(state: &Settings, on: bool) -> Task<Message> {
    if card(state).is_none() {
        return Task::none();
    }
    acted(move || network::set_wifi(on))
}

/// Ask the card to sweep for networks again, so the list is fresh. The card refuses one that came
/// too soon, which is not worth saying, and what it finds arrives through the service's signals.
pub fn scan(state: &Settings) -> Task<Message> {
    let Some(device) = card(state).map(|wireless| wireless.path.clone()) else {
        return Task::none();
    };
    Task::future(async move {
        let (sender, receiver) = oneshot::channel();
        thread::spawn(move || {
            network::scan(&device);
            let _ = sender.send(());
        });
        let _ = receiver.await;
        Message::Scanned
    })
}

/// A network in the list was pressed. One that asks for a password nobody saved opens the field
/// under it; any other is joined at once.
pub fn join(state: &mut Settings, at: usize) -> Task<Message> {
    let Some((device, network)) = card(state).and_then(|wireless| {
        wireless
            .networks
            .get(at)
            .map(|network| (wireless.path.clone(), network.clone()))
    }) else {
        return Task::none();
    };
    if network.active {
        return Task::none();
    }
    state.problem = None;
    state.doing = None;
    if !network.security.joinable() {
        state.joining = None;
        state.problem = Some(format!(
            "{} asks for a user name, which Settings cannot do yet.",
            network.name
        ));
        return Task::none();
    }
    if network.security.secured() && network.saved.is_none() {
        let name = network.name.clone();
        state.joining = Some(Joining {
            network,
            device,
            password: String::new(),
            busy: false,
        });
        state.doing = Some(format!("Type the password for {name}."));
        return crate::widgets::focus(FIELD);
    }
    state.joining = None;
    state.doing = Some(format!("Connecting to {}", network.name));
    joining(device, network, None)
}

/// The password that was typed was given. A join that fails says so and leaves the field up, so it
/// can be tried again.
pub fn joined(state: &mut Settings) -> Task<Message> {
    let Some(asked) = state.joining.as_mut() else {
        return Task::none();
    };
    if asked.busy || asked.password.chars().count() < asked.network.security.shortest() {
        return Task::none();
    }
    asked.busy = true;
    let (device, network) = (asked.device.clone(), asked.network.clone());
    let password = asked.password.clone();
    state.problem = None;
    state.doing = Some(format!("Connecting to {}", network.name));
    joining(device, network, Some(password))
}

/// Start a join and wait for it, on a thread of its own.
fn joining(device: String, network: Network, password: Option<String>) -> Task<Message> {
    acted(move || {
        let started = network::join(&device, &network, password.as_deref())?;
        network::wait(&started, JOIN_WAIT).map_err(|_| match password {
            Some(_) => format!(
                "Could not connect to {}. Check the password and try again.",
                network.name
            ),
            None => format!("Could not connect to {}.", network.name),
        })
    })
}

/// Run something that asks `NetworkManager` on a thread of its own, and say how it went.
fn acted(work: impl FnOnce() -> Result<(), String> + Send + 'static) -> Task<Message> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let _ = sender.send(work());
    });
    Task::perform(receiver, |said| {
        Message::Acted(said.unwrap_or_else(|_| Err("It stopped before it finished.".to_string())))
    })
}

/// The wireless card, when the machine has one and `NetworkManager` has answered.
fn card(state: &Settings) -> Option<&network::Wireless> {
    picture(state).and_then(|picture| picture.wireless.as_ref())
}

/// What `NetworkManager` said, when it has answered and had something to say.
fn picture(state: &Settings) -> Option<&Picture> {
    state
        .network
        .as_ref()
        .and_then(|answered| answered.as_ref().ok())
}

/// The place in the list of the network with this name, for `--set join`.
#[must_use]
pub fn named(state: &Settings, name: &str) -> Option<usize> {
    let name = name.trim();
    card(state)?
        .networks
        .iter()
        .position(|network| network.name == name)
}

/// The lines `rift-settings --state` prints about the network, once `NetworkManager` has answered.
#[must_use]
pub fn state(state: &Settings) -> Vec<String> {
    let Some(picture) = picture(state) else {
        return Vec::new();
    };
    let mut lines = vec![format!(
        "wifi {}",
        match &picture.wireless {
            None => "none",
            Some(_) if picture.wifi && picture.radio => "on",
            Some(_) => "off",
        }
    )];
    if let Some(wireless) = &picture.wireless {
        lines.push(format!("networks {}", wireless.networks.len()));
        if let Some(network) = wireless.active() {
            lines.push(format!("network {}", network.name));
        }
    }
    lines.push(format!(
        "wired {}",
        picture
            .wired
            .as_ref()
            .map_or("none", |wired| wired.link.word())
    ));
    lines.push(format!(
        "address {}",
        picture
            .wired
            .as_ref()
            .and_then(|wired| wired.addresses.v4.first())
            .map_or("none", String::as_str)
    ));
    lines
}

/// The Wi-Fi page: the radio, the network this machine is on, and the ones it can see.
pub fn wifi(state: &Settings, look: Colors) -> Element<'_, Message> {
    let mut page = column![].spacing(GAP).width(Fill);
    match asked(
        state,
        look,
        "NetworkManager is not answering, so the networks are not here.",
    ) {
        Err(said) => return said,
        Ok(picture) => {
            let Some(wireless) = &picture.wireless else {
                return page
                    .push(note(look, "This machine has no Wi-Fi card."))
                    .push(hidden(look))
                    .into();
            };
            page = page.push(group(look, vec![radio(look, picture)]));
            if picture.wifi && picture.radio {
                if let Some(network) = wireless.active() {
                    page = page.push(connected(look, network, wireless));
                }
                page = page.push(around(state, look, wireless));
            } else {
                page = page.push(note(look, "Wi-Fi is off, so there is nothing to show."));
            }
        }
    }
    page.push(line(state, look)).push(hidden(look)).into()
}

/// The Network page: the cable and what it has been given.
pub fn wired(state: &Settings, look: Colors) -> Element<'_, Message> {
    let mut page = column![].spacing(GAP).width(Fill);
    match asked(
        state,
        look,
        "NetworkManager is not answering, so the cable is not here.",
    ) {
        Err(said) => return said,
        Ok(picture) => match &picture.wired {
            None => {
                page = page.push(note(look, "This machine has no port for a network cable."));
            }
            Some(cable) => {
                let mut rows = vec![setting(
                    look,
                    "Cable",
                    None,
                    said(look, cable.link.wired_word().to_string()),
                )];
                rows.extend(facts(look, &cable.addresses));
                page = page.push(column![heading(look, "Wired"), group(look, rows)].spacing(8));
            }
        },
    }
    page.push(
        column![
            heading(look, "VPN"),
            note(
                look,
                "A VPN is not in Settings yet. nmcli imports a WireGuard or OpenVPN connection and \
                 turns it on.",
            ),
        ]
        .spacing(8),
    )
    .push(note(
        look,
        "Proxies, and an address set by hand, are not in Settings yet.",
    ))
    .into()
}

/// What `NetworkManager` answered, or the page that says it did not, in the words of the page
/// that asked.
fn asked<'a>(
    state: &'a Settings,
    look: Colors,
    missing: &'static str,
) -> Result<&'a Picture, Element<'a, Message>> {
    match &state.network {
        None => Err(note(look, "Asking NetworkManager about the network.")),
        Some(Err(why)) => Err(column![note(look, missing), note(look, why)]
            .spacing(GAP)
            .width(Fill)
            .into()),
        Some(Ok(picture)) => Ok(picture),
    }
}

/// The Wi-Fi row: the switch, or what is keeping the radio off.
fn radio(look: Colors, picture: &Picture) -> Element<'_, Message> {
    if picture.radio {
        setting(
            look,
            "Wi-Fi",
            None,
            crate::widgets::switch(look, picture.wifi, Message::Wifi),
        )
    } else {
        setting(
            look,
            "Wi-Fi",
            None,
            said(
                look,
                "A switch on this machine holds the radio off".to_string(),
            ),
        )
    }
}

/// The network the machine is on: how strong it is, how it is secured, and its address.
fn connected<'a>(
    look: Colors,
    network: &'a Network,
    wireless: &'a network::Wireless,
) -> Element<'a, Message> {
    let rows = vec![
        setting(
            look,
            &network.name,
            Some(secured_words(network.security)),
            strength(look, network),
        ),
        setting(
            look,
            "Address",
            None,
            said(
                look,
                wireless
                    .addresses
                    .v4
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "None yet".to_string()),
            ),
        ),
    ];
    column![heading(look, "Connected"), group(look, rows)]
        .spacing(8)
        .into()
}

/// Every other network the card sees, strongest first, with the password field under the one being
/// joined.
fn around<'a>(
    state: &'a Settings,
    look: Colors,
    wireless: &'a network::Wireless,
) -> Element<'a, Message> {
    let mut rows: Vec<Element<'a, Message>> = Vec::new();
    for (at, network) in wireless.networks.iter().enumerate() {
        if network.active {
            continue;
        }
        rows.push(choice(
            look,
            &network.name,
            Some(under(network)),
            Some(strength(look, network)),
            false,
            Message::Join(at),
        ));
        if let Some(asked) = &state.joining {
            if asked.network.ssid == network.ssid {
                rows.push(password(look, asked));
            }
        }
    }
    if rows.is_empty() {
        return column![
            heading(look, "Networks"),
            note(look, "No other network is in range."),
        ]
        .spacing(8)
        .into();
    }
    column![heading(look, "Networks"), group(look, rows)]
        .spacing(8)
        .into()
}

/// The row the password is typed into, under the network it is for.
fn password(look: Colors, asked: &Joining) -> Element<'_, Message> {
    let hint = if asked.busy {
        "Connecting."
    } else if asked.password.chars().count() < asked.network.security.shortest() {
        "At least eight characters."
    } else {
        "Press Enter to join."
    };
    setting(
        look,
        "Password",
        Some(hint),
        field(
            look,
            "Password",
            &asked.password,
            true,
            FIELD,
            Message::Password,
            Message::Joined,
        ),
    )
}

/// The signal, as the bars the shell's menu draws and the number under them.
fn strength<'a>(look: Colors, network: &Network) -> Element<'a, Message> {
    let mut beside = row![].spacing(8).align_y(Center);
    if network.security.secured() {
        beside = beside.push(icons::symbolic(
            look.dim,
            "network-wireless-encrypted-symbolic",
            ICON,
        ));
    }
    beside
        .push(
            text(format!("{} per cent", network.strength))
                .size(TEXT_SIZE)
                .color(look.dim),
        )
        .push(icons::symbolic(
            look.text,
            &network::signal_icon("wireless", network.strength),
            ICON,
        ))
        .into()
}

/// What a row of the list says under the network's name.
fn under(network: &Network) -> &'static str {
    if network.saved.is_some() {
        "Joined before"
    } else {
        secured_words(network.security)
    }
}

/// What a network asks of whoever joins it, in words.
const fn secured_words(security: Security) -> &'static str {
    match security {
        Security::Open => "Open, with no password",
        Security::Enhanced => "Open, and encrypted all the same",
        Security::Password => "Asks for a WPA2 password",
        Security::Sae => "Asks for a WPA3 password",
        Security::Wep => "Asks for an old WEP key",
        Security::Enterprise => "Asks for a user name, which Settings cannot do yet",
    }
}

/// The rows of a device's addresses. What it was not given is not a row.
fn facts<'a>(look: Colors, addresses: &'a network::Addresses) -> Vec<Element<'a, Message>> {
    let mut rows = Vec::new();
    let mut fact = |label: &'a str, value: String| {
        if !value.is_empty() {
            rows.push(setting(look, label, None, said(look, value)));
        }
    };
    fact("Name", addresses.interface.clone());
    fact("Hardware address", addresses.hardware.clone());
    fact("IPv4 address", addresses.v4.join(", "));
    fact("IPv6 address", addresses.v6.join(", "));
    fact(
        "Default route",
        addresses.gateway.clone().unwrap_or_default(),
    );
    fact("DNS", addresses.dns.join(", "));
    rows
}

/// The line under the page: what went wrong, or what is happening.
fn line(state: &Settings, look: Colors) -> Element<'_, Message> {
    match (&state.problem, &state.doing) {
        (Some(why), _) => text(why).size(TEXT_SIZE).color(look.error).into(),
        (None, Some(doing)) => note(look, doing),
        (None, None) => iced::widget::space().height(0.0).into(),
    }
}

/// The sentence at the foot of the Wi-Fi page.
fn hidden<'a>(look: Colors) -> Element<'a, Message> {
    note(
        look,
        "Hidden networks, and networks that ask for a user name, are not in Settings yet.",
    )
}

/// What a row is set to, at the right of it.
fn said<'a>(look: Colors, value: String) -> Element<'a, Message> {
    text(value).size(TEXT_SIZE).color(look.dim).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use librift::network::{Addresses, Link, Wired, Wireless};

    fn network(name: &str, strength: u8, security: Security, active: bool) -> Network {
        Network {
            name: name.to_string(),
            ssid: name.as_bytes().to_vec(),
            strength,
            security,
            point: format!("/ap/{name}"),
            active,
            saved: None,
        }
    }

    fn settings(picture: Option<Picture>) -> Settings {
        let mut state = Settings::bare();
        state.network = picture.map(Ok);
        state
    }

    #[test]
    fn a_network_says_what_it_asks_for() {
        assert_eq!(secured_words(Security::Open), "Open, with no password");
        assert_eq!(
            under(&network("Cafe", 90, Security::Open, false)),
            "Open, with no password"
        );
        let mut saved = network("Home", 70, Security::Password, false);
        assert_eq!(under(&saved), "Asks for a WPA2 password");
        saved.saved = Some("/settings/1".to_string());
        assert_eq!(under(&saved), "Joined before");
        for security in [
            Security::Open,
            Security::Enhanced,
            Security::Password,
            Security::Sae,
            Security::Wep,
            Security::Enterprise,
        ] {
            let words = secured_words(security);
            assert!(words.is_ascii() && !words.ends_with('.'), "{words}");
        }
    }

    #[test]
    fn the_state_lines_say_what_the_machine_has() {
        // the boot test's machine: a cable with an address, no card
        let vm = Picture {
            wifi: true,
            radio: true,
            wired: Some(Wired {
                path: "/devices/2".to_string(),
                link: Link::Connected,
                addresses: Addresses {
                    interface: "eth0".to_string(),
                    v4: vec!["10.0.2.15/24".to_string()],
                    ..Addresses::default()
                },
            }),
            wireless: None,
        };
        assert_eq!(
            state(&settings(Some(vm))),
            ["wifi none", "wired connected", "address 10.0.2.15/24"]
        );
        // nothing has answered yet, so nothing is printed
        assert!(state(&settings(None)).is_empty());
    }

    #[test]
    fn a_card_prints_its_networks_and_the_one_it_is_on() {
        let picture = Picture {
            wifi: true,
            radio: true,
            wired: None,
            wireless: Some(Wireless {
                path: "/devices/3".to_string(),
                link: Link::Connected,
                addresses: Addresses::default(),
                networks: vec![
                    network("Home", 80, Security::Password, true),
                    network("Cafe", 40, Security::Open, false),
                ],
            }),
        };
        let state_of = settings(Some(picture.clone()));
        assert_eq!(
            state(&state_of),
            [
                "wifi on",
                "networks 2",
                "network Home",
                "wired none",
                "address none"
            ]
        );
        assert_eq!(named(&state_of, "Cafe"), Some(1));
        assert_eq!(named(&state_of, " Home "), Some(0));
        assert_eq!(named(&state_of, "Nowhere"), None);
        // the radio off leaves the card there and the list unsaid
        let off = settings(Some(Picture {
            wifi: false,
            ..picture
        }));
        assert_eq!(state(&off)[0], "wifi off");
    }
}
