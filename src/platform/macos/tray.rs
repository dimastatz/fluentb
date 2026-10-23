//! The menu-bar icon and menu, drawn from the pure models in `crate::tray`.

use crate::facade::TrayStatus;
use crate::tray::icon;
use crate::tray::menu::{self, MenuEntry, MenuModel};
use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// The icon plus the status line, which is the only menu entry that changes.
pub struct Tray {
    icon: TrayIcon,
    status: MenuItem,
}

impl Tray {
    /// Creates the icon. Must run on the main thread once the event loop has started.
    pub fn new(model: &MenuModel, status: TrayStatus) -> Result<Self, String> {
        let menu = Menu::new();
        let status_item = MenuItem::new(&model.status, false, None);
        menu.append(&status_item)
            .map_err(|error| error.to_string())?;
        model
            .entries
            .iter()
            .try_for_each(|entry| append(&menu, entry))?;
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .build()
            .map_err(|error| error.to_string())?;
        let tray = Self {
            icon,
            status: status_item,
        };
        tray.show(status)?;
        Ok(tray)
    }

    /// Shows `status` in the icon, the tooltip and the status line.
    pub fn show(&self, status: TrayStatus) -> Result<(), String> {
        let image = icon::icon(status);
        let size = u32::from(image.size);
        let native = Icon::from_rgba(image.rgba, size, size).map_err(|error| error.to_string())?;
        if image.template {
            self.icon.set_icon_templated(Some(native))
        } else {
            self.icon.set_icon(Some(native))
        }
        .map_err(|error| error.to_string())?;
        self.icon
            .set_tooltip(Some(menu::tooltip(status)))
            .map_err(|error| error.to_string())?;
        self.status.set_text(menu::status_label(status));
        Ok(())
    }
}

fn append(menu: &Menu, entry: &MenuEntry) -> Result<(), String> {
    match entry {
        MenuEntry::Label(text) => menu.append(&MenuItem::new(text, false, None)),
        MenuEntry::Separator => menu.append(&PredefinedMenuItem::separator()),
        MenuEntry::Action { action, title } => {
            menu.append(&MenuItem::with_id(action.id(), title, true, None))
        }
    }
    .map_err(|error| error.to_string())
}
