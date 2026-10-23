//! User settings: hotkeys, transcription, and rewrite style.
//!
//! [`parse`] turns TOML text into a [`Config`] without any I/O; [`load`] is the only
//! function that touches the file system. Both always return a usable `Config`:
//! anything invalid falls back to its default and produces a [`ConfigWarning`].

mod load;
mod parse;
mod values;
mod warning;

pub use load::{default_path, load, path_for, DEFAULT_FILE};
pub use parse::parse;
pub use values::{DictationMode, Language, ModelSize, Style, WhisperModel};
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
    pub model: WhisperModel,
    pub language: Language,
    pub mode: DictationMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewrite {
    pub style: Style,
}

impl Config {
    /// The settings used when the config file is missing or a value is invalid.
    #[must_use]
    pub fn defaults(os: Os) -> Self {
        Self {
            hotkeys: Hotkeys::defaults(os),
            dictation: Dictation {
                model: WhisperModel {
                    size: ModelSize::Base,
                    english_only: true,
                },
                language: Language::Auto,
                mode: DictationMode::Toggle,
            },
            rewrite: Rewrite {
                style: Style::Neutral,
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
