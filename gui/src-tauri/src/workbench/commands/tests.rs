use super::*;

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
    assert_eq!(DATABASE_WORKERS.available_permits(), 4);
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
    let pending = crate::workbench::codex::NormalizedEvent::ServerRequest {
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
fn lag_recovery_requires_the_exact_native_thread_and_terminal_turn() {
    let snapshot = json!({"thread":{"id":"thread","turns":[
        {"id":"old","status":"completed"}, {"id":"current","status":"inProgress"}
    ]}});
    assert!(codex::recovered_terminal(&snapshot, "thread", "current").is_none());
    assert!(codex::recovered_terminal(&snapshot, "other-thread", "old").is_none());
    let complete =
        json!({"thread":{"id":"thread","turns":[{"id":"current","status":"completed"}]}});
    assert!(codex::recovered_terminal(&complete, "thread", "current").is_some());
}
