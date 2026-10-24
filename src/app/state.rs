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
