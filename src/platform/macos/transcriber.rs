//! Local speech-to-text with whisper.cpp. Audio never leaves the machine.

use crate::audio;
use crate::config::{Dictation, WhisperModel};
use crate::facade::{Audio, FacadeError, Result};
use crate::{models, transcript};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

const MAX_THREADS: usize = 8;

/// Loads the configured model once and keeps it in memory between dictations.
pub struct Transcriber {
    model: WhisperModel,
    path: Option<PathBuf>,
    language: Option<String>,
    context: Option<WhisperContext>,
    /// A model load running in the background, started by [`Transcriber::preload`].
    loading: Option<mpsc::Receiver<Result<WhisperContext>>>,
}

impl Transcriber {
    pub fn new(dictation: &Dictation) -> Self {
        Self {
            model: dictation.model,
            path: models::default_path(dictation.model),
            language: models::whisper_language(dictation.model, &dictation.language)
                .map(str::to_owned),
            context: None,
            loading: None,
        }
    }

    /// Starts loading the model in the background, so startup isn't blocked by the
    /// several seconds Metal needs. Fails at once if the model file is missing.
    pub fn preload(&mut self) -> Result<()> {
        let path = model_file(self.path.as_deref(), self.model)?.to_owned();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let _ = sender.send(open(&path));
        });
        self.loading = Some(receiver);
        Ok(())
    }

    /// Returns the model, waiting for a background load or loading it now.
    pub fn load(&mut self) -> Result<&WhisperContext> {
        if self.context.is_none() {
            let loaded = match self.loading.take() {
                Some(loading) => loading.recv().map_err(transcription)?,
                None => open(model_file(self.path.as_deref(), self.model)?),
            };
            self.context = Some(loaded?);
        }
        self.context
            .as_ref()
            .ok_or_else(|| FacadeError::Transcription("the speech model isn't loaded".into()))
    }

    pub fn transcribe(&mut self, audio: &Audio) -> Result<String> {
        if audio::is_too_short(audio) {
            return Ok(String::new());
        }
        let language = self.language.clone();
        let mut state = self.load()?.create_state().map_err(transcription)?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(language.as_deref());
        params.set_n_threads(threads());
        params.set_no_context(true);
        params.set_suppress_blank(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        state.full(params, &audio.samples).map_err(transcription)?;
        let segments = state
            .as_iter()
            .map(|segment| segment.to_str_lossy().map(std::borrow::Cow::into_owned))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(transcription)?;
        Ok(transcript::clean(segments.iter().map(String::as_str)))
    }
}

/// The model file, or an error telling the user where to put it.
fn model_file(path: Option<&Path>, model: WhisperModel) -> Result<&Path> {
    let path = path.ok_or_else(|| {
        FacadeError::Transcription("couldn't find the models directory (is HOME set?)".into())
    })?;
    if path.is_file() {
        Ok(path)
    } else {
        Err(FacadeError::Transcription(format!(
            "speech model not found at {}. Download it from {} and save it there",
            path.display(),
            models::download_url(model)
        )))
    }
}

fn open(path: &Path) -> Result<WhisperContext> {
    WhisperContext::new_with_params(path, WhisperContextParameters::default())
        .map_err(transcription)
}

fn threads() -> i32 {
    let available = thread::available_parallelism().map_or(1, usize::from);
    i32::try_from(available.min(MAX_THREADS)).unwrap_or(1)
}

fn transcription(error: impl std::fmt::Display) -> FacadeError {
    FacadeError::Transcription(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::platform::Os;
    use std::process::Command;

    /// Speaks a sentence with `say`, converts it to 16 kHz mono float WAV, and returns
    /// the samples. Uses only built-in macOS tools.
    fn spoken(sentence: &str) -> Vec<f32> {
        let dir = tempfile::tempdir().unwrap();
        let aiff = dir.path().join("speech.aiff");
        let wav = dir.path().join("speech.wav");
        assert!(Command::new("say")
            .arg("-o")
            .arg(&aiff)
            .arg(sentence)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("afconvert")
            .args(["-f", "WAVE", "-d", "LEF32@16000", "-c", "1"])
            .arg(&aiff)
            .arg(&wav)
            .status()
            .unwrap()
            .success());
        let bytes = std::fs::read(wav).unwrap();
        let data = bytes.windows(4).position(|w| w == b"data").unwrap() + 8;
        bytes[data..]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| f32::from_le_bytes(*bytes))
            .collect()
    }

    /// Needs the default model in place; run with `cargo test -- --ignored`.
    #[test]
    #[ignore = "needs the speech model downloaded"]
    fn transcribes_spoken_english() {
        let mut transcriber = Transcriber::new(&Config::defaults(Os::MacOs).dictation);
        transcriber.preload().unwrap();
        let audio = Audio {
            samples: spoken("Please send the report by Friday."),
            sample_rate: audio::WHISPER_SAMPLE_RATE,
        };

        let text = transcriber.transcribe(&audio).unwrap();

        assert_eq!(text, "Please send the report by Friday.");
    }

    #[test]
    fn short_audio_is_not_transcribed() {
        let mut transcriber = Transcriber::new(&Config::defaults(Os::MacOs).dictation);
        let audio = Audio {
            samples: vec![0.0; 100],
            sample_rate: audio::WHISPER_SAMPLE_RATE,
        };

        assert_eq!(transcriber.transcribe(&audio), Ok(String::new()));
    }
}
