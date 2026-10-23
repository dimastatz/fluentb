//! The tray menu, built from the config and the current status.

use crate::config::Config;
use crate::facade::{Hotkey, TrayStatus};
use crate::platform::Os;

/// What a clickable menu entry does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    OpenConfig,
    Quit,
}

impl MenuAction {
    const ALL: [Self; 2] = [Self::OpenConfig, Self::Quit];

    /// The id the platform menu reports when the entry is chosen.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::OpenConfig => "open-config",
            Self::Quit => "quit",
        }
    }

    /// The action for a menu id, or `None` for ids fluentb didn't create.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.id() == id)
    }
}

/// One entry below the status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry {
    /// Informational text, shown disabled.
    Label(String),
    Action {
        action: MenuAction,
        title: String,
    },
    Separator,
}

/// The whole menu: a status line that changes with the state, then fixed entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuModel {
    pub status: String,
    pub entries: Vec<MenuEntry>,
}

/// Builds the menu for `status` with the hotkeys from `config`, written the way `os`
/// shows them.
#[must_use]
pub fn menu_model(config: &Config, status: TrayStatus, os: Os) -> MenuModel {
    let hotkeys = config.bindings().into_iter().map(|binding| {
        MenuEntry::Label(format!(
            "{}: {}",
            hotkey_name(binding.hotkey),
            binding.accelerator.display(os)
        ))
    });
    let actions = [
        (MenuAction::OpenConfig, "Open config…"),
        (MenuAction::Quit, "Quit fluentb"),
    ]
    .into_iter()
    .map(|(action, title)| MenuEntry::Action {
        action,
        title: title.to_owned(),
    });
    MenuModel {
        status: status_label(status).to_owned(),
        entries: std::iter::once(MenuEntry::Separator)
            .chain(hotkeys)
            .chain([MenuEntry::Separator])
            .chain(actions)
            .collect(),
    }
}

/// The status line text.
#[must_use]
pub const fn status_label(status: TrayStatus) -> &'static str {
    match status {
        TrayStatus::Idle => "Idle",
        TrayStatus::Recording => "Recording…",
        TrayStatus::Transcribing => "Transcribing…",
        TrayStatus::Rewriting => "Rewriting…",
        TrayStatus::Error => "Something went wrong",
    }
}

/// The tooltip shown when hovering over the icon.
#[must_use]
pub fn tooltip(status: TrayStatus) -> String {
    format!("fluentb: {}", status_label(status))
}

const fn hotkey_name(hotkey: Hotkey) -> &'static str {
    match hotkey {
        Hotkey::Dictate => "Dictate",
        Hotkey::Rewrite => "Rewrite",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(text: &str) -> MenuEntry {
        MenuEntry::Label(text.into())
    }

    fn action(action: MenuAction, title: &str) -> MenuEntry {
        MenuEntry::Action {
            action,
            title: title.into(),
        }
    }

    #[test]
    fn builds_the_menu_from_config_and_status() {
        let menu = menu_model(
            &Config::defaults(Os::MacOs),
            TrayStatus::Recording,
            Os::MacOs,
        );

        assert_eq!(
            menu,
            MenuModel {
                status: "Recording…".into(),
                entries: vec![
                    MenuEntry::Separator,
                    label("Dictate: ⇧⌘D"),
                    label("Rewrite: ⇧⌘R"),
                    MenuEntry::Separator,
                    action(MenuAction::OpenConfig, "Open config…"),
                    action(MenuAction::Quit, "Quit fluentb"),
                ],
            }
        );
    }

    #[test]
    fn shows_hotkeys_the_way_the_os_writes_them() {
        let menu = menu_model(
            &Config::defaults(Os::Windows),
            TrayStatus::Idle,
            Os::Windows,
        );

        assert_eq!(menu.status, "Idle");
        assert_eq!(menu.entries[1], label("Dictate: Ctrl+Shift+D"));
        assert_eq!(menu.entries[2], label("Rewrite: Ctrl+Shift+R"));
    }

    #[test]
    fn every_status_has_a_label_and_tooltip() {
        let cases = [
            (TrayStatus::Idle, "Idle"),
            (TrayStatus::Recording, "Recording…"),
            (TrayStatus::Transcribing, "Transcribing…"),
            (TrayStatus::Rewriting, "Rewriting…"),
            (TrayStatus::Error, "Something went wrong"),
        ];

        cases.iter().for_each(|(status, text)| {
            assert_eq!(status_label(*status), *text);
            assert_eq!(tooltip(*status), format!("fluentb: {text}"));
        });
    }

    #[test]
    fn menu_ids_round_trip() {
        MenuAction::ALL
            .iter()
            .for_each(|action| assert_eq!(MenuAction::from_id(action.id()), Some(*action)));
        assert_eq!(MenuAction::from_id("about"), None);
    }
}
