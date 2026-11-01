use super::codex::{
    close_abandoned_turns, open_thread, refuse_unsupported_request, stop_codex, BridgeEvents,
};
use super::*;
use crate::agent_runtime::events::NormalizedEvent;
use crate::workbench::store::SessionBinding;

#[test]
fn storage_maintenance_and_native_setup_exclude_each_other() {
    let queue = Arc::new(Semaphore::new(1));
    let setup = queue.clone().try_acquire_owned().unwrap();
    assert!(storage_turn_permit(queue.clone()).is_err());
    drop(setup);
    let maintenance = storage_turn_permit(queue.clone()).unwrap();
    assert!(queue.clone().try_acquire_owned().is_err());
    drop(maintenance);
    assert!(queue.try_acquire_owned().is_ok());
}

#[test]
fn database_worker_pool_is_deliberately_bounded() {
    assert_eq!(DATABASE_WORKER_LIMIT, 4);
}

#[test]
fn authentication_urls_must_be_bounded_https_urls() {
    assert!(is_safe_auth_url("https://auth.openai.com/authorize?x=1"));
    assert!(!is_safe_auth_url("http://auth.openai.com/authorize"));
    assert!(!is_safe_auth_url("file:///tmp/credential"));
    assert!(!is_safe_auth_url("https://"));
    assert!(!is_safe_auth_url("https://user@auth.openai.com/login"));
    assert!(!is_safe_auth_url("https://auth.openai.com/\nheader"));
}

#[test]
fn interactive_server_responses_are_method_scoped() {
    assert!(validate_server_response(
        "item/commandExecution/requestApproval",
        &json!({"decision":"accept"}),
    )
    .is_ok());
    assert!(validate_server_response(
        "item/tool/requestUserInput",
        &json!({"answers":{"choice":{"answers":["A"]}}}),
    )
    .is_ok());
    assert!(
        validate_server_response("item/fileChange/requestApproval", &json!({"answers":{}}),)
            .is_err()
    );
    assert!(validate_server_response("future/request", &json!({})).is_err());

    let pending = super::super::codex::NormalizedEvent::ServerRequest {
        epoch: 7,
        request_id: json!("approval-1"),
        method: "item/fileChange/requestApproval".to_string(),
        params: json!({"threadId":"thread-1","turnId":"turn-1"}),
    };
    assert!(validate_pending_request(&pending, 7, "item/fileChange/requestApproval").is_ok());
    assert!(
        validate_pending_request(&pending, 7, "item/commandExecution/requestApproval").is_err()
    );
    assert!(validate_pending_request(&pending, 8, "item/fileChange/requestApproval").is_err());
}

#[test]
fn transcript_export_text_accepts_string_and_content_parts() {
    assert_eq!(
        transcript_text(&json!({"text":"hello"})).as_deref(),
        Some("hello")
    );
    assert_eq!(
        transcript_text(
            &json!({"content":[{"type":"text","text":"one"},{"type":"text","text":"two"}]})
        )
        .as_deref(),
        Some("one\ntwo")
    );
}

#[test]
fn approval_card_epoch_is_claimed_once_even_when_ids_are_reused() {
    let pending = crate::agent_runtime::events::NormalizedEvent::ServerRequest {
        epoch: 8,
        request_id: json!(42),
        method: "item/fileChange/requestApproval".into(),
        params: json!({"threadId":"new-thread","turnId":"new-turn"}),
    };
    let mut registry = HashMap::from([("42".into(), pending)]);
    let mut response = ResolveServerRequest {
        epoch: 7,
        request_id: json!(42),
        method: "item/fileChange/requestApproval".into(),
        result: Some(json!({"decision":"accept"})),
        decline_message: None,
    };
    assert!(codex::claim_pending_request(&mut registry, &response, 8).is_err());
    assert_eq!(registry.len(), 1);
    response.epoch = 8;
    assert!(codex::claim_pending_request(&mut registry, &response, 8).is_ok());
    assert!(codex::claim_pending_request(&mut registry, &response, 8).is_err());
}

#[test]
fn acknowledged_turn_keeps_permit_on_bookkeeping_failure() {
    let queue = Arc::new(Semaphore::new(1));
    let mut state = ActiveTurnState {
        setup_in_progress: true,
        epoch: Some(8),
        thread_id: Some("thread".into()),
        turn_id: Some("turn".into()),
        permit: Some(queue.clone().try_acquire_owned().unwrap()),
        ..Default::default()
    };
    state.finish_setup(true);
    assert!(queue.clone().try_acquire_owned().is_err());
    state.completion_during_setup = Some(("thread".into(), "stale-turn".into()));
    state.finish_setup(true);
    assert!(queue.clone().try_acquire_owned().is_err());
    state.completion_during_setup = Some(("thread".into(), "turn".into()));
    state.finish_setup(true);
    assert!(queue.try_acquire_owned().is_ok());
}

#[test]
fn lag_recovery_requires_the_exact_terminal_turn() {
    let recent = json!({"data":[
        {"id":"current","status":"inProgress","items":[],"itemsView":"notLoaded"},
        {"id":"old","status":"completed","items":[],"itemsView":"notLoaded"}
    ]});
    assert!(codex::recovered_terminal(&recent, "current").is_none());
    assert!(codex::recovered_terminal(&recent, "missing").is_none());
    assert!(codex::recovered_terminal(&json!({}), "current").is_none());
    let complete = json!({"data":[{"id":"current","status":"interrupted","items":[]}]});
    assert_eq!(
        codex::recovered_terminal(&complete, "current").unwrap()["status"],
        "interrupted"
    );
}

#[test]
fn automated_selection_inherits_saved_model_and_effort_for_every_provider() {
    for runtime in [
        "codex",
        "claude",
        "openai-api",
        "anthropic-api",
        "google-api",
        "local",
    ] {
        let saved = json!({"runtime":runtime,"model":"chosen-model","effort":"high"});
        let mut request = SendTurnRequest {
            session_id: "session".into(),
            text: "prompt".into(),
            client_submission_id: "submission".into(),
            model: None,
            effort: None,
        };
        request.inherit_selection(&saved);
        assert_eq!(request.model.as_deref(), Some("chosen-model"));
        assert_eq!(request.effort.as_deref(), Some("high"));
        request.model = Some("automation-model".into());
        request.effort = None;
        request.inherit_selection(&saved);
        assert_eq!(request.model.as_deref(), Some("automation-model"));
        assert!(request.effort.is_none());
        request.model = None;
        request.effort = Some("low".into());
        request.inherit_selection(&saved);
        assert_eq!(request.effort.as_deref(), Some("low"));
    }
}

#[tokio::test]
async fn cancelled_store_wait_keeps_gate_until_worker_finishes() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(temp.path()).unwrap();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(run_store_owned(store, move |_| {
        let _ = started_tx.send(());
        release_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        let _ = finished_tx.send(());
        Ok(())
    }));
    started_rx.await.unwrap();
    task.abort();
    let _ = task.await;
    assert!(DATABASE_GATE.try_write().is_err());
    release_tx.send(()).unwrap();
    finished_rx.await.unwrap();
    let _gate = tokio::time::timeout(std::time::Duration::from_secs(2), DATABASE_GATE.write())
        .await
        .unwrap();
}

#[test]
fn new_conversations_copy_a_chosen_default_only() {
    let settings = crate::settings::Settings {
        preferred_provider: "claude".into(),
        default_model: "opus".into(),
        default_effort: "high".into(),
        ..Default::default()
    };
    assert_eq!(default_session_overrides(&settings).unwrap(), None);

    let chosen = crate::settings::Settings {
        default_model_chosen: true,
        ..settings
    };
    assert_eq!(
        default_session_overrides(&chosen).unwrap(),
        Some(json!({"runtime":"claude","model":"opus","effort":"high"}))
    );

    let api = crate::settings::Settings {
        claude_access_mode: "api".into(),
        ..chosen.clone()
    };
    assert_eq!(
        default_session_overrides(&api).unwrap().unwrap()["runtime"],
        "anthropic-api"
    );

    // The conversation picker sets a thinking level only for a chosen model.
    let automatic = crate::settings::Settings {
        default_model: String::new(),
        ..chosen
    };
    assert_eq!(
        default_session_overrides(&automatic).unwrap(),
        Some(json!({"runtime":"claude"}))
    );
}

/// A connected App Server that answers each request from `answer`: `Ok` as a
/// result, `Err` as a protocol error. The list collects every request.
fn fake_app_server(
    root: &str,
    answer: impl Fn(&Value) -> Result<Value, Value> + Send + 'static,
) -> (
    crate::workbench::codex::AppServerSupervisor,
    Arc<StdMutex<Vec<Value>>>,
) {
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};
    let (client_io, server_io) = tokio::io::duplex(1024 * 1024);
    let (client_reader, client_writer) = tokio::io::split(client_io);
    let (server_reader, mut server_writer) = tokio::io::split(server_io);
    let requests = Arc::new(StdMutex::new(Vec::new()));
    let seen = requests.clone();
    tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(server_reader);
        let mut line = String::new();
        while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
            let request: Value = serde_json::from_str(&line).unwrap();
            line.clear();
            let reply = match answer(&request) {
                Ok(result) => json!({"id":request["id"],"result":result}),
                Err(error) => json!({"id":request["id"],"error":error}),
            };
            seen.lock().unwrap().push(request);
            let frame = format!("{reply}\n");
            if server_writer.write_all(frame.as_bytes()).await.is_err() {
                return;
            }
        }
    });
    let initialize = crate::workbench::codex::InitializeResult {
        user_agent: "codex_cli_rs/0.147.0".to_string(),
        platform_family: "unix".to_string(),
        platform_os: "macos".to_string(),
        codex_home: root.to_string(),
    };
    let client = crate::workbench::codex::AppServerClient::from_io(8, client_reader, client_writer);
    (
        crate::workbench::codex::AppServerSupervisor::from_test_client(8, client, initialize),
        requests,
    )
}

fn methods(requests: &StdMutex<Vec<Value>>) -> Vec<String> {
    requests
        .lock()
        .unwrap()
        .iter()
        .map(|request| request["method"].as_str().unwrap().to_string())
        .collect()
}

/// A conversation whose Codex thread saw one message and holds `turn` open.
fn codex_conversation(store: &Store, label: &str, turn: Option<&str>) -> (String, SessionBinding) {
    let session = store
        .create_session(crate::workbench::store::CreateSessionRequest {
            workspace_id: None,
            title: "Saved thread".into(),
            operation_id: format!("codex-conversation-{label}"),
        })
        .unwrap()
        .record
        .id;
    let thread = format!("thread-{label}");
    let binding = store
        .bind_session(&session, "codex-0-147-0-epoch-1", &thread)
        .unwrap();
    store
        .project_codex_event(
            &binding.id,
            &NormalizedEvent::ItemCompleted {
                epoch: 1,
                thread_id: thread,
                turn_id: format!("turn-{label}-first"),
                item_id: format!("item-{label}"),
                item_kind: "userMessage".into(),
                item: json!({"text":"The calibration is 0.95"}),
            },
        )
        .unwrap();
    if let Some(turn) = turn {
        store
            .record_turn_submission(&binding.id, &format!("submission-{label}"), turn)
            .unwrap();
    }
    (session, binding)
}

fn send_request(session: &str) -> SendTurnRequest {
    SendTurnRequest {
        session_id: session.into(),
        text: "Next question".into(),
        client_submission_id: "submission-next".into(),
        model: None,
        effort: None,
    }
}

/// Echoes the requested folder and profile, as a healthy `thread/start` does.
fn started_thread(request: &Value) -> Value {
    let params = &request["params"];
    json!({
        "thread":{"id":"thread-successor","sessionId":"provider-session"},
        "model":"gpt-test","modelProvider":"openai",
        "activePermissionProfile":{"id":params["permissions"]},
        "approvalPolicy":"untrusted","approvalsReviewer":"user",
        "cwd":params["cwd"],
        "instructionSources":[],
        "runtimeWorkspaceRoots":params["runtimeWorkspaceRoots"]
    })
}

#[tokio::test]
async fn a_thread_codex_holds_no_history_for_moves_to_one_seeded_successor() {
    let store = crate::orchestration::tests::disposable_workspace();
    let (session, binding) = codex_conversation(&store, "no-rollout", None);
    let prepared = crate::workbench::research::prepare_turn(&store, &session).unwrap();
    store
        .set_binding_harness(&binding.id, &prepared.effective.fingerprint, &[])
        .unwrap();
    let binding = store.active_binding(&session).unwrap().unwrap();
    let root = store.runtime_root(&session).unwrap();
    let root = root.to_string_lossy().into_owned();
    assert!(crate::workbench::history::codex_context(
        &store,
        &session,
        Some(&binding),
        &prepared.effective.fingerprint
    )
    .unwrap()
    .is_none());

    let (supervisor, requests) =
        fake_app_server(&root, |request| match request["method"].as_str() {
            Some("thread/resume") => Err(
                json!({"code":-32600,"message":"no rollout found for thread id thread-no-rollout"}),
            ),
            _ => Ok(started_thread(request)),
        });
    let connection = open_thread(
        &supervisor,
        &send_request(&session),
        Some(binding),
        None,
        &prepared,
        root,
    )
    .await
    .unwrap();

    assert_eq!(connection.thread_id, "thread-successor");
    assert_eq!(methods(&requests), ["thread/resume", "thread/start"]);
    let requests = requests.lock().unwrap();
    assert!(requests[1]["params"]["developerInstructions"]
        .as_str()
        .unwrap()
        .contains("The calibration is 0.95"));
}

#[tokio::test]
async fn a_resume_that_fails_for_any_other_reason_never_starts_a_successor() {
    let store = crate::orchestration::tests::disposable_workspace();
    let (session, binding) = codex_conversation(&store, "resume-busy", None);
    let prepared = crate::workbench::research::prepare_turn(&store, &session).unwrap();
    let root = store.runtime_root(&session).unwrap();
    let root = root.to_string_lossy().into_owned();
    let (supervisor, requests) = fake_app_server(&root, |_| {
        Err(json!({"code":-32603,"message":"The server is overloaded"}))
    });
    let error = open_thread(
        &supervisor,
        &send_request(&session),
        Some(binding),
        None,
        &prepared,
        root,
    )
    .await
    .unwrap_err();
    assert!(error.message.contains("overloaded"));
    assert_eq!(methods(&requests), ["thread/resume"]);
}

fn turn_state(store: &Store, session: &str, turn: &str) -> (String, bool) {
    let snapshot = store.conversation_snapshot(session).unwrap();
    let record = snapshot
        .turns
        .iter()
        .find(|record| record.provider_turn_id.as_deref() == Some(turn))
        .unwrap();
    (record.state.clone(), record.terminal_at.is_some())
}

#[tokio::test]
async fn an_open_turn_nothing_owns_closes_under_any_harness_or_selected_connection() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let store = crate::orchestration::tests::disposable_workspace();

    // No harness in common and no reachable thread, as after an update.
    let (session, binding) = codex_conversation(&store, "stale", Some("turn-stale"));
    store
        .set_binding_harness(&binding.id, &"a".repeat(64), &[])
        .unwrap();
    close_abandoned_turns(None, binding).await.unwrap();
    assert_eq!(
        turn_state(&store, &session, "turn-stale"),
        ("interrupted".to_string(), true)
    );
    let snapshot = store.conversation_snapshot(&session).unwrap();
    assert!(snapshot.turns.iter().all(|turn| turn.terminal_at.is_some()));

    // Reachable history that no longer lists the turn.
    let (session, binding) = codex_conversation(&store, "absent", Some("turn-absent"));
    let (supervisor, requests) = fake_app_server("/", |_| Ok(json!({"data":[],"nextCursor":null})));
    close_abandoned_turns(Some(&supervisor), binding)
        .await
        .unwrap();
    assert_eq!(methods(&requests), ["thread/turns/list"]);
    assert_eq!(
        turn_state(&store, &session, "turn-absent"),
        ("interrupted".to_string(), true)
    );

    // Reachable history that recorded a real ending keeps that ending.
    let (session, binding) = codex_conversation(&store, "ended", Some("turn-ended"));
    let (supervisor, _) = fake_app_server("/", |_| {
        Ok(json!({"data":[{"id":"turn-ended","status":"completed","items":[]}],"nextCursor":null}))
    });
    close_abandoned_turns(Some(&supervisor), binding)
        .await
        .unwrap();
    assert_eq!(
        turn_state(&store, &session, "turn-ended"),
        ("completed".to_string(), true)
    );
}

#[tokio::test]
async fn an_open_turn_with_a_live_owner_is_left_running() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let store = crate::orchestration::tests::disposable_workspace();
    let (session, binding) = codex_conversation(&store, "owned", Some("turn-owned"));
    let thread = binding.provider_thread_id.clone();
    accept_local_turn(11, &thread, "turn-owned");
    close_abandoned_turns(None, binding.clone()).await.unwrap();
    assert!(!turn_state(&store, &session, "turn-owned").1);
    finish_local_turn(11, &thread, "turn-owned");
    close_abandoned_turns(None, binding).await.unwrap();
    assert!(turn_state(&store, &session, "turn-owned").1);
}

fn draft_fixture(draft: &str) -> (tempfile::TempDir, Store, String) {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(&temp.path().join("store")).unwrap();
    let session = store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "Draft".into(),
            operation_id: "draft-session".into(),
        })
        .unwrap()
        .record
        .id;
    set_draft(&store, &session, draft, "draft-first");
    (temp, store, session)
}

fn set_draft(store: &Store, session: &str, draft: &str, operation: &str) {
    let revision = store.session_snapshot(session).unwrap().session.revision;
    store
        .update_session(UpdateSessionRequest {
            session_id: session.to_string(),
            expected_revision: revision,
            operation_id: operation.into(),
            title: None,
            draft: Some(draft.into()),
            overrides: None,
            archived: None,
            preset_id: None,
            paper_id: None,
            clear_paper: None,
        })
        .unwrap();
}

#[test]
fn a_sent_message_is_cleared_but_a_newer_draft_survives_the_acknowledgement() {
    let (_temp, store, session) = draft_fixture("Sent message");
    assert!(clear_sent_draft(&store, &session, "Sent message\n", "one").unwrap());
    assert_eq!(store.session_snapshot(&session).unwrap().session.draft, "");

    let (_temp, store, session) = draft_fixture("Sent message");
    set_draft(&store, &session, "Typed while sending", "draft-newer");
    assert!(!clear_sent_draft(&store, &session, "Sent message", "two").unwrap());
    assert_eq!(
        store.session_snapshot(&session).unwrap().session.draft,
        "Typed while sending"
    );
}

#[test]
fn a_revision_conflict_on_the_acknowledgement_gets_one_more_attempt() {
    let (_temp, store, session) = draft_fixture("Sent message");
    let mut attempts = 0;
    let cleared = clear_sent_draft_with(&store, &session, "Sent message", "one", |request| {
        attempts += 1;
        if attempts == 1 {
            let title_write = store.session_snapshot(&session).unwrap().session.revision;
            store
                .update_session(UpdateSessionRequest {
                    session_id: session.clone(),
                    expected_revision: title_write,
                    operation_id: "title-write".into(),
                    title: Some("A new title".into()),
                    draft: None,
                    overrides: None,
                    archived: None,
                    preset_id: None,
                    paper_id: None,
                    clear_paper: None,
                })
                .unwrap();
        }
        store.update_session(request).map(|_| ())
    })
    .unwrap();
    assert!(cleared);
    assert_eq!(attempts, 2);
    assert_eq!(store.session_snapshot(&session).unwrap().session.draft, "");

    let (_temp, store, session) = draft_fixture("Sent message");
    let mut attempts = 0;
    let repeated = clear_sent_draft_with(&store, &session, "Sent message", "two", |_| {
        attempts += 1;
        Err(WorkbenchError::conflict("Another write came first"))
    });
    assert_eq!(repeated.unwrap_err().code, "storage_conflict");
    assert_eq!(attempts, 2);
}

#[tokio::test]
async fn a_running_response_refuses_delete_and_move_in_conversation_terms() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let _permit = turn_queue().clone().acquire_owned().await.unwrap();
    let delete = workbench_delete_session(DeleteSessionRequest {
        session_id: "any-conversation".into(),
        operation_id: "delete-refused".into(),
    })
    .await
    .unwrap_err();
    let moved = workbench_move_session(MoveSessionRequest {
        session_id: "any-conversation".into(),
        expected_revision: 1,
        workspace_id: None,
        operation_id: "move-refused".into(),
    })
    .await
    .unwrap_err();
    for error in [delete, moved] {
        assert_eq!(error.code, "storage_conflict");
        assert!(error.message.contains("response is still running"));
        assert!(!error.message.contains("private storage"));
    }
}

fn bridge_client(
    epoch: u64,
) -> (
    crate::workbench::codex::AppServerClient,
    tokio::io::DuplexStream,
) {
    let (client_io, server_io) = tokio::io::duplex(1024 * 1024);
    let (reader, writer) = tokio::io::split(client_io);
    (
        crate::workbench::codex::AppServerClient::from_io(epoch, reader, writer),
        server_io,
    )
}

fn bridge_source(client: &crate::workbench::codex::AppServerClient) -> BridgeEvents {
    BridgeEvents {
        events: client.subscribe(),
        requests: client.connection_request_sink(),
        ended: false,
    }
}

#[tokio::test]
async fn the_event_bridge_ends_with_its_connection() {
    let (client, _server) = bridge_client(21);
    let mut source = bridge_source(&client);
    let held = client.clone();
    let bridge = tokio::spawn(async move {
        let _connection = held;
        let mut closed = 0;
        while let Some(received) = source.next().await {
            if matches!(received, Ok(NormalizedEvent::ConnectionClosed { .. })) {
                closed += 1;
            }
        }
        closed
    });
    client.fail("The connection ended", true);
    let closed = tokio::time::timeout(std::time::Duration::from_secs(5), bridge)
        .await
        .expect("the bridge must finish after its connection closes")
        .unwrap();
    assert_eq!(closed, 1);
}

#[tokio::test]
async fn an_approval_inside_a_lagged_stretch_still_reaches_the_bridge() {
    use tokio::io::AsyncWriteExt as _;
    let (client, mut server) = bridge_client(22);
    let mut source = bridge_source(&client);
    let approval = json!({"id":"approval-1","method":"item/commandExecution/requestApproval","params":{"threadId":"thread-1","turnId":"turn-1"}});
    server
        .write_all(format!("{approval}\n").as_bytes())
        .await
        .unwrap();
    for index in 0..4200 {
        let frame = json!({"method":"simulator/progress","params":{"index":index}});
        server
            .write_all(format!("{frame}\n").as_bytes())
            .await
            .unwrap();
    }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    match source.next().await {
        Some(Ok(NormalizedEvent::ServerRequest { request_id, .. })) => {
            assert_eq!(request_id, json!("approval-1"))
        }
        other => panic!("expected the approval first, got {other:?}"),
    }
    assert!(matches!(
        source.next().await,
        Some(Err(tokio::sync::broadcast::error::RecvError::Lagged(_)))
    ));
}

#[tokio::test]
async fn a_request_no_screen_can_answer_is_refused_at_once() {
    let (supervisor, requests) = fake_app_server("/fixture", |_| Ok(json!({})));
    let request = |method: &str| NormalizedEvent::ServerRequest {
        epoch: 8,
        request_id: json!(method),
        method: method.into(),
        params: json!({"threadId":"thread-1","turnId":"turn-1"}),
    };
    for method in [
        "mcpServer/elicitation/request",
        "attestation/generate",
        "currentTime/read",
        "applyPatchApproval",
        "execCommandApproval",
    ] {
        assert!(refuse_unsupported_request(&supervisor, &request(method)).await);
    }
    for method in [
        "item/commandExecution/requestApproval",
        "item/fileChange/requestApproval",
        "item/tool/requestUserInput",
        "item/permissions/requestApproval",
    ] {
        assert!(!refuse_unsupported_request(&supervisor, &request(method)).await);
    }
    let refused = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let seen = requests.lock().unwrap().clone();
            if seen.len() == 5 {
                return seen;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    for frame in refused {
        assert!(frame["result"].is_null());
        let method = frame["id"].as_str().unwrap();
        assert!(frame["error"]["message"].as_str().unwrap().contains(method));
    }
}

#[tokio::test]
async fn exit_interrupts_the_running_turn_and_stops_app_server() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let (supervisor, requests) = fake_app_server("/fixture", |request| {
        if request["method"] == "turn/interrupt" {
            complete_active_turn(8, "thread-exit", "turn-exit");
        }
        Ok(json!({}))
    });
    let supervisor = Arc::new(supervisor);
    *active_turn_state().lock().unwrap() = ActiveTurnState {
        epoch: Some(8),
        thread_id: Some("thread-exit".into()),
        turn_id: Some("turn-exit".into()),
        ..Default::default()
    };
    let manager = crate::workbench::codex::SupervisorManager::default();
    manager.install(supervisor.clone()).await;
    stop_codex(&manager).await;
    assert_eq!(methods(&requests), ["turn/interrupt"]);
    assert!(!supervisor.status().connected);
    assert!(manager.current().await.is_none());
    assert!(!turn_has_live_owner("thread-exit", "turn-exit"));
}
