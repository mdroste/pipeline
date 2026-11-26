//! Application update, storage, and managed engine commands.

use tauri::AppHandle;

#[tauri::command]
pub async fn cli_tools_status() -> Result<crate::cli_tools::CliToolsStatus, String> {
    tokio::task::spawn_blocking(crate::cli_tools::status)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn install_cli_tools() -> Result<crate::cli_tools::CliToolsStatus, String> {
    tokio::task::spawn_blocking(crate::cli_tools::install)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn uninstall_cli_tools() -> Result<crate::cli_tools::CliToolsStatus, String> {
    tokio::task::spawn_blocking(crate::cli_tools::uninstall)
        .await
        .map_err(|e| e.to_string())?
}

/// Open the active research data folder. The path is resolved server-side
/// (never passed from the frontend) so there is nothing to sanitize.
#[tauri::command]
pub async fn open_pipeline_dir() -> Result<(), String> {
    let dir = crate::storage::data_root()?;

    crate::file_viewer::open_trusted_path(&dir)
        .map_err(|error| format!("Failed to open folder: {error}"))
}

#[tauri::command]
pub async fn get_storage_settings() -> Result<crate::storage::StorageSettings, String> {
    tokio::task::spawn_blocking(crate::storage::settings)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn set_storage_directory(
    directory: String,
) -> Result<crate::storage::StorageSettings, String> {
    tokio::task::spawn_blocking(move || crate::storage::set_directory(&directory))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<crate::updates::UpdateInfo, String> {
    crate::updates::check_now(&crate::emit::from_app(app)).await
}

/// Saved update preference and last result; sends no request.
#[tauri::command]
pub async fn update_status() -> Result<crate::updates::UpdateStatus, String> {
    tokio::task::spawn_blocking(crate::updates::status)
        .await
        .map_err(|e| format!("Update status task failed: {e}"))
}

#[tauri::command]
pub async fn update_set_automatic(
    app: AppHandle,
    enabled: bool,
) -> Result<crate::updates::UpdateStatus, String> {
    let bus = crate::emit::from_app(app);
    tokio::task::spawn_blocking(move || crate::updates::set_automatic(&bus, enabled))
        .await
        .map_err(|e| format!("Update preference task failed: {e}"))?
}

#[tauri::command]
pub fn update_install_state() -> crate::updates::InstallState {
    crate::updates::install_state()
}

/// Start downloading the newest release; `update-install` events report progress.
#[tauri::command]
pub async fn update_install_start(app: AppHandle) -> Result<crate::updates::InstallState, String> {
    crate::updates::install_start(&app)
}

#[tauri::command]
pub fn update_install_cancel(app: AppHandle) -> crate::updates::InstallState {
    crate::updates::install_cancel(&app)
}

/// Install the downloaded update and restart. On Windows this ends the process.
#[tauri::command]
pub async fn update_install_restart(app: AppHandle) -> Result<(), String> {
    crate::updates::install_restart(app).await
}

#[tauri::command]
pub fn update_running_automations() -> usize {
    crate::orchestration::running_count()
}

// --- Managed local engines ---

/// Status of every installable engine. The disk-usage walk can touch
/// multi-GB trees, so it runs off the async runtime.
#[tauri::command]
pub async fn list_engines() -> Result<Vec<crate::engines::EngineStatus>, String> {
    tokio::task::spawn_blocking(crate::engines::engine_statuses)
        .await
        .map_err(|e| format!("Engine status task failed: {e}"))
}

#[tauri::command]
pub async fn install_engine(app: AppHandle, engine_id: String) -> Result<(), String> {
    let bus = crate::emit::from_app(app);
    crate::engines::install_engine(&bus, &engine_id).await
}

#[tauri::command]
pub async fn uninstall_engine(app: AppHandle, engine_id: String) -> Result<(), String> {
    let bus = crate::emit::from_app(app);
    crate::engines::uninstall_engine(&bus, &engine_id).await
}

#[tauri::command]
pub fn cancel_engine_install() {
    crate::engines::cancel_install();
}
