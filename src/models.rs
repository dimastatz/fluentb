//! Where models live on disk, where they come from, and how Whisper is configured.

use crate::config::{Language, RewriteModel, WhisperModel};
use crate::platform::Os;
use std::path::PathBuf;

const APP_DIR: &str = "fluentb";
const MODELS_DIR: &str = "models";

/// A model fluentb runs locally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    /// Speech to text, with whisper.cpp.
    Whisper(WhisperModel),
    /// Text rewriting, with llama.cpp.
    Rewrite(RewriteModel),
}

impl Model {
    /// The file name, as published upstream (e.g. `ggml-base.en.bin`).
    #[must_use]
    pub fn file_name(self) -> String {
        match self {
            Self::Whisper(model) => format!("ggml-{model}.bin"),
            Self::Rewrite(model) => rewrite_source(model).1.to_owned(),
        }
    }

    /// Where to download the model from: the publisher's own Hugging Face repository,
    /// or a well-known conversion where the publisher has no GGUF of that size.
    #[must_use]
    pub fn download_url(self) -> String {
        let repository = match self {
            Self::Whisper(_) => "ggerganov/whisper.cpp",
            Self::Rewrite(model) => rewrite_source(model).0,
        };
        format!(
            "https://huggingface.co/{repository}/resolve/main/{}",
            self.file_name()
        )
    }
}

/// The Hugging Face repository and file of each rewrite model (`Q4_K_M` quantization).
const fn rewrite_source(model: RewriteModel) -> (&'static str, &'static str) {
    match model {
        RewriteModel::Qwen3_4B => ("Qwen/Qwen3-4B-GGUF", "Qwen3-4B-Q4_K_M.gguf"),
        RewriteModel::Gemma3_4B => ("ggml-org/gemma-3-4b-it-GGUF", "gemma-3-4b-it-Q4_K_M.gguf"),
        RewriteModel::Qwen3_1_7B => ("unsloth/Qwen3-1.7B-GGUF", "Qwen3-1.7B-Q4_K_M.gguf"),
    }
}

/// The models directory for `os`, given a way to read environment variables.
///
/// - macOS: `~/Library/Application Support/fluentb/models/`
/// - Windows: `%LOCALAPPDATA%\fluentb\models\`
/// - Linux: `$XDG_DATA_HOME/fluentb/models/`, or `~/.local/share/fluentb/models/`
pub fn dir_for(os: Os, env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let var = |name: &str| {
        env(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    let base = match os {
        Os::MacOs => var("HOME").map(|home| home.join("Library").join("Application Support")),
        Os::Windows => var("LOCALAPPDATA"),
        Os::Linux => var("XDG_DATA_HOME")
            .filter(|dir| dir.is_absolute())
            .or_else(|| var("HOME").map(|home| home.join(".local").join("share"))),
    };
    base.map(|dir| dir.join(APP_DIR).join(MODELS_DIR))
}

/// The models directory on this machine.
#[must_use]
pub fn default_dir() -> Option<PathBuf> {
    dir_for(Os::current(), |name| std::env::var(name).ok())
}

/// The model file path for `os`, given a way to read environment variables.
///
/// - macOS: `~/Library/Application Support/fluentb/models/`
/// - Windows: `%LOCALAPPDATA%\fluentb\models\`
/// - Linux: `$XDG_DATA_HOME/fluentb/models/`, or `~/.local/share/fluentb/models/`
pub fn path_for(os: Os, env: impl Fn(&str) -> Option<String>, model: Model) -> Option<PathBuf> {
    dir_for(os, env).map(|dir| dir.join(model.file_name()))
}

/// The model file path on this machine.
#[must_use]
pub fn default_path(model: Model) -> Option<PathBuf> {
    path_for(Os::current(), |name| std::env::var(name).ok(), model)
}

/// The language code to pass to Whisper, or `None` to auto-detect.
///
/// English-only models always get `en`, since they can't transcribe anything else.
#[must_use]
pub fn whisper_language(model: WhisperModel, language: &Language) -> Option<&str> {
    match language {
        _ if model.english_only => Some("en"),
        Language::Auto => None,
        Language::Code(code) => Some(code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModelSize;

    const BASE_EN: WhisperModel = WhisperModel {
        size: ModelSize::Base,
        english_only: true,
    };
    const LARGE: WhisperModel = WhisperModel {
        size: ModelSize::LargeV3,
        english_only: false,
    };
    const BASE_EN_MODEL: Model = Model::Whisper(BASE_EN);

    fn env(vars: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn names_files_like_their_publishers() {
        assert_eq!(BASE_EN_MODEL.file_name(), "ggml-base.en.bin");
        let cases = [
            (
                Model::Whisper(LARGE),
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin",
            ),
            (
                Model::Rewrite(RewriteModel::Qwen3_4B),
                "https://huggingface.co/Qwen/Qwen3-4B-GGUF/resolve/main/Qwen3-4B-Q4_K_M.gguf",
            ),
            (
                Model::Rewrite(RewriteModel::Gemma3_4B),
                "https://huggingface.co/ggml-org/gemma-3-4b-it-GGUF/resolve/main/gemma-3-4b-it-Q4_K_M.gguf",
            ),
            (
                Model::Rewrite(RewriteModel::Qwen3_1_7B),
                "https://huggingface.co/unsloth/Qwen3-1.7B-GGUF/resolve/main/Qwen3-1.7B-Q4_K_M.gguf",
            ),
        ];

        cases
            .iter()
            .for_each(|(model, url)| assert_eq!(model.download_url(), *url, "{model:?}"));
    }

    #[test]
    fn path_follows_each_platform_convention() {
        let cases = [
            (
                Os::MacOs,
                Some("/Users/u/Library/Application Support/fluentb/models/ggml-base.en.bin"),
                env(&[("HOME", "/Users/u")]),
            ),
            (
                Os::Windows,
                Some("/Local/fluentb/models/ggml-base.en.bin"),
                env(&[("LOCALAPPDATA", "/Local")]),
            ),
            (
                Os::Linux,
                Some("/data/fluentb/models/ggml-base.en.bin"),
                env(&[("XDG_DATA_HOME", "/data"), ("HOME", "/home/u")]),
            ),
            (
                Os::Linux,
                Some("/home/u/.local/share/fluentb/models/ggml-base.en.bin"),
                env(&[("XDG_DATA_HOME", "rel"), ("HOME", "/home/u")]),
            ),
            (Os::MacOs, None, env(&[("HOME", "")])),
        ];

        cases.into_iter().for_each(|(os, expected, env)| {
            assert_eq!(
                path_for(os, env, BASE_EN_MODEL),
                expected.map(PathBuf::from),
                "{os:?}"
            );
        });
    }

    #[test]
    fn default_path_ends_in_the_model_file() {
        assert!(default_path(BASE_EN_MODEL)
            .is_none_or(|path| path.ends_with("models/ggml-base.en.bin")));
        assert!(default_dir().is_none_or(|dir| dir.ends_with("fluentb/models")));
    }

    #[test]
    fn chooses_the_whisper_language() {
        assert_eq!(whisper_language(BASE_EN, &Language::Auto), Some("en"));
        assert_eq!(
            whisper_language(BASE_EN, &Language::Code("he".into())),
            Some("en")
        );
        assert_eq!(whisper_language(LARGE, &Language::Auto), None);
        assert_eq!(
            whisper_language(LARGE, &Language::Code("he".into())),
            Some("he")
        );
    }
}
