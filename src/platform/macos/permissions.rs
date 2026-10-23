//! Microphone and Accessibility permission checks.

use crate::facade::{FacadeError, Result};
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};
use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

/// Fails if the user has denied microphone access. When access hasn't been decided
/// yet, opening the microphone shows the system prompt.
pub fn check_microphone() -> Result<()> {
    // SAFETY: AVMediaTypeAudio is an immutable framework constant, and
    // authorizationStatusForMediaType accepts it per Apple's documentation.
    let status = unsafe {
        AVMediaTypeAudio.map(|media| AVCaptureDevice::authorizationStatusForMediaType(media))
    };
    match status {
        Some(AVAuthorizationStatus::Denied | AVAuthorizationStatus::Restricted) => {
            Err(FacadeError::Microphone(
                "access is denied. Allow it in System Settings → Privacy & Security → \
                 Microphone, then try again"
                    .into(),
            ))
        }
        _ => Ok(()),
    }
}

/// Fails, and asks macOS to show its prompt, if fluentb can't send keystrokes.
pub fn check_accessibility() -> Result<()> {
    // SAFETY: kAXTrustedCheckOptionPrompt is an immutable framework constant, and the
    // options dictionary outlives the call.
    let trusted = unsafe {
        let prompt = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let options = CFDictionary::from_CFType_pairs(&[(prompt, CFBoolean::true_value())]);
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef())
    };
    if trusted {
        Ok(())
    } else {
        Err(FacadeError::TextAccess(
            "fluentb needs Accessibility access to type text. Allow it in System Settings → \
             Privacy & Security → Accessibility, then try again"
                .into(),
        ))
    }
}
