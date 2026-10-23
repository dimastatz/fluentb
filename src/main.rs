//! The fluentb binary. It only starts the platform the build targets; the logic lives
//! in the library.

fn main() -> std::process::ExitCode {
    fluentb::platform::run()
}
