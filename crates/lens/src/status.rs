//! What the icons at the right of the bar say: the network from `NetworkManager`, the volume from
//! `PipeWire` and the battery from `UPower`, each read by asking the program that owns it. An icon
//! is there only when the system has something to say, so a machine with no battery shows no
//! battery.

use std::process::Command;

use crate::icons;

/// The state of the connection `NetworkManager` is using.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    /// A cable is up.
    Wired,
    /// A wireless network is up, at this strength out of a hundred.
    Wireless(u8),
    /// There are devices but none is connected.
    Offline,
    /// The wireless radio is off and there is nothing else.
    Off,
    /// `NetworkManager` did not answer.
    Unknown,
}

impl Network {
    /// The name of the icon for this state.
    #[must_use]
    pub fn icon(self) -> String {
        match self {
            Self::Wired => "network-wired-symbolic".to_string(),
            Self::Wireless(strength) => icons::signal("wireless", strength),
            Self::Offline => "network-offline-symbolic".to_string(),
            Self::Off => "network-wireless-disabled-symbolic".to_string(),
            Self::Unknown => "network-no-route-symbolic".to_string(),
        }
    }

    /// The word `lens --state` prints for it.
    #[must_use]
    pub fn word(self) -> String {
        match self {
            Self::Wired => "wired".to_string(),
            Self::Wireless(strength) => format!("wifi {strength}"),
            Self::Offline => "offline".to_string(),
            Self::Off => "off".to_string(),
            Self::Unknown => "unknown".to_string(),
        }
    }
}

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

/// What `UPower` says about the battery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Battery {
    /// Percent of full.
    pub level: u8,
    /// On the charger.
    pub charging: bool,
    /// Full and on the charger.
    pub full: bool,
}

impl Battery {
    /// The name of the icon for this level and state.
    #[must_use]
    pub fn icon(self) -> String {
        let step = u16::from(self.level) / 10 * 10;
        if self.full {
            "battery-level-100-charged-symbolic".to_string()
        } else if self.charging {
            format!("battery-level-{step}-charging-symbolic")
        } else {
            format!("battery-level-{step}-symbolic")
        }
    }

    /// The words `lens --state` prints for it.
    #[must_use]
    pub fn word(self) -> String {
        let state = if self.full {
            " charged"
        } else if self.charging {
            " charging"
        } else {
            ""
        };
        format!("{}{state}", self.level)
    }
}

/// Everything the right of the bar shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    /// The connection.
    pub network: Network,
    /// The default sink, when there is one.
    pub volume: Option<Volume>,
    /// The battery, when the machine has one.
    pub battery: Option<Battery>,
}

impl Default for Status {
    fn default() -> Self {
        Self {
            network: Network::Unknown,
            volume: None,
            battery: None,
        }
    }
}

impl Status {
    /// Ask each program what it has. Runs child processes, so the bar's own thread calls it, not
    /// the one that draws.
    #[must_use]
    pub fn read() -> Self {
        Self {
            network: network(),
            volume: volume(),
            battery: battery(),
        }
    }
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

fn network() -> Network {
    let Some(devices) = ask("nmcli", ["-t", "-f", "TYPE,STATE", "device", "status"]) else {
        return Network::Unknown;
    };
    match read_devices(&devices) {
        Network::Wireless(_) => Network::Wireless(
            ask("nmcli", ["-t", "-f", "IN-USE,SIGNAL", "device", "wifi"])
                .as_deref()
                .and_then(read_signal)
                .unwrap_or(0),
        ),
        Network::Off => {
            // a wireless device that is unavailable is usually the radio, which nmcli knows about
            match ask("nmcli", ["-t", "radio", "wifi"])
                .as_deref()
                .map(str::trim)
            {
                Some("enabled") => Network::Offline,
                _ => Network::Off,
            }
        }
        other => other,
    }
}

/// `nmcli -t -f TYPE,STATE device status`: one `type:state` line per device, the connected ones
/// first. A cable wins over wireless, the way the icon in every desktop does.
fn read_devices(printed: &str) -> Network {
    let mut wireless = false;
    let mut radio = false;
    for line in printed.lines() {
        let Some((kind, state)) = line.split_once(':') else {
            continue;
        };
        let state = state.trim();
        match kind.trim() {
            "ethernet" | "bridge" | "bond" if state == "connected" => return Network::Wired,
            "wifi" | "wifi-p2p" => {
                radio = true;
                if state == "connected" {
                    wireless = true;
                }
            }
            _ => {}
        }
    }
    if wireless {
        Network::Wireless(0)
    } else if radio {
        Network::Off
    } else if printed.trim().is_empty() {
        Network::Unknown
    } else {
        Network::Offline
    }
}

/// `nmcli -t -f IN-USE,SIGNAL device wifi`: the network in use is the line that starts with a
/// star.
fn read_signal(printed: &str) -> Option<u8> {
    printed.lines().find_map(|line| {
        let (used, signal) = line.split_once(':')?;
        if used.trim() == "*" {
            signal.trim().parse().ok()
        } else {
            None
        }
    })
}

fn volume() -> Option<Volume> {
    read_volume(&ask("wpctl", ["get-volume", "@DEFAULT_AUDIO_SINK@"])?)
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

fn battery() -> Option<Battery> {
    let paths = ask("upower", ["--enumerate"])?;
    let path = paths
        .lines()
        .map(str::trim)
        .find(|line| line.contains("/battery_"))?;
    read_battery(&ask("upower", ["--show-info", path])?)
}

/// `upower --show-info <path>` prints `state:` and `percentage:` in its battery block.
fn read_battery(printed: &str) -> Option<Battery> {
    let mut level = None;
    let mut state = None;
    for line in printed.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "percentage" => level = value.trim_end_matches('%').parse::<f32>().ok(),
            "state" => state = Some(value.to_string()),
            _ => {}
        }
    }
    let level = level?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let level = level.clamp(0.0, 100.0).round() as u8;
    let state = state.unwrap_or_default();
    Some(Battery {
        level,
        charging: state == "charging" || state == "pending-charge",
        full: state == "fully-charged",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cable_wins_over_wireless() {
        let printed = "ethernet:connected\nwifi:connected\nloopback:unmanaged\n";
        assert_eq!(read_devices(printed), Network::Wired);
    }

    #[test]
    fn wireless_alone_is_wireless() {
        assert_eq!(
            read_devices("wifi:connected\nethernet:unavailable\n"),
            Network::Wireless(0)
        );
    }

    #[test]
    fn a_wireless_device_that_is_not_connected_may_be_the_radio() {
        assert_eq!(read_devices("wifi:unavailable\n"), Network::Off);
        assert_eq!(read_devices("ethernet:disconnected\n"), Network::Offline);
        assert_eq!(read_devices(""), Network::Unknown);
    }

    #[test]
    fn the_signal_comes_from_the_network_in_use() {
        let printed = "*:72\n :100\n :43\n";
        assert_eq!(read_signal(printed), Some(72));
        assert_eq!(read_signal(" :100\n :43\n"), None);
    }

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
    fn the_battery_block_reads_back() {
        let printed = "  native-path:          BAT0\n  power supply:         yes\n  battery\n    present:             yes\n    state:               discharging\n    percentage:          72%\n";
        assert_eq!(
            read_battery(printed),
            Some(Battery {
                level: 72,
                charging: false,
                full: false
            })
        );
        let charging = "    state:               charging\n    percentage:          9.5%\n";
        let battery = read_battery(charging).expect("the battery");
        assert_eq!(battery.level, 10);
        assert!(battery.charging);
        assert_eq!(battery.icon(), "battery-level-10-charging-symbolic");
        let charged = "    state:               fully-charged\n    percentage:          100%\n";
        let battery = read_battery(charged).expect("the battery");
        assert!(battery.full);
        assert_eq!(battery.icon(), "battery-level-100-charged-symbolic");
        assert_eq!(battery.word(), "100 charged");
        assert_eq!(read_battery("  daemon\n    on-battery: no\n"), None);
    }

    #[test]
    fn the_state_words_say_what_the_icons_show() {
        assert_eq!(Network::Wired.word(), "wired");
        assert_eq!(Network::Wireless(72).word(), "wifi 72");
        assert_eq!(Network::Wired.icon(), "network-wired-symbolic");
        assert_eq!(
            Network::Wireless(72).icon(),
            "network-wireless-signal-good-symbolic"
        );
        assert_eq!(
            Volume {
                level: 40,
                muted: true
            }
            .word(),
            "40 muted"
        );
    }
}
