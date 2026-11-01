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
    crate::agent_runtime::codex::chatgpt::chatgpt_account_status(app, Some(refresh_token))
        .await
        .map(|status| status.account)
        .map_err(crate::workbench::codex::RequestError::unavailable)
}

#[tauri::command]
pub async fn workbench_codex_login_start(
    app: tauri::AppHandle,
) -> Result<crate::workbench::codex::LoginStart, crate::workbench::codex::RequestError> {
    crate::agent_runtime::codex::chatgpt::begin_browser_login(app)
        .await
        .map(|(_, login)| login)
        .map_err(crate::workbench::codex::RequestError::unavailable)
}

#[tauri::command]
pub async fn workbench_codex_login_cancel(
    login_id: String,
) -> Result<bool, crate::workbench::codex::RequestError> {
    crate::agent_runtime::codex::chatgpt::connect()
        .await
        .map_err(crate::workbench::codex::RequestError::unavailable)?
        .cancel_login(&login_id, None)
        .await
        .map_err(crate::workbench::codex::RequestError::unavailable)
}

#[tauri::command]
pub async fn workbench_codex_logout() -> Result<(), crate::workbench::codex::RequestError> {
    crate::agent_runtime::codex::chatgpt::chatgpt_logout()
        .await
        .map_err(crate::workbench::codex::RequestError::unavailable)
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
    submit_turn(Some(app), request, None, Default::default()).await
}

pub(crate) fn task_turn_available() -> bool {
    turn_queue().available_permits() > 0
}

/// `app` is required to reach a runtime. Without it the checks and preparation
/// still run and the message is reported as not sent, which lets a test drive a
/// real task submission up to this boundary with no provider.
pub(crate) async fn submit_turn(
    app: Option<tauri::AppHandle>,
    request: SendTurnRequest,
    task_binding: Option<crate::workbench::tasks::TaskBinding>,
    control: crate::workbench::submission::Control,
) -> Result<SendTurnResult, RuntimeCommandError> {
    control.check()?;
    let mut request = request;
    let automated = task_binding.is_some();
    let origin = TurnOrigin::of(automated, control.0.is_some());
    // Admission writes nothing, so a refused send leaves its draft and its
    // conversation exactly as they were.
    let permit = turn_queue().clone().try_acquire_owned().map_err(|_| {
        let (code, message) = if automated {
            ("task_busy", "task_busy: Workspace is running another turn")
        } else {
            (TURN_BUSY, "Another response is still running")
        };
        RuntimeCommandError {
            code: code.into(),
            message: message.into(),
            retryable: true,
            recovery: None,
        }
    })?;
    if let Some(binding) = task_binding {
        run_store(move |store| crate::workbench::tasks::validate_binding(&store, &binding)).await?;
    }
    let owner_session = request.session_id.clone();
    let announcer = app.clone();
    begin_turn_setup(owner_session.clone(), origin, permit);
    if let Some(app) = &announcer {
        announce_active_turn(app, None);
    }

    let mut dispatched_supervisor = None;
    // Set when the message is about to be written to App Server. A failure
    // before then proves the message was not sent. API and Claude never set it:
    // each one fails only before its prompt leaves, and succeeds from there on.
    let mut handed_off = false;
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
        let (binding, runtime_root, prepared, runtime) = run_store(move |store| {
            if store.session_snapshot(&session_id)?.session.workspace_id.as_deref().is_some_and(crate::workbench::research::jobs::workspace_active) {return Err(WorkbenchError::conflict("This project has a local job holding its write resources. Finish or stop it before starting a research turn."));}
            if !automated { crate::workbench::project::conversation_copy::prepare(&store, &session_id)?; }
            Ok::<_, WorkbenchError>((
                store.active_binding(&session_id)?,
                store.runtime_root(&session_id)?,
                crate::workbench::research::prepare_turn(&store, &session_id)?,
                store.session_snapshot(&session_id)?.session.overrides,
            ))
        })
        .await?;
        control.check()?;
        if automated {
            request.inherit_selection(&runtime);
        }
        let desktop = |app: Option<tauri::AppHandle>| {
            app.ok_or_else(|| {
                WorkbenchError::invalid("Workspace execution requires the desktop runtime")
            })
        };
        if let Some(provider) = crate::workbench::api::selected(&runtime)? {
            return crate::workbench::api::submit(desktop(app)?, request, binding, prepared, automated, provider, control).await;
        }
        if crate::workbench::claude::selected(&runtime)? {
            return crate::workbench::claude::submit(desktop(app)?, request, binding, prepared, automated, control).await;
        }
        let app = desktop(app)?;
        let history_session = request.session_id.clone();
        let history_binding = binding.clone();
        let fingerprint = prepared.effective.fingerprint.clone();
        let transferred_history = run_store(move |store| crate::workbench::history::codex_context(&store, &history_session, history_binding.as_ref(), &fingerprint)).await?;
        let root = runtime_root.to_string_lossy().into_owned();
        let supervisor = crate::workbench::codex::supervisor_manager().connect().await?;
        let account_lease = supervisor.account_lease().await?;
        active_turn_state().lock().unwrap_or_else(|error| error.into_inner()).account_lease = account_lease;
        active_turn_state()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .epoch = Some(supervisor.status().epoch);
        ensure_event_bridge(app, supervisor.clone());
        let connection = open_thread(&supervisor, &request, binding, transferred_history, &prepared, root).await?;
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
        let sent_text = request.text.clone();
        let turn_request = crate::workbench::codex::StartTurnRequest {
            workbench_binding_id: binding_id.clone(),
            thread_id: connection.thread_id.clone(),
            text: request.text,
            client_user_message_id: request.client_submission_id.clone(),
            model: request.model,
            effort: request.effort,
        };
        // Validation (including the live model catalog check) writes nothing
        // to App Server, so its failures must not be treated as an uncertain
        // dispatch that tears the connection down.
        control.preflight(async { supervisor.validate_turn(&turn_request).await.map_err(Into::into) }).await?;
        control.check()?;
        // A thread only counts as resumable once a turn may have reached it.
        // This stays ahead of the send: after an unconfirmed send, reconciling
        // has to find the thread to learn whether the turn arrived.
        let binding_for_snapshot = binding_id.clone();
        let fingerprint = prepared.effective.fingerprint.clone();
        let instruction_sources = connection.instruction_sources.clone();
        run_store(move |store| {
            store.set_binding_harness(&binding_for_snapshot, &fingerprint, &instruction_sources)
        })
        .await?;
        handed_off = true;
        dispatched_supervisor = Some(supervisor.clone());
        let turn_id = supervisor
            .dispatch_turn(turn_request, |turn_id| {
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
        let session_id = request.session_id;
        let submission = request.client_submission_id;
        run_store(move |store| clear_sent_draft(&store, &session_id, &sent_text, &submission))
            .await
            .unwrap_or(false)
        };
        Ok(SendTurnResult {
            epoch: supervisor.status().epoch,
            thread_id: connection.thread_id,
            turn_id,
            draft_cleared,
        })
    }
    .await;
    if !handed_off {
        if let Err(error) = &mut result {
            error.code = TURN_NOT_SENT.into();
        }
    }
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
    drop(state);
    if let Some(app) = &announcer {
        let failure = result.as_ref().err();
        announce_active_turn(app, failure.map(|error| (owner_session.as_str(), error)));
    }
    result
}

/// Continues the bound thread, or starts a successor seeded with the app's own
/// history when there is nothing to continue. Binding the successor retires the
/// old thread. Only `resume_thread`'s definite answers lead here, so an
/// unconfirmed send is never repeated on a new thread.
pub(super) async fn open_thread(
    supervisor: &crate::workbench::codex::AppServerSupervisor,
    request: &SendTurnRequest,
    binding: Option<crate::workbench::store::SessionBinding>,
    transferred_history: Option<String>,
    prepared: &crate::workbench::research::PreparedHarness,
    root: String,
) -> Result<crate::workbench::codex::ThreadConnection, RuntimeCommandError> {
    let effective = &prepared.effective;
    let permission_profile = effective.permission_profile.as_str();
    let config = || {
        Some(effective.capabilities.codex_config(
            effective.web_search,
            effective.command_network,
            permission_profile,
        ))
    };
    if let Some(binding) = binding.filter(|_| transferred_history.is_none()) {
        let resumed = supervisor
            .resume_thread(
                &request.session_id,
                &binding.provider_thread_id,
                permission_profile,
                &root,
                vec![root.clone()],
                config(),
            )
            .await?;
        if let Some(connection) = resumed {
            return Ok(connection);
        }
    }
    let history = match transferred_history {
        Some(history) => history,
        None => {
            let session = request.session_id.clone();
            run_store(move |store| crate::workbench::history::recent_context(&store, &session))
                .await?
        }
    };
    Ok(supervisor
        .start_thread(crate::workbench::codex::StartThreadRequest {
            base_instructions: effective.preset.base_instructions.clone(),
            workbench_session_id: request.session_id.clone(),
            cwd: root.clone(),
            runtime_workspace_roots: vec![root],
            permissions: permission_profile.to_string(),
            developer_instructions: format!("{}\n{}", effective.developer_instructions, history),
            model: request.model.clone(),
            effort: request.effort.clone(),
            dynamic_tools: effective.dynamic_tools.clone(),
            config: config(),
        })
        .await?)
}

/// Code for a submission that failed before the message left Pipeline. The
/// message was definitely not sent, so the renderer can restore it as an
/// unsent draft instead of reporting an uncertain delivery.
const TURN_NOT_SENT: &str = "turn_not_sent";

/// Code for a composer send refused because another turn holds the permit. A
/// send never waits for one: the turn ahead may be waiting on an approval the
/// researcher can't see from here.
const TURN_BUSY: &str = "turn_busy";

#[tauri::command]
pub async fn workbench_codex_interrupt_turn(
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
#[tauri::command]
pub async fn workbench_codex_reconcile_session(
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
pub(super) async fn close_abandoned_turns(
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
    if binding.harness_fingerprint.as_deref() != Some(&harness.fingerprint) {
        return Ok(false);
    }
    let permission_profile = harness.permission_profile.as_str();
    let supervisor = crate::workbench::codex::supervisor_manager()
        .connect()
        .await?;
    ensure_event_bridge(app.clone(), supervisor.clone());
    let root = root.to_string_lossy().into_owned();
    let Some(connection) = supervisor
        .resume_thread(
            session_id,
            &binding.provider_thread_id,
            permission_profile,
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
    pending: &mut HashMap<String, crate::agent_runtime::events::NormalizedEvent>,
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
pub fn workbench_codex_pending_requests() -> Vec<PendingRequest> {
    let mut pending = pending_server_requests()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .values()
        .cloned()
        .collect::<Vec<_>>();
    pending.sort_by_key(|event| match event {
        crate::agent_runtime::events::NormalizedEvent::ServerRequest { request_id, .. } => {
            request_key(request_id)
        }
        _ => String::new(),
    });
    pending.into_iter().map(with_session).collect()
}

/// What the event bridge reads from one connection: its server requests, which
/// lag can't drop, and everything else. A request goes first: the transport
/// queues it before any later event, so its resolution can't overtake it.
pub(super) struct BridgeEvents {
    pub(super) events:
        tokio::sync::broadcast::Receiver<crate::agent_runtime::events::NormalizedEvent>,
    pub(super) requests: crate::agent_runtime::codex::ServerRequestSink,
    pub(super) ended: bool,
}

impl BridgeEvents {
    /// `None` after the connection's last event. The bridge holds the
    /// connection open, so `Closed` alone would never arrive.
    pub(super) async fn next(
        &mut self,
    ) -> Option<
        Result<
            crate::agent_runtime::events::NormalizedEvent,
            tokio::sync::broadcast::error::RecvError,
        >,
    > {
        if self.ended {
            return None;
        }
        let received = tokio::select! {
            biased;
            Some(request) = self.requests.recv() => Ok(request),
            received = self.events.recv() => received,
        };
        self.ended = matches!(
            received,
            Ok(crate::agent_runtime::events::NormalizedEvent::ConnectionClosed { .. })
        );
        Some(received)
    }
}

/// Refuses a server request no screen can answer. It would otherwise show a card
/// whose buttons can't reach Codex, and the turn would wait until Stop.
pub(super) async fn refuse_unsupported_request(
    supervisor: &crate::workbench::codex::AppServerSupervisor,
    event: &crate::agent_runtime::events::NormalizedEvent,
) -> bool {
    let crate::agent_runtime::events::NormalizedEvent::ServerRequest {
        request_id, method, ..
    } = event
    else {
        return false;
    };
    let any_answer = json!({"decision":"decline","answers":{},"permissions":{}});
    if validate_server_response(method, &any_answer).is_ok() {
        return false;
    }
    let refusal = crate::workbench::codex::RequestError::invalid(format!(
        "Pipeline does not support {method}"
    ));
    let _ = supervisor
        .respond_to_server_request(request_id.clone(), Err(refusal))
        .await;
    true
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
    let mut source = BridgeEvents {
        events: supervisor.subscribe(),
        requests: supervisor.request_sink(),
        ended: false,
    };
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
                crate::workbench::conversation_tools::stop_all().await;
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
                    if refuse_unsupported_request(&supervisor, &event).await {
                        continue;
                    }
                    match &event {
                        crate::agent_runtime::events::NormalizedEvent::ServerRequest {
                            request_id,
                            ..
                        } => {
                            pending_server_requests()
                                .lock()
                                .unwrap_or_else(|error| error.into_inner())
                                .insert(request_key(request_id), event.clone());
                        }
                        crate::agent_runtime::events::NormalizedEvent::ServerRequestResolved {
                            request_id,
                            ..
                        } => {
                            pending_server_requests()
                                .lock()
                                .unwrap_or_else(|error| error.into_inner())
                                .remove(&request_key(request_id));
                        }
                        crate::agent_runtime::events::NormalizedEvent::ConnectionClosed {
                            epoch,
                            ..
                        } => {
                            pending_server_requests()
                                .lock()
                                .unwrap_or_else(|error| error.into_inner())
                                .clear();
                            // Transport failure alone does not prove the native
                            // process stopped. Reap it before releasing account use.
                            supervisor.shutdown().await;
                            crate::workbench::conversation_tools::stop_all().await;
                            close_active_connection(*epoch);
                        }
                        crate::agent_runtime::events::NormalizedEvent::TurnCompleted {
                            epoch,
                            thread_id,
                            turn_id,
                            status,
                            ..
                        } => {
                            if let Err(error) =
                                crate::workbench::conversation_tools::finish(thread_id, turn_id)
                                    .await
                            {
                                let _ = app.emit("workbench:event", crate::agent_runtime::events::NormalizedEvent::ProtocolWarning {epoch:*epoch,message:format!("The working copy could not be captured: {}. Its files are retained; refresh the working copy before accepting changes.",error.message)});
                            }
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
                    crate::workbench::conversation_tools::stop_all().await;
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

pub(super) async fn stop_codex(manager: &crate::workbench::codex::SupervisorManager) {
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
            if let Err(error) = crate::workbench::conversation_tools::finish(&thread, &turn).await {
                let _ = app.emit("workbench:event", crate::agent_runtime::events::NormalizedEvent::ProtocolWarning {epoch,message:format!("The working copy could not be captured: {}. Its files are retained; refresh before accepting changes.",error.message)});
            }
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
pub(super) fn recovered_terminal<'a>(recent: &'a Value, turn: &str) -> Option<&'a Value> {
    recent["data"].as_array()?.iter().find(|t| {
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
    event: &crate::agent_runtime::events::NormalizedEvent,
    active_epoch: u64,
    submitted_method: &str,
) -> Result<(), crate::workbench::codex::RequestError> {
    match event {
        crate::agent_runtime::events::NormalizedEvent::ServerRequest { epoch, method, .. }
            if *epoch == active_epoch && method == submitted_method =>
        {
            Ok(())
        }
        crate::agent_runtime::events::NormalizedEvent::ServerRequest { epoch, .. }
            if *epoch != active_epoch =>
        {
            Err(crate::workbench::codex::RequestError::invalid(
                "Server request belongs to an expired connection",
            ))
        }
        crate::agent_runtime::events::NormalizedEvent::ServerRequest { .. } => Err(
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
