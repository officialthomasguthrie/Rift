//! The owner's name and password. Root is a tmpfs, so NixOS makes the owner's account again at
//! every boot with the name and the password every drive starts with, and what the owner chose is
//! kept on persist, a line each. `vault owner`, which vault-owner.service runs at every boot before
//! anyone logs in, puts the kept name into the password file and the kept hash into the shadow
//! file, and the service runs it again after `SetOwnerName` or `SetOwnerPassword` has kept
//! something new, so the change is there at once. The password files are written by that unit
//! alone, outside the sandbox Vault answers the bus in.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use librift::owner::{self as account, Account};
use sha_crypt::ShaCrypt;
use yescrypt::{PasswordHasher, PasswordVerifier, Yescrypt};

/// How long a wrong current password is held before the answer, the way PAM holds one after a
/// wrong password at the lock screen.
const WRONG_WAIT: Duration = Duration::from_secs(2);
/// The unit that puts what persist keeps into the password files.
const UNIT: &str = "vault-owner.service";

/// Why a change to the owner was not made.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The current password is not the owner's.
    Wrong(String),
    /// The name or the new password cannot be used, and why.
    Invalid(String),
    /// Something could not be read or written.
    Failed(String),
}

/// The owner's account, the password files of the running system, and where persist keeps what
/// the owner chose.
pub struct Owner {
    /// The account's login name.
    pub user: String,
    passwd: PathBuf,
    shadow: PathBuf,
    name_file: PathBuf,
    password_file: PathBuf,
}

impl Owner {
    /// The running system's.
    #[must_use]
    pub fn system() -> Self {
        Self::at(
            Path::new("/etc"),
            Path::new(account::NAME_FILE),
            Path::new(account::PASSWORD_FILE),
        )
    }

    /// The same, with the password files in `etc` and the kept name and hash where they say.
    #[must_use]
    pub fn at(etc: &Path, name_file: &Path, password_file: &Path) -> Self {
        Self {
            user: account::USER.to_string(),
            passwd: etc.join("passwd"),
            shadow: etc.join("shadow"),
            name_file: name_file.to_path_buf(),
            password_file: password_file.to_path_buf(),
        }
    }

    /// The owner's account as the password file has it now.
    ///
    /// # Errors
    ///
    /// A sentence when the file cannot be read or has no such account.
    pub fn account(&self) -> Result<Account, String> {
        let passwd = read(&self.passwd)?;
        account::account(&passwd, &self.user).ok_or_else(|| {
            format!(
                "{} has no account called {}.",
                self.passwd.display(),
                self.user
            )
        })
    }

    /// Whether the password is still the one every drive starts with.
    ///
    /// # Errors
    ///
    /// A sentence when the shadow file cannot be read or has no such account.
    pub fn image_password(&self) -> Result<bool, String> {
        Ok(checks(account::IMAGE_PASSWORD, &self.hash()?))
    }

    /// The hash of the owner's password as the shadow file has it now.
    fn hash(&self) -> Result<String, String> {
        let shadow = read(&self.shadow)?;
        account::hash_in(&shadow, &self.user)
            .map(ToString::to_string)
            .ok_or_else(|| {
                format!(
                    "{} has no account called {}.",
                    self.shadow.display(),
                    self.user
                )
            })
    }

    /// Keep a new name on persist.
    ///
    /// # Errors
    ///
    /// [`Refusal::Invalid`] for a name that cannot go into the password file, and
    /// [`Refusal::Failed`] when it could not be kept.
    pub fn keep_name(&self, name: &str) -> Result<(), Refusal> {
        if let Some(why) = account::name_problem(name) {
            return Err(Refusal::Invalid(why.to_string()));
        }
        write_kept(&self.name_file, name.trim()).map_err(Refusal::Failed)
    }

    /// Check the current password against the shadow file, and keep the hash of a new one on
    /// persist. A wrong current password is answered after a pause, as PAM answers one.
    ///
    /// # Errors
    ///
    /// [`Refusal::Wrong`] when `current` is not the owner's password, [`Refusal::Invalid`] for a
    /// new one that cannot be used, and [`Refusal::Failed`] when it could not be checked or kept.
    pub fn keep_password(&self, current: &str, new: &str) -> Result<(), Refusal> {
        let hash = self.hash().map_err(Refusal::Failed)?;
        if !checks(current, &hash) {
            thread::sleep(WRONG_WAIT);
            return Err(Refusal::Wrong(
                "The current password is incorrect.".to_string(),
            ));
        }
        if let Some(why) = account::password_problem(new) {
            return Err(Refusal::Invalid(why.to_string()));
        }
        let hash = Yescrypt::default()
            .hash_password(new.as_bytes())
            .map_err(|e| Refusal::Failed(format!("Could not hash the new password: {e}")))?;
        write_kept(&self.password_file, hash.as_str()).map_err(Refusal::Failed)
    }

    /// Put what persist keeps into the password files: the kept name into the comment field of
    /// the password file and the kept hash into the shadow file, each only when it is not there
    /// already. A line for each change. This is `vault owner`.
    ///
    /// # Errors
    ///
    /// A sentence when a file cannot be read or written, the kept hash is not one Vault made, or
    /// the files have no such account. Nothing is written after the first thing that fails.
    pub fn apply(&self) -> Result<Vec<String>, String> {
        let mut said = Vec::new();
        if let Some(name) = read_kept(&self.name_file)? {
            if let Some(why) = account::name_problem(&name) {
                return Err(format!("The kept name cannot be used: {why}"));
            }
            let passwd = read(&self.passwd)?;
            let now = account::account(&passwd, &self.user).ok_or_else(|| {
                format!(
                    "{} has no account called {}.",
                    self.passwd.display(),
                    self.user
                )
            })?;
            if now.name != name {
                let changed = account::with_name(&passwd, &self.user, &name).ok_or_else(|| {
                    format!(
                        "{} has no account called {}.",
                        self.passwd.display(),
                        self.user
                    )
                })?;
                replace(&self.passwd, &changed)?;
                said.push(format!("The owner is called {name}."));
            }
        }
        if let Some(hash) = read_kept(&self.password_file)? {
            if !usable(&hash) {
                return Err(format!(
                    "{} does not hold a hash Vault made, so the password stays as it is.",
                    self.password_file.display()
                ));
            }
            let shadow = read(&self.shadow)?;
            if account::hash_in(&shadow, &self.user) != Some(hash.as_str()) {
                let changed = account::with_hash(&shadow, &self.user, &hash).ok_or_else(|| {
                    format!(
                        "{} has no account called {}.",
                        self.shadow.display(),
                        self.user
                    )
                })?;
                replace(&self.shadow, &changed)?;
                said.push("The owner's password is the one they chose.".to_string());
            }
        }
        Ok(said)
    }
}

/// Have vault-owner.service put what persist keeps into the password files now, and wait for it.
/// The unit writes them outside the sandbox Vault answers the bus in, which keeps /etc read only.
///
/// # Errors
///
/// A sentence when systemctl could not be run or the unit failed.
pub fn apply_now() -> Result<(), String> {
    let done = Command::new("systemctl")
        .args(["start", UNIT])
        .output()
        .map_err(|e| format!("Could not run systemctl: {e}"))?;
    if done.status.success() {
        return Ok(());
    }
    Err(format!(
        "The change is kept for the next boot, but {UNIT} could not make it now: {}",
        String::from_utf8_lossy(&done.stderr).trim()
    ))
}

/// Whether `password` is the one `hash` was made from. A hash of a kind Vault does not know checks
/// nothing: yescrypt is what Vault and NixOS's own tools make, and SHA-512 crypt what NixOS makes of
/// the image's password.
#[must_use]
pub fn checks(password: &str, hash: &str) -> bool {
    let password = password.as_bytes();
    if hash.starts_with("$y$") {
        Yescrypt::default().verify_password(password, hash).is_ok()
    } else if hash.starts_with("$6$") {
        ShaCrypt::SHA512.verify_password(password, hash).is_ok()
    } else {
        false
    }
}

/// Whether a kept hash is one Vault made: yescrypt, one line, nothing that would end a field of the
/// shadow file.
fn usable(hash: &str) -> bool {
    hash.starts_with("$y$") && !hash.contains([':', '\n']) && hash.len() < 256
}

/// A file of the running system, whole.
fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("Could not read {}: {e}", path.display()))
}

/// The line persist keeps in `path`, or nothing when there is no file.
fn read_kept(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text.trim_end_matches(['\n', '\r']).to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Could not read {}: {e}", path.display())),
    }
}

/// Keep one line in `path`, for root alone, whole or not at all: it goes into a file beside it
/// that is synced, then takes its place.
fn write_kept(path: &Path, line: &str) -> Result<(), String> {
    let beside = path.with_extension("new");
    let written = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&beside)
        .and_then(|mut file| {
            file.write_all(format!("{line}\n").as_bytes())?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&beside, path));
    written.map_err(|e| {
        let _ = fs::remove_file(&beside);
        format!("Could not keep {}: {e}", path.display())
    })
}

/// Put `text` in the place of a file of the running system in one step, with the owner, group and
/// mode the file has now, so a program reading it finds the old file or the new one and never half
/// of either.
fn replace(path: &Path, text: &str) -> Result<(), String> {
    let now = fs::metadata(path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    let beside = path.with_extension("vault");
    let written = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(now.mode() & 0o7777)
        .open(&beside)
        .and_then(|mut file| {
            std::os::unix::fs::fchown(&file, Some(now.uid()), Some(now.gid()))?;
            file.set_permissions(now.permissions())?;
            file.write_all(text.as_bytes())?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&beside, path));
    written.map_err(|e| {
        let _ = fs::remove_file(&beside);
        format!("Could not write {}: {e}", path.display())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "rift" hashed by libxcrypt 4.5.2, the library `pam_unix` checks with: `mkpasswd -m yescrypt`.
    const XCRYPT_YESCRYPT: &str =
        "$y$j9T$yLj12EHpgT2f0XZEdLzMh.$VvvfoPyUfAlT09f0AJmqBrXd/Vz38Zlh0HWKhCwwFb6";
    /// "rift" hashed by OpenSSL: `openssl passwd -6 -salt abcdefgh rift`, the same SHA-512 crypt
    /// NixOS makes of an initial password.
    const OPENSSL_SHA512: &str = "$6$abcdefgh$0DF/Lk/w1HjowiZjA18mfq2DR0FddyaAzkodGOw3Zj9gfNeGYW3a2yU8W8zdtVuARwANJNjugoSczm1mLWroE.";

    const PASSWD: &str = "root:x:0:0:System administrator:/root:/bin/fish\n\
                          rift:x:1000:100:Rift owner:/home/rift:/bin/fish\n";

    /// A scratch folder with a password file and a shadow file, the owner's password being the
    /// image's.
    fn scratch(name: &str) -> (PathBuf, Owner) {
        let dir = std::env::temp_dir().join(format!("vault-owner-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("etc")).expect("scratch folder");
        fs::create_dir_all(dir.join("kept")).expect("scratch folder");
        fs::write(dir.join("etc/passwd"), PASSWD).expect("passwd");
        fs::write(
            dir.join("etc/shadow"),
            format!("root:!:1::::::\nrift:{OPENSSL_SHA512}:1::::::\n"),
        )
        .expect("shadow");
        let owner = Owner::at(
            &dir.join("etc"),
            &dir.join("kept/name"),
            &dir.join("kept/password"),
        );
        (dir, owner)
    }

    #[test]
    fn a_password_checks_against_the_hashes_other_tools_make() {
        assert!(checks("rift", XCRYPT_YESCRYPT));
        assert!(!checks("Rift", XCRYPT_YESCRYPT));
        assert!(checks("rift", OPENSSL_SHA512));
        assert!(!checks("rift ", OPENSSL_SHA512));
        // a locked account, a hash of a kind Vault does not know, and no hash at all check nothing
        for hash in ["!", "*", "", "$1$abc$def", "$2b$10$abc"] {
            assert!(!checks("rift", hash), "{hash}");
        }
    }

    #[test]
    fn a_new_hash_is_yescrypt_that_checks_its_own_password() {
        let hash = Yescrypt::default()
            .hash_password(b"new secret")
            .expect("hashed");
        let hash = hash.as_str();
        assert!(hash.starts_with("$y$j9T$"), "{hash}");
        assert!(usable(hash));
        assert!(checks("new secret", hash));
        assert!(!checks("new secreT", hash));
        assert!(!usable(OPENSSL_SHA512));
        assert!(!usable("$y$j9T$a:b"));
    }

    #[test]
    fn the_image_password_is_found_out_by_checking_it() {
        let (dir, owner) = scratch("image");
        assert_eq!(owner.image_password(), Ok(true));
        assert_eq!(
            owner.account().map(|found| found.name),
            Ok("Rift owner".to_string())
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_kept_name_and_password_reach_the_password_files_once() {
        let (dir, owner) = scratch("keep");
        // nothing kept, nothing written
        assert_eq!(owner.apply(), Ok(Vec::new()));
        assert_eq!(
            owner.keep_password("wrong", "new secret"),
            Err(Refusal::Wrong(
                "The current password is incorrect.".to_string()
            ))
        );
        assert!(matches!(
            owner.keep_password("rift", ""),
            Err(Refusal::Invalid(_))
        ));
        assert!(matches!(
            owner.keep_name("Taylor, Sam"),
            Err(Refusal::Invalid(_))
        ));
        assert_eq!(owner.keep_name(" Sam Taylor "), Ok(()));
        assert_eq!(owner.keep_password("rift", "new secret"), Ok(()));
        let kept = fs::metadata(dir.join("kept/password")).expect("kept");
        assert_eq!(kept.mode() & 0o777, 0o600);
        assert_eq!(
            owner.apply(),
            Ok(vec![
                "The owner is called Sam Taylor.".to_string(),
                "The owner's password is the one they chose.".to_string(),
            ])
        );
        assert_eq!(
            owner.account().map(|found| found.name),
            Ok("Sam Taylor".to_string())
        );
        assert_eq!(owner.image_password(), Ok(false));
        let shadow = fs::read_to_string(dir.join("etc/shadow")).expect("shadow");
        let hash = account::hash_in(&shadow, "rift").expect("rift is there");
        assert!(checks("new secret", hash));
        assert!(shadow.starts_with("root:!:1::::::\n"), "{shadow}");
        // a second run finds both in place and writes nothing
        assert_eq!(owner.apply(), Ok(Vec::new()));
        // the image's password again, checked against the one chosen
        assert_eq!(owner.keep_password("new secret", "rift"), Ok(()));
        assert_eq!(owner.apply().map(|said| said.len()), Ok(1));
        assert_eq!(owner.image_password(), Ok(true));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_kept_hash_vault_did_not_make_is_left_out() {
        let (dir, owner) = scratch("foreign");
        fs::write(dir.join("kept/password"), "rift::0\n").expect("kept");
        assert!(owner.apply().is_err());
        assert_eq!(owner.image_password(), Ok(true));
        let _ = fs::remove_dir_all(dir);
    }
}
