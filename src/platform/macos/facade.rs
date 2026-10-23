//! [`Facade`] for macOS, built from the other modules in this directory.

use super::recorder::Recording;
use super::transcriber::Transcriber;
use super::{insert, notify, permissions, MainRequest};
use crate::config::Config;
use crate::facade::{
    Audio, Facade, FacadeError, Hotkey, HotkeyBinding, InputEvent, Result, TrayStatus,
};
use crate::platform::keymap;
use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
use std::sync::mpsc;
use tao::event_loop::EventLoopProxy;

const REWRITE_UNAVAILABLE: &str = "isn't available yet; it arrives in a later version";

pub struct MacFacade {
    proxy: EventLoopProxy<MainRequest>,
    /// Registered hotkey ids and what they trigger.
    hotkeys: Vec<(u32, Hotkey)>,
    recording: Option<Recording>,
    transcriber: Transcriber,
}

impl MacFacade {
    pub fn new(config: &Config, proxy: EventLoopProxy<MainRequest>) -> Self {
        Self {
            proxy,
            hotkeys: Vec::new(),
            recording: None,
            transcriber: Transcriber::new(&config.dictation),
        }
    }

    /// Starts loading the speech model in the background; fails at once if it's missing.
    pub fn preload_model(&mut self) -> Result<()> {
        self.transcriber.preload()
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
        self.proxy
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
        GlobalHotKeyEvent::receiver()
            .iter()
            .filter(|event| event.state() == HotKeyState::Pressed)
            .find_map(|event| {
                self.hotkeys
                    .iter()
                    .find(|(id, _)| *id == event.id())
                    .map(|(_, hotkey)| InputEvent::HotkeyPressed(*hotkey))
            })
            .ok_or_else(|| FacadeError::Hotkeys("the hotkey listener stopped".into()))
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
        Err(FacadeError::Rewrite(format!(
            "Rewrite {REWRITE_UNAVAILABLE}"
        )))
    }

    fn rewrite(&mut self, _text: &str) -> Result<String> {
        Err(FacadeError::Rewrite(format!(
            "Rewrite {REWRITE_UNAVAILABLE}"
        )))
    }

    fn insert_text(&mut self, text: &str) -> Result<()> {
        permissions::check_accessibility()?;
        insert::paste(text)
    }

    fn set_tray_status(&mut self, status: TrayStatus) -> Result<()> {
        // The tray icon arrives in M4; until then, report state in the terminal.
        eprintln!("fluentb: {status:?}");
        Ok(())
    }

    fn notify(&mut self, message: &str) -> Result<()> {
        notify::show(message);
        Ok(())
    }
}
