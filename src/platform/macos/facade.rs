//! [`Facade`] for macOS, built from the other modules in this directory.

use super::recorder::Recording;
use super::rewriter::Rewriter;
use super::transcriber::Transcriber;
use super::{insert, notify, permissions, selection, MainRequest};
use crate::config::{self, Config, Language, Rewrite, RewriteModel, WhisperModel};
use crate::download::format_size;
use crate::facade::{
    Audio, Facade, FacadeError, Hotkey, HotkeyBinding, InputEvent, ModelStatus, Replacement,
    Result, TrayStatus,
};
use crate::models::{self, Kind, Model};
use crate::platform::bridge::{self, Route, Signal, Wait};
use crate::platform::keymap;
use crate::tray::menu::MenuAction;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use tao::event_loop::EventLoopProxy;

/// Everything the facade needs from startup.
pub struct Setup {
    pub config_path: Option<PathBuf>,
    /// Hotkey, menu and download events.
    pub signals: mpsc::Receiver<Signal>,
    pub proxy: EventLoopProxy<MainRequest>,
    pub speech: WhisperModel,
    pub rewrite: RewriteModel,
    /// Queues a model for the download thread.
    pub downloads: mpsc::Sender<Model>,
    pub models_dir: Option<PathBuf>,
}

pub struct MacFacade {
    setup: Setup,
    language: Language,
    rewrite_config: Rewrite,
    /// Registered hotkey ids and what they trigger.
    hotkeys: Vec<(u32, Hotkey)>,
    recording: Option<Recording>,
    transcriber: Transcriber,
    rewriter: Rewriter,
}

impl MacFacade {
    pub fn new(config: &Config, setup: Setup) -> Self {
        Self {
            transcriber: Transcriber::new(setup.speech, &config.dictation.language),
            rewriter: Rewriter::new(setup.rewrite, &config.rewrite),
            language: config.dictation.language.clone(),
            rewrite_config: config.rewrite,
            setup,
            hotkeys: Vec::new(),
            recording: None,
        }
    }

    /// Starts loading the speech model in the background; fails at once if it's missing.
    pub fn preload_model(&mut self) -> Result<()> {
        self.transcriber.preload()
    }

    /// Carries out a menu action the core isn't involved in. Switching models returns
    /// the new model's status for the core.
    fn act(&mut self, action: MenuAction) -> Option<InputEvent> {
        match action {
            MenuAction::OpenConfig => open_config(self.setup.config_path.as_deref()),
            MenuAction::FixPermission(permission) => permissions::open_settings(permission),
            MenuAction::RemoveUnusedModels => self.remove_unused_models(),
            MenuAction::SelectModel(model) => return Some(self.select(model)),
            MenuAction::Quit => {}
        }
        None
    }

    /// Switches to `model`: saves it in the config, swaps the engine, and downloads
    /// the model if it isn't on disk yet.
    fn select(&mut self, model: Model) -> InputEvent {
        let (section, kind) = match model {
            Model::Whisper(whisper) => {
                self.setup.speech = whisper;
                self.transcriber = Transcriber::new(whisper, &self.language);
                ("dictation", Kind::Speech)
            }
            Model::Rewrite(rewrite) => {
                self.setup.rewrite = rewrite;
                self.rewriter = Rewriter::new(rewrite, &self.rewrite_config);
                ("rewrite", Kind::Rewrite)
            }
        };
        if let Err(reason) = save_choice(self.setup.config_path.as_deref(), section, model) {
            notify::show(&format!("Couldn't save the model choice: {reason}"));
        }
        let _ = self.setup.proxy.send_event(MainRequest::Models {
            speech: Model::Whisper(self.setup.speech),
            rewrite: Model::Rewrite(self.setup.rewrite),
        });
        let status = if self.is_installed(model) {
            self.ready(model);
            ModelStatus::Ready
        } else {
            let _ = self.setup.downloads.send(model);
            ModelStatus::Downloading { percent: 0 }
        };
        InputEvent::ModelChanged(kind, status)
    }

    /// Passes a download update to the core if `model` is still the one in use.
    fn update(&mut self, model: Model, status: ModelStatus) -> Option<InputEvent> {
        if !self.in_use().contains(&model) {
            return None;
        }
        if status == ModelStatus::Ready {
            self.ready(model);
        }
        Some(InputEvent::ModelChanged(model.kind(), status))
    }

    /// Starts loading a speech model that just became available.
    fn ready(&mut self, model: Model) {
        if model.kind() == Kind::Speech {
            if let Err(error) = self.transcriber.preload() {
                notify::show(&error.to_string());
            }
        }
    }

    fn in_use(&self) -> [Model; 2] {
        [
            Model::Whisper(self.setup.speech),
            Model::Rewrite(self.setup.rewrite),
        ]
    }

    fn is_installed(&self, model: Model) -> bool {
        let size = self
            .setup
            .models_dir
            .as_ref()
            .and_then(|dir| fs::metadata(dir.join(model.file_name())).ok())
            .map(|metadata| metadata.len());
        models::is_complete(model, size)
    }

    /// Deletes downloaded models, and unfinished downloads, that aren't in use.
    fn remove_unused_models(&self) {
        let Some(dir) = self.setup.models_dir.as_deref() else {
            return;
        };
        let files: Vec<String> = fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default();
        let removed: Vec<u64> = models::unused_files(&files, &self.in_use())
            .iter()
            .filter_map(|name| {
                let path = dir.join(name);
                let size = fs::metadata(&path).map(|metadata| metadata.len()).ok()?;
                fs::remove_file(&path).ok().map(|()| size)
            })
            .collect();
        notify::show(&if removed.is_empty() {
            "There are no unused models to remove.".to_owned()
        } else {
            format!(
                "Removed {} unused model file{}, freeing {}.",
                removed.len(),
                if removed.len() == 1 { "" } else { "s" },
                format_size(removed.iter().sum())
            )
        });
    }
}

impl Facade for MacFacade {
    fn register_hotkeys(&mut self, bindings: &[HotkeyBinding]) -> Result<()> {
        let mapped = bindings
            .iter()
            .map(|binding| {
                keymap::to_hotkey(&binding.accelerator)
                    .map(|hotkey| (hotkey, binding.hotkey))
                    .ok_or_else(|| {
                        FacadeError::Hotkeys(format!("{:?} can't be registered", binding.hotkey))
                    })
            })
            .collect::<Result<Vec<_>>>()?;
        let (reply, response) = mpsc::channel();
        self.setup
            .proxy
            .send_event(MainRequest::RegisterHotkeys {
                hotkeys: mapped.iter().map(|(hotkey, _)| *hotkey).collect(),
                reply,
            })
            .map_err(|_| FacadeError::Hotkeys("the event loop has stopped".into()))?;
        response
            .recv()
            .map_err(|_| FacadeError::Hotkeys("the event loop has stopped".into()))?
            .map_err(FacadeError::Hotkeys)?;
        self.hotkeys = mapped
            .into_iter()
            .map(|(hotkey, action)| (hotkey.id(), action))
            .collect();
        Ok(())
    }

    fn next_event(&mut self) -> Result<InputEvent> {
        // Waits for input, waking up to unload the rewrite model once it goes idle.
        loop {
            let deadline = self.rewriter.time_until_unload();
            match bridge::next_route_within(&self.setup.signals, &self.hotkeys, deadline) {
                Wait::Route(Route::Input(event)) => return Ok(event),
                Wait::Route(Route::Action(action)) => {
                    if let Some(event) = self.act(action) {
                        return Ok(event);
                    }
                }
                Wait::Route(Route::Model(model, status)) => {
                    if let Some(event) = self.update(model, status) {
                        return Ok(event);
                    }
                }
                Wait::TimedOut => self.rewriter.unload_if_idle(),
                Wait::Closed => {
                    return Err(FacadeError::Hotkeys("the event listener stopped".into()))
                }
            }
        }
    }

    fn start_recording(&mut self) -> Result<()> {
        permissions::check_microphone()?;
        self.recording = Some(Recording::start()?);
        Ok(())
    }

    fn stop_recording(&mut self) -> Result<Audio> {
        self.recording
            .take()
            .map(Recording::stop)
            .ok_or_else(|| FacadeError::Microphone("not recording".into()))
    }

    fn transcribe(&mut self, audio: &Audio) -> Result<String> {
        self.transcriber.transcribe(audio)
    }

    fn selected_text(&mut self) -> Result<Option<String>> {
        permissions::check_accessibility()?;
        selection::read()
    }

    fn rewrite(&mut self, text: &str) -> Result<String> {
        self.rewriter.rewrite(text)
    }

    fn replace_selection(&mut self, original: &str, replacement: &str) -> Result<Replacement> {
        permissions::check_accessibility()?;
        if selection::read()?.as_deref() == Some(original) {
            insert::paste(replacement)?;
            Ok(Replacement::Replaced)
        } else {
            insert::copy(replacement)?;
            Ok(Replacement::Copied)
        }
    }

    fn insert_text(&mut self, text: &str) -> Result<()> {
        permissions::check_accessibility()?;
        insert::paste(text)
    }

    fn set_tray_status(&mut self, status: TrayStatus) -> Result<()> {
        self.setup
            .proxy
            .send_event(MainRequest::SetStatus(status))
            .map_err(|_| FacadeError::Tray("the event loop has stopped".into()))
    }

    fn notify(&mut self, message: &str) -> Result<()> {
        notify::show(message);
        Ok(())
    }
}

/// Writes `model` into the config file's `[section]`, keeping the rest of the file.
fn save_choice(
    path: Option<&Path>,
    section: &str,
    model: Model,
) -> std::result::Result<(), String> {
    let path = path.ok_or("couldn't find the config file (is HOME set?)")?;
    let text = fs::read_to_string(path).unwrap_or_default();
    fs::write(
        path,
        config::set_string(&text, section, "model", &model.id()),
    )
    .map_err(|error| error.to_string())
}

/// Opens the config file in the default text editor, reporting failures.
fn open_config(path: Option<&Path>) {
    let result = path
        .ok_or_else(|| "couldn't find the config file (is HOME set?)".to_owned())
        .and_then(|path| {
            Command::new("open")
                .arg("-t")
                .arg(path)
                .status()
                .map_err(|error| error.to_string())
        })
        .and_then(|status| {
            status
                .success()
                .then_some(())
                .ok_or_else(|| format!("`open` failed with {status}"))
        });
    if let Err(reason) = result {
        notify::show(&format!("Couldn't open the config: {reason}"));
    }
}
