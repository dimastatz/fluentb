//! Operating-system support.
//!
//! [`Os`] says which OS fluentb runs on. Pure code takes it as a parameter instead of
//! checking `cfg!` itself, so behaviour for every OS can be tested on any machine.
//! Each OS's `Facade` implementation lives in its own submodule.

pub mod applescript;
pub mod keymap;
#[cfg(target_os = "macos")]
pub mod macos;

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
