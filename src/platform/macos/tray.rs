//! The menu-bar icon and menu, drawn from the pure models in `crate::tray`.

use crate::facade::TrayStatus;
use crate::tray::icon;
use crate::tray::menu::{self, MenuEntry, MenuModel};
use tray_icon::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// The icon plus the status line, the menu entry that changes most often.
pub struct Tray {
    icon: TrayIcon,
    status: MenuItem,
}

impl Tray {
    /// Creates the icon. Must run on the main thread once the event loop has started.
    pub fn new(model: &MenuModel, status: TrayStatus) -> Result<Self, String> {
        let (menu, status_item) = build(model)?;
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .build()
            .map_err(|error| error.to_string())?;
        let tray = Self {
            icon,
            status: status_item,
        };
        tray.show(status, &model.status)?;
        Ok(tray)
    }

    /// Replaces the whole menu, e.g. after switching models or a permission change.
    pub fn rebuild(&mut self, model: &MenuModel) -> Result<(), String> {
        let (menu, status_item) = build(model)?;
        self.icon.set_menu(Some(Box::new(menu)));
        self.status = status_item;
        Ok(())
    }

    /// Shows `status` in the icon and tooltip, and `line` as the status line.
    pub fn show(&self, status: TrayStatus, line: &str) -> Result<(), String> {
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
        self.status.set_text(line);
        Ok(())
    }

    pub fn set_status_line(&self, line: &str) {
        self.status.set_text(line);
    }
}

fn build(model: &MenuModel) -> Result<(Menu, MenuItem), String> {
    let menu = Menu::new();
    let status = MenuItem::new(&model.status, false, None);
    menu.append(&status).map_err(|error| error.to_string())?;
    model.entries.iter().try_for_each(|entry| {
        menu.append(item(entry)?.as_ref())
            .map_err(|error| error.to_string())
    })?;
    Ok((menu, status))
}

fn item(entry: &MenuEntry) -> Result<Box<dyn IsMenuItem>, String> {
    Ok(match entry {
        MenuEntry::Label(text) => Box::new(MenuItem::new(text, false, None)),
        MenuEntry::Action { action, title } => {
            Box::new(MenuItem::with_id(action.id(), title, true, None))
        }
        MenuEntry::Choice {
            action,
            title,
            checked,
        } => Box::new(CheckMenuItem::with_id(
            action.id(),
            title,
            true,
            *checked,
            None,
        )),
        MenuEntry::Separator => Box::new(PredefinedMenuItem::separator()),
        MenuEntry::Submenu { title, entries } => {
            let submenu = Submenu::new(title, true);
            entries.iter().try_for_each(|entry| {
                submenu
                    .append(item(entry)?.as_ref())
                    .map_err(|error| error.to_string())
            })?;
            Box::new(submenu)
        }
    })
}
