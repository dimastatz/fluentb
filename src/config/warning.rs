//! Problems found while loading the config, reported to the user at startup.

use std::fmt;
use std::path::PathBuf;

/// A config problem. fluentb keeps running with defaults for the affected settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigWarning {
    /// No home or config directory could be found in the environment.
    NoConfigDir,
    Unreadable {
        path: PathBuf,
        reason: String,
    },
    /// The commented default file couldn't be created on first run.
    NotCreated {
        path: PathBuf,
        reason: String,
    },
    Syntax(String),
    UnknownKey(String),
    InvalidValue {
        key: String,
        reason: String,
    },
    /// Dictate and Rewrite were given the same hotkey.
    HotkeyConflict,
}

impl fmt::Display for ConfigWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoConfigDir => write!(
                f,
                "Couldn't find a config directory, so default settings are used."
            ),
            Self::Unreadable { path, reason } => write!(
                f,
                "Couldn't read {}: {reason}. Default settings are used.",
                path.display()
            ),
            Self::NotCreated { path, reason } => {
                write!(f, "Couldn't create {}: {reason}.", path.display())
            }
            Self::Syntax(reason) => write!(
                f,
                "The config file has a syntax error, so default settings are used: {reason}"
            ),
            Self::UnknownKey(key) => write!(f, "Unknown config key `{key}` was ignored."),
            Self::InvalidValue { key, reason } => {
                write!(f, "Invalid `{key}`: {reason}. The default is used.")
            }
            Self::HotkeyConflict => write!(
                f,
                "Dictate and Rewrite can't share a hotkey, so the default hotkeys are used."
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_name_the_problem() {
        let path = PathBuf::from("/c/config.toml");
        let cases = [
            (
                ConfigWarning::NoConfigDir,
                "Couldn't find a config directory, so default settings are used.",
            ),
            (
                ConfigWarning::Unreadable {
                    path: path.clone(),
                    reason: "denied".into(),
                },
                "Couldn't read /c/config.toml: denied. Default settings are used.",
            ),
            (
                ConfigWarning::NotCreated {
                    path,
                    reason: "read-only".into(),
                },
                "Couldn't create /c/config.toml: read-only.",
            ),
            (
                ConfigWarning::Syntax("line 1".into()),
                "The config file has a syntax error, so default settings are used: line 1",
            ),
            (
                ConfigWarning::UnknownKey("hotkeys.x".into()),
                "Unknown config key `hotkeys.x` was ignored.",
            ),
            (
                ConfigWarning::InvalidValue {
                    key: "rewrite.style".into(),
                    reason: "bad".into(),
                },
                "Invalid `rewrite.style`: bad. The default is used.",
            ),
            (
                ConfigWarning::HotkeyConflict,
                "Dictate and Rewrite can't share a hotkey, so the default hotkeys are used.",
            ),
        ];

        cases
            .into_iter()
            .for_each(|(warning, expected)| assert_eq!(warning.to_string(), expected));
    }
}
