//! Quasar on the system bus: `dev.rift.Quasar` at `/dev/rift/Quasar`.
//!
//! `Ask` takes a question and returns two strings, a kind and a text: `answer` and the answer in
//! words, or `action` and the words of one of Lens's OS commands. It runs nothing. `Embed` turns
//! texts into vectors for search by meaning; whoever calls it reads the files, Quasar never does.
//! `Say` turns words into a wav and hands the caller the audio itself, for the same reason: what
//! is done with it is the caller's, and Quasar writes into nobody's files. `Listen` is `Say` the
//! other way round: the bytes of a recording go in and the words that were said come back, so
//! Quasar opens none of the owner's files for that either.
//! The properties say which models run, for which tier, and whether they answer yet; every change
//! to them is signalled, so a client can wait for `ready` without polling.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use librift::Component;
use zbus::fdo;

use crate::backend::{State, Status};
use crate::{chat, embed, speech, voice};
use librift::models::Embedding;

/// The longest question `Ask` takes, in characters.
const LONGEST_QUESTION: usize = 4000;
/// The most texts one call to `Embed` takes.
const MOST_TEXTS: usize = 32;
/// The longest text `Embed` takes, in characters. A part of a file is at most 1500, and 4000 is
/// still well inside the embedding model's context at one token a character.
const LONGEST_TEXT: usize = 4000;
/// The longest text `Say` takes, in characters. About a minute of speech, which is as much as
/// anyone wants read out in one go and keeps the wav well inside what a bus message carries.
const LONGEST_SAY: usize = 1000;
/// The largest recording `Listen` takes, in bytes. Six minutes of what the voice writes, or a
/// minute and a half of a recording at the rate a microphone is usually read.
const LARGEST_RECORDING: usize = 16 << 20;

/// The object that answers on the bus.
pub struct Quasar {
    /// The chat model's backend.
    pub status: Arc<Mutex<Status>>,
    /// The chat model's socket.
    pub socket: PathBuf,
    /// The embedding model's backend.
    pub embedding: Arc<Mutex<Status>>,
    /// The embedding model's socket.
    pub embedding_socket: PathBuf,
    /// The embedding models in the manifest, for the words that go in front of each text.
    pub embeddings: Vec<Embedding>,
    /// The voice's state. There is no server for it, so it is ready as soon as one is on the drive.
    pub voice: Arc<Mutex<Status>>,
    /// How to run the voice.
    pub speaker: Arc<voice::Voice>,
    /// The voices in the manifest, for the files the one that runs reads.
    pub voices: Vec<librift::models::Voice>,
    /// The speech model's state. There is no server for it either, so it is ready as soon as one
    /// is on the drive.
    pub speech: Arc<Mutex<Status>>,
    /// How to turn speech into words.
    pub listener: Arc<speech::Speech>,
    /// The speech models in the manifest, for the file the one that runs reads.
    pub speeches: Vec<librift::models::Speech>,
}

fn now(status: &Mutex<Status>) -> Status {
    status
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

#[zbus::interface(name = "dev.rift.Quasar")]
impl Quasar {
    /// Answers a question: `answer` and plain text, or `action` and the words of a command.
    #[zbus(out_args("kind", "text"))]
    async fn ask(&self, question: String) -> fdo::Result<(String, String)> {
        let question = admit(&question, &now(&self.status))?;
        let socket = self.socket.clone();
        let reply = blocking::unblock(move || chat::ask(&socket, &question))
            .await
            .map_err(fdo::Error::Failed)?;
        Ok((reply.kind().to_string(), reply.into_text()))
    }

    /// A vector for each text, in their order. `kind` is `query` for the words a person searches
    /// with and `document` for what is searched.
    #[zbus(out_args("vectors"))]
    async fn embed(&self, kind: &str, texts: Vec<String>) -> fdo::Result<Vec<Vec<f64>>> {
        let status = now(&self.embedding);
        let model = self
            .embeddings
            .iter()
            .find(|model| model.id == status.model);
        let texts = admit_texts(kind, texts, &status, model)?;
        let socket = self.embedding_socket.clone();
        blocking::unblock(move || embed::vectors(&socket, &texts))
            .await
            .map_err(fdo::Error::Failed)
    }

    /// The words as a wav, said with the voice on the drive.
    #[zbus(out_args("wav"))]
    async fn say(&self, text: String) -> fdo::Result<Vec<u8>> {
        let status = now(&self.voice);
        let text = admit_words(&text, &status)?;
        let voice = self
            .voices
            .iter()
            .find(|voice| voice.id == status.model)
            .cloned()
            .ok_or_else(|| {
                fdo::Error::Failed("The voice that runs is not in the manifest.".into())
            })?;
        let speaker = Arc::clone(&self.speaker);
        blocking::unblock(move || speaker.say(&voice, &text))
            .await
            .map_err(fdo::Error::Failed)
    }

    /// The words that were said in the recording. A wav goes in, and what came back empty was a
    /// recording with nothing in it.
    #[zbus(out_args("words"))]
    async fn listen(&self, wav: Vec<u8>) -> fdo::Result<String> {
        let status = now(&self.speech);
        admit_recording(&wav, &status)?;
        let model = self
            .speeches
            .iter()
            .find(|model| model.id == status.model)
            .cloned()
            .ok_or_else(|| {
                fdo::Error::Failed("The speech model that runs is not in the manifest.".into())
            })?;
        let listener = Arc::clone(&self.listener);
        blocking::unblock(move || listener.listen(&model, &wav))
            .await
            .map_err(fdo::Error::Failed)
    }

    /// Manifest id of the model that runs or loads. Empty when there is none.
    #[zbus(property)]
    fn model(&self) -> String {
        now(&self.status).model
    }

    /// The tier Orbit reported. Empty when it did not say.
    #[zbus(property)]
    fn tier(&self) -> String {
        now(&self.status).tier
    }

    /// `none`, `loading`, `ready` or `failed`.
    #[zbus(property)]
    fn state(&self) -> String {
        now(&self.status).state.name().to_string()
    }

    /// Why nothing answers, in a sentence. Empty when the model runs.
    #[zbus(property)]
    fn error(&self) -> String {
        now(&self.status).error
    }

    /// Manifest id of the embedding model that runs or loads. Empty when there is none.
    #[zbus(property)]
    fn embedding_model(&self) -> String {
        now(&self.embedding).model
    }

    /// The embedding model's state: `none`, `loading`, `ready` or `failed`.
    #[zbus(property)]
    fn embedding_state(&self) -> String {
        now(&self.embedding).state.name().to_string()
    }

    /// Why search by meaning cannot run, in a sentence. Empty when the embedding model runs.
    #[zbus(property)]
    fn embedding_error(&self) -> String {
        now(&self.embedding).error
    }

    /// Manifest id of the voice that says words out loud. Empty when there is none.
    #[zbus(property)]
    fn voice(&self) -> String {
        now(&self.voice).model
    }

    /// The voice's state: `none` or `ready`. Nothing runs between sentences, so it never loads.
    #[zbus(property)]
    fn voice_state(&self) -> String {
        now(&self.voice).state.name().to_string()
    }

    /// Why nothing can say words out loud, in a sentence. Empty when a voice is on the drive.
    #[zbus(property)]
    fn voice_error(&self) -> String {
        now(&self.voice).error
    }

    /// Manifest id of the speech model that turns speech into words. Empty when there is none.
    #[zbus(property)]
    fn speech(&self) -> String {
        now(&self.speech).model
    }

    /// The speech model's state: `none` or `ready`. Nothing runs between recordings, so it never
    /// loads.
    #[zbus(property)]
    fn speech_state(&self) -> String {
        now(&self.speech).state.name().to_string()
    }

    /// Why nothing can turn speech into words, in a sentence. Empty when a model is on the drive.
    #[zbus(property)]
    fn speech_error(&self) -> String {
        now(&self.speech).error
    }
}

/// The question as it goes to the model, or why it cannot go now.
fn admit(question: &str, status: &Status) -> fdo::Result<String> {
    let question = question.trim();
    if question.is_empty() {
        return Err(fdo::Error::InvalidArgs("Ask needs a question.".into()));
    }
    if question.chars().count() > LONGEST_QUESTION {
        return Err(fdo::Error::InvalidArgs(format!(
            "A question can be at most {LONGEST_QUESTION} characters long."
        )));
    }
    match status.state {
        State::Ready => Ok(question.to_string()),
        State::Loading => Err(fdo::Error::Failed(
            "The model is still loading. Try again in a moment.".into(),
        )),
        State::NoModel | State::Failed => Err(fdo::Error::Failed(status.error.clone())),
    }
}

/// The words as they go to the voice, or why they cannot go now.
fn admit_words(text: &str, status: &Status) -> fdo::Result<String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(fdo::Error::InvalidArgs("Say needs words to say.".into()));
    }
    if text.chars().count() > LONGEST_SAY {
        return Err(fdo::Error::InvalidArgs(format!(
            "Say takes at most {LONGEST_SAY} characters at a time."
        )));
    }
    match status.state {
        State::Ready => Ok(text.to_string()),
        _ => Err(fdo::Error::Failed(status.error.clone())),
    }
}

/// Whether the recording can go to whisper now, or why it cannot.
fn admit_recording(wav: &[u8], status: &Status) -> fdo::Result<()> {
    if wav.is_empty() {
        return Err(fdo::Error::InvalidArgs(
            "Listen needs a recording to read.".into(),
        ));
    }
    if wav.len() > LARGEST_RECORDING {
        return Err(fdo::Error::InvalidArgs(format!(
            "Listen takes a recording of at most {} megabytes.",
            LARGEST_RECORDING >> 20
        )));
    }
    // a wav, and nothing else: the program reads a few other kinds of audio file, and a recording
    // is what Rift itself writes and what the desktop will hand it
    if wav.len() < 12 || &wav[..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return Err(fdo::Error::InvalidArgs(
            "Listen takes a wav recording, and that is not one.".into(),
        ));
    }
    match status.state {
        State::Ready => Ok(()),
        _ => Err(fdo::Error::Failed(status.error.clone())),
    }
}

/// The texts as they go to the embedding model, with the running model's words for `kind` in
/// front of each, or why they cannot go now.
fn admit_texts(
    kind: &str,
    texts: Vec<String>,
    status: &Status,
    model: Option<&Embedding>,
) -> fdo::Result<Vec<String>> {
    if kind != "query" && kind != "document" {
        return Err(fdo::Error::InvalidArgs(format!(
            "Embed takes a query or a document, not {kind:?}."
        )));
    }
    if texts.is_empty() || texts.len() > MOST_TEXTS {
        return Err(fdo::Error::InvalidArgs(format!(
            "Embed takes from 1 to {MOST_TEXTS} texts."
        )));
    }
    if texts.iter().any(|text| text.trim().is_empty()) {
        return Err(fdo::Error::InvalidArgs(
            "Embed needs words in every text.".into(),
        ));
    }
    if texts.iter().any(|text| text.chars().count() > LONGEST_TEXT) {
        return Err(fdo::Error::InvalidArgs(format!(
            "A text can be at most {LONGEST_TEXT} characters long."
        )));
    }
    match status.state {
        State::Ready => {}
        State::Loading => {
            return Err(fdo::Error::Failed(
                "The embedding model is still loading. Try again in a moment.".into(),
            ));
        }
        State::NoModel | State::Failed => return Err(fdo::Error::Failed(status.error.clone())),
    }
    let model = model.ok_or_else(|| {
        fdo::Error::Failed("The embedding model that runs is not in the manifest.".into())
    })?;
    let prefix = if kind == "query" {
        &model.query_prefix
    } else {
        &model.document_prefix
    };
    Ok(texts
        .into_iter()
        .map(|text| prefix.clone() + &text)
        .collect())
}

/// Connects to the system bus and serves the object. The name comes later, from `take_name`.
///
/// # Errors
///
/// When the system bus is not there.
pub fn connect(quasar: Quasar) -> zbus::Result<zbus::blocking::Connection> {
    zbus::blocking::connection::Builder::system()?
        .serve_at(Component::Quasar.dbus_path(), quasar)?
        .build()
}

/// Takes `dev.rift.Quasar`. systemd counts the service as started from here on.
///
/// # Errors
///
/// When another process owns the name or the policy does not allow it.
pub fn take_name(connection: &zbus::blocking::Connection) -> zbus::Result<()> {
    connection.request_name(Component::Quasar.dbus_name())
}

/// Signals every property, after a status changed.
///
/// # Errors
///
/// When the object is not served or the bus is gone.
pub fn announce(connection: &zbus::blocking::Connection) -> zbus::Result<()> {
    let object = connection
        .object_server()
        .interface::<_, Quasar>(Component::Quasar.dbus_path())?;
    let quasar = object.get();
    let emitter = object.signal_emitter();
    zbus::block_on(async {
        quasar.state_changed(emitter).await?;
        quasar.model_changed(emitter).await?;
        quasar.tier_changed(emitter).await?;
        quasar.error_changed(emitter).await?;
        quasar.embedding_state_changed(emitter).await?;
        quasar.embedding_model_changed(emitter).await?;
        quasar.embedding_error_changed(emitter).await?;
        quasar.voice_state_changed(emitter).await?;
        quasar.voice_changed(emitter).await?;
        quasar.voice_error_changed(emitter).await?;
        quasar.speech_state_changed(emitter).await?;
        quasar.speech_changed(emitter).await?;
        quasar.speech_error_changed(emitter).await
    })
}

/// Orbit's `AiTier`.
///
/// # Errors
///
/// When Orbit is not on the bus.
pub fn orbit_tier(connection: &zbus::blocking::Connection) -> zbus::Result<String> {
    let orbit = Component::Orbit;
    let proxy: zbus::blocking::Proxy<'_> = zbus::blocking::proxy::Builder::new(connection)
        .destination(orbit.dbus_name())?
        .path(orbit.dbus_path())?
        .interface(orbit.dbus_name())?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()?;
    proxy.get_property("AiTier")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(state: State, error: &str) -> Status {
        Status {
            state,
            model: "qwen3-0.6b-q8_0".into(),
            tier: "small".into(),
            error: error.into(),
        }
    }

    #[test]
    fn a_question_goes_through_once_the_model_is_ready() {
        let ready = status(State::Ready, "");
        assert_eq!(
            admit("  What is the capital of France?\n", &ready).unwrap(),
            "What is the capital of France?"
        );
    }

    #[test]
    fn an_empty_or_long_question_is_refused() {
        let ready = status(State::Ready, "");
        assert!(matches!(
            admit(" \n", &ready),
            Err(fdo::Error::InvalidArgs(_))
        ));
        let long = "a".repeat(LONGEST_QUESTION + 1);
        assert!(matches!(
            admit(&long, &ready),
            Err(fdo::Error::InvalidArgs(_))
        ));
    }

    #[test]
    fn without_a_ready_model_the_reason_is_the_answer() {
        let loading = status(State::Loading, "");
        assert!(matches!(
            admit("hi", &loading),
            Err(fdo::Error::Failed(why)) if why.contains("still loading")
        ));
        let none = status(
            State::NoModel,
            "No chat model that fits this machine is on the drive.",
        );
        assert!(matches!(
            admit("hi", &none),
            Err(fdo::Error::Failed(why)) if why.starts_with("No chat model")
        ));
    }

    fn nomic() -> Embedding {
        Embedding {
            id: "nomic-embed-text-v1.5-q8".into(),
            file: "nomic-embed-text-v1.5.Q8_0.gguf".into(),
            query_prefix: "search_query: ".into(),
            document_prefix: "search_document: ".into(),
        }
    }

    fn texts(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn each_text_gets_the_words_for_its_side_of_the_search() {
        let ready = Status {
            model: "nomic-embed-text-v1.5-q8".into(),
            ..status(State::Ready, "")
        };
        let model = nomic();
        assert_eq!(
            admit_texts("query", texts(&["bicycle repair"]), &ready, Some(&model)).unwrap(),
            ["search_query: bicycle repair"]
        );
        assert_eq!(
            admit_texts(
                "document",
                texts(&["bike.txt\nOil the chain", "b"]),
                &ready,
                Some(&model)
            )
            .unwrap(),
            [
                "search_document: bike.txt\nOil the chain",
                "search_document: b"
            ]
        );
    }

    #[test]
    fn texts_embed_refuses() {
        let ready = status(State::Ready, "");
        let model = nomic();
        let refused = |kind: &str, list: Vec<String>| {
            matches!(
                admit_texts(kind, list, &ready, Some(&model)),
                Err(fdo::Error::InvalidArgs(_))
            )
        };
        assert!(refused("answer", texts(&["a"])));
        assert!(refused("query", Vec::new()));
        assert!(refused("query", vec!["a".to_string(); MOST_TEXTS + 1]));
        assert!(refused("document", texts(&["a", " \n"])));
        assert!(refused("document", vec!["a".repeat(LONGEST_TEXT + 1)]));
        assert!(matches!(
            admit_texts("query", texts(&["a"]), &ready, None),
            Err(fdo::Error::Failed(why)) if why.contains("not in the manifest")
        ));
    }

    #[test]
    fn words_go_to_the_voice_once_one_is_on_the_drive() {
        let ready = Status {
            model: "piper-en-us-lessac-medium".into(),
            ..status(State::Ready, "")
        };
        assert_eq!(
            admit_words("  Rift says this out loud.\n", &ready).unwrap(),
            "Rift says this out loud."
        );
        assert!(matches!(
            admit_words(" \n", &ready),
            Err(fdo::Error::InvalidArgs(_))
        ));
        let long = "a".repeat(LONGEST_SAY + 1);
        assert!(matches!(
            admit_words(&long, &ready),
            Err(fdo::Error::InvalidArgs(_))
        ));
        let none = status(
            State::NoModel,
            "Saying words out loud needs en_US-lessac-medium.onnx, which is not on the drive.",
        );
        assert!(matches!(
            admit_words("hello", &none),
            Err(fdo::Error::Failed(why)) if why.starts_with("Saying words out loud needs")
        ));
    }

    #[test]
    fn a_recording_goes_to_whisper_once_a_speech_model_is_on_the_drive() {
        let ready = Status {
            model: "whisper-base".into(),
            ..status(State::Ready, "")
        };
        let wav = |bytes: &[u8]| {
            let mut file = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
            file.extend_from_slice(bytes);
            file
        };
        assert!(admit_recording(&wav(b"the samples"), &ready).is_ok());
        assert!(matches!(
            admit_recording(&[], &ready),
            Err(fdo::Error::InvalidArgs(_))
        ));
        assert!(matches!(
            admit_recording(b"ID3 a song", &ready),
            Err(fdo::Error::InvalidArgs(why)) if why.contains("not one")
        ));
        assert!(matches!(
            admit_recording(&wav(&vec![0; LARGEST_RECORDING]), &ready),
            Err(fdo::Error::InvalidArgs(why)) if why.contains("at most 16 megabytes")
        ));
        let none = status(
            State::NoModel,
            "Turning speech into words needs ggml-base.bin, which is not on the drive.",
        );
        assert!(matches!(
            admit_recording(&wav(b"the samples"), &none),
            Err(fdo::Error::Failed(why)) if why.starts_with("Turning speech into words needs")
        ));
    }

    #[test]
    fn without_a_ready_embedding_model_the_reason_is_the_answer() {
        let model = nomic();
        let loading = status(State::Loading, "");
        assert!(matches!(
            admit_texts("query", texts(&["a"]), &loading, Some(&model)),
            Err(fdo::Error::Failed(why)) if why.contains("still loading")
        ));
        let none = status(
            State::NoModel,
            "Search by meaning needs nomic-embed-text-v1.5.Q8_0.gguf, which is not on the drive.",
        );
        assert!(matches!(
            admit_texts("query", texts(&["a"]), &none, Some(&model)),
            Err(fdo::Error::Failed(why)) if why.starts_with("Search by meaning needs")
        ));
    }
}
