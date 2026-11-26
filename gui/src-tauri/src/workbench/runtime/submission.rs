//! Provider-neutral admission, preparation, dispatch and acknowledgement.
use super::codex::ensure_event_bridge;
use super::{
    active_turn_state, announce_active_turn, begin_turn_setup, clear_sent_draft,
    close_active_connection, run_store, turn_queue, RuntimeCommandError, SendTurnRequest,
    SendTurnResult, TurnOrigin,
};
use crate::workbench::chat_approval::Mode;
use crate::workbench::store::{UpdateSessionRequest, WorkbenchError};
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
        let approval_mode = (!automated).then(|| Mode::parse(&runtime["approvalMode"])).flatten();
        if let Some(provider) = crate::workbench::api::selected(&runtime)? {
            return crate::workbench::api::submit(desktop(app)?, request, binding, prepared, automated, provider, control).await;
        }
        if crate::workbench::claude::selected(&runtime)? {
            let auth_mode = crate::agent_runtime::claude::AuthMode::from_runtime(runtime["runtime"].as_str().unwrap_or("claude")).map_err(WorkbenchError::invalid)?;
            crate::usage_allowance::claude_dispatch(auth_mode).map_err(WorkbenchError::invalid)?;
            return crate::workbench::claude::submit(desktop(app)?, request, binding, prepared, automated, approval_mode.unwrap_or(Mode::Sandbox), auth_mode, control).await;
        }
        let app = desktop(app)?;
        let history_session = request.session_id.clone();
        let history_binding = binding.clone();
        let approval_mode = approval_mode.unwrap_or_default();
        let fingerprint = approval_mode.codex_fingerprint(&prepared.effective.fingerprint);
        let history_fingerprint = fingerprint.clone();
        let transferred_history = run_store(move |store| crate::workbench::history::codex_context(&store, &history_session, history_binding.as_ref(), &history_fingerprint)).await?;
        // Codex runs the model's commands in this folder; a verbatim `\\?\` path
        // would put cmd.exe in C:\Windows. Path checks canonicalize both sides.
        let root = crate::file_viewer::system_path(&runtime_root).to_string_lossy().into_owned();
        let supervisor = crate::workbench::codex::supervisor_manager().connect().await?;
        let account_lease = supervisor.account_lease().await?;
        active_turn_state().lock().unwrap_or_else(|error| error.into_inner()).account_lease = account_lease;
        active_turn_state()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .epoch = Some(supervisor.status().epoch);
        ensure_event_bridge(app, supervisor.clone());
        let connection = open_thread(&supervisor, &request, binding, transferred_history, &prepared, root, approval_mode).await?;
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
        let instruction_sources = connection.instruction_sources.clone();
        run_store(move |store| {
            store.set_binding_harness(&binding_for_snapshot, &fingerprint, &instruction_sources)
        })
        .await?;
        crate::usage_allowance::subscription_dispatch().map_err(WorkbenchError::invalid)?;
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
pub(crate) async fn open_thread(
    supervisor: &crate::workbench::codex::AppServerSupervisor,
    request: &SendTurnRequest,
    binding: Option<crate::workbench::store::SessionBinding>,
    transferred_history: Option<String>,
    prepared: &crate::workbench::research::PreparedHarness,
    root: String,
    approval_mode: Mode,
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
                (permission_profile, approval_mode),
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
            approval_mode,
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
