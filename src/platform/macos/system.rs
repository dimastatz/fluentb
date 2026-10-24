//! Facts about this Mac used to choose default models.

use crate::models::System;
use crate::platform::system::{first_language, memory_bytes};
use std::process::Command;

/// The preferred language and the amount of memory, where they can be read.
pub fn detect() -> System {
    System {
        language: output("defaults", &["read", "-g", "AppleLanguages"])
            .and_then(|text| first_language(&text)),
        memory_bytes: output("sysctl", &["-n", "hw.memsize"]).and_then(|text| memory_bytes(&text)),
    }
}

fn output(program: &str, args: &[&str]) -> Option<String> {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
}
