//! Saying words out loud. The voice is one of the models the manifest declares, and the program
//! that reads it is run once for one sentence and then gone: it loads in under a second, holds
//! nothing between sentences and would only sit on memory the chat model wants. So there is no
//! third server beside the two llama-servers, and nothing at all runs until a voice is on the
//! drive, which is what the manifest and the `Voice` property say.
//!
//! The wav comes back to the caller as bytes. Quasar writes nothing into anyone's files, the same
//! way it reads none of them for search by meaning.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use librift::models::{self, Manifest};

use crate::backend::{State, Status};
use crate::child;

/// How long one sentence may take. The voice says about ten seconds of speech a second on one
/// core, so this is many times what the longest text quasard takes can need.
const SAY_TIMEOUT: Duration = Duration::from_secs(120);
/// How often the models directory is looked at again, so a voice copied onto the drive is picked
/// up without touching the unit.
const LOOK_AGAIN: Duration = Duration::from_secs(30);
/// How many cores one sentence uses. Two is enough to stay far ahead of the speech.
const THREADS: u32 = 2;

/// Names the wav files of one process apart, so two sentences at once do not write the same file.
static SENTENCE: AtomicU64 = AtomicU64::new(0);

/// How to say words out loud.
pub struct Voice {
    /// The program that turns text into a wav.
    pub program: PathBuf,
    /// Where the voices are.
    pub models_dir: PathBuf,
    /// The espeak-ng data the voice was trained against, which turns words into phonemes.
    pub data_dir: PathBuf,
}

impl Voice {
    /// The program's arguments for one sentence.
    pub fn args(&self, voice: &models::Voice, wav: &Path, text: &str) -> Vec<String> {
        vec![
            format!(
                "--vits-model={}",
                self.models_dir.join(&voice.file).display()
            ),
            format!(
                "--vits-tokens={}",
                self.models_dir.join(&voice.tokens).display()
            ),
            format!("--vits-data-dir={}", self.data_dir.display()),
            format!("--num-threads={THREADS}"),
            format!("--output-filename={}", wav.display()),
            text.to_string(),
        ]
    }

    /// Says the text with the voice and gives back the wav.
    ///
    /// # Errors
    ///
    /// A sentence that says why there is no audio.
    pub fn say(&self, voice: &models::Voice, text: &str) -> Result<Vec<u8>, String> {
        // quasard's own tmp, which systemd gives it alone and empties when it stops
        let wav = std::env::temp_dir().join(format!(
            "quasar-say-{}-{}.wav",
            std::process::id(),
            SENTENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let said = self.run(voice, &wav, text);
        let audio = said
            .and_then(|()| fs::read(&wav).map_err(|e| format!("The voice wrote no audio: {e}.")));
        let _ = fs::remove_file(&wav);
        audio
    }

    /// Runs the program for one sentence, and kills it if it stops answering.
    fn run(&self, voice: &models::Voice, wav: &Path, text: &str) -> Result<(), String> {
        let args = self.args(voice, wav, text);
        let ended = child::run(&self.program, &args, SAY_TIMEOUT, "the voice")?;
        if ended.stopped {
            return Err(format!(
                "The voice took longer than {} seconds to say that.",
                SAY_TIMEOUT.as_secs()
            ));
        }
        if !ended.ok {
            return Err(format!("The voice could not say that: {}", ended.why));
        }
        Ok(())
    }

    /// Keeps the `Voice` properties current, for as long as quasard runs. `notify` is called after
    /// every change, with the lock released.
    pub fn watch(&self, manifest: &Manifest, status: &Mutex<Status>, notify: &dyn Fn()) -> ! {
        let mut said = String::new();
        loop {
            let on_drive = |file: &str| self.models_dir.join(file).is_file();
            let (state, model, error) = match manifest.pick_voice(on_drive) {
                Ok(voice) => (State::Ready, voice.id.clone(), String::new()),
                Err(why) => (State::NoModel, String::new(), why),
            };
            let line = if error.is_empty() {
                format!("saying words out loud with {model}")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn voice() -> Voice {
        Voice {
            program: PathBuf::from("sherpa-onnx-offline-tts"),
            models_dir: PathBuf::from("/var/lib/rift/models"),
            data_dir: PathBuf::from("/nix/store/espeak/share/espeak-ng-data"),
        }
    }

    fn lessac() -> models::Voice {
        models::Voice {
            id: "piper-en-us-lessac-medium".into(),
            file: "en_US-lessac-medium.onnx".into(),
            tokens: "en_US-lessac-medium.tokens.txt".into(),
        }
    }

    #[test]
    fn the_voice_reads_its_own_files_and_writes_one_wav() {
        let args = voice().args(
            &lessac(),
            Path::new("/tmp/quasar-say-1-0.wav"),
            "Rift says this out loud.",
        );
        assert_eq!(
            args,
            [
                "--vits-model=/var/lib/rift/models/en_US-lessac-medium.onnx",
                "--vits-tokens=/var/lib/rift/models/en_US-lessac-medium.tokens.txt",
                "--vits-data-dir=/nix/store/espeak/share/espeak-ng-data",
                "--num-threads=2",
                "--output-filename=/tmp/quasar-say-1-0.wav",
                "Rift says this out loud.",
            ]
        );
    }

    #[test]
    fn the_text_goes_last_and_whole() {
        let text = "It said \"go left\", then it said 'go right'.\nAnd then nothing.";
        let args = voice().args(&lessac(), Path::new("/tmp/a.wav"), text);
        assert_eq!(args.last().map(String::as_str), Some(text));
    }
}
