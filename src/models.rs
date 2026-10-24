//! Where models live on disk, where they come from, and how Whisper is configured.

use crate::config::{Choice, Language, ModelSize, RewriteModel, WhisperModel};
use crate::platform::Os;
use std::path::PathBuf;

const APP_DIR: &str = "fluentb";
/// Suffix of a download in progress; it is renamed once its hash checks out.
pub const PARTIAL_SUFFIX: &str = ".part";
/// Below this much memory, `auto` picks the light rewrite model.
const LIGHT_REWRITE_BELOW: u64 = 16 * 1024 * 1024 * 1024;
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

/// What kind of work a model does. Each kind has one active model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Speech,
    Rewrite,
}

impl Model {
    #[must_use]
    pub const fn kind(self) -> Kind {
        match self {
            Self::Whisper(_) => Kind::Speech,
            Self::Rewrite(_) => Kind::Rewrite,
        }
    }

    /// The name used in the config file and the menu, e.g. `base.en` or `qwen3-4b`.
    #[must_use]
    pub fn id(self) -> String {
        match self {
            Self::Whisper(model) => model.to_string(),
            Self::Rewrite(model) => model.to_string(),
        }
    }
}

/// A downloadable model file and how to verify it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub model: Model,
    pub size: u64,
    /// Lowercase hex SHA-256 of the whole file, as published by Hugging Face.
    pub sha256: &'static str,
}

const fn whisper(size: ModelSize, english_only: bool) -> Model {
    Model::Whisper(WhisperModel { size, english_only })
}

/// Every model fluentb can download, with the sizes and hashes published upstream.
pub const REGISTRY: [Entry; 12] = [
    Entry {
        model: whisper(ModelSize::Tiny, false),
        size: 77_691_713,
        sha256: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
    },
    Entry {
        model: whisper(ModelSize::Tiny, true),
        size: 77_704_715,
        sha256: "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f",
    },
    Entry {
        model: whisper(ModelSize::Base, false),
        size: 147_951_465,
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
    },
    Entry {
        model: whisper(ModelSize::Base, true),
        size: 147_964_211,
        sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002",
    },
    Entry {
        model: whisper(ModelSize::Small, false),
        size: 487_601_967,
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
    },
    Entry {
        model: whisper(ModelSize::Small, true),
        size: 487_614_201,
        sha256: "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d",
    },
    Entry {
        model: whisper(ModelSize::Medium, false),
        size: 1_533_763_059,
        sha256: "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208",
    },
    Entry {
        model: whisper(ModelSize::Medium, true),
        size: 1_533_774_781,
        sha256: "cc37e93478338ec7700281a7ac30a10128929eb8f427dda2e865faa8f6da4356",
    },
    Entry {
        model: whisper(ModelSize::LargeV3, false),
        size: 3_095_033_483,
        sha256: "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2",
    },
    Entry {
        model: Model::Rewrite(RewriteModel::Qwen3_4B),
        size: 2_497_280_256,
        sha256: "7485fe6f11af29433bc51cab58009521f205840f5b4ae3a32fa7f92e8534fdf5",
    },
    Entry {
        model: Model::Rewrite(RewriteModel::Gemma3_4B),
        size: 2_489_757_856,
        sha256: "882e8d2db44dc554fb0ea5077cb7e4bc49e7342a1f0da57901c0802ea21a0863",
    },
    Entry {
        model: Model::Rewrite(RewriteModel::Qwen3_1_7B),
        size: 1_107_409_472,
        sha256: "b139949c5bd74937ad8ed8c8cf3d9ffb1e99c866c823204dc42c0d91fa181897",
    },
];

/// The registry entry for `model`. Every model fluentb can configure has one.
#[must_use]
pub fn entry(model: Model) -> Option<&'static Entry> {
    REGISTRY.iter().find(|entry| entry.model == model)
}

/// The models of one kind, in menu order.
pub fn of_kind(kind: Kind) -> impl Iterator<Item = Model> {
    REGISTRY
        .iter()
        .map(|entry| entry.model)
        .filter(move |model| model.kind() == kind)
}

/// What fluentb knows about the machine when choosing models.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct System {
    /// The preferred language, e.g. `en-US` or `he`.
    pub language: Option<String>,
    pub memory_bytes: Option<u64>,
}

/// The Whisper model to use: `base.en` on English systems, `base` otherwise.
#[must_use]
pub fn choose_whisper(choice: Choice<WhisperModel>, system: &System) -> WhisperModel {
    match choice {
        Choice::Fixed(model) => model,
        Choice::Auto => WhisperModel {
            size: ModelSize::Base,
            english_only: system
                .language
                .as_deref()
                .is_none_or(|language| language.to_lowercase().starts_with("en")),
        },
    }
}

/// The rewrite model to use: the light one on machines with under 16 GB of memory.
#[must_use]
pub fn choose_rewrite(choice: Choice<RewriteModel>, system: &System) -> RewriteModel {
    match (choice, system.memory_bytes) {
        (Choice::Fixed(model), _) => model,
        (Choice::Auto, Some(memory)) if memory < LIGHT_REWRITE_BELOW => RewriteModel::Qwen3_1_7B,
        (Choice::Auto, _) => RewriteModel::Qwen3_4B,
    }
}

/// The models in `wanted` that aren't installed yet, in order, so speech comes
/// before the much larger rewrite model.
pub fn pending(wanted: &[Model], installed: impl Fn(Model) -> bool) -> Vec<Model> {
    wanted
        .iter()
        .copied()
        .filter(|model| !installed(*model))
        .collect()
}

/// Whether a file of `file_size` bytes is a complete copy of `model`. Hashes are
/// checked when downloading; at startup the size is enough.
#[must_use]
pub fn is_complete(model: Model, file_size: Option<u64>) -> bool {
    entry(model).is_some_and(|entry| file_size == Some(entry.size))
}

/// Total bytes to download for `models`.
#[must_use]
pub fn total_size(models: &[Model]) -> u64 {
    models
        .iter()
        .filter_map(|model| entry(*model))
        .map(|entry| entry.size)
        .sum()
}

/// The files in the models folder that belong to models not `in_use`, including
/// unfinished downloads. Files fluentb doesn't know about are never listed.
#[must_use]
pub fn unused_files(files: &[String], in_use: &[Model]) -> Vec<String> {
    let unused: Vec<String> = REGISTRY
        .iter()
        .map(|entry| entry.model)
        .filter(|model| !in_use.contains(model))
        .map(Model::file_name)
        .collect();
    files
        .iter()
        .filter(|file| {
            let name = file.strip_suffix(PARTIAL_SUFFIX).unwrap_or(file);
            unused.iter().any(|unused| unused == name)
        })
        .cloned()
        .collect()
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
    fn registry_covers_every_model_once_with_valid_hashes() {
        let whisper = ModelSize::ALL.into_iter().flat_map(|size| {
            [false, true]
                .into_iter()
                .filter(move |english| !(*english && size == ModelSize::LargeV3))
                .map(move |english_only| whisper(size, english_only))
        });
        let all: Vec<Model> = whisper
            .chain(RewriteModel::ALL.into_iter().map(Model::Rewrite))
            .collect();

        assert_eq!(all.len(), REGISTRY.len());
        all.iter().for_each(|model| {
            let entry = entry(*model).unwrap();
            assert_eq!(entry.sha256.len(), 64, "{model:?}");
            assert!(
                entry
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "{model:?}"
            );
            assert!(entry.size > 50_000_000, "{model:?}");
            assert!(entry
                .model
                .download_url()
                .starts_with("https://huggingface.co/"));
        });
        let ids: std::collections::HashSet<String> = all.iter().map(|m| m.id()).collect();
        assert_eq!(ids.len(), all.len(), "ids are unique");
    }

    #[test]
    fn lists_models_by_kind() {
        assert_eq!(of_kind(Kind::Speech).count(), 9);
        assert_eq!(
            of_kind(Kind::Rewrite).collect::<Vec<_>>(),
            RewriteModel::ALL.map(Model::Rewrite)
        );
        assert_eq!(BASE_EN_MODEL.kind(), Kind::Speech);
        assert_eq!(BASE_EN_MODEL.id(), "base.en");
        assert_eq!(Model::Rewrite(RewriteModel::Qwen3_4B).id(), "qwen3-4b");
    }

    #[test]
    fn auto_picks_models_for_the_machine() {
        let system = |language: Option<&str>, gigabytes: Option<u64>| System {
            language: language.map(str::to_owned),
            memory_bytes: gigabytes.map(|gb| gb * 1024 * 1024 * 1024),
        };
        let base = WhisperModel {
            size: ModelSize::Base,
            english_only: false,
        };

        assert_eq!(
            choose_whisper(Choice::Auto, &system(Some("en-US"), None)),
            BASE_EN
        );
        assert_eq!(
            choose_whisper(Choice::Auto, &system(Some("EN"), None)),
            BASE_EN
        );
        assert_eq!(
            choose_whisper(Choice::Auto, &system(Some("he-IL"), None)),
            base
        );
        assert_eq!(choose_whisper(Choice::Auto, &system(None, None)), BASE_EN);
        assert_eq!(
            choose_whisper(Choice::Fixed(LARGE), &system(Some("en"), None)),
            LARGE
        );

        assert_eq!(
            choose_rewrite(Choice::Auto, &system(None, Some(8))),
            RewriteModel::Qwen3_1_7B
        );
        assert_eq!(
            choose_rewrite(Choice::Auto, &system(None, Some(16))),
            RewriteModel::Qwen3_4B
        );
        assert_eq!(
            choose_rewrite(Choice::Auto, &system(None, None)),
            RewriteModel::Qwen3_4B
        );
        assert_eq!(
            choose_rewrite(
                Choice::Fixed(RewriteModel::Gemma3_4B),
                &system(None, Some(8))
            ),
            RewriteModel::Gemma3_4B
        );
    }

    #[test]
    fn finds_what_still_needs_downloading() {
        let rewrite = Model::Rewrite(RewriteModel::Qwen3_4B);
        let wanted = [BASE_EN_MODEL, rewrite];

        assert_eq!(pending(&wanted, |_| false), wanted);
        assert_eq!(pending(&wanted, |model| model == BASE_EN_MODEL), [rewrite]);
        assert!(pending(&wanted, |_| true).is_empty());
        assert_eq!(total_size(&wanted), 147_964_211 + 2_497_280_256);
    }

    #[test]
    fn a_file_is_complete_when_its_size_matches() {
        assert!(is_complete(BASE_EN_MODEL, Some(147_964_211)));
        assert!(!is_complete(BASE_EN_MODEL, Some(1_000)));
        assert!(!is_complete(BASE_EN_MODEL, None));
    }

    #[test]
    fn unused_files_are_known_models_not_in_use() {
        let files = [
            "ggml-base.en.bin",
            "ggml-small.bin",
            "Qwen3-4B-Q4_K_M.gguf.part",
            "gemma-3-4b-it-Q4_K_M.gguf",
            "my-notes.txt",
            ".DS_Store",
        ]
        .map(str::to_owned);
        let in_use = [BASE_EN_MODEL, Model::Rewrite(RewriteModel::Qwen3_1_7B)];

        assert_eq!(
            unused_files(&files, &in_use),
            [
                "ggml-small.bin",
                "Qwen3-4B-Q4_K_M.gguf.part",
                "gemma-3-4b-it-Q4_K_M.gguf"
            ]
        );
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
