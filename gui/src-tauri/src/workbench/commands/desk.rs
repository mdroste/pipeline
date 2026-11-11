//! Desk Tauri commands.

use super::*;

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
pub(crate) fn task_pending_request(thread: &str, turn: &str) -> Option<String> {
    pending_server_requests()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .find_map(|event| {
            if let crate::agent_runtime::events::NormalizedEvent::ServerRequest {
                method,
                params,
                ..
            } = event
            {
                if params.get("threadId").and_then(Value::as_str) == Some(thread)
                    && params.get("turnId").and_then(Value::as_str) == Some(turn)
                    && method != "item/tool/call"
                {
                    return Some(
                        params
                            .get("reason")
                            .or_else(|| params.get("questions"))
                            .map(Value::to_string)
                            .unwrap_or_else(|| method.clone()),
                    );
                }
            }
            None
        })
}
pub(crate) async fn deliver_task(
    binding: crate::workbench::tasks::TaskBinding,
    operation: String,
    value: Value,
) -> Result<Value, String> {
    let _permit = turn_queue()
        .clone()
        .try_acquire_owned()
        .map_err(|_| "task_busy: Workspace is running another turn".to_string())?;
    run_store(move |store| {
        crate::workbench::tasks::validate_context(&store, &binding)?;
        let mut result =
            crate::workbench::tasks::deliver(&store, &binding.session_id, &operation, value)?;
        let next = crate::workbench::tasks::binding(&store, &binding.session_id)?;
        result["harnessFingerprint"] = serde_json::json!(next.harness_fingerprint);
        result["cursor"] = serde_json::json!(next.cursor);
        Ok(result)
    })
    .await
    .map_err(|e| e.message)
}

#[tauri::command]
pub async fn workbench_file_reveal(
    request: crate::workbench::project::FileReadRequest,
) -> WorkbenchResult<()> {
    run_store(move |store| {
        let path = crate::workbench::project::reveal_target(&store, &request)?;
        crate::file_viewer::reveal(&path).map_err(WorkbenchError::invalid)
    })
    .await
}
