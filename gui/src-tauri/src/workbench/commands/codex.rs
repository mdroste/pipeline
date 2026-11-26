mod account;
pub(crate) mod requests;
pub use account::*;
pub use requests::*;
// Tauri wrappers; runtime services own mutable turn state.
use super::*;
#[tauri::command]
pub async fn workbench_reconcile_workspace_roots(
    operation_id: String,
) -> WorkbenchResult<ReconcileResult> {
    crate::workbench::runtime::codex::reconcile_workspace_roots(operation_id).await
}

#[tauri::command]
pub async fn workbench_codex_send_turn(
    app: tauri::AppHandle,
    request: SendTurnRequest,
) -> Result<SendTurnResult, RuntimeCommandError> {
    crate::workbench::runtime::codex::codex_send_turn(app, request).await
}

#[tauri::command]
pub async fn workbench_codex_interrupt_turn(
    thread_id: String,
    turn_id: String,
) -> Result<(), crate::workbench::codex::RequestError> {
    crate::workbench::runtime::codex::codex_interrupt_turn(thread_id, turn_id).await
}

#[tauri::command]
pub async fn workbench_codex_reconcile_session(
    app: tauri::AppHandle,
    session_id: String,
) -> Result<bool, RuntimeCommandError> {
    crate::workbench::runtime::codex::codex_reconcile_session(app, session_id).await
}
