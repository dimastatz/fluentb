//! Where the config file lives, and reading it from disk.

use super::{parse, Config, ConfigWarning};
use crate::platform::Os;
use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

/// Written on first run so users can discover every option.
pub const DEFAULT_FILE: &str = r#"# fluentb settings. Delete a line to go back to its default.
# Changes take effect the next time fluentb starts.
version = 1

[hotkeys]
# Modifiers: Cmd, Ctrl, Alt (or Option), Shift, Super (or Win).
# Cmd means Command on macOS and Ctrl on Windows and Linux.
dictate = "Cmd+Shift+D"
rewrite = "Cmd+Shift+R"

[dictation]
# auto (base.en on English systems, base otherwise), tiny, base, small, medium
# (add .en for English only), or large-v3.
model = "auto"
# "auto" or a two-letter language code such as "en", "he", or "de".
language = "auto"
# toggle: press to start and again to stop. push-to-talk: hold while speaking.
mode = "toggle"
# Play a short sound when recording starts and stops.
sounds = true
# Recording stops on its own after this many seconds.
max_recording_seconds = 120

[rewrite]
# neutral, casual, formal, or concise.
style = "neutral"
# The local model that rewrites text: auto (qwen3-4b, or qwen3-1.7b on Macs with
# less than 16 GB of memory), qwen3-4b, gemma3-4b, or qwen3-1.7b.
model = "auto"
# The model is unloaded after this many idle minutes, to free memory.
unload_after_minutes = 10
# Set to true to keep the model in memory for instant rewrites.
keep_loaded = false
# Longer selections are not rewritten.
max_characters = 4000
"#;

const APP_DIR: &str = "fluentb";
const FILE_NAME: &str = "config.toml";

/// The config file path for `os`, given a way to read environment variables.
///
/// - macOS: `~/Library/Application Support/fluentb/config.toml`
/// - Windows: `%APPDATA%\fluentb\config.toml`
/// - Linux: `$XDG_CONFIG_HOME/fluentb/config.toml`, or `~/.config/fluentb/config.toml`
pub fn path_for(os: Os, env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let var = |name: &str| {
        env(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    let base = match os {
        Os::MacOs => var("HOME").map(|home| home.join("Library").join("Application Support")),
        Os::Windows => var("APPDATA"),
        Os::Linux => var("XDG_CONFIG_HOME")
            .filter(|dir| dir.is_absolute())
            .or_else(|| var("HOME").map(|home| home.join(".config"))),
    };
    base.map(|dir| dir.join(APP_DIR).join(FILE_NAME))
}

/// The config file path on this machine.
#[must_use]
pub fn default_path() -> Option<PathBuf> {
    path_for(Os::current(), |name| std::env::var(name).ok())
}

/// Loads the config at `path`, or the defaults if there is no path.
///
/// A missing file is normal on first run: the defaults are used and a commented
/// default file is created for the user to edit.
#[must_use]
pub fn load(path: Option<&Path>, os: Os) -> (Config, Vec<ConfigWarning>) {
    let Some(path) = path else {
        return (Config::defaults(os), vec![ConfigWarning::NoConfigDir]);
    };
    match fs::read_to_string(path) {
        Ok(text) => parse(&text, os),
        Err(error) if error.kind() == ErrorKind::NotFound => (
            Config::defaults(os),
            create_default(path)
                .err()
                .map(|error| ConfigWarning::NotCreated {
                    path: path.to_owned(),
                    reason: error.to_string(),
                })
                .into_iter()
                .collect(),
        ),
        Err(error) => (
            Config::defaults(os),
            vec![ConfigWarning::Unreadable {
                path: path.to_owned(),
                reason: error.to_string(),
            }],
        ),
    }
}

/// Writes [`DEFAULT_FILE`], never overwriting a file that appeared in the meantime.
fn create_default(path: &Path) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(DEFAULT_FILE.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Style;
    use std::collections::HashMap;

    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| vars.get(name).cloned()
    }

    #[test]
    fn path_follows_each_platform_convention() {
        let cases = [
            (
                Os::MacOs,
                env(&[("HOME", "/Users/u")]),
                Some("/Users/u/Library/Application Support/fluentb/config.toml"),
            ),
            (
                Os::Windows,
                env(&[("APPDATA", "/Roaming")]),
                Some("/Roaming/fluentb/config.toml"),
            ),
            (
                Os::Linux,
                env(&[("XDG_CONFIG_HOME", "/xdg"), ("HOME", "/home/u")]),
                Some("/xdg/fluentb/config.toml"),
            ),
            (
                Os::Linux,
                env(&[("XDG_CONFIG_HOME", "relative"), ("HOME", "/home/u")]),
                Some("/home/u/.config/fluentb/config.toml"),
            ),
            (
                Os::Linux,
                env(&[("XDG_CONFIG_HOME", ""), ("HOME", "/home/u")]),
                Some("/home/u/.config/fluentb/config.toml"),
            ),
            (Os::MacOs, env(&[]), None),
            (Os::Windows, env(&[("HOME", "/h")]), None),
        ];

        cases.into_iter().for_each(|(os, env, expected)| {
            assert_eq!(path_for(os, env), expected.map(PathBuf::from), "{os:?}");
        });
    }

    #[test]
    fn default_path_ends_in_the_app_config_file() {
        let path = default_path();

        assert!(path.is_none_or(|path| path.ends_with("fluentb/config.toml")));
    }

    #[test]
    fn missing_file_gives_defaults_and_creates_the_default_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.toml");

        let loaded = load(Some(&path), Os::MacOs);

        assert_eq!(loaded, (Config::defaults(Os::MacOs), Vec::new()));
        assert_eq!(fs::read_to_string(&path).unwrap(), DEFAULT_FILE);
    }

    #[test]
    fn reads_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "[rewrite]\nstyle = \"casual\"\n").unwrap();

        let (config, warnings) = load(Some(&path), Os::Linux);

        assert_eq!(config.rewrite.style, Style::Casual);
        assert_eq!(warnings, []);
    }

    #[test]
    fn unreadable_file_gives_defaults_and_a_warning() {
        let dir = tempfile::tempdir().unwrap();

        let (config, warnings) = load(Some(dir.path()), Os::MacOs);

        assert_eq!(config, Config::defaults(Os::MacOs));
        assert!(matches!(
            warnings.as_slice(),
            [ConfigWarning::Unreadable { path, .. }] if path == dir.path()
        ));
    }

    #[cfg(unix)]
    #[test]
    fn uncreatable_default_file_is_reported() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let read_only = dir.path().join("read-only");
        fs::create_dir(&read_only).unwrap();
        fs::set_permissions(&read_only, fs::Permissions::from_mode(0o555)).unwrap();
        let path = read_only.join("config.toml");

        let (config, warnings) = load(Some(&path), Os::MacOs);

        fs::set_permissions(&read_only, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(config, Config::defaults(Os::MacOs));
        assert!(matches!(
            warnings.as_slice(),
            [ConfigWarning::NotCreated { path: reported, .. }] if *reported == path
        ));
    }

    #[test]
    fn no_config_dir_gives_defaults_and_a_warning() {
        assert_eq!(
            load(None, Os::Windows),
            (
                Config::defaults(Os::Windows),
                vec![ConfigWarning::NoConfigDir]
            )
        );
    }
}
