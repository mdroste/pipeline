#[tauri::command]
pub async fn workbench_codex_connect(
    app: tauri::AppHandle,
) -> Result<crate::workbench::codex::SupervisorStatus, crate::workbench::codex::RequestError> {
    crate::workbench::runtime::codex::account::codex_connect(app).await
}

#[tauri::command]
pub async fn workbench_codex_account_state(
    app: tauri::AppHandle,
    refresh_token: bool,
) -> Result<crate::workbench::codex::AccountState, crate::workbench::codex::RequestError> {
    crate::workbench::runtime::codex::account::codex_account_state(app, refresh_token).await
}

#[tauri::command]
pub async fn workbench_codex_login_start(
    app: tauri::AppHandle,
) -> Result<crate::workbench::codex::LoginStart, crate::workbench::codex::RequestError> {
    crate::workbench::runtime::codex::account::codex_login_start(app).await
}

#[tauri::command]
pub async fn workbench_codex_login_cancel(
    login_id: String,
) -> Result<bool, crate::workbench::codex::RequestError> {
    crate::workbench::runtime::codex::account::codex_login_cancel(login_id).await
}

#[tauri::command]
pub async fn workbench_codex_logout() -> Result<(), crate::workbench::codex::RequestError> {
    crate::workbench::runtime::codex::account::codex_logout().await
}

#[tauri::command]
pub async fn workbench_codex_model_catalog(
) -> Result<crate::workbench::codex::ModelCatalog, crate::workbench::codex::RequestError> {
    crate::workbench::runtime::codex::account::codex_model_catalog().await
}

#[tauri::command]
pub async fn workbench_codex_validate_model_selection(
    model: String,
    effort: Option<String>,
) -> Result<crate::workbench::codex::WorkspaceModel, crate::workbench::codex::RequestError> {
    crate::workbench::runtime::codex::account::codex_validate_model_selection(model, effort).await
}

#[tauri::command]
pub async fn workbench_codex_rate_limits(
) -> Result<crate::workbench::codex::RateLimits, crate::workbench::codex::RequestError> {
    crate::workbench::runtime::codex::account::codex_rate_limits().await
}
