//! The macOS menu bar: Tauri's default menu plus a sidebar item in View.
//!
//! The frontend owns whether the sidebar is folded, because that also depends
//! on the window width. The item only reports clicks, and the frontend reports
//! back so the item's title stays true. Other platforms have no menu bar, so
//! the report is a no-op there.

use tauri::{AppHandle, Manager};

const HIDE_SIDEBAR: &str = "Hide Sidebar";
const SHOW_SIDEBAR: &str = "Show Sidebar";

/// `Menu::get` searches only the top level, so the item is kept for retitling.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
struct SidebarItem(tauri::menu::MenuItem<tauri::Wry>);

#[cfg(target_os = "macos")]
const TOGGLE_SIDEBAR_ID: &str = "toggle-sidebar";

#[cfg(target_os = "macos")]
pub fn build(app: &AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItem, MenuItemKind, PredefinedMenuItem};

    let menu = Menu::default(app)?;
    let view = menu.items()?.into_iter().find_map(|item| match item {
        MenuItemKind::Submenu(submenu) if submenu.text().is_ok_and(|text| text == "View") => {
            Some(submenu)
        }
        _ => None,
    });
    if let Some(view) = view {
        let sidebar = MenuItem::with_id(
            app,
            TOGGLE_SIDEBAR_ID,
            HIDE_SIDEBAR,
            true,
            Some("Cmd+Ctrl+S"),
        )?;
        view.prepend_items(&[&sidebar, &PredefinedMenuItem::separator(app)?])?;
        app.manage(SidebarItem(sidebar));
    }
    Ok(menu)
}

#[cfg(target_os = "macos")]
pub fn handle_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    use tauri::Emitter;

    if event.id() == TOGGLE_SIDEBAR_ID {
        let _ = app.emit_to("main", "menu:toggle-sidebar", ());
    }
}

/// Keep the View menu's title in step with the sidebar.
#[tauri::command]
pub fn set_sidebar_menu_collapsed(app: AppHandle, collapsed: bool) -> Result<(), String> {
    let Some(item) = app.try_state::<SidebarItem>() else {
        return Ok(());
    };
    item.0
        .set_text(if collapsed {
            SHOW_SIDEBAR
        } else {
            HIDE_SIDEBAR
        })
        .map_err(|error| format!("failed to update the View menu: {error}"))
}
