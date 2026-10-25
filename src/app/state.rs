//! What the app is doing, and the events that move it along.

use crate::facade::{Audio, FacadeError, InputEvent, Replacement};

/// What fluentb is currently doing.
///
/// Errors are not a resting state: a failure is reported and the app returns to
/// [`State::Idle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    /// The microphone is capturing a dictation.
    Recording,
    /// Recording has stopped and the audio is being transcribed.
    Transcribing,
    /// The selection is being read or rewritten.
    Rewriting,
}

/// Everything the state machine reacts to: input from the user, and the results of
/// effects it asked for.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Input(InputEvent),
    RecordingStopped(Audio),
    Transcribed(String),
    SelectionRead(Option<String>),
    Rewritten { original: String, rewritten: String },
    SelectionReplaced(Replacement),
    Failed(FacadeError),
}

impl Event {
    /// The event's name without its payload (audio, transcripts, selections), safe
    /// to log.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Input(InputEvent::HotkeyPressed(_)) => "hotkey-pressed",
            Self::Input(InputEvent::HotkeyReleased(_)) => "hotkey-released",
            Self::Input(InputEvent::TimerFired(_)) => "timer-fired",
            Self::Input(InputEvent::MicrophoneLost) => "microphone-lost",
            Self::Input(InputEvent::QuitRequested) => "quit-requested",
            Self::Input(InputEvent::ModelChanged(..)) => "model-changed",
            Self::RecordingStopped(_) => "recording-stopped",
            Self::Transcribed(_) => "transcribed",
            Self::SelectionRead(_) => "selection-read",
            Self::Rewritten { .. } => "rewritten",
            Self::SelectionReplaced(_) => "selection-replaced",
            Self::Failed(_) => "failed",
        }
    }
}
