//! The pure transition function at the heart of fluentb.

use super::effect::Effect;
use super::state::{Event, State};
use crate::audio;
use crate::facade::{Audio, Cue, FacadeError, Hotkey, InputEvent, Replacement, Timer, TrayStatus};

pub const NO_SELECTION: &str = "Select some text to rewrite";
pub const EMPTY_REWRITE: &str = "Rewrite returned no text; your text was left unchanged";
pub const COPIED: &str =
    "The selection changed during the rewrite, so the result was copied to the clipboard";
pub const NO_SPEECH: &str = "No speech detected";
pub const MICROPHONE_LOST: &str =
    "The microphone was disconnected, so recording stopped. Transcribing what was recorded.";

/// Decides the next state and the effects to run for `event` in `state`.
///
/// Total over every `(state, event)` pair: anything not listed (a hotkey while busy,
/// a stray result) leaves the state unchanged and does nothing. Esc is listened for
/// exactly while the app is busy.
#[must_use]
pub fn next(state: State, event: Event) -> (State, Vec<Effect>) {
    let (next_state, effects) = step(state, event);
    let busy = |state| state != State::Idle;
    if busy(state) == busy(next_state) {
        (next_state, effects)
    } else {
        let listen = Effect::ListenForCancel(busy(next_state));
        (next_state, effects.into_iter().chain([listen]).collect())
    }
}

fn step(state: State, event: Event) -> (State, Vec<Effect>) {
    match (state, event) {
        (State::Recording, Event::Input(InputEvent::QuitRequested)) => {
            (state, vec![Effect::DiscardRecording, Effect::Exit])
        }
        (_, Event::Input(InputEvent::QuitRequested)) => (state, vec![Effect::Exit]),

        (_, Event::Failed(FacadeError::Cancelled))
        | (
            State::Recording | State::Transcribing | State::Rewriting,
            Event::Input(InputEvent::HotkeyPressed(Hotkey::Cancel)),
        ) => (State::Idle, cancel(state)),
        (State::Recording, Event::Failed(error)) => (
            State::Idle,
            std::iter::once(Effect::DiscardRecording)
                .chain(report(&error))
                .collect(),
        ),
        (_, Event::Failed(error)) => (State::Idle, report(&error)),

        (State::Idle, Event::Input(InputEvent::HotkeyPressed(Hotkey::Dictate))) => (
            State::Recording,
            vec![
                Effect::SetTray(TrayStatus::Recording),
                Effect::PlayCue(Cue::Start),
                Effect::StartRecording,
                Effect::StartTimer(Timer::MaxRecording),
            ],
        ),
        (
            State::Recording,
            Event::Input(
                InputEvent::HotkeyPressed(Hotkey::Dictate)
                | InputEvent::HotkeyReleased(Hotkey::Dictate)
                | InputEvent::TimerFired(Timer::MaxRecording),
            ),
        ) => (State::Transcribing, stop()),
        (State::Recording, Event::Input(InputEvent::MicrophoneLost)) => (
            State::Transcribing,
            stop()
                .into_iter()
                .chain([Effect::Notify(MICROPHONE_LOST.into())])
                .collect(),
        ),
        (State::Transcribing, Event::RecordingStopped(audio)) => transcribe(&audio),
        (State::Transcribing, Event::Transcribed(text)) if is_blank(&text) => {
            (State::Idle, vec![Effect::SetTray(TrayStatus::Idle)])
        }

        (State::Idle, Event::Input(InputEvent::HotkeyPressed(Hotkey::Rewrite))) => {
            (State::Rewriting, vec![Effect::ReadSelection])
        }
        (State::Rewriting, Event::SelectionRead(Some(text))) if !is_blank(&text) => (
            state,
            vec![
                Effect::SetTray(TrayStatus::Rewriting),
                Effect::Rewrite(text),
            ],
        ),
        (State::Rewriting, Event::SelectionRead(_)) => {
            (State::Idle, vec![Effect::Notify(NO_SELECTION.into())])
        }
        (State::Rewriting, Event::Rewritten { rewritten, .. }) if is_blank(&rewritten) => (
            State::Idle,
            vec![
                Effect::SetTray(TrayStatus::Idle),
                Effect::Notify(EMPTY_REWRITE.into()),
            ],
        ),
        (
            State::Rewriting,
            Event::Rewritten {
                original,
                rewritten,
            },
        ) => (
            state,
            vec![Effect::ReplaceSelection {
                original,
                replacement: rewritten,
            }],
        ),
        (State::Rewriting, Event::SelectionReplaced(Replacement::Replaced)) => {
            (State::Idle, vec![Effect::SetTray(TrayStatus::Idle)])
        }
        (State::Rewriting, Event::SelectionReplaced(Replacement::Copied)) => (
            State::Idle,
            vec![
                Effect::SetTray(TrayStatus::Idle),
                Effect::Notify(COPIED.into()),
            ],
        ),
        (State::Transcribing, Event::Transcribed(text)) => (State::Idle, insert(text)),

        (State::Idle, Event::Input(InputEvent::TimerFired(Timer::ClearError))) => {
            (state, vec![Effect::SetTray(TrayStatus::Idle)])
        }

        (state, _) => (state, Vec::new()),
    }
}

fn is_blank(text: &str) -> bool {
    text.trim().is_empty()
}

fn stop() -> Vec<Effect> {
    vec![
        Effect::StopRecording,
        Effect::PlayCue(Cue::Stop),
        Effect::SetTray(TrayStatus::Transcribing),
    ]
}

/// Transcribes the speech in `audio`. An accidental tap is dropped quietly, and a
/// clip with no speech is never sent to Whisper, which would invent words for it.
fn transcribe(audio: &Audio) -> (State, Vec<Effect>) {
    if audio::is_too_short(audio) {
        return (State::Idle, vec![Effect::SetTray(TrayStatus::Idle)]);
    }
    audio::trim_silence(audio).map_or_else(
        || {
            (
                State::Idle,
                vec![
                    Effect::SetTray(TrayStatus::Idle),
                    Effect::Notify(NO_SPEECH.into()),
                ],
            )
        },
        |speech| (State::Transcribing, vec![Effect::Transcribe(speech)]),
    )
}

fn insert(text: String) -> Vec<Effect> {
    vec![Effect::InsertText(text), Effect::SetTray(TrayStatus::Idle)]
}

/// Drops the work in progress without telling the user, who asked for it.
fn cancel(state: State) -> Vec<Effect> {
    let discard = (state == State::Recording).then_some(Effect::DiscardRecording);
    discard
        .into_iter()
        .chain([Effect::SetTray(TrayStatus::Idle)])
        .collect()
}

/// Flags the error on the tray, tells the user, and clears the flag after a moment.
fn report(error: &FacadeError) -> Vec<Effect> {
    vec![
        Effect::SetTray(TrayStatus::Error),
        Effect::Notify(error.to_string()),
        Effect::StartTimer(Timer::ClearError),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATES: [State; 4] = [
        State::Idle,
        State::Recording,
        State::Transcribing,
        State::Rewriting,
    ];

    fn input(event: InputEvent) -> Event {
        Event::Input(event)
    }

    fn hotkey(hotkey: Hotkey) -> Event {
        input(InputEvent::HotkeyPressed(hotkey))
    }

    fn tray(status: TrayStatus) -> Effect {
        Effect::SetTray(status)
    }

    /// One second of loud audio: speech as far as the silence check is concerned.
    fn audio() -> Audio {
        Audio {
            samples: vec![0.5; 16_000],
            sample_rate: 16_000,
        }
    }

    fn rewritten(text: &str) -> Event {
        Event::Rewritten {
            original: "draft".into(),
            rewritten: text.into(),
        }
    }

    fn all_events() -> Vec<Event> {
        vec![
            hotkey(Hotkey::Dictate),
            hotkey(Hotkey::Rewrite),
            hotkey(Hotkey::Cancel),
            input(InputEvent::HotkeyReleased(Hotkey::Dictate)),
            input(InputEvent::HotkeyReleased(Hotkey::Cancel)),
            input(InputEvent::TimerFired(Timer::MaxRecording)),
            input(InputEvent::TimerFired(Timer::ClearError)),
            input(InputEvent::MicrophoneLost),
            input(InputEvent::QuitRequested),
            Event::RecordingStopped(audio()),
            Event::RecordingStopped(Audio {
                samples: vec![0.0; 16_000],
                sample_rate: 16_000,
            }),
            Event::Transcribed("text".into()),
            Event::Transcribed(" ".into()),
            Event::SelectionRead(Some("text".into())),
            Event::SelectionRead(Some("\n".into())),
            Event::SelectionRead(None),
            rewritten("text"),
            rewritten(""),
            Event::SelectionReplaced(Replacement::Replaced),
            Event::SelectionReplaced(Replacement::Copied),
            Event::Failed(FacadeError::Rewrite("boom".into())),
            Event::Failed(FacadeError::Cancelled),
        ]
    }

    fn check(cases: Vec<(State, Event, State, Vec<Effect>)>) {
        cases
            .into_iter()
            .for_each(|(state, event, expected_state, expected_effects)| {
                assert_eq!(
                    next(state, event.clone()),
                    (expected_state, expected_effects),
                    "{state:?} + {event:?}"
                );
            });
    }

    fn stopped() -> Vec<Effect> {
        vec![
            Effect::StopRecording,
            Effect::PlayCue(Cue::Stop),
            tray(TrayStatus::Transcribing),
        ]
    }

    #[test]
    fn follows_the_dictation_transitions() {
        check(vec![
            (
                State::Idle,
                hotkey(Hotkey::Dictate),
                State::Recording,
                vec![
                    tray(TrayStatus::Recording),
                    Effect::PlayCue(Cue::Start),
                    Effect::StartRecording,
                    Effect::StartTimer(Timer::MaxRecording),
                    Effect::ListenForCancel(true),
                ],
            ),
            (
                State::Recording,
                hotkey(Hotkey::Dictate),
                State::Transcribing,
                stopped(),
            ),
            (
                State::Transcribing,
                Event::RecordingStopped(audio()),
                State::Transcribing,
                vec![Effect::Transcribe(audio())],
            ),
            (
                State::Transcribing,
                Event::Transcribed("hi".into()),
                State::Idle,
                vec![
                    Effect::InsertText("hi".into()),
                    tray(TrayStatus::Idle),
                    Effect::ListenForCancel(false),
                ],
            ),
            (
                State::Transcribing,
                Event::Transcribed("  \n".into()),
                State::Idle,
                vec![tray(TrayStatus::Idle), Effect::ListenForCancel(false)],
            ),
            (
                State::Recording,
                input(InputEvent::QuitRequested),
                State::Recording,
                vec![Effect::DiscardRecording, Effect::Exit],
            ),
            (
                State::Idle,
                input(InputEvent::QuitRequested),
                State::Idle,
                vec![Effect::Exit],
            ),
        ]);
    }

    #[test]
    fn recording_stops_on_release_timeout_or_a_lost_microphone() {
        check(vec![
            (
                State::Recording,
                input(InputEvent::HotkeyReleased(Hotkey::Dictate)),
                State::Transcribing,
                stopped(),
            ),
            (
                State::Recording,
                input(InputEvent::TimerFired(Timer::MaxRecording)),
                State::Transcribing,
                stopped(),
            ),
            (
                State::Recording,
                input(InputEvent::MicrophoneLost),
                State::Transcribing,
                stopped()
                    .into_iter()
                    .chain([Effect::Notify(MICROPHONE_LOST.into())])
                    .collect(),
            ),
        ]);
    }

    #[test]
    fn late_timers_and_releases_are_ignored() {
        let late = [
            (State::Idle, InputEvent::TimerFired(Timer::MaxRecording)),
            (
                State::Transcribing,
                InputEvent::TimerFired(Timer::MaxRecording),
            ),
            (State::Recording, InputEvent::TimerFired(Timer::ClearError)),
            (State::Rewriting, InputEvent::TimerFired(Timer::ClearError)),
            (State::Idle, InputEvent::HotkeyReleased(Hotkey::Dictate)),
            (
                State::Recording,
                InputEvent::HotkeyReleased(Hotkey::Rewrite),
            ),
            (State::Idle, InputEvent::MicrophoneLost),
            (State::Idle, InputEvent::HotkeyPressed(Hotkey::Cancel)),
        ];

        late.into_iter().for_each(|(state, event)| {
            assert_eq!(
                next(state, input(event.clone())),
                (state, Vec::new()),
                "{state:?} + {event:?}"
            );
        });
    }

    #[test]
    fn silent_or_tiny_recordings_are_not_transcribed() {
        let silent = Audio {
            samples: vec![0.0; 32_000],
            sample_rate: 16_000,
        };
        let tap = Audio {
            samples: vec![0.5; 100],
            sample_rate: 16_000,
        };
        let padded = Audio {
            samples: [vec![0.0; 16_000], vec![0.5; 16_000], vec![0.0; 16_000]].concat(),
            sample_rate: 16_000,
        };

        check(vec![
            (
                State::Transcribing,
                Event::RecordingStopped(silent),
                State::Idle,
                vec![
                    tray(TrayStatus::Idle),
                    Effect::Notify(NO_SPEECH.into()),
                    Effect::ListenForCancel(false),
                ],
            ),
            (
                State::Transcribing,
                Event::RecordingStopped(tap),
                State::Idle,
                vec![tray(TrayStatus::Idle), Effect::ListenForCancel(false)],
            ),
        ]);
        let (state, effects) = next(State::Transcribing, Event::RecordingStopped(padded));
        assert_eq!(state, State::Transcribing);
        assert!(
            matches!(effects.as_slice(), [Effect::Transcribe(speech)] if speech.samples.len() < 24_000)
        );
    }

    #[test]
    fn esc_cancels_every_busy_state() {
        let cancelled = |state: State, event: Event, discard: bool| {
            let effects = discard
                .then_some(Effect::DiscardRecording)
                .into_iter()
                .chain([tray(TrayStatus::Idle), Effect::ListenForCancel(false)])
                .collect();
            (state, event, State::Idle, effects)
        };
        let cancel = || hotkey(Hotkey::Cancel);
        let interrupted = || Event::Failed(FacadeError::Cancelled);

        check(vec![
            cancelled(State::Recording, cancel(), true),
            cancelled(State::Transcribing, cancel(), false),
            cancelled(State::Rewriting, cancel(), false),
            cancelled(State::Recording, interrupted(), true),
            cancelled(State::Transcribing, interrupted(), false),
            cancelled(State::Rewriting, interrupted(), false),
        ]);
        assert_eq!(
            next(State::Idle, interrupted()),
            (State::Idle, vec![tray(TrayStatus::Idle)])
        );
    }

    #[test]
    fn error_icon_clears_only_when_idle() {
        assert_eq!(
            next(
                State::Idle,
                input(InputEvent::TimerFired(Timer::ClearError))
            ),
            (State::Idle, vec![tray(TrayStatus::Idle)])
        );
    }

    #[test]
    fn follows_the_rewrite_transitions() {
        check(vec![
            (
                State::Idle,
                hotkey(Hotkey::Rewrite),
                State::Rewriting,
                vec![Effect::ReadSelection, Effect::ListenForCancel(true)],
            ),
            (
                State::Rewriting,
                Event::SelectionRead(Some("draft".into())),
                State::Rewriting,
                vec![tray(TrayStatus::Rewriting), Effect::Rewrite("draft".into())],
            ),
            (
                State::Rewriting,
                Event::SelectionRead(None),
                State::Idle,
                vec![
                    Effect::Notify(NO_SELECTION.into()),
                    Effect::ListenForCancel(false),
                ],
            ),
            (
                State::Rewriting,
                Event::SelectionRead(Some(" ".into())),
                State::Idle,
                vec![
                    Effect::Notify(NO_SELECTION.into()),
                    Effect::ListenForCancel(false),
                ],
            ),
            (
                State::Rewriting,
                rewritten("Draft."),
                State::Rewriting,
                vec![Effect::ReplaceSelection {
                    original: "draft".into(),
                    replacement: "Draft.".into(),
                }],
            ),
            (
                State::Rewriting,
                Event::SelectionReplaced(Replacement::Replaced),
                State::Idle,
                vec![tray(TrayStatus::Idle), Effect::ListenForCancel(false)],
            ),
            (
                State::Rewriting,
                Event::SelectionReplaced(Replacement::Copied),
                State::Idle,
                vec![
                    tray(TrayStatus::Idle),
                    Effect::Notify(COPIED.into()),
                    Effect::ListenForCancel(false),
                ],
            ),
            (
                State::Rewriting,
                rewritten(""),
                State::Idle,
                vec![
                    tray(TrayStatus::Idle),
                    Effect::Notify(EMPTY_REWRITE.into()),
                    Effect::ListenForCancel(false),
                ],
            ),
        ]);
    }

    #[test]
    fn failure_reports_and_returns_to_idle_from_every_state() {
        let error = FacadeError::Transcription("no model".into());
        let reported = vec![
            tray(TrayStatus::Error),
            Effect::Notify("transcription: no model".into()),
            Effect::StartTimer(Timer::ClearError),
        ];

        STATES.into_iter().for_each(|state| {
            let discard = (state == State::Recording).then_some(Effect::DiscardRecording);
            let listen = (state != State::Idle).then_some(Effect::ListenForCancel(false));
            let expected: Vec<Effect> = discard
                .into_iter()
                .chain(reported.clone())
                .chain(listen)
                .collect();
            assert_eq!(
                next(state, Event::Failed(error.clone())),
                (State::Idle, expected),
                "{state:?}"
            );
        });
    }

    #[test]
    fn hotkeys_are_ignored_while_busy() {
        let ignored = [
            (State::Recording, Hotkey::Rewrite),
            (State::Transcribing, Hotkey::Dictate),
            (State::Transcribing, Hotkey::Rewrite),
            (State::Rewriting, Hotkey::Dictate),
            (State::Rewriting, Hotkey::Rewrite),
        ];

        ignored.into_iter().for_each(|(state, key)| {
            assert_eq!(next(state, hotkey(key)), (state, Vec::new()), "{state:?}");
        });
    }

    #[test]
    fn stray_results_are_ignored() {
        let stray = [
            (State::Idle, Event::RecordingStopped(audio())),
            (State::Idle, Event::Transcribed("late".into())),
            (State::Recording, Event::SelectionRead(None)),
            (State::Transcribing, rewritten("late")),
            (State::Idle, Event::SelectionReplaced(Replacement::Replaced)),
            (State::Rewriting, Event::Transcribed("late".into())),
        ];

        stray.into_iter().for_each(|(state, event)| {
            assert_eq!(next(state, event), (state, Vec::new()), "{state:?}");
        });
    }

    #[test]
    fn every_pair_settles_in_a_known_state() {
        STATES
            .into_iter()
            .flat_map(|state| all_events().into_iter().map(move |event| (state, event)))
            .for_each(|(state, event)| {
                let (next_state, effects) = next(state, event.clone());
                let exits = effects.last() == Some(&Effect::Exit);
                let listens: Vec<Effect> = effects
                    .iter()
                    .filter(|effect| matches!(effect, Effect::ListenForCancel(_)))
                    .cloned()
                    .collect();
                let busy_changed = (state == State::Idle) != (next_state == State::Idle);
                assert!(
                    STATES.contains(&next_state),
                    "{state:?} + {event:?} -> {next_state:?}"
                );
                assert!(
                    !exits || event == input(InputEvent::QuitRequested),
                    "only quit exits: {state:?} + {event:?}"
                );
                assert_eq!(
                    listens,
                    if busy_changed {
                        vec![Effect::ListenForCancel(next_state != State::Idle)]
                    } else {
                        Vec::new()
                    },
                    "Esc is listened for exactly while busy: {state:?} + {event:?}"
                );
            });
    }

    #[test]
    fn kinds_never_carry_user_content() {
        let secret = "my secret words";
        let events = [
            Event::Transcribed(secret.into()),
            Event::SelectionRead(Some(secret.into())),
            Event::Rewritten {
                original: secret.into(),
                rewritten: secret.into(),
            },
        ];
        let effects = [
            Effect::Rewrite(secret.into()),
            Effect::InsertText(secret.into()),
            Effect::ReplaceSelection {
                original: secret.into(),
                replacement: secret.into(),
            },
            Effect::Notify(secret.into()),
        ];

        events
            .iter()
            .map(Event::kind)
            .chain(effects.iter().map(Effect::kind))
            .for_each(|kind| assert!(!kind.contains("secret"), "{kind}"));
        let all_kinds: Vec<&str> = all_events().iter().map(Event::kind).collect();
        assert!(all_kinds.contains(&"timer-fired"));
    }
}
