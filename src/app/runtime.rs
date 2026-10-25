//! Runs the state machine against a real [`Facade`].
//!
//! This is the only module that calls the facade. Each effect's result comes back as
//! an [`Event`] and is fed straight into [`next`] before the next input is read.
//!
//! Every transition and effect is logged by kind and timing only: events and effects
//! carry transcripts and selections, so they are never logged themselves.

use super::effect::Effect;
use super::input;
use super::models::{self, HotkeyLabels, Models};
use super::state::{Event, State};
use super::transition::next;
use crate::accelerator::Accelerator;
use crate::config::{Config, ConfigWarning, DictationMode};
use crate::facade::{Facade, FacadeError, Hotkey, HotkeyBinding, InputEvent, Result, Timer};
use crate::platform::Os;
use std::panic::{self, AssertUnwindSafe};
use std::time::{Duration, Instant};

/// How long the error icon stays up.
pub const ERROR_DISPLAY: Duration = Duration::from_secs(2);

/// What the runtime needs from the config to carry out effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The Dictate and Rewrite hotkeys; Esc is added while busy.
    pub bindings: Vec<HotkeyBinding>,
    pub mode: DictationMode,
    pub sounds: bool,
    pub max_recording: Duration,
}

impl Settings {
    #[must_use]
    pub fn from_config(config: &Config) -> Self {
        Self {
            bindings: config.bindings(),
            mode: config.dictation.mode,
            sounds: config.dictation.sounds,
            max_recording: Duration::from_secs(u64::from(config.dictation.max_recording_seconds)),
        }
    }

    /// The hotkeys to register, with Esc as the cancel hotkey while busy.
    fn bindings_while(&self, busy: bool) -> Vec<HotkeyBinding> {
        let cancel = busy.then(|| HotkeyBinding {
            hotkey: Hotkey::Cancel,
            accelerator: Accelerator::esc(),
        });
        self.bindings.iter().cloned().chain(cancel).collect()
    }

    const fn duration(&self, timer: Timer) -> Duration {
        match timer {
            Timer::MaxRecording => self.max_recording,
            Timer::ClearError => ERROR_DISPLAY,
        }
    }
}

/// What running one effect led to.
enum Outcome {
    /// Carry on, feeding back the follow-up event if there is one.
    Continue(Option<Event>),
    Exit,
}

/// Registers the configured hotkeys, reports any config problems, and processes input
/// until the user quits. `models` is the status of each model at startup; changes
/// arrive later as [`InputEvent::ModelChanged`].
///
/// Returns an error only if the hotkeys can't be registered or input can't be read;
/// every other failure is reported to the user and the app keeps running.
pub fn run_app(
    facade: &mut dyn Facade,
    config: &Config,
    warnings: &[ConfigWarning],
    models: Models,
) -> Result<()> {
    let settings = Settings::from_config(config);
    facade.register_hotkeys(&settings.bindings)?;
    if !warnings.is_empty() {
        perform(Effect::Notify(config_notice(warnings)), &settings, facade);
    }
    let labels = HotkeyLabels {
        dictate: config.hotkeys.dictate.display(Os::current()),
        rewrite: config.hotkeys.rewrite.display(Os::current()),
    };
    tracing::info!(mode = ?settings.mode, sounds = settings.sounds, "started");
    let mut current = Some((State::Idle, models));
    while let Some((state, models)) = current {
        let event = Event::Input(facade.next_event()?);
        current = handle(state, models, event, &labels, &settings, facade);
    }
    tracing::info!("quit");
    Ok(())
}

/// Applies one input event, keeping track of model status, dropping hotkey events
/// the dictation mode doesn't use, and holding back hotkeys whose model isn't ready.
/// Returns `None` if the app should exit.
fn handle(
    state: State,
    models: Models,
    event: Event,
    labels: &HotkeyLabels,
    settings: &Settings,
    facade: &mut dyn Facade,
) -> Option<(State, Models)> {
    if let Event::Input(InputEvent::ModelChanged(kind, status)) = event {
        if let Some(message) = models::announce(kind, models.get(kind), &status, labels) {
            perform(Effect::Notify(message), settings, facade);
        }
        return Some((state, models.with(kind, status)));
    }
    if let Event::Input(input) = &event {
        if !input::accepts(settings.mode, state, input) {
            return Some((state, models));
        }
    }
    if let Some(message) = models.blocked(state, &event) {
        perform(Effect::Notify(message), settings, facade);
        return Some((state, models));
    }
    dispatch(state, event, settings, facade).map(|state| (state, models))
}

/// Applies `event` and every follow-up event it causes, returning the settled state,
/// or `None` if the app should exit.
pub fn dispatch(
    state: State,
    event: Event,
    settings: &Settings,
    facade: &mut dyn Facade,
) -> Option<State> {
    let kind = event.kind();
    let (next_state, effects) = next(state, event);
    if next_state != state || !effects.is_empty() {
        tracing::info!(
            from = ?state,
            event = kind,
            to = ?next_state,
            effects = ?effects.iter().map(Effect::kind).collect::<Vec<_>>(),
            "transition"
        );
    }
    let follow_ups = effects
        .into_iter()
        .map(|effect| match perform(effect, settings, facade) {
            Outcome::Continue(event) => Some(event),
            Outcome::Exit => None,
        })
        .collect::<Option<Vec<_>>>()?;
    follow_ups
        .into_iter()
        .flatten()
        .try_fold(next_state, |state, event| {
            dispatch(state, event, settings, facade)
        })
}

/// One notification for all config problems, so startup shows a single message.
fn config_notice(warnings: &[ConfigWarning]) -> String {
    std::iter::once("fluentb settings:".to_owned())
        .chain(warnings.iter().map(ToString::to_string))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Carries out one effect, logging how long it took. A panic inside the facade is
/// caught here, so a bug in one effect is reported like any failure instead of
/// killing the app; status updates swallow theirs, so reporting can't loop.
fn perform(effect: Effect, settings: &Settings, facade: &mut dyn Facade) -> Outcome {
    let kind = effect.kind();
    let best_effort = is_best_effort(&effect);
    let started = Instant::now();
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| run(effect, settings, facade)))
        .unwrap_or_else(|_| {
            tracing::error!(effect = kind, "effect panicked");
            Outcome::Continue(
                (!best_effort)
                    .then(|| Event::Failed(FacadeError::Internal(format!("{kind} crashed")))),
            )
        });
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    if let Outcome::Continue(Some(Event::Failed(error))) = &outcome {
        tracing::warn!(
            effect = kind,
            error = error.kind(),
            elapsed_ms,
            "effect failed"
        );
    } else {
        tracing::info!(effect = kind, elapsed_ms, "effect");
    }
    outcome
}

/// Effects whose failure is never reported back to the state machine.
const fn is_best_effort(effect: &Effect) -> bool {
    matches!(
        effect,
        Effect::DiscardRecording
            | Effect::SetTray(_)
            | Effect::Notify(_)
            | Effect::PlayCue(_)
            | Effect::StartTimer(_)
            | Effect::ListenForCancel(_)
    )
}

fn run(effect: Effect, settings: &Settings, facade: &mut dyn Facade) -> Outcome {
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
        Effect::PlayCue(cue) => {
            if settings.sounds {
                let _ = facade.play_cue(cue);
            }
            None
        }
        Effect::StartTimer(timer) => {
            let _ = facade.start_timer(timer, settings.duration(timer));
            None
        }
        Effect::ListenForCancel(busy) => {
            let _ = facade.register_hotkeys(&settings.bindings_while(busy));
            None
        }
        Effect::Exit => return Outcome::Exit,
    };
    Outcome::Continue(follow_up)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::transition::{COPIED, EMPTY_REWRITE, MICROPHONE_LOST, NO_SELECTION, NO_SPEECH};
    use crate::facade::{Cue, ModelStatus, TrayStatus};
    use crate::models::Kind;
    use crate::platform::Os;
    use crate::testing::{capture_logs, Call, FakeFacade};

    const DICTATE: InputEvent = InputEvent::HotkeyPressed(Hotkey::Dictate);
    const REWRITE: InputEvent = InputEvent::HotkeyPressed(Hotkey::Rewrite);
    const QUIT: InputEvent = InputEvent::QuitRequested;
    const RELEASE: InputEvent = InputEvent::HotkeyReleased(Hotkey::Dictate);
    const CANCEL: InputEvent = InputEvent::HotkeyPressed(Hotkey::Cancel);

    fn config() -> Config {
        Config::defaults(Os::MacOs)
    }

    fn run(facade: &mut FakeFacade) {
        assert_eq!(run_app(facade, &config(), &[], Models::ready()), Ok(()));
    }

    #[test]
    fn dictation_waits_for_the_speech_model() {
        let ready = InputEvent::ModelChanged(Kind::Speech, ModelStatus::Ready);
        let mut facade = FakeFacade::with_events([DICTATE, ready, DICTATE, DICTATE, QUIT]);
        let models = Models {
            speech: ModelStatus::Downloading { percent: 42 },
            rewrite: ModelStatus::Downloading { percent: 0 },
        };

        assert_eq!(run_app(&mut facade, &config(), &[], models), Ok(()));

        assert_eq!(facade.inserted, ["hello world"]);
        assert_eq!(
            facade.notifications,
            [
                "The speech model is still downloading (42%). Dictation will work as soon as it's ready.",
                "Dictation is ready: press ⇧⌘D, speak, then press it again.",
            ]
        );
    }

    #[test]
    fn model_progress_is_tracked_quietly() {
        let progress =
            InputEvent::ModelChanged(Kind::Rewrite, ModelStatus::Downloading { percent: 60 });
        let mut facade = FakeFacade {
            selection: Some("draft".into()),
            ..FakeFacade::with_events([progress, REWRITE, QUIT])
        };

        run(&mut facade);

        assert!(facade.inserted.is_empty());
        assert_eq!(
            facade.notifications,
            ["The rewrite model is still downloading (60%). Rewrite will work as soon as it's ready."]
        );
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
        let settings = Settings::from_config(&config());
        let state = dispatch(State::Rewriting, rewritten, &settings, &mut facade);

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
            assert!(
                facade.timers.contains(&(Timer::ClearError, ERROR_DISPLAY)),
                "{call:?}"
            );
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
            run_app(&mut facade, &config(), &[], Models::ready()),
            Err(Call::RegisterHotkeys.error())
        );
    }

    #[test]
    fn input_failure_stops_the_app() {
        let mut facade = FakeFacade::with_events([DICTATE]);

        assert_eq!(
            run_app(&mut facade, &config(), &[], Models::ready()),
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

        assert_eq!(
            run_app(&mut facade, &config(), &warnings, Models::ready()),
            Ok(())
        );

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

        assert_eq!(run_app(&mut facade, &config, &[], Models::ready()), Ok(()));

        assert_eq!(facade.bindings, config.bindings());
        assert!(facade.notifications.is_empty());
    }

    fn config_with(text: &str) -> Config {
        crate::config::parse(text, Os::MacOs).0
    }

    fn cancel_binding() -> HotkeyBinding {
        HotkeyBinding {
            hotkey: Hotkey::Cancel,
            accelerator: Accelerator::esc(),
        }
    }

    #[test]
    fn push_to_talk_records_while_the_hotkey_is_held() {
        let config = config_with("[dictation]\nmode = \"push-to-talk\"\n");
        let mut facade = FakeFacade::with_events([DICTATE, DICTATE, RELEASE, RELEASE, QUIT]);

        assert_eq!(run_app(&mut facade, &config, &[], Models::ready()), Ok(()));

        assert_eq!(facade.inserted, ["hello world"]);
        assert!(!facade.recording);
    }

    #[test]
    fn toggle_mode_ignores_releases() {
        let mut facade = FakeFacade::with_events([DICTATE, RELEASE, DICTATE, RELEASE, QUIT]);

        run(&mut facade);

        assert_eq!(facade.inserted, ["hello world"]);
    }

    #[test]
    fn esc_is_registered_only_while_busy() {
        let mut facade = FakeFacade::with_events([DICTATE]);

        assert!(run_app(&mut facade, &config(), &[], Models::ready()).is_err());

        assert_eq!(
            facade.bindings,
            [config().bindings(), vec![cancel_binding()]].concat()
        );
    }

    #[test]
    fn esc_discards_a_recording() {
        let mut facade = FakeFacade::with_events([DICTATE, CANCEL, QUIT]);

        run(&mut facade);

        assert!(facade.inserted.is_empty());
        assert!(facade.notifications.is_empty());
        assert!(!facade.recording);
        assert_eq!(facade.tray.last(), Some(&TrayStatus::Idle));
        assert_eq!(facade.bindings, config().bindings());
    }

    #[test]
    fn esc_interrupts_transcription_and_rewrite_quietly() {
        [
            (Call::Transcribe, vec![DICTATE, DICTATE]),
            (Call::Rewrite, vec![REWRITE]),
        ]
        .into_iter()
        .for_each(|(call, script)| {
            let mut facade = FakeFacade {
                selection: Some("draft".into()),
                cancelled: vec![call],
                ..FakeFacade::with_events(script.into_iter().chain([CANCEL, QUIT]))
            };

            run(&mut facade);

            assert!(facade.inserted.is_empty(), "{call:?}");
            assert!(facade.notifications.is_empty(), "{call:?}");
            assert_eq!(facade.tray.last(), Some(&TrayStatus::Idle), "{call:?}");
            assert_eq!(facade.bindings, config().bindings(), "{call:?}");
        });
    }

    #[test]
    fn long_recordings_stop_on_their_own() {
        let config = config_with("[dictation]\nmax_recording_seconds = 30\n");
        let timeout = InputEvent::TimerFired(Timer::MaxRecording);
        let mut facade = FakeFacade::with_events([DICTATE, timeout, QUIT]);

        assert_eq!(run_app(&mut facade, &config, &[], Models::ready()), Ok(()));

        assert_eq!(facade.inserted, ["hello world"]);
        assert_eq!(
            facade.timers,
            [(Timer::MaxRecording, Duration::from_secs(30))]
        );
    }

    #[test]
    fn silent_recordings_insert_nothing() {
        let mut facade = FakeFacade {
            audio: vec![0.0; 32_000],
            ..FakeFacade::with_events([DICTATE, DICTATE, QUIT])
        };

        run(&mut facade);

        assert!(facade.inserted.is_empty());
        assert_eq!(facade.notifications, [NO_SPEECH]);
        assert_eq!(facade.tray.last(), Some(&TrayStatus::Idle));
    }

    #[test]
    fn a_lost_microphone_transcribes_what_was_recorded() {
        let mut facade = FakeFacade::with_events([DICTATE, InputEvent::MicrophoneLost, QUIT]);

        run(&mut facade);

        assert_eq!(facade.inserted, ["hello world"]);
        assert_eq!(facade.notifications, [MICROPHONE_LOST]);
    }

    #[test]
    fn cues_play_only_when_sounds_are_on() {
        let mut facade = FakeFacade::with_events([DICTATE, DICTATE, QUIT]);
        run(&mut facade);
        assert_eq!(facade.cues, [Cue::Start, Cue::Stop]);

        let quiet = config_with("[dictation]\nsounds = false\n");
        let mut facade = FakeFacade::with_events([DICTATE, DICTATE, QUIT]);
        assert_eq!(run_app(&mut facade, &quiet, &[], Models::ready()), Ok(()));
        assert!(facade.cues.is_empty());
        assert_eq!(facade.inserted, ["hello world"]);
    }

    #[test]
    fn the_error_icon_clears_after_a_moment() {
        let clear = InputEvent::TimerFired(Timer::ClearError);
        let mut facade = FakeFacade {
            failing: vec![Call::StartRecording],
            ..FakeFacade::with_events([DICTATE, clear, QUIT])
        };

        run(&mut facade);

        assert_eq!(
            facade.tray,
            [TrayStatus::Recording, TrayStatus::Error, TrayStatus::Idle]
        );
    }

    #[test]
    fn a_panicking_effect_is_reported_and_the_app_keeps_working() {
        let mut facade = FakeFacade {
            panicking: vec![Call::Transcribe],
            ..FakeFacade::with_events([DICTATE, DICTATE, DICTATE, DICTATE, QUIT])
        };

        run(&mut facade);

        assert_eq!(
            facade.notifications,
            ["internal error: transcribe crashed"; 2]
        );
        assert!(!facade.recording);
    }

    #[test]
    fn a_panicking_status_update_is_swallowed() {
        let mut facade = FakeFacade {
            panicking: vec![Call::StartRecording, Call::Notify, Call::SetTrayStatus],
            ..FakeFacade::with_events([DICTATE, DICTATE, QUIT])
        };

        run(&mut facade);

        assert!(facade.notifications.is_empty());
        assert!(!facade.recording);
    }

    #[test]
    fn logs_never_contain_user_content() {
        let secret = "SECRET";
        let logs = capture_logs(|| {
            let mut facade = FakeFacade {
                transcript: format!("{secret} transcript"),
                selection: Some(format!("{secret} selection")),
                ..FakeFacade::with_events([DICTATE, DICTATE, REWRITE, QUIT])
            };
            run(&mut facade);
            assert_eq!(facade.inserted.len(), 2);

            let mut failing = FakeFacade {
                selection: Some(format!("{secret} selection")),
                selection_after_rewrite: Some(format!("{secret} other")),
                failing: vec![Call::InsertText],
                panicking: vec![Call::Rewrite],
                ..FakeFacade::with_events([DICTATE, DICTATE, REWRITE, QUIT])
            };
            run(&mut failing);
        });

        assert!(logs.contains("transition"), "{logs}");
        assert!(logs.contains("transcribe"), "{logs}");
        assert!(logs.contains("elapsed_ms"), "{logs}");
        assert!(logs.contains("effect failed"), "{logs}");
        assert!(!logs.contains(secret), "{logs}");
    }
}
