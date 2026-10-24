//! First-run onboarding: what to say, and how fluentb remembers it has run before.
//!
//! The steps themselves (asking for permissions, starting downloads) are carried out
//! by each platform adapter; the core announces each model when it becomes ready.

use crate::app::HotkeyLabels;
use crate::download::format_size;
use std::path::{Path, PathBuf};

const MARKER: &str = "first_run_complete";

/// The file whose presence means onboarding already ran; kept next to the config.
#[must_use]
pub fn marker_path(config_path: &Path) -> PathBuf {
    config_path.with_file_name(MARKER)
}

/// The first notification a new user sees.
#[must_use]
pub fn welcome(download_bytes: u64, labels: &HotkeyLabels) -> String {
    if download_bytes == 0 {
        format!(
            "fluentb lives in your menu bar. Press {} to dictate, or select text and press {} \
             to rewrite it.",
            labels.dictate, labels.rewrite
        )
    } else {
        format!(
            "fluentb lives in your menu bar. It is downloading about {} of models in the \
             background; dictation will be ready first.",
            format_size(download_bytes)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels() -> HotkeyLabels {
        HotkeyLabels {
            dictate: "⇧⌘D".into(),
            rewrite: "⇧⌘R".into(),
        }
    }

    #[test]
    fn marker_sits_next_to_the_config() {
        assert_eq!(
            marker_path(Path::new("/c/fluentb/config.toml")),
            Path::new("/c/fluentb/first_run_complete")
        );
    }

    #[test]
    fn welcome_mentions_the_download_size() {
        assert_eq!(
            welcome(2_645_244_467, &labels()),
            "fluentb lives in your menu bar. It is downloading about 2.6 GB of models in the \
             background; dictation will be ready first."
        );
    }

    #[test]
    fn welcome_teaches_the_hotkeys_when_nothing_to_download() {
        assert_eq!(
            welcome(0, &labels()),
            "fluentb lives in your menu bar. Press ⇧⌘D to dictate, or select text and press ⇧⌘R \
             to rewrite it."
        );
    }
}
