//! Codex account, catalog, and connection commands.

use super::ensure_event_bridge;

pub(crate) async fn codex_connect(
    app: tauri::AppHandle,
) -> Result<crate::workbench::codex::SupervisorStatus, crate::workbench::codex::RequestError> {
    let supervisor = crate::workbench::codex::supervisor_manager()
        .connect()
        .await?;
    ensure_event_bridge(app, supervisor.clone());
    Ok(supervisor.status())
}

pub(crate) async fn codex_account_state(
    app: tauri::AppHandle,
    refresh_token: bool,
) -> Result<crate::workbench::codex::AccountState, crate::workbench::codex::RequestError> {
    crate::agent_runtime::codex::chatgpt::chatgpt_account_status(app, Some(refresh_token))
        .await
        .map(|status| status.account)
        .map_err(crate::workbench::codex::RequestError::unavailable)
}

pub(crate) async fn codex_login_start(
    app: tauri::AppHandle,
) -> Result<crate::workbench::codex::LoginStart, crate::workbench::codex::RequestError> {
    crate::agent_runtime::codex::chatgpt::begin_browser_login(app)
        .await
        .map(|(_, login)| login)
        .map_err(crate::workbench::codex::RequestError::unavailable)
}

pub(crate) async fn codex_login_cancel(
    login_id: String,
) -> Result<bool, crate::workbench::codex::RequestError> {
    crate::agent_runtime::codex::chatgpt::connect()
        .await
        .map_err(crate::workbench::codex::RequestError::unavailable)?
        .cancel_login(&login_id, None)
        .await
        .map_err(crate::workbench::codex::RequestError::unavailable)
}

pub(crate) async fn codex_logout() -> Result<(), crate::workbench::codex::RequestError> {
    crate::agent_runtime::codex::chatgpt::chatgpt_logout()
        .await
        .map_err(crate::workbench::codex::RequestError::unavailable)
}

pub(crate) async fn codex_model_catalog(
) -> Result<crate::workbench::codex::ModelCatalog, crate::workbench::codex::RequestError> {
    crate::workbench::codex::supervisor_manager()
        .connect()
        .await?
        .model_catalog()
        .await
}

pub(crate) async fn codex_validate_model_selection(
    model: String,
    effort: Option<String>,
) -> Result<crate::workbench::codex::WorkspaceModel, crate::workbench::codex::RequestError> {
    crate::workbench::codex::supervisor_manager()
        .connect()
        .await?
        .validate_model_selection(&model, effort.as_deref())
        .await
}

pub(crate) async fn codex_rate_limits(
) -> Result<crate::workbench::codex::RateLimits, crate::workbench::codex::RequestError> {
    crate::workbench::codex::supervisor_manager()
        .connect()
        .await?
        .rate_limits()
        .await
}
