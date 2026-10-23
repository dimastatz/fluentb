//! The pure transition function at the heart of fluentb.

use super::effect::Effect;
use super::state::{Event, State};
use crate::facade::{FacadeError, Hotkey, InputEvent, TrayStatus};

pub const NO_SELECTION: &str = "Select some text to rewrite";
pub const EMPTY_REWRITE: &str = "Rewrite returned no text; your text was left unchanged";

/// Decides the next state and the effects to run for `event` in `state`.
///
/// Total over every `(state, event)` pair: anything not listed (a hotkey while busy,
/// a stray result) leaves the state unchanged and does nothing.
pub fn next(state: State, event: Event) -> (State, Vec<Effect>) {
    match (state, event) {
        (State::Recording, Event::Input(InputEvent::QuitRequested)) => {
            (state, vec![Effect::DiscardRecording, Effect::Exit])
        }
        (_, Event::Input(InputEvent::QuitRequested)) => (state, vec![Effect::Exit]),

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
                Effect::StartRecording,
            ],
        ),
        (State::Recording, Event::Input(InputEvent::HotkeyPressed(Hotkey::Dictate))) => (
            State::Transcribing,
            vec![
                Effect::StopRecording,
                Effect::SetTray(TrayStatus::Transcribing),
            ],
        ),
        (State::Transcribing, Event::RecordingStopped(audio)) => {
            (state, vec![Effect::Transcribe(audio)])
        }
        (State::Transcribing, Event::Transcribed(text)) if is_blank(&text) => {
            (State::Idle, vec![Effect::SetTray(TrayStatus::Idle)])
        }
        (State::Transcribing, Event::Transcribed(text)) => (State::Idle, insert(text)),

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
        (State::Rewriting, Event::Rewritten(text)) if is_blank(&text) => (
            State::Idle,
            vec![
                Effect::SetTray(TrayStatus::Idle),
                Effect::Notify(EMPTY_REWRITE.into()),
            ],
        ),
        (State::Rewriting, Event::Rewritten(text)) => (State::Idle, insert(text)),

        (state, _) => (state, Vec::new()),
    }
}

fn is_blank(text: &str) -> bool {
    text.trim().is_empty()
}

fn insert(text: String) -> Vec<Effect> {
    vec![Effect::InsertText(text), Effect::SetTray(TrayStatus::Idle)]
}

/// Flags the error on the tray, tells the user, and settles back to idle.
fn report(error: &FacadeError) -> Vec<Effect> {
    vec![
        Effect::SetTray(TrayStatus::Error),
        Effect::Notify(error.to_string()),
        Effect::SetTray(TrayStatus::Idle),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facade::Audio;

    const STATES: [State; 4] = [
        State::Idle,
        State::Recording,
        State::Transcribing,
        State::Rewriting,
    ];

    fn hotkey(hotkey: Hotkey) -> Event {
        Event::Input(InputEvent::HotkeyPressed(hotkey))
    }

    fn audio() -> Audio {
        Audio {
            samples: vec![0.5; 4],
            sample_rate: 16_000,
        }
    }

    fn all_events() -> Vec<Event> {
        vec![
            hotkey(Hotkey::Dictate),
            hotkey(Hotkey::Rewrite),
            Event::Input(InputEvent::QuitRequested),
            Event::RecordingStopped(audio()),
            Event::Transcribed("text".into()),
            Event::Transcribed(" ".into()),
            Event::SelectionRead(Some("text".into())),
            Event::SelectionRead(Some("\n".into())),
            Event::SelectionRead(None),
            Event::Rewritten("text".into()),
            Event::Rewritten("".into()),
            Event::Failed(FacadeError::Rewrite("boom".into())),
        ]
    }

    #[test]
    fn follows_the_transition_table() {
        let tray = |status| Effect::SetTray(status);
        let cases = [
            (
                State::Idle,
                hotkey(Hotkey::Dictate),
                State::Recording,
                vec![tray(TrayStatus::Recording), Effect::StartRecording],
            ),
            (
                State::Recording,
                hotkey(Hotkey::Dictate),
                State::Transcribing,
                vec![Effect::StopRecording, tray(TrayStatus::Transcribing)],
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
                vec![Effect::InsertText("hi".into()), tray(TrayStatus::Idle)],
            ),
            (
                State::Transcribing,
                Event::Transcribed("  \n".into()),
                State::Idle,
                vec![tray(TrayStatus::Idle)],
            ),
            (
                State::Idle,
                hotkey(Hotkey::Rewrite),
                State::Rewriting,
                vec![Effect::ReadSelection],
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
                vec![Effect::Notify(NO_SELECTION.into())],
            ),
            (
                State::Rewriting,
                Event::SelectionRead(Some(" ".into())),
                State::Idle,
                vec![Effect::Notify(NO_SELECTION.into())],
            ),
            (
                State::Rewriting,
                Event::Rewritten("Draft.".into()),
                State::Idle,
                vec![Effect::InsertText("Draft.".into()), tray(TrayStatus::Idle)],
            ),
            (
                State::Rewriting,
                Event::Rewritten("".into()),
                State::Idle,
                vec![tray(TrayStatus::Idle), Effect::Notify(EMPTY_REWRITE.into())],
            ),
            (
                State::Recording,
                Event::Input(InputEvent::QuitRequested),
                State::Recording,
                vec![Effect::DiscardRecording, Effect::Exit],
            ),
            (
                State::Idle,
                Event::Input(InputEvent::QuitRequested),
                State::Idle,
                vec![Effect::Exit],
            ),
        ];

        cases
            .into_iter()
            .for_each(|(state, event, expected_state, expected_effects)| {
                assert_eq!(
                    next(state, event.clone()),
                    (expected_state, expected_effects),
                    "{state:?} + {event:?}"
                )
            });
    }

    #[test]
    fn failure_reports_and_returns_to_idle_from_every_state() {
        let error = FacadeError::Transcription("no model".into());
        let reported = vec![
            Effect::SetTray(TrayStatus::Error),
            Effect::Notify("transcription: no model".into()),
            Effect::SetTray(TrayStatus::Idle),
        ];

        STATES.into_iter().for_each(|state| {
            let expected = match state {
                State::Recording => std::iter::once(Effect::DiscardRecording)
                    .chain(reported.clone())
                    .collect(),
                _ => reported.clone(),
            };
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
            assert_eq!(next(state, hotkey(key)), (state, Vec::new()), "{state:?}")
        });
    }

    #[test]
    fn stray_results_are_ignored() {
        let stray = [
            (State::Idle, Event::RecordingStopped(audio())),
            (State::Idle, Event::Transcribed("late".into())),
            (State::Recording, Event::SelectionRead(None)),
            (State::Transcribing, Event::Rewritten("late".into())),
            (State::Rewriting, Event::Transcribed("late".into())),
        ];

        stray.into_iter().for_each(|(state, event)| {
            assert_eq!(next(state, event), (state, Vec::new()), "{state:?}")
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
                assert!(
                    STATES.contains(&next_state),
                    "{state:?} + {event:?} -> {next_state:?}"
                );
                assert!(
                    !exits || event == Event::Input(InputEvent::QuitRequested),
                    "only quit exits: {state:?} + {event:?}"
                );
            });
    }
}
