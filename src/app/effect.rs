//! Side effects the state machine asks for, described as data.

use crate::facade::{Audio, Cue, Timer, TrayStatus};

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
    /// Puts a rewrite in place of the selection it was made from.
    ReplaceSelection {
        original: String,
        replacement: String,
    },
    InsertText(String),
    /// Best effort: a tray failure is not reported back.
    SetTray(TrayStatus),
    /// Best effort: a notification failure is not reported back.
    Notify(String),
    /// Best effort, and only if sounds are turned on.
    PlayCue(Cue),
    /// Best effort; the runtime picks the duration from the settings.
    StartTimer(Timer),
    /// Registers Esc as the cancel hotkey while busy, and releases it when idle, so
    /// it never steals Esc from other apps. Best effort.
    ListenForCancel(bool),
    Exit,
}

impl Effect {
    /// The effect's name without its payload, safe to log.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::StartRecording => "start-recording",
            Self::StopRecording => "stop-recording",
            Self::DiscardRecording => "discard-recording",
            Self::Transcribe(_) => "transcribe",
            Self::ReadSelection => "read-selection",
            Self::Rewrite(_) => "rewrite",
            Self::ReplaceSelection { .. } => "replace-selection",
            Self::InsertText(_) => "insert-text",
            Self::SetTray(_) => "set-tray",
            Self::Notify(_) => "notify",
            Self::PlayCue(_) => "play-cue",
            Self::StartTimer(_) => "start-timer",
            Self::ListenForCancel(_) => "listen-for-cancel",
            Self::Exit => "exit",
        }
    }
}
