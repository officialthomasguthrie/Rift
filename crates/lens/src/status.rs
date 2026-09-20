//! What the icons at the right of the bar and the system menu say: the network from
//! `NetworkManager`, Bluetooth from `BlueZ` and the battery from `UPower`, all read off the system
//! bus, the volume from `librift::sound` and the brightness from `brightnessctl`. An icon is there
//! only when the system has something to say, so a machine with no battery shows no battery.

use librift::battery::{Battery, Charge};
use librift::bluetooth;
use librift::network::{self, Link};
use librift::os::{ask, change};
use librift::sound::{Side, Volume};

/// Everything the right of the bar and the system menu show.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    /// What `NetworkManager` says, or `None` until it has answered, or when it is not running.
    pub network: Option<network::Picture>,
    /// The default sink, when there is one.
    pub volume: Option<Volume>,
    /// The battery, when the machine has one.
    pub battery: Option<Battery>,
    /// Bluetooth, when the machine has an adapter.
    pub bluetooth: Option<bluetooth::Picture>,
    /// The screen's backlight in percent, when the machine has one.
    pub brightness: Option<u8>,
}

impl Status {
    /// The icons for the bar, in the order every desktop puts them: the network, Bluetooth when a
    /// device is connected, the volume, the battery.
    #[must_use]
    pub fn icons(&self) -> Vec<String> {
        let mut names = vec![network_icon(self.network.as_ref())];
        if self
            .bluetooth
            .as_ref()
            .is_some_and(|bluetooth| bluetooth.devices.iter().any(|device| device.connected))
        {
            names.push("bluetooth-active-symbolic".to_string());
        }
        if let Some(volume) = self.volume {
            names.push(volume.icon(Side::Output).to_string());
        }
        if let Some(battery) = self.battery {
            names.push(battery_icon(battery));
        }
        names
    }
}

/// The network icon: a cable that is up wins over wireless, the way the icon in every desktop
/// does, then a connection on its way up, then a radio that is off.
#[must_use]
pub fn network_icon(picture: Option<&network::Picture>) -> String {
    let Some(picture) = picture else {
        return "network-no-route-symbolic".to_string();
    };
    let wired = picture.wired.as_ref().map(|wired| wired.link);
    let wireless = picture.wireless.as_ref();
    if wired == Some(Link::Connected) {
        return "network-wired-symbolic".to_string();
    }
    if let Some(wireless) = wireless.filter(|wireless| wireless.link == Link::Connected) {
        let strength = wireless.active().map_or(0, |network| network.strength);
        return librift::network::signal_icon("wireless", strength);
    }
    if wired == Some(Link::Connecting) {
        return "network-wired-acquiring-symbolic".to_string();
    }
    if wireless.is_some_and(|wireless| wireless.link == Link::Connecting) {
        return "network-wireless-acquiring-symbolic".to_string();
    }
    if wireless.is_some() && !(picture.wifi && picture.radio) {
        return "network-wireless-disabled-symbolic".to_string();
    }
    "network-offline-symbolic".to_string()
}

/// The word `lens --state` prints for the network: `wired`, `wifi <strength>`, `connecting`,
/// `off`, `offline`, or `unknown` when `NetworkManager` has not answered.
#[must_use]
pub fn network_word(picture: Option<&network::Picture>) -> String {
    let Some(picture) = picture else {
        return "unknown".to_string();
    };
    let wired = picture.wired.as_ref().map(|wired| wired.link);
    let wireless = picture.wireless.as_ref();
    if wired == Some(Link::Connected) {
        "wired".to_string()
    } else if let Some(wireless) = wireless.filter(|wireless| wireless.link == Link::Connected) {
        format!(
            "wifi {}",
            wireless.active().map_or(0, |network| network.strength)
        )
    } else if wired == Some(Link::Connecting)
        || wireless.is_some_and(|wireless| wireless.link == Link::Connecting)
    {
        "connecting".to_string()
    } else if wireless.is_some() && !(picture.wifi && picture.radio) {
        "off".to_string()
    } else {
        "offline".to_string()
    }
}

/// The word `lens --state` prints for the cable: `connected`, `connecting`, `disconnected`,
/// `unplugged`, or `none` when the machine has no port for one.
#[must_use]
pub fn wired_word(picture: Option<&network::Picture>) -> String {
    picture
        .and_then(|picture| picture.wired.as_ref())
        .map_or("none", |wired| wired.link.word())
        .to_string()
}

/// The words `lens --state` prints for Wi-Fi: `on` and how many networks it sees, `off`, or `none`
/// when the machine has no wireless card.
#[must_use]
pub fn wifi_word(picture: Option<&network::Picture>) -> String {
    let Some(picture) = picture else {
        return "none".to_string();
    };
    match &picture.wireless {
        None => "none".to_string(),
        Some(_) if !(picture.wifi && picture.radio) => "off".to_string(),
        Some(wireless) => format!("on {}", wireless.networks.len()),
    }
}

/// The words `lens --state` prints for Bluetooth: `on` and how many devices are paired, `off`, or
/// `none` when the machine has no adapter.
#[must_use]
pub fn bluetooth_word(bluetooth: Option<&bluetooth::Picture>) -> String {
    match bluetooth {
        None => "none".to_string(),
        Some(picture) if picture.powered => format!("on {}", picture.devices.len()),
        Some(_) => "off".to_string(),
    }
}

/// The battery icon for this level and state.
#[must_use]
pub fn battery_icon(battery: Battery) -> String {
    let step = u16::from(battery.level) / 10 * 10;
    match battery.charge {
        Charge::Full => "battery-level-100-charged-symbolic".to_string(),
        Charge::Charging => format!("battery-level-{step}-charging-symbolic"),
        Charge::Discharging | Charge::Idle => format!("battery-level-{step}-symbolic"),
    }
}

/// The words `lens --state` prints for the battery.
#[must_use]
pub fn battery_word(battery: Battery) -> String {
    let state = match battery.charge {
        Charge::Full => " charged",
        Charge::Charging => " charging",
        Charge::Discharging | Charge::Idle => "",
    };
    format!("{}{state}", battery.level)
}

/// The backlight in percent, when the machine has one. Only the backlight class counts: the
/// keyboard's lights are brightness devices too.
#[must_use]
pub fn brightness() -> Option<u8> {
    read_brightness(&ask(
        "brightnessctl",
        &["--machine-readable", "--class=backlight", "info"],
    )?)
}

/// `brightnessctl -m` prints `name,class,current,percent,max`, and the percent it prints is
/// rounded down, so it is worked out again from the two numbers.
fn read_brightness(printed: &str) -> Option<u8> {
    let line = printed.lines().find(|line| line.contains(",backlight,"))?;
    let fields: Vec<&str> = line.split(',').collect();
    let current: u64 = fields.get(2)?.trim().parse().ok()?;
    let max: u64 = fields.get(4)?.trim().parse().ok()?;
    if max == 0 {
        return None;
    }
    u8::try_from((current * 100 + max / 2) / max).ok()
}

/// Turn the backlight up or down by a step. Like the slider, it never goes all the way to black.
///
/// # Errors
///
/// When `brightnessctl` is not there, the machine has no backlight, or it refuses.
pub fn step_brightness(up: bool) -> Result<(), String> {
    change(
        "brightnessctl",
        &[
            "--class=backlight",
            "--min-value=1",
            "set",
            if up { "+10%" } else { "10%-" },
        ],
    )
}

/// Set the backlight. It never goes all the way to black, which would leave a screen that looks
/// switched off.
///
/// # Errors
///
/// When `brightnessctl` is not there or refuses.
pub fn set_brightness(percent: u8) -> Result<(), String> {
    change(
        "brightnessctl",
        &[
            "--class=backlight",
            "--min-value=1",
            "set",
            &format!("{}%", percent.min(100)),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use librift::network::{Network, Picture, Security, Wired, Wireless};

    #[test]
    fn the_brightness_comes_from_the_backlight() {
        let printed = "intel_backlight,backlight,48000,50%,96000\n";
        assert_eq!(read_brightness(printed), Some(50));
        // the percent brightnessctl prints is rounded down
        assert_eq!(
            read_brightness("amdgpu_bl0,backlight,127,49%,255\n"),
            Some(50)
        );
        // a keyboard light is not the screen
        assert_eq!(read_brightness("input0::capslock,leds,0,0%,1\n"), None);
        assert_eq!(read_brightness("x,backlight,5,0%,0\n"), None);
    }

    fn wireless(link: Link, strength: u8) -> Wireless {
        Wireless {
            path: "/devices/3".into(),
            link,
            addresses: network::Addresses::default(),
            networks: vec![Network {
                name: "Home".into(),
                ssid: b"Home".to_vec(),
                strength,
                security: Security::Password,
                point: "/ap/1".into(),
                active: link == Link::Connected,
                saved: Some("/settings/1".into()),
            }],
        }
    }

    #[test]
    fn the_network_icon_and_word_follow_the_picture() {
        let cable = |link| Picture {
            wifi: true,
            radio: true,
            wired: Some(Wired {
                path: "/devices/2".into(),
                link,
                ..Wired::default()
            }),
            wireless: Some(wireless(Link::Connected, 72)),
        };
        // a cable that is up wins
        assert_eq!(
            network_icon(Some(&cable(Link::Connected))),
            "network-wired-symbolic"
        );
        assert_eq!(network_word(Some(&cable(Link::Connected))), "wired");
        // unplugged, the wireless network shows with its strength
        assert_eq!(
            network_icon(Some(&cable(Link::Unavailable))),
            "network-wireless-signal-good-symbolic"
        );
        assert_eq!(network_word(Some(&cable(Link::Unavailable))), "wifi 72");

        let radio_off = Picture {
            wifi: false,
            radio: true,
            wired: None,
            wireless: Some(wireless(Link::Unavailable, 0)),
        };
        assert_eq!(
            network_icon(Some(&radio_off)),
            "network-wireless-disabled-symbolic"
        );
        assert_eq!(network_word(Some(&radio_off)), "off");

        let joining = Picture {
            wifi: true,
            radio: true,
            wired: None,
            wireless: Some(wireless(Link::Connecting, 40)),
        };
        assert_eq!(
            network_icon(Some(&joining)),
            "network-wireless-acquiring-symbolic"
        );
        assert_eq!(network_word(Some(&joining)), "connecting");

        let nothing = Picture {
            wifi: true,
            radio: true,
            wired: Some(Wired {
                path: "/devices/2".into(),
                link: Link::Disconnected,
                ..Wired::default()
            }),
            wireless: None,
        };
        assert_eq!(network_icon(Some(&nothing)), "network-offline-symbolic");
        assert_eq!(network_word(Some(&nothing)), "offline");
        assert_eq!(network_icon(None), "network-no-route-symbolic");
        assert_eq!(network_word(None), "unknown");
    }

    #[test]
    fn the_battery_icon_and_word_follow_the_charge() {
        let at = |level, charge| Battery {
            level,
            charge,
            seconds: 0,
        };
        assert_eq!(
            battery_icon(at(9, Charge::Charging)),
            "battery-level-0-charging-symbolic"
        );
        assert_eq!(
            battery_icon(at(72, Charge::Discharging)),
            "battery-level-70-symbolic"
        );
        assert_eq!(
            battery_icon(at(100, Charge::Full)),
            "battery-level-100-charged-symbolic"
        );
        assert_eq!(battery_word(at(100, Charge::Full)), "100 charged");
        assert_eq!(battery_word(at(40, Charge::Charging)), "40 charging");
        assert_eq!(battery_word(at(72, Charge::Discharging)), "72");
    }

    #[test]
    fn the_icons_are_there_only_when_there_is_something_to_say() {
        let mut status = Status::default();
        assert_eq!(status.icons(), ["network-no-route-symbolic"]);
        status.volume = Some(Volume {
            level: 40,
            muted: true,
        });
        status.bluetooth = Some(bluetooth::Picture {
            adapter: "/org/bluez/hci0".into(),
            powered: true,
            devices: vec![bluetooth::Device {
                path: "/org/bluez/hci0/dev_1".into(),
                name: "Headphones".into(),
                icon: None,
                connected: true,
            }],
        });
        assert_eq!(
            status.icons(),
            [
                "network-no-route-symbolic",
                "bluetooth-active-symbolic",
                "audio-volume-muted-symbolic"
            ]
        );
    }
}
