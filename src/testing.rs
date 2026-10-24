//! A scripted [`Facade`] for tests: it plays back queued input events, records every
//! side effect, and can be told to fail any call.

use crate::facade::{
    Audio, Facade, FacadeError, HotkeyBinding, InputEvent, Replacement, Result, TrayStatus,
};
use std::collections::VecDeque;

pub const SAMPLE_RATE: u32 = 16_000;

/// A facade method the fake can be told to fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Call {
    RegisterHotkeys,
    StartRecording,
    StopRecording,
    Transcribe,
    SelectedText,
    Rewrite,
    ReplaceSelection,
    InsertText,
    SetTrayStatus,
    Notify,
}

impl Call {
    /// The error the fake returns when this call is set to fail.
    pub fn error(self) -> FacadeError {
        let reason = format!("{self:?} failed");
        match self {
            Self::RegisterHotkeys => FacadeError::Hotkeys(reason),
            Self::StartRecording | Self::StopRecording => FacadeError::Microphone(reason),
            Self::Transcribe => FacadeError::Transcription(reason),
            Self::SelectedText | Self::InsertText | Self::ReplaceSelection => {
                FacadeError::TextAccess(reason)
            }
            Self::Rewrite => FacadeError::Rewrite(reason),
            Self::SetTrayStatus | Self::Notify => FacadeError::Tray(reason),
        }
    }
}

/// Scripted stand-in for a platform implementation that records every call.
#[derive(Debug)]
pub struct FakeFacade {
    pub events: VecDeque<InputEvent>,
    pub transcript: String,
    pub selection: Option<String>,
    /// What is selected by the time a rewrite finishes, if the user changed it.
    pub selection_after_rewrite: Option<String>,
    pub failing: Vec<Call>,
    pub recording: bool,
    pub bindings: Vec<HotkeyBinding>,
    pub inserted: Vec<String>,
    pub clipboard: Vec<String>,
    pub tray: Vec<TrayStatus>,
    pub notifications: Vec<String>,
}

impl Default for FakeFacade {
    fn default() -> Self {
        Self {
            events: VecDeque::new(),
            transcript: "hello world".into(),
            selection: None,
            selection_after_rewrite: None,
            failing: Vec::new(),
            recording: false,
            bindings: Vec::new(),
            inserted: Vec::new(),
            clipboard: Vec::new(),
            tray: Vec::new(),
            notifications: Vec::new(),
        }
    }
}

impl FakeFacade {
    /// A fake that delivers `events` in order.
    pub fn with_events(events: impl IntoIterator<Item = InputEvent>) -> Self {
        Self {
            events: events.into_iter().collect(),
            ..Self::default()
        }
    }

    fn check(&self, call: Call) -> Result<()> {
        if self.failing.contains(&call) {
            Err(call.error())
        } else {
            Ok(())
        }
    }
}

impl Facade for FakeFacade {
    fn register_hotkeys(&mut self, bindings: &[HotkeyBinding]) -> Result<()> {
        self.check(Call::RegisterHotkeys)?;
        self.bindings = bindings.to_vec();
        Ok(())
    }

    fn next_event(&mut self) -> Result<InputEvent> {
        self.events
            .pop_front()
            .ok_or_else(|| FacadeError::Hotkeys("no more events".into()))
    }

    fn start_recording(&mut self) -> Result<()> {
        self.check(Call::StartRecording)?;
        self.recording = true;
        Ok(())
    }

    fn stop_recording(&mut self) -> Result<Audio> {
        if !self.recording {
            return Err(FacadeError::Microphone("not recording".into()));
        }
        self.recording = false;
        self.check(Call::StopRecording)?;
        Ok(Audio {
            samples: vec![0.0; SAMPLE_RATE as usize],
            sample_rate: SAMPLE_RATE,
        })
    }

    fn transcribe(&mut self, _audio: &Audio) -> Result<String> {
        self.check(Call::Transcribe)?;
        Ok(self.transcript.clone())
    }

    fn selected_text(&mut self) -> Result<Option<String>> {
        self.check(Call::SelectedText)?;
        Ok(self.selection.clone())
    }

    fn rewrite(&mut self, text: &str) -> Result<String> {
        self.check(Call::Rewrite)?;
        Ok(text.trim().into())
    }

    fn replace_selection(&mut self, original: &str, replacement: &str) -> Result<Replacement> {
        self.check(Call::ReplaceSelection)?;
        let current = self
            .selection_after_rewrite
            .as_ref()
            .or(self.selection.as_ref());
        if current.map(String::as_str) == Some(original) {
            self.inserted.push(replacement.into());
            Ok(Replacement::Replaced)
        } else {
            self.clipboard.push(replacement.into());
            Ok(Replacement::Copied)
        }
    }

    fn insert_text(&mut self, text: &str) -> Result<()> {
        self.check(Call::InsertText)?;
        self.inserted.push(text.into());
        Ok(())
    }

    fn set_tray_status(&mut self, status: TrayStatus) -> Result<()> {
        self.check(Call::SetTrayStatus)?;
        self.tray.push(status);
        Ok(())
    }

    fn notify(&mut self, message: &str) -> Result<()> {
        self.check(Call::Notify)?;
        self.notifications.push(message.into());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facade::Hotkey;

    #[test]
    fn delivers_events_in_order_then_reports_exhaustion() {
        let mut facade = FakeFacade::with_events([
            InputEvent::HotkeyPressed(Hotkey::Rewrite),
            InputEvent::QuitRequested,
        ]);

        assert_eq!(
            facade.next_event(),
            Ok(InputEvent::HotkeyPressed(Hotkey::Rewrite))
        );
        assert_eq!(facade.next_event(), Ok(InputEvent::QuitRequested));
        assert_eq!(
            facade.next_event(),
            Err(FacadeError::Hotkeys("no more events".into()))
        );
    }

    #[test]
    fn stop_recording_without_start_fails() {
        let mut facade = FakeFacade::default();

        assert_eq!(
            facade.stop_recording(),
            Err(FacadeError::Microphone("not recording".into()))
        );
    }

    #[test]
    fn failing_call_returns_its_subsystem_error() {
        let mut facade = FakeFacade {
            failing: vec![Call::Notify],
            ..FakeFacade::default()
        };

        assert_eq!(facade.notify("hi"), Err(Call::Notify.error()));
        assert!(facade.notifications.is_empty());
    }
}
