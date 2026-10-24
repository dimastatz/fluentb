//! Which models are ready, and what to tell the user about them.
//!
//! Each hotkey needs its own model: dictation works as soon as the small speech model
//! is ready, without waiting for the much larger rewrite model.

use super::state::{Event, State};
use crate::facade::{Hotkey, InputEvent, ModelStatus};
use crate::models::Kind;

/// The status of each kind of model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Models {
    pub speech: ModelStatus,
    pub rewrite: ModelStatus,
}

/// How each hotkey is written, for messages such as "press ⇧⌘D".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyLabels {
    pub dictate: String,
    pub rewrite: String,
}

impl Models {
    /// Both models ready.
    #[must_use]
    pub const fn ready() -> Self {
        Self {
            speech: ModelStatus::Ready,
            rewrite: ModelStatus::Ready,
        }
    }

    #[must_use]
    pub const fn get(&self, kind: Kind) -> &ModelStatus {
        match kind {
            Kind::Speech => &self.speech,
            Kind::Rewrite => &self.rewrite,
        }
    }

    #[must_use]
    pub fn with(self, kind: Kind, status: ModelStatus) -> Self {
        match kind {
            Kind::Speech => Self {
                speech: status,
                ..self
            },
            Kind::Rewrite => Self {
                rewrite: status,
                ..self
            },
        }
    }

    /// A message explaining why `event` can't start yet, or `None` if it can. Only a
    /// hotkey that would start something new is held back.
    #[must_use]
    pub fn blocked(&self, state: State, event: &Event) -> Option<String> {
        let kind = match (state, event) {
            (State::Idle, Event::Input(InputEvent::HotkeyPressed(Hotkey::Dictate))) => Kind::Speech,
            (State::Idle, Event::Input(InputEvent::HotkeyPressed(Hotkey::Rewrite))) => {
                Kind::Rewrite
            }
            _ => return None,
        };
        match self.get(kind) {
            ModelStatus::Ready => None,
            ModelStatus::Downloading { percent } => Some(format!(
                "The {} is still downloading ({percent}%). {} will work as soon as it's ready.",
                name(kind),
                feature(kind)
            )),
            ModelStatus::Failed(reason) => Some(failed(kind, reason)),
        }
    }
}

/// What to tell the user when a model's status changes from `before` to `after`.
#[must_use]
pub fn announce(
    kind: Kind,
    before: &ModelStatus,
    after: &ModelStatus,
    labels: &HotkeyLabels,
) -> Option<String> {
    match (before, after, kind) {
        (ModelStatus::Ready, _, _)
        | (_, ModelStatus::Downloading { .. }, _)
        | (ModelStatus::Failed(_), ModelStatus::Failed(_), _) => None,
        (_, ModelStatus::Ready, Kind::Speech) => Some(format!(
            "Dictation is ready: press {}, speak, then press it again.",
            labels.dictate
        )),
        (_, ModelStatus::Ready, Kind::Rewrite) => Some(format!(
            "Rewrite is ready: select text and press {}.",
            labels.rewrite
        )),
        (_, ModelStatus::Failed(reason), _) => Some(failed(kind, reason)),
    }
}

const fn name(kind: Kind) -> &'static str {
    match kind {
        Kind::Speech => "speech model",
        Kind::Rewrite => "rewrite model",
    }
}

const fn feature(kind: Kind) -> &'static str {
    match kind {
        Kind::Speech => "Dictation",
        Kind::Rewrite => "Rewrite",
    }
}

fn failed(kind: Kind, reason: &str) -> String {
    format!(
        "Couldn't download the {}: {reason}. fluentb will try again the next time it starts.",
        name(kind)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels() -> HotkeyLabels {
        HotkeyLabels {
            dictate: "⇧⌘D".into(),
            rewrite: "⇧⌘R".into(),
        }
    }

    fn hotkey(hotkey: Hotkey) -> Event {
        Event::Input(InputEvent::HotkeyPressed(hotkey))
    }

    #[test]
    fn each_hotkey_waits_only_for_its_own_model() {
        let models = Models::ready().with(Kind::Rewrite, ModelStatus::Downloading { percent: 10 });

        assert_eq!(models.blocked(State::Idle, &hotkey(Hotkey::Dictate)), None);
        assert_eq!(
            models.blocked(State::Idle, &hotkey(Hotkey::Rewrite)).as_deref(),
            Some("The rewrite model is still downloading (10%). Rewrite will work as soon as it's ready.")
        );
    }

    #[test]
    fn explains_a_failed_download() {
        let models = Models::ready().with(Kind::Speech, ModelStatus::Failed("offline".into()));

        assert_eq!(
            models.blocked(State::Idle, &hotkey(Hotkey::Dictate)).as_deref(),
            Some("Couldn't download the speech model: offline. fluentb will try again the next time it starts.")
        );
    }

    #[test]
    fn never_blocks_finishing_something_already_started() {
        let models = Models {
            speech: ModelStatus::Downloading { percent: 0 },
            rewrite: ModelStatus::Downloading { percent: 0 },
        };

        assert_eq!(
            models.blocked(State::Recording, &hotkey(Hotkey::Dictate)),
            None
        );
        assert_eq!(
            models.blocked(State::Idle, &Event::Input(InputEvent::QuitRequested)),
            None
        );
        assert_eq!(
            models.blocked(State::Idle, &Event::Transcribed("hi".into())),
            None
        );
    }

    #[test]
    fn announces_readiness_and_failures_once() {
        let downloading = ModelStatus::Downloading { percent: 99 };
        let failed = ModelStatus::Failed("disk full".into());

        assert_eq!(
            announce(Kind::Speech, &downloading, &ModelStatus::Ready, &labels()).as_deref(),
            Some("Dictation is ready: press ⇧⌘D, speak, then press it again.")
        );
        assert_eq!(
            announce(Kind::Rewrite, &failed, &ModelStatus::Ready, &labels()).as_deref(),
            Some("Rewrite is ready: select text and press ⇧⌘R.")
        );
        assert_eq!(
            announce(Kind::Rewrite, &downloading, &failed, &labels()).as_deref(),
            Some("Couldn't download the rewrite model: disk full. fluentb will try again the next time it starts.")
        );
        assert_eq!(announce(Kind::Speech, &failed, &failed, &labels()), None);
        assert_eq!(
            announce(Kind::Speech, &downloading, &downloading, &labels()),
            None
        );
        assert_eq!(
            announce(
                Kind::Speech,
                &ModelStatus::Ready,
                &ModelStatus::Ready,
                &labels()
            ),
            None
        );
        assert_eq!(
            announce(Kind::Speech, &ModelStatus::Ready, &downloading, &labels()),
            None,
            "switching models starts a download quietly"
        );
    }

    #[test]
    fn reads_and_updates_each_kind() {
        let models = Models::ready().with(Kind::Speech, ModelStatus::Downloading { percent: 5 });

        assert_eq!(
            models.get(Kind::Speech),
            &ModelStatus::Downloading { percent: 5 }
        );
        assert_eq!(models.get(Kind::Rewrite), &ModelStatus::Ready);
    }
}
