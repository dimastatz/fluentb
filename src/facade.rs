//! The single boundary between fluentb's pure state manager and the outside world.
//!
//! The state manager decides *what* should happen; an implementation of [`Facade`]
//! is the only place that actually touches the OS, the microphone, Whisper, or the
//! rewrite engine. Keeping every side effect behind this trait lets the state manager
//! stay pure and lets tests drive it with a fake implementation.
//!
//! Each platform (macOS now, Windows and Linux later) provides its own implementation.

use crate::accelerator::Accelerator;
use crate::models::Kind;
use std::fmt;
use std::time::Duration;

/// The global hotkeys fluentb listens for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hotkey {
    /// Start or stop dictation into the focused text field.
    Dictate,
    /// Rewrite the currently selected text into clean, fluent text.
    Rewrite,
    /// Esc: throw away the dictation or rewrite in progress. Registered only while busy.
    Cancel,
}

/// A key combination bound to a [`Hotkey`], e.g. `Cmd+Shift+D`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyBinding {
    pub hotkey: Hotkey,
    pub accelerator: Accelerator,
}

/// A macOS-style privacy permission fluentb needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// To record dictation.
    Microphone,
    /// To read selections and type text into other apps.
    Accessibility,
}

/// Whether a model can be used yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelStatus {
    Ready,
    Downloading {
        percent: u8,
    },
    /// The download failed; it is retried the next time fluentb starts.
    Failed(String),
}

/// A timer the state manager can start. Starting one again replaces the pending one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timer {
    /// Stops a recording that has run for the configured maximum length.
    MaxRecording,
    /// Clears the error icon after a short while.
    ClearError,
}

/// A short sound played when recording starts or stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    Start,
    Stop,
}

/// Events the outside world delivers to the state manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    HotkeyPressed(Hotkey),
    /// A hotkey was let go, for push-to-talk.
    HotkeyReleased(Hotkey),
    /// A timer started with [`Facade::start_timer`] ran out.
    TimerFired(Timer),
    /// The microphone disappeared mid-recording (e.g. Bluetooth headphones disconnected).
    MicrophoneLost,
    /// The user chose "Quit" from the tray menu.
    QuitRequested,
    /// A model finished downloading, made progress, or failed.
    ModelChanged(Kind, ModelStatus),
}

/// Captured microphone audio: mono samples at `sample_rate` Hz.
#[derive(Debug, Clone, PartialEq)]
pub struct Audio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// How a rewrite reached the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Replacement {
    /// The original selection was replaced in place.
    Replaced,
    /// The focus or selection changed during the rewrite, so the result was copied to
    /// the clipboard instead of overwriting something else.
    Copied,
}

/// What the tray icon should show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayStatus {
    Idle,
    Recording,
    Transcribing,
    Rewriting,
    Error,
}

/// The subsystem that failed, with a human-readable reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FacadeError {
    Hotkeys(String),
    Microphone(String),
    Transcription(String),
    Rewrite(String),
    TextAccess(String),
    Tray(String),
    /// The user pressed Esc while the work was running; not reported to the user.
    Cancelled,
    /// A bug: a facade call panicked. The app reports it and keeps running.
    Internal(String),
}

impl FacadeError {
    /// The failing subsystem without the reason, safe to log: reasons may quote paths
    /// or engine messages, never user text, but logs record kinds only.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Hotkeys(_) => "hotkeys",
            Self::Microphone(_) => "microphone",
            Self::Transcription(_) => "transcription",
            Self::Rewrite(_) => "rewrite",
            Self::TextAccess(_) => "text access",
            Self::Tray(_) => "tray",
            Self::Cancelled => "cancelled",
            Self::Internal(_) => "internal",
        }
    }
}

impl fmt::Display for FacadeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hotkeys(reason) => write!(f, "hotkeys: {reason}"),
            Self::Microphone(reason) => write!(f, "microphone: {reason}"),
            Self::Transcription(reason) => write!(f, "transcription: {reason}"),
            Self::Rewrite(reason) => write!(f, "rewrite: {reason}"),
            Self::TextAccess(reason) => write!(f, "text access: {reason}"),
            Self::Tray(reason) => write!(f, "tray: {reason}"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Internal(reason) => write!(f, "internal error: {reason}"),
        }
    }
}

impl std::error::Error for FacadeError {}

pub type Result<T> = std::result::Result<T, FacadeError>;

/// Every side effect the state manager needs.
pub trait Facade {
    // Input

    /// Registers the global hotkeys, replacing any previously registered ones.
    fn register_hotkeys(&mut self, bindings: &[HotkeyBinding]) -> Result<()>;

    /// Blocks until the next input event arrives.
    fn next_event(&mut self) -> Result<InputEvent>;

    // Dictation

    /// Starts capturing audio from the default microphone.
    fn start_recording(&mut self) -> Result<()>;

    /// Stops capturing and returns everything recorded since [`Facade::start_recording`].
    fn stop_recording(&mut self) -> Result<Audio>;

    /// Transcribes audio to text with the local Whisper model. Audio never leaves the machine.
    fn transcribe(&mut self, audio: &Audio) -> Result<String>;

    // Rewrite

    /// Returns the text selected in the focused app, or `None` if nothing is selected.
    fn selected_text(&mut self) -> Result<Option<String>>;

    /// Rewrites text into clean, fluent, correctly styled text with a local model.
    /// User text never leaves the machine.
    fn rewrite(&mut self, text: &str) -> Result<String>;

    /// Replaces the selection with `replacement` if it still holds `original`;
    /// otherwise copies `replacement` to the clipboard.
    fn replace_selection(&mut self, original: &str, replacement: &str) -> Result<Replacement>;

    // Output

    /// Types text into the focused text field, replacing the current selection if there is one.
    fn insert_text(&mut self, text: &str) -> Result<()>;

    /// Updates the tray icon to reflect the current state.
    fn set_tray_status(&mut self, status: TrayStatus) -> Result<()>;

    /// Shows a short, non-blocking notification, e.g. to report an error.
    fn notify(&mut self, message: &str) -> Result<()>;

    /// Plays a short, non-blocking sound.
    fn play_cue(&mut self, cue: Cue) -> Result<()>;

    // Time

    /// Makes [`Facade::next_event`] deliver [`InputEvent::TimerFired`] after `after`,
    /// replacing a pending timer of the same kind.
    fn start_timer(&mut self, timer: Timer, after: Duration) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_names_the_failing_subsystem() {
        let cases = [
            (FacadeError::Hotkeys("taken".into()), "hotkeys: taken"),
            (
                FacadeError::Microphone("denied".into()),
                "microphone: denied",
            ),
            (
                FacadeError::Transcription("no model".into()),
                "transcription: no model",
            ),
            (FacadeError::Rewrite("timeout".into()), "rewrite: timeout"),
            (
                FacadeError::TextAccess("no focus".into()),
                "text access: no focus",
            ),
            (FacadeError::Tray("hidden".into()), "tray: hidden"),
            (FacadeError::Cancelled, "cancelled"),
            (
                FacadeError::Internal("panicked".into()),
                "internal error: panicked",
            ),
        ];

        cases.iter().for_each(|(error, expected)| {
            assert_eq!(error.to_string(), *expected);
            assert!(expected.starts_with(error.kind()), "{error:?}");
        });
    }

    #[test]
    fn error_works_as_std_error() {
        let error: Box<dyn std::error::Error> = Box::new(FacadeError::Tray("hidden".into()));

        assert_eq!(error.to_string(), "tray: hidden");
    }
}
