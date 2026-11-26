//! Codex Tauri commands.

pub(crate) mod account;
pub(crate) mod bridge_events;
pub(crate) mod requests;

// Include the generated Tauri registration macros with the public handlers.

pub(crate) use requests::refuse_unsupported_request;
pub(crate) use requests::validate_pending_request;
#[cfg(test)]
pub(crate) use requests::{claim_pending_request, validate_server_response};
use requests::{clear_pending, record_pending, remove_pending};

use super::{
    active_turn_state, announce_active_turn, close_active_connection, complete_active_turn,
    has_saved_chat_approval, run_store, spawn_title_generation, stop_connection_tools, submit_turn,
    turn_has_live_owner, turn_setup_in_progress, with_session, RuntimeCommandError,
    SendTurnRequest, SendTurnResult, TitleRuntime, TurnOrigin,
};
use crate::workbench::chat_approval::Mode;
use crate::workbench::store::{ReconcileResult, WorkbenchError, WorkbenchResult};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::Emitter;

static EVENT_BRIDGE_EPOCH: AtomicU64 = AtomicU64::new(0);
pub(crate) use bridge_events::BridgeEvents;

pub(crate) async fn reconcile_workspace_roots(
    operation_id: String,
) -> WorkbenchResult<ReconcileResult> {
    run_store(move |store| store.reconcile_workspace_roots(&operation_id)).await
}

pub(crate) async fn codex_send_turn(
    app: tauri::AppHandle,
    request: SendTurnRequest,
) -> Result<SendTurnResult, RuntimeCommandError> {
    submit_turn(Some(app), request, None, Default::default()).await
}

pub(crate) async fn codex_interrupt_turn(
    thread_id: String,
    turn_id: String,
) -> Result<(), crate::workbench::codex::RequestError> {
    {
        let state = active_turn_state()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if state.thread_id.as_deref() == Some(&thread_id)
            && state.turn_id.as_deref() == Some(&turn_id)
            && state
                .owner
                .as_ref()
                .is_some_and(|owner| owner.origin == TurnOrigin::Automation)
        {
            return Err(crate::workbench::codex::RequestError::invalid(
                "Stop this turn from its automation.",
            ));
        }
    }
    interrupt_runtime_turn(thread_id, turn_id).await
}

pub(crate) async fn interrupt_runtime_turn(
    thread_id: String,
    turn_id: String,
) -> Result<(), crate::workbench::codex::RequestError> {
    if thread_id.starts_with("api-") {
        return crate::workbench::api::interrupt(&thread_id, &turn_id);
    }
    if thread_id.starts_with("claude-") {
        let result = crate::workbench::claude::interrupt(&thread_id, &turn_id);
        let _ = run_store(move |store| {
            crate::workbench::research::cancel_executions_for_provider_turn(
                &store, &thread_id, &turn_id,
            )
        })
        .await;
        return result;
    }
    let result = crate::workbench::codex::supervisor_manager()
        .connect()
        .await?
        .interrupt_turn(&thread_id, &turn_id)
        .await;
    let cancellation_thread = thread_id.clone();
    let cancellation_turn = turn_id.clone();
    let _ = run_store(move |store| {
        crate::workbench::research::cancel_executions_for_provider_turn(
            &store,
            &cancellation_thread,
            &cancellation_turn,
        )
    })
    .await;
    result
}

/// API and Claude turns are owned in process and need no connection to settle.
/// False leaves the conversation to Codex.
pub(crate) async fn reconcile_local_session(session_id: String) -> WorkbenchResult<bool> {
    run_store(move |store| {
        let snapshot = store.session_snapshot(&session_id)?;
        let api = crate::workbench::api::selected(&snapshot.session.overrides)?.is_some();
        let claude = !api && crate::workbench::claude::selected(&snapshot.session.overrides)?;
        let binding = store.active_binding(&session_id)?;
        match binding.as_ref().map(|b| b.runtime_namespace.as_str()) {
            Some(crate::workbench::api::NAMESPACE) => {
                crate::workbench::api::reconcile(store, session_id)
            }
            Some(crate::workbench::claude::NAMESPACE) => {
                crate::workbench::claude::reconcile(store, session_id)
            }
            _ => Ok(api || claude),
        }
    })
    .await
}

/// True when the conversation's state may have changed, so it is worth
/// reloading. Closing an abandoned turn needs no matching harness or selected
/// connection: how a turn ended doesn't depend on either.
pub(crate) async fn codex_reconcile_session(
    app: tauri::AppHandle,
    session_id: String,
) -> Result<bool, RuntimeCommandError> {
    let reconciled = if reconcile_local_session(session_id.clone()).await? {
        Ok(true)
    } else {
        reconcile_codex_thread(&app, &session_id).await
    };
    if settle_abandoned_turns(&app, session_id).await? {
        return Ok(true);
    }
    reconciled
}

/// Closes open turns on the conversation's Codex binding that nothing owns, as
/// a quit or crash leaves them. Each ends as Codex recorded it where the thread
/// can still be read.
async fn settle_abandoned_turns(
    app: &tauri::AppHandle,
    session_id: String,
) -> Result<bool, RuntimeCommandError> {
    let abandoned = run_store(move |store| {
        let Some(binding) = store.active_binding(&session_id)?.filter(|binding| {
            !matches!(
                binding.runtime_namespace.as_str(),
                crate::workbench::api::NAMESPACE | crate::workbench::claude::NAMESPACE
            )
        }) else {
            return Ok(None);
        };
        let open = store.open_provider_turns(&binding.id)?;
        let thread = binding.provider_thread_id.as_str();
        Ok(open
            .iter()
            .any(|turn| turn_is_abandoned(thread, turn))
            .then_some(binding))
    })
    .await?;
    let Some(binding) = abandoned else {
        return Ok(false);
    };
    let native = crate::workbench::codex::supervisor_manager()
        .connect()
        .await
        .ok();
    if let Some(supervisor) = &native {
        ensure_event_bridge(app.clone(), supervisor.clone());
    }
    close_abandoned_turns(native.as_deref(), binding).await?;
    Ok(true)
}

/// Read after the turn list: a turn recorded during setup has its owner by now.
fn turn_is_abandoned(thread: &str, turn: &str) -> bool {
    !turn_setup_in_progress() && !turn_has_live_owner(thread, turn)
}

/// Projects Codex's own ending for each open turn it still knows, then marks
/// the rest interrupted. Without a live owner, none of them is still running.
pub(crate) async fn close_abandoned_turns(
    native: Option<&crate::workbench::codex::AppServerSupervisor>,
    binding: crate::workbench::store::SessionBinding,
) -> WorkbenchResult<()> {
    let thread = binding.provider_thread_id;
    if let Some(supervisor) = native {
        // An error means the thread is unavailable, which the next step covers.
        let _ = supervisor.reconcile_thread(&binding.id, &thread).await;
    }
    let binding = binding.id;
    run_store(move |store| {
        for turn in store.open_provider_turns(&binding)? {
            if !turn_is_abandoned(&thread, &turn) {
                continue;
            }
            let event = crate::agent_runtime::events::NormalizedEvent::TurnCompleted {
                epoch: crate::workbench::api::next_epoch(),
                thread_id: thread.clone(),
                turn: json!({
                    "id": turn,
                    "status": "interrupted",
                    "error": {"message": "The connection ended; this turn was not replayed."}
                }),
                turn_id: turn,
                status: "interrupted".into(),
            };
            store.project_codex_event(&binding, &event)?;
        }
        Ok(())
    })
    .await
}

async fn reconcile_codex_thread(
    app: &tauri::AppHandle,
    session_id: &str,
) -> Result<bool, RuntimeCommandError> {
    let lookup_session_id = session_id.to_string();
    let (binding, root, harness) = run_store(move |store| {
        Ok::<_, WorkbenchError>((
            store.active_binding(&lookup_session_id)?,
            store.runtime_root(&lookup_session_id)?,
            crate::workbench::research::resolve_harness(&store, &lookup_session_id)?,
        ))
    })
    .await
    .map_err(RuntimeCommandError::from)?;
    let Some(binding) = binding else {
        return Ok(false);
    };
    let Some(approval_mode) = [Mode::Ask, Mode::Auto, Mode::Sandbox]
        .into_iter()
        .find(|mode| {
            binding.harness_fingerprint.as_deref()
                == Some(&mode.codex_fingerprint(&harness.fingerprint))
        })
    else {
        return Ok(false);
    };
    let permission_profile = harness.permission_profile.as_str();
    let supervisor = crate::workbench::codex::supervisor_manager()
        .connect()
        .await?;
    ensure_event_bridge(app.clone(), supervisor.clone());
    let root = crate::file_viewer::system_path(&root)
        .to_string_lossy()
        .into_owned();
    let Some(connection) = supervisor
        .resume_thread(
            session_id,
            &binding.provider_thread_id,
            (permission_profile, approval_mode),
            &root,
            vec![root.clone()],
            Some(harness.capabilities.codex_config(
                harness.web_search,
                harness.command_network,
                permission_profile,
            )),
        )
        .await?
    else {
        // The next message continues on a successor thread.
        return Ok(false);
    };
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
    reconcile_active_turn(app, &supervisor).await;
    Ok(true)
}

pub(crate) fn ensure_event_bridge(
    app: tauri::AppHandle,
    supervisor: Arc<crate::workbench::codex::AppServerSupervisor>,
) {
    let epoch = supervisor.status().epoch;
    if EVENT_BRIDGE_EPOCH.swap(epoch, Ordering::AcqRel) == epoch {
        return;
    }
    clear_pending();
    let mut source = BridgeEvents::new(supervisor.subscribe(), supervisor.request_sink());
    let recovery = supervisor.clone();
    let recovery_app = app.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            if EVENT_BRIDGE_EPOCH.load(Ordering::Acquire) != epoch {
                break;
            }
            if !recovery.status().connected {
                recovery.shutdown().await;
                stop_connection_tools(epoch).await;
                close_active_connection(epoch);
                announce_active_turn(&recovery_app, None);
                break;
            }
            reconcile_active_turn(&recovery_app, &recovery).await;
        }
    });
    tokio::spawn(async move {
        while let Some(received) = source.next().await {
            match received {
                Ok(event) => {
                    if EVENT_BRIDGE_EPOCH.load(Ordering::Acquire) != epoch {
                        return;
                    }
                    // Host-owned side calls (conversation titles) run in their
                    // own ephemeral threads; their events must not reach the
                    // pending-request registry, turn state, or the frontend.
                    if crate::workbench::codex::event_thread_id(&event)
                        .is_some_and(|thread| supervisor.is_side_thread(thread))
                    {
                        continue;
                    }
                    if let crate::agent_runtime::events::NormalizedEvent::ServerRequest {
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
                                let response = if crate::workbench::conversation_tools::owns(
                                    params["tool"].as_str().unwrap_or_default(),
                                ) {
                                    // Boxed because this future is too large for a thread's stack unoptimized.
                                    Box::pin(crate::workbench::conversation_tools::handle(params))
                                        .await
                                } else {
                                    run_store(move |store| {
                                        crate::workbench::research::handle_dynamic_tool_call(
                                            &store, &params,
                                        )
                                    })
                                    .await
                                };
                                crate::workbench::research::jobs::launch_pending();
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
                                        while crate::workbench::research::jobs::execution_active(
                                            &execution_id,
                                        ) {
                                            tokio::time::sleep(std::time::Duration::from_millis(
                                                75,
                                            ))
                                            .await;
                                        }
                                        if let Ok(finished) = run_store(move |store| {
                                            crate::workbench::research::jobs::complete_tool_result(
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
                                let succeeded = response
                                    .get("success")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false);
                                let delivered = tool_supervisor
                                    .respond_to_tool_call(request_id, response)
                                    .await;
                                let success = succeeded && matches!(delivered, Ok(true));
                                let _ = tool_app.emit(
                                    "workbench:event",
                                    json!({"kind":"researchToolCompleted","epoch":epoch,"success":success}),
                                );
                            });
                            continue;
                        }
                    }
                    if crate::workbench::discovery::resolve_autonomous_request(&supervisor, &event)
                        .await
                    {
                        continue;
                    }
                    if refuse_unsupported_request(&supervisor, &event).await {
                        continue;
                    }
                    if let crate::agent_runtime::events::NormalizedEvent::ServerRequest {
                        request_id,
                        ..
                    } = &event
                    {
                        if has_saved_chat_approval(&event).await {
                            let _ = supervisor
                                .respond_to_server_request(
                                    request_id.clone(),
                                    Ok(json!({"decision":"accept"})),
                                )
                                .await;
                            continue;
                        }
                    }
                    crate::workbench::tasks::activity::observe(&event);
                    match &event {
                        crate::agent_runtime::events::NormalizedEvent::ServerRequest {
                            request_id,
                            ..
                        } => {
                            record_pending(request_id, &event);
                        }
                        crate::agent_runtime::events::NormalizedEvent::ServerRequestResolved {
                            request_id,
                            ..
                        } => {
                            remove_pending(request_id);
                        }
                        crate::agent_runtime::events::NormalizedEvent::ConnectionClosed {
                            epoch,
                            ..
                        } => {
                            clear_pending();
                            // Transport failure alone does not prove the native
                            // process stopped. Reap it before releasing account use.
                            supervisor.shutdown().await;
                            stop_connection_tools(*epoch).await;
                            close_active_connection(*epoch);
                        }
                        crate::agent_runtime::events::NormalizedEvent::TurnCompleted {
                            epoch,
                            thread_id,
                            turn_id,
                            status,
                            ..
                        } => {
                            crate::workbench::conversation_tools::report_capture(
                                &app,
                                *epoch,
                                crate::workbench::conversation_tools::finish(thread_id, turn_id)
                                    .await,
                            );
                            complete_active_turn(*epoch, thread_id, turn_id);
                            if status == "completed" {
                                spawn_title_generation(
                                    app.clone(),
                                    TitleRuntime::Codex(supervisor.clone()),
                                    thread_id.clone(),
                                    turn_id.clone(),
                                );
                            }
                        }
                        // Nothing in the app reads these, and some carry a whole
                        // command's output or the turn's cumulative diff.
                        crate::agent_runtime::events::NormalizedEvent::UnknownNotification {
                            ..
                        } => continue,
                        _ => {}
                    }
                    let _ = match event {
                        crate::agent_runtime::events::NormalizedEvent::ServerRequest { .. } => {
                            app.emit("workbench:event", with_session(event))
                        }
                        event => app.emit("workbench:event", event),
                    };
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    reconcile_active_turn(&app, &supervisor).await;
                    let _ = app.emit(
                        "workbench:event-lagged",
                        json!({"epoch":epoch,"skipped":skipped}),
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    supervisor.shutdown().await;
                    stop_connection_tools(epoch).await;
                    close_active_connection(epoch);
                    announce_active_turn(&app, None);
                    return;
                }
            }
        }
    });
}

/// Stops ChatGPT conversations for an app exit: the running turn is asked to
/// stop and given a moment to record how it ended, then App Server is stopped.
/// A turn left open is closed as interrupted on the next launch, and a
/// connection still starting can't hold up the exit: App Server stops by itself
/// when this process ends.
pub(crate) async fn stop_codex_runtime() {
    let stop = stop_codex(crate::workbench::codex::supervisor_manager());
    let _ = tokio::time::timeout(std::time::Duration::from_secs(12), stop).await;
}

pub(crate) async fn stop_codex(manager: &crate::workbench::codex::SupervisorManager) {
    let Some(supervisor) = manager.current().await else {
        return;
    };
    let identity = {
        let state = active_turn_state()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state
            .thread_id
            .clone()
            .zip(state.turn_id.clone())
            .filter(|_| state.epoch == Some(supervisor.status().epoch))
    };
    if let Some((thread, turn)) = identity {
        let interrupted = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            supervisor.interrupt_turn(&thread, &turn),
        )
        .await;
        if matches!(interrupted, Ok(Ok(()))) {
            for _ in 0..40 {
                if !turn_has_live_owner(&thread, &turn) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }
    }
    manager.shutdown().await;
}

/// Recover permit ownership from authoritative native state after event loss.
async fn reconcile_active_turn(
    app: &tauri::AppHandle,
    supervisor: &crate::workbench::codex::AppServerSupervisor,
) {
    let epoch = supervisor.status().epoch;
    let identity = {
        let state = active_turn_state()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if state.epoch != Some(epoch) {
            return;
        }
        state.thread_id.clone().zip(state.turn_id.clone())
    };
    let Some((thread, turn)) = identity else {
        return;
    };
    // Status only: the heartbeat runs every few seconds for the whole turn,
    // so it must not reread the conversation's items.
    if let Ok(recent) = supervisor.recent_turns(&thread).await {
        if let Some(native_turn) = recovered_terminal(&recent, &turn) {
            crate::workbench::conversation_tools::report_capture(
                app,
                epoch,
                crate::workbench::conversation_tools::finish(&thread, &turn).await,
            );
            complete_active_turn(epoch, &thread, &turn);
            // The same loss that hid completion from the permit owner can hide
            // it from the renderer. Re-emit the authoritative terminal identity.
            let _ = app.emit(
                "workbench:event",
                crate::agent_runtime::events::NormalizedEvent::TurnCompleted {
                    epoch,
                    thread_id: thread,
                    turn_id: turn,
                    status: native_turn["status"].as_str().unwrap().into(),
                    turn: native_turn.clone(),
                },
            );
        }
    }
}

/// The terminal native turn with this id in a `thread/turns/list` page.
pub(crate) fn recovered_terminal<'a>(recent: &'a Value, turn: &str) -> Option<&'a Value> {
    recent["data"].as_array()?.iter().find(|t| {
        t["id"] == turn
            && matches!(
                t["status"].as_str(),
                Some("completed" | "failed" | "interrupted")
            )
    })
}
