//! `rift ai`: a question for Quasar from the terminal. An answer is printed. A command Quasar
//! proposes is printed as well and follows the rule Lens's field follows: one that only reads
//! runs at once, one that changes something runs after a yes. Without a question it prints
//! Quasar's state. `rift ai index` and `rift ai search` are search by meaning in home,
//! `rift ai say` reads words out loud with the voice on the drive, and `rift ai listen` writes down
//! the words in a recording.

use std::process::{Command, ExitCode, Stdio};

use librift::os::{self, Action};
use librift::quasar::{self, Reply, Status};

use crate::{search, text};

const USAGE: &str = "Usage: rift ai [--yes] [question]
       rift ai index
       rift ai search <words>
       rift ai say [--wav <file>] <words>
       rift ai listen <file>";

const HELP: &str =
    "Asks Quasar a question and prints the answer. When Quasar proposes a command that \
changes something, it runs only after you confirm it, or at once with --yes. Without a question, \
shows which models Quasar runs and whether they are ready.

  index    bring the search index of your home folder up to date. It also runs every 15 minutes.
  search   list the files in your home folder closest in meaning to the words, best first.
  say      read the words out loud. With --wav the audio goes into that file instead.
  listen   write down the words in a wav recording.";

const SAY_USAGE: &str = "Usage: rift ai say [--wav <file>] <words>";
const SAY_HELP: &str = "Reads the words out loud in the voice on the drive, through the machine's \
speakers. With --wav the sound is written to that file instead of played, and the line says how \
long it is.";
const LISTEN_USAGE: &str = "Usage: rift ai listen <file>";
const LISTEN_HELP: &str = "Writes down what was said in a wav recording and prints it. The \
recording is read here and its bytes go to Quasar, which opens no files of yours. When nothing was \
said in it, it says so.";

/// The program that plays the wav.
const PLAYER: &str = "pw-play";

pub fn run(args: &[String]) -> ExitCode {
    let (yes, words) = match args.first().map(String::as_str) {
        Some("--help" | "-h") => {
            println!("{USAGE}\n\n{HELP}");
            return ExitCode::SUCCESS;
        }
        Some("index") => return search::index(&args[1..]),
        Some("search") => return search::search(&args[1..]),
        Some("say") => return say(&args[1..]),
        Some("listen") => return listen(&args[1..]),
        Some("--yes" | "-y") => (true, &args[1..]),
        _ => (false, args),
    };
    let question = words.join(" ");
    if question.trim().is_empty() {
        return state();
    }
    let reply = match quasar::ask(&question) {
        Ok((kind, text)) => quasar::read(&kind, &text),
        Err(why) => Reply::Refused(why),
    };
    match reply {
        Reply::Answer(answer) => {
            println!("{answer}");
            ExitCode::SUCCESS
        }
        Reply::Action(action) => act(&action, yes),
        Reply::Refused(why) => {
            eprintln!("{why}");
            ExitCode::FAILURE
        }
    }
}

/// `rift ai say`: the words out loud, or into a wav file of the caller's choosing.
fn say(args: &[String]) -> ExitCode {
    if args
        .first()
        .is_some_and(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        println!("{SAY_USAGE}\n\n{SAY_HELP}");
        return ExitCode::SUCCESS;
    }
    let (wav, words) = if args.first().is_some_and(|arg| arg == "--wav") {
        let Some(file) = args.get(1) else {
            eprintln!("--wav needs a file to write.");
            return ExitCode::FAILURE;
        };
        (Some(file.clone()), &args[2..])
    } else {
        (None, args)
    };
    let text = words.join(" ");
    if text.trim().is_empty() {
        eprintln!("{SAY_USAGE}");
        return ExitCode::FAILURE;
    }
    let audio = match quasar::say(&text) {
        Ok(audio) => audio,
        Err(why) => {
            eprintln!("{why}");
            return ExitCode::FAILURE;
        }
    };
    match wav {
        Some(file) => match std::fs::write(&file, &audio) {
            Ok(()) => {
                println!("{}", wrote(&file, audio.len()));
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("Could not write {file}: {e}.");
                ExitCode::FAILURE
            }
        },
        None => play(&audio),
    }
}

/// `rift ai listen`: the words in a recording. The file is read here, as the caller, and the bytes
/// go to Quasar, which opens none of the owner's files.
fn listen(args: &[String]) -> ExitCode {
    if args
        .first()
        .is_some_and(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        println!("{LISTEN_USAGE}\n\n{LISTEN_HELP}");
        return ExitCode::SUCCESS;
    }
    let [file] = args else {
        eprintln!("{LISTEN_USAGE}");
        return ExitCode::FAILURE;
    };
    let wav = match std::fs::read(file) {
        Ok(wav) => wav,
        Err(e) => {
            eprintln!("Could not read {file}: {e}.");
            return ExitCode::FAILURE;
        }
    };
    match quasar::listen(&wav) {
        Ok(words) if words.is_empty() => {
            eprintln!("Nothing was said in {file}.");
            ExitCode::FAILURE
        }
        Ok(words) => {
            println!("{words}");
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("{why}");
            ExitCode::FAILURE
        }
    }
}

/// The line that says where the audio went and how long it is.
fn wrote(file: &str, bytes: usize) -> String {
    format!("Wrote {} to {file}.", seconds(bytes))
}

/// How long a wav of this many bytes is, as the voice writes them: one channel of 16 bit samples
/// at 22050 a second, after a header of 44 bytes.
fn seconds(bytes: usize) -> String {
    let samples = bytes.saturating_sub(44) / 2;
    #[expect(clippy::cast_precision_loss, reason = "a wav of seconds, not of hours")]
    let seconds = samples as f64 / 22050.0;
    format!("{seconds:.1} seconds")
}

/// Plays the wav on the machine's speakers. The player reads a file, so the audio goes into the
/// caller's own runtime directory for as long as it plays and is gone afterwards.
fn play(audio: &[u8]) -> ExitCode {
    let file = std::env::var_os("XDG_RUNTIME_DIR")
        .map_or_else(std::env::temp_dir, std::path::PathBuf::from)
        .join(format!("rift-say-{}.wav", std::process::id()));
    if let Err(e) = std::fs::write(&file, audio) {
        eprintln!("Could not write the words to {}: {e}.", file.display());
        return ExitCode::FAILURE;
    }
    let played = Command::new(PLAYER)
        .arg(&file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status();
    let _ = std::fs::remove_file(&file);
    match played {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => {
            eprintln!("{PLAYER} ended with {status}.");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("Could not play the words: {e}.");
            ExitCode::FAILURE
        }
    }
}

fn state() -> ExitCode {
    match quasar::status() {
        Ok(status) => {
            print!("{}", text::table(&rows(&status)));
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("{why}");
            ExitCode::FAILURE
        }
    }
}

fn rows(status: &Status) -> Vec<(&'static str, String)> {
    let or_none = |value: &str| {
        if value.is_empty() {
            "none".to_string()
        } else {
            value.to_string()
        }
    };
    let mut rows = vec![
        ("State", status.state.clone()),
        ("Model", or_none(&status.model)),
        ("Tier", or_none(&status.tier)),
    ];
    if !status.error.is_empty() {
        rows.push(("Error", status.error.clone()));
    }
    let search = if status.embedding_model.is_empty() {
        status.embedding_state.clone()
    } else {
        format!("{}, {}", status.embedding_state, status.embedding_model)
    };
    rows.push(("Search", search));
    if !status.embedding_error.is_empty() {
        rows.push(("Search error", status.embedding_error.clone()));
    }
    let voice = if status.voice.is_empty() {
        status.voice_state.clone()
    } else {
        format!("{}, {}", status.voice_state, status.voice)
    };
    rows.push(("Voice", voice));
    if !status.voice_error.is_empty() {
        rows.push(("Voice error", status.voice_error.clone()));
    }
    let speech = if status.speech.is_empty() {
        status.speech_state.clone()
    } else {
        format!("{}, {}", status.speech_state, status.speech)
    };
    rows.push(("Speech", speech));
    if !status.speech_error.is_empty() {
        rows.push(("Speech error", status.speech_error.clone()));
    }
    rows
}

/// The command Quasar proposed: printed first, then run, after a yes when it changes something.
fn act(action: &Action, yes: bool) -> ExitCode {
    println!("{}: {}", action.summary, text::command_line(action));
    if action.mutating && !yes {
        match text::confirm("Run this command?") {
            Some(true) => {}
            Some(false) => {
                eprintln!("Nothing was run.");
                return ExitCode::FAILURE;
            }
            None => {
                eprintln!("Nothing was run. Add --yes to run it without a question.");
                return ExitCode::FAILURE;
            }
        }
    }
    match os::run(action) {
        Ok(output) => {
            if !output.is_empty() {
                println!("{output}");
            }
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("{why}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_models_are_six_rows() {
        let ready = Status {
            state: "ready".into(),
            model: "qwen3-0.6b-q8_0".into(),
            tier: "small".into(),
            error: String::new(),
            embedding_state: "ready".into(),
            embedding_model: "nomic-embed-text-v1.5-q8".into(),
            embedding_error: String::new(),
            voice_state: "ready".into(),
            voice: "piper-en-us-lessac-medium".into(),
            voice_error: String::new(),
            speech_state: "ready".into(),
            speech: "whisper-base".into(),
            speech_error: String::new(),
        };
        assert_eq!(
            text::table(&rows(&ready)),
            "State:  ready\nModel:  qwen3-0.6b-q8_0\nTier:   small\n\
             Search: ready, nomic-embed-text-v1.5-q8\n\
             Voice:  ready, piper-en-us-lessac-medium\n\
             Speech: ready, whisper-base\n"
        );
    }

    #[test]
    fn a_wav_is_as_long_as_the_samples_in_it() {
        assert_eq!(seconds(44), "0.0 seconds");
        assert_eq!(seconds(44 + 22050 * 2), "1.0 seconds");
        assert_eq!(seconds(44 + 22050 * 2 * 5), "5.0 seconds");
        assert_eq!(
            wrote("said.wav", 44 + 22050 * 2 * 3),
            "Wrote 3.0 seconds to said.wav."
        );
    }

    #[test]
    fn without_a_model_the_reason_is_a_row() {
        let none = Status {
            state: "none".into(),
            model: String::new(),
            tier: "small".into(),
            error: "No chat model that fits this machine is on the drive.".into(),
            embedding_state: "none".into(),
            embedding_model: String::new(),
            embedding_error:
                "Search by meaning needs nomic-embed-text-v1.5.Q8_0.gguf, which is not on the drive."
                    .into(),
            voice_state: "none".into(),
            voice: String::new(),
            voice_error: "Saying words out loud needs en_US-lessac-medium.onnx, which is not on \
                          the drive."
                .into(),
            speech_state: "none".into(),
            speech: String::new(),
            speech_error: "Turning speech into words needs ggml-base.bin, which is not on the \
                           drive."
                .into(),
        };
        assert_eq!(
            rows(&none),
            [
                ("State", "none".to_string()),
                ("Model", "none".to_string()),
                ("Tier", "small".to_string()),
                (
                    "Error",
                    "No chat model that fits this machine is on the drive.".to_string()
                ),
                ("Search", "none".to_string()),
                (
                    "Search error",
                    "Search by meaning needs nomic-embed-text-v1.5.Q8_0.gguf, which is not on \
                     the drive."
                        .to_string()
                ),
                ("Voice", "none".to_string()),
                (
                    "Voice error",
                    "Saying words out loud needs en_US-lessac-medium.onnx, which is not on the \
                     drive."
                        .to_string()
                ),
                ("Speech", "none".to_string()),
                (
                    "Speech error",
                    "Turning speech into words needs ggml-base.bin, which is not on the drive."
                        .to_string()
                ),
            ]
        );
    }
}
