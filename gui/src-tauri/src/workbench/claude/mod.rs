//! Workspace-owned Claude Code conversations. The native binary authenticates;
//! app-owned transcripts supply bounded history to independent `claude -p` turns.

mod protocol;
mod shell;
#[cfg(test)]
mod tests;
pub(crate) mod title;
mod tools;
pub(crate) mod transport;
pub(crate) mod usage;

use super::codex::NormalizedEvent;
use super::commands::{run_store, RuntimeCommandError, SendTurnRequest, SendTurnResult};
use super::research::PreparedHarness;
use super::store::{SessionBinding, Store, WorkbenchError, WorkbenchResult};
use serde_json::{json, Value};
use std::sync::{Mutex, OnceLock};
use tauri::Emitter;
use tokio::sync::watch;

pub(crate) const NAMESPACE: &str = "claude-local-v1";
struct Running {
    thread: String,
    turn: String,
    cancel: watch::Sender<bool>,
}
static RUNNING: OnceLock<Mutex<Option<Running>>> = OnceLock::new();
fn running() -> &'static Mutex<Option<Running>> {
    RUNNING.get_or_init(|| Mutex::new(None))
}

pub(crate) fn selected(overrides: &Value) -> WorkbenchResult<bool> {
    match overrides.get("runtime") {
        None => Ok(false),
        Some(Value::String(value)) if value == "codex" => Ok(false),
        Some(Value::String(value)) if value == "claude" => Ok(true),
        _ => Err(WorkbenchError::invalid(
            "Choose ChatGPT or Claude Code for this conversation",
        )),
    }
}

pub(crate) fn error(message: impl Into<String>) -> RuntimeCommandError {
    WorkbenchError::invalid(message).into()
}

/// An event epoch for Claude work that no turn started, such as a title request.
pub(crate) fn fresh_epoch() -> u64 {
    protocol::next_epoch()
}

#[derive(Clone)]
pub(super) struct Turn {
    pub epoch: u64,
    pub thread: String,
    pub id: String,
    pub binding: String,
    pub session: String,
    pub model: Option<String>,
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
        let item_id = format!("{}-{suffix}", self.id);
        self.emit(app, NormalizedEvent::ItemCompleted {
            epoch: self.epoch, thread_id: self.thread.clone(), turn_id: self.id.clone(),
            item_id: item_id.clone(), item_kind: kind.into(),
            item: json!({"id":item_id,"type":kind,"text":text,"runtime":"claude","model":self.model}),
        }).await
    }
}

pub(crate) async fn submit(
    app: tauri::AppHandle,
    request: SendTurnRequest,
    binding: Option<SessionBinding>,
    prepared: PreparedHarness,
    automated: bool,
) -> Result<SendTurnResult, RuntimeCommandError> {
    let session = request.session_id.clone();
    let text = request.text.clone();
    let shell = shell::available();
    let (prompt, root) = run_store(move |store| {
        let root = if shell {
            Some(store.runtime_root(&session)?)
        } else {
            None
        };
        Ok((protocol::prompt(&store, &session, &text)?, root))
    })
    .await?;
    let tool_specs = tools::catalog(&prepared.effective);
    let args = protocol::arguments(
        &prepared.effective,
        request.model.as_deref(),
        request.effort.as_deref(),
        root.as_deref(),
    );
    let sandbox = root.as_deref().map(shell::sandbox);
    // CLI auth, configuration and handshake all finish before sending a prompt.
    let mut process = transport::Process::start(&args, sandbox.as_ref())
        .await
        .map_err(error)?;
    let initialization = process.initialize(&tool_specs).await.map_err(error)?;
    protocol::validate_model(
        &initialization,
        request.model.as_deref(),
        request.effort.as_deref(),
    )
    .map_err(error)?;
    let epoch = protocol::next_epoch();
    let session = request.session_id.clone();
    let fingerprint = prepared.effective.fingerprint.clone();
    let thread = binding
        .filter(|b| {
            b.runtime_namespace == NAMESPACE
                && b.harness_fingerprint.as_deref() == Some(&fingerprint)
        })
        .map(|b| b.provider_thread_id)
        .unwrap_or(format!("claude-{}", protocol::identity()?));
    let thread_copy = thread.clone();
    let submission = request.client_submission_id.clone();
    let id = format!("claude-turn-{}", protocol::identity()?);
    let turn_id = id.clone();
    let config = prepared.config_snapshot_id;
    let context = prepared.context_snapshot_id;
    let bound = run_store(move |store| {
        let bound = store.bind_session(&session, NAMESPACE, &thread_copy)?;
        store.set_binding_harness(
            &bound.id,
            &fingerprint,
            &["Claude Code default and Pipeline conversation profile".into()],
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
        session: request.session_id.clone(),
        model: request.model,
    };
    super::commands::accept_claude_turn(epoch, &thread, &id);
    let (cancel, receiver) = watch::channel(false);
    *running().lock().unwrap_or_else(|e| e.into_inner()) = Some(Running {
        thread: thread.clone(),
        turn: id.clone(),
        cancel,
    });
    let task_turn = turn.clone();
    let original = prompt.user_text.clone();
    tokio::spawn(async move {
        let result = transport::drive(
            &mut process,
            &app,
            &task_turn,
            &request.text,
            &prompt,
            tool_specs,
            receiver,
        )
        .await;
        process.stop().await;
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
                    &format!("Claude could not complete this turn: {message}"),
                )
                .await;
        }
        let event = NormalizedEvent::TurnCompleted {
            epoch,
            thread_id: thread.clone(),
            turn_id: id.clone(),
            status: status.into(),
            turn: json!({"id":id,"status":status,"error":detail}),
        };
        if task_turn.emit(&app, event.clone()).await.is_err() {
            let _ = app.emit("workbench:event", event);
        }
        *running().lock().unwrap_or_else(|e| e.into_inner()) = None;
        super::commands::finish_claude_turn(epoch, &thread, &id);
        if status == "completed" {
            super::commands::spawn_title_generation(
                app,
                super::commands::TitleRuntime::Claude { epoch },
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
        .ok_or_else(|| {
            super::codex::RequestError::invalid("This Claude turn is no longer active")
        })?;
    let _ = active.cancel.send(true);
    Ok(())
}

pub(crate) async fn shutdown() {
    if let Some(active) = running().lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        let _ = active.cancel.send(true);
    }
    // Give the worker time to kill its process group and persist interruption.
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
            epoch: protocol::next_epoch(), thread_id: binding.provider_thread_id.clone(), turn_id: id.clone(), status: "interrupted".into(),
            turn: json!({"id":id,"status":"interrupted","error":{"message":"Claude process ended; this turn was not replayed."}}),
        })?;
    }
    Ok(true)
}
