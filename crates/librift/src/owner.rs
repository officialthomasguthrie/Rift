//! The owner of the drive: the account a person logs in to, the name the lock screen greets them by,
//! and the password it asks for.
//!
//! Root is a tmpfs, so NixOS makes the account again at every boot, with the name and the password
//! every drive starts with. The owner's own are kept on persist, a line each in a folder of their
//! own: Vault writes both there and into the password files of the running system, and puts them
//! into the password files again at every boot before anyone logs in. Settings asks Vault over the
//! system bus.

#[cfg(feature = "bus")]
use std::time::Duration;

#[cfg(feature = "bus")]
use crate::{Component, bus};

/// The account the owner logs in to. The image makes it, and a drive has one owner.
pub const USER: &str = "rift";
/// The name the image gives the owner until they choose one.
pub const IMAGE_NAME: &str = "Rift owner";
/// The password the image gives the owner until they choose one. Every drive starts with it.
pub const IMAGE_PASSWORD: &str = "rift";
/// The folder on persist the owner's own name and password are kept in, root's alone.
pub const FOLDER: &str = "/var/lib/rift/owner";
/// The name the owner chose, one line.
pub const NAME_FILE: &str = "/var/lib/rift/owner/name";
/// The hash of the password the owner chose, one line.
pub const PASSWORD_FILE: &str = "/var/lib/rift/owner/password";
/// How long a name may be, in characters.
pub const NAME_MOST: usize = 64;
/// How long a password may be, in bytes. PAM hands a module no more than 512.
pub const PASSWORD_MOST: usize = 256;

/// How long a change may take: Vault hashes the password, then has the password files written.
#[cfg(feature = "bus")]
const SET_TIMEOUT: Duration = Duration::from_secs(60);

/// An account in the password file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// The login name.
    pub user: String,
    /// Its uid.
    pub uid: u32,
    /// The full name, the first part of the comment field. Empty when there is none.
    pub name: String,
}

/// The account called `user` in the text of a password file.
#[must_use]
pub fn account(passwd: &str, user: &str) -> Option<Account> {
    passwd.lines().find_map(|line| {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() < 7 || fields[0] != user {
            return None;
        }
        Some(Account {
            user: user.to_string(),
            uid: fields[2].parse().ok()?,
            name: full_name(fields[4]).to_string(),
        })
    })
}

/// The full name in a comment field: what comes before the first comma. An office and a phone
/// number may follow it.
fn full_name(comment: &str) -> &str {
    comment.split(',').next().unwrap_or("").trim()
}

/// The text of a password file with the full name of `user` changed to `name`. What the comment
/// field has after the name, from its first comma on, stays. Nothing when there is no such account.
#[must_use]
pub fn with_name(passwd: &str, user: &str, name: &str) -> Option<String> {
    with_field(passwd, user, 4, 7, |comment| match comment.find(',') {
        Some(at) => format!("{name}{}", &comment[at..]),
        None => name.to_string(),
    })
}

/// The hash of `user`'s password in the text of a shadow file.
#[must_use]
pub fn hash_in<'a>(shadow: &'a str, user: &str) -> Option<&'a str> {
    shadow.lines().find_map(|line| {
        let mut fields = line.split(':');
        (fields.next() == Some(user))
            .then(|| fields.next())
            .flatten()
    })
}

/// The text of a shadow file with `user`'s hash changed to `hash`. Nothing when there is no such
/// account.
#[must_use]
pub fn with_hash(shadow: &str, user: &str, hash: &str) -> Option<String> {
    with_field(shadow, user, 1, 2, |_| hash.to_string())
}

/// A file of lines of fields split by colons with one field of `user`'s line changed, every other
/// line and field as it was. A line with fewer than `least` fields is not an account's.
fn with_field(
    text: &str,
    user: &str,
    at: usize,
    least: usize,
    change: impl Fn(&str) -> String,
) -> Option<String> {
    let mut found = false;
    let mut out = String::with_capacity(text.len() + 64);
    for line in text.split_inclusive('\n') {
        let (body, end) = match line.strip_suffix('\n') {
            Some(body) => (body, "\n"),
            None => (line, ""),
        };
        let mut fields: Vec<String> = body.split(':').map(ToString::to_string).collect();
        if !found && fields.len() >= least && fields[0] == user {
            fields[at] = change(&fields[at]);
            found = true;
        }
        out.push_str(&fields.join(":"));
        out.push_str(end);
    }
    found.then_some(out)
}

/// What is wrong with a name the owner chose, in a sentence, or nothing. The name goes into the
/// comment field of the password file, where a colon ends the field, a comma ends the name and a
/// line break ends the account.
#[must_use]
pub fn name_problem(name: &str) -> Option<&'static str> {
    let name = name.trim();
    if name.is_empty() {
        Some("The name is empty.")
    } else if name.chars().count() > NAME_MOST {
        Some("The name is longer than 64 characters.")
    } else if name.contains([':', ',']) || name.chars().any(char::is_control) {
        Some("A name cannot have a colon, a comma or a line break in it.")
    } else {
        None
    }
}

/// What is wrong with a new password, in a sentence, or nothing.
#[must_use]
pub fn password_problem(password: &str) -> Option<&'static str> {
    if password.is_empty() {
        Some("The new password is empty.")
    } else if password.len() > PASSWORD_MOST {
        Some("The new password is longer than 256 characters.")
    } else if password.chars().any(char::is_control) {
        Some("A password cannot have a line break or a tab in it.")
    } else {
        None
    }
}

/// What Vault says about the owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owner {
    /// The account they log in to.
    pub user: String,
    /// The name the lock screen greets them by.
    pub name: String,
    /// Whether the password is still the one every drive starts with.
    pub image_password: bool,
}

/// What Vault says about the owner. Only the owner and root may ask.
///
/// # Errors
///
/// A sentence when the bus or Vault is not there, or Vault could not read the password files.
#[cfg(feature = "bus")]
pub fn read() -> Result<Owner, String> {
    let vault = Component::Vault;
    let connection = bus::connect(bus::PROPERTY_TIMEOUT)?;
    let proxy = bus::proxy(&connection, vault)?;
    let (user, name, image_password): (String, String, bool) = proxy
        .call("Owner", &())
        .map_err(|e| bus::sentence(vault, e))?;
    Ok(Owner {
        user,
        name,
        image_password,
    })
}

/// Give the owner a new name, now and at every boot after.
///
/// # Errors
///
/// A sentence when the bus or Vault is not there, or Vault refused the name or could not keep it.
#[cfg(feature = "bus")]
pub fn set_name(name: &str) -> Result<(), String> {
    let vault = Component::Vault;
    let connection = bus::connect(SET_TIMEOUT)?;
    let proxy = bus::proxy(&connection, vault)?;
    proxy
        .call("SetOwnerName", &(name,))
        .map_err(|e| bus::sentence(vault, e))
}

/// Give the owner a new password, now and at every boot after. Vault checks the current one first.
///
/// # Errors
///
/// A sentence when the bus or Vault is not there, the current password is not the owner's, or
/// Vault refused the new one or could not keep it.
#[cfg(feature = "bus")]
pub fn set_password(current: &str, new: &str) -> Result<(), String> {
    let vault = Component::Vault;
    let connection = bus::connect(SET_TIMEOUT)?;
    let proxy = bus::proxy(&connection, vault)?;
    proxy
        .call("SetOwnerPassword", &(current, new))
        .map_err(|e| bus::sentence(vault, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSWD: &str = "root:x:0:0:System administrator:/root:/run/current-system/sw/bin/fish\n\
                          rift:x:1000:100:Rift owner:/home/rift:/run/current-system/sw/bin/fish\n\
                          quasar:x:990:990::/var/empty:/run/current-system/sw/bin/nologin\n";
    const SHADOW: &str = "root:!:1::::::\nrift:$6$abcdefgh$hash:1::::::\nquasar:!:1::::::\n";

    #[test]
    fn the_account_is_found_by_its_login_name() {
        assert_eq!(
            account(PASSWD, "rift"),
            Some(Account {
                user: "rift".into(),
                uid: 1000,
                name: "Rift owner".into(),
            })
        );
        assert_eq!(
            account(PASSWD, "quasar").map(|found| found.name),
            Some(String::new())
        );
        assert_eq!(account(PASSWD, "rif"), None);
        assert_eq!(account("rift:x:1000\n", "rift"), None);
    }

    #[test]
    fn a_new_name_changes_that_account_and_nothing_else() {
        let changed = with_name(PASSWD, "rift", "Sam Taylor").expect("rift is there");
        assert_eq!(
            changed,
            PASSWD.replace(":Rift owner:", ":Sam Taylor:"),
            "one field of one line"
        );
        assert_eq!(
            account(&changed, "rift").map(|found| found.name),
            Some("Sam Taylor".into())
        );
        // an office and a phone after the name stay where they were
        let office = PASSWD.replace(":Rift owner:", ":Rift owner,Room 4,555:");
        assert!(
            with_name(&office, "rift", "Sam")
                .expect("rift is there")
                .contains(":Sam,Room 4,555:")
        );
        // a file without a line end at the end keeps it that way
        assert_eq!(
            with_name("rift:x:1000:100:A:/home/rift:/bin/fish", "rift", "B").as_deref(),
            Some("rift:x:1000:100:B:/home/rift:/bin/fish")
        );
        assert_eq!(with_name(PASSWD, "nobody", "Sam"), None);
    }

    #[test]
    fn a_new_hash_changes_that_account_and_nothing_else() {
        assert_eq!(hash_in(SHADOW, "rift"), Some("$6$abcdefgh$hash"));
        assert_eq!(hash_in(SHADOW, "root"), Some("!"));
        assert_eq!(hash_in(SHADOW, "nobody"), None);
        let changed = with_hash(SHADOW, "rift", "$y$j9T$salt$other").expect("rift is there");
        assert_eq!(
            changed,
            SHADOW.replace("$6$abcdefgh$hash", "$y$j9T$salt$other")
        );
        assert_eq!(with_hash(SHADOW, "nobody", "$y$"), None);
    }

    #[test]
    fn a_name_is_one_line_without_a_colon_or_a_comma() {
        assert_eq!(name_problem("Sam Taylor"), None);
        assert_eq!(name_problem("Zo\u{eb} Bront\u{eb}"), None);
        assert_eq!(name_problem("  "), Some("The name is empty."));
        assert!(name_problem("Taylor, Sam").is_some());
        assert!(name_problem("Sam:Taylor").is_some());
        assert!(name_problem("Sam\nTaylor").is_some());
        assert!(name_problem(&"a".repeat(NAME_MOST)).is_none());
        assert!(name_problem(&"a".repeat(NAME_MOST + 1)).is_some());
    }

    #[test]
    fn a_password_is_one_line_that_is_not_empty() {
        assert_eq!(password_problem("correct horse"), None);
        assert_eq!(password_problem(""), Some("The new password is empty."));
        assert!(password_problem("two\nlines").is_some());
        assert!(password_problem(&"a".repeat(PASSWORD_MOST + 1)).is_some());
    }

    #[test]
    fn the_sentences_are_sentences() {
        for said in [
            name_problem(""),
            name_problem(&"a".repeat(65)),
            name_problem("a,b"),
            password_problem(""),
            password_problem(&"a".repeat(257)),
            password_problem("a\tb"),
        ] {
            let said = said.expect("each of these is refused");
            assert!(said.ends_with('.') && said.is_ascii(), "{said}");
        }
    }
}
