//! Conversation title commands and provider dispatch.

use super::{ensure_event_bridge, run_store, RuntimeCommandError};
use crate::workbench::{
    api, claude, codex,
    store::{self, UpdateSessionRequest, WorkbenchResult},
    tasks,
    titles::{self, TitlePreferences},
};
use serde_json::json;
use std::sync::Arc;
use tauri::Emitter;

#[tauri::command]
pub async fn workbench_get_title_preferences() -> WorkbenchResult<TitlePreferences> {
    run_store(|store| titles::load_preferences(&store)).await
}

#[tauri::command]
pub async fn workbench_save_title_preferences(
    preferences: TitlePreferences,
) -> WorkbenchResult<TitlePreferences> {
    run_store(move |store| titles::save_preferences(&store, preferences)).await
}

/// Explicitly (re)generates a conversation's title, replacing any current one.
#[tauri::command]
pub async fn workbench_generate_session_title(
    app: tauri::AppHandle,
    session_id: String,
) -> Result<String, RuntimeCommandError> {
    let selected_session = session_id.clone();
    let overrides =
        run_store(move |store| Ok(store.session_snapshot(&selected_session)?.session.overrides))
            .await?;
    let runtime = if let Some(provider) = api::selected(&overrides)? {
        TitleRuntime::Api {
            epoch: api::next_epoch(),
            connection: api::config::load(provider, overrides["model"].as_str(), None)
                .await
                .map_err(api::error)?,
        }
    } else if claude::selected(&overrides)? {
        TitleRuntime::Claude {
            epoch: claude::fresh_epoch(),
        }
    } else {
        let supervisor = codex::supervisor_manager().connect().await?;
        ensure_event_bridge(app.clone(), supervisor.clone());
        TitleRuntime::Codex(supervisor)
    };
    generate_session_title(&app, &runtime, session_id, true)
        .await?
        .ok_or_else(|| RuntimeCommandError {
            code: "title_unavailable".to_string(),
            message: "The conversation needs one completed exchange before it can be titled"
                .to_string(),
            retryable: true,
            recovery: None,
        })
}

const TITLE_CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
const TITLE_PROJECTION_ATTEMPTS: usize = 10;

/// The provider that writes a conversation's title: the one it talks to.
pub(crate) enum TitleRuntime {
    Codex(Arc<codex::AppServerSupervisor>),
    Claude {
        epoch: u64,
    },
    Api {
        epoch: u64,
        connection: api::Connection,
    },
}

impl TitleRuntime {
    fn namespace(&self) -> String {
        match self {
            Self::Codex(supervisor) => supervisor.runtime_namespace(),
            Self::Claude { .. } => claude::NAMESPACE.to_string(),
            Self::Api { .. } => api::NAMESPACE.to_string(),
        }
    }

    fn epoch(&self) -> u64 {
        match self {
            Self::Codex(supervisor) => supervisor.status().epoch,
            Self::Claude { epoch } | Self::Api { epoch, .. } => *epoch,
        }
    }
}

/// One title side call. Returns `Ok(None)` when nothing should change: titles
/// are disabled, the user already named the conversation, or no completed
/// exchange exists yet. `force` skips the first two checks.
async fn generate_session_title(
    app: &tauri::AppHandle,
    runtime: &TitleRuntime,
    session_id: String,
    force: bool,
) -> Result<Option<String>, RuntimeCommandError> {
    let preferences = run_store(|store| titles::load_preferences(&store)).await?;
    if !force && !preferences.enabled {
        return Ok(None);
    }
    if !force {
        let lookup = session_id.clone();
        let session = run_store(move |store| store.session_snapshot(&lookup)).await?;
        if !titles::is_default_title(&session.session.title) {
            return Ok(None);
        }
    }
    let mut exchange = None;
    for attempt in 0..TITLE_PROJECTION_ATTEMPTS {
        let lookup = session_id.clone();
        let snapshot = run_store(move |store| store.conversation_snapshot(&lookup)).await?;
        if !force && !titles::is_default_title(&snapshot.session.title) {
            return Ok(None);
        }
        exchange = titles::first_exchange(&snapshot);
        if exchange.is_some() {
            break;
        }
        // The final assistant item is projected by a separate task; give it a
        // moment to land before deciding the exchange is incomplete.
        if attempt + 1 < TITLE_PROJECTION_ATTEMPTS {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }
    }
    let Some((user, assistant)) = exchange else {
        return Ok(None);
    };
    let raw = match runtime {
        TitleRuntime::Api { connection, .. } => {
            api::title(connection, &titles::title_prompt(&user, &assistant))
                .await
                .map_err(api::error)?
        }
        TitleRuntime::Codex(supervisor) => {
            codex_title(supervisor, &preferences, &user, &assistant).await?
        }
        // Title preferences name ChatGPT models; Claude uses its cheapest offered model.
        TitleRuntime::Claude { .. } => {
            claude::title::generate(titles::title_prompt(&user, &assistant), TITLE_CALL_TIMEOUT)
                .await
                .map_err(|message| RuntimeCommandError {
                    code: "title_failed".to_string(),
                    message,
                    retryable: true,
                    recovery: None,
                })?
        }
    };
    let Some(title) = titles::sanitize_title(&raw) else {
        return Err(RuntimeCommandError {
            code: "title_unusable".to_string(),
            message: "The model did not return a usable title".to_string(),
            retryable: true,
            recovery: None,
        });
    };
    let stored_title = title.clone();
    let apply_session = session_id.clone();
    let applied = run_store(move |store| {
        for attempt in 0..2 {
            let current = store.session_snapshot(&apply_session)?.session;
            if !force && !titles::is_default_title(&current.title) {
                return Ok(false);
            }
            match store.update_session(UpdateSessionRequest {
                session_id: apply_session.clone(),
                expected_revision: current.revision,
                operation_id: format!("auto-title-{}", store::operation_suffix()?),
                title: Some(stored_title.clone()),
                draft: None,
                overrides: None,
                archived: None,
                preset_id: None,
                paper_id: None,
                clear_paper: None,
            }) {
                Ok(_) => return Ok(true),
                Err(error) if error.code == "storage_conflict" && attempt == 0 => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(false)
    })
    .await?;
    if !applied {
        return Ok(None);
    }
    let _ = app.emit(
        "workbench:event",
        json!({
            "kind": "sessionTitleUpdated",
            "epoch": runtime.epoch(),
            "sessionId": session_id,
            "title": title
        }),
    );
    Ok(Some(title))
}

/// The ChatGPT title side call: an ephemeral read-only App Server thread.
async fn codex_title(
    supervisor: &Arc<codex::AppServerSupervisor>,
    preferences: &TitlePreferences,
    user: &str,
    assistant: &str,
) -> Result<String, RuntimeCommandError> {
    let catalog = supervisor.model_catalog().await?;
    let (model, effort) = match titles::choose_model(&catalog.models, preferences) {
        Some((model, effort)) => (Some(model), effort),
        None => (None, None),
    };
    let scratch = tempfile::Builder::new()
        .prefix("pipeline-workbench-title-")
        .tempdir()
        .map_err(|error| RuntimeCommandError {
            code: "storage_error".to_string(),
            message: format!("Could not create a scratch directory for the title call: {error}"),
            retryable: true,
            recovery: None,
        })?;
    let raw = supervisor
        .run_side_turn(codex::SideTurnRequest {
            cwd: scratch.path().to_string_lossy().into_owned(),
            permissions: "workbench-inspect".to_string(),
            developer_instructions: titles::TITLE_INSTRUCTIONS.to_string(),
            text: titles::title_prompt(user, assistant),
            model,
            effort,
            timeout: TITLE_CALL_TIMEOUT,
        })
        .await?;
    drop(scratch);
    Ok(raw)
}

/// Background title generation after a conversation turn completes.
pub(crate) fn spawn_title_generation(
    app: tauri::AppHandle,
    runtime: TitleRuntime,
    thread_id: String,
    turn_id: String,
) {
    tokio::spawn(async move {
        let task_thread = thread_id.clone();
        if run_store(move |s| tasks::is_task_turn(&s, &task_thread, &turn_id))
            .await
            .unwrap_or(true)
        {
            return;
        }
        let namespace = runtime.namespace();
        let lookup_thread = thread_id.clone();
        let session = run_store(move |store| store.session_for_thread(&namespace, &lookup_thread))
            .await
            .ok()
            .flatten();
        let Some(session_id) = session else {
            return;
        };
        if let Err(error) = generate_session_title(&app, &runtime, session_id.clone(), false).await
        {
            let _ = app.emit(
                "workbench:event",
                json!({
                    "kind": "sessionTitleFailed",
                    "epoch": runtime.epoch(),
                    "sessionId": session_id,
                    "error": error.message
                }),
            );
        }
    });
}
