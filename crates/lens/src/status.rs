//! What the icons at the right of the bar and the system menu say: the network from
//! `NetworkManager`, Bluetooth from `BlueZ` and the battery from `UPower`, all read off the system
//! bus, and the volume and the brightness from the programs that own them, `wpctl` and
//! `brightnessctl`. An icon is there only when the system has something to say, so a machine with
//! no battery shows no battery.

use std::collections::HashSet;
use std::process::Command;

use librift::battery::{Battery, Charge};
use librift::bluetooth;
use librift::network::{self, Link};

use crate::icons;

/// The default sink's volume, as a percentage, and whether it is muted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Volume {
    /// Percent of full. `PipeWire` allows more than a hundred.
    pub level: u16,
    /// Muted, whatever the level is.
    pub muted: bool,
}

impl Volume {
    /// The name of the icon for this level.
    #[must_use]
    pub fn icon(self) -> &'static str {
        if self.muted || self.level == 0 {
            "audio-volume-muted-symbolic"
        } else if self.level <= 33 {
            "audio-volume-low-symbolic"
        } else if self.level <= 66 {
            "audio-volume-medium-symbolic"
        } else if self.level <= 100 {
            "audio-volume-high-symbolic"
        } else {
            "audio-volume-overamplified-symbolic"
        }
    }

    /// The words `lens --state` prints for it.
    #[must_use]
    pub fn word(self) -> String {
        if self.muted {
            format!("{} muted", self.level)
        } else {
            self.level.to_string()
        }
    }
}

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
            names.push(volume.icon().to_string());
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
        return icons::signal("wireless", strength);
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
    let link = picture
        .and_then(|picture| picture.wired.as_ref())
        .map(|wired| wired.link);
    match link {
        None => "none",
        Some(Link::Connected) => "connected",
        Some(Link::Connecting) => "connecting",
        Some(Link::Disconnected) => "disconnected",
        Some(Link::Unavailable) => "unplugged",
    }
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

/// What a program printed, or `None` when it is not there or it failed.
fn ask<const N: usize>(program: &str, args: [&str; N]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        None
    }
}

/// Run a program that changes something, and say what went wrong when it did.
fn change(program: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("Could not run {program}: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&output.stderr);
    Err(said
        .lines()
        .next()
        .filter(|line| !line.trim().is_empty())
        .map_or_else(
            || format!("{program} failed ({})", output.status),
            |line| line.trim().to_string(),
        ))
}

/// The sink `wpctl` means when it is not told which.
const SINK: &str = "@DEFAULT_AUDIO_SINK@";

/// The default sink's volume now.
#[must_use]
pub fn volume() -> Option<Volume> {
    read_volume(&ask("wpctl", ["get-volume", SINK])?)
}

/// `wpctl get-volume @DEFAULT_AUDIO_SINK@` prints `Volume: 0.40` and `[MUTED]` when it is muted.
fn read_volume(printed: &str) -> Option<Volume> {
    let rest = printed.split_once("Volume:")?.1;
    let share: f32 = rest.split_whitespace().next()?.parse().ok()?;
    if !share.is_finite() || share < 0.0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let level = (share * 100.0).round().min(f32::from(u16::MAX)) as u16;
    Some(Volume {
        level,
        muted: printed.contains("[MUTED]"),
    })
}

/// Set the default sink's volume, and unmute it: moving the slider is asking to hear something.
///
/// # Errors
///
/// When `wpctl` is not there or refuses.
pub fn set_volume(percent: u8) -> Result<(), String> {
    change("wpctl", &["set-volume", SINK, &fraction(percent)])?;
    change("wpctl", &["set-mute", SINK, "0"])
}

/// A percentage as the fraction `wpctl` takes: 40 is `0.40`.
fn fraction(percent: u8) -> String {
    let percent = percent.min(100);
    format!("{}.{:02}", percent / 100, percent % 100)
}

/// Mute the default sink, or unmute it.
///
/// # Errors
///
/// When `wpctl` is not there or refuses.
pub fn toggle_mute() -> Result<(), String> {
    change("wpctl", &["set-mute", SINK, "toggle"])
}

/// The backlight in percent, when the machine has one. Only the backlight class counts: the
/// keyboard's lights are brightness devices too.
#[must_use]
pub fn brightness() -> Option<u8> {
    read_brightness(&ask(
        "brightnessctl",
        ["--machine-readable", "--class=backlight", "info"],
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

/// Reads what `pw-mon` prints and says which of its lines mean the sound may have changed: a sink
/// or a card that came, changed or went. A program connecting to `PipeWire` is an event too, and
/// every `wpctl` the shell runs is one, so those are not.
#[derive(Debug, Default)]
pub struct Monitor {
    /// The event the lines are about now.
    event: Option<Event>,
    /// The object it is about.
    id: Option<u32>,
    /// The nodes and devices seen so far, so their removal can be told from a program's.
    audio: HashSet<u32>,
}

/// The three kinds of event `pw-mon` prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Event {
    Added,
    Changed,
    Removed,
}

impl Monitor {
    /// Take one line in. True when the volume should be read again.
    pub fn line(&mut self, line: &str) -> bool {
        let line = line.trim();
        let event = match line {
            "added:" => Some(Event::Added),
            "changed:" => Some(Event::Changed),
            "removed:" => Some(Event::Removed),
            _ => None,
        };
        if event.is_some() {
            self.event = event;
            self.id = None;
            return false;
        }
        if let Some(id) = line
            .strip_prefix("id: ")
            .and_then(|id| id.trim().parse().ok())
        {
            self.id = Some(id);
            // a removal says nothing after the id
            return self.event == Some(Event::Removed) && self.audio.remove(&id);
        }
        if let Some(kind) = line.strip_prefix("type: ") {
            let audio = kind.starts_with("PipeWire:Interface:Node")
                || kind.starts_with("PipeWire:Interface:Device");
            if !audio {
                return false;
            }
            if let Some(id) = self.id {
                self.audio.insert(id);
            }
            return matches!(self.event, Some(Event::Added | Event::Changed));
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use librift::network::{Network, Picture, Security, Wired, Wireless};

    #[test]
    fn the_volume_line_reads_back() {
        assert_eq!(
            read_volume("Volume: 0.40\n"),
            Some(Volume {
                level: 40,
                muted: false
            })
        );
        assert_eq!(
            read_volume("Volume: 0.65 [MUTED]\n"),
            Some(Volume {
                level: 65,
                muted: true
            })
        );
        assert_eq!(
            read_volume("Volume: 1.30\n"),
            Some(Volume {
                level: 130,
                muted: false
            })
        );
        assert_eq!(read_volume("Node 45 not found\n"), None);
    }

    #[test]
    fn the_volume_icon_follows_the_level() {
        let at = |level, muted| Volume { level, muted }.icon();
        assert_eq!(at(0, false), "audio-volume-muted-symbolic");
        assert_eq!(at(70, true), "audio-volume-muted-symbolic");
        assert_eq!(at(20, false), "audio-volume-low-symbolic");
        assert_eq!(at(50, false), "audio-volume-medium-symbolic");
        assert_eq!(at(100, false), "audio-volume-high-symbolic");
        assert_eq!(at(130, false), "audio-volume-overamplified-symbolic");
    }

    #[test]
    fn a_percentage_is_the_fraction_wpctl_takes() {
        assert_eq!(fraction(40), "0.40");
        assert_eq!(fraction(5), "0.05");
        assert_eq!(fraction(100), "1.00");
        assert_eq!(fraction(200), "1.00");
    }

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

    #[test]
    fn a_sink_that_changes_is_worth_a_look_and_a_program_that_connects_is_not() {
        let mut monitor = Monitor::default();
        let feed = |monitor: &mut Monitor, lines: &str| {
            lines
                .lines()
                .map(|line| monitor.line(line))
                .filter(|wanted| *wanted)
                .count()
        };
        let sink = "changed:\n\tid: 47\n\tpermissions: r-xm-\n\ttype: PipeWire:Interface:Node (version 3)\n";
        assert_eq!(feed(&mut monitor, sink), 1);
        // wpctl itself: a client comes and goes
        let client = "added:\n\tid: 56\n\tpermissions: rwxm-\n\ttype: PipeWire:Interface:Client (version 3)\n\tproperties:\n\t\tapplication.name = \"wpctl\"\nremoved:\n\tid: 56\n";
        assert_eq!(feed(&mut monitor, client), 0);
        // the sink going away is worth a look, once
        assert_eq!(feed(&mut monitor, "removed:\n\tid: 47\n"), 1);
        assert_eq!(feed(&mut monitor, "removed:\n\tid: 47\n"), 0);
        let card = "added:\n\tid: 46\n\ttype: PipeWire:Interface:Device (version 3)\n";
        assert_eq!(feed(&mut monitor, card), 1);
    }
}
