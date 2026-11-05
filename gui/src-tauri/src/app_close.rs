//! One close policy for the window, native Quit menu, and tray.
use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicBool, AtomicI32, Ordering},
    Mutex,
};
use tauri::{Emitter, Manager};

#[derive(Default)]
pub(crate) struct CloseState {
    listeners: Mutex<HashSet<String>>,
    approved: AtomicBool,
    exit_code: AtomicI32,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CloseAction {
    Hide,
    Quit,
}

#[tauri::command]
pub(crate) fn set_app_close_guard(
    window: tauri::WebviewWindow,
    id: String,
    active: bool,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Only the main window can handle quitting".into());
    }
    let state = window.state::<CloseState>();
    let mut listeners = state.listeners.lock().map_err(|e| e.to_string())?;
    if active {
        listeners.insert(id);
    } else {
        listeners.remove(&id);
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn app_close_hides() -> bool {
    crate::orchestration::background_enabled()
}

#[tauri::command]
pub(crate) fn complete_app_close(
    window: tauri::WebviewWindow,
    action: CloseAction,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Only the main window can close Pipeline".into());
    }
    match action {
        CloseAction::Hide => {
            // If the setting changed during the request, let the user retry;
            // never convert a hide into an unconfirmed quit.
            if !app_close_hides() {
                return Err("Background operation changed. Try closing the window again.".into());
            }
            window.hide().map_err(|e| e.to_string())
        }
        CloseAction::Quit => {
            let state = window.state::<CloseState>();
            state.approved.store(true, Ordering::Release);
            window
                .app_handle()
                .exit(state.exit_code.load(Ordering::Acquire));
            Ok(())
        }
    }
}

pub(crate) fn on_exit_requested(
    app: &tauri::AppHandle,
    api: &tauri::ExitRequestApi,
    code: Option<i32>,
) {
    // Tauri cannot prevent a restart. Update installation has its own explicit
    // confirmation, and must retain its existing restart behavior.
    if code == Some(tauri::RESTART_EXIT_CODE) {
        return;
    }
    let state = app.state::<CloseState>();
    if state.approved.load(Ordering::Acquire)
        || state
            .listeners
            .lock()
            .map_or(true, |listeners| listeners.is_empty())
    {
        return;
    }
    if let Some(window) = app.get_webview_window("main") {
        api.prevent_exit();
        state.exit_code.store(code.unwrap_or(0), Ordering::Release);
        // A tray Quit must make its confirmation visible even if already hidden.
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        if let Err(error) = window.emit("app:quit-requested", ()) {
            eprintln!("Could not request quit confirmation: {error}");
        }
    }
}
