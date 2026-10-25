//! A scripted [`Facade`] for tests: it plays back queued input events, records every
//! side effect, and can be told to fail any call.

use crate::facade::{
    Audio, Cue, Facade, FacadeError, HotkeyBinding, InputEvent, Replacement, Result, Timer,
    TrayStatus,
};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::io;
use std::sync::Once;
use std::time::Duration;

pub const SAMPLE_RATE: u32 = 16_000;

thread_local! {
    static CAPTURED: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
}

/// Writes log lines into this thread's capture buffer, if one is open.
struct ThreadBuffer;

impl io::Write for ThreadBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        CAPTURED.with_borrow_mut(|buffer| {
            if let Some(buffer) = buffer {
                buffer.extend_from_slice(bytes);
            }
        });
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Returns everything logged on this thread while `f` runs.
///
/// One global subscriber serves every test. A scoped `with_default` per test would
/// race with tests on other threads, which can cache a log call as disabled.
pub fn capture_logs(f: impl FnOnce()) -> String {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(|| ThreadBuffer)
            .finish();
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
    tracing::callsite::rebuild_interest_cache();
    CAPTURED.set(Some(Vec::new()));
    f();
    let bytes = CAPTURED.take().unwrap_or_default();
    String::from_utf8_lossy(&bytes).into_owned()
}

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
    PlayCue,
    StartTimer,
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
            Self::SetTrayStatus | Self::Notify | Self::PlayCue | Self::StartTimer => {
                FacadeError::Tray(reason)
            }
        }
    }
}

/// Scripted stand-in for a platform implementation that records every call.
#[derive(Debug)]
pub struct FakeFacade {
    pub events: VecDeque<InputEvent>,
    /// The samples `stop_recording` returns.
    pub audio: Vec<f32>,
    pub transcript: String,
    pub selection: Option<String>,
    /// What is selected by the time a rewrite finishes, if the user changed it.
    pub selection_after_rewrite: Option<String>,
    pub failing: Vec<Call>,
    /// Calls that report [`FacadeError::Cancelled`], as if Esc interrupted them.
    pub cancelled: Vec<Call>,
    /// Calls that panic, as a bug in a platform adapter would.
    pub panicking: Vec<Call>,
    pub recording: bool,
    pub bindings: Vec<HotkeyBinding>,
    pub inserted: Vec<String>,
    pub clipboard: Vec<String>,
    pub tray: Vec<TrayStatus>,
    pub notifications: Vec<String>,
    pub cues: Vec<Cue>,
    pub timers: Vec<(Timer, Duration)>,
}

impl Default for FakeFacade {
    fn default() -> Self {
        Self {
            events: VecDeque::new(),
            // One second of sound, loud enough to count as speech.
            audio: vec![0.5; SAMPLE_RATE as usize],
            transcript: "hello world".into(),
            selection: None,
            selection_after_rewrite: None,
            failing: Vec::new(),
            cancelled: Vec::new(),
            panicking: Vec::new(),
            recording: false,
            bindings: Vec::new(),
            inserted: Vec::new(),
            clipboard: Vec::new(),
            tray: Vec::new(),
            notifications: Vec::new(),
            cues: Vec::new(),
            timers: Vec::new(),
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
        assert!(!self.panicking.contains(&call), "{call:?} panicked");
        if self.failing.contains(&call) {
            Err(call.error())
        } else if self.cancelled.contains(&call) {
            Err(FacadeError::Cancelled)
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
            samples: self.audio.clone(),
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

    fn play_cue(&mut self, cue: Cue) -> Result<()> {
        self.check(Call::PlayCue)?;
        self.cues.push(cue);
        Ok(())
    }

    fn start_timer(&mut self, timer: Timer, after: Duration) -> Result<()> {
        self.check(Call::StartTimer)?;
        self.timers.push((timer, after));
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

    #[test]
    fn cancelled_call_reports_cancellation() {
        let mut facade = FakeFacade {
            cancelled: vec![Call::Rewrite],
            ..FakeFacade::default()
        };

        assert_eq!(facade.rewrite("hi"), Err(FacadeError::Cancelled));
    }

    #[test]
    #[should_panic(expected = "PlayCue panicked")]
    fn panicking_call_panics() {
        let mut facade = FakeFacade {
            panicking: vec![Call::PlayCue],
            ..FakeFacade::default()
        };

        let _ = facade.play_cue(Cue::Start);
    }
}
