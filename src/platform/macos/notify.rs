//! User-visible notifications via `osascript`, which works without an app bundle.

use crate::platform::applescript;
use std::process::{Command, Stdio};

/// Shows `message` as a notification and echoes it to the terminal. Best effort.
pub fn show(message: &str) {
    eprintln!("fluentb: {message}");
    let _ = Command::new("osascript")
        .arg("-e")
        .arg(applescript::notification("fluentb", message))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}
