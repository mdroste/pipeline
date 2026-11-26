//! Conversation records, draft acknowledgements, and lifecycle edits.

use super::{active_turn_state, run_store, turn_queue};
use crate::workbench::{
    chat_approval, research,
    store::{
        ConversationSnapshot, CreateSessionRequest, DeleteSessionRequest, MoveSessionRequest,
        Mutation, SessionCursor, SessionList, SessionSnapshot, TranscriptCursor, TranscriptPage,
        UpdateSessionRequest, WorkbenchError, WorkbenchResult, WorkbenchSession,
    },
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Moving or deleting a conversation waits for the app's one running response,
/// wherever it is, so nothing here can race that turn.
pub(super) fn conversation_turn_permit(
    queue: Arc<Semaphore>,
) -> WorkbenchResult<OwnedSemaphorePermit> {
    queue.try_acquire_owned().map_err(|_| {
        WorkbenchError::conflict(
            "A response is still running. Wait for it to finish or stop it, then try again.",
        )
    })
}

#[tauri::command]
pub async fn workbench_create_session(
    request: CreateSessionRequest,
) -> WorkbenchResult<Mutation<WorkbenchSession>> {
    run_store(move |store| {
        let overrides = default_session_overrides(&crate::settings::load())?;
        store.create_session_with_overrides(request, overrides.as_ref())
    })
    .await
}

/// New conversations copy the chosen default, so changing it later never moves
/// a conversation that has already started. With no choice made they keep the
/// ChatGPT fallback that conversations without a saved runtime use.
pub(super) fn default_session_overrides(
    settings: &crate::settings::Settings,
) -> WorkbenchResult<Option<Value>> {
    if !settings.default_model_chosen {
        return Ok(None);
    }
    let connection =
        crate::connections::ConnectionId::for_review(&settings.preferred_provider, settings)
            .map_err(WorkbenchError::invalid)?;
    let mut overrides = json!({ "runtime": connection });
    // The conversation picker offers thinking levels only for a chosen model.
    if !settings.default_model.is_empty() {
        overrides["model"] = settings.default_model.as_str().into();
        if !settings.default_effort.is_empty() {
            overrides["effort"] = settings.default_effort.as_str().into();
        }
    }
    Ok(Some(overrides))
}

#[tauri::command]
pub async fn workbench_update_session(
    request: UpdateSessionRequest,
) -> WorkbenchResult<Mutation<WorkbenchSession>> {
    run_store(move |store| store.update_session(request)).await
}

#[tauri::command]
pub async fn workbench_clear_chat_approvals(session_id: String) -> WorkbenchResult<()> {
    run_store(move |store| chat_approval::clear(&store, &session_id)).await
}

/// Check the conversation binding and the active turn before a lifecycle edit.
pub(super) async fn ensure_session_idle(session_id: String) -> WorkbenchResult<()> {
    let binding = run_store(move |store| store.active_binding(&session_id)).await?;
    let state = active_turn_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if state.setup_in_progress {
        return Err(WorkbenchError::conflict(
            "Wait for the Workspace turn that is starting to finish",
        ));
    }
    if let Some(binding) = binding {
        if state.permit.is_some() && state.thread_id.as_deref() == Some(&binding.provider_thread_id)
        {
            return Err(WorkbenchError::conflict(
                "Stop this conversation's active turn first",
            ));
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn workbench_move_session(
    request: MoveSessionRequest,
) -> WorkbenchResult<Mutation<WorkbenchSession>> {
    let _permit = conversation_turn_permit(turn_queue().clone())?;
    if research::artifact_runs::session_active(&request.session_id) {
        return Err(WorkbenchError::conflict(
            "Stop or finish this conversation's local script before moving it.",
        ));
    }
    ensure_session_idle(request.session_id.clone()).await?;
    run_store(move |store| store.move_session(request)).await
}

#[tauri::command]
pub async fn workbench_delete_session(request: DeleteSessionRequest) -> WorkbenchResult<i64> {
    let _permit = conversation_turn_permit(turn_queue().clone())?;
    if research::artifact_runs::session_active(&request.session_id) {
        return Err(WorkbenchError::conflict(
            "Stop or finish this conversation's local script before deleting it.",
        ));
    }
    ensure_session_idle(request.session_id.clone()).await?;
    run_store(move |store| store.delete_session(request)).await
}

#[tauri::command]
pub async fn workbench_list_sessions(
    workspace_id: Option<String>,
    include_archived: bool,
    cursor: Option<SessionCursor>,
) -> WorkbenchResult<SessionList> {
    run_store(move |store| {
        store.list_sessions_page(workspace_id.as_deref(), include_archived, cursor.as_ref())
    })
    .await
}

#[tauri::command]
pub async fn workbench_session_snapshot(session_id: String) -> WorkbenchResult<SessionSnapshot> {
    run_store(move |store| store.session_snapshot(&session_id)).await
}

#[tauri::command]
pub async fn workbench_conversation_snapshot(
    session_id: String,
) -> WorkbenchResult<ConversationSnapshot> {
    run_store(move |store| store.conversation_snapshot(&session_id)).await
}

#[tauri::command]
pub async fn workbench_transcript_page(
    session_id: String,
    before: Option<TranscriptCursor>,
) -> WorkbenchResult<TranscriptPage> {
    run_store(move |store| store.transcript_page(&session_id, before.as_ref())).await
}

#[tauri::command]
pub async fn workbench_conversation_index(
    archived: bool,
    unfiled: bool,
    query: String,
    palette: bool,
    cursor: Option<crate::workbench::store::IndexCursor>,
) -> WorkbenchResult<Value> {
    run_store(move |store| {
        store.conversation_index(archived, unfiled, &query, palette, cursor.as_ref())
    })
    .await
}
