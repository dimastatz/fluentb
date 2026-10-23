//! Types text into the focused app by pasting it, then restores the clipboard.

use crate::facade::{FacadeError, Result};
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use std::thread;
use std::time::Duration;

/// The virtual key code for V on macOS keyboards.
const KEY_V: CGKeyCode = 9;
/// How long the target app gets to read the clipboard before it is restored.
const PASTE_SETTLE: Duration = Duration::from_millis(250);

/// Pastes `text` into the focused app. The previous clipboard text is restored
/// afterwards; non-text clipboard contents (images, files) are not preserved.
pub fn paste(text: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new().map_err(text_access)?;
    let previous = clipboard.get_text().ok();
    clipboard.set_text(text).map_err(text_access)?;
    press_command_v()?;
    thread::sleep(PASTE_SETTLE);
    match previous {
        Some(previous) => clipboard.set_text(previous),
        None => clipboard.clear(),
    }
    .map_err(text_access)
}

fn press_command_v() -> Result<()> {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| FacadeError::TextAccess("couldn't create a keyboard event".into()))?;
    [true, false].into_iter().try_for_each(|key_down| {
        let event = CGEvent::new_keyboard_event(source.clone(), KEY_V, key_down)
            .map_err(|()| FacadeError::TextAccess("couldn't create a keyboard event".into()))?;
        // Exactly Command, so modifiers still held from the hotkey don't leak in.
        event.set_flags(CGEventFlags::CGEventFlagCommand);
        event.post(CGEventTapLocation::HID);
        Ok(())
    })
}

fn text_access(error: impl std::fmt::Display) -> FacadeError {
    FacadeError::TextAccess(error.to_string())
}
