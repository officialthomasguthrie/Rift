//! The mouse and the touchpad: how the owner wants them to behave, which Settings keeps in a file
//! of its own under `~/.config/rift` and hands to Horizon as a part of its config, and which of the
//! two this machine has, out of the kernel's list of input devices.
//!
//! The part holds whole `mouse`, `trackpoint`, `trackball` and `touchpad` blocks. The system config
//! includes it after its own input block, and a block in an include takes the place of the one
//! before it rather than adding to it, so every setting is written every time, the image's own
//! defaults with them.

use std::fmt::Write as _;
use std::fs;

use crate::appearance::{home, write_beside};

/// Where the settings live, under home: a line each, the way `rift-settings --state` prints them.
pub const SETTING: &str = ".config/rift/pointer";
/// Where the part of Horizon's config goes, under home. Horizon reads its config again when the
/// file changes.
pub const HORIZON_PART: &str = ".local/state/rift/pointer.kdl";
/// The kernel's list of input devices, which anyone may read.
pub const DEVICES: &str = "/proc/bus/input/devices";
/// The slowest the pointer moves, in tenths of libinput's range. Nought is libinput's own speed.
pub const SPEED_LEAST: i32 = -10;
/// And the fastest.
pub const SPEED_MOST: i32 = 10;

/// The names of the settings, in the order the file and `rift-settings --state` have them.
pub const NAMES: [&str; 9] = [
    "primary-button",
    "mouse-speed",
    "mouse-acceleration",
    "mouse-natural-scrolling",
    "touchpad-speed",
    "tap-to-click",
    "touchpad-natural-scrolling",
    "disable-while-typing",
    "edge-scrolling",
];

/// The kind of pointing device a setting is for, and a device is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A mouse, and a trackpoint or a trackball, which move the pointer the same way.
    Mouse,
    /// A touchpad.
    Touchpad,
}

impl Kind {
    /// The kind of device a setting is for, by its name. The primary button is for both.
    #[must_use]
    pub fn of(name: &str) -> Option<Self> {
        match name {
            "mouse-speed" | "mouse-acceleration" | "mouse-natural-scrolling" => Some(Self::Mouse),
            "touchpad-speed"
            | "tap-to-click"
            | "touchpad-natural-scrolling"
            | "disable-while-typing"
            | "edge-scrolling" => Some(Self::Touchpad),
            _ => None,
        }
    }

    /// The word `rift-settings --state` prints before a device's name.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Mouse => "mouse",
            Self::Touchpad => "touchpad",
        }
    }
}

/// How a touchpad scrolls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Scrolling {
    /// With two fingers anywhere on it, which is what libinput does on a touchpad that can tell two
    /// fingers apart.
    #[default]
    TwoFingers,
    /// With one finger along its right edge.
    Edge,
}

/// The mouse, and a trackpoint or a trackball beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mouse {
    /// How fast the pointer moves, from -10 to 10.
    pub speed: i32,
    /// Whether the pointer goes further when the mouse moves faster, which is libinput's adaptive
    /// profile. Off is the flat one.
    pub acceleration: bool,
    /// Whether scrolling moves the content rather than the view.
    pub natural: bool,
}

impl Default for Mouse {
    fn default() -> Self {
        Self {
            speed: 0,
            acceleration: true,
            natural: false,
        }
    }
}

/// The touchpad. The image turns tapping, natural scrolling and being off while typing on, the way
/// most laptops come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Touchpad {
    /// How fast the pointer moves, from -10 to 10.
    pub speed: i32,
    /// Whether a tap clicks.
    pub tap: bool,
    /// Whether scrolling moves the content rather than the view.
    pub natural: bool,
    /// Whether it ignores a touch while a key is being typed.
    pub off_while_typing: bool,
    /// How it scrolls.
    pub scrolling: Scrolling,
}

impl Default for Touchpad {
    fn default() -> Self {
        Self {
            speed: 0,
            tap: true,
            natural: true,
            off_while_typing: true,
            scrolling: Scrolling::TwoFingers,
        }
    }
}

/// Every setting of the mouse and the touchpad.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pointer {
    /// Whether the right button is the one that clicks, on every mouse and touchpad.
    pub left_handed: bool,
    /// The mouse.
    pub mouse: Mouse,
    /// The touchpad.
    pub touchpad: Touchpad,
}

/// On or off, in the words of the file.
const fn switch(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

/// On or off out of the file's words, or nothing for a word that is neither.
fn flag(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

/// A speed out of the file's words, kept inside its range.
fn speed(value: &str) -> Option<i32> {
    value
        .trim()
        .parse::<i32>()
        .ok()
        .map(|speed| speed.clamp(SPEED_LEAST, SPEED_MOST))
}

/// A speed as libinput takes it, from -1 to 1 with one decimal.
fn accel(speed: i32) -> String {
    format!("{:.1}", f64::from(speed) / 10.0)
}

impl Pointer {
    /// The owner's settings, or the image's own where there is no file or a line is not there.
    #[must_use]
    pub fn read() -> Self {
        home()
            .and_then(|home| fs::read_to_string(home.join(SETTING)).ok())
            .map_or_else(Self::default, |text| Self::parse(&text))
    }

    /// The settings a file holds, the image's own for any it does not.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut pointer = Self::default();
        for line in text.lines() {
            if let Some((name, value)) = line.trim().split_once(' ') {
                pointer.set(name, value);
            }
        }
        pointer
    }

    /// Change one setting by its name, the way the file and `rift-settings --set` name it. Whether
    /// the name and the value were both understood.
    pub fn set(&mut self, name: &str, value: &str) -> bool {
        match name {
            "primary-button" => match value.trim().to_ascii_lowercase().as_str() {
                "left" => self.left_handed = false,
                "right" => self.left_handed = true,
                _ => return false,
            },
            "mouse-speed" | "touchpad-speed" => {
                let Some(speed) = speed(value) else {
                    return false;
                };
                if name == "mouse-speed" {
                    self.mouse.speed = speed;
                } else {
                    self.touchpad.speed = speed;
                }
            }
            _ => {
                let Some(on) = flag(value) else {
                    return false;
                };
                match name {
                    "mouse-acceleration" => self.mouse.acceleration = on,
                    "mouse-natural-scrolling" => self.mouse.natural = on,
                    "tap-to-click" => self.touchpad.tap = on,
                    "touchpad-natural-scrolling" => self.touchpad.natural = on,
                    "disable-while-typing" => self.touchpad.off_while_typing = on,
                    "edge-scrolling" => {
                        self.touchpad.scrolling = if on {
                            Scrolling::Edge
                        } else {
                            Scrolling::TwoFingers
                        };
                    }
                    _ => return false,
                }
            }
        }
        true
    }

    /// A line for each setting, its name and its value, in the order of [`NAMES`].
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let values = [
            (if self.left_handed { "right" } else { "left" }).to_string(),
            self.mouse.speed.to_string(),
            switch(self.mouse.acceleration).to_string(),
            switch(self.mouse.natural).to_string(),
            self.touchpad.speed.to_string(),
            switch(self.touchpad.tap).to_string(),
            switch(self.touchpad.natural).to_string(),
            switch(self.touchpad.off_while_typing).to_string(),
            switch(self.touchpad.scrolling == Scrolling::Edge).to_string(),
        ];
        NAMES
            .iter()
            .zip(values)
            .map(|(name, value)| format!("{name} {value}"))
            .collect()
    }

    /// The part of Horizon's config for these settings: the mouse's block, the same for a
    /// trackpoint and a trackball, and the touchpad's.
    #[must_use]
    pub fn horizon(&self) -> String {
        let mut part = String::from("// written from ~/.config/rift/pointer\ninput {\n");
        let mut mouse = String::new();
        let _ = writeln!(mouse, "        accel-speed {}", accel(self.mouse.speed));
        if !self.mouse.acceleration {
            mouse.push_str("        accel-profile \"flat\"\n");
        }
        if self.mouse.natural {
            mouse.push_str("        natural-scroll\n");
        }
        if self.left_handed {
            mouse.push_str("        left-handed\n");
        }
        for device in ["mouse", "trackpoint", "trackball"] {
            let _ = write!(part, "    {device} {{\n{mouse}    }}\n");
        }
        let pad = &self.touchpad;
        part.push_str("    touchpad {\n");
        let _ = writeln!(part, "        accel-speed {}", accel(pad.speed));
        for (on, node) in [
            (pad.tap, "tap"),
            (pad.natural, "natural-scroll"),
            (pad.off_while_typing, "dwt"),
            (pad.scrolling == Scrolling::Edge, "scroll-method \"edge\""),
            (self.left_handed, "left-handed"),
        ] {
            if on {
                let _ = writeln!(part, "        {node}");
            }
        }
        part.push_str("    }\n}\n");
        part
    }

    /// Write the settings and the part of Horizon's config made from them. Horizon reads its config
    /// again when the part changes, so the mouse and the touchpad follow at once.
    ///
    /// # Errors
    ///
    /// A sentence when there is no home or a file could not be written.
    pub fn save(&self) -> Result<(), String> {
        let home = home().ok_or("There is no home to keep the mouse settings in.")?;
        write_beside(&home.join(SETTING), &(self.lines().join("\n") + "\n"))?;
        self.write_horizon()
    }

    /// Write the part of Horizon's config.
    ///
    /// # Errors
    ///
    /// A sentence when there is no home or the file could not be written.
    pub fn write_horizon(&self) -> Result<(), String> {
        let path = home()
            .ok_or("There is no home to write the compositor's part into.")?
            .join(HORIZON_PART);
        write_beside(&path, &self.horizon()).map(|_| ())
    }
}

/// Write the part of Horizon's config again from the owner's settings, when there are any. The
/// shell does this when the session starts, so a part written by an older image is written the way
/// this one writes it. A drive whose owner has chosen nothing has no part, and the system config's
/// own blocks hold.
///
/// # Errors
///
/// A sentence when the file could not be written.
pub fn apply() -> Result<(), String> {
    let chosen = home().is_some_and(|home| home.join(SETTING).is_file());
    if chosen {
        Pointer::read().write_horizon()
    } else {
        Ok(())
    }
}

/// One pointing device the kernel knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// What it calls itself, `SynPS/2 Synaptics TouchPad`.
    pub name: String,
    /// Whether it is a mouse or a touchpad.
    pub kind: Kind,
}

/// The pointing devices plugged in now, mice first.
///
/// # Errors
///
/// A sentence when the kernel's list could not be read.
pub fn devices() -> Result<Vec<Device>, String> {
    fs::read_to_string(DEVICES)
        .map(|text| parse_devices(&text))
        .map_err(|e| format!("Could not read {DEVICES}: {e}"))
}

/// The pointing devices in the kernel's list, mice first, each kind in the kernel's order.
#[must_use]
pub fn parse_devices(text: &str) -> Vec<Device> {
    let mut found: Vec<Device> = text
        .split("\n\n")
        .filter_map(|block| {
            let (name, bits) = described(block);
            Some(Device {
                name: name?,
                kind: kind(&bits)?,
            })
        })
        .collect();
    found.sort_by_key(|device| device.kind == Kind::Touchpad);
    found
}

/// A device's bitmaps: which kinds of event it sends, and which keys, relative axes, absolute axes
/// and properties it has.
#[derive(Debug, Default)]
struct Bits {
    events: Vec<u64>,
    keys: Vec<u64>,
    relative: Vec<u64>,
    absolute: Vec<u64>,
    properties: Vec<u64>,
}

/// Whether a bit is set in a bitmap the kernel printed, a word at a time from the lowest.
fn has(words: &[u64], bit: usize) -> bool {
    words
        .get(bit / 64)
        .is_some_and(|word| (word >> (bit % 64)) & 1 == 1)
}

/// A bitmap as the kernel prints it: hexadecimal words of 64 bits, the highest first.
fn bitmap(text: &str) -> Vec<u64> {
    text.split_whitespace()
        .rev()
        .map(|word| u64::from_str_radix(word, 16).unwrap_or(0))
        .collect()
}

/// The name and the bitmaps of one device out of its block in the kernel's list.
fn described(block: &str) -> (Option<String>, Bits) {
    let mut name = None;
    let mut bits = Bits::default();
    for line in block.lines() {
        if let Some(rest) = line.strip_prefix("N: Name=") {
            name = Some(rest.trim().trim_matches('"').to_string());
        } else if let Some(rest) = line.strip_prefix("B: ") {
            let Some((what, value)) = rest.split_once('=') else {
                continue;
            };
            let words = bitmap(value);
            match what {
                "EV" => bits.events = words,
                "KEY" => bits.keys = words,
                "REL" => bits.relative = words,
                "ABS" => bits.absolute = words,
                "PROP" => bits.properties = words,
                _ => {}
            }
        }
    }
    (name, bits)
}

/// Whether a device is a mouse, a touchpad or neither, by the rules udev's `input_id` tags them with,
/// in its order, which libinput and Horizon then read: a pen makes a drawing tablet, a finger on a
/// surface that is not a screen a touchpad, a trackpoint a mouse, a mouse button on absolute axes a
/// mouse, which is what a virtual machine's tablet is, and a mouse button a mouse on anything that
/// is not a joystick or a tablet's pad.
fn kind(bits: &Bits) -> Option<Kind> {
    const EV_KEY: usize = 1;
    const EV_REL: usize = 2;
    const REL_X: usize = 0;
    const REL_Y: usize = 1;
    const REL_HWHEEL: usize = 6;
    const REL_WHEEL: usize = 8;
    const ABS_X: usize = 0;
    const ABS_Y: usize = 1;
    const ABS_Z: usize = 2;
    const ABS_RX: usize = 3;
    const ABS_PRESSURE: usize = 0x18;
    const ABS_MT_SLOT: usize = 0x2f;
    const ABS_MT_POSITION_X: usize = 0x35;
    const ABS_MT_POSITION_Y: usize = 0x36;
    const BTN_0: usize = 0x100;
    const BTN_1: usize = 0x101;
    const BTN_MOUSE: usize = 0x110;
    const BTN_JOYSTICK: usize = 0x120;
    const BTN_DIGI: usize = 0x140;
    const BTN_TOOL_PEN: usize = 0x140;
    const BTN_TOOL_FINGER: usize = 0x145;
    const BTN_STYLUS: usize = 0x14b;
    const BTN_DPAD_UP: usize = 0x220;
    const BTN_DPAD_RIGHT: usize = 0x223;
    const BTN_TRIGGER_HAPPY1: usize = 0x2c0;
    const BTN_TRIGGER_HAPPY40: usize = 0x2e7;
    const PROP_DIRECT: usize = 1;
    const PROP_POINTING_STICK: usize = 5;
    const PROP_ACCELEROMETER: usize = 6;

    let key = |bit| has(&bits.keys, bit);
    let abs = |bit| has(&bits.absolute, bit);
    let rel = |bit| has(&bits.events, EV_REL) && has(&bits.relative, bit);
    let prop = |bit| has(&bits.properties, bit);
    let positioned = abs(ABS_X) && abs(ABS_Y);
    // a device that reports every axis there is claims the touch axes along with the rest
    let touches = abs(ABS_MT_POSITION_X)
        && abs(ABS_MT_POSITION_Y)
        && !(abs(ABS_MT_SLOT) && abs(ABS_MT_SLOT - 1));
    if prop(PROP_ACCELEROMETER) || (!has(&bits.events, EV_KEY) && positioned && abs(ABS_Z)) {
        return None;
    }
    let pen = key(BTN_TOOL_PEN) || key(BTN_STYLUS);
    let finger = key(BTN_TOOL_FINGER) && !key(BTN_TOOL_PEN);
    if (positioned || touches) && pen {
        return None;
    }
    if (positioned || touches) && finger && !prop(PROP_DIRECT) {
        return Some(Kind::Touchpad);
    }
    if prop(PROP_POINTING_STICK) {
        return Some(Kind::Mouse);
    }
    let buttons = (BTN_MOUSE..BTN_JOYSTICK).any(key);
    if positioned {
        return buttons.then_some(Kind::Mouse);
    }
    // a mouse with more than sixteen buttons runs into the joystick's, which udev allows for
    let joystick = (!key(BTN_JOYSTICK - 1)
        && ((BTN_JOYSTICK..BTN_DIGI).any(key)
            || (BTN_TRIGGER_HAPPY1..=BTN_TRIGGER_HAPPY40).any(key)
            || (BTN_DPAD_UP..=BTN_DPAD_RIGHT).any(key)))
        || (ABS_RX..ABS_PRESSURE).any(abs);
    let pad = key(BTN_0)
        && key(BTN_1)
        && !key(BTN_TOOL_PEN)
        && (rel(REL_WHEEL) || rel(REL_HWHEEL))
        && !(rel(REL_X) && rel(REL_Y));
    (buttons && !joystick && !pad).then_some(Kind::Mouse)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A laptop with a trackpoint, a USB mouse, a touch screen and a pen tablet plugged in, the way
    /// the kernel lists it, a gamepad and a tablet's pad that each have a mouse button, a pointer
    /// on a screen, and the two pointers of a qemu machine.
    const DEVICES_LISTED: &str = "\
I: Bus=0011 Vendor=0001 Product=0001 Version=ab41
N: Name=\"AT Translated Set 2 keyboard\"
P: Phys=isa0060/serio0/input0
S: Sysfs=/devices/platform/i8042/serio0/input/input0
U: Uniq=
H: Handlers=sysrq kbd event0 leds
B: PROP=0
B: EV=120013
B: KEY=402000000 3803078f800d001 feffffdfffefffff fffffffffffffffe
B: MSC=10
B: LED=7

I: Bus=0011 Vendor=0002 Product=0007 Version=01b1
N: Name=\"SynPS/2 Synaptics TouchPad\"
P: Phys=isa0060/serio1/input0
S: Sysfs=/devices/platform/i8042/serio1/input/input5
U: Uniq=
H: Handlers=mouse0 event4
B: PROP=5
B: EV=b
B: KEY=e520 10000 0 0 0 0
B: ABS=660800011000003

I: Bus=0011 Vendor=0002 Product=000a Version=0000
N: Name=\"TPPS/2 IBM TrackPoint\"
P: Phys=synaptics-pt/serio0/input0
S: Sysfs=/devices/platform/i8042/serio1/serio2/input/input6
U: Uniq=
H: Handlers=mouse1 event5
B: PROP=21
B: EV=7
B: KEY=70000 0 0 0 0
B: REL=3

I: Bus=0003 Vendor=046d Product=c52b Version=0111
N: Name=\"Logitech USB Receiver\"
P: Phys=usb-0000:00:14.0-2/input1
S: Sysfs=/devices/pci0000:00/0000:00:14.0/usb1/1-2/1-2:1.1/0003:046D:C52B.0003/input/input9
U: Uniq=
H: Handlers=kbd mouse2 event7
B: PROP=0
B: EV=17
B: KEY=ffff0000 0 0 0 0
B: REL=1943
B: MSC=10

I: Bus=0018 Vendor=04f3 Product=2b7c Version=0100
N: Name=\"ELAN Touchscreen\"
P: Phys=i2c-ELAN2514:00
S: Sysfs=/devices/platform/AMDI0010:00/i2c-0/i2c-ELAN2514:00/input/input12
U: Uniq=
H: Handlers=event9
B: PROP=2
B: EV=b
B: KEY=400 0 0 0 0 0
B: ABS=3273800000000003

I: Bus=0003 Vendor=056a Product=0374 Version=0110
N: Name=\"Wacom Intuos S Pen\"
P: Phys=usb-0000:00:14.0-3/input0
S: Sysfs=/devices/pci0000:00/0000:00:14.0/usb1/1-3/1-3:1.0/0003:056A:0374.0004/input/input13
U: Uniq=
H: Handlers=mouse3 event10
B: PROP=1
B: EV=1b
B: KEY=1c03 0 0 0 0 0
B: ABS=3000003
B: MSC=1

I: Bus=0019 Vendor=0000 Product=0001 Version=0000
N: Name=\"Power Button\"
P: Phys=LNXPWRBN/button/input0
S: Sysfs=/devices/LNXSYSTM:00/LNXPWRBN:00/input/input2
U: Uniq=
H: Handlers=kbd event2
B: PROP=0
B: EV=3
B: KEY=10000000000000 0

I: Bus=0011 Vendor=0002 Product=0006 Version=0000
N: Name=\"ImExPS/2 Generic Explorer Mouse\"
P: Phys=isa0060/serio1/input0
S: Sysfs=/devices/platform/i8042/serio1/input/input3
U: Uniq=
H: Handlers=mouse4 event3
B: PROP=1
B: EV=7
B: KEY=1f0000 0 0 0 0
B: REL=143

I: Bus=0003 Vendor=045e Product=028e Version=0114
N: Name=\"Gamepad with a mouse button\"
P: Phys=usb-0000:00:14.0-4/input0
S: Sysfs=/devices/pci0000:00/0000:00:14.0/usb1/1-4/1-4:1.0/input/input14
U: Uniq=
H: Handlers=event11 js0
B: PROP=0
B: EV=1b
B: KEY=7cdb000000010000 0 0 0 0
B: ABS=30018

I: Bus=0003 Vendor=056a Product=0374 Version=0110
N: Name=\"Wacom Intuos S Pad\"
P: Phys=usb-0000:00:14.0-3/input0
S: Sysfs=/devices/pci0000:00/0000:00:14.0/usb1/1-3/1-3:1.0/0003:056A:0374.0004/input/input15
U: Uniq=
H: Handlers=event12
B: PROP=0
B: EV=f
B: KEY=10003 0 0 0 0
B: REL=100
B: MSC=1

I: Bus=0006 Vendor=0627 Product=0003 Version=0001
N: Name=\"Pointer on a screen\"
P: Phys=virtio4/input0
S: Sysfs=/devices/pci0000:00/0000:00:05.0/virtio4/input/input16
U: Uniq=
H: Handlers=event13
B: PROP=2
B: EV=b
B: KEY=1f0000 0 0 0 0
B: ABS=3

I: Bus=0006 Vendor=0627 Product=0003 Version=0001
N: Name=\"QEMU Virtio Tablet\"
P: Phys=virtio3/input0
S: Sysfs=/devices/pci0000:00/0000:00:04.0/virtio3/input/input5
U: Uniq=
H: Handlers=mouse5 event4
B: PROP=0
B: EV=f
B: KEY=1f0000 0 0 0 0
B: REL=900
B: ABS=3
";

    #[test]
    fn the_kernels_list_says_which_devices_point() {
        let found = parse_devices(DEVICES_LISTED);
        let named: Vec<(&str, Kind)> = found
            .iter()
            .map(|device| (device.name.as_str(), device.kind))
            .collect();
        assert_eq!(
            named,
            [
                ("TPPS/2 IBM TrackPoint", Kind::Mouse),
                ("Logitech USB Receiver", Kind::Mouse),
                ("ImExPS/2 Generic Explorer Mouse", Kind::Mouse),
                ("Pointer on a screen", Kind::Mouse),
                ("QEMU Virtio Tablet", Kind::Mouse),
                ("SynPS/2 Synaptics TouchPad", Kind::Touchpad),
            ]
        );
        assert!(parse_devices("").is_empty());
    }

    #[test]
    fn a_bitmap_is_read_from_its_lowest_word() {
        let words = bitmap("1f0000 0 0 0 0");
        // the left button is 0x110, the sixteenth bit of the fifth word
        assert!(has(&words, 0x110) && has(&words, 0x114));
        assert!(!has(&words, 0x115) && !has(&words, 0) && !has(&words, 0x400));
        assert!(bitmap("").is_empty());
    }

    #[test]
    fn the_settings_are_a_line_each_and_read_back() {
        let image = Pointer::default();
        assert_eq!(
            image.lines(),
            [
                "primary-button left",
                "mouse-speed 0",
                "mouse-acceleration on",
                "mouse-natural-scrolling off",
                "touchpad-speed 0",
                "tap-to-click on",
                "touchpad-natural-scrolling on",
                "disable-while-typing on",
                "edge-scrolling off",
            ]
        );
        let mut chosen = image;
        assert!(chosen.set("primary-button", "right"));
        assert!(chosen.set("mouse-speed", "-3"));
        assert!(chosen.set("mouse-acceleration", "off"));
        assert!(chosen.set("touchpad-speed", "40"));
        assert!(chosen.set("edge-scrolling", "on"));
        assert!(chosen.set("tap-to-click", "OFF"));
        assert_eq!(chosen.mouse.speed, -3);
        // a speed is kept inside its range
        assert_eq!(chosen.touchpad.speed, SPEED_MOST);
        assert_eq!(Pointer::parse(&chosen.lines().join("\n")), chosen);
        // a name or a value that is not one changes nothing
        assert!(!chosen.set("primary-button", "middle"));
        assert!(!chosen.set("mouse-speed", "fast"));
        assert!(!chosen.set("tap-to-click", "maybe"));
        assert!(!chosen.set("scroll-wheel", "on"));
        assert_eq!(Pointer::parse(&chosen.lines().join("\n")), chosen);
        // and a file with nothing in it is the image's own settings
        assert_eq!(Pointer::parse("\nnonsense\n"), image);
        for name in NAMES {
            let kind = Kind::of(name);
            assert_eq!(kind.is_none(), name == "primary-button", "{name}");
        }
    }

    #[test]
    fn the_part_holds_every_block_whole() {
        assert_eq!(
            Pointer::default().horizon(),
            "// written from ~/.config/rift/pointer\n\
             input {\n\
             \x20   mouse {\n\
             \x20       accel-speed 0.0\n\
             \x20   }\n\
             \x20   trackpoint {\n\
             \x20       accel-speed 0.0\n\
             \x20   }\n\
             \x20   trackball {\n\
             \x20       accel-speed 0.0\n\
             \x20   }\n\
             \x20   touchpad {\n\
             \x20       accel-speed 0.0\n\
             \x20       tap\n\
             \x20       natural-scroll\n\
             \x20       dwt\n\
             \x20   }\n\
             }\n"
        );
        let mut chosen = Pointer {
            left_handed: true,
            ..Pointer::default()
        };
        chosen.mouse = Mouse {
            speed: -5,
            acceleration: false,
            natural: true,
        };
        chosen.touchpad.tap = false;
        chosen.touchpad.speed = 3;
        chosen.touchpad.scrolling = Scrolling::Edge;
        let part = chosen.horizon();
        assert!(part.contains(
            "    mouse {\n        accel-speed -0.5\n        accel-profile \"flat\"\n        \
             natural-scroll\n        left-handed\n    }\n"
        ));
        assert!(part.contains(
            "    touchpad {\n        accel-speed 0.3\n        natural-scroll\n        dwt\n        \
             scroll-method \"edge\"\n        left-handed\n    }\n"
        ));
        assert_eq!(part.matches("accel-profile \"flat\"").count(), 3);
    }
}
