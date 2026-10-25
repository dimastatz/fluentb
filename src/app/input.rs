//! How the Dictate hotkey behaves in each dictation mode.
//!
//! The transition table stops a recording on either a press or a release of the
//! Dictate hotkey; [`accepts`] lets through only the one the mode uses.

use super::state::State;
use crate::config::DictationMode;
use crate::facade::{Hotkey, InputEvent};

/// Whether `event` means anything in `mode`: toggle mode ignores key releases, and
/// push-to-talk ignores pressing Dictate again while it is held.
#[must_use]
pub fn accepts(mode: DictationMode, state: State, event: &InputEvent) -> bool {
    match (mode, event) {
        (DictationMode::Toggle, InputEvent::HotkeyReleased(_)) => false,
        (DictationMode::PushToTalk, InputEvent::HotkeyPressed(Hotkey::Dictate)) => {
            state != State::Recording
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRESS: InputEvent = InputEvent::HotkeyPressed(Hotkey::Dictate);
    const RELEASE: InputEvent = InputEvent::HotkeyReleased(Hotkey::Dictate);

    #[test]
    fn toggle_mode_ignores_releases() {
        [State::Idle, State::Recording]
            .into_iter()
            .for_each(|state| {
                assert!(accepts(DictationMode::Toggle, state, &PRESS));
                assert!(!accepts(DictationMode::Toggle, state, &RELEASE));
            });
        assert!(!accepts(
            DictationMode::Toggle,
            State::Recording,
            &InputEvent::HotkeyReleased(Hotkey::Cancel)
        ));
    }

    #[test]
    fn push_to_talk_starts_on_press_and_stops_on_release() {
        assert!(accepts(DictationMode::PushToTalk, State::Idle, &PRESS));
        assert!(!accepts(
            DictationMode::PushToTalk,
            State::Recording,
            &PRESS
        ));
        assert!(accepts(
            DictationMode::PushToTalk,
            State::Recording,
            &RELEASE
        ));
        assert!(accepts(
            DictationMode::PushToTalk,
            State::Recording,
            &InputEvent::HotkeyPressed(Hotkey::Cancel)
        ));
    }
}
