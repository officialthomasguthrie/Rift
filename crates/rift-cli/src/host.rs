//! `rift host`: what Orbit remembers about this machine, one row per setting, or one value by
//! itself. `rift host set` writes the settings a person decides into the profile, through Orbit,
//! which is the only thing that writes that file.

use std::process::ExitCode;

use librift::orbit::{self, Host, Output};

use crate::text;

const USAGE: &str = "Usage: rift host [class | tier]\n       rift host set <class | tier | gpu> \
<value>\n       rift host set scale <screen> <1 or 2>";

const HELP: &str = "Shows what Orbit remembers about this machine. class prints the host class \
(owned, trusted or borrowed) by itself, and tier the AI tier. set writes one of them into this \
machine's profile: the class, the AI tier, the graphics path, or the size a screen is drawn at.";

pub fn run(args: &[String]) -> ExitCode {
    let one = match args {
        [] => None,
        [arg] if arg == "--help" || arg == "-h" => {
            println!("{USAGE}\n\n{HELP}");
            return ExitCode::SUCCESS;
        }
        [arg] if field(arg).is_some() => field(arg),
        [arg, rest @ ..] if arg == "set" => return set(rest),
        [arg, rest @ ..] => return text::unknown("host", rest.first().unwrap_or(arg), USAGE),
    };
    match orbit::host() {
        Ok(host) => {
            match one {
                Some(value) => println!("{}", value(host)),
                None => print!("{}", text::table(&rows(&host))),
            }
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("{why}");
            ExitCode::FAILURE
        }
    }
}

/// `rift host set <name> <value>`, and `rift host set scale <screen> <1 or 2>`. Orbit writes it
/// into the `[set]` layer of this machine's profile and says so on the bus.
fn set(args: &[String]) -> ExitCode {
    let written = match args {
        [name, value] if name != "scale" => orbit::set(name, value).map(|()| {
            println!("{name} is {value} for this machine.");
        }),
        [name, screen, size] if name == "scale" => match size.trim().parse::<u32>() {
            Ok(scale) => orbit::set_display_scale(screen, scale).map(|()| {
                // the compositor reads the scale from a part of its config under home, which the
                // session writes when it starts. a shell with no home of its own is no reason to
                // fail: the profile is where the setting lives
                let _ = orbit::follow();
                println!("{screen} is drawn at scale {scale}.");
            }),
            Err(_) => Err(format!(
                "\"{}\" is not a size a screen is drawn at. It is 1 or 2.",
                size.trim()
            )),
        },
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match written {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("{why}");
            ExitCode::FAILURE
        }
    }
}

/// The value `rift host <name>` prints by itself.
fn field(name: &str) -> Option<fn(Host) -> String> {
    match name {
        "class" => Some(|host| host.class),
        "tier" => Some(|host| host.ai_tier),
        _ => None,
    }
}

fn rows(host: &Host) -> Vec<(&'static str, String)> {
    let mut rows = vec![
        ("Fingerprint", host.fingerprint.clone()),
        ("Class", host.class.clone()),
    ];
    if host.outputs.is_empty() {
        rows.push(("Display", "none".into()));
    }
    for output in &host.outputs {
        rows.push(("Display", describe(output)));
    }
    rows.push(("GPU path", host.gpu_path.clone()));
    rows.push(("AI tier", host.ai_tier.clone()));
    rows
}

fn describe(output: &Output) -> String {
    let Output {
        connector,
        width,
        height,
        scale,
        ..
    } = output;
    if *width == 0 {
        format!("{connector}, no EDID, scale {scale}")
    } else {
        format!("{connector}, {width}x{height}, scale {scale}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(outputs: Vec<Output>) -> Host {
        Host {
            fingerprint: "5297c0f65d6a".repeat(5) + "abcd",
            class: "borrowed".into(),
            outputs,
            gpu_path: "none".into(),
            ai_tier: "small".into(),
        }
    }

    #[test]
    fn a_virtual_machine_reads_as_rows() {
        let qemu = host(vec![Output {
            connector: "Virtual-1".into(),
            width: 1280,
            height: 800,
            width_cm: 32,
            height_cm: 20,
            scale: 1,
        }]);
        let expected = format!(
            "Fingerprint: {}\nClass:       borrowed\nDisplay:     Virtual-1, 1280x800, scale 1\n\
             GPU path:    none\nAI tier:     small\n",
            qemu.fingerprint
        );
        assert_eq!(text::table(&rows(&qemu)), expected);
    }

    #[test]
    fn every_output_gets_a_row() {
        let laptop = host(vec![
            Output {
                connector: "eDP-1".into(),
                width: 2880,
                height: 1800,
                width_cm: 30,
                height_cm: 19,
                scale: 2,
            },
            Output {
                connector: "HDMI-A-1".into(),
                width: 0,
                height: 0,
                width_cm: 0,
                height_cm: 0,
                scale: 1,
            },
        ]);
        let displays: Vec<String> = rows(&laptop)
            .into_iter()
            .filter(|(label, _)| *label == "Display")
            .map(|(_, value)| value)
            .collect();
        assert_eq!(
            displays,
            ["eDP-1, 2880x1800, scale 2", "HDMI-A-1, no EDID, scale 1"]
        );
        let headless = rows(&host(Vec::new()));
        assert!(headless.contains(&("Display", "none".to_string())));
    }

    #[test]
    fn class_and_tier_print_one_value_each() {
        let value = |name| field(name).map(|get| get(host(Vec::new())));
        assert_eq!(value("class").as_deref(), Some("borrowed"));
        assert_eq!(value("tier").as_deref(), Some("small"));
        assert_eq!(value("fingerprint"), None);
    }
}
