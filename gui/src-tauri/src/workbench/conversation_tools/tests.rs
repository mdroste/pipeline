use super::*;
use crate::workbench::{
    research,
    store::{CreateSessionRequest, UpdateSessionRequest},
};

fn fixture() -> (tempfile::TempDir, Store, String, Value) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_at(dir.path()).unwrap();
    let session = store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "Web".into(),
            operation_id: "create".into(),
        })
        .unwrap()
        .record;
    store
        .update_session(UpdateSessionRequest {
            session_id: session.id.clone(),
            expected_revision: session.revision,
            operation_id: "access".into(),
            title: None,
            draft: None,
            overrides: Some(json!({"webSearch":true})),
            archived: None,
            preset_id: None,
            paper_id: None,
            clear_paper: None,
        })
        .unwrap();
    let prepared = research::prepare_turn(&store, &session.id).unwrap();
    let binding = store.bind_session(&session.id, "test", "thread").unwrap();
    store
        .record_turn_submission(&binding.id, "submission", "turn")
        .unwrap();
    store
        .attach_turn_snapshots(
            &binding.id,
            "submission",
            &prepared.config_snapshot_id,
            &prepared.context_snapshot_id,
        )
        .unwrap();
    let call = json!({"threadId":"thread","turnId":"turn","callId":"one","tool":"web_fetch","arguments":{"url":"https://example.org"}});
    (dir, store, session.id, call)
}

#[test]
fn receipts_reject_reuse_and_never_repeat_unknown_outcomes() {
    let (_dir, store, _session, call) = fixture();
    assert!(matches!(prepare(&store, &call).unwrap(), Admission::Run(_)));
    let Admission::Recorded(previous) = prepare(&store, &call).unwrap() else {
        panic!("Repeated a side effect");
    };
    assert_eq!(previous["success"], false);
    let mut changed = call.clone();
    changed["arguments"]["url"] = json!("https://example.com");
    assert!(prepare(&store, &changed).is_err());
    let mut foreign = call.clone();
    foreign["turnId"] = json!("other");
    assert!(prepare(&store, &foreign).is_err());
    store
        .connection()
        .unwrap()
        .execute("UPDATE turns SET terminal_at='done'", [])
        .unwrap();
    assert!(prepare(&store, &call).is_err());
}

#[test]
fn current_access_and_captured_catalog_both_gate_calls() {
    let (_dir, store, session, call) = fixture();
    let mut unavailable = call.clone();
    unavailable["tool"] = json!("browser");
    assert!(prepare(&store, &unavailable).is_err());
    let Admission::Run(context) = prepare(&store, &call).unwrap() else {
        panic!()
    };
    store
        .connection()
        .unwrap()
        .execute(
            "UPDATE sessions SET overrides_json='{}' WHERE id=?1",
            [session],
        )
        .unwrap();
    assert!(active(&store, &context).is_err());
    assert!(prepare(&store, &call).is_err());
}

#[test]
fn unfiled_conversations_never_receive_project_write_tools() {
    let (_dir, store, session, _call) = fixture();
    let effective = research::resolve_harness(&store, &session).unwrap();
    assert!(effective
        .dynamic_tools
        .iter()
        .any(|v| v["name"] == "web_fetch"));
    assert!(!effective
        .dynamic_tools
        .iter()
        .any(|v| v["name"] == "web_download" || v["name"] == "project_create"));
}
