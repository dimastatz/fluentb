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

mod downloads;
mod facade;
mod insert;
mod notify;
mod permissions;
mod recorder;
mod rewriter;
mod selection;
mod transcriber;
mod tray;

use crate::accelerator::Accelerator;
use crate::app::{self, HotkeyLabels, Models};
use crate::config::{self, Config, ConfigWarning};
use crate::facade::{ModelStatus, TrayStatus};
use crate::logging;
use crate::models::{self, Kind, Model};
use crate::onboarding;
use crate::platform::bridge::Signal;
use crate::platform::instance::{self, Instance};
use crate::platform::{keymap, login, Os};
use crate::tray::menu::{self, MenuState};
use facade::{MacFacade, Setup};
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::fs::{self, File};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tray::Tray;
use tray_icon::menu::MenuEvent;

/// How often to recheck permissions while one is missing, and otherwise.
const PERMISSION_POLL_MISSING: Duration = Duration::from_secs(2);
const PERMISSION_POLL_GRANTED: Duration = Duration::from_secs(30);

/// Requests the worker and download threads send to the main thread.
pub(crate) enum MainRequest {
    /// Replace the registered hotkeys, replying with the outcome.
    RegisterHotkeys {
        hotkeys: Vec<HotKey>,
        reply: mpsc::Sender<Result<(), String>>,
    },
    /// Show a new status in the menu bar.
    SetStatus(TrayStatus),
    /// The download in progress, for the status line.
    Download(Option<(Kind, u8)>),
    /// The models in use changed, so the menu's check marks must too.
    Models {
        speech: Model,
        rewrite: Model,
    },
    /// Launch at login was turned on or off.
    LaunchAtLogin(bool),
    Exit(i32),
}

/// Runs fluentb until the user quits or a fatal error occurs.
pub fn run() -> ! {
    whisper_rs::install_logging_hooks();
    // Held until the process exits, so buffered log lines keep being written.
    let _log = logging::default_dir().and_then(|dir| {
        logging::init(&dir)
            .map_err(|error| eprintln!("fluentb: logging is off: {error}"))
            .ok()
    });
    let config_path = config::default_path();
    // Held until the process exits; the OS releases the lock then.
    let _instance = single_instance(config_path.as_deref());
    let (config, warnings) = config::load(config_path.as_deref(), Os::current());
    let speech = models::choose_whisper(config.dictation.model);
    let rewrite = models::choose_rewrite(config.rewrite.model);
    let models_dir = models::default_dir();
    let pending = models::pending(
        &[Model::Whisper(speech), Model::Rewrite(rewrite)],
        |model| is_installed(models_dir.as_deref(), model),
    );
    let initial = initial_models(&pending);

    let (signals_sender, signals) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    forward_signals(&signals_sender, &cancel);
    let mut event_loop = EventLoopBuilder::<MainRequest>::with_user_event().build();
    // A menu-bar app: no Dock icon, no main window.
    event_loop.set_activation_policy(ActivationPolicy::Accessory);
    let proxy = event_loop.create_proxy();
    let downloads = downloads::spawn(models_dir.clone(), signals_sender.clone(), proxy.clone());
    pending.iter().for_each(|model| {
        let _ = downloads.send(*model);
    });
    onboard(&config, config_path.as_deref(), &pending);

    let menu_config = config.clone();
    let setup = Setup {
        config_path,
        signals,
        signals_sender,
        cancel,
        proxy: proxy.clone(),
        speech,
        rewrite,
        downloads,
        models_dir,
    };
    thread::spawn(move || {
        let code = run_worker(&config, &warnings, setup, initial);
        let _ = proxy.send_event(MainRequest::Exit(code));
    });

    let mut main = MainLoop {
        manager: GlobalHotKeyManager::new().map_err(|error| error.to_string()),
        registered: Vec::new(),
        tray: None,
        menu_config,
        state: MenuState {
            status: TrayStatus::Idle,
            download: None,
            speech: Model::Whisper(speech),
            rewrite: Model::Rewrite(rewrite),
            missing: permissions::missing(),
            launch_at_login: launch_agent_path().is_some_and(|path| path.exists()),
        },
    };
    event_loop.run(move |event, _, control_flow| {
        let poll = if main.state.missing.is_empty() {
            PERMISSION_POLL_GRANTED
        } else {
            PERMISSION_POLL_MISSING
        };
        *control_flow = ControlFlow::WaitUntil(Instant::now() + poll);
        main.handle(event);
    })
}

/// What the main thread owns: hotkey registration and the menu-bar icon.
struct MainLoop {
    manager: Result<GlobalHotKeyManager, String>,
    registered: Vec<HotKey>,
    tray: Option<Tray>,
    menu_config: Config,
    state: MenuState,
}

impl MainLoop {
    fn handle(&mut self, event: Event<MainRequest>) {
        let rebuild = match event {
            Event::NewEvents(StartCause::Init) => {
                let model = menu::menu_model(&self.menu_config, &self.state, Os::MacOs);
                match Tray::new(&model, self.state.status) {
                    Ok(created) => self.tray = Some(created),
                    Err(error) => {
                        notify::show(&format!("fluentb couldn't add its menu-bar icon: {error}"));
                        std::process::exit(1);
                    }
                }
                false
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                let missing = permissions::missing();
                let changed = missing != self.state.missing;
                self.state.missing = missing;
                changed
            }
            Event::UserEvent(request) => self.request(request),
            _ => false,
        };
        if rebuild {
            let model = menu::menu_model(&self.menu_config, &self.state, Os::MacOs);
            if let Some(Err(error)) = self.tray.as_mut().map(|tray| tray.rebuild(&model)) {
                eprintln!("fluentb: couldn't update the menu: {error}");
            }
        }
    }

    /// Carries out a request from another thread; returns whether the menu changed
    /// shape and must be rebuilt.
    fn request(&mut self, request: MainRequest) -> bool {
        match request {
            MainRequest::RegisterHotkeys { hotkeys, reply } => {
                // Only the difference is (un)registered: re-registering a hotkey the
                // user is holding would lose its release, which push-to-talk needs.
                let (stale, kept): (Vec<HotKey>, Vec<HotKey>) = self
                    .registered
                    .iter()
                    .partition(|hotkey| !hotkeys.contains(hotkey));
                let fresh: Vec<HotKey> = hotkeys
                    .iter()
                    .filter(|hotkey| !kept.contains(hotkey))
                    .copied()
                    .collect();
                let result = self
                    .manager
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|manager| {
                        let _ = manager.unregister_all(&stale);
                        manager
                            .register_all(&fresh)
                            .map_err(|error| error.to_string())
                    });
                self.registered = if result.is_ok() { hotkeys } else { kept };
                let _ = reply.send(result);
                false
            }
            MainRequest::SetStatus(status) => {
                self.state.status = status;
                let line = menu::status_line(status, self.state.download);
                if let Some(Err(error)) = self.tray.as_ref().map(|tray| tray.show(status, &line)) {
                    eprintln!("fluentb: couldn't update the menu-bar icon: {error}");
                }
                false
            }
            MainRequest::Download(download) => {
                self.state.download = download;
                if let Some(tray) = self.tray.as_ref() {
                    tray.set_status_line(&menu::status_line(self.state.status, download));
                }
                false
            }
            MainRequest::Models { speech, rewrite } => {
                self.state.speech = speech;
                self.state.rewrite = rewrite;
                true
            }
            MainRequest::LaunchAtLogin(enabled) => {
                self.state.launch_at_login = enabled;
                true
            }
            MainRequest::Exit(code) => std::process::exit(code),
        }
    }
}

/// Where the launch-at-login agent goes.
pub(crate) fn launch_agent_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(|home| login::agent_path(Path::new(&home)))
}

/// Whether `model` is fully downloaded into `dir`.
fn is_installed(dir: Option<&Path>, model: Model) -> bool {
    let size = dir
        .and_then(|dir| fs::metadata(dir.join(model.file_name())).ok())
        .map(|metadata| metadata.len());
    models::is_complete(model, size)
}

/// Each model is ready unless it still needs downloading.
fn initial_models(pending: &[Model]) -> Models {
    let status = |kind: Kind| {
        if pending.iter().any(|model| model.kind() == kind) {
            ModelStatus::Downloading { percent: 0 }
        } else {
            ModelStatus::Ready
        }
    };
    Models {
        speech: status(Kind::Speech),
        rewrite: status(Kind::Rewrite),
    }
}

/// On first run: welcome the user and ask for both permissions up front, so a
/// missing permission doesn't surprise them on their first hotkey press.
fn onboard(config: &Config, config_path: Option<&Path>, pending: &[Model]) {
    let Some(marker) = config_path.map(onboarding::marker_path) else {
        return;
    };
    if marker.exists() {
        return;
    }
    let labels = HotkeyLabels {
        dictate: config.hotkeys.dictate.display(Os::MacOs),
        rewrite: config.hotkeys.rewrite.display(Os::MacOs),
    };
    notify::show(&onboarding::welcome(models::total_size(pending), &labels));
    permissions::request_microphone();
    permissions::request_accessibility();
    if let Err(error) = fs::write(&marker, "") {
        eprintln!("fluentb: couldn't record that onboarding ran: {error}");
    }
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
/// Esc also sets `cancel` at once, since the worker may be busy transcribing or
/// rewriting and won't read the channel until it's done.
fn forward_signals(signals: &mpsc::Sender<Signal>, cancel: &Arc<AtomicBool>) {
    let hotkeys = signals.clone();
    let cancel = Arc::clone(cancel);
    let cancel_id = keymap::to_hotkey(&Accelerator::esc()).map(|hotkey| hotkey.id());
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        let pressed = event.state() == HotKeyState::Pressed;
        if pressed && Some(event.id()) == cancel_id {
            cancel.store(true, Ordering::Relaxed);
        }
        let _ = hotkeys.send(Signal::Hotkey {
            id: event.id(),
            pressed,
        });
    }));
    let menu = signals.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let _ = menu.send(Signal::Menu(event.id.0));
    }));
}

/// Runs the state manager; returns the process exit code.
fn run_worker(config: &Config, warnings: &[ConfigWarning], setup: Setup, models: Models) -> i32 {
    let speech_ready = models.speech == ModelStatus::Ready;
    let mut facade = MacFacade::new(config, setup);
    if speech_ready {
        if let Err(error) = facade.preload_model() {
            notify::show(&error.to_string());
        }
    }
    eprintln!("fluentb is running in the menu bar.");
    // Effects already catch their own panics; this catches one while reading input,
    // so the user hears about it instead of being left with dead hotkeys. Returning
    // drops the facade, which closes the microphone.
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        app::run_app(&mut facade, config, warnings, models)
    }));
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            tracing::error!(error = error.kind(), "stopped");
            notify::show(&format!("fluentb stopped: {error}"));
            1
        }
        Err(_) => {
            tracing::error!("worker panicked");
            notify::show("fluentb stopped because of an internal error. Please restart it.");
            1
        }
    }
}
