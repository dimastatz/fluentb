//! Operating-system support: all platform-dependent code lives under this module.
//!
//! Each OS gets an adapter in its own submodule (`macos`, later `windows` and `linux`)
//! that implements the [`Facade`](crate::facade::Facade) strategy. [`run`] is the only
//! place that picks one, at compile time, for the OS the binary is built for; nothing
//! outside this module uses `#[cfg(target_os)]`.
//!
//! [`Os`] says which OS fluentb runs on. Pure code takes it as a parameter instead of
//! checking `cfg!` itself, so behaviour for every OS can be tested on any machine.

use std::process::ExitCode;

pub mod applescript;
pub mod bridge;
pub mod instance;
pub mod keymap;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod system;

/// Starts fluentb with the adapter for the OS this binary was built for.
#[cfg(target_os = "macos")]
#[must_use]
pub fn run() -> ExitCode {
    macos::run()
}

/// Starts fluentb with the adapter for the OS this binary was built for.
#[cfg(not(target_os = "macos"))]
#[must_use]
pub fn run() -> ExitCode {
    eprintln!("fluentb currently supports macOS only; Windows and Linux are planned.");
    ExitCode::FAILURE
}

/// The operating systems fluentb targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Windows,
    Linux,
}

impl Os {
    /// The OS this binary was built for.
    #[must_use]
    pub const fn current() -> Self {
        #[cfg(target_os = "macos")]
        {
            Self::MacOs
        }
        #[cfg(windows)]
        {
            Self::Windows
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            Self::Linux
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn current_is_macos_on_macos() {
        assert_eq!(Os::current(), Os::MacOs);
    }

    #[cfg(windows)]
    #[test]
    fn current_is_windows_on_windows() {
        assert_eq!(Os::current(), Os::Windows);
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    #[test]
    fn current_is_linux_elsewhere() {
        assert_eq!(Os::current(), Os::Linux);
    }
}
