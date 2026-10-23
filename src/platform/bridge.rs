//! Merges hotkey and tray-menu events into the single channel the core reads.
//!
//! Platform event handlers only forward raw [`Signal`]s; [`route`] decides what each
//! one means, so the mapping is tested without a GUI.

use crate::facade::{Hotkey, InputEvent};
use crate::tray::menu::MenuAction;
use std::sync::mpsc::Receiver;

/// A raw event from the platform's hotkey or menu handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    Hotkey { id: u32, pressed: bool },
    Menu(String),
}

/// What a signal asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// An event for the core.
    Input(InputEvent),
    /// Open the config file; the adapter does this itself, the core isn't involved.
    OpenConfig,
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
            MenuAction::OpenConfig => Route::OpenConfig,
            MenuAction::Quit => Route::Input(InputEvent::QuitRequested),
        }),
    }
}

/// Blocks until a signal routes somewhere, or returns `None` once every sender is gone.
#[must_use]
pub fn next_route(signals: &Receiver<Signal>, hotkeys: &[(u32, Hotkey)]) -> Option<Route> {
    signals.iter().find_map(|signal| route(&signal, hotkeys))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    const HOTKEYS: [(u32, Hotkey); 2] = [(7, Hotkey::Dictate), (9, Hotkey::Rewrite)];

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
            (Signal::Menu("open-config".into()), Some(Route::OpenConfig)),
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
