//! Thin asynchronous Tauri facade for the blocking Workbench research store.

use super::store::{
    ClearWorkspaceRootRequest, ConversationSnapshot, CreateSessionRequest, CreateWorkspaceRequest,
    DeleteSessionRequest, MoveSessionRequest, Mutation, ReconcileResult,
    RegisterWorkspaceRootRequest, SessionList, SessionSnapshot, Store, UpdateSessionRequest,
    UpdateWorkspaceRequest, WorkbenchError, WorkbenchResult, WorkbenchSession, Workspace,
    WorkspaceList,
};
use super::titles::{self, TitlePreferences};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use tauri::Emitter;
use tauri_plugin_shell::ShellExt;
use tokio::sync::{OwnedSemaphorePermit, RwLock, Semaphore};

const DATABASE_WORKER_LIMIT: usize = 4;
static DATABASE_WORKERS: Semaphore = Semaphore::const_new(DATABASE_WORKER_LIMIT);
static DATABASE_GATE: RwLock<()> = RwLock::const_new(());
static EVENT_BRIDGE_EPOCH: AtomicU64 = AtomicU64::new(0);
static TURN_QUEUE: OnceLock<Arc<Semaphore>> = OnceLock::new();
static ACTIVE_TURN_STATE: OnceLock<StdMutex<ActiveTurnState>> = OnceLock::new();
static PENDING_SERVER_REQUESTS: OnceLock<StdMutex<HashMap<String, super::codex::NormalizedEvent>>> =
    OnceLock::new();

fn pending_server_requests() -> &'static StdMutex<HashMap<String, super::codex::NormalizedEvent>> {
    PENDING_SERVER_REQUESTS.get_or_init(|| StdMutex::new(HashMap::new()))
}

fn turn_queue() -> &'static Arc<Semaphore> {
    TURN_QUEUE.get_or_init(|| Arc::new(Semaphore::new(1)))
}

#[derive(Default)]
struct ActiveTurnState {
    setup_in_progress: bool,
    epoch: Option<u64>,
    thread_id: Option<String>,
    turn_id: Option<String>,
    completion_during_setup: Option<(String, String)>,
    connection_closed_during_setup: bool,
    permit: Option<OwnedSemaphorePermit>,
}

impl ActiveTurnState {
    fn finish_setup(&mut self, failed: bool) {
        self.setup_in_progress = false;
        let completed = self
            .thread_id
            .as_ref()
            .zip(self.turn_id.as_ref())
            .is_some_and(|(thread, turn)| {
                self.completion_during_setup.as_ref() == Some(&(thread.clone(), turn.clone()))
            });
        if (failed && self.turn_id.is_none()) || self.connection_closed_during_setup || completed {
            *self = Self::default();
        }
    }
}

fn active_turn_state() -> &'static StdMutex<ActiveTurnState> {
    ACTIVE_TURN_STATE.get_or_init(|| StdMutex::new(ActiveTurnState::default()))
}

fn complete_active_turn(epoch: u64, thread_id: &str, turn_id: &str) {
    let mut state = active_turn_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if state.epoch != Some(epoch) || state.thread_id.as_deref() != Some(thread_id) {
        return;
    }
    if state.setup_in_progress {
        state.completion_during_setup = Some((thread_id.to_string(), turn_id.to_string()));
    } else if state.turn_id.as_deref() == Some(turn_id) {
        let thread_id = thread_id.to_owned();
        let turn_id = turn_id.to_owned();
        if super::research::jobs::has_active() {
            tokio::spawn(async move {
                let _ = run_store(move |store| {
                    super::research::cancel_executions_for_provider_turn(
                        &store, &thread_id, &turn_id,
                    )
                })
                .await;
            });
        }
        *state = ActiveTurnState::default();
    }
}

fn close_active_connection(epoch: u64) {
    let mut state = active_turn_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if state.epoch != Some(epoch) {
        return;
    }
    if let (Some(thread), Some(turn)) = (&state.thread_id, &state.turn_id) {
        let thread = thread.clone();
        let turn = turn.clone();
        if super::research::jobs::has_active() {
            tokio::spawn(async move {
                let _ = run_store(move |store| {
                    super::research::cancel_executions_for_provider_turn(&store, &thread, &turn)
                })
                .await;
            });
        }
    }
    if state.setup_in_progress {
        state.connection_closed_during_setup = true;
    } else {
        *state = ActiveTurnState::default();
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCommandError {
    code: String,
    message: String,
    retryable: bool,
    recovery: Option<String>,
}

impl From<WorkbenchError> for RuntimeCommandError {
    fn from(error: WorkbenchError) -> Self {
        Self {
            code: error.code,
            message: error.message,
            retryable: error.retryable,
            recovery: error.recovery,
        }
    }
}

impl From<super::codex::RequestError> for RuntimeCommandError {
    fn from(error: super::codex::RequestError) -> Self {
        Self {
            code: error.code,
            message: error.message,
            retryable: error.retryable,
            recovery: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendTurnRequest {
    pub(crate) session_id: String,
    pub(crate) text: String,
    pub(crate) client_submission_id: String,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendTurnResult {
    pub(crate) epoch: u64,
    pub(crate) thread_id: String,
    pub(crate) turn_id: String,
    pub(crate) draft_cleared: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveServerRequest {
    epoch: u64,
    request_id: Value,
    method: String,
    result: Option<Value>,
    decline_message: Option<String>,
}

pub(crate) async fn run_store<T, F>(operation: F) -> WorkbenchResult<T>
where
    T: Send + 'static,
    F: FnOnce(Store) -> WorkbenchResult<T> + Send + 'static,
{
    let _gate = DATABASE_GATE.read().await;
    run_store_ungated(operation).await
}

pub(crate) async fn run_store_owned<T, F>(store: Store, operation: F) -> WorkbenchResult<T>
where
    T: Send + 'static,
    F: FnOnce(Store) -> WorkbenchResult<T> + Send + 'static,
{
    let _gate = DATABASE_GATE.read().await;
    let _permit = DATABASE_WORKERS
        .acquire()
        .await
        .map_err(|_| WorkbenchError::worker("Database workers are unavailable", true))?;
    tokio::task::spawn_blocking(move || operation(store))
        .await
        .map_err(|e| WorkbenchError::worker(format!("Database worker ended: {e}"), true))?
}

async fn run_store_ungated<T, F>(operation: F) -> WorkbenchResult<T>
where
    T: Send + 'static,
    F: FnOnce(Store) -> WorkbenchResult<T> + Send + 'static,
{
    let _permit = DATABASE_WORKERS
        .acquire()
        .await
        .map_err(|_| WorkbenchError::worker("Workbench database workers are unavailable", true))?;
    tokio::task::spawn_blocking(move || operation(Store::open_default()?))
        .await
        .map_err(|error| {
            WorkbenchError::worker(format!("Workbench database worker stopped: {error}"), true)
        })?
}

async fn run_store_exclusive<T, F>(operation: F) -> WorkbenchResult<T>
where
    T: Send + 'static,
    F: FnOnce(Store) -> WorkbenchResult<T> + Send + 'static,
{
    let _gate = DATABASE_GATE.write().await;
    run_store_ungated(operation).await
}

fn storage_turn_permit(queue: Arc<Semaphore>) -> WorkbenchResult<OwnedSemaphorePermit> {
    queue.try_acquire_owned().map_err(|_| {
        WorkbenchError::conflict(
            "Stop or finish the Workspace turn before changing private storage",
        )
    })
}

async fn run_storage_maintenance<T, F>(operation: F) -> WorkbenchResult<T>
where
    T: Send + 'static,
    F: FnOnce(Store) -> WorkbenchResult<T> + Send + 'static,
{
    // The native permit also covers setup before a turn has a persisted identity.
    // The exclusive database gate prevents local jobs/captures starting mid-move.
    let _turn = storage_turn_permit(turn_queue().clone())?;
    run_store_exclusive(move |store| {
        if super::research::jobs::has_active() {
            return Err(WorkbenchError::conflict(
                "Stop or finish local jobs before changing private storage",
            ));
        }
        operation(store)
    })
    .await
}

#[tauri::command]
pub async fn workbench_create_workspace(
    request: CreateWorkspaceRequest,
) -> WorkbenchResult<Mutation<Workspace>> {
    run_store(move |store| store.create_workspace(request)).await
}

#[tauri::command]
pub async fn workbench_register_workspace_root(
    request: RegisterWorkspaceRootRequest,
) -> WorkbenchResult<Mutation<Workspace>> {
    run_store(move |store| store.register_workspace_root(request)).await
}

#[tauri::command]
pub async fn workbench_clear_workspace_root(
    request: ClearWorkspaceRootRequest,
) -> WorkbenchResult<Mutation<Workspace>> {
    run_store(move |store| store.clear_workspace_root(request)).await
}

#[tauri::command]
pub async fn workbench_get_workspace(workspace_id: String) -> WorkbenchResult<Workspace> {
    run_store(move |store| store.workspace(&workspace_id)).await
}

#[tauri::command]
pub async fn workbench_list_workspaces(include_archived: bool) -> WorkbenchResult<WorkspaceList> {
    run_store(move |store| store.list_workspaces(include_archived)).await
}

#[tauri::command]
pub async fn workbench_update_workspace(
    request: UpdateWorkspaceRequest,
) -> WorkbenchResult<Mutation<Workspace>> {
    run_store(move |store| store.update_workspace(request)).await
}

#[tauri::command]
pub async fn workbench_create_session(
    request: CreateSessionRequest,
) -> WorkbenchResult<Mutation<WorkbenchSession>> {
    run_store(move |store| store.create_session(request)).await
}

#[tauri::command]
pub async fn workbench_update_session(
    request: UpdateSessionRequest,
) -> WorkbenchResult<Mutation<WorkbenchSession>> {
    run_store(move |store| store.update_session(request)).await
}

/// Refuses while the conversation's native thread is mid-turn or a turn is
/// being set up, so lifecycle edits never race the single active turn.
async fn ensure_session_idle(session_id: String) -> WorkbenchResult<()> {
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
    let _permit = storage_turn_permit(turn_queue().clone())?;
    ensure_session_idle(request.session_id.clone()).await?;
    run_store(move |store| store.move_session(request)).await
}

#[tauri::command]
pub async fn workbench_delete_session(request: DeleteSessionRequest) -> WorkbenchResult<i64> {
    let _permit = storage_turn_permit(turn_queue().clone())?;
    ensure_session_idle(request.session_id.clone()).await?;
    run_store(move |store| store.delete_session(request)).await
}

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
    let supervisor = super::codex::supervisor_manager().connect().await?;
    ensure_event_bridge(app.clone(), supervisor.clone());
    generate_session_title(&app, &supervisor, session_id, true)
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

/// One title side call. Returns `Ok(None)` when nothing should change: titles
/// are disabled, the user already named the conversation, or no completed
/// exchange exists yet. `force` skips the first two checks.
async fn generate_session_title(
    app: &tauri::AppHandle,
    supervisor: &Arc<super::codex::AppServerSupervisor>,
    session_id: String,
    force: bool,
) -> Result<Option<String>, RuntimeCommandError> {
    let preferences = run_store(|store| titles::load_preferences(&store)).await?;
    if !force && !preferences.enabled {
        return Ok(None);
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
    let catalog = supervisor.model_catalog().await?;
    let (model, effort) = match titles::choose_model(&catalog.models, &preferences) {
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
        .run_side_turn(super::codex::SideTurnRequest {
            cwd: scratch.path().to_string_lossy().into_owned(),
            permissions: "workbench-inspect".to_string(),
            developer_instructions: titles::TITLE_INSTRUCTIONS.to_string(),
            text: titles::title_prompt(&user, &assistant),
            model,
            effort,
            timeout: TITLE_CALL_TIMEOUT,
        })
        .await?;
    drop(scratch);
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
                operation_id: format!("auto-title-{}", super::store::operation_suffix()?),
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
            "epoch": supervisor.status().epoch,
            "sessionId": session_id,
            "title": title
        }),
    );
    Ok(Some(title))
}

/// Background title generation after a conversation turn completes.
fn spawn_title_generation(
    app: tauri::AppHandle,
    supervisor: Arc<super::codex::AppServerSupervisor>,
    thread_id: String,
    turn_id: String,
) {
    tokio::spawn(async move {
        let task_thread = thread_id.clone();
        if run_store(move |s| super::tasks::is_task_turn(&s, &task_thread, &turn_id))
            .await
            .unwrap_or(true)
        {
            return;
        }
        let namespace = supervisor.runtime_namespace();
        let lookup_thread = thread_id.clone();
        let session = run_store(move |store| store.session_for_thread(&namespace, &lookup_thread))
            .await
            .ok()
            .flatten();
        let Some(session_id) = session else {
            return;
        };
        if let Err(error) =
            generate_session_title(&app, &supervisor, session_id.clone(), false).await
        {
            let _ = app.emit(
                "workbench:event",
                json!({
                    "kind": "sessionTitleFailed",
                    "epoch": supervisor.status().epoch,
                    "sessionId": session_id,
                    "error": error.message
                }),
            );
        }
    });
}

#[tauri::command]
pub async fn workbench_list_sessions(
    workspace_id: Option<String>,
    include_archived: bool,
) -> WorkbenchResult<SessionList> {
    run_store(move |store| store.list_sessions(workspace_id.as_deref(), include_archived)).await
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

mod codex;
mod desk;
mod project;
mod research;

#[cfg(test)]
#[path = "commands/tests.rs"]
mod tests;

#[cfg(test)]
use crate::agent_runtime::codex::is_safe_auth_url;
#[cfg(test)]
use codex::{validate_pending_request, validate_server_response};

pub use codex::*;
use codex::{ensure_event_bridge, transcript_text};
pub use desk::*;
pub use project::*;
pub use research::*;
