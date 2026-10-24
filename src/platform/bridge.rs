//! Merges hotkey and tray-menu events into the single channel the core reads.
//!
//! Platform event handlers only forward raw [`Signal`]s; [`route`] decides what each
//! one means, so the mapping is tested without a GUI.

use crate::facade::{Hotkey, InputEvent, ModelStatus};
use crate::models::Model;
use crate::tray::menu::MenuAction;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// A raw event from the platform's hotkey or menu handler, or from the downloader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    Hotkey {
        id: u32,
        pressed: bool,
    },
    Menu(String),
    /// A download's progress or result.
    Model(Model, ModelStatus),
}

/// What a signal asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// An event for the core.
    Input(InputEvent),
    /// A menu action the adapter carries out itself (open the config, switch or remove
    /// models, open System Settings); the core isn't involved.
    Action(MenuAction),
    /// A download update. The adapter passes it to the core only if `Model` is still
    /// the one in use, so a superseded download can't mark the wrong model ready.
    Model(Model, ModelStatus),
}

/// Maps `signal` given the registered hotkey ids, or `None` if it should be ignored
/// (a key release, or an id fluentb didn't register).
#[must_use]
pub fn route(signal: &Signal, hotkeys: &[(u32, Hotkey)]) -> Option<Route> {
    match signal {
        Signal::Hotkey { pressed: false, .. } => None,
        Signal::Hotkey { id, .. } => hotkeys
            .iter()
            .find(|(registered, _)| registered == id)
            .map(|(_, hotkey)| Route::Input(InputEvent::HotkeyPressed(*hotkey))),
        Signal::Menu(id) => MenuAction::from_id(id).map(|action| match action {
            MenuAction::Quit => Route::Input(InputEvent::QuitRequested),
            other => Route::Action(other),
        }),
        Signal::Model(model, status) => Some(Route::Model(*model, status.clone())),
    }
}

/// Blocks until a signal routes somewhere, or returns `None` once every sender is gone.
#[must_use]
pub fn next_route(signals: &Receiver<Signal>, hotkeys: &[(u32, Hotkey)]) -> Option<Route> {
    signals.iter().find_map(|signal| route(&signal, hotkeys))
}

/// What waiting for a signal with a deadline led to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wait {
    Route(Route),
    /// The deadline passed first, so time-based work (unloading a model) can run.
    TimedOut,
    /// Every sender is gone.
    Closed,
}

/// Like [`next_route`], but gives up after `timeout`; `None` waits forever.
#[must_use]
pub fn next_route_within(
    signals: &Receiver<Signal>,
    hotkeys: &[(u32, Hotkey)],
    timeout: Option<Duration>,
) -> Wait {
    let Some(timeout) = timeout else {
        return next_route(signals, hotkeys).map_or(Wait::Closed, Wait::Route);
    };
    let deadline = Instant::now() + timeout;
    std::iter::from_fn(|| {
        let left = deadline.saturating_duration_since(Instant::now());
        Some(match signals.recv_timeout(left) {
            Ok(signal) => route(&signal, hotkeys).map(Wait::Route),
            Err(RecvTimeoutError::Timeout) => Some(Wait::TimedOut),
            Err(RecvTimeoutError::Disconnected) => Some(Wait::Closed),
        })
    })
    .flatten()
    .next()
    .unwrap_or(Wait::Closed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    const HOTKEYS: [(u32, Hotkey); 2] = [(7, Hotkey::Dictate), (9, Hotkey::Rewrite)];
    const QWEN: Model = Model::Rewrite(crate::config::RewriteModel::Qwen3_4B);

    #[test]
    fn routes_signals() {
        let cases = [
            (
                Signal::Hotkey {
                    id: 7,
                    pressed: true,
                },
                Some(Route::Input(InputEvent::HotkeyPressed(Hotkey::Dictate))),
            ),
            (
                Signal::Hotkey {
                    id: 9,
                    pressed: true,
                },
                Some(Route::Input(InputEvent::HotkeyPressed(Hotkey::Rewrite))),
            ),
            (
                Signal::Hotkey {
                    id: 7,
                    pressed: false,
                },
                None,
            ),
            (
                Signal::Hotkey {
                    id: 8,
                    pressed: true,
                },
                None,
            ),
            (
                Signal::Menu("quit".into()),
                Some(Route::Input(InputEvent::QuitRequested)),
            ),
            (
                Signal::Menu("open-config".into()),
                Some(Route::Action(MenuAction::OpenConfig)),
            ),
            (
                Signal::Menu("remove-unused-models".into()),
                Some(Route::Action(MenuAction::RemoveUnusedModels)),
            ),
            (
                Signal::Model(QWEN, ModelStatus::Downloading { percent: 3 }),
                Some(Route::Model(QWEN, ModelStatus::Downloading { percent: 3 })),
            ),
            (Signal::Menu("about".into()), None),
        ];

        cases
            .iter()
            .for_each(|(signal, expected)| assert_eq!(route(signal, &HOTKEYS), *expected));
    }

    #[test]
    fn skips_ignored_signals_until_one_routes() {
        let (sender, signals) = mpsc::channel();
        [
            Signal::Hotkey {
                id: 7,
                pressed: false,
            },
            Signal::Menu("about".into()),
            Signal::Menu("quit".into()),
        ]
        .into_iter()
        .for_each(|signal| sender.send(signal).unwrap());

        assert_eq!(
            next_route(&signals, &HOTKEYS),
            Some(Route::Input(InputEvent::QuitRequested))
        );
    }

    #[test]
    fn waits_with_a_deadline() {
        let (sender, signals) = mpsc::channel();
        let short = Some(Duration::from_millis(10));

        assert_eq!(next_route_within(&signals, &HOTKEYS, short), Wait::TimedOut);

        [Signal::Menu("about".into()), Signal::Menu("quit".into())]
            .into_iter()
            .for_each(|signal| sender.send(signal).unwrap());
        assert_eq!(
            next_route_within(&signals, &HOTKEYS, short),
            Wait::Route(Route::Input(InputEvent::QuitRequested)),
            "ignored signals don't end the wait"
        );

        sender.send(Signal::Menu("open-config".into())).unwrap();
        assert_eq!(
            next_route_within(&signals, &HOTKEYS, None),
            Wait::Route(Route::Action(MenuAction::OpenConfig))
        );

        drop(sender);
        assert_eq!(next_route_within(&signals, &HOTKEYS, short), Wait::Closed);
        assert_eq!(next_route_within(&signals, &HOTKEYS, None), Wait::Closed);
    }

    #[test]
    fn signals_from_another_thread_arrive_in_order() {
        let (sender, signals) = mpsc::channel();
        std::thread::spawn(move || {
            sender
                .send(Signal::Hotkey {
                    id: 7,
                    pressed: true,
                })
                .unwrap();
            sender
                .send(Signal::Hotkey {
                    id: 7,
                    pressed: true,
                })
                .unwrap();
        });

        let dictate = Some(Route::Input(InputEvent::HotkeyPressed(Hotkey::Dictate)));
        assert_eq!(next_route(&signals, &HOTKEYS), dictate);
        assert_eq!(next_route(&signals, &HOTKEYS), dictate);
        assert_eq!(next_route(&signals, &HOTKEYS), None, "senders gone");
    }
}
