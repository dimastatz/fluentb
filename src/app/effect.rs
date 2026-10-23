//! Side effects the state machine asks for, described as data.

use crate::facade::{Audio, TrayStatus};

/// One side effect for the runtime to carry out.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    StartRecording,
    StopRecording,
    /// Stops the microphone and throws the audio away, ignoring any error.
    DiscardRecording,
    Transcribe(Audio),
    ReadSelection,
    Rewrite(String),
    InsertText(String),
    /// Best effort: a tray failure is not reported back.
    SetTray(TrayStatus),
    /// Best effort: a notification failure is not reported back.
    Notify(String),
    Exit,
}
