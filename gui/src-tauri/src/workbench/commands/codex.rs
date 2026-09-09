//! Codex Tauri commands.

use super::*;

#[tauri::command]
pub async fn workbench_reconcile_workspace_roots(
    operation_id: String,
) -> WorkbenchResult<ReconcileResult> {
    run_store(move |store| store.reconcile_workspace_roots(&operation_id)).await
}

#[tauri::command]
pub async fn workbench_codex_connect(
    app: tauri::AppHandle,
) -> Result<crate::workbench::codex::SupervisorStatus, crate::workbench::codex::RequestError> {
    let supervisor = crate::workbench::codex::supervisor_manager()
        .connect()
        .await?;
    ensure_event_bridge(app, supervisor.clone());
    Ok(supervisor.status())
}

#[tauri::command]
pub async fn workbench_codex_account_state(
    app: tauri::AppHandle,
    refresh_token: bool,
) -> Result<crate::workbench::codex::AccountState, crate::workbench::codex::RequestError> {
    let supervisor = crate::workbench::codex::supervisor_manager()
        .connect()
        .await?;
    ensure_event_bridge(app, supervisor.clone());
    supervisor.account_state(refresh_token).await
}

#[tauri::command]
pub async fn workbench_codex_login_start(
    app: tauri::AppHandle,
) -> Result<crate::workbench::codex::LoginStart, crate::workbench::codex::RequestError> {
    let supervisor = crate::workbench::codex::supervisor_manager()
        .connect()
        .await?;
    ensure_event_bridge(app.clone(), supervisor.clone());
    let login = supervisor.login_start().await?;
    if !is_safe_auth_url(&login.auth_url) {
        let _ = supervisor.login_cancel(&login.login_id).await;
        return Err(crate::workbench::codex::RequestError::unavailable(
            "Codex App Server returned an unsafe authentication URL",
        ));
    }
    if let Err(error) = open_auth_url(&app, &login.auth_url) {
        let _ = supervisor.login_cancel(&login.login_id).await;
        return Err(crate::workbench::codex::RequestError::unavailable(format!(
            "Could not open the ChatGPT sign-in page: {error}"
        )));
    }
    Ok(login)
}

#[tauri::command]
pub async fn workbench_codex_login_cancel(
    login_id: String,
) -> Result<bool, crate::workbench::codex::RequestError> {
    crate::workbench::codex::supervisor_manager()
        .connect()
        .await?
        .login_cancel(&login_id)
        .await
}

#[tauri::command]
pub async fn workbench_codex_logout() -> Result<(), crate::workbench::codex::RequestError> {
    crate::workbench::codex::supervisor_manager()
        .connect()
        .await?
        .logout()
        .await
}

#[tauri::command]
pub async fn workbench_codex_model_catalog(
) -> Result<crate::workbench::codex::ModelCatalog, crate::workbench::codex::RequestError> {
    crate::workbench::codex::supervisor_manager()
        .connect()
        .await?
        .model_catalog()
        .await
}

#[tauri::command]
pub async fn workbench_codex_validate_model_selection(
    model: String,
    effort: Option<String>,
) -> Result<crate::workbench::codex::WorkspaceModel, crate::workbench::codex::RequestError> {
    crate::workbench::codex::supervisor_manager()
        .connect()
        .await?
        .validate_model_selection(&model, effort.as_deref())
        .await
}

#[tauri::command]
pub async fn workbench_codex_rate_limits(
) -> Result<crate::workbench::codex::RateLimits, crate::workbench::codex::RequestError> {
    crate::workbench::codex::supervisor_manager()
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
    task_binding: Option<crate::workbench::tasks::TaskBinding>,
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
        run_store(move |store| crate::workbench::tasks::validate_binding(&store, &binding)).await?;
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
        state.permit = Some(permit);
    }

    let mut dispatched_supervisor = None;
    let mut result: Result<SendTurnResult, RuntimeCommandError> = async {
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
            if store.session_snapshot(&session_id)?.session.workspace_id.as_deref().is_some_and(crate::workbench::research::jobs::workspace_active) {return Err(WorkbenchError::conflict("This project has a local job holding its write resources. Finish or stop it before starting a research turn."));}
            Ok::<_, WorkbenchError>((
                store.active_binding(&session_id)?,
                store.runtime_root(&session_id)?,
                crate::workbench::research::prepare_turn(&store, &session_id)?,
            ))
        })
        .await?;
        let root = runtime_root.to_string_lossy().into_owned();
        let permission_profile = prepared.effective.permission_profile.as_str();
        let supervisor = crate::workbench::codex::supervisor_manager().connect().await?;
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
                .start_thread(crate::workbench::codex::StartThreadRequest {
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
        dispatched_supervisor = Some(supervisor.clone());
        let turn_id = supervisor
            .start_turn_with_acceptance(crate::workbench::codex::StartTurnRequest {
                workbench_binding_id: binding_id.clone(),
                thread_id: connection.thread_id.clone(),
                text: request.text,
                client_user_message_id: request.client_submission_id.clone(),
                model: request.model,
                effort: request.effort,
            }, |turn_id| {
                active_turn_state().lock().unwrap_or_else(|error| error.into_inner()).turn_id = Some(turn_id.to_owned());
            })
            .await?;
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
            epoch: supervisor.status().epoch,
            thread_id: connection.thread_id,
            turn_id,
            draft_cleared,
        })
    }
    .await;
    // An unacknowledged dispatch retains ownership until its connection is reaped.
    let unacknowledged = result.is_err()
        && active_turn_state()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .turn_id
            .is_none();
    if unacknowledged {
        if let Some(supervisor) = dispatched_supervisor.as_ref() {
            supervisor.shutdown().await;
            close_active_connection(supervisor.status().epoch);
        }
    }
    let mut state = active_turn_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if state.turn_id.is_some() {
        if let Err(error) = &mut result {
            error.code = "accepted_turn_bookkeeping_failed".into();
            error.retryable = false;
            error.recovery = Some("The native turn was accepted. Reconcile the conversation before deciding whether another submission is needed.".into());
        }
    }
    state.finish_setup(result.is_err());
    result
}

#[tauri::command]
pub async fn workbench_codex_interrupt_turn(
    thread_id: String,
    turn_id: String,
) -> Result<(), crate::workbench::codex::RequestError> {
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
            crate::workbench::research::resolve_harness(&store, &lookup_session_id)?,
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
    let permission_profile = harness.permission_profile.as_str();
    let supervisor = crate::workbench::codex::supervisor_manager()
        .connect()
        .await?;
    ensure_event_bridge(app.clone(), supervisor.clone());
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
    reconcile_active_turn(&app, &supervisor).await;
    Ok(true)
}

#[tauri::command]
pub async fn workbench_codex_resolve_server_request(
    request: ResolveServerRequest,
) -> Result<(), crate::workbench::codex::RequestError> {
    let supervisor = crate::workbench::codex::supervisor_manager()
        .connect()
        .await?;
    let response = if let Some(result) = request.result.clone() {
        validate_server_response(&request.method, &result)?;
        Ok(result)
    } else {
        Err(crate::workbench::codex::RequestError::invalid(
            request
                .decline_message
                .clone()
                .unwrap_or_else(|| "The user declined this request".to_string()),
        ))
    };
    claim_pending_request(
        &mut pending_server_requests()
            .lock()
            .unwrap_or_else(|error| error.into_inner()),
        &request,
        supervisor.status().epoch,
    )?;
    supervisor
        .respond_to_server_request(request.request_id, response)
        .await?;
    Ok(())
}

pub(super) fn claim_pending_request(
    pending: &mut HashMap<String, crate::workbench::codex::NormalizedEvent>,
    request: &ResolveServerRequest,
    epoch: u64,
) -> Result<(), crate::workbench::codex::RequestError> {
    if request.epoch != epoch {
        return Err(crate::workbench::codex::RequestError::invalid(
            "Server request belongs to an expired connection",
        ));
    }
    let key = request_key(&request.request_id);
    let event = pending.get(&key).ok_or_else(|| {
        crate::workbench::codex::RequestError::invalid("Server request is no longer pending")
    })?;
    validate_pending_request(event, request.epoch, &request.method)?;
    // Claim before the awaited write: an uncertain write must never be replayed.
    pending.remove(&key);
    Ok(())
}

#[tauri::command]
pub fn workbench_codex_pending_requests() -> Vec<crate::workbench::codex::NormalizedEvent> {
    let mut pending = pending_server_requests()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .values()
        .cloned()
        .collect::<Vec<_>>();
    pending.sort_by_key(|event| match event {
        crate::workbench::codex::NormalizedEvent::ServerRequest { request_id, .. } => {
            request_key(request_id)
        }
        _ => String::new(),
    });
    pending
}

pub(super) fn ensure_event_bridge(
    app: tauri::AppHandle,
    supervisor: Arc<crate::workbench::codex::AppServerSupervisor>,
) {
    let epoch = supervisor.status().epoch;
    if EVENT_BRIDGE_EPOCH.swap(epoch, Ordering::AcqRel) == epoch {
        return;
    }
    pending_server_requests()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();
    let mut events = supervisor.subscribe();
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
                close_active_connection(epoch);
                break;
            }
            reconcile_active_turn(&recovery_app, &recovery).await;
        }
    });
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
                    if crate::workbench::codex::event_thread_id(&event)
                        .is_some_and(|thread| supervisor.is_side_thread(thread))
                    {
                        continue;
                    }
                    if let crate::workbench::codex::NormalizedEvent::ServerRequest {
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
                                    crate::workbench::research::handle_dynamic_tool_call(
                                        &store, &params,
                                    )
                                })
                                .await;
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
                    if crate::workbench::discovery::resolve_autonomous_request(&supervisor, &event)
                        .await
                    {
                        continue;
                    }
                    match &event {
                        crate::workbench::codex::NormalizedEvent::ServerRequest {
                            request_id,
                            ..
                        } => {
                            pending_server_requests()
                                .lock()
                                .unwrap_or_else(|error| error.into_inner())
                                .insert(request_key(request_id), event.clone());
                        }
                        crate::workbench::codex::NormalizedEvent::ServerRequestResolved {
                            request_id,
                            ..
                        } => {
                            pending_server_requests()
                                .lock()
                                .unwrap_or_else(|error| error.into_inner())
                                .remove(&request_key(request_id));
                        }
                        crate::workbench::codex::NormalizedEvent::ConnectionClosed {
                            epoch,
                            ..
                        } => {
                            pending_server_requests()
                                .lock()
                                .unwrap_or_else(|error| error.into_inner())
                                .clear();
                            close_active_connection(*epoch);
                        }
                        crate::workbench::codex::NormalizedEvent::TurnCompleted {
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
                    reconcile_active_turn(&app, &supervisor).await;
                    let _ = app.emit(
                        "workbench:event-lagged",
                        json!({"epoch":epoch,"skipped":skipped}),
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    close_active_connection(epoch);
                    return;
                }
            }
        }
    });
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
    if let Ok(snapshot) = supervisor.read_thread(&thread).await {
        if let Some(native_turn) = recovered_terminal(&snapshot, &thread, &turn) {
            complete_active_turn(epoch, &thread, &turn);
            // The same loss that hid completion from the permit owner can hide
            // it from the renderer. Re-emit the authoritative terminal identity.
            let _ = app.emit(
                "workbench:event",
                crate::workbench::codex::NormalizedEvent::TurnCompleted {
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

pub(super) fn recovered_terminal<'a>(
    snapshot: &'a Value,
    thread: &str,
    turn: &str,
) -> Option<&'a Value> {
    if snapshot["thread"]["id"] != thread {
        return None;
    }
    snapshot["thread"]["turns"].as_array()?.iter().find(|t| {
        t["id"] == turn
            && matches!(
                t["status"].as_str(),
                Some("completed" | "failed" | "interrupted")
            )
    })
}

fn request_key(request_id: &Value) -> String {
    serde_json::to_string(request_id).unwrap_or_else(|_| "invalid-request-id".to_string())
}

pub(super) fn validate_pending_request(
    event: &crate::workbench::codex::NormalizedEvent,
    active_epoch: u64,
    submitted_method: &str,
) -> Result<(), crate::workbench::codex::RequestError> {
    match event {
        crate::workbench::codex::NormalizedEvent::ServerRequest { epoch, method, .. }
            if *epoch == active_epoch && method == submitted_method =>
        {
            Ok(())
        }
        crate::workbench::codex::NormalizedEvent::ServerRequest { epoch, .. }
            if *epoch != active_epoch =>
        {
            Err(crate::workbench::codex::RequestError::invalid(
                "Server request belongs to an expired connection",
            ))
        }
        crate::workbench::codex::NormalizedEvent::ServerRequest { .. } => Err(
            crate::workbench::codex::RequestError::invalid("Server request method did not match"),
        ),
        _ => Err(crate::workbench::codex::RequestError::invalid(
            "Pending request registry contained a non-request event",
        )),
    }
}

pub(super) fn validate_server_response(
    method: &str,
    result: &Value,
) -> Result<(), crate::workbench::codex::RequestError> {
    match method {
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
            if !matches!(
                result.get("decision").and_then(Value::as_str),
                Some("accept" | "acceptForSession" | "decline" | "cancel")
            ) {
                return Err(crate::workbench::codex::RequestError::invalid(
                    "Invalid approval decision",
                ));
            }
        }
        "item/tool/requestUserInput" => {
            if !result.get("answers").is_some_and(Value::is_object) {
                return Err(crate::workbench::codex::RequestError::invalid(
                    "User-input response omitted answers",
                ));
            }
        }
        "item/permissions/requestApproval" => {
            if !result.get("permissions").is_some_and(Value::is_object) {
                return Err(crate::workbench::codex::RequestError::invalid(
                    "Permission response omitted permissions",
                ));
            }
        }
        _ => {
            return Err(crate::workbench::codex::RequestError::invalid(
                "Unsupported server request method",
            ))
        }
    }
    Ok(())
}

pub(super) fn transcript_text(payload: &Value) -> Option<String> {
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
