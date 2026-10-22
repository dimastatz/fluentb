//! The single boundary between fluentb's pure state manager and the outside world.
//!
//! The state manager decides *what* should happen; an implementation of [`Facade`]
//! is the only place that actually touches the OS, the microphone, Whisper, or the
//! rewrite engine. Keeping every side effect behind this trait lets the state manager
//! stay pure and lets tests drive it with a fake implementation.
//!
//! Each platform (macOS now, Windows and Linux later) provides its own implementation.

use std::fmt;

/// The global hotkeys fluentb listens for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hotkey {
    /// Start or stop dictation into the focused text field.
    Dictate,
    /// Rewrite the currently selected text into clean, fluent text.
    Rewrite,
}

/// A key combination bound to a [`Hotkey`], e.g. `"Cmd+Shift+D"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyBinding {
    pub hotkey: Hotkey,
    pub accelerator: String,
}

/// Events the outside world delivers to the state manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    HotkeyPressed(Hotkey),
    /// The user chose "Quit" from the tray menu.
    QuitRequested,
}

/// Captured microphone audio: mono samples at `sample_rate` Hz.
#[derive(Debug, Clone, PartialEq)]
pub struct Audio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
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

    /// Rewrites text into clean, fluent, correctly styled text.
    fn rewrite(&mut self, text: &str) -> Result<String>;

    // Output

    /// Types text into the focused text field, replacing the current selection if there is one.
    fn insert_text(&mut self, text: &str) -> Result<()>;

    /// Updates the tray icon to reflect the current state.
    fn set_tray_status(&mut self, status: TrayStatus) -> Result<()>;

    /// Shows a short, non-blocking notification, e.g. to report an error.
    fn notify(&mut self, message: &str) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// Scripted stand-in for a platform implementation that records every call.
    #[derive(Default)]
    struct FakeFacade {
        events: VecDeque<InputEvent>,
        selection: Option<String>,
        recording: bool,
        bindings: Vec<HotkeyBinding>,
        inserted: Vec<String>,
        tray: Vec<TrayStatus>,
        notifications: Vec<String>,
    }

    const SAMPLE_RATE: u32 = 16_000;

    impl Facade for FakeFacade {
        fn register_hotkeys(&mut self, bindings: &[HotkeyBinding]) -> Result<()> {
            self.bindings = bindings.to_vec();
            Ok(())
        }

        fn next_event(&mut self) -> Result<InputEvent> {
            self.events
                .pop_front()
                .ok_or_else(|| FacadeError::Hotkeys("no more events".into()))
        }

        fn start_recording(&mut self) -> Result<()> {
            self.recording = true;
            Ok(())
        }

        fn stop_recording(&mut self) -> Result<Audio> {
            if !self.recording {
                return Err(FacadeError::Microphone("not recording".into()));
            }
            self.recording = false;
            Ok(Audio {
                samples: vec![0.0; SAMPLE_RATE as usize],
                sample_rate: SAMPLE_RATE,
            })
        }

        fn transcribe(&mut self, _audio: &Audio) -> Result<String> {
            Ok("hello world".into())
        }

        fn selected_text(&mut self) -> Result<Option<String>> {
            Ok(self.selection.clone())
        }

        fn rewrite(&mut self, text: &str) -> Result<String> {
            Ok(text.trim().into())
        }

        fn insert_text(&mut self, text: &str) -> Result<()> {
            self.inserted.push(text.into());
            Ok(())
        }

        fn set_tray_status(&mut self, status: TrayStatus) -> Result<()> {
            self.tray.push(status);
            Ok(())
        }

        fn notify(&mut self, message: &str) -> Result<()> {
            self.notifications.push(message.into());
            Ok(())
        }
    }

    /// Drives a full dictation through a trait object, as the state manager will.
    fn dictate(facade: &mut dyn Facade) -> Result<()> {
        facade.set_tray_status(TrayStatus::Recording)?;
        facade.start_recording()?;
        let audio = facade.stop_recording()?;
        facade.set_tray_status(TrayStatus::Transcribing)?;
        let text = facade.transcribe(&audio)?;
        facade.insert_text(&text)?;
        facade.set_tray_status(TrayStatus::Idle)
    }

    /// Rewrites the selection through a trait object, or tells the user there is none.
    fn rewrite_selection(facade: &mut dyn Facade) -> Result<()> {
        match facade.selected_text()? {
            Some(text) => {
                facade.set_tray_status(TrayStatus::Rewriting)?;
                let rewritten = facade.rewrite(&text)?;
                facade.insert_text(&rewritten)?;
                facade.set_tray_status(TrayStatus::Idle)
            }
            None => facade.notify("Select some text to rewrite"),
        }
    }

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
        ];

        cases
            .iter()
            .for_each(|(error, expected)| assert_eq!(error.to_string(), *expected));
    }

    #[test]
    fn error_works_as_std_error() {
        let error: Box<dyn std::error::Error> = Box::new(FacadeError::Tray("hidden".into()));

        assert_eq!(error.to_string(), "tray: hidden");
    }

    #[test]
    fn registers_hotkeys_and_delivers_events_in_order() {
        let bindings = vec![
            HotkeyBinding {
                hotkey: Hotkey::Dictate,
                accelerator: "Cmd+Shift+D".into(),
            },
            HotkeyBinding {
                hotkey: Hotkey::Rewrite,
                accelerator: "Cmd+Shift+R".into(),
            },
        ];
        let mut facade = FakeFacade {
            events: VecDeque::from([
                InputEvent::HotkeyPressed(Hotkey::Rewrite),
                InputEvent::QuitRequested,
            ]),
            ..FakeFacade::default()
        };

        facade.register_hotkeys(&bindings).unwrap();

        assert_eq!(facade.bindings, bindings);
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
    fn dictation_inserts_transcribed_text() {
        let mut facade = FakeFacade::default();

        dictate(&mut facade).unwrap();

        assert_eq!(facade.inserted, ["hello world"]);
        assert_eq!(
            facade.tray,
            [
                TrayStatus::Recording,
                TrayStatus::Transcribing,
                TrayStatus::Idle
            ]
        );
    }

    #[test]
    fn stop_recording_returns_captured_audio() {
        let mut facade = FakeFacade::default();

        facade.start_recording().unwrap();

        assert_eq!(
            facade.stop_recording(),
            Ok(Audio {
                samples: vec![0.0; SAMPLE_RATE as usize],
                sample_rate: SAMPLE_RATE,
            })
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
    fn rewrite_replaces_selected_text() {
        let mut facade = FakeFacade {
            selection: Some("  teh draft  ".into()),
            ..FakeFacade::default()
        };

        rewrite_selection(&mut facade).unwrap();

        assert_eq!(facade.inserted, ["teh draft"]);
        assert_eq!(facade.tray, [TrayStatus::Rewriting, TrayStatus::Idle]);
        assert!(facade.notifications.is_empty());
    }

    #[test]
    fn rewrite_without_selection_notifies_user() {
        let mut facade = FakeFacade::default();

        rewrite_selection(&mut facade).unwrap();

        assert!(facade.inserted.is_empty());
        assert_eq!(facade.notifications, ["Select some text to rewrite"]);
    }

    #[test]
    fn tray_reports_errors() {
        let mut facade = FakeFacade::default();

        facade.set_tray_status(TrayStatus::Error).unwrap();

        assert_eq!(facade.tray, [TrayStatus::Error]);
    }
}
