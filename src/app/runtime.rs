//! Runs the state machine against a real [`Facade`].
//!
//! This is the only module that calls the facade. Each effect's result comes back as
//! an [`Event`] and is fed straight into [`next`] before the next input is read.

use super::effect::Effect;
use super::state::{Event, State};
use super::transition::next;
use crate::config::{Config, ConfigWarning};
use crate::facade::{Facade, Result};

/// What running one effect led to.
enum Outcome {
    /// Carry on, feeding back the follow-up event if there is one.
    Continue(Option<Event>),
    Exit,
}

/// Registers the configured hotkeys, reports any config problems, and processes input
/// until the user quits.
///
/// Returns an error only if the hotkeys can't be registered or input can't be read;
/// every other failure is reported to the user and the app keeps running.
pub fn run_app(facade: &mut dyn Facade, config: &Config, warnings: &[ConfigWarning]) -> Result<()> {
    facade.register_hotkeys(&config.bindings())?;
    if !warnings.is_empty() {
        perform(Effect::Notify(config_notice(warnings)), facade);
    }
    let mut state = Some(State::Idle);
    while let Some(current) = state {
        let event = Event::Input(facade.next_event()?);
        state = dispatch(current, event, facade);
    }
    Ok(())
}

/// Applies `event` and every follow-up event it causes, returning the settled state,
/// or `None` if the app should exit.
pub fn dispatch(state: State, event: Event, facade: &mut dyn Facade) -> Option<State> {
    let (state, effects) = next(state, event);
    let follow_ups = effects
        .into_iter()
        .map(|effect| match perform(effect, facade) {
            Outcome::Continue(event) => Some(event),
            Outcome::Exit => None,
        })
        .collect::<Option<Vec<_>>>()?;
    follow_ups
        .into_iter()
        .flatten()
        .try_fold(state, |state, event| dispatch(state, event, facade))
}

/// One notification for all config problems, so startup shows a single message.
fn config_notice(warnings: &[ConfigWarning]) -> String {
    std::iter::once("fluentb settings:".to_owned())
        .chain(warnings.iter().map(ToString::to_string))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Carries out one effect. Status updates (tray, notifications) are best effort so a
/// failure to report can never trigger another report.
fn perform(effect: Effect, facade: &mut dyn Facade) -> Outcome {
    let follow_up = match effect {
        Effect::StartRecording => facade.start_recording().err().map(Event::Failed),
        Effect::StopRecording => Some(
            facade
                .stop_recording()
                .map_or_else(Event::Failed, Event::RecordingStopped),
        ),
        Effect::DiscardRecording => {
            let _ = facade.stop_recording();
            None
        }
        Effect::Transcribe(audio) => Some(
            facade
                .transcribe(&audio)
                .map_or_else(Event::Failed, Event::Transcribed),
        ),
        Effect::ReadSelection => Some(
            facade
                .selected_text()
                .map_or_else(Event::Failed, Event::SelectionRead),
        ),
        Effect::Rewrite(original) => Some(facade.rewrite(&original).map_or_else(
            Event::Failed,
            |rewritten| Event::Rewritten {
                original,
                rewritten,
            },
        )),
        Effect::ReplaceSelection {
            original,
            replacement,
        } => Some(
            facade
                .replace_selection(&original, &replacement)
                .map_or_else(Event::Failed, Event::SelectionReplaced),
        ),
        Effect::InsertText(text) => facade.insert_text(&text).err().map(Event::Failed),
        Effect::SetTray(status) => {
            let _ = facade.set_tray_status(status);
            None
        }
        Effect::Notify(message) => {
            let _ = facade.notify(&message);
            None
        }
        Effect::Exit => return Outcome::Exit,
    };
    Outcome::Continue(follow_up)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::transition::{COPIED, EMPTY_REWRITE, NO_SELECTION};
    use crate::facade::{FacadeError, Hotkey, InputEvent, TrayStatus};
    use crate::platform::Os;
    use crate::testing::{Call, FakeFacade};

    const DICTATE: InputEvent = InputEvent::HotkeyPressed(Hotkey::Dictate);
    const REWRITE: InputEvent = InputEvent::HotkeyPressed(Hotkey::Rewrite);
    const QUIT: InputEvent = InputEvent::QuitRequested;

    fn config() -> Config {
        Config::defaults(Os::MacOs)
    }

    fn run(facade: &mut FakeFacade) {
        assert_eq!(run_app(facade, &config(), &[]), Ok(()));
    }

    #[test]
    fn dictation_inserts_transcribed_text() {
        let mut facade = FakeFacade::with_events([DICTATE, DICTATE, QUIT]);

        run(&mut facade);

        assert_eq!(facade.bindings, config().bindings());
        assert_eq!(facade.inserted, ["hello world"]);
        assert_eq!(
            facade.tray,
            [
                TrayStatus::Recording,
                TrayStatus::Transcribing,
                TrayStatus::Idle
            ]
        );
        assert!(facade.notifications.is_empty());
        assert!(!facade.recording);
    }

    #[test]
    fn blank_transcription_inserts_nothing() {
        let mut facade = FakeFacade {
            transcript: "  ".into(),
            ..FakeFacade::with_events([DICTATE, DICTATE, QUIT])
        };

        run(&mut facade);

        assert!(facade.inserted.is_empty());
        assert_eq!(facade.tray.last(), Some(&TrayStatus::Idle));
    }

    #[test]
    fn rewrite_replaces_selected_text() {
        let mut facade = FakeFacade {
            selection: Some("  teh draft  ".into()),
            ..FakeFacade::with_events([REWRITE, QUIT])
        };

        run(&mut facade);

        assert_eq!(facade.inserted, ["teh draft"]);
        assert_eq!(facade.tray, [TrayStatus::Rewriting, TrayStatus::Idle]);
        assert!(facade.notifications.is_empty());
    }

    #[test]
    fn rewrite_is_copied_when_the_selection_changed() {
        let mut facade = FakeFacade {
            selection: Some("draft".into()),
            selection_after_rewrite: Some("something else".into()),
            ..FakeFacade::with_events([REWRITE, QUIT])
        };

        run(&mut facade);

        assert!(facade.inserted.is_empty());
        assert_eq!(facade.clipboard, ["draft"]);
        assert_eq!(facade.notifications, [COPIED]);
        assert_eq!(facade.tray.last(), Some(&TrayStatus::Idle));
    }

    #[test]
    fn rewrite_without_selection_notifies_user() {
        let mut facade = FakeFacade::with_events([REWRITE, QUIT]);

        run(&mut facade);

        assert!(facade.inserted.is_empty());
        assert_eq!(facade.notifications, [NO_SELECTION]);
    }

    #[test]
    fn blank_rewrite_leaves_text_unchanged() {
        let mut facade = FakeFacade::default();
        let rewritten = Event::Rewritten {
            original: "draft".into(),
            rewritten: " ".into(),
        };
        let state = dispatch(State::Rewriting, rewritten, &mut facade);

        assert_eq!(state, Some(State::Idle));
        assert!(facade.inserted.is_empty());
        assert_eq!(facade.notifications, [EMPTY_REWRITE]);
    }

    #[test]
    fn each_failure_is_reported_and_the_app_keeps_working() {
        // Each script attempts the action twice where possible; seeing the failure
        // reported for both attempts proves the first one didn't wedge the app.
        let failing = [
            (Call::StartRecording, vec![DICTATE, DICTATE], 2),
            (Call::StopRecording, vec![DICTATE, DICTATE], 1),
            (Call::Transcribe, vec![DICTATE, DICTATE], 1),
            (Call::InsertText, vec![DICTATE, DICTATE], 1),
            (Call::SelectedText, vec![REWRITE, REWRITE], 2),
            (Call::Rewrite, vec![REWRITE, REWRITE], 2),
            (Call::ReplaceSelection, vec![REWRITE, REWRITE], 2),
        ];

        failing.into_iter().for_each(|(call, script, attempts)| {
            let mut facade = FakeFacade {
                selection: Some("draft".into()),
                failing: vec![call],
                ..FakeFacade::with_events(script.into_iter().chain([QUIT]))
            };

            run(&mut facade);

            assert_eq!(
                facade.notifications,
                vec![call.error().to_string(); attempts],
                "{call:?}"
            );
            assert!(facade.tray.contains(&TrayStatus::Error), "{call:?}");
            assert_eq!(facade.tray.last(), Some(&TrayStatus::Idle), "{call:?}");
            assert!(!facade.recording, "{call:?}");
        });
    }

    #[test]
    fn failing_status_updates_do_not_loop() {
        let mut facade = FakeFacade {
            failing: vec![Call::StartRecording, Call::SetTrayStatus, Call::Notify],
            ..FakeFacade::with_events([DICTATE, QUIT])
        };

        run(&mut facade);

        assert!(facade.tray.is_empty());
        assert!(facade.notifications.is_empty());
    }

    #[test]
    fn rewrite_hotkey_is_ignored_while_recording() {
        let mut facade = FakeFacade {
            selection: Some("draft".into()),
            ..FakeFacade::with_events([DICTATE, REWRITE, DICTATE, QUIT])
        };

        run(&mut facade);

        assert_eq!(facade.inserted, ["hello world"]);
        assert!(!facade.tray.contains(&TrayStatus::Rewriting));
    }

    #[test]
    fn quit_while_recording_releases_the_microphone() {
        let mut facade = FakeFacade::with_events([DICTATE, QUIT]);

        run(&mut facade);

        assert!(!facade.recording);
        assert!(facade.inserted.is_empty());
    }

    #[test]
    fn hotkey_registration_failure_stops_the_app() {
        let mut facade = FakeFacade {
            failing: vec![Call::RegisterHotkeys],
            ..FakeFacade::with_events([QUIT])
        };

        assert_eq!(
            run_app(&mut facade, &config(), &[]),
            Err(Call::RegisterHotkeys.error())
        );
    }

    #[test]
    fn input_failure_stops_the_app() {
        let mut facade = FakeFacade::with_events([DICTATE]);

        assert_eq!(
            run_app(&mut facade, &config(), &[]),
            Err(FacadeError::Hotkeys("no more events".into()))
        );
    }

    #[test]
    fn config_warnings_are_shown_once_at_startup() {
        let mut facade = FakeFacade::with_events([QUIT]);
        let warnings = [
            ConfigWarning::UnknownKey("theme".into()),
            ConfigWarning::HotkeyConflict,
        ];

        assert_eq!(run_app(&mut facade, &config(), &warnings), Ok(()));

        assert_eq!(
            facade.notifications,
            [format!(
                "fluentb settings:\n{}\n{}",
                warnings[0], warnings[1]
            )]
        );
    }

    #[test]
    fn registers_the_configured_hotkeys() {
        let (config, _) = crate::config::parse("[hotkeys]\ndictate = \"Alt+Space\"\n", Os::Linux);
        let mut facade = FakeFacade::with_events([QUIT]);

        assert_eq!(run_app(&mut facade, &config, &[]), Ok(()));

        assert_eq!(facade.bindings, config.bindings());
        assert!(facade.notifications.is_empty());
    }
}
