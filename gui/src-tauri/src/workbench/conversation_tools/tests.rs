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

/// A project conversation mid-turn, with the tools its access offers.
fn project_turn(overrides: Value) -> (tempfile::TempDir, Store, String, String, Value) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let store = Store::open_at(&dir.path().join("store")).unwrap();
    let workspace = store
        .create_workspace(crate::workbench::store::CreateWorkspaceRequest {
            name: "Wages".into(),
            root: Some(root.to_string_lossy().into_owned()),
            operation_id: "workspace".into(),
        })
        .unwrap()
        .record
        .id;
    let session = store
        .create_session(CreateSessionRequest {
            workspace_id: Some(workspace.clone()),
            title: "Long turn".into(),
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
            overrides: Some(overrides),
            archived: None,
            preset_id: Some("research_assistant".into()),
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
    let call = json!({"threadId":"thread","turnId":"turn","callId":"one","tool":"project_files","arguments":{}});
    (dir, store, workspace, session.id, call)
}

#[test]
fn project_activity_during_a_turn_leaves_its_tools_available() {
    let (_dir, store, workspace, session, call) = project_turn(json!({"webSearch":true}));
    let before = research::resolve_harness(&store, &session).unwrap();
    let Admission::Run(context) = prepare(&store, &call).unwrap() else {
        panic!("The turn's first tool was refused");
    };
    research::create_note(
        &store,
        research::CreateNoteRequest {
            workspace_id: workspace,
            paper_id: None,
            kind: "decision".into(),
            body: "Cluster by state".into(),
            state: Some("accepted".into()),
            origin: "user".into(),
            pinned: true,
            operation_id: "note".into(),
        },
        false,
    )
    .unwrap();
    let after = research::resolve_harness(&store, &session).unwrap();
    assert_ne!(before.context_preview, after.context_preview);
    assert_ne!(before.fingerprint, after.fingerprint);
    // A tool already running keeps going, and later tools still start.
    active(&store, &context).unwrap();
    let mut next = call.clone();
    next["callId"] = json!("two");
    next["tool"] = json!("web_fetch");
    next["arguments"] = json!({"url":"https://example.org"});
    assert!(matches!(prepare(&store, &next).unwrap(), Admission::Run(_)));
}

#[test]
fn the_same_setting_moving_scope_leaves_tools_available() {
    let (_dir, store, _workspace, session, call) = project_turn(json!({"webSearch":true}));
    let before = research::resolve_harness(&store, &session).unwrap();
    let saved = research::load_config(&store, None).unwrap();
    research::save_config(
        &store,
        research::SaveWorkspaceConfigRequest {
            workspace_id: None,
            expected_revision: (saved.revision > 0).then_some(saved.revision),
            operation_id: "global-web".into(),
            body: json!({"webSearch":true}),
        },
    )
    .unwrap();
    let revision = store.session_snapshot(&session).unwrap().session.revision;
    store
        .update_session(UpdateSessionRequest {
            session_id: session.clone(),
            expected_revision: revision,
            operation_id: "clear-override".into(),
            title: None,
            draft: None,
            overrides: Some(json!({})),
            archived: None,
            preset_id: None,
            paper_id: None,
            clear_paper: None,
        })
        .unwrap();
    let after = research::resolve_harness(&store, &session).unwrap();
    assert!(after.web_search);
    assert_ne!(before.fingerprint, after.fingerprint);
    assert!(matches!(prepare(&store, &call).unwrap(), Admission::Run(_)));
}

#[test]
fn a_real_access_change_still_stops_the_tools() {
    for (name, overrides) in [
        ("mode", json!({"webSearch":true,"mode":"edit"})),
        ("network", json!({"webSearch":true,"commandNetwork":true})),
        ("browser", json!({"webSearch":true,"browser":true})),
        ("file mode", json!({"webSearch":true,"fileMode":"direct"})),
        (
            "domains",
            json!({"webSearch":true,"commandDomains":"example.org"}),
        ),
    ] {
        let (_dir, store, _workspace, session, call) = project_turn(json!({"webSearch":true}));
        let Admission::Run(context) = prepare(&store, &call).unwrap() else {
            panic!()
        };
        store
            .connection()
            .unwrap()
            .execute(
                "UPDATE sessions SET overrides_json=?2 WHERE id=?1",
                params![session, overrides.to_string()],
            )
            .unwrap();
        assert!(active(&store, &context).is_err(), "{name}");
        let mut next = call.clone();
        next["callId"] = json!("two");
        assert!(prepare(&store, &next).is_err(), "{name}");
    }
}

#[test]
fn moving_to_another_project_or_working_copy_stops_the_tools() {
    let (_dir, store, _workspace, session, call) = project_turn(json!({"webSearch":true}));
    let Admission::Run(context) = prepare(&store, &call).unwrap() else {
        panic!()
    };
    store
        .connection()
        .unwrap()
        .execute(
            "UPDATE sessions SET overrides_json=?2 WHERE id=?1",
            params![
                session,
                json!({"webSearch":true,"projectCheckpointId":"another-copy"}).to_string()
            ],
        )
        .unwrap();
    assert!(active(&store, &context).is_err());
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

#[tokio::test]
async fn a_closed_connection_stops_only_what_it_owns() {
    use crate::workbench::commands::{accept_local_turn, finish_local_turn, stop_connection_tools};
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let watching = |thread: &str, turn: &str| {
        cancellations()
            .lock()
            .unwrap()
            .entry((thread.into(), turn.into()))
            .or_insert_with(|| watch::channel(false).0)
            .subscribe()
    };
    let (closed, claude, newer) = (500, 501, 502);

    let claude_tool = watching("claude-thread", "claude-turn");
    accept_local_turn(claude, "claude-thread", "claude-turn");
    stop_connection_tools(closed).await;
    assert!(
        !*claude_tool.borrow(),
        "another runtime's turn keeps its tool"
    );
    finish_local_turn(claude, "claude-thread", "claude-turn");

    accept_local_turn(newer, "chatgpt-thread", "newer-turn");
    stop_connection_tools(closed).await;
    assert!(
        !*claude_tool.borrow(),
        "a newer connection's turn is left alone"
    );
    finish_local_turn(newer, "chatgpt-thread", "newer-turn");

    accept_local_turn(closed, "chatgpt-thread", "own-turn");
    let own_tool = watching("chatgpt-thread", "own-turn");
    stop_connection_tools(closed).await;
    assert!(*own_tool.borrow(), "the connection's own turn stops");
    finish_local_turn(closed, "chatgpt-thread", "own-turn");

    let left_over = watching("chatgpt-thread", "finished-turn");
    stop_connection_tools(closed).await;
    assert!(
        *left_over.borrow(),
        "nothing is running, so nothing is spared"
    );
}
