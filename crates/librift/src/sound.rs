//! `PipeWire` from a client's side: the default output and the default input, the volume of each,
//! and the devices there are to pick between. `wpctl` is the program that answers for it, the way
//! nmcli answers for networks, and `pw-mon` says when something has changed.
//!
//! The shell's system menu and the Sound page in Settings both read it here, so the two say the
//! same about the same machine.

use std::collections::HashSet;

use crate::os;

/// Which half of the sound something is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// What the machine plays: the default sink.
    Output,
    /// What it hears: the default source.
    Input,
}

impl Side {
    /// Both halves, the output first, which is the one a page leads with.
    pub const ALL: [Side; 2] = [Side::Output, Side::Input];

    /// The name `wpctl` takes for whichever device is the default one.
    #[must_use]
    pub const fn target(self) -> &'static str {
        match self {
            Self::Output => "@DEFAULT_AUDIO_SINK@",
            Self::Input => "@DEFAULT_AUDIO_SOURCE@",
        }
    }

    /// The word a state line and a setting are named with.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Output => "output",
            Self::Input => "input",
        }
    }

    /// What `wpctl status` heads this half's list with.
    const fn list(self) -> &'static str {
        match self {
            Self::Output => "Sinks",
            Self::Input => "Sources",
        }
    }
}

/// A volume, as a percentage, and whether it is muted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Volume {
    /// Percent of full. `PipeWire` allows more than a hundred.
    pub level: u16,
    /// Muted, whatever the level is.
    pub muted: bool,
}

impl Volume {
    /// The name of the icon for this level, for the half of the sound it belongs to.
    #[must_use]
    pub fn icon(self, side: Side) -> &'static str {
        if side == Side::Input {
            return if self.muted {
                "microphone-disabled-symbolic"
            } else {
                "audio-input-microphone-symbolic"
            };
        }
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

    /// The words a state line prints for it.
    #[must_use]
    pub fn word(self) -> String {
        if self.muted {
            format!("{} muted", self.level)
        } else {
            self.level.to_string()
        }
    }
}

/// One device the sound can be played on or taken from, as `PipeWire` numbers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// The node id, which is what `wpctl` is given to pick one.
    pub id: u32,
    /// What it calls itself.
    pub name: String,
    /// Whether it is the one being used.
    pub default: bool,
}

/// What `PipeWire` has: the volume of each half, and the devices of each half.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Picture {
    /// The default sink's volume, when there is a sink.
    pub output: Option<Volume>,
    /// The default source's volume, when there is a source.
    pub input: Option<Volume>,
    /// The sinks, the one in use marked.
    pub outputs: Vec<Device>,
    /// The sources, the same way.
    pub inputs: Vec<Device>,
}

impl Picture {
    /// The volume of one half.
    #[must_use]
    pub fn volume(&self, side: Side) -> Option<Volume> {
        match side {
            Side::Output => self.output,
            Side::Input => self.input,
        }
    }

    /// The devices of one half.
    #[must_use]
    pub fn devices(&self, side: Side) -> &[Device] {
        match side {
            Side::Output => &self.outputs,
            Side::Input => &self.inputs,
        }
    }

    /// The device of one half that is being used.
    #[must_use]
    pub fn using(&self, side: Side) -> Option<&Device> {
        self.devices(side).iter().find(|device| device.default)
    }
}

/// What `PipeWire` has now.
///
/// # Errors
///
/// A sentence when `wpctl` is not there or `PipeWire` does not answer it.
pub fn read() -> Result<Picture, String> {
    let printed = os::asked("wpctl", &["status"])?;
    let (outputs, inputs) = devices(&printed);
    Ok(Picture {
        output: volume(Side::Output),
        input: volume(Side::Input),
        outputs,
        inputs,
    })
}

/// The volume of one half now, or `None` when the machine has no device for it.
#[must_use]
pub fn volume(side: Side) -> Option<Volume> {
    read_volume(&os::ask("wpctl", &["get-volume", side.target()])?)
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

/// Set one half's volume, and unmute it: moving the slider is asking to hear something.
///
/// # Errors
///
/// When `wpctl` is not there or refuses.
pub fn set_volume(side: Side, percent: u8) -> Result<(), String> {
    os::change("wpctl", &["set-volume", side.target(), &fraction(percent)])?;
    set_muted(side, false)
}

/// A percentage as the fraction `wpctl` takes: 40 is `0.40`.
fn fraction(percent: u8) -> String {
    let percent = percent.min(100);
    format!("{}.{:02}", percent / 100, percent % 100)
}

/// Mute one half or unmute it.
///
/// # Errors
///
/// When `wpctl` is not there or refuses.
pub fn set_muted(side: Side, muted: bool) -> Result<(), String> {
    os::change(
        "wpctl",
        &["set-mute", side.target(), if muted { "1" } else { "0" }],
    )
}

/// Mute one half if it is playing and unmute it if it is not.
///
/// # Errors
///
/// When `wpctl` is not there or refuses.
pub fn toggle_mute(side: Side) -> Result<(), String> {
    os::change("wpctl", &["set-mute", side.target(), "toggle"])
}

/// Turn the output up by a step, never past full, and unmute it: a key that turns the sound up is
/// asking to hear something. Down is the same step the other way and leaves a muted sink muted.
///
/// # Errors
///
/// When `wpctl` is not there or refuses.
pub fn step_volume(up: bool) -> Result<(), String> {
    let sink = Side::Output.target();
    if up {
        set_muted(Side::Output, false)?;
        os::change("wpctl", &["set-volume", sink, "0.05+", "-l", "1.0"])
    } else {
        os::change("wpctl", &["set-volume", sink, "0.05-"])
    }
}

/// Use this device, by the id `wpctl status` gave it. `PipeWire` remembers the choice.
///
/// # Errors
///
/// When `wpctl` is not there or refuses.
pub fn set_default(id: u32) -> Result<(), String> {
    os::change("wpctl", &["set-default", &id.to_string()])
}

/// The devices `wpctl status` lists, the outputs and then the inputs. Only its Audio section
/// counts: the Video one lists the cameras as sources of their own.
fn devices(printed: &str) -> (Vec<Device>, Vec<Device>) {
    let (mut outputs, mut inputs) = (Vec::new(), Vec::new());
    let mut audio = false;
    let mut listing: Option<Side> = None;
    for line in printed.lines() {
        // Audio, Video and Settings are headed at the left margin; everything under one of them is
        // indented, and drawn into a tree
        if !line.starts_with(char::is_whitespace) {
            audio = line.trim() == "Audio";
            listing = None;
            continue;
        }
        let said = bare(line);
        if said.is_empty() {
            continue;
        }
        if let Some(name) = said.strip_suffix(':') {
            listing = Side::ALL.into_iter().find(|side| side.list() == name);
            continue;
        }
        let (Some(side), Some(device)) = (listing.filter(|_| audio), device(said)) else {
            continue;
        };
        match side {
            Side::Output => outputs.push(device),
            Side::Input => inputs.push(device),
        }
    }
    (outputs, inputs)
}

/// A line of `wpctl status` without the lines it draws its tree with.
fn bare(line: &str) -> &str {
    line.trim_matches(|c: char| c.is_whitespace() || ('\u{2500}'..='\u{257f}').contains(&c))
}

/// One row of a list: `*   51. Built-in Audio Analog Stereo   [vol: 0.40]`, where the star is on
/// the device being used and the number is the id `wpctl` takes.
fn device(said: &str) -> Option<Device> {
    let (default, rest) = said
        .strip_prefix('*')
        .map_or((false, said), |rest| (true, rest.trim_start()));
    let (id, rest) = rest.split_once('.')?;
    let id: u32 = id.trim().parse().ok()?;
    let name = rest
        .rsplit_once('[')
        .filter(|(_, after)| after.trim_end().ends_with(']'))
        .map_or(rest, |(name, _)| name)
        .trim();
    (!name.is_empty()).then(|| Device {
        id,
        name: name.to_string(),
        default,
    })
}

/// Reads what `pw-mon` prints and says which of its lines mean the sound may have changed: a sink
/// or a card that came, changed or went. A program connecting to `PipeWire` is an event too, and
/// every `wpctl` that is run is one, so those are not.
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
    /// Take one line in. True when the sound should be read again.
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

    /// What `wpctl status` prints on a machine with one card, a second output over HDMI and a
    /// camera, with the tree it draws around it.
    const STATUS: &str = "PipeWire 'pipewire-0' [1.2.7, rift@rift, cookie:1234567]
 \u{2514}\u{2500} Clients:
        32. WirePlumber                         [1.2.7, rift@rift, pid:600]
        45. wpctl                               [1.2.7, rift@rift, pid:1201]

Audio
 \u{251c}\u{2500} Devices:
 \u{2502}      46. Built-in Audio                      [alsa]
 \u{2502}  
 \u{251c}\u{2500} Sinks:
 \u{2502}  *   51. Built-in Audio Analog Stereo        [vol: 0.40]
 \u{2502}      52. HDMI / DisplayPort                  [vol: 1.00]
 \u{2502}  
 \u{251c}\u{2500} Sink endpoints:
 \u{2502}  
 \u{251c}\u{2500} Sources:
 \u{2502}  *   53. Built-in Audio Analog Stereo        [vol: 1.00 MUTED]
 \u{2502}  
 \u{251c}\u{2500} Source endpoints:
 \u{2502}  
 \u{2514}\u{2500} Streams:

Video
 \u{251c}\u{2500} Devices:
 \u{2502}      54. Integrated Camera                   [v4l2]
 \u{2502}  
 \u{251c}\u{2500} Sinks:
 \u{2502}  
 \u{251c}\u{2500} Sources:
 \u{2502}      55. Integrated Camera (V4L2)
 \u{2502}  
 \u{2514}\u{2500} Streams:

Settings
 \u{2514}\u{2500} Default Configured Devices:
         0. Audio/Sink    alsa_output.pci-0000_00_1f.3.analog-stereo
";

    #[test]
    fn the_status_lists_the_outputs_and_the_inputs_with_the_one_in_use_marked() {
        let (outputs, inputs) = devices(STATUS);
        assert_eq!(
            outputs,
            vec![
                Device {
                    id: 51,
                    name: "Built-in Audio Analog Stereo".to_string(),
                    default: true
                },
                Device {
                    id: 52,
                    name: "HDMI / DisplayPort".to_string(),
                    default: false
                },
            ]
        );
        // the camera is a source of the Video section, and the Settings section names no device
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].id, 53);
        assert!(inputs[0].default);
        let picture = Picture {
            outputs,
            inputs,
            ..Picture::default()
        };
        assert_eq!(
            picture.using(Side::Output).map(|device| device.id),
            Some(51)
        );
        assert_eq!(picture.devices(Side::Input).len(), 1);
    }

    #[test]
    fn a_machine_with_no_sound_card_lists_nothing() {
        let bare = "PipeWire 'pipewire-0' [1.2.7, rift@rift, cookie:7]\n\nAudio\n \u{251c}\u{2500} Devices:\n \u{2502}  \n \u{251c}\u{2500} Sinks:\n \u{2502}  \n \u{251c}\u{2500} Sources:\n \u{2502}  \n \u{2514}\u{2500} Streams:\n";
        assert_eq!(devices(bare), (Vec::new(), Vec::new()));
        // and nothing that is not a row of a list is taken for one
        assert_eq!(device("Sink endpoints:"), None);
        assert_eq!(device("*   51."), None);
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
    fn the_volume_icon_follows_the_level_and_the_half_it_is_of() {
        let at = |level, muted| Volume { level, muted }.icon(Side::Output);
        assert_eq!(at(0, false), "audio-volume-muted-symbolic");
        assert_eq!(at(70, true), "audio-volume-muted-symbolic");
        assert_eq!(at(20, false), "audio-volume-low-symbolic");
        assert_eq!(at(50, false), "audio-volume-medium-symbolic");
        assert_eq!(at(100, false), "audio-volume-high-symbolic");
        assert_eq!(at(130, false), "audio-volume-overamplified-symbolic");
        let heard = |muted| Volume { level: 60, muted }.icon(Side::Input);
        assert_eq!(heard(false), "audio-input-microphone-symbolic");
        assert_eq!(heard(true), "microphone-disabled-symbolic");
        assert_eq!(
            Volume {
                level: 60,
                muted: true
            }
            .word(),
            "60 muted"
        );
    }

    #[test]
    fn a_percentage_is_the_fraction_wpctl_takes() {
        assert_eq!(fraction(0), "0.00");
        assert_eq!(fraction(7), "0.07");
        assert_eq!(fraction(40), "0.40");
        assert_eq!(fraction(100), "1.00");
        assert_eq!(fraction(200), "1.00");
    }

    #[test]
    fn each_half_names_the_device_wpctl_takes() {
        assert_eq!(Side::Output.target(), "@DEFAULT_AUDIO_SINK@");
        assert_eq!(Side::Input.target(), "@DEFAULT_AUDIO_SOURCE@");
        assert_eq!(Side::ALL.map(Side::word), ["output", "input"]);
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
