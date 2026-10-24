//! The non-hotkey setting values and how they are written in the config file.

use std::fmt;
use std::str::FromStr;

/// A Whisper model size, as published by whisper.cpp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSize {
    Tiny,
    Base,
    Small,
    Medium,
    LargeV3,
}

/// Which Whisper model transcribes speech, e.g. `base.en`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhisperModel {
    pub size: ModelSize,
    /// The `.en` variant: English only, faster and more accurate for English.
    pub english_only: bool,
}

/// The spoken language Whisper should expect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Language {
    /// Let Whisper detect the language.
    Auto,
    /// A two-letter ISO 639-1 code such as `en` or `he`.
    Code(String),
}

/// How the Dictate hotkey controls recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictationMode {
    /// Press once to start, again to stop.
    Toggle,
    /// Hold to record, release to stop (implemented in M7).
    PushToTalk,
}

/// The local language model that rewrites text. All run on-device with llama.cpp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewriteModel {
    /// Qwen3 4B: the default. Apache 2.0.
    Qwen3_4B,
    /// Gemma 3 4B: strong multilingual rewriting. Gemma Terms of Use.
    Gemma3_4B,
    /// Qwen3 1.7B: the light option for Macs with less memory. Apache 2.0.
    Qwen3_1_7B,
}

/// The tone the rewrite engine aims for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Neutral,
    Casual,
    Formal,
    Concise,
}

impl ModelSize {
    const ALL: [Self; 5] = [
        Self::Tiny,
        Self::Base,
        Self::Small,
        Self::Medium,
        Self::LargeV3,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::Tiny => "tiny",
            Self::Base => "base",
            Self::Small => "small",
            Self::Medium => "medium",
            Self::LargeV3 => "large-v3",
        }
    }
}

impl RewriteModel {
    pub const ALL: [Self; 3] = [Self::Qwen3_4B, Self::Gemma3_4B, Self::Qwen3_1_7B];

    /// The name used in the config file.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Qwen3_4B => "qwen3-4b",
            Self::Gemma3_4B => "gemma3-4b",
            Self::Qwen3_1_7B => "qwen3-1.7b",
        }
    }
}

impl fmt::Display for RewriteModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

impl FromStr for RewriteModel {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|model| model.id() == text)
            .ok_or_else(|| {
                format!("unknown rewrite model \"{text}\" (expected qwen3-4b, gemma3-4b, or qwen3-1.7b)")
            })
    }
}

impl fmt::Display for WhisperModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let suffix = if self.english_only { ".en" } else { "" };
        write!(f, "{}{suffix}", self.size.name())
    }
}

impl FromStr for WhisperModel {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        let (name, english_only) = text
            .strip_suffix(".en")
            .map_or((text, false), |name| (name, true));
        ModelSize::ALL
            .into_iter()
            .find(|size| size.name() == name)
            // whisper.cpp publishes no English-only large-v3.
            .filter(|size| !(english_only && *size == ModelSize::LargeV3))
            .map(|size| Self { size, english_only })
            .ok_or_else(|| {
                format!(
                    "unknown model \"{text}\" (expected tiny, base, small, or medium, \
                     optionally with .en, or large-v3)"
                )
            })
    }
}

impl FromStr for Language {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        match text {
            "auto" => Ok(Self::Auto),
            code if code.len() == 2 && code.bytes().all(|b| b.is_ascii_lowercase()) => {
                Ok(Self::Code(code.into()))
            }
            _ => Err(format!(
                "unknown language \"{text}\" (expected \"auto\" or a two-letter code such as \"en\")"
            )),
        }
    }
}

impl FromStr for DictationMode {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        match text {
            "toggle" => Ok(Self::Toggle),
            "push-to-talk" => Ok(Self::PushToTalk),
            _ => Err(format!(
                "unknown mode \"{text}\" (expected toggle or push-to-talk)"
            )),
        }
    }
}

impl FromStr for Style {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        match text {
            "neutral" => Ok(Self::Neutral),
            "casual" => Ok(Self::Casual),
            "formal" => Ok(Self::Formal),
            "concise" => Ok(Self::Concise),
            _ => Err(format!(
                "unknown style \"{text}\" (expected neutral, casual, formal, or concise)"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whisper_models_round_trip() {
        [
            "tiny",
            "tiny.en",
            "base",
            "base.en",
            "small",
            "small.en",
            "medium",
            "medium.en",
            "large-v3",
        ]
        .into_iter()
        .for_each(|name| {
            let model: WhisperModel = name.parse().unwrap();
            assert_eq!(model.to_string(), name);
        });
    }

    #[test]
    fn rejects_unknown_whisper_models() {
        ["huge", "large-v3.en", "base.EN", "", ".en"]
            .into_iter()
            .for_each(|name| {
                let error = name.parse::<WhisperModel>().unwrap_err();
                assert!(
                    error.starts_with(&format!("unknown model \"{name}\"")),
                    "{error}"
                );
            });
    }

    #[test]
    fn rewrite_models_round_trip() {
        RewriteModel::ALL.into_iter().for_each(|model| {
            assert_eq!(model.to_string().parse(), Ok(model));
        });
        assert!("qwen2.5-3b"
            .parse::<RewriteModel>()
            .unwrap_err()
            .starts_with("unknown rewrite model \"qwen2.5-3b\""));
    }

    #[test]
    fn parses_languages() {
        assert_eq!("auto".parse(), Ok(Language::Auto));
        assert_eq!("he".parse(), Ok(Language::Code("he".into())));
        ["EN", "eng", "e", "", "e1"].into_iter().for_each(|text| {
            assert!(text.parse::<Language>().is_err(), "{text:?}");
        });
    }

    #[test]
    fn parses_modes_and_styles() {
        assert_eq!("toggle".parse(), Ok(DictationMode::Toggle));
        assert_eq!("push-to-talk".parse(), Ok(DictationMode::PushToTalk));
        assert!("hold".parse::<DictationMode>().is_err());

        let styles = [
            ("neutral", Style::Neutral),
            ("casual", Style::Casual),
            ("formal", Style::Formal),
            ("concise", Style::Concise),
        ];
        styles
            .into_iter()
            .for_each(|(text, style)| assert_eq!(text.parse(), Ok(style)));
        assert!("pirate".parse::<Style>().is_err());
    }
}
