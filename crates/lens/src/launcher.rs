//! Starting an app. The desktop entries are librift's, since Settings names the apps the dock keeps
//! too; the shell starts one here, in a scope of its own under the owner's systemd.

use std::fmt::Write as _;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

pub use librift::apps::{App, load};

/// The terminal that wraps apps with `Terminal=true`. It is given the app's own class, so the
/// window belongs to that app and not to the terminal, and the dock has one item per app.
const TERMINAL: &str = "ghostty";

/// Start the app and let it go.
///
/// The shell is a unit of the owner's systemd, and a child left in that unit is stopped with it, so
/// a crash of the shell would take every app it started. systemd-run makes the app a scope of its
/// own before the app starts, which is what Horizon does for what a key starts, so nothing the app
/// starts early is left behind in the shell's unit either. A thread waits for it: a child nobody
/// waits for stays in the process table after it ends.
///
/// # Errors
///
/// When the program cannot be started.
pub fn launch(app: &App) -> Result<(), String> {
    let class = app.terminal.then(|| format!("--class={}", app.class()));
    let mut words: Vec<&str> = Vec::new();
    if let Some(class) = class.as_deref() {
        words.extend([TERMINAL, class, "-e"]);
    }
    words.extend(app.exec.iter().map(String::as_str));
    if words.is_empty() {
        return Err("Nothing to run".to_string());
    }
    let mut child = Command::new("systemd-run")
        .args([
            "--user",
            "--scope",
            "--quiet",
            "--collect",
            "--slice=app.slice",
        ])
        .arg(format!("--unit={}", scope(&app.id)))
        .arg(format!("--description={}", app.name))
        .arg("--")
        .args(&words)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start {}: {e}", app.name))?;
    let name = app.name.clone();
    std::thread::spawn(move || match child.wait() {
        Ok(status) if !status.success() => eprintln!("lens: {name} ended: {status}"),
        Ok(_) => {}
        Err(why) => eprintln!("lens: could not wait for {name}: {why}"),
    });
    Ok(())
}

/// The name of an app's scope, the way the systemd documentation for desktops names them:
/// `app-rift-<id>-<number>`. A character a unit name cannot hold, and a dash, which would read as
/// the end of the id, is written as its escape, the way GNOME and Horizon write theirs.
#[must_use]
pub fn scope(id: &str) -> String {
    let mut name = String::from("app-rift-");
    for byte in id.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'.') {
            name.push(char::from(byte));
        } else {
            let _ = write!(name, "\\x{byte:02x}");
        }
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let _ = write!(name, "-{now}");
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scope_is_named_after_the_app() {
        let named = scope("com.mitchellh.ghostty");
        let (id, number) = named.rsplit_once('-').expect("a number at the end");
        assert_eq!(id, "app-rift-com.mitchellh.ghostty");
        assert!(!number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()));
        // a dash inside the id would read as its end, and a space is no part of a unit name
        assert!(scope("virt-manager").starts_with("app-rift-virt\\x2dmanager-"));
        assert!(scope("my app").starts_with("app-rift-my\\x20app-"));
    }
}
