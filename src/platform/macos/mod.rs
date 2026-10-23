//! The macOS app: a Facade implementation plus the entry point that wires it up.
//!
//! This module is OS glue only, so it is excluded from coverage and checked by hand
//! (see `docs/specs/m03-macos-dictation.md`). Anything that can be pure lives in the
//! tested modules (`audio`, `transcript`, `models`, `platform::keymap`, ...).
//!
//! Threads: macOS needs global hotkeys registered and delivered on the main thread,
//! so the main thread runs the event loop and the state manager runs on a worker.
#![allow(unsafe_code)]

mod facade;
mod insert;
mod notify;
mod permissions;
mod recorder;
mod transcriber;

use crate::app;
use crate::config;
use crate::platform::Os;
use facade::MacFacade;
use global_hotkey::hotkey::HotKey;
use global_hotkey::GlobalHotKeyManager;
use std::sync::mpsc;
use std::thread;
use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};

/// Requests the worker thread sends to the main thread.
pub(crate) enum MainRequest {
    /// Replace the registered hotkeys, replying with the outcome.
    RegisterHotkeys {
        hotkeys: Vec<HotKey>,
        reply: mpsc::Sender<Result<(), String>>,
    },
    Exit(i32),
}

/// Runs fluentb until the user quits or a fatal error occurs.
pub fn run() -> ! {
    whisper_rs::install_logging_hooks();
    let (config, warnings) = config::load(config::default_path().as_deref(), Os::current());

    let mut event_loop = EventLoopBuilder::<MainRequest>::with_user_event().build();
    // A menu-bar style app: no Dock icon, no main window.
    event_loop.set_activation_policy(ActivationPolicy::Accessory);
    let proxy = event_loop.create_proxy();
    thread::spawn(move || {
        let code = run_worker(&config, &warnings, proxy.clone());
        let _ = proxy.send_event(MainRequest::Exit(code));
    });

    let manager = GlobalHotKeyManager::new().map_err(|error| error.to_string());
    let mut registered: Vec<HotKey> = Vec::new();
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
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
            Event::UserEvent(MainRequest::Exit(code)) => std::process::exit(code),
            _ => {}
        }
    })
}

/// Runs the state manager; returns the process exit code.
fn run_worker(
    config: &config::Config,
    warnings: &[config::ConfigWarning],
    proxy: EventLoopProxy<MainRequest>,
) -> i32 {
    let mut facade = MacFacade::new(config, proxy);
    if let Err(error) = facade.preload_model() {
        notify::show(&error.to_string());
    }
    eprintln!("fluentb is running. Press Ctrl+C to quit.");
    match app::run_app(&mut facade, config, warnings) {
        Ok(()) => 0,
        Err(error) => {
            notify::show(&format!("fluentb stopped: {error}"));
            1
        }
    }
}
