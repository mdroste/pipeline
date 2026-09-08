//! One native runtime owns execution even when its window is hidden.
use tauri::Manager;

pub fn install(app: &tauri::AppHandle) -> Result<(), String> {
    if app.tray_by_id("pipeline-tasks").is_some() {
        return Ok(());
    }
    let show =
        tauri::menu::MenuItem::with_id(app, "task-show", "Open Pipeline", true, None::<&str>)
            .map_err(super::store::err)?;
    let quit =
        tauri::menu::MenuItem::with_id(app, "task-quit", "Quit Pipeline", true, None::<&str>)
            .map_err(super::store::err)?;
    let menu = tauri::menu::Menu::with_items(app, &[&show, &quit]).map_err(super::store::err)?;
    let mut builder = tauri::tray::TrayIconBuilder::with_id("pipeline-tasks")
        .tooltip("Pipeline — tasks continue while this computer is awake")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "task-show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
            "task-quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app).map_err(super::store::err)?;
    Ok(())
}
