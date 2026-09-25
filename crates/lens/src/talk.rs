//! The shell's ear and its mouth. A key starts a recording, a second one stops it, and what was
//! said goes into the field: `pw-record` writes a wav into the session's runtime directory while
//! the key is on, Quasar's `Listen` turns it into words, and the answer to a question asked this
//! way is read back out with `Say` and played through `pw-play`.
//!
//! Neither program is a server. The recorder is a child of the shell, so it lives in the shell's
//! own control group and goes when the shell goes, and the wav is removed as soon as it has been
//! read. Nothing leaves the machine and nothing is kept.

// the shell is linux only, and so is everything that talks to pipewire
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// The program that records from the default source.
const RECORDER: &str = "pw-record";
/// The program that plays a wav on the default sink.
const PLAYER: &str = "pw-play";
/// How many samples a second the recording holds. Whisper reads whatever it is given, and this is
/// the rate it works at, so the bytes stay small and nothing has to be converted.
const RATE: u32 = 16000;
/// How long the shell goes on listening when nobody stops it. A minute of a room is nothing
/// anybody said on purpose, so a recording that reaches this is thrown away.
pub const MOST: Duration = Duration::from_secs(60);
/// How long to wait for the recorder to write the end of the file after it has been asked to stop.
const STOPPING: Duration = Duration::from_secs(5);
/// How often to look while waiting for that.
const LOOK: Duration = Duration::from_millis(50);
/// How loud the loudest sample has to be for the recording to hold speech rather than a room, as a
/// part of what a 16 bit sample can hold. The voice's own sentences peak above two thirds of that,
/// and a model handed silence makes a word up rather than saying nothing.
const HEARD: i32 = 500;
/// How many characters of an answer are read out loud. Quasar's `Say` takes a thousand, which is
/// about a minute of speech, and an answer longer than that is one to read.
const ALOUD: usize = 1000;
/// The header of a wav, up to the first sample: the RIFF and WAVE marks, the format of the sound
/// and the mark and the length of the data.
const HEADER: usize = 44;

/// A recording that is being made.
#[derive(Debug)]
pub struct Recording {
    /// The recorder, which is a child of the shell.
    child: Child,
    /// The file it is writing.
    file: PathBuf,
}

/// Start recording from the machine's own microphone, which is whatever the default source is.
///
/// # Errors
///
/// When the session has no runtime directory, or the recorder could not be started.
pub fn start() -> Result<Recording, String> {
    let file = wav("heard")?;
    let _ = std::fs::remove_file(&file);
    let child = Command::new(RECORDER)
        .args(["--rate", &RATE.to_string()])
        .args(["--channels", "1"])
        .args(["--format", "s16"])
        .arg(&file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start {RECORDER}: {e}"))?;
    Ok(Recording { child, file })
}

impl Recording {
    /// Stop the recorder and give back the wav it wrote. The recorder is asked to stop rather than
    /// killed, since it writes the length of the sound into the header on its way out.
    ///
    /// # Errors
    ///
    /// When the recorder could not be stopped, or it wrote no file.
    pub fn stop(mut self) -> Result<Vec<u8>, String> {
        let _ = Command::new("kill")
            .args(["-TERM", &self.child.id().to_string()])
            .status();
        let until = std::time::Instant::now() + STOPPING;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Err(e) => return Err(format!("Could not wait for {RECORDER}: {e}")),
                Ok(None) if std::time::Instant::now() > until => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
                Ok(None) => std::thread::sleep(LOOK),
            }
        }
        let mut wav =
            std::fs::read(&self.file).map_err(|e| format!("Could not read the recording: {e}"))?;
        let _ = std::fs::remove_file(&self.file);
        mend(&mut wav);
        Ok(wav)
    }
}

/// The words in a recording, or nothing at all when nobody spoke into it.
///
/// # Errors
///
/// A sentence for the line under the field: no model on the drive to read it, or Quasar did not
/// answer.
pub fn words(wav: &[u8]) -> Result<Option<String>, String> {
    if loudest(wav) < HEARD {
        return Ok(None);
    }
    let said = librift::quasar::listen(wav)?;
    Ok(Some(said).filter(|said| !said.trim().is_empty()))
}

/// Read an answer out loud on the machine's speakers. The player reads a file, so the sound goes
/// into the session's own runtime directory for as long as it plays and is gone afterwards.
///
/// # Errors
///
/// A sentence when there is no voice on the drive, or the sound could not be played.
pub fn say(answer: &str) -> Result<String, String> {
    let text = aloud(answer);
    let sound = librift::quasar::say(&text)?;
    let file = wav("said")?;
    std::fs::write(&file, &sound).map_err(|e| format!("Could not write the answer: {e}"))?;
    let played = Command::new(PLAYER)
        .arg(&file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status();
    let _ = std::fs::remove_file(&file);
    match played {
        Ok(status) if status.success() => Ok(text),
        Ok(status) => Err(format!("{PLAYER} ended with {status}")),
        Err(e) => Err(format!("Could not play the answer: {e}")),
    }
}

/// Where a wav of the shell's own goes while it is being written or played: the session's runtime
/// directory, which belongs to one person and is emptied when they log out.
fn wav(what: &str) -> Result<PathBuf, String> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .map(|dir| dir.join(format!("lens-{what}.wav")))
        .ok_or_else(|| "Lens could not find the session runtime directory".to_string())
}

/// The loudest sample in a recording, as a number out of the 32767 a 16 bit sample can hold.
/// Nothing at all is zero, which is what a machine with no microphone records.
#[must_use]
pub fn loudest(wav: &[u8]) -> i32 {
    wav.get(HEADER..)
        .unwrap_or_default()
        .chunks_exact(2)
        .map(|sample| i32::from(i16::from_le_bytes([sample[0], sample[1]])).abs())
        .max()
        .unwrap_or(0)
}

/// Write the lengths in the header from the file itself, when the recorder left them behind. It is
/// asked to stop in the middle of a recording, and a wav whose header says nothing is in it is a
/// wav no model will read.
fn mend(wav: &mut [u8]) {
    let whole = wav.len();
    if whole < HEADER || &wav[..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return;
    }
    let riff = u32::try_from(whole - 8).unwrap_or(u32::MAX);
    if u32::from_le_bytes([wav[4], wav[5], wav[6], wav[7]]) != riff {
        wav[4..8].copy_from_slice(&riff.to_le_bytes());
    }
    // the chunks of a wav, one length after another, until the one the samples are in
    let mut at = 12;
    while at + 8 <= whole {
        let size = u32::from_le_bytes([wav[at + 4], wav[at + 5], wav[at + 6], wav[at + 7]]);
        let size = usize::try_from(size).unwrap_or(usize::MAX);
        if &wav[at..at + 4] == b"data" {
            let rest = whole - at - 8;
            if size == 0 || size > rest {
                let rest = u32::try_from(rest).unwrap_or(u32::MAX);
                wav[at + 4..at + 8].copy_from_slice(&rest.to_le_bytes());
            }
            return;
        }
        // a chunk of an odd length is followed by a byte of nothing
        match at.checked_add(8 + size + (size & 1)) {
            Some(next) => at = next,
            None => return,
        }
    }
}

/// The part of an answer that is read out loud: whole sentences up to the thousand characters
/// Quasar takes, or the words that fit when the first sentence is longer than that.
#[must_use]
pub fn aloud(answer: &str) -> String {
    let answer = answer.trim();
    if answer.chars().count() <= ALOUD {
        return answer.to_string();
    }
    let mut end = ALOUD;
    while !answer.is_char_boundary(end) {
        end -= 1;
    }
    let fits = &answer[..end];
    let cut = fits
        .rfind(['.', '!', '?'])
        .map(|at| at + 1)
        .or_else(|| fits.rfind(char::is_whitespace))
        .unwrap_or(end);
    fits[..cut].trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wav of these samples, with the lengths in the header as the recorder writes them.
    fn recording(samples: &[i16]) -> Vec<u8> {
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&u32::try_from(36 + data.len()).unwrap().to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&RATE.to_le_bytes());
        wav.extend_from_slice(&(RATE * 2).to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        wav.extend_from_slice(&data);
        wav
    }

    #[test]
    fn a_room_is_quieter_than_a_sentence() {
        assert_eq!(loudest(&recording(&[])), 0);
        assert_eq!(loudest(&recording(&[0, 0, 0, 0])), 0);
        assert_eq!(loudest(&recording(&[3, -7, 2])), 7);
        assert!(loudest(&recording(&[10, -20000, 30])) > HEARD);
        assert!(loudest(&recording(&[10, -20, 30])) < HEARD);
        assert_eq!(loudest(b"not a recording"), 0);
    }

    #[test]
    fn a_header_the_recorder_left_behind_is_written_from_the_file() {
        let mut wav = recording(&[1, 2, 3, 4]);
        let whole = wav.len();
        wav[4..8].copy_from_slice(&0u32.to_le_bytes());
        wav[40..44].copy_from_slice(&0u32.to_le_bytes());
        mend(&mut wav);
        assert_eq!(
            u32::from_le_bytes([wav[4], wav[5], wav[6], wav[7]]),
            u32::try_from(whole - 8).unwrap()
        );
        assert_eq!(u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]), 8);
        assert_eq!(loudest(&wav), 4);
    }

    #[test]
    fn a_header_that_is_right_is_left_alone() {
        let whole = recording(&[5, 6]);
        let mut wav = whole.clone();
        mend(&mut wav);
        assert_eq!(wav, whole);
        // and a file that is not a wav at all is not written to
        let mut junk = b"this is not audio".to_vec();
        let was = junk.clone();
        mend(&mut junk);
        assert_eq!(junk, was);
    }

    #[test]
    fn what_is_read_out_loud_is_whole_sentences() {
        assert_eq!(
            aloud("  The capital of France is Paris.  "),
            "The capital of France is Paris."
        );
        let long = format!(
            "{} And this last one does not fit.",
            "A sentence of some length. ".repeat(40)
        );
        let said = aloud(&long);
        assert!(said.chars().count() <= ALOUD);
        assert!(said.ends_with("A sentence of some length."));
        // one sentence longer than the whole of it is cut between words
        let one = "word ".repeat(400);
        let said = aloud(&one);
        assert!(said.chars().count() <= ALOUD);
        assert!(said.ends_with("word"));
    }
}
