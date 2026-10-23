//! The macOS app: a Facade implementation plus the entry point that wires it up.
//!
//! This module is OS glue only, so it is excluded from coverage and checked by hand
//! (see `docs/specs/m03-macos-dictation.md`). Anything that can be pure lives in the
//! tested modules (`audio`, `transcript`, `models`, `platform::keymap`, ...).
//!
//! Threads: macOS needs global hotkeys and the menu-bar icon on the main thread, so
//! the main thread runs the event loop and the state manager runs on a worker. Hotkey
//! and menu events reach the worker through one channel (`platform::bridge`); the
//! worker reaches the main thread through `MainRequest`s.
#![allow(unsafe_code)]

mod facade;
mod insert;
mod notify;
mod permissions;
mod recorder;
mod transcriber;
mod tray;

use crate::app;
use crate::config;
use crate::facade::TrayStatus;
use crate::platform::bridge::Signal;
use crate::platform::instance::{self, Instance};
use crate::platform::Os;
use crate::tray::menu;
use facade::MacFacade;
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tray::Tray;
use tray_icon::menu::MenuEvent;

/// Requests the worker thread sends to the main thread.
pub(crate) enum MainRequest {
    /// Replace the registered hotkeys, replying with the outcome.
    RegisterHotkeys {
        hotkeys: Vec<HotKey>,
        reply: mpsc::Sender<Result<(), String>>,
    },
    /// Show a new status in the menu bar.
    SetStatus(TrayStatus),
    Exit(i32),
}

/// Runs fluentb until the user quits or a fatal error occurs.
pub fn run() -> ! {
    whisper_rs::install_logging_hooks();
    let config_path = config::default_path();
    // Held until the process exits; the OS releases the lock then.
    let _instance = single_instance(config_path.as_deref());
    let (config, warnings) = config::load(config_path.as_deref(), Os::current());
    let menu = menu::menu_model(&config, TrayStatus::Idle, Os::MacOs);
    let (signals_sender, signals) = mpsc::channel();
    forward_signals(&signals_sender);

    let mut event_loop = EventLoopBuilder::<MainRequest>::with_user_event().build();
    // A menu-bar app: no Dock icon, no main window.
    event_loop.set_activation_policy(ActivationPolicy::Accessory);
    let proxy = event_loop.create_proxy();
    thread::spawn(move || {
        let code = run_worker(&config, &warnings, config_path, signals, proxy.clone());
        let _ = proxy.send_event(MainRequest::Exit(code));
    });

    let manager = GlobalHotKeyManager::new().map_err(|error| error.to_string());
    let mut registered: Vec<HotKey> = Vec::new();
    let mut tray: Option<Tray> = None;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(StartCause::Init) => match Tray::new(&menu, TrayStatus::Idle) {
                Ok(created) => tray = Some(created),
                Err(error) => {
                    notify::show(&format!("fluentb couldn't add its menu-bar icon: {error}"));
                    std::process::exit(1);
                }
            },
            Event::UserEvent(MainRequest::RegisterHotkeys { hotkeys, reply }) => {
                let result = manager.as_ref().map_err(Clone::clone).and_then(|manager| {
                    let _ = manager.unregister_all(&registered);
                    manager
                        .register_all(&hotkeys)
                        .map_err(|error| error.to_string())
                });
                if result.is_ok() {
                    registered = hotkeys;
                }
                let _ = reply.send(result);
            }
            Event::UserEvent(MainRequest::SetStatus(status)) => {
                if let Some(Err(error)) = tray.as_ref().map(|tray| tray.show(status)) {
                    eprintln!("fluentb: couldn't update the menu-bar icon: {error}");
                }
            }
            Event::UserEvent(MainRequest::Exit(code)) => std::process::exit(code),
            _ => {}
        }
    })
}

/// Takes the single-instance lock, or exits if another fluentb is running. Runs
/// without the lock if it can't be taken at all.
fn single_instance(config_path: Option<&Path>) -> Option<File> {
    match config_path.map(|path| instance::acquire(&instance::lock_path(path)))? {
        Ok(Instance::First(file)) => Some(file),
        Ok(Instance::AlreadyRunning) => {
            notify::show("fluentb is already running.");
            std::process::exit(1)
        }
        Err(error) => {
            eprintln!("fluentb: couldn't check for another running copy: {error}");
            None
        }
    }
}

/// Sends hotkey and menu events, which arrive on the main thread, to the worker.
fn forward_signals(signals: &mpsc::Sender<Signal>) {
    let hotkeys = signals.clone();
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        let _ = hotkeys.send(Signal::Hotkey {
            id: event.id(),
            pressed: event.state() == HotKeyState::Pressed,
        });
    }));
    let menu = signals.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let _ = menu.send(Signal::Menu(event.id.0));
    }));
}

/// Runs the state manager; returns the process exit code.
fn run_worker(
    config: &config::Config,
    warnings: &[config::ConfigWarning],
    config_path: Option<PathBuf>,
    signals: mpsc::Receiver<Signal>,
    proxy: EventLoopProxy<MainRequest>,
) -> i32 {
    let mut facade = MacFacade::new(config, config_path, signals, proxy);
    if let Err(error) = facade.preload_model() {
        notify::show(&error.to_string());
    }
    eprintln!("fluentb is running in the menu bar.");
    match app::run_app(&mut facade, config, warnings) {
        Ok(()) => 0,
        Err(error) => {
            notify::show(&format!("fluentb stopped: {error}"));
            1
        }
    }
}
