use super::*;
use crate::{
    agent_runtime::events::NormalizedEvent,
    workbench::{research, store::*},
};

fn fixture() -> (tempfile::TempDir, Store, String) {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(&temp.path().join("store")).unwrap();
    let id = store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "History".into(),
            operation_id: "create".into(),
        })
        .unwrap()
        .record
        .id;
    let binding = store
        .bind_session(&id, "codex-0-147-0-epoch-1", "thread")
        .unwrap();
    let fingerprint = research::resolve_harness(&store, &id).unwrap().fingerprint;
    store
        .set_binding_harness(&binding.id, &fingerprint, &[])
        .unwrap();
    for (item_id, kind, text) in [
        ("user", "userMessage", "Remember the calibration is 0.95"),
        ("answer", "agentMessage", "I will use 0.95"),
    ] {
        store
            .project_codex_event(
                &binding.id,
                &NormalizedEvent::ItemCompleted {
                    epoch: 1,
                    thread_id: "thread".into(),
                    turn_id: "turn".into(),
                    item_id: item_id.into(),
                    item_kind: kind.into(),
                    item: json!({"text":text}),
                },
            )
            .unwrap();
    }
    (temp, store, id)
}

fn context(store: &Store, session: &str) -> Option<String> {
    codex_context(
        store,
        session,
        store.active_binding(session).unwrap().as_ref(),
        &research::resolve_harness(store, session)
            .unwrap()
            .fingerprint,
    )
    .unwrap()
}

#[test]
fn changed_settings_start_with_history_but_matching_resume_does_not_repeat_it() {
    let (_temp, store, id) = fixture();
    assert!(context(&store, &id).is_none());
    let before = store.session_snapshot(&id).unwrap().session;
    store.update_session(serde_json::from_value(json!({"sessionId":id,"expectedRevision":before.revision,"operationId":"change-settings","overrides":{"webSearch":true}})).unwrap()).unwrap();
    let transferred = context(&store, &id).unwrap();
    assert!(transferred.contains("Remember the calibration is 0.95"));
    assert!(transferred.contains("I will use 0.95"));
}

#[test]
fn retired_bindings_and_project_moves_keep_app_owned_history() {
    for retired in [false, true] {
        let (_temp, store, id) = fixture();
        if retired {
            // Restored stores retain transcript rows but retire native bindings.
            store
                .connection()
                .unwrap()
                .execute("UPDATE session_bindings SET retired_at='restored'", [])
                .unwrap();
            assert!(store.active_binding(&id).unwrap().is_none());
        } else {
            let ws = store
                .create_workspace(CreateWorkspaceRequest {
                    name: "Other project".into(),
                    root: None,
                    operation_id: "project".into(),
                })
                .unwrap()
                .record;
            store
                .move_session(MoveSessionRequest {
                    session_id: id.clone(),
                    expected_revision: store.session_snapshot(&id).unwrap().session.revision,
                    workspace_id: Some(ws.id),
                    operation_id: "move".into(),
                })
                .unwrap();
        }
        assert!(context(&store, &id)
            .unwrap()
            .contains("calibration is 0.95"));
    }
}

#[test]
fn provider_switches_use_history_and_new_sessions_have_an_empty_history() {
    let (_temp, store, id) = fixture();
    for runtime in [
        crate::workbench::claude::NAMESPACE,
        crate::workbench::api::NAMESPACE,
    ] {
        store.bind_session(&id, runtime, runtime).unwrap();
        assert!(context(&store, &id)
            .unwrap()
            .contains("calibration is 0.95"));
    }
    let fresh = store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "New".into(),
            operation_id: "fresh".into(),
        })
        .unwrap()
        .record
        .id;
    let empty = context(&store, &fresh).unwrap();
    assert!(empty.contains("\n[]\n"));
    assert!(!empty.contains("0.95"));
}

fn project(store: &Store, binding: &str, item_id: &str, kind: &str, item: Value) {
    project_on(store, binding, "thread", item_id, kind, item);
}

fn project_on(store: &Store, binding: &str, thread: &str, item_id: &str, kind: &str, item: Value) {
    store
        .project_codex_event(
            binding,
            &NormalizedEvent::ItemCompleted {
                epoch: 1,
                thread_id: thread.into(),
                turn_id: "later-turn".into(),
                item_id: item_id.into(),
                item_kind: kind.into(),
                item,
            },
        )
        .unwrap();
}

fn binding_id(store: &Store, session: &str) -> String {
    store.active_binding(session).unwrap().unwrap().id
}

fn exchange(store: &Store, binding: &str, name: &str, text: &str) {
    project(
        store,
        binding,
        &format!("{name}-user"),
        "userMessage",
        json!({ "text": text }),
    );
    project(
        store,
        binding,
        &format!("{name}-answer"),
        "agentMessage",
        json!({"text":format!("Answer to {name}")}),
    );
}

#[test]
fn a_long_run_of_tool_items_does_not_hide_earlier_exchanges() {
    let (_temp, store, id) = fixture();
    let binding = binding_id(&store, &id);
    for index in 0..5 {
        exchange(
            &store,
            &binding,
            &format!("question-{index}"),
            &format!("Question number {index}"),
        );
    }
    for index in 0..300 {
        project(
            &store,
            &binding,
            &format!("tool-{index}"),
            "toolCall",
            json!({"id":index,"tool":"project_read","output":"contents"}),
        );
    }
    let history = recent_context(&store, &id).unwrap();
    for index in 0..5 {
        assert!(history.contains(&format!("Question number {index}")));
        assert!(history.contains(&format!("Answer to question-{index}")));
    }
    assert!(history.contains("Remember the calibration is 0.95"));
    assert!(history.ends_with("History truncated: false"));
}

#[test]
fn an_oversized_message_is_skipped_and_the_older_exchanges_stay() {
    let (_temp, store, id) = fixture();
    let binding = binding_id(&store, &id);
    exchange(
        &store,
        &binding,
        "pasted",
        &"x".repeat(MAX_HISTORY_BYTES + 1),
    );
    let history = recent_context(&store, &id).unwrap();
    assert!(!history.contains("xxxx"));
    assert!(history.contains("Answer to pasted"));
    assert!(history.contains("Remember the calibration is 0.95"));
    assert!(history.ends_with("History truncated: true"));
}

#[test]
fn the_budget_ends_the_walk_and_reports_the_messages_left_out() {
    let (_temp, store, id) = fixture();
    let binding = binding_id(&store, &id);
    // Each message is a sixth of the budget, so the oldest cannot all fit.
    for index in 0..8 {
        exchange(
            &store,
            &binding,
            &format!("large-{index}"),
            &format!("{index}{}", "y".repeat(MAX_HISTORY_BYTES / 6)),
        );
    }
    let history = recent_context(&store, &id).unwrap();
    let body = history.split('\n').nth(1).unwrap();
    let kept: Vec<Value> = serde_json::from_str(body).unwrap();
    let bytes: usize = kept.iter().map(|m| m["text"].as_str().unwrap().len()).sum();
    assert!(bytes <= MAX_HISTORY_BYTES);
    assert!(kept.len() < 18);
    assert!(history.ends_with("History truncated: true"));
}

#[test]
fn a_message_replayed_on_a_successor_binding_appears_once() {
    let (_temp, store, id) = fixture();
    let first = binding_id(&store, &id);
    exchange(&store, &first, "shared", "The replayed question");
    let successor = store
        .bind_session(&id, "codex-0-147-0-epoch-2", "next")
        .unwrap();
    assert_ne!(first, successor.id);
    project_on(
        &store,
        &successor.id,
        "next",
        "shared-user",
        "userMessage",
        json!({"text":"The replayed question"}),
    );
    let history = recent_context(&store, &id).unwrap();
    assert_eq!(history.matches("The replayed question").count(), 1);
    assert!(history.ends_with("History truncated: false"));
}

#[test]
fn reopening_a_thread_on_a_new_connection_keeps_it_resumable() {
    let (_temp, store, id) = fixture();
    let first = store.active_binding(&id).unwrap().unwrap();
    let fingerprint = first.harness_fingerprint.clone().unwrap();
    store
        .set_binding_harness(&first.id, &fingerprint, &["AGENTS.md".to_string()])
        .unwrap();

    let reopened = store
        .bind_session(&id, "codex-0-147-0-epoch-2", "thread")
        .unwrap();
    assert_ne!(reopened.id, first.id);
    assert_eq!(reopened.harness_fingerprint.as_deref(), Some(&*fingerprint));
    assert_eq!(reopened.instruction_sources, ["AGENTS.md"]);
    assert!(codex_context(&store, &id, Some(&reopened), &fingerprint)
        .unwrap()
        .is_none());

    // A different thread starts from nothing and needs the app's history.
    let successor = store
        .bind_session(&id, "codex-0-147-0-epoch-2", "thread-2")
        .unwrap();
    assert_eq!(successor.harness_fingerprint, None);
    assert!(successor.instruction_sources.is_empty());
    assert!(codex_context(&store, &id, Some(&successor), &fingerprint)
        .unwrap()
        .is_some());
}
