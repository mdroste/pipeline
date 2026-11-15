//! Desk Tauri commands.

use super::{ensure_session_idle, run_store, WorkbenchResult};

// Research desk commands keep all app-owned storage work on the bounded gate.
#[tauri::command]
pub async fn workbench_desk_records(
    workspace_id: String,
    kind: String,
) -> WorkbenchResult<Vec<crate::workbench::desk::DeskRecord>> {
    run_store(move |s| crate::workbench::desk::records(&s, &workspace_id, &kind)).await
}
#[tauri::command]
pub async fn workbench_research_object(
    workspace_id: String,
    object: crate::workbench::desk::ResearchObjectRef,
) -> WorkbenchResult<crate::workbench::search::ResearchObject> {
    run_store(move |s| crate::workbench::search::read_object(&s, &workspace_id, &object, 64 * 1024))
        .await
}
#[tauri::command]
pub async fn workbench_context_selection(
    session_id: String,
) -> WorkbenchResult<crate::workbench::desk::ContextSelection> {
    run_store(move |s| crate::workbench::desk::context(&s, &session_id)).await
}
#[tauri::command]
pub async fn workbench_save_context_selection(
    session_id: String,
    expected_revision: i64,
    items: Vec<crate::workbench::desk::ContextItem>,
) -> WorkbenchResult<crate::workbench::desk::ContextSelection> {
    ensure_session_idle(session_id.clone()).await?;
    run_store(move |s| {
        crate::workbench::desk::save_context(&s, &session_id, expected_revision, items)
    })
    .await
}
