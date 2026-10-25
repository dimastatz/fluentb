//! User settings: hotkeys, transcription, and rewrite style.
//!
//! [`parse`] turns TOML text into a [`Config`] without any I/O; [`load`] is the only
//! function that touches the file system. Both always return a usable `Config`:
//! anything invalid falls back to its default and produces a [`ConfigWarning`].

mod edit;
mod load;
mod parse;
mod values;
mod warning;

pub use edit::set_string;
pub use load::{default_path, load, path_for, DEFAULT_FILE};
pub use parse::parse;
pub use values::{Choice, DictationMode, Language, ModelSize, RewriteModel, Style, WhisperModel};
pub use warning::ConfigWarning;

use crate::accelerator::{Accelerator, Key, Modifier};
use crate::facade::{Hotkey, HotkeyBinding};
use crate::platform::Os;

/// Every user-configurable setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub hotkeys: Hotkeys,
    pub dictation: Dictation,
    pub rewrite: Rewrite,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotkeys {
    pub dictate: Accelerator,
    pub rewrite: Accelerator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dictation {
    pub model: Choice<WhisperModel>,
    pub language: Language,
    pub mode: DictationMode,
    /// Play a short sound when recording starts and stops.
    pub sounds: bool,
    /// Recording stops on its own after this many seconds.
    pub max_recording_seconds: u32,
}

impl Dictation {
    pub const DEFAULT_MAX_RECORDING_SECONDS: u32 = 120;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rewrite {
    pub style: Style,
    pub model: Choice<RewriteModel>,
    /// Keep the model in memory instead of unloading it when idle.
    pub keep_loaded: bool,
    /// Minutes without a rewrite before the model is unloaded to free memory.
    pub unload_after_minutes: u32,
    /// Longer selections are refused rather than rewritten.
    pub max_characters: usize,
}

impl Rewrite {
    pub const DEFAULT_UNLOAD_AFTER_MINUTES: u32 = 10;
    pub const DEFAULT_MAX_CHARACTERS: usize = 4_000;
}

impl Config {
    /// The settings used when the config file is missing or a value is invalid.
    #[must_use]
    pub fn defaults(os: Os) -> Self {
        Self {
            hotkeys: Hotkeys::defaults(os),
            dictation: Dictation {
                model: Choice::Auto,
                language: Language::Auto,
                mode: DictationMode::Toggle,
                sounds: true,
                max_recording_seconds: Dictation::DEFAULT_MAX_RECORDING_SECONDS,
            },
            rewrite: Rewrite {
                style: Style::Neutral,
                model: Choice::Auto,
                keep_loaded: false,
                unload_after_minutes: Rewrite::DEFAULT_UNLOAD_AFTER_MINUTES,
                max_characters: Rewrite::DEFAULT_MAX_CHARACTERS,
            },
        }
    }

    /// The hotkeys to register with the platform.
    #[must_use]
    pub fn bindings(&self) -> Vec<HotkeyBinding> {
        vec![
            HotkeyBinding {
                hotkey: Hotkey::Dictate,
                accelerator: self.hotkeys.dictate.clone(),
            },
            HotkeyBinding {
                hotkey: Hotkey::Rewrite,
                accelerator: self.hotkeys.rewrite.clone(),
            },
        ]
    }
}

impl Hotkeys {
    /// `Cmd+Shift+D` and `Cmd+Shift+R`, where Cmd is Ctrl outside macOS.
    #[must_use]
    pub fn defaults(os: Os) -> Self {
        let primary = match os {
            Os::MacOs => Modifier::Meta,
            Os::Windows | Os::Linux => Modifier::Ctrl,
        };
        let with = |key| Accelerator::new([primary, Modifier::Shift], Key::Char(key));
        Self {
            dictate: with('D'),
            rewrite: with('R'),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accelerator;

    #[test]
    fn default_hotkeys_match_the_documented_strings() {
        [Os::MacOs, Os::Windows, Os::Linux]
            .into_iter()
            .for_each(|os| {
                let hotkeys = Hotkeys::defaults(os);
                assert_eq!(accelerator::parse("Cmd+Shift+D", os), Ok(hotkeys.dictate));
                assert_eq!(accelerator::parse("Cmd+Shift+R", os), Ok(hotkeys.rewrite));
            });
    }

    #[test]
    fn bindings_cover_both_hotkeys() {
        let config = Config::defaults(Os::MacOs);

        assert_eq!(
            config.bindings(),
            [
                HotkeyBinding {
                    hotkey: Hotkey::Dictate,
                    accelerator: config.hotkeys.dictate.clone(),
                },
                HotkeyBinding {
                    hotkey: Hotkey::Rewrite,
                    accelerator: config.hotkeys.rewrite.clone(),
                },
            ]
        );
    }
}
