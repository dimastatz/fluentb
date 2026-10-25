//! The tray menu, built from the config and what the app is doing.

use crate::config::Config;
use crate::download::format_size;
use crate::facade::{Hotkey, Permission, TrayStatus};
use crate::models::{self, Kind, Model};
use crate::platform::Os;

/// What a clickable menu entry does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    OpenConfig,
    Quit,
    /// Switch to this model, downloading it if needed.
    SelectModel(Model),
    RemoveUnusedModels,
    /// Open System Settings where this permission is granted.
    FixPermission(Permission),
    /// Turn starting fluentb at login on or off.
    ToggleLaunchAtLogin,
    /// Show the log folder.
    RevealLogs,
}

impl MenuAction {
    /// The id the platform menu reports when the entry is chosen.
    #[must_use]
    pub fn id(self) -> String {
        match self {
            Self::OpenConfig => "open-config".into(),
            Self::Quit => "quit".into(),
            Self::SelectModel(model) => format!("model:{}", model.id()),
            Self::RemoveUnusedModels => "remove-unused-models".into(),
            Self::FixPermission(Permission::Microphone) => "fix:microphone".into(),
            Self::FixPermission(Permission::Accessibility) => "fix:accessibility".into(),
            Self::ToggleLaunchAtLogin => "launch-at-login".into(),
            Self::RevealLogs => "reveal-logs".into(),
        }
    }

    /// The action for a menu id, or `None` for ids fluentb didn't create.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        let model = |name: &str| {
            models::REGISTRY
                .iter()
                .map(|entry| entry.model)
                .find(|model| model.id() == name)
        };
        match id {
            "open-config" => Some(Self::OpenConfig),
            "quit" => Some(Self::Quit),
            "remove-unused-models" => Some(Self::RemoveUnusedModels),
            "fix:microphone" => Some(Self::FixPermission(Permission::Microphone)),
            "fix:accessibility" => Some(Self::FixPermission(Permission::Accessibility)),
            "launch-at-login" => Some(Self::ToggleLaunchAtLogin),
            "reveal-logs" => Some(Self::RevealLogs),
            other => other
                .strip_prefix("model:")
                .and_then(model)
                .map(Self::SelectModel),
        }
    }
}

/// One menu entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry {
    /// Informational text, shown disabled.
    Label(String),
    Action {
        action: MenuAction,
        title: String,
    },
    /// An action shown with a check mark when `checked`.
    Choice {
        action: MenuAction,
        title: String,
        checked: bool,
    },
    Submenu {
        title: String,
        entries: Vec<MenuEntry>,
    },
    Separator,
}

/// The whole menu: a status line that changes often, then the other entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuModel {
    pub status: String,
    pub entries: Vec<MenuEntry>,
}

/// Everything besides the config that shapes the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuState {
    pub status: TrayStatus,
    /// The model download in progress and its percentage.
    pub download: Option<(Kind, u8)>,
    pub speech: Model,
    pub rewrite: Model,
    /// Permissions the user hasn't granted.
    pub missing: Vec<Permission>,
    pub launch_at_login: bool,
}

/// Builds the menu for `state`, with the hotkeys from `config` written the way `os`
/// shows them.
#[must_use]
pub fn menu_model(config: &Config, state: &MenuState, os: Os) -> MenuModel {
    let warnings = state.missing.iter().map(|permission| MenuEntry::Action {
        action: MenuAction::FixPermission(*permission),
        title: format!("⚠ {} not granted — Fix…", permission_name(*permission)),
    });
    let hotkeys = config.bindings().into_iter().map(|binding| {
        MenuEntry::Label(format!(
            "{}: {}",
            hotkey_name(binding.hotkey),
            binding.accelerator.display(os)
        ))
    });
    let model_menus = [
        ("Speech model", Kind::Speech, state.speech),
        ("Rewrite model", Kind::Rewrite, state.rewrite),
    ]
    .into_iter()
    .map(|(title, kind, current)| MenuEntry::Submenu {
        title: title.to_owned(),
        entries: models::of_kind(kind)
            .map(|model| MenuEntry::Choice {
                action: MenuAction::SelectModel(model),
                title: model_title(model),
                checked: model == current,
            })
            .collect(),
    });
    let launch_at_login = MenuEntry::Choice {
        action: MenuAction::ToggleLaunchAtLogin,
        title: "Launch at login".into(),
        checked: state.launch_at_login,
    };
    let actions = [
        (MenuAction::OpenConfig, "Open config…"),
        (MenuAction::RevealLogs, "Reveal logs"),
        (MenuAction::Quit, "Quit fluentb"),
    ]
    .into_iter()
    .map(|(action, title)| MenuEntry::Action {
        action,
        title: title.to_owned(),
    });
    MenuModel {
        status: status_line(state.status, state.download),
        entries: warnings
            .chain([MenuEntry::Separator])
            .chain(hotkeys)
            .chain([MenuEntry::Separator])
            .chain(model_menus)
            .chain([
                MenuEntry::Action {
                    action: MenuAction::RemoveUnusedModels,
                    title: "Remove unused models".into(),
                },
                MenuEntry::Separator,
                launch_at_login,
            ])
            .chain(actions)
            .collect(),
    }
}

/// The status line: what the app is doing, or the download in progress while idle.
#[must_use]
pub fn status_line(status: TrayStatus, download: Option<(Kind, u8)>) -> String {
    match (status, download) {
        (TrayStatus::Idle, Some((Kind::Speech, percent))) => {
            format!("Downloading speech model… {percent}%")
        }
        (TrayStatus::Idle, Some((Kind::Rewrite, percent))) => {
            format!("Downloading rewrite model… {percent}%")
        }
        (status, _) => status_label(status).to_owned(),
    }
}

/// The status text.
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

fn model_title(model: Model) -> String {
    let size = models::entry(model).map_or_else(String::new, |entry| {
        format!(" ({})", format_size(entry.size))
    });
    format!("{}{size}", model.id())
}

const fn permission_name(permission: Permission) -> &'static str {
    match permission {
        Permission::Microphone => "Microphone access",
        Permission::Accessibility => "Accessibility access",
    }
}

const fn hotkey_name(hotkey: Hotkey) -> &'static str {
    match hotkey {
        Hotkey::Dictate => "Dictate",
        Hotkey::Rewrite => "Rewrite",
        Hotkey::Cancel => "Cancel",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ModelSize, RewriteModel, WhisperModel};

    const BASE_EN: Model = Model::Whisper(WhisperModel {
        size: ModelSize::Base,
        english_only: true,
    });
    const QWEN: Model = Model::Rewrite(RewriteModel::Qwen3_4B);

    fn state() -> MenuState {
        MenuState {
            status: TrayStatus::Idle,
            download: None,
            speech: BASE_EN,
            rewrite: QWEN,
            missing: Vec::new(),
            launch_at_login: false,
        }
    }

    fn label(text: &str) -> MenuEntry {
        MenuEntry::Label(text.into())
    }

    fn action(action: MenuAction, title: &str) -> MenuEntry {
        MenuEntry::Action {
            action,
            title: title.into(),
        }
    }

    fn submenu<'a>(menu: &'a MenuModel, title: &str) -> &'a [MenuEntry] {
        menu.entries
            .iter()
            .find_map(|entry| match entry {
                MenuEntry::Submenu { title: t, entries } if t == title => Some(entries.as_slice()),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn builds_the_menu_from_config_and_state() {
        let menu = menu_model(&Config::defaults(Os::MacOs), &state(), Os::MacOs);

        assert_eq!(menu.status, "Idle");
        let flat: Vec<&MenuEntry> = menu
            .entries
            .iter()
            .filter(|entry| !matches!(entry, MenuEntry::Submenu { .. }))
            .collect();
        assert_eq!(
            flat,
            [
                &MenuEntry::Separator,
                &label("Dictate: ⇧⌘D"),
                &label("Rewrite: ⇧⌘R"),
                &MenuEntry::Separator,
                &action(MenuAction::RemoveUnusedModels, "Remove unused models"),
                &MenuEntry::Separator,
                &MenuEntry::Choice {
                    action: MenuAction::ToggleLaunchAtLogin,
                    title: "Launch at login".into(),
                    checked: false,
                },
                &action(MenuAction::OpenConfig, "Open config…"),
                &action(MenuAction::RevealLogs, "Reveal logs"),
                &action(MenuAction::Quit, "Quit fluentb"),
            ]
        );
    }

    #[test]
    fn launch_at_login_is_checked_when_on() {
        let state = MenuState {
            launch_at_login: true,
            ..state()
        };

        let menu = menu_model(&Config::defaults(Os::MacOs), &state, Os::MacOs);

        assert!(menu.entries.contains(&MenuEntry::Choice {
            action: MenuAction::ToggleLaunchAtLogin,
            title: "Launch at login".into(),
            checked: true,
        }));
    }

    #[test]
    fn model_submenus_check_the_current_models() {
        let menu = menu_model(&Config::defaults(Os::MacOs), &state(), Os::MacOs);

        let speech = submenu(&menu, "Speech model");
        assert_eq!(speech.len(), 9);
        assert!(speech.contains(&MenuEntry::Choice {
            action: MenuAction::SelectModel(BASE_EN),
            title: "base.en (148 MB)".into(),
            checked: true,
        }));
        let rewrite = submenu(&menu, "Rewrite model");
        assert_eq!(
            rewrite,
            [
                MenuEntry::Choice {
                    action: MenuAction::SelectModel(Model::Rewrite(RewriteModel::Qwen3_0_6B)),
                    title: "qwen3-0.6b (397 MB)".into(),
                    checked: false,
                },
                MenuEntry::Choice {
                    action: MenuAction::SelectModel(Model::Rewrite(RewriteModel::Qwen3_1_7B)),
                    title: "qwen3-1.7b (1.1 GB)".into(),
                    checked: false,
                },
                MenuEntry::Choice {
                    action: MenuAction::SelectModel(QWEN),
                    title: "qwen3-4b (2.5 GB)".into(),
                    checked: true,
                },
                MenuEntry::Choice {
                    action: MenuAction::SelectModel(Model::Rewrite(RewriteModel::Gemma3_4B)),
                    title: "gemma3-4b (2.5 GB)".into(),
                    checked: false,
                },
            ]
        );
    }

    #[test]
    fn missing_permissions_come_first() {
        let state = MenuState {
            missing: vec![Permission::Accessibility, Permission::Microphone],
            ..state()
        };

        let menu = menu_model(&Config::defaults(Os::MacOs), &state, Os::MacOs);

        assert_eq!(
            menu.entries[..2],
            [
                action(
                    MenuAction::FixPermission(Permission::Accessibility),
                    "⚠ Accessibility access not granted — Fix…"
                ),
                action(
                    MenuAction::FixPermission(Permission::Microphone),
                    "⚠ Microphone access not granted — Fix…"
                ),
            ]
        );
    }

    #[test]
    fn shows_hotkeys_the_way_the_os_writes_them() {
        let menu = menu_model(&Config::defaults(Os::Windows), &state(), Os::Windows);

        assert_eq!(menu.entries[1], label("Dictate: Ctrl+Shift+D"));
        assert_eq!(menu.entries[2], label("Rewrite: Ctrl+Shift+R"));
    }

    #[test]
    fn status_line_shows_downloads_while_idle() {
        assert_eq!(
            status_line(TrayStatus::Idle, Some((Kind::Speech, 42))),
            "Downloading speech model… 42%"
        );
        assert_eq!(
            status_line(TrayStatus::Idle, Some((Kind::Rewrite, 7))),
            "Downloading rewrite model… 7%"
        );
        assert_eq!(
            status_line(TrayStatus::Recording, Some((Kind::Rewrite, 7))),
            "Recording…"
        );
        assert_eq!(status_line(TrayStatus::Idle, None), "Idle");
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
        let actions = [
            MenuAction::OpenConfig,
            MenuAction::Quit,
            MenuAction::RemoveUnusedModels,
            MenuAction::FixPermission(Permission::Microphone),
            MenuAction::FixPermission(Permission::Accessibility),
            MenuAction::ToggleLaunchAtLogin,
            MenuAction::RevealLogs,
        ]
        .into_iter()
        .chain(
            models::REGISTRY
                .iter()
                .map(|entry| MenuAction::SelectModel(entry.model)),
        );

        actions.for_each(|action| assert_eq!(MenuAction::from_id(&action.id()), Some(action)));
        assert_eq!(MenuAction::SelectModel(BASE_EN).id(), "model:base.en");
        ["about", "model:huge", "model:", "fix:camera"]
            .iter()
            .for_each(|id| assert_eq!(MenuAction::from_id(id), None, "{id}"));
    }
}
