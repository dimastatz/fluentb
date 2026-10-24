//! Reads the text selected in the focused app.
//!
//! First through the Accessibility API, which leaves the clipboard alone. Apps that
//! don't expose their text that way (browsers, some Electron apps) fall back to
//! pressing Cmd+C and reading the clipboard, which is then restored.

use super::insert;
use crate::facade::{FacadeError, Result};
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::string::{CFString, CFStringRef};
use std::ffi::c_void;
use std::thread;
use std::time::{Duration, Instant};

type AXUIElementRef = *const c_void;
type AXError = i32;
const AX_SUCCESS: AXError = 0;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> AXError;
}

/// How long the app gets to put the copied text on the clipboard.
const COPY_TIMEOUT: Duration = Duration::from_millis(500);
const COPY_POLL: Duration = Duration::from_millis(20);

/// The selected text, or `None` if nothing is selected.
pub fn read() -> Result<Option<String>> {
    match accessibility_selection().filter(|text| !text.is_empty()) {
        Some(text) => Ok(Some(text)),
        None => copied_selection(),
    }
}

/// The selection as the Accessibility API reports it, if the app exposes it.
fn accessibility_selection() -> Option<String> {
    // SAFETY: every CF object is created or copied here and wrapped under the create
    // rule, so it is released exactly once; null results are checked first.
    unsafe {
        let system = wrap(AXUIElementCreateSystemWide())?;
        if let Some(app) = copy_attribute(&system, "AXFocusedApplication") {
            // Electron apps only build their accessibility tree when asked to.
            let attribute = CFString::new("AXManualAccessibility");
            let _ = AXUIElementSetAttributeValue(
                app.as_CFTypeRef(),
                attribute.as_concrete_TypeRef(),
                CFBoolean::true_value().as_CFTypeRef(),
            );
        }
        let focused = copy_attribute(&system, "AXFocusedUIElement")?;
        copy_attribute(&focused, "AXSelectedText")?
            .downcast::<CFString>()
            .map(|text| text.to_string())
    }
}

/// # Safety
/// `element` must be a valid `AXUIElement`.
unsafe fn copy_attribute(element: &CFType, name: &str) -> Option<CFType> {
    let attribute = CFString::new(name);
    let mut value: CFTypeRef = std::ptr::null();
    let error = AXUIElementCopyAttributeValue(
        element.as_CFTypeRef(),
        attribute.as_concrete_TypeRef(),
        &raw mut value,
    );
    if error == AX_SUCCESS {
        wrap(value)
    } else {
        None
    }
}

/// # Safety
/// `reference` must be null or a CF object the caller owns.
unsafe fn wrap(reference: CFTypeRef) -> Option<CFType> {
    (!reference.is_null()).then(|| CFType::wrap_under_create_rule(reference))
}

/// Presses Cmd+C and reads what lands on the clipboard, restoring the clipboard text.
fn copied_selection() -> Result<Option<String>> {
    let mut clipboard = arboard::Clipboard::new().map_err(text_access)?;
    let previous = clipboard.get_text().ok();
    clipboard.clear().map_err(text_access)?;
    insert::press_command(insert::KEY_C)?;
    let deadline = Instant::now() + COPY_TIMEOUT;
    let copied = std::iter::from_fn(|| {
        (Instant::now() < deadline).then(|| {
            thread::sleep(COPY_POLL);
            clipboard.get_text().ok().filter(|text| !text.is_empty())
        })
    })
    .flatten()
    .next();
    match previous {
        Some(previous) => clipboard.set_text(previous),
        None => clipboard.clear(),
    }
    .map_err(text_access)?;
    Ok(copied)
}

fn text_access(error: impl std::fmt::Display) -> FacadeError {
    FacadeError::TextAccess(error.to_string())
}
