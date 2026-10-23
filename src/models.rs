//! Where Whisper models live on disk and how they are configured.
//!
//! M3 loads a model the user placed there; M6 will download it automatically.

use crate::config::{Language, WhisperModel};
use crate::platform::Os;
use std::path::PathBuf;

const APP_DIR: &str = "fluentb";
const MODELS_DIR: &str = "models";

/// The model file name used by whisper.cpp releases, e.g. `ggml-base.en.bin`.
#[must_use]
pub fn file_name(model: WhisperModel) -> String {
    format!("ggml-{model}.bin")
}

/// Where to download `model` from (whisper.cpp's official GGML builds).
#[must_use]
pub fn download_url(model: WhisperModel) -> String {
    format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
        file_name(model)
    )
}

/// The model file path for `os`, given a way to read environment variables.
///
/// - macOS: `~/Library/Application Support/fluentb/models/`
/// - Windows: `%LOCALAPPDATA%\fluentb\models\`
/// - Linux: `$XDG_DATA_HOME/fluentb/models/`, or `~/.local/share/fluentb/models/`
pub fn path_for(
    os: Os,
    env: impl Fn(&str) -> Option<String>,
    model: WhisperModel,
) -> Option<PathBuf> {
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
    base.map(|dir| dir.join(APP_DIR).join(MODELS_DIR).join(file_name(model)))
}

/// The model file path on this machine.
#[must_use]
pub fn default_path(model: WhisperModel) -> Option<PathBuf> {
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

    fn env(vars: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn names_files_like_whisper_cpp() {
        assert_eq!(file_name(BASE_EN), "ggml-base.en.bin");
        assert_eq!(
            download_url(LARGE),
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin"
        );
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
                path_for(os, env, BASE_EN),
                expected.map(PathBuf::from),
                "{os:?}"
            );
        });
    }

    #[test]
    fn default_path_ends_in_the_model_file() {
        assert!(default_path(BASE_EN).is_none_or(|path| path.ends_with("models/ggml-base.en.bin")));
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
