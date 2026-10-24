//! Microphone and Accessibility permissions: checking, requesting, and opening the
//! right System Settings pane.

use crate::facade::{FacadeError, Permission, Result};
use block2::RcBlock;
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};
use objc2::runtime::Bool;
use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
use std::process::Command;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

/// Fails if the user has denied microphone access. When access hasn't been decided
/// yet, opening the microphone shows the system prompt.
pub fn check_microphone() -> Result<()> {
    if microphone_denied() {
        Err(FacadeError::Microphone(
            "access is denied. Allow it in System Settings → Privacy & Security → \
             Microphone, then try again"
                .into(),
        ))
    } else {
        Ok(())
    }
}

/// Fails, and asks macOS to show its prompt, if fluentb can't send keystrokes.
pub fn check_accessibility() -> Result<()> {
    if request_accessibility() {
        Ok(())
    } else {
        Err(FacadeError::TextAccess(
            "fluentb needs Accessibility access to read and type text. Allow it in System \
             Settings → Privacy & Security → Accessibility, then try again"
                .into(),
        ))
    }
}

/// Permissions that are missing right now, without prompting. Microphone access that
/// hasn't been asked for yet doesn't count: the first recording asks.
pub fn missing() -> Vec<Permission> {
    // SAFETY: AXIsProcessTrusted takes no arguments and only reads process state.
    let trusted = unsafe { AXIsProcessTrusted() };
    [
        (Permission::Accessibility, !trusted),
        (Permission::Microphone, microphone_denied()),
    ]
    .into_iter()
    .filter_map(|(permission, missing)| missing.then_some(permission))
    .collect()
}

/// Shows the microphone prompt if the user hasn't decided yet. Doesn't wait.
pub fn request_microphone() {
    if microphone_status() == Some(AVAuthorizationStatus::NotDetermined) {
        let done = RcBlock::new(|_granted: Bool| {});
        // SAFETY: AVMediaTypeAudio is an immutable framework constant, and the block
        // is retained by AVFoundation until it has been called.
        unsafe {
            if let Some(media) = AVMediaTypeAudio {
                AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &done);
            }
        }
    }
}

/// Whether fluentb may control the keyboard, showing the system prompt if not.
pub fn request_accessibility() -> bool {
    // SAFETY: kAXTrustedCheckOptionPrompt is an immutable framework constant, and the
    // options dictionary outlives the call.
    unsafe {
        let prompt = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let options = CFDictionary::from_CFType_pairs(&[(prompt, CFBoolean::true_value())]);
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef())
    }
}

/// Opens System Settings where `permission` is granted.
pub fn open_settings(permission: Permission) {
    let pane = match permission {
        Permission::Microphone => "Privacy_Microphone",
        Permission::Accessibility => "Privacy_Accessibility",
    };
    let _ = Command::new("open")
        .arg(format!(
            "x-apple.systempreferences:com.apple.preference.security?{pane}"
        ))
        .status();
}

fn microphone_denied() -> bool {
    matches!(
        microphone_status(),
        Some(AVAuthorizationStatus::Denied | AVAuthorizationStatus::Restricted)
    )
}

fn microphone_status() -> Option<AVAuthorizationStatus> {
    // SAFETY: AVMediaTypeAudio is an immutable framework constant, and
    // authorizationStatusForMediaType accepts it per Apple's documentation.
    unsafe { AVMediaTypeAudio.map(|media| AVCaptureDevice::authorizationStatusForMediaType(media)) }
}
