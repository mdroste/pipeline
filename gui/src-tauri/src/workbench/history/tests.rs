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
