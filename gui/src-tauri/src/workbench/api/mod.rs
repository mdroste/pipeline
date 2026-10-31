//! Conversation-owned API lifecycle, persistence, cancellation and recovery.
//! Device credentials are resolved once per turn. Review run settings and its
//! supervised calls never enter this runtime. Submissions are saved before HTTP;
//! interrupted or ambiguous requests are never automatically replayed.
pub(crate) mod config;
mod drive;
#[cfg(test)]
use crate::agent_runtime::api::streaming;
#[cfg(test)]
mod streaming_tests;
#[cfg(test)]
mod tests;
use crate::agent_runtime::api::wire;
pub(crate) use crate::connections::{ApiConnection as Connection, ApiProvider as Provider};
pub(crate) use config::selected;
pub(crate) use drive::title;
use std::sync::atomic::{AtomicU64, Ordering};
static EPOCH: AtomicU64 = AtomicU64::new(1 << 42);
pub(crate) fn next_epoch() -> u64 {
    EPOCH.fetch_add(1, Ordering::AcqRel)
}
fn identity() -> WorkbenchResult<String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|e| WorkbenchError::invalid(e.to_string()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
use super::commands::{run_store, RuntimeCommandError, SendTurnRequest, SendTurnResult};
use super::research::PreparedHarness;
use super::store::{SessionBinding, Store, WorkbenchError, WorkbenchResult};
use crate::agent_runtime::events::NormalizedEvent;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::Emitter;
use tokio::sync::watch;

pub(crate) const NAMESPACE: &str = "api-local-v1";
struct Running {
    thread: String,
    turn: String,
    cancel: watch::Sender<bool>,
}
static RUNNING: OnceLock<Mutex<Option<Running>>> = OnceLock::new();
fn running() -> &'static Mutex<Option<Running>> {
    RUNNING.get_or_init(|| Mutex::new(None))
}

pub(crate) fn error(message: impl Into<String>) -> RuntimeCommandError {
    WorkbenchError::invalid(message).into()
}

#[derive(Clone)]
pub(super) struct Turn {
    pub epoch: u64,
    pub thread: String,
    pub id: String,
    pub binding: String,
    pub model: Option<String>,
    pub runtime: String,
    pub usage: Arc<Mutex<Vec<Value>>>,
}

impl Turn {
    pub async fn emit(&self, app: &tauri::AppHandle, event: NormalizedEvent) -> Result<(), String> {
        let binding = self.binding.clone();
        let persisted = event.clone();
        run_store(move |store| store.project_codex_event(&binding, &persisted))
            .await
            .map_err(|e| e.message)?;
        app.emit("workbench:event", event)
            .map_err(|e| e.to_string())
    }

    pub async fn item(
        &self,
        app: &tauri::AppHandle,
        suffix: &str,
        kind: &str,
        text: &str,
    ) -> Result<(), String> {
        self.emit(app, self.item_event(suffix, kind, text, None))
            .await
    }

    fn item_event(
        &self,
        suffix: &str,
        kind: &str,
        text: &str,
        phase: Option<&str>,
    ) -> NormalizedEvent {
        let item_id = format!("{}-{suffix}", self.id);
        NormalizedEvent::ItemCompleted {
            epoch: self.epoch,
            thread_id: self.thread.clone(),
            turn_id: self.id.clone(),
            item_id: item_id.clone(),
            item_kind: kind.into(),
            item: json!({"id":item_id,"type":kind,"text":text,"runtime":self.runtime,"model":self.model,"phase":phase}),
        }
    }

    fn reply_event(&self, round: usize, reply: &wire::Reply) -> NormalizedEvent {
        self.item_event(
            &format!("assistant-{round}"),
            "agentMessage",
            &reply.text,
            Some(if reply.calls.is_empty() {
                "final_answer"
            } else {
                "commentary"
            }),
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn submit(
    app: tauri::AppHandle,
    request: SendTurnRequest,
    binding: Option<SessionBinding>,
    prepared: PreparedHarness,
    automated: bool,
    provider: Provider,
    control: super::submission::Control,
) -> Result<SendTurnResult, RuntimeCommandError> {
    if request.text.trim().is_empty()
        || request.text.len() > 1_000_000
        || request.text.contains('\0')
    {
        return Err(error("Messages must contain text and fit within 1 MB"));
    }
    let session = request.session_id.clone();
    let text = request.text.clone();
    let prompt = run_store(move |store| {
        Ok(format!(
            "{}\n\nCurrent user message:\n{text}",
            super::history::recent_context(&store, &session)?
        ))
    })
    .await?;
    let connection = control
        .preflight(async {
            config::load(
                provider,
                request.model.as_deref(),
                request.effort.as_deref(),
            )
            .await
            .map_err(error)
        })
        .await?;
    let client = wire::client_for(&connection).map_err(error)?;
    let tool_specs = prepared.effective.dynamic_tools.clone();
    let instructions = format!("{}\n{}\nUse only the supplied Pipeline tools. Run commands with the sandboxed shell tool when supplied, or with authorized research execution profiles. Never claim to have run unavailable tools. Web access uses web_search, web_fetch and the browser when enabled.",prepared.effective.preset.base_instructions.as_deref().unwrap_or("You are a research assistant."),prepared.effective.developer_instructions);
    let epoch = next_epoch();
    let session = request.session_id.clone();
    let fingerprint = prepared.effective.fingerprint.clone();
    let thread = binding
        .filter(|b| {
            b.runtime_namespace == NAMESPACE
                && b.harness_fingerprint.as_deref() == Some(&fingerprint)
        })
        .map(|b| b.provider_thread_id)
        .unwrap_or(format!("api-{}", identity()?));
    let thread_copy = thread.clone();
    let submission = request.client_submission_id.clone();
    let id = format!("api-turn-{}", identity()?);
    let turn_id = id.clone();
    let config = prepared.config_snapshot_id;
    let context = prepared.context_snapshot_id;
    let bound = run_store(move |store| {
        let bound = store.bind_session(&session, NAMESPACE, &thread_copy)?;
        store.set_binding_harness(
            &bound.id,
            &fingerprint,
            &["API connection and Pipeline conversation profile".into()],
        )?;
        store.record_turn_submission(&bound.id, &submission, &turn_id)?;
        store.attach_turn_snapshots(&bound.id, &submission, &config, &context)?;
        Ok(bound)
    })
    .await?;
    let turn = Turn {
        epoch,
        thread: thread.clone(),
        id: id.clone(),
        binding: bound.id,
        model: Some(connection.model.clone()),
        runtime: provider.runtime().into(),
        usage: Arc::new(Mutex::new(Vec::new())),
    };
    // The draft may be cleared immediately after acknowledgement. Persist its
    // transcript copy before handing the request to the asynchronous worker.
    turn.item(&app, "user", "userMessage", &request.text)
        .await
        .map_err(error)?;
    super::commands::accept_local_turn(epoch, &thread, &id);
    let (cancel, receiver) = watch::channel(false);
    *running().lock().unwrap_or_else(|e| e.into_inner()) = Some(Running {
        thread: thread.clone(),
        turn: id.clone(),
        cancel,
    });
    let task_turn = turn.clone();
    let original = request.text.clone();
    tokio::spawn(async move {
        let result = control
            .run(drive::run(
                &client,
                &connection,
                &app,
                &task_turn,
                &instructions,
                &prompt,
                &tool_specs,
                receiver,
            ))
            .await;
        let capture = super::conversation_tools::finish(&task_turn.thread, &task_turn.id).await;
        let result = result.and(capture.map_err(|error| error.message));
        let cleanup_thread = thread.clone();
        let cleanup_turn = id.clone();
        let _ = run_store(move |store| {
            super::research::cancel_executions_for_provider_turn(
                &store,
                &cleanup_thread,
                &cleanup_turn,
            )
        })
        .await;
        let (status, detail) = match result {
            Ok(()) => ("completed", None),
            Err(message) if message == "interrupted" => ("interrupted", None),
            Err(message) => ("failed", Some(message)),
        };
        if let Some(message) = detail.as_deref() {
            let _ = task_turn
                .item(
                    &app,
                    "error",
                    "agentMessage",
                    &format!("The API could not complete this turn: {message}"),
                )
                .await;
        }
        let event = NormalizedEvent::TurnCompleted {
            epoch,
            thread_id: thread.clone(),
            turn_id: id.clone(),
            status: status.into(),
            turn: json!({"id":id,"status":status,"error":detail,"usage":{"apiCalls":task_turn.usage.lock().unwrap_or_else(|e|e.into_inner()).clone()}}),
        };
        if task_turn.emit(&app, event.clone()).await.is_err() {
            let _ = app.emit("workbench:event", event);
        }
        *running().lock().unwrap_or_else(|e| e.into_inner()) = None;
        super::commands::finish_local_turn(epoch, &thread, &id);
        if status == "completed" {
            super::commands::spawn_title_generation(
                app,
                super::commands::TitleRuntime::Api { epoch, connection },
                thread,
                id,
            );
        }
    });
    let draft_cleared = if automated {
        false
    } else {
        let session = request.session_id;
        run_store(move |store| {
            let snapshot = store.session_snapshot(&session)?;
            if snapshot.session.draft.trim() != original.trim() {
                return Ok(false);
            }
            store.update_session(super::store::UpdateSessionRequest {
                session_id: session,
                expected_revision: snapshot.session.revision,
                operation_id: format!("send-ack-{}", request.client_submission_id),
                title: None,
                draft: Some(String::new()),
                overrides: None,
                archived: None,
                preset_id: None,
                paper_id: None,
                clear_paper: None,
            })?;
            Ok(true)
        })
        .await
        .unwrap_or(false)
    };
    Ok(SendTurnResult {
        epoch,
        thread_id: turn.thread,
        turn_id: turn.id,
        draft_cleared,
    })
}

pub(crate) fn interrupt(thread: &str, turn: &str) -> Result<(), super::codex::RequestError> {
    let state = running().lock().unwrap_or_else(|e| e.into_inner());
    let active = state
        .as_ref()
        .filter(|r| r.thread == thread && r.turn == turn)
        .ok_or_else(|| super::codex::RequestError::invalid("This API turn is no longer active"))?;
    let _ = active.cancel.send(true);
    Ok(())
}

pub(crate) async fn shutdown() {
    if let Some(active) = running().lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        let _ = active.cancel.send(true);
    }
    // Give the worker time to cancel tools and persist interruption.
    for _ in 0..100 {
        if running()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_none()
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

pub(crate) fn reconcile(store: Store, session: String) -> WorkbenchResult<bool> {
    // Keep the owner stable while reading persisted state. A finishing worker
    // must not clear its ownership between this snapshot and the liveness check.
    let owner = running().lock().unwrap_or_else(|e| e.into_inner());
    let snapshot = store.conversation_snapshot(&session)?;
    let Some(binding) = snapshot
        .active_binding
        .filter(|b| b.runtime_namespace == NAMESPACE)
    else {
        return Ok(false);
    };
    // Submission records the turn before installing its process owner. Check
    // setup after the snapshot so a newly recorded turn cannot look abandoned.
    if super::commands::turn_setup_in_progress() {
        return Ok(true);
    }
    for turn in snapshot
        .turns
        .iter()
        .filter(|t| t.binding_id == binding.id && t.terminal_at.is_none())
    {
        let Some(id) = &turn.provider_turn_id else {
            continue;
        };
        if owner
            .as_ref()
            .is_some_and(|r| r.thread == binding.provider_thread_id && &r.turn == id)
        {
            continue;
        }
        store.project_codex_event(&binding.id, &NormalizedEvent::TurnCompleted {
            epoch: next_epoch(), thread_id: binding.provider_thread_id.clone(), turn_id: id.clone(), status: "interrupted".into(),
            turn: json!({"id":id,"status":"interrupted","error":{"message":"The API connection ended; this turn was not replayed."}}),
        })?;
    }
    Ok(true)
}
