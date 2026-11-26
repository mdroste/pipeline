//! Workspace-owned Claude Code conversations. The native binary authenticates;
//! app-owned transcripts supply bounded history to independent `claude -p` turns.
//!
//! The Workspace turn permit covers Claude turns too. Claude bindings use their
//! own namespace, and their IDs are never sent to Codex resume; switching
//! providers starts a new binding with the same bounded history, and truncation
//! is disclosed in the supplied context. Native tools other than sandboxed Bash
//! are disabled; Pipeline tools arrive only through the scoped MCP bridge. A
//! stale accepted turn becomes interrupted on restart and is never replayed.

mod activity;
mod answers;
pub(crate) mod approval;
mod budget;
pub(crate) mod idea_team;
mod protocol;
mod shell;
#[cfg(test)]
mod tests;
pub(crate) mod title;
mod tools;
pub(crate) mod transport;
pub(crate) mod usage;
mod wsl;

use super::chat_approval::Mode;
use super::research::PreparedHarness;
use super::runtime::{run_store, RuntimeCommandError, SendTurnRequest, SendTurnResult};
use super::store::{SessionBinding, Store, WorkbenchError, WorkbenchResult};
use super::turn_loop::{persist_and_publish, TurnEvents, UnsavedText};
use crate::agent_runtime::events::NormalizedEvent;
use serde_json::{json, Value};
use std::sync::{Mutex, OnceLock};
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
        Some(Value::String(value)) if matches!(value.as_str(), "claude" | "claude-code-api") => {
            Ok(true)
        }
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
    pub model: Option<String>,
    pub auth_mode: crate::agent_runtime::claude::AuthMode,
    pub approval_root: Option<String>,
    pub unsaved: UnsavedText,
}

impl Turn {
    pub async fn emit(&self, app: &impl TurnEvents, event: NormalizedEvent) -> Result<(), String> {
        persist_and_publish(&self.binding, app, event).await
    }

    pub async fn item(
        &self,
        app: &impl TurnEvents,
        suffix: &str,
        kind: &str,
        text: &str,
    ) -> Result<(), String> {
        let item_id = format!("{}-{suffix}", self.id);
        self.emit(app, NormalizedEvent::ItemCompleted {
            epoch: self.epoch, thread_id: self.thread.clone(), turn_id: self.id.clone(),
            item_id: item_id.clone(), item_kind: kind.into(),
            item: json!({"id":item_id,"type":kind,"text":text,"runtime":self.auth_mode.runtime(),"model":self.model}),
        }).await
    }

    /// Keeps reply text a stopped or failed turn had streamed: on screen, and
    /// in the history of the next message.
    pub async fn save_unfinished(&self, app: &impl TurnEvents) {
        for (item_id, text) in self.unsaved.take() {
            if text.trim().is_empty() {
                continue;
            }
            let item = json!({"id":item_id,"type":"agentMessage","text":text,"runtime":self.auth_mode.runtime(),"model":self.model,"phase":"incomplete"});
            let event = NormalizedEvent::ItemCompleted {
                epoch: self.epoch,
                thread_id: self.thread.clone(),
                turn_id: self.id.clone(),
                item_id,
                item_kind: "agentMessage".into(),
                item,
            };
            let _ = self.emit(app, event).await;
        }
    }
}

// This provider boundary carries the prepared harness and explicit turn authority.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn submit(
    app: tauri::AppHandle,
    request: SendTurnRequest,
    binding: Option<SessionBinding>,
    prepared: PreparedHarness,
    automated: bool,
    approval_mode: Mode,
    auth_mode: crate::agent_runtime::claude::AuthMode,
    control: super::submission::Control,
) -> Result<SendTurnResult, RuntimeCommandError> {
    let session = request.session_id.clone();
    let text = request.text.clone();
    let wsl = control
        .preflight(async {
            wsl::detect_async()
                .await
                .environment()
                .map_err(|message| WorkbenchError::worker(message, true).into())
        })
        .await?;
    let shell = {
        let wsl = wsl.clone();
        tokio::task::spawn_blocking(move || shell::available(wsl.as_ref()))
            .await
            .unwrap_or(false)
    };
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
    let approval_root = root
        .as_ref()
        .filter(|_| approval_mode != Mode::Sandbox)
        .map(|path| path.to_string_lossy().into_owned());
    let args = protocol::arguments(
        &prepared.effective,
        request.model.as_deref(),
        request.effort.as_deref(),
        root.as_deref(),
        approval_mode,
    );
    // Bash starts in a root it can write, so relative paths outlive this
    // turn's temporary process folder.
    let working = root
        .as_deref()
        .filter(|_| prepared.effective.capabilities.shell_write);
    let sandbox = root.as_deref().map(|root| {
        let mut sandbox = shell::sandbox(
            root,
            &prepared.effective.capabilities,
            prepared.effective.command_network,
        );
        if approval_root.is_some() {
            sandbox["autoAllowBashIfSandboxed"] = json!(false);
        }
        sandbox
    });
    // CLI auth, configuration and handshake all finish before sending a prompt.
    let mut process = control
        .preflight(async {
            transport::Process::start_in(auth_mode, &args, sandbox.as_ref(), working, wsl)
                .await
                .map_err(error)
        })
        .await?;
    let initialization = control
        .preflight(async { process.initialize(&tool_specs).await.map_err(error) })
        .await?;
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
    let (bound, lease) = run_store(move |store| {
        let lease = super::submission::session_lease(&store, &session)?.ok_or_else(|| {
            WorkbenchError::conflict("Another Pipeline instance owns this conversation's turn")
        })?;
        let bound = store.bind_session(&session, NAMESPACE, &thread_copy)?;
        store.set_binding_harness(
            &bound.id,
            &fingerprint,
            &["Claude Code default and Pipeline conversation profile".into()],
        )?;
        store.record_turn_submission(&bound.id, &submission, &turn_id)?;
        store.attach_turn_snapshots(&bound.id, &submission, &config, &context)?;
        Ok((bound, lease))
    })
    .await?;
    let turn = Turn {
        epoch,
        thread: thread.clone(),
        id: id.clone(),
        binding: bound.id,
        model: request.model,
        auth_mode,
        approval_root,
        unsaved: UnsavedText::default(),
    };
    // The draft is cleared on acknowledgement, and Stop or a store failure can
    // end the worker before it writes anything. Save the transcript copy first.
    turn.item(&app, "user", "userMessage", &request.text)
        .await
        .map_err(error)?;
    super::runtime::accept_local_turn(epoch, &thread, &id);
    let (cancel, receiver) = watch::channel(false);
    *running().lock().unwrap_or_else(|e| e.into_inner()) = Some(Running {
        thread: thread.clone(),
        turn: id.clone(),
        cancel,
    });
    let task_turn = turn.clone();
    let original = prompt.user_text.clone();
    tokio::spawn(async move {
        let _lease = lease;
        let result = control
            .run(transport::drive(
                &mut process,
                &app,
                &task_turn,
                &prompt,
                tool_specs,
                transport::turn_clock(),
                receiver,
            ))
            .await;
        process.stop().await;
        if result.is_err() {
            task_turn.save_unfinished(&app).await;
        }
        let completion = super::runtime::finalization::LocalCompletion {
            epoch,
            thread: &thread,
            turn: &id,
            binding: &task_turn.binding,
        };
        completion.capture(&app).await;
        let (status, detail) = super::runtime::finalization::outcome(result);
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
        completion
            .publish(&app, event, || {
                *running().lock().unwrap_or_else(|e| e.into_inner()) = None;
            })
            .await;
        if status == "completed" {
            super::runtime::spawn_title_generation(
                app,
                super::runtime::TitleRuntime::Claude { epoch, auth_mode },
                thread,
                id,
            );
        }
    });
    let draft_cleared = if automated {
        false
    } else {
        let session = request.session_id;
        let submission = request.client_submission_id;
        run_store(move |store| {
            super::runtime::clear_sent_draft(&store, &session, &original, &submission)
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
    let lease = super::submission::session_lease(&store, &session)?;
    let owner = running().lock().unwrap_or_else(|e| e.into_inner());
    let snapshot = store.conversation_snapshot(&session)?;
    let Some(binding) = snapshot
        .active_binding
        .filter(|b| b.runtime_namespace == NAMESPACE)
    else {
        return Ok(false);
    };
    let Some(_lease) = lease else {
        return Ok(true);
    };
    // Submission records the turn before installing its process owner. Check
    // setup after the snapshot so a newly recorded turn cannot look abandoned.
    if super::runtime::turn_setup_in_progress() {
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
