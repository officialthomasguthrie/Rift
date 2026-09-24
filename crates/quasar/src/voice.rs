//! Saying words out loud. The voice is one of the models the manifest declares, and the program
//! that reads it is run once for one sentence and then gone: it loads in under a second, holds
//! nothing between sentences and would only sit on memory the chat model wants. So there is no
//! third server beside the two llama-servers, and nothing at all runs until a voice is on the
//! drive, which is what the manifest and the `Voice` property say.
//!
//! The wav comes back to the caller as bytes. Quasar writes nothing into anyone's files, the same
//! way it reads none of them for search by meaning.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use librift::models::{self, Manifest};

use crate::backend::{State, Status};

/// How long one sentence may take. The voice says about ten seconds of speech a second on one
/// core, so this is many times what the longest text quasard takes can need.
const SAY_TIMEOUT: Duration = Duration::from_secs(120);
/// How often the program is looked at while it says a sentence.
const POLL: Duration = Duration::from_millis(20);
/// How often the models directory is looked at again, so a voice copied onto the drive is picked
/// up without touching the unit.
const LOOK_AGAIN: Duration = Duration::from_secs(30);
/// How many cores one sentence uses. Two is enough to stay far ahead of the speech.
const THREADS: u32 = 2;
/// How much of what the program said about a failure is kept.
const TROUBLE: u64 = 8192;

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
        let mut child = Command::new(&self.program)
            .args(self.args(voice, wav, text))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Could not start the voice: {e}."))?;
        // the program says a few lines about the model on its way, and everything it says about a
        // failure is on the same stream. reading it on a thread keeps a full pipe from stopping it
        let mut said = child.stderr.take().map(|stderr| {
            thread::spawn(move || {
                let mut text = String::new();
                let _ = stderr.take(TROUBLE).read_to_string(&mut text);
                text
            })
        });
        let deadline = Instant::now() + SAY_TIMEOUT;
        let exit = loop {
            match child.try_wait() {
                Ok(Some(exit)) => break Ok(exit),
                Ok(None) => {}
                Err(e) => break Err(format!("Could not wait for the voice: {e}.")),
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!(
                    "The voice took longer than {} seconds to say that.",
                    SAY_TIMEOUT.as_secs()
                ));
            }
            thread::sleep(POLL);
        };
        let trouble = said
            .take()
            .and_then(|reader| reader.join().ok())
            .unwrap_or_default();
        match exit {
            Err(why) => Err(why),
            Ok(exit) if exit.success() => Ok(()),
            Ok(exit) => Err(format!(
                "The voice could not say that: {}",
                why(&trouble, exit)
            )),
        }
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

/// What to put in front of a caller about a failed run: the last line the program said, or the
/// way it ended when it said nothing.
fn why(trouble: &str, exit: std::process::ExitStatus) -> String {
    trouble
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .map_or_else(|| exit.to_string(), ToString::to_string)
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

    #[test]
    fn a_failure_says_the_last_line_the_program_said() {
        let exit = std::process::Command::new("false").status().unwrap();
        assert_eq!(
            why("loading the model\n'sample_rate' does not exist\n\n", exit),
            "'sample_rate' does not exist"
        );
        assert_eq!(why("  \n", exit), exit.to_string());
    }
}
