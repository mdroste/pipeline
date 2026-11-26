use crate::workbench::runtime::{PendingRequest, ResolveServerRequest};
#[tauri::command]
pub async fn workbench_codex_resolve_server_request(
    request: ResolveServerRequest,
) -> Result<(), crate::workbench::codex::RequestError> {
    crate::workbench::runtime::codex::requests::codex_resolve_server_request(request).await
}

#[tauri::command]
pub fn workbench_codex_pending_requests() -> Vec<PendingRequest> {
    crate::workbench::runtime::codex::requests::codex_pending_requests()
}
