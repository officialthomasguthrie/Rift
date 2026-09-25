//! Turning speech into words. The speech model is one of the models the manifest declares, and
//! whisper is run once for one recording and then gone, the same way the voice is: the weights
//! load in a twentieth of a second, and reading a few seconds of speech takes a second or two of
//! it, almost all in the encoder. A server that sat there holding the model would save that
//! twentieth of a second and keep a few hundred megabytes from the chat model for as long as the
//! machine is on. So nothing runs until someone asks, and nothing at all runs until a speech model
//! is on the drive, which is what the manifest and the `Speech` property say.
//!
//! The recording comes in as bytes and the words go back as words. Quasar opens none of the
//! owner's files, and its unit could not read them if it tried.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use librift::models::{self, Manifest};

use crate::backend::{State, Status};
use crate::child;

/// How long one recording may take. Whisper reads a minute of speech in a few seconds on two
/// cores, so this is many times what the longest recording quasard takes can need.
const LISTEN_TIMEOUT: Duration = Duration::from_secs(240);
/// How often the models directory is looked at again, so a speech model copied onto the drive is
/// picked up without touching the unit.
const LOOK_AGAIN: Duration = Duration::from_secs(30);
/// How many cores one recording uses. Two is within a tenth of what four give: the work is one
/// pass of the encoder either way.
const THREADS: u32 = 2;
/// The language the words come back in. Rift speaks English, and the voice that says words out
/// loud is an English one, so the model is told so rather than left to guess from a few seconds of
/// sound.
const LANGUAGE: &str = "en";

/// Names the files of one process apart, so two recordings at once do not write the same file.
static RECORDING: AtomicU64 = AtomicU64::new(0);

/// How to turn speech into words.
pub struct Speech {
    /// The program that reads a recording.
    pub program: PathBuf,
    /// Where the speech models are.
    pub models_dir: PathBuf,
}

impl Speech {
    /// The program's arguments for one recording. Whisper takes a flag and its value as two
    /// arguments, not as one with an equals sign in it.
    pub fn args(&self, model: &models::Speech, wav: &Path) -> Vec<String> {
        vec![
            "--model".to_string(),
            self.models_dir.join(&model.file).display().to_string(),
            "--file".to_string(),
            wav.display().to_string(),
            "--language".to_string(),
            LANGUAGE.to_string(),
            "--threads".to_string(),
            THREADS.to_string(),
            "--no-timestamps".to_string(),
            "--no-prints".to_string(),
        ]
    }

    /// The words that were said in the recording. Nothing said gives nothing back.
    ///
    /// # Errors
    ///
    /// A sentence that says why there are no words.
    pub fn listen(&self, model: &models::Speech, wav: &[u8]) -> Result<String, String> {
        // quasard's own tmp, which systemd gives it alone and empties when it stops
        let file = std::env::temp_dir().join(format!(
            "quasar-listen-{}-{}.wav",
            std::process::id(),
            RECORDING.fetch_add(1, Ordering::Relaxed)
        ));
        let written = fs::write(&file, wav)
            .map_err(|e| format!("Could not put the recording where whisper reads it: {e}."));
        let heard = written.and_then(|()| self.run(model, &file));
        let _ = fs::remove_file(&file);
        heard
    }

    /// Runs whisper for one recording, and kills it if it stops answering.
    fn run(&self, model: &models::Speech, wav: &Path) -> Result<String, String> {
        let args = self.args(model, wav);
        let ended = child::run(&self.program, &args, LISTEN_TIMEOUT, "whisper")?;
        if ended.stopped {
            return Err(format!(
                "Whisper took longer than {} seconds to read that.",
                LISTEN_TIMEOUT.as_secs()
            ));
        }
        // whisper ends well and writes nothing at all when the file is not audio it can read, so
        // what it wrote is what says it worked, not the way it ended. a recording with nothing in
        // it does write a line, and that line is in brackets
        if !ended.ok || ended.out.trim().is_empty() {
            return Err(format!("Whisper could not read that: {}", ended.why));
        }
        Ok(words(&ended.out))
    }

    /// Keeps the `Speech` properties current, for as long as quasard runs. `notify` is called after
    /// every change, with the lock released.
    pub fn watch(&self, manifest: &Manifest, status: &Mutex<Status>, notify: &dyn Fn()) -> ! {
        let mut said = String::new();
        loop {
            let on_drive = |file: &str| self.models_dir.join(file).is_file();
            let (state, model, error) = match manifest.pick_speech(on_drive) {
                Ok(model) => (State::Ready, model.id.clone(), String::new()),
                Err(why) => (State::NoModel, String::new(), why),
            };
            let line = if error.is_empty() {
                format!("turning speech into words with {model}")
            } else {
                error.clone()
            };
            if line != said {
                println!("quasar: {line}");
                said = line;
            }
            let changed = {
                let mut status = status.lock().unwrap_or_else(PoisonError::into_inner);
                let before = status.clone();
                status.state = state;
                status.model = model;
                status.error = error;
                *status != before
            };
            if changed {
                notify();
            }
            thread::sleep(LOOK_AGAIN);
        }
    }
}

/// The words out of what whisper printed: one line for each stretch of speech, each with a space
/// in front of it, and a line in brackets where it heard something that is not words at all.
#[must_use]
pub fn words(printed: &str) -> String {
    printed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !marker(line))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether the line is whisper's own mark for a stretch of sound with no words in it, such as
/// `[BLANK_AUDIO]`.
fn marker(line: &str) -> bool {
    line.starts_with('[') && line.ends_with(']')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speech() -> Speech {
        Speech {
            program: PathBuf::from("whisper-cli"),
            models_dir: PathBuf::from("/var/lib/rift/models"),
        }
    }

    fn base() -> models::Speech {
        models::Speech {
            id: "whisper-base".into(),
            file: "ggml-base.bin".into(),
        }
    }

    #[test]
    fn whisper_reads_one_file_and_writes_the_words_plainly() {
        let args = speech().args(&base(), Path::new("/tmp/quasar-listen-1-0.wav"));
        assert_eq!(
            args,
            [
                "--model",
                "/var/lib/rift/models/ggml-base.bin",
                "--file",
                "/tmp/quasar-listen-1-0.wav",
                "--language",
                "en",
                "--threads",
                "2",
                "--no-timestamps",
                "--no-prints",
            ]
        );
    }

    #[test]
    fn the_words_are_the_lines_it_printed() {
        assert_eq!(
            words(" Rift runs the model on the drive and says this out loud.\n"),
            "Rift runs the model on the drive and says this out loud."
        );
        assert_eq!(
            words(" The first thing it heard.\n And then the second.\n"),
            "The first thing it heard. And then the second."
        );
    }

    #[test]
    fn a_recording_with_nothing_in_it_gives_no_words() {
        assert_eq!(words(""), "");
        assert_eq!(words("\n \n"), "");
        assert_eq!(words(" [BLANK_AUDIO]\n"), "");
        assert_eq!(
            words(" [MUSIC]\n Then someone spoke.\n"),
            "Then someone spoke."
        );
    }
}
