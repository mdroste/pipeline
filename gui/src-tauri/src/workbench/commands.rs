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
    pub(crate) thread_id: String,
    pub(crate) turn_id: String,
    pub(crate) draft_cleared: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveServerRequest {
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
    ensure_session_idle(request.session_id.clone()).await?;
    run_store(move |store| store.move_session(request)).await
}

#[tauri::command]
pub async fn workbench_delete_session(request: DeleteSessionRequest) -> WorkbenchResult<i64> {
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

#[tauri::command]
pub async fn workbench_harness_catalog(
    workspace_id: Option<String>,
) -> WorkbenchResult<super::research::HarnessCatalog> {
    run_store(move |store| super::research::harness_catalog(&store, workspace_id.as_deref())).await
}

#[tauri::command]
pub async fn workbench_native_prompt_catalog() -> WorkbenchResult<super::codex::native_prompts::NativePromptCatalog> {
    let home = run_store(|store| Ok(store.codex_home_path())).await?;
    super::codex::native_prompts::catalog(home).await.map_err(WorkbenchError::invalid)
}

#[tauri::command]
pub async fn workbench_effective_harness(
    session_id: String,
) -> WorkbenchResult<super::research::EffectiveHarness> {
    run_store(move |store| super::research::resolve_harness(&store, &session_id)).await
}

#[tauri::command]
pub async fn workbench_get_workspace_config(
    workspace_id: Option<String>,
) -> WorkbenchResult<super::research::WorkspaceConfig> {
    run_store(move |store| super::research::load_config(&store, workspace_id.as_deref())).await
}

#[tauri::command]
pub async fn workbench_save_workspace_config(
    request: super::research::SaveWorkspaceConfigRequest,
) -> WorkbenchResult<super::research::WorkspaceConfig> {
    run_store(move |store| super::research::save_config(&store, request)).await
}

#[tauri::command]
pub async fn workbench_clone_preset(
    request: super::research::ClonePresetRequest,
) -> WorkbenchResult<super::research::HarnessPreset> {
    run_store(move |store| super::research::clone_preset(&store, request)).await
}

#[tauri::command]
pub async fn workbench_update_preset(
    request: super::research::UpdatePresetRequest,
) -> WorkbenchResult<super::research::HarnessPreset> {
    run_store(move |store| super::research::update_preset(&store, request)).await
}

#[tauri::command]
pub async fn workbench_create_note(
    request: super::research::CreateNoteRequest,
) -> WorkbenchResult<super::research::ResearchNote> {
    run_store(move |store| super::research::create_note(&store, request, false)).await
}

#[tauri::command]
pub async fn workbench_update_note(
    request: super::research::UpdateNoteRequest,
) -> WorkbenchResult<super::research::ResearchNote> {
    run_store(move |store| super::research::update_note(&store, request)).await
}

#[tauri::command]
pub async fn workbench_list_notes(
    workspace_id: String,
    include_rejected: bool,
) -> WorkbenchResult<Vec<super::research::ResearchNote>> {
    run_store(move |store| super::research::list_notes(&store, &workspace_id, include_rejected))
        .await
}

#[tauri::command]
pub async fn workbench_import_paper(
    request: super::research::ImportPaperRequest,
) -> WorkbenchResult<super::research::PaperWithRevision> {
    run_store(move |store| super::research::import_paper(&store, request)).await
}

#[tauri::command]
pub async fn workbench_list_papers(
    workspace_id: String,
) -> WorkbenchResult<Vec<super::research::PaperWithRevision>> {
    run_store(move |store| super::research::list_papers(&store, &workspace_id)).await
}

#[tauri::command]
pub async fn workbench_paper_read(
    request: super::research::PaperReadRequest,
) -> WorkbenchResult<super::research::PaperReadResult> {
    run_store(move |store| super::research::paper_read(&store, request)).await
}

#[tauri::command]
pub async fn workbench_paper_search(
    workspace_id: String,
    revision_id: String,
    query: String,
    limit: usize,
) -> WorkbenchResult<Vec<super::research::PaperSearchHit>> {
    run_store(move |store| {
        super::research::paper_search(&store, &workspace_id, &revision_id, &query, limit)
    })
    .await
}

#[tauri::command]
pub async fn workbench_import_source(
    request: super::research::ImportSourceRequest,
) -> WorkbenchResult<super::research::SourceImportResult> {
    run_store(move |store| super::research::import_source(&store, request)).await
}

#[tauri::command]
pub async fn workbench_list_sources(
    workspace_id: String,
) -> WorkbenchResult<Vec<super::research::SourceRecord>> {
    run_store(move |store| super::research::list_sources(&store, &workspace_id)).await
}

#[tauri::command]
pub async fn workbench_propose_claim(
    request: super::research::ProposeClaimRequest,
) -> WorkbenchResult<super::research::ClaimRecord> {
    run_store(move |store| super::research::propose_claim(&store, request, false)).await
}

#[tauri::command]
pub async fn workbench_set_claim_state(
    request: super::research::SetClaimStateRequest,
) -> WorkbenchResult<super::research::ClaimRecord> {
    run_store(move |store| super::research::set_claim_state(&store, request)).await
}

#[tauri::command]
pub async fn workbench_propose_evidence(
    request: super::research::ProposeEvidenceRequest,
) -> WorkbenchResult<super::research::EvidenceRecord> {
    run_store(move |store| super::research::propose_evidence(&store, request, false)).await
}

#[tauri::command]
pub async fn workbench_confirm_evidence(
    request: super::research::ConfirmEvidenceRequest,
) -> WorkbenchResult<super::research::EvidenceRecord> {
    run_store(move |store| super::research::confirm_evidence(&store, request)).await
}

#[tauri::command]
pub async fn workbench_research_ledger(
    workspace_id: String,
) -> WorkbenchResult<super::research::ResearchLedger> {
    run_store(move |store| super::research::research_ledger(&store, &workspace_id)).await
}

#[tauri::command]
pub async fn workbench_save_execution_profile(
    request: super::research::SaveExecutionProfileRequest,
) -> WorkbenchResult<super::research::ExecutionProfile> {
    run_store(move |store| super::research::save_execution_profile(&store, request)).await
}

#[tauri::command]
pub async fn workbench_list_execution_profiles(
    workspace_id: String,
) -> WorkbenchResult<Vec<super::research::ExecutionProfile>> {
    run_store(move |store| super::research::list_execution_profiles(&store, &workspace_id)).await
}

#[tauri::command]
pub async fn workbench_run_execution(
    request: super::research::RunExecutionRequest,
) -> WorkbenchResult<super::research::ResearchExecution> {
    let _turn_guard = turn_queue().clone().try_acquire_owned().map_err(|_| {
        WorkbenchError::conflict(
            "Wait for the active research turn before starting a detached local job",
        )
    })?;
    let receipt =
        run_store(move |store| super::research::jobs::queue(&store, request, "detached", None))
            .await?;
    super::research::jobs::launch_pending();
    Ok(receipt)
}

#[tauri::command]
pub async fn workbench_list_executions(
    workspace_id: String,
) -> WorkbenchResult<Vec<super::research::ResearchExecution>> {
    run_store(move |store| super::research::list_executions(&store, &workspace_id)).await
}

#[tauri::command]
pub async fn workbench_cancel_execution(
    request: super::research::CancelExecutionRequest,
) -> WorkbenchResult<()> {
    run_store(move |store| super::research::cancel_execution(&store, request)).await
}

#[tauri::command]
pub async fn workbench_compare_results(
    request: super::research::CompareResultsRequest,
) -> WorkbenchResult<super::research::ResultComparison> {
    run_store(move |_store| super::research::compare_results(request)).await
}

#[tauri::command]
pub async fn workbench_record_structured_result(
    request: super::research::RecordStructuredResultRequest,
) -> WorkbenchResult<super::research::ResearchResultV1> {
    run_store(move |store| super::research::record_structured_result(&store, request)).await
}

#[tauri::command]
pub async fn workbench_list_structured_results(
    workspace_id: String,
) -> WorkbenchResult<Vec<super::research::ResearchResultV1>> {
    run_store(move |store| super::research::list_structured_results(&store, &workspace_id)).await
}

#[tauri::command]
pub async fn workbench_verify_evidence_results(
    request: super::research::VerifyEvidenceResultsRequest,
) -> WorkbenchResult<super::research::VerificationRecord> {
    run_store(move |store| super::research::verify_evidence_results(&store, request)).await
}

#[tauri::command]
pub async fn workbench_list_recipes(
    workspace_id: Option<String>,
) -> WorkbenchResult<Vec<super::release::ResearchRecipe>> {
    run_store(move |store| super::release::list_recipes(&store, workspace_id.as_deref())).await
}

#[tauri::command]
pub async fn workbench_clone_recipe(
    request: super::release::CloneRecipeRequest,
) -> WorkbenchResult<super::release::ResearchRecipe> {
    run_store(move |store| super::release::clone_recipe(&store, request)).await
}

#[tauri::command]
pub async fn workbench_update_recipe(
    request: super::release::UpdateRecipeRequest,
) -> WorkbenchResult<super::release::ResearchRecipe> {
    run_store(move |store| super::release::update_recipe(&store, request)).await
}

#[tauri::command]
pub async fn workbench_check_recipe_inputs(
    session_id: String,
    recipe_id: String,
) -> WorkbenchResult<Vec<super::release::InputCheck>> {
    run_store(move |store| super::release::check_recipe_inputs(&store, &session_id, &recipe_id))
        .await
}

#[tauri::command]
pub async fn workbench_start_recipe(
    request: super::release::StartRecipeRequest,
) -> WorkbenchResult<super::release::RecipeRun> {
    run_store(move |store| super::release::start_recipe(&store, request)).await
}

#[tauri::command]
pub async fn workbench_complete_recipe(
    request: super::release::CompleteRecipeRequest,
) -> WorkbenchResult<super::release::RecipeRun> {
    run_store(move |store| super::release::complete_recipe(&store, request)).await
}

#[tauri::command]
pub async fn workbench_list_recipe_runs(
    session_id: String,
) -> WorkbenchResult<Vec<super::release::RecipeRun>> {
    run_store(move |store| super::release::list_recipe_runs(&store, &session_id)).await
}

#[tauri::command]
pub async fn workbench_performance_budgets() -> Vec<super::release::PerformanceBudget> {
    super::release::performance_budgets()
}

#[tauri::command]
pub async fn workbench_record_performance(
    request: super::release::RecordPerformanceRequest,
) -> WorkbenchResult<Value> {
    run_store(move |store| super::release::record_performance(&store, request)).await
}

#[tauri::command]
pub async fn workbench_record_research_evaluation(
    request: super::release::RecordEvaluationRequest,
) -> WorkbenchResult<super::release::EvaluationRecord> {
    run_store(move |store| super::release::record_evaluation(&store, request)).await
}

#[tauri::command]
pub async fn workbench_list_research_evaluations(
    fixture_id: Option<String>,
) -> WorkbenchResult<Vec<super::release::EvaluationRecord>> {
    run_store(move |store| super::release::list_evaluations(&store, fixture_id.as_deref())).await
}

#[tauri::command]
pub async fn workbench_prepare_review_handoff(
    request: super::release::PrepareReviewHandoffRequest,
) -> WorkbenchResult<super::release::ReviewHandoff> {
    run_store(move |store| super::release::prepare_review_handoff(&store, request)).await
}

#[tauri::command]
pub async fn workbench_link_review_handoff(
    request: super::release::LinkReviewHandoffRequest,
) -> WorkbenchResult<super::release::ReviewHandoff> {
    run_store(move |store| super::release::link_review_handoff(&store, request)).await
}

#[tauri::command]
pub async fn workbench_export_research_archive(
    request: super::release::ExportArchiveRequest,
) -> WorkbenchResult<super::release::ArchiveReport> {
    run_store_exclusive(move |store| {
        if super::research::jobs::has_active() {
            return Err(WorkbenchError::invalid(
                "Stop local jobs before exporting a consistent archive",
            ));
        }
        super::release::refresh_retained_blobs(&store)?;
        super::release::export_archive(&store, request)
    })
    .await
}

#[tauri::command]
pub async fn workbench_preview_project_exchange(
    workspace_id: String,
    selection: super::release::ExchangeSelection,
) -> WorkbenchResult<super::release::ExchangePreview> {
    run_store(move |store| super::release::preview_exchange(&store, &workspace_id, &selection))
        .await
}
#[tauri::command]
pub async fn workbench_export_project_exchange(
    request: super::release::ExportExchangeRequest,
) -> WorkbenchResult<super::release::ExchangeReport> {
    run_store(move |store| super::release::export_exchange(&store, request)).await
}
#[tauri::command]
pub async fn workbench_inspect_project_exchange(
    path: String,
) -> WorkbenchResult<super::release::ExchangeInspection> {
    run_store(move |store| super::release::inspect_exchange(&store, &path)).await
}
#[tauri::command]
pub async fn workbench_preview_project_exchange_import(
    path: String,
    target: super::release::ExchangeTarget,
) -> WorkbenchResult<super::release::ImportPreview> {
    run_store(move |store| super::release::preview_import(&store, &path, &target)).await
}
#[tauri::command]
pub async fn workbench_import_project_exchange(
    request: super::release::ImportExchangeRequest,
) -> WorkbenchResult<super::release::ImportReport> {
    run_store(move |store| super::release::import_exchange(&store, request)).await
}
#[tauri::command]
pub async fn workbench_list_exchange_conflicts(
    workspace_id: String,
) -> WorkbenchResult<Vec<super::release::ExchangeConflict>> {
    run_store(move |store| super::release::list_conflicts(&store, &workspace_id)).await
}
#[tauri::command]
pub async fn workbench_resolve_exchange_conflict(
    workspace_id: String,
    conflict_id: String,
    take_imported: bool,
) -> WorkbenchResult<super::release::ExchangeConflict> {
    run_store(move |store| {
        super::release::resolve_conflict(&store, &workspace_id, &conflict_id, take_imported)
    })
    .await
}
#[tauri::command]
pub async fn workbench_storage_report() -> WorkbenchResult<super::release::StorageReport> {
    run_store_exclusive(move |store| super::release::storage_report(&store)).await
}
#[tauri::command]
pub async fn workbench_prune_storage(
    request: super::release::PruneRequest,
) -> WorkbenchResult<super::release::PrunePlan> {
    if request.apply {
        run_storage_maintenance(move |store| super::release::prune_storage(&store, request)).await
    } else {
        run_store(move |store| super::release::prune_storage(&store, request)).await
    }
}
#[tauri::command]
pub async fn workbench_restore_trash(
    trash_id: String,
) -> WorkbenchResult<super::release::TrashEntry> {
    run_storage_maintenance(move |store| super::release::restore_trash(&store, &trash_id)).await
}
#[tauri::command]
pub async fn workbench_empty_trash() -> WorkbenchResult<usize> {
    run_storage_maintenance(move |store| super::release::empty_trash(&store)).await
}
#[tauri::command]
pub async fn workbench_draft_workflow(
    request: super::release::DraftWorkflowRequest,
) -> WorkbenchResult<super::release::DraftWorkflow> {
    run_store(move |store| super::release::draft_workflow(&store, request)).await
}
#[tauri::command]
pub async fn workbench_inspect_research_archive(
    request: super::release::ExportArchiveRequest,
) -> WorkbenchResult<super::release::ArchiveInspection> {
    run_store(move |_store| super::release::inspect_archive(request)).await
}

#[tauri::command]
pub async fn workbench_import_research_archive(
    request: super::release::ImportArchiveRequest,
) -> WorkbenchResult<super::release::ArchiveReport> {
    run_store_exclusive(move |store| {
        let state = active_turn_state()
            .lock()
            .map_err(|_| WorkbenchError::worker("Workspace turn state is unavailable", true))?;
        if state.setup_in_progress || state.permit.is_some() || super::research::jobs::has_active()
        {
            return Err(WorkbenchError::invalid(
                "Stop the active Workspace turn before restoring a research archive",
            ));
        }
        super::release::import_archive(&store, request)
    })
    .await
}

#[tauri::command]
pub async fn workbench_export_conversation(
    session_id: String,
    path: String,
) -> WorkbenchResult<()> {
    run_store(move |store| {
        let target = std::path::PathBuf::from(path);
        if !target.is_absolute()
            || target.extension().and_then(|value| value.to_str()) != Some("md")
        {
            return Err(WorkbenchError::invalid(
                "Conversation exports require an absolute .md path",
            ));
        }
        let snapshot = store.conversation_snapshot(&session_id)?;
        let mut markdown = format!("# {}\n\n", snapshot.session.title);
        for item in snapshot.items {
            if !item.item_kind.to_ascii_lowercase().contains("message") {
                continue;
            }
            let Some(text) = transcript_text(&item.payload) else {
                continue;
            };
            let role = if item.item_kind.to_ascii_lowercase().contains("user") {
                "You"
            } else {
                "ChatGPT"
            };
            markdown.push_str(&format!("## {role}\n\n{text}\n\n"));
        }
        if markdown.len() > 64 * 1024 * 1024 {
            return Err(WorkbenchError::invalid(
                "Conversation export exceeds 64 MiB",
            ));
        }
        std::fs::write(&target, markdown).map_err(|error| {
            WorkbenchError::storage("Failed to write conversation export", error)
        })?;
        std::fs::File::open(&target)
            .and_then(|file| file.sync_all())
            .map_err(|error| {
                WorkbenchError::storage("Failed to sync conversation export", error)
            })?;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn workbench_reconcile_workspace_roots(
    operation_id: String,
) -> WorkbenchResult<ReconcileResult> {
    run_store(move |store| store.reconcile_workspace_roots(&operation_id)).await
}

#[tauri::command]
pub async fn workbench_codex_connect(
    app: tauri::AppHandle,
) -> Result<super::codex::SupervisorStatus, super::codex::RequestError> {
    let supervisor = super::codex::supervisor_manager().connect().await?;
    ensure_event_bridge(app, supervisor.clone());
    Ok(supervisor.status())
}

#[tauri::command]
pub async fn workbench_codex_account_state(
    app: tauri::AppHandle,
    refresh_token: bool,
) -> Result<super::codex::AccountState, super::codex::RequestError> {
    let supervisor = super::codex::supervisor_manager().connect().await?;
    ensure_event_bridge(app, supervisor.clone());
    supervisor.account_state(refresh_token).await
}

#[tauri::command]
pub async fn workbench_codex_login_start(
    app: tauri::AppHandle,
) -> Result<super::codex::LoginStart, super::codex::RequestError> {
    let supervisor = super::codex::supervisor_manager().connect().await?;
    ensure_event_bridge(app.clone(), supervisor.clone());
    let login = supervisor.login_start().await?;
    if !is_safe_auth_url(&login.auth_url) {
        let _ = supervisor.login_cancel(&login.login_id).await;
        return Err(super::codex::RequestError::unavailable(
            "Codex App Server returned an unsafe authentication URL",
        ));
    }
    if let Err(error) = open_auth_url(&app, &login.auth_url) {
        let _ = supervisor.login_cancel(&login.login_id).await;
        return Err(super::codex::RequestError::unavailable(format!(
            "Could not open the ChatGPT sign-in page: {error}"
        )));
    }
    Ok(login)
}

#[tauri::command]
pub async fn workbench_codex_login_cancel(
    login_id: String,
) -> Result<bool, super::codex::RequestError> {
    super::codex::supervisor_manager()
        .connect()
        .await?
        .login_cancel(&login_id)
        .await
}

#[tauri::command]
pub async fn workbench_codex_logout() -> Result<(), super::codex::RequestError> {
    super::codex::supervisor_manager()
        .connect()
        .await?
        .logout()
        .await
}

#[tauri::command]
pub async fn workbench_codex_model_catalog(
) -> Result<super::codex::ModelCatalog, super::codex::RequestError> {
    super::codex::supervisor_manager()
        .connect()
        .await?
        .model_catalog()
        .await
}

#[tauri::command]
pub async fn workbench_codex_validate_model_selection(
    model: String,
    effort: Option<String>,
) -> Result<super::codex::WorkspaceModel, super::codex::RequestError> {
    super::codex::supervisor_manager()
        .connect()
        .await?
        .validate_model_selection(&model, effort.as_deref())
        .await
}

#[tauri::command]
pub async fn workbench_codex_rate_limits(
) -> Result<super::codex::RateLimits, super::codex::RequestError> {
    super::codex::supervisor_manager()
        .connect()
        .await?
        .rate_limits()
        .await
}

#[tauri::command]
pub async fn workbench_codex_send_turn(
    app: tauri::AppHandle,
    request: SendTurnRequest,
) -> Result<SendTurnResult, RuntimeCommandError> {
    submit_turn(app, request, None).await
}

pub(crate) fn task_turn_available() -> bool {
    turn_queue().available_permits() > 0
}

pub(crate) async fn submit_turn(
    app: tauri::AppHandle,
    request: SendTurnRequest,
    task_binding: Option<super::tasks::TaskBinding>,
) -> Result<SendTurnResult, RuntimeCommandError> {
    let automated = task_binding.is_some();
    let permit = if automated {
        turn_queue()
            .clone()
            .try_acquire_owned()
            .map_err(|_| RuntimeCommandError {
                code: "task_busy".into(),
                message: "task_busy: Workspace is running another turn".into(),
                retryable: true,
                recovery: None,
            })?
    } else {
        turn_queue()
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| RuntimeCommandError {
                code: "turn_queue_closed".into(),
                message: "The Workspace turn queue is unavailable".into(),
                retryable: true,
                recovery: None,
            })?
    };
    if let Some(binding) = task_binding {
        run_store(move |store| super::tasks::validate_binding(&store, &binding)).await?;
    }
    {
        let mut state = active_turn_state()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.setup_in_progress = true;
        state.epoch = None;
        state.thread_id = None;
        state.turn_id = None;
        state.completion_during_setup = None;
        state.connection_closed_during_setup = false;
        state.permit = None;
    }

    let result: Result<SendTurnResult, RuntimeCommandError> = async {
        if !automated {
        let draft_operation = format!("send-draft-{}", request.client_submission_id);
        let session_id = request.session_id.clone();
        let text = request.text.clone();
        run_store(move |store| {
            for attempt in 0..2 {
                let snapshot = store.session_snapshot(&session_id)?;
                if snapshot.session.draft == text {
                    return Ok(());
                }
                match store.update_session(UpdateSessionRequest {
                    session_id: session_id.clone(),
                    expected_revision: snapshot.session.revision,
                    operation_id: draft_operation.clone(),
                    title: None,
                    draft: Some(text.clone()),
                    overrides: None,
                    archived: None,
                    preset_id: None,
                    paper_id: None,
                    clear_paper: None,
                }) {
                    Ok(_) => return Ok(()),
                    Err(error) if error.code == "storage_conflict" && attempt == 0 => continue,
                    Err(error) => return Err(error),
                }
            }
            Err(WorkbenchError::worker(
                "Workspace draft changed repeatedly while starting the turn",
                true,
            ))
        })
        .await?;

        }

        let session_id = request.session_id.clone();
        let (binding, runtime_root, prepared) = run_store(move |store| {
            if store.session_snapshot(&session_id)?.session.workspace_id.as_deref().is_some_and(super::research::jobs::workspace_active) {return Err(WorkbenchError::conflict("This project has a local job holding its write resources. Finish or stop it before starting a research turn."));}
            Ok::<_, WorkbenchError>((
                store.active_binding(&session_id)?,
                store.runtime_root(&session_id)?,
                super::research::prepare_turn(&store, &session_id)?,
            ))
        })
        .await?;
        let root = runtime_root.to_string_lossy().into_owned();
        let permission_profile = prepared.effective.permission_profile.as_str();
        let supervisor = super::codex::supervisor_manager().connect().await?;
        active_turn_state()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .epoch = Some(supervisor.status().epoch);
        ensure_event_bridge(app, supervisor.clone());
        let connection = if let Some(binding) = binding.filter(|binding| {
            binding.harness_fingerprint.as_deref() == Some(&prepared.effective.fingerprint)
        }) {
            supervisor
                .resume_thread(
                    &request.session_id,
                    &binding.provider_thread_id,
                    permission_profile,
                    vec![root],
                )
                .await?
        } else {
            supervisor
                .start_thread(super::codex::StartThreadRequest {
                    base_instructions: prepared.effective.preset.base_instructions.clone(),
                    workbench_session_id: request.session_id.clone(),
                    cwd: root.clone(),
                    runtime_workspace_roots: vec![root],
                    permissions: permission_profile.to_string(),
                    developer_instructions: prepared.effective.developer_instructions.clone(),
                    model: request.model.clone(),
                    effort: request.effort.clone(),
                    dynamic_tools: prepared.effective.dynamic_tools.clone(),
                })
                .await?
        };
        active_turn_state()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .thread_id = Some(connection.thread_id.clone());
        let binding_id =
            connection
                .workbench_binding_id
                .clone()
                .ok_or_else(|| RuntimeCommandError {
                    code: "binding_unavailable".to_string(),
                    message: "Workspace could not establish a durable conversation binding"
                        .to_string(),
                    retryable: true,
                    recovery: None,
                })?;
        let binding_for_snapshot = binding_id.clone();
        let fingerprint = prepared.effective.fingerprint.clone();
        let instruction_sources = connection.instruction_sources.clone();
        run_store(move |store| {
            store.set_binding_harness(&binding_for_snapshot, &fingerprint, &instruction_sources)
        })
        .await?;
        let turn_id = supervisor
            .start_turn(super::codex::StartTurnRequest {
                workbench_binding_id: binding_id.clone(),
                thread_id: connection.thread_id.clone(),
                text: request.text,
                client_user_message_id: request.client_submission_id.clone(),
                model: request.model,
                effort: request.effort,
            })
            .await?;
        active_turn_state()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .turn_id = Some(turn_id.clone());

        let snapshot_binding = binding_id.clone();
        let snapshot_submission = request.client_submission_id.clone();
        let config_snapshot = prepared.config_snapshot_id;
        let context_snapshot = prepared.context_snapshot_id;
        run_store(move |store| {
            store.attach_turn_snapshots(
                &snapshot_binding,
                &snapshot_submission,
                &config_snapshot,
                &context_snapshot,
            )
        })
        .await?;

        let draft_cleared = if automated { false } else {
        let clear_operation = format!("send-ack-{}", request.client_submission_id);
        let session_id = request.session_id;
        run_store(move |store| {
            let snapshot = store.session_snapshot(&session_id)?;
            store.update_session(UpdateSessionRequest {
                session_id,
                expected_revision: snapshot.session.revision,
                operation_id: clear_operation,
                title: None,
                draft: Some(String::new()),
                overrides: None,
                archived: None,
                preset_id: None,
                paper_id: None,
                clear_paper: None,
            })?;
            Ok::<_, WorkbenchError>(())
        })
        .await
        .is_ok()
        };
        Ok(SendTurnResult {
            thread_id: connection.thread_id,
            turn_id,
            draft_cleared,
        })
    }
    .await;
    let mut state = active_turn_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    state.setup_in_progress = false;
    let completed_current_turn = result.as_ref().is_ok_and(|completed| {
        state.completion_during_setup.as_ref()
            == Some(&(completed.thread_id.clone(), completed.turn_id.clone()))
    });
    if result.is_err() || state.connection_closed_during_setup || completed_current_turn {
        *state = ActiveTurnState::default();
    } else {
        state.permit = Some(permit);
        state.completion_during_setup = None;
    }
    drop(state);
    result
}

#[tauri::command]
pub async fn workbench_codex_interrupt_turn(
    thread_id: String,
    turn_id: String,
) -> Result<(), super::codex::RequestError> {
    let result = super::codex::supervisor_manager()
        .connect()
        .await?
        .interrupt_turn(&thread_id, &turn_id)
        .await;
    let cancellation_thread = thread_id.clone();
    let cancellation_turn = turn_id.clone();
    let _ = run_store(move |store| {
        super::research::cancel_executions_for_provider_turn(
            &store,
            &cancellation_thread,
            &cancellation_turn,
        )
    })
    .await;
    result
}

#[tauri::command]
pub async fn workbench_codex_reconcile_session(
    app: tauri::AppHandle,
    session_id: String,
) -> Result<bool, RuntimeCommandError> {
    let lookup_session_id = session_id.clone();
    let (binding, root, harness) = run_store(move |store| {
        Ok::<_, WorkbenchError>((
            store.active_binding(&lookup_session_id)?,
            store.runtime_root(&lookup_session_id)?,
            super::research::resolve_harness(&store, &lookup_session_id)?,
        ))
    })
    .await
    .map_err(RuntimeCommandError::from)?;
    let Some(binding) = binding else {
        return Ok(false);
    };
    if binding.harness_fingerprint.as_deref() != Some(&harness.fingerprint) {
        return Ok(false);
    }
    let permission_profile = match (harness.mode.as_str(), harness.command_network) {
        ("edit", true) => "workbench-edit-network",
        ("edit", false) => "workbench-edit",
        ("inspect", true) => "workbench-inspect-network",
        _ => "workbench-inspect",
    };
    let supervisor = super::codex::supervisor_manager().connect().await?;
    ensure_event_bridge(app, supervisor.clone());
    let connection = supervisor
        .resume_thread(
            &session_id,
            &binding.provider_thread_id,
            permission_profile,
            vec![root.to_string_lossy().into_owned()],
        )
        .await?;
    let binding_id = connection
        .workbench_binding_id
        .ok_or_else(|| RuntimeCommandError {
            code: "binding_unavailable".to_string(),
            message: "Workspace could not resume the durable conversation binding".to_string(),
            retryable: true,
            recovery: None,
        })?;
    supervisor
        .reconcile_thread(&binding_id, &connection.thread_id)
        .await?;
    Ok(true)
}

#[tauri::command]
pub async fn workbench_codex_resolve_server_request(
    request: ResolveServerRequest,
) -> Result<(), super::codex::RequestError> {
    let supervisor = super::codex::supervisor_manager().connect().await?;
    let key = request_key(&request.request_id);
    let pending = pending_server_requests()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(&key)
        .cloned()
        .ok_or_else(|| {
            super::codex::RequestError::invalid("Server request is no longer pending")
        })?;
    validate_pending_request(&pending, supervisor.status().epoch, &request.method)?;
    let response = if let Some(result) = request.result {
        validate_server_response(&request.method, &result)?;
        Ok(result)
    } else {
        Err(super::codex::RequestError::invalid(
            request
                .decline_message
                .unwrap_or_else(|| "The user declined this request".to_string()),
        ))
    };
    supervisor
        .respond_to_server_request(request.request_id, response)
        .await?;
    pending_server_requests()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(&key);
    Ok(())
}

#[tauri::command]
pub fn workbench_codex_pending_requests() -> Vec<super::codex::NormalizedEvent> {
    let mut pending = pending_server_requests()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .values()
        .cloned()
        .collect::<Vec<_>>();
    pending.sort_by_key(|event| match event {
        super::codex::NormalizedEvent::ServerRequest { request_id, .. } => request_key(request_id),
        _ => String::new(),
    });
    pending
}

fn ensure_event_bridge(app: tauri::AppHandle, supervisor: Arc<super::codex::AppServerSupervisor>) {
    let epoch = supervisor.status().epoch;
    if EVENT_BRIDGE_EPOCH.swap(epoch, Ordering::AcqRel) == epoch {
        return;
    }
    pending_server_requests()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();
    let mut events = supervisor.subscribe();
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    if EVENT_BRIDGE_EPOCH.load(Ordering::Acquire) != epoch {
                        return;
                    }
                    // Host-owned side calls (conversation titles) run in their
                    // own ephemeral threads; their events must not reach the
                    // pending-request registry, turn state, or the frontend.
                    if super::codex::event_thread_id(&event)
                        .is_some_and(|thread| supervisor.is_side_thread(thread))
                    {
                        continue;
                    }
                    if let super::codex::NormalizedEvent::ServerRequest {
                        request_id,
                        method,
                        params,
                        ..
                    } = &event
                    {
                        if method == "item/tool/call" {
                            let request_id = request_id.clone();
                            let params = params.clone();
                            let is_execution = params.get("tool").and_then(Value::as_str)
                                == Some("workbench_research_run");
                            let tool_supervisor = supervisor.clone();
                            let tool_app = app.clone();
                            tokio::spawn(async move {
                                let response = run_store(move |store| {
                                    super::research::handle_dynamic_tool_call(&store, &params)
                                })
                                .await;
                                super::research::jobs::launch_pending();
                                let mut response = match response {
                                    Ok(value) => value,
                                    Err(error) => json!({
                                        "success": false,
                                        "contentItems": [{
                                            "type": "inputText",
                                            "text": serde_json::to_string(&json!({
                                                "code": error.code,
                                                "message": error.message,
                                                "retryable": error.retryable
                                            })).unwrap_or_else(|_| "{\"error\":\"Research tool failed\"}".to_string())
                                        }]
                                    }),
                                };
                                if is_execution && response["success"] == true {
                                    let execution_id = response["contentItems"][0]["text"]
                                        .as_str()
                                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                                        .and_then(|v| v["id"].as_str().map(str::to_owned));
                                    if let Some(execution_id) = execution_id {
                                        while super::research::jobs::execution_active(&execution_id)
                                        {
                                            tokio::time::sleep(std::time::Duration::from_millis(
                                                75,
                                            ))
                                            .await;
                                        }
                                        if let Ok(finished) = run_store(move |store| {
                                            super::research::jobs::complete_tool_result(
                                                &store,
                                                &execution_id,
                                            )
                                        })
                                        .await
                                        {
                                            response = finished;
                                        }
                                    }
                                }
                                let success = response
                                    .get("success")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false);
                                let _ = tool_supervisor
                                    .respond_to_server_request(request_id, Ok(response))
                                    .await;
                                let _ = tool_app.emit(
                                    "workbench:event",
                                    json!({"kind":"researchToolCompleted","epoch":epoch,"success":success}),
                                );
                            });
                            continue;
                        }
                    }
                    match &event {
                        super::codex::NormalizedEvent::ServerRequest { request_id, .. } => {
                            pending_server_requests()
                                .lock()
                                .unwrap_or_else(|error| error.into_inner())
                                .insert(request_key(request_id), event.clone());
                        }
                        super::codex::NormalizedEvent::ServerRequestResolved {
                            request_id, ..
                        } => {
                            pending_server_requests()
                                .lock()
                                .unwrap_or_else(|error| error.into_inner())
                                .remove(&request_key(request_id));
                        }
                        super::codex::NormalizedEvent::ConnectionClosed { epoch, .. } => {
                            pending_server_requests()
                                .lock()
                                .unwrap_or_else(|error| error.into_inner())
                                .clear();
                            close_active_connection(*epoch);
                        }
                        super::codex::NormalizedEvent::TurnCompleted {
                            epoch,
                            thread_id,
                            turn_id,
                            status,
                            ..
                        } => {
                            complete_active_turn(*epoch, thread_id, turn_id);
                            if status == "completed" {
                                spawn_title_generation(
                                    app.clone(),
                                    supervisor.clone(),
                                    thread_id.clone(),
                                    turn_id.clone(),
                                );
                            }
                        }
                        _ => {}
                    }
                    let _ = app.emit("workbench:event", event);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    let _ = app.emit(
                        "workbench:event-lagged",
                        json!({"epoch":epoch,"skipped":skipped}),
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            }
        }
    });
}

fn request_key(request_id: &Value) -> String {
    serde_json::to_string(request_id).unwrap_or_else(|_| "invalid-request-id".to_string())
}

fn validate_pending_request(
    event: &super::codex::NormalizedEvent,
    active_epoch: u64,
    submitted_method: &str,
) -> Result<(), super::codex::RequestError> {
    match event {
        super::codex::NormalizedEvent::ServerRequest { epoch, method, .. }
            if *epoch == active_epoch && method == submitted_method =>
        {
            Ok(())
        }
        super::codex::NormalizedEvent::ServerRequest { epoch, .. } if *epoch != active_epoch => {
            Err(super::codex::RequestError::invalid(
                "Server request belongs to an expired connection",
            ))
        }
        super::codex::NormalizedEvent::ServerRequest { .. } => Err(
            super::codex::RequestError::invalid("Server request method did not match"),
        ),
        _ => Err(super::codex::RequestError::invalid(
            "Pending request registry contained a non-request event",
        )),
    }
}

fn validate_server_response(
    method: &str,
    result: &Value,
) -> Result<(), super::codex::RequestError> {
    match method {
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
            if !matches!(
                result.get("decision").and_then(Value::as_str),
                Some("accept" | "acceptForSession" | "decline" | "cancel")
            ) {
                return Err(super::codex::RequestError::invalid(
                    "Invalid approval decision",
                ));
            }
        }
        "item/tool/requestUserInput" => {
            if !result.get("answers").is_some_and(Value::is_object) {
                return Err(super::codex::RequestError::invalid(
                    "User-input response omitted answers",
                ));
            }
        }
        "item/permissions/requestApproval" => {
            if !result.get("permissions").is_some_and(Value::is_object) {
                return Err(super::codex::RequestError::invalid(
                    "Permission response omitted permissions",
                ));
            }
        }
        _ => {
            return Err(super::codex::RequestError::invalid(
                "Unsupported server request method",
            ))
        }
    }
    Ok(())
}

fn transcript_text(payload: &Value) -> Option<String> {
    for key in ["text", "message", "content"] {
        match payload.get(key) {
            Some(Value::String(text)) if !text.is_empty() => return Some(text.clone()),
            Some(Value::Array(parts)) => {
                let text = parts
                    .iter()
                    .filter_map(|part| {
                        part.as_str()
                            .or_else(|| part.get("text").and_then(Value::as_str))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.is_empty() {
                    return Some(text);
                }
            }
            _ => {}
        }
    }
    None
}

use crate::agent_runtime::codex::account::is_safe_auth_url;

// Pipeline already ships the shell plugin and grants only its URL-open
// capability. Keep this deprecated call isolated until the application moves
// all external URL handling to tauri-plugin-opener.
#[allow(deprecated)]
fn open_auth_url(app: &tauri::AppHandle, url: &str) -> Result<(), tauri_plugin_shell::Error> {
    app.shell().open(url.to_string(), None)
}

#[cfg(test)]
mod tests;

#[tauri::command]
pub async fn workbench_project_home(
    workspace_id: String,
) -> WorkbenchResult<super::project::ProjectHome> {
    run_store(move |store| super::project::home(&store, &workspace_id)).await
}
#[tauri::command]
pub async fn workbench_project_mutate(
    request: super::project::ProjectMutation,
) -> WorkbenchResult<Value> {
    // Hold the same permit as native turns across checkpoint and acceptance I/O.
    // Checking active state alone would race a turn starting immediately afterward.
    let _turn_guard = if matches!(
        request.action,
        super::project::ProjectAction::Checkpoint { .. }
            | super::project::ProjectAction::CaptureChanges { .. }
            | super::project::ProjectAction::Apply { .. }
            | super::project::ProjectAction::Undo { .. }
            | super::project::ProjectAction::Recover { .. }
            | super::project::ProjectAction::Reject { .. }
    ) {
        Some(turn_queue().clone().try_acquire_owned().map_err(|_| WorkbenchError::conflict("Wait for the active Workspace turn to finish or stop it before changing a task's files"))?)
    } else {
        None
    };
    run_store(move |store| super::project::mutate(&store, request)).await
}
#[tauri::command]
pub async fn workbench_document_read(
    workspace_id: String,
    revision_id: String,
    start: usize,
    page: Option<u32>,
) -> WorkbenchResult<super::project::DocumentView> {
    run_store(move |store| {
        super::project::read_document(&store, &workspace_id, &revision_id, start, page)
    })
    .await
}
#[tauri::command]
pub async fn workbench_anchor_mapping(
    workspace_id: String,
    anchor_id: String,
    revision_id: String,
) -> WorkbenchResult<super::project::AnchorMapping> {
    run_store(move |store| {
        super::project::map_anchor(&store, &workspace_id, &anchor_id, &revision_id)
    })
    .await
}
#[tauri::command]
pub async fn workbench_file_read(
    request: super::project::FileReadRequest,
) -> WorkbenchResult<super::project::FilePreview> {
    run_store(move |store| super::project::read_workspace_file(&store, request)).await
}
#[tauri::command]
pub async fn workbench_snapshot_preview(
    workspace_id: String,
    hash: String,
) -> WorkbenchResult<super::project::SnapshotPreview> {
    run_store(move |store| super::project::preview_snapshot(&store, &workspace_id, &hash)).await
}
#[tauri::command]
pub async fn workbench_task_session(
    workspace_id: String,
    checkpoint_id: String,
) -> WorkbenchResult<WorkbenchSession> {
    run_store(move |store| super::project::task_session(&store, &workspace_id, &checkpoint_id))
        .await
}

#[tauri::command]
pub async fn workbench_preview_host_execution(
    profile_id: String,
) -> WorkbenchResult<super::research::HostExecutionPreview> {
    run_store(move |store| super::research::preview_host_execution(&store, &profile_id)).await
}
#[tauri::command]
pub async fn workbench_authorize_host_execution(
    profile_id: String,
    fingerprint: String,
) -> WorkbenchResult<super::research::HostExecutionPreview> {
    run_store(move |store| {
        super::research::authorize_host_execution(&store, &profile_id, &fingerprint)
    })
    .await
}

#[tauri::command]
pub fn workbench_project_capabilities() -> Value {
    super::project::capabilities()
}

#[tauri::command]
pub async fn workbench_working_file_preview(
    workspace_id: String,
    path: String,
) -> WorkbenchResult<super::project::SnapshotPreview> {
    run_store(move |store| super::project::preview_working_file(&store, &workspace_id, &path)).await
}

#[tauri::command]
pub async fn workbench_list_jobs(
    workspace_id: String,
) -> WorkbenchResult<Vec<super::research::jobs::JobStatus>> {
    run_store(move |store| super::research::jobs::list_jobs(&store, &workspace_id)).await
}
#[tauri::command]
pub async fn workbench_job_log(
    workspace_id: String,
    execution_id: String,
    stream: String,
    offset: usize,
) -> WorkbenchResult<super::research::jobs::JobLogPage> {
    run_store(move |store| {
        super::research::jobs::log_page(&store, &workspace_id, &execution_id, &stream, offset)
    })
    .await
}
#[tauri::command]
pub async fn workbench_reconcile_job(
    workspace_id: String,
    execution_id: String,
) -> WorkbenchResult<super::research::ResearchExecution> {
    run_store(move |store| {
        super::research::jobs::reconcile_job(&store, &workspace_id, &execution_id)
    })
    .await
}

#[tauri::command]
pub async fn workbench_studio_mutate(
    request: super::project::StudioMutation,
) -> WorkbenchResult<Value> {
    let _turn_guard = if matches!(
        request.action,
        super::project::StudioAction::SaveText { .. }
            | super::project::StudioAction::GenerateValues { .. }
    ) {
        Some(turn_queue().clone().try_acquire_owned().map_err(|_| {
            WorkbenchError::conflict("Stop the active research turn before saving files")
        })?)
    } else {
        None
    };
    run_store(move |store| super::project::studio_mutate(&store, request)).await
}
#[tauri::command]
pub async fn workbench_studio_records(
    workspace_id: String,
    kind: String,
) -> WorkbenchResult<Vec<super::project::ProjectRecord>> {
    run_store(move |store| super::project::studio_records(&store, &workspace_id, &kind)).await
}
#[tauri::command]
pub async fn workbench_theory_overview(
    workspace_id: String,
) -> WorkbenchResult<super::project::TheoryOverview> {
    run_store(move |store| super::project::theory_overview(&store, &workspace_id)).await
}
#[tauri::command]
pub async fn workbench_studio_history(
    workspace_id: String,
    object_id: String,
) -> WorkbenchResult<Vec<Value>> {
    run_store(move |store| super::project::studio_history(&store, &workspace_id, &object_id)).await
}
#[tauri::command]
pub async fn workbench_editor_file(
    workspace_id: String,
    checkpoint_id: Option<String>,
    path: String,
) -> WorkbenchResult<super::project::EditorFile> {
    run_store(move |store| {
        super::project::editor_file(&store, &workspace_id, checkpoint_id.as_deref(), &path)
    })
    .await
}
#[tauri::command]
pub async fn workbench_editor_external(
    app: tauri::AppHandle,
    workspace_id: String,
    checkpoint_id: Option<String>,
    path: String,
) -> WorkbenchResult<()> {
    let path = run_store(move |store| {
        super::project::editor_external_path(&store, &workspace_id, checkpoint_id.as_deref(), &path)
    })
    .await?;
    #[allow(deprecated)]
    app.shell()
        .open(path, None)
        .map_err(|e| WorkbenchError::storage("Failed to open external editor", e))
}
#[tauri::command]
pub async fn workbench_build_sync(request: super::project::SyncRequest) -> WorkbenchResult<Value> {
    run_store(move |store| super::project::synchronize(&store, request)).await
}
#[tauri::command]
pub async fn workbench_workflow_finding_preview(run_id: String) -> WorkbenchResult<Value> {
    let report = crate::commands::get_run_report(run_id.clone())
        .await
        .map_err(WorkbenchError::invalid)?;
    let findings = crate::findings::canonical_findings(&report).ok_or_else(|| {
        WorkbenchError::invalid("The selected Workflow run has no canonical findings")
    })?;
    super::project::preview_findings(&super::project::FindingPackage {
        version: 1,
        run_id,
        source_step_id: findings.source_step_id,
        findings: findings.findings,
    })
}
#[tauri::command]
pub async fn workbench_finding_package_preview(
    package: super::project::FindingPackage,
) -> WorkbenchResult<Value> {
    super::project::preview_findings(&package)
}
#[tauri::command]
pub async fn workbench_report_preview(
    workspace_id: String,
    revision_id: String,
) -> WorkbenchResult<Vec<super::project::ReportComment>> {
    run_store(move |store| super::project::preview_report(&store, &workspace_id, &revision_id))
        .await
}
#[tauri::command]
pub async fn workbench_response_export(
    workspace_id: String,
    selected: Vec<String>,
    format: String,
) -> WorkbenchResult<Value> {
    run_store(move |store| {
        super::project::export_responses(&store, &workspace_id, &selected, &format)
    })
    .await
}
#[tauri::command]
pub async fn workbench_focus_review(
    request: super::project::FocusReviewRequest,
) -> WorkbenchResult<super::release::ReviewHandoff> {
    run_store(move |store| super::project::focus_review(&store, request)).await
}
#[tauri::command]
pub async fn workbench_compare_experiment(
    request: super::project::CompareExperimentRequest,
) -> WorkbenchResult<Value> {
    run_store(move |store| super::project::compare_experiment(&store, request)).await
}
#[tauri::command]
pub async fn workbench_compare_series(
    workspace_id: String,
    left_id: String,
    right_id: String,
    rationale: String,
) -> WorkbenchResult<Value> {
    run_store(move |store| {
        super::project::compare_series(&store, &workspace_id, &left_id, &right_id, &rationale)
    })
    .await
}
#[tauri::command]
pub async fn workbench_binding_coverage(workspace_id: String) -> WorkbenchResult<Vec<Value>> {
    run_store(move |store| super::project::binding_coverage(&store, &workspace_id)).await
}
#[tauri::command]
pub async fn workbench_bibliography_preview(
    workspace_id: String,
    revision_id: String,
) -> WorkbenchResult<Value> {
    run_store(move |store| {
        super::project::preview_bibliography(&store, &workspace_id, &revision_id)
    })
    .await
}
#[tauri::command]
pub async fn workbench_citation_navigation(
    workspace_id: String,
    revision_id: String,
) -> WorkbenchResult<Value> {
    run_store(move |store| super::project::citation_navigation(&store, &workspace_id, &revision_id))
        .await
}
#[tauri::command]
pub async fn workbench_source_passage(
    workspace_id: String,
    version_id: String,
    start: usize,
    length: usize,
) -> WorkbenchResult<super::research::SourceReadResult> {
    run_store(move |store| {
        super::research::source_read(
            &store,
            &workspace_id,
            &version_id,
            start,
            length.min(64 * 1024),
        )
    })
    .await
}
#[tauri::command]
pub async fn workbench_zotero_preview(
    collection: Option<String>,
    start: usize,
    expected_server: Option<String>,
) -> WorkbenchResult<super::project::ZoteroPreview> {
    super::project::zotero_preview(collection, start, expected_server).await
}
#[tauri::command]
pub async fn workbench_zotero_import(
    workspace_id: String,
    preview: super::project::ZoteroPreview,
    selected: Vec<String>,
) -> WorkbenchResult<Value> {
    run_store(move |store| super::project::import_zotero(&store, &workspace_id, preview, &selected))
        .await
}

#[tauri::command]
pub async fn workbench_studio_export_file(path: String, content: String) -> WorkbenchResult<()> {
    run_store(move |_store| {
        let path = std::path::PathBuf::from(path);
        if !path.is_absolute()
            || !matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("md" | "tex" | "json")
            )
            || content.len() > 8 * 1024 * 1024
        {
            return Err(WorkbenchError::invalid(
                "Choose an absolute Markdown, TeX or JSON export path (up to 8 MiB)",
            ));
        }
        std::fs::write(&path, content)
            .map_err(|e| WorkbenchError::storage("Failed to export research document", e))?;
        std::fs::File::open(path)
            .and_then(|f| f.sync_all())
            .map_err(|e| WorkbenchError::storage("Failed to sync research export", e))
    })
    .await
}

// Research desk commands keep all app-owned storage work on the bounded gate.
#[tauri::command]
pub async fn workbench_desk_records(
    workspace_id: String,
    kind: String,
) -> WorkbenchResult<Vec<super::desk::DeskRecord>> {
    run_store(move |s| super::desk::records(&s, &workspace_id, &kind)).await
}
#[tauri::command]
pub async fn workbench_research_object(
    workspace_id: String,
    object: super::desk::ResearchObjectRef,
) -> WorkbenchResult<super::search::ResearchObject> {
    run_store(move |s| super::search::read_object(&s, &workspace_id, &object, 64 * 1024)).await
}
#[tauri::command]
pub async fn workbench_context_selection(
    session_id: String,
) -> WorkbenchResult<super::desk::ContextSelection> {
    run_store(move |s| super::desk::context(&s, &session_id)).await
}
#[tauri::command]
pub async fn workbench_save_context_selection(
    session_id: String,
    expected_revision: i64,
    items: Vec<super::desk::ContextItem>,
) -> WorkbenchResult<super::desk::ContextSelection> {
    ensure_session_idle(session_id.clone()).await?;
    run_store(move |s| super::desk::save_context(&s, &session_id, expected_revision, items)).await
}
#[tauri::command]
pub async fn workbench_reading_collection(
    request: super::desk::SaveCollection,
) -> WorkbenchResult<super::desk::DeskRecord> {
    run_store(move |s| super::desk::save_collection(&s, request)).await
}
#[tauri::command]
pub async fn workbench_research_search(
    request: super::search::SearchRequest,
) -> WorkbenchResult<super::search::SearchPage> {
    run_store(move |s| super::search::search(&s, request)).await
}
#[tauri::command]
pub async fn workbench_research_index(
    workspace_id: String,
    rebuild: bool,
) -> WorkbenchResult<super::search::IndexStatus> {
    run_store(move |s| {
        if rebuild {
            super::search::rebuild(&s, &workspace_id)?;
        }
        super::search::advance(&s, &workspace_id)
    })
    .await
}
#[tauri::command]
pub async fn workbench_save_relation(
    workspace_id: String,
    relation: super::project::relations::Relation,
    operation_id: String,
) -> WorkbenchResult<super::desk::DeskRecord> {
    run_store(move |s| {
        super::project::relations::save_relation(&s, &workspace_id, relation, &operation_id)
    })
    .await
}
#[tauri::command]
pub async fn workbench_save_decision(
    workspace_id: String,
    title: String,
    decision: super::project::relations::Decision,
    supersedes: Option<String>,
    operation_id: String,
) -> WorkbenchResult<super::desk::DeskRecord> {
    run_store(move |s| {
        super::project::relations::save_decision(
            &s,
            &workspace_id,
            &title,
            decision,
            supersedes.as_deref(),
            &operation_id,
        )
    })
    .await
}
#[tauri::command]
pub async fn workbench_change_impact(
    workspace_id: String,
) -> WorkbenchResult<super::project::relations::ImpactReport> {
    run_store(move |s| super::project::relations::impact(&s, &workspace_id)).await
}
#[tauri::command]
pub async fn workbench_session_handoff(
    session_id: String,
    operation_id: String,
) -> WorkbenchResult<super::desk::DeskRecord> {
    run_store(move |s| super::project::relations::handoff(&s, &session_id, &operation_id)).await
}
#[tauri::command]
pub async fn workbench_data_policy(
    workspace_id: String,
    policy: Option<super::data::DataPolicy>,
) -> WorkbenchResult<super::data::DataPolicy> {
    run_store(move |s| match policy {
        Some(p) => super::data::save_policy(&s, &workspace_id, p),
        None => super::data::policy(&s, &workspace_id),
    })
    .await
}
#[tauri::command]
pub async fn workbench_import_dataset(
    request: super::data::ImportDataset,
) -> WorkbenchResult<super::desk::DeskRecord> {
    run_store(move |s| super::data::import(&s, request)).await
}
#[tauri::command]
pub async fn workbench_save_sample(
    workspace_id: String,
    title: String,
    sample: super::data::SampleDefinition,
    supersedes: Option<String>,
    operation_id: String,
) -> WorkbenchResult<super::desk::DeskRecord> {
    run_store(move |s| {
        super::data::save_sample(
            &s,
            &workspace_id,
            &title,
            sample,
            supersedes.as_deref(),
            &operation_id,
        )
    })
    .await
}
#[tauri::command]
pub async fn workbench_acquisition_network(
    workspace_id: String,
    enabled: Option<bool>,
) -> WorkbenchResult<bool> {
    run_store(move |s| match enabled {
        Some(e) => super::acquisition::set_network(&s, &workspace_id, e),
        None => super::acquisition::network_enabled(&s, &workspace_id),
    })
    .await
}
async fn acquisition_allowed(ws: String) -> WorkbenchResult<()> {
    if !run_store(move |s| super::acquisition::network_enabled(&s, &ws)).await? {
        return Err(WorkbenchError::invalid(
            "Enable literature and data acquisition for this Workspace first",
        ));
    }
    Ok(())
}
#[tauri::command]
pub async fn workbench_crossref_lookup(
    workspace_id: String,
    query: String,
    operation_id: String,
) -> WorkbenchResult<super::desk::DeskRecord> {
    acquisition_allowed(workspace_id.clone()).await?;
    let result = super::acquisition::crossref(&query).await;
    run_store(move |s| {
        super::acquisition::record_lookup(&s, &workspace_id, &query, result, &operation_id)
    })
    .await
}
#[tauri::command]
pub async fn workbench_acquire_candidate(
    workspace_id: String,
    receipt_id: String,
    index: usize,
    citation_key: Option<String>,
    operation_id: String,
) -> WorkbenchResult<super::desk::DeskRecord> {
    run_store(move |s| {
        super::acquisition::import_candidate(
            &s,
            &workspace_id,
            &receipt_id,
            index,
            citation_key,
            &operation_id,
        )
    })
    .await
}
#[tauri::command]
pub async fn workbench_acquire_pdf(
    workspace_id: String,
    title: String,
    url: String,
    operation_id: String,
) -> WorkbenchResult<super::desk::DeskRecord> {
    acquisition_allowed(workspace_id.clone()).await?;
    let result = super::acquisition::download_pdf(&url).await;
    run_store(move |s| {
        super::acquisition::record_pdf(&s, &workspace_id, &title, &url, result, &operation_id)
    })
    .await
}
#[tauri::command]
pub async fn workbench_acquire_fred(
    request: super::acquisition::FredRequest,
) -> WorkbenchResult<super::desk::DeskRecord> {
    acquisition_allowed(request.workspace_id.clone()).await?;
    let result = super::acquisition::fred(&request).await;
    let ws = request.workspace_id;
    let series = request.series_id;
    let vintage = request.vintage;
    let operation = request.operation_id;
    run_store(move|s|match result{Ok((bytes,provenance))=>super::data::import_bytes(&s,&ws,&format!("{series} · {vintage}"),&bytes,"csv",provenance,None,&operation),Err(e)=>super::desk::insert(&s,&ws,"acquisition",&series,json!({"provider":"FRED/ALFRED","seriesId":series,"requestedVintage":vintage,"retrievedAt":super::desk::now(),"state":"failed","error":e.message}),None,&operation)}).await
}

pub(crate) fn task_pending_request(thread: &str, turn: &str) -> Option<String> {
    pending_server_requests()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .find_map(|event| {
            if let super::codex::NormalizedEvent::ServerRequest { method, params, .. } = event {
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
#[tauri::command]
pub async fn workbench_capture_execution_plan(
    request: super::research::execution_plan::CapturePlanRequest,
) -> WorkbenchResult<super::desk::DeskRecord> {
    run_store(move |s| super::research::execution_plan::capture(&s, request)).await
}
#[tauri::command]
pub async fn workbench_execution_plan_status(
    workspace_id: String,
    plan_id: String,
) -> WorkbenchResult<super::research::execution_plan::PlanStatus> {
    run_store(move |s| super::research::execution_plan::status(&s, &workspace_id, &plan_id)).await
}
#[tauri::command]
pub async fn workbench_authorize_execution_plan(
    workspace_id: String,
    plan_id: String,
    fingerprint: String,
) -> WorkbenchResult<super::research::execution_plan::PlanStatus> {
    run_store(move |s| {
        super::research::execution_plan::authorize(&s, &workspace_id, &plan_id, &fingerprint)
    })
    .await
}
#[tauri::command]
pub async fn workbench_import_dataset_metadata(
    request: super::data::MetadataDataset,
) -> WorkbenchResult<super::desk::DeskRecord> {
    run_store(move |s| super::data::import_metadata(&s, request)).await
}
#[tauri::command]
pub async fn workbench_dataset_rows(
    workspace_id: String,
    dataset_id: String,
    start: usize,
) -> WorkbenchResult<super::data::RowPreview> {
    run_store(move |s| super::data::preview_rows(&s, &workspace_id, &dataset_id, start, false))
        .await
}
#[tauri::command]
pub async fn workbench_reading_inbox_state(
    workspace_id: String,
    receipt_id: String,
    state: String,
    operation_id: String,
) -> WorkbenchResult<super::desk::DeskRecord> {
    run_store(move |s| {
        super::acquisition::inbox_state(&s, &workspace_id, &receipt_id, &state, &operation_id)
    })
    .await
}

pub(crate) async fn deliver_task(
    binding: super::tasks::TaskBinding,
    operation: String,
    value: Value,
) -> Result<Value, String> {
    let _permit = turn_queue()
        .clone()
        .try_acquire_owned()
        .map_err(|_| "task_busy: Workspace is running another turn".to_string())?;
    run_store(move |store| {
        super::tasks::validate_binding(&store, &binding)?;
        let mut result = super::tasks::deliver(&store, &binding.session_id, &operation, value)?;
        let next = super::tasks::binding(&store, &binding.session_id)?;
        result["harnessFingerprint"] = serde_json::json!(next.harness_fingerprint);
        result["cursor"] = serde_json::json!(next.cursor);
        Ok(result)
    })
    .await
    .map_err(|e| e.message)
}

#[tauri::command]
pub async fn workbench_file_reveal(
    request: super::project::FileReadRequest,
) -> WorkbenchResult<()> {
    run_store(move |store| {
        let file = super::project::read_workspace_file(&store, request)?;
        let path = file.external_path.ok_or_else(|| {
            WorkbenchError::invalid("Captured documents have no writable external file")
        })?;
        crate::file_viewer::reveal(std::path::Path::new(&path)).map_err(WorkbenchError::invalid)
    })
    .await
}
