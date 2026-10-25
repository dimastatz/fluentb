//! Diagnostic logs: a daily rolling file in the platform's log folder.
//!
//! Only fluentb's own events are recorded, and those carry event kinds, states and
//! timings, never audio, transcripts, selections or rewrites (see `app::runtime`).
//! Third-party crates are filtered out, since they can't be held to that rule.

use crate::platform::Os;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{Level, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Layer;

/// Daily files kept before the oldest is deleted.
const KEEP_DAYS: usize = 7;

/// The log folder for `os`, given a way to read environment variables.
///
/// - macOS: `~/Library/Logs/fluentb`
/// - Windows: `%LOCALAPPDATA%\fluentb\logs`
/// - Linux: `$XDG_STATE_HOME/fluentb`, or `~/.local/state/fluentb`
pub fn dir_for(os: Os, env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let var = |name: &str| {
        env(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    match os {
        Os::MacOs => var("HOME").map(|home| home.join("Library").join("Logs").join("fluentb")),
        Os::Windows => var("LOCALAPPDATA").map(|dir| dir.join("fluentb").join("logs")),
        Os::Linux => var("XDG_STATE_HOME")
            .filter(|dir| dir.is_absolute())
            .or_else(|| var("HOME").map(|home| home.join(".local").join("state")))
            .map(|dir| dir.join("fluentb")),
    }
}

/// The log folder on this machine.
#[must_use]
pub fn default_dir() -> Option<PathBuf> {
    dir_for(Os::current(), |name| std::env::var(name).ok())
}

/// Starts logging to `dir` for the rest of the process. Keep the returned guard
/// alive: dropping it flushes and stops the background writer.
pub fn init(dir: &Path) -> Result<WorkerGuard, String> {
    let (subscriber, guard) = file_subscriber(dir)?;
    tracing::subscriber::set_global_default(subscriber).map_err(|error| error.to_string())?;
    Ok(guard)
}

/// A subscriber writing fluentb's own events to a daily file in `dir`.
fn file_subscriber(dir: &Path) -> Result<(impl Subscriber + Send + Sync, WorkerGuard), String> {
    fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("fluentb")
        .filename_suffix("log")
        .max_log_files(KEEP_DAYS)
        .build(dir)
        .map_err(|error| error.to_string())?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let layer = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_filter(own_events());
    Ok((tracing_subscriber::registry().with(layer), guard))
}

/// fluentb's own events at info level and above.
fn own_events() -> Targets {
    Targets::new().with_target(env!("CARGO_CRATE_NAME"), Level::INFO)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| vars.get(name).cloned()
    }

    #[test]
    fn dir_follows_each_platform_convention() {
        let cases = [
            (
                Os::MacOs,
                env(&[("HOME", "/Users/u")]),
                Some("/Users/u/Library/Logs/fluentb"),
            ),
            (
                Os::Windows,
                env(&[("LOCALAPPDATA", "/Local")]),
                Some("/Local/fluentb/logs"),
            ),
            (
                Os::Linux,
                env(&[("XDG_STATE_HOME", "/state"), ("HOME", "/home/u")]),
                Some("/state/fluentb"),
            ),
            (
                Os::Linux,
                env(&[("XDG_STATE_HOME", "rel"), ("HOME", "/home/u")]),
                Some("/home/u/.local/state/fluentb"),
            ),
            (Os::MacOs, env(&[]), None),
            (Os::Windows, env(&[("HOME", "/h")]), None),
        ];

        cases.into_iter().for_each(|(os, env, expected)| {
            assert_eq!(dir_for(os, env), expected.map(PathBuf::from), "{os:?}");
        });
        assert!(default_dir().is_none_or(|dir| dir.ends_with("fluentb")));
    }

    #[test]
    fn records_only_fluentbs_own_events() {
        let targets = own_events();

        assert!(targets.would_enable("fluentb::app::runtime", &Level::INFO));
        assert!(!targets.would_enable("fluentb::app::runtime", &Level::DEBUG));
        assert!(!targets.would_enable("llama_cpp_2", &Level::ERROR));
        assert!(!targets.would_enable("whisper_rs", &Level::INFO));
    }

    #[test]
    fn writes_to_a_daily_log_file() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("logs");

        let (subscriber, guard) = file_subscriber(&logs).unwrap();
        tracing::subscriber::with_default(subscriber, || {
            tracing::callsite::rebuild_interest_cache();
            tracing::info!(event = "hotkey-pressed", "transition");
        });
        drop(guard);

        let files: Vec<_> = fs::read_dir(&logs)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(files.len(), 1, "{files:?}");
        let name = files[0].file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("fluentb."), "{name}");
        assert_eq!(files[0].extension(), Some("log".as_ref()));
        assert!(fs::read_to_string(&files[0])
            .unwrap()
            .contains("hotkey-pressed"));
    }

    #[test]
    fn creating_the_folder_can_fail() {
        let file = tempfile::NamedTempFile::new().unwrap();

        assert!(file_subscriber(&file.path().join("logs")).is_err());
    }
}
