//! The fluentb binary. Platform glue only; the logic lives in the library.

#[cfg(target_os = "macos")]
fn main() {
    fluentb::platform::macos::run()
}

#[cfg(not(target_os = "macos"))]
fn main() -> std::process::ExitCode {
    eprintln!("fluentb currently supports macOS only; Windows and Linux are planned.");
    std::process::ExitCode::FAILURE
}
