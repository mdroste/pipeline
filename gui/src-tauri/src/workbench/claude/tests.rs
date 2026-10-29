use super::super::store::{CreateSessionRequest, CreateWorkspaceRequest, UpdateSessionRequest};
use super::*;

struct Fixture {
    _temp: tempfile::TempDir,
    store: Store,
    root: std::path::PathBuf,
    session: String,
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let store = Store::open_at(&temp.path().join("store")).unwrap();
    let workspace = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Claude test".into(),
            root: Some(root.to_string_lossy().into()),
            operation_id: "workspace".into(),
        })
        .unwrap()
        .record;
    let session = store
        .create_session(CreateSessionRequest {
            workspace_id: Some(workspace.id),
            title: "Test".into(),
            operation_id: "session".into(),
        })
        .unwrap()
        .record
        .id;
    Fixture {
        _temp: temp,
        store,
        root,
        session,
    }
}

fn select(f: &Fixture, mode: &str) {
    let session = f.store.session_snapshot(&f.session).unwrap().session;
    f.store
        .update_session(UpdateSessionRequest {
            session_id: session.id,
            expected_revision: session.revision,
            operation_id: format!("select-{mode}"),
            title: None,
            draft: None,
            overrides: Some(json!({"runtime":"claude","mode":mode})),
            archived: None,
            preset_id: None,
            paper_id: None,
            clear_paper: None,
        })
        .unwrap();
}

#[test]
fn claude_selection_is_explicit_and_rejects_unknown_values() {
    assert!(!selected(&json!({})).unwrap());
    assert!(!selected(&json!({"runtime":"codex"})).unwrap());
    assert!(selected(&json!({"runtime":"claude"})).unwrap());
    assert!(selected(&json!({"runtime":"other"})).is_err());
    assert!(selected(&json!({"runtime":false})).is_err());
}

#[test]
fn claude_recovery_keeps_history_and_interrupts_without_replay() {
    let f = fixture();
    select(&f, "inspect");
    let binding = f
        .store
        .bind_session(&f.session, NAMESPACE, "claude-test")
        .unwrap();
    f.store
        .record_turn_submission(&binding.id, "submission", "turn")
        .unwrap();
    for (id, kind, payload) in [
        (
            "user",
            "userMessage",
            json!({"content":[{"type":"inputText","text":"Prior question"}]}),
        ),
        (
            "assistant",
            "agentMessage",
            json!({"text":"Prior answer","runtime":"claude"}),
        ),
    ] {
        f.store
            .project_codex_event(
                &binding.id,
                &NormalizedEvent::ItemCompleted {
                    epoch: protocol::next_epoch(),
                    thread_id: "claude-test".into(),
                    turn_id: "turn".into(),
                    item_id: id.into(),
                    item_kind: kind.into(),
                    item: payload,
                },
            )
            .unwrap();
    }
    let prompt = protocol::prompt(&f.store, &f.session, "Next question").unwrap();
    assert!(prompt.text.contains("Prior question"));
    assert!(prompt.text.contains("Prior answer"));
    assert_eq!(prompt.user_text, "Next question");
    assert!(protocol::prompt(&f.store, &f.session, " ").is_err());
    reconcile(f.store.clone(), f.session.clone()).unwrap();
    reconcile(f.store.clone(), f.session.clone()).unwrap();
    let snapshot = f.store.conversation_snapshot(&f.session).unwrap();
    assert_eq!(snapshot.turns.len(), 1);
    assert_eq!(snapshot.turns[0].state, "interrupted");
    assert_eq!(snapshot.items.len(), 2);
}

#[test]
fn claude_project_reads_and_edits_obey_scope_mode_and_content_hash() {
    let f = fixture();
    std::fs::write(f.root.join("paper.md"), "Original").unwrap();
    let call = |name, input: Value, operation| {
        super::super::project::assistant_files::call(&f.store, &f.session, name, &input, operation)
    };
    select(&f, "inspect");
    let read = call("project_read", json!({"path":"paper.md"}), "read").unwrap();
    assert_eq!(read["text"], "Original");
    assert!(call(
        "project_read",
        json!({"path":"../outside.txt"}),
        "traversal"
    )
    .is_err());
    assert!(call("project_read", json!({"path":".git/config"}), "metadata").is_err());
    let edit = json!({"path":"paper.md","expectedHash":read["hash"],"content":"Revised"});
    assert!(call("project_edit", edit.clone(), "inspect-edit").is_err());
    select(&f, "edit");
    #[cfg(unix)]
    {
        call("project_edit", edit.clone(), "edit-one").unwrap();
        assert_eq!(
            std::fs::read_to_string(f.root.join("paper.md")).unwrap(),
            "Revised"
        );
        assert!(call("project_edit", edit, "stale-edit").is_err());
        std::os::unix::fs::symlink(f.root.join("paper.md"), f.root.join("link.md")).unwrap();
        assert!(call("project_read", json!({"path":"link.md"}), "symlink").is_err());
    }
}

#[tokio::test]
async fn claude_mcp_advertises_only_captured_tools_and_rejects_early_execution() {
    let specs = vec![json!({"name":"project_read","inputSchema":{"type":"object"}})];
    let request = json!({"server_name":"pipeline","message":{"id":1,"method":"tools/list"}});
    assert_eq!(
        tools::message(&request, &specs, None).await.unwrap()["mcp_response"]["result"]["tools"],
        json!(specs)
    );
    let call = json!({"server_name":"pipeline","message":{"id":2,"method":"tools/call","params":{"name":"project_read","arguments":{}}}});
    assert!(tools::message(&call, &specs, None).await.is_err());
    assert!(
        tools::message(&json!({"server_name":"other"}), &specs, None)
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "Requires installed, subscription-authenticated Claude Code; performs no model turn"]
async fn claude_subscription_handshake_probe() {
    let f = fixture();
    select(&f, "inspect");
    let effective = super::super::research::resolve_harness(&f.store, &f.session).unwrap();
    let specs = tools::catalog(&effective);
    let args = protocol::arguments(&effective, None, None);
    let mut process = transport::Process::start(&args).await.unwrap();
    let result = process.initialize(&specs).await;
    process.stop().await;
    let initialization = result.unwrap();
    protocol::validate_model(&initialization, None, None).unwrap();
    assert!(!initialization["models"].as_array().unwrap().is_empty());
    println!("Subscription handshake and model catalog succeeded; no prompt sent.");
}

#[tokio::test]
#[ignore = "Uses subscription quota for one small model turn and an inert MCP fixture"]
async fn claude_subscription_model_probe() {
    let executable = crate::deps::resolve_command("claude").unwrap();
    let mut version = executable.command(["--version"]);
    crate::agent_runtime::claude::configure_subscription(&mut version);
    let version = crate::process::run_bounded_independent(
        &mut version,
        std::time::Duration::from_secs(15),
        4096,
    )
    .unwrap();
    assert!(version.status.success());
    println!(
        "CLI under test: {}",
        String::from_utf8_lossy(&version.stdout).trim()
    );
    let f = fixture();
    select(&f, "inspect");
    let effective = super::super::research::resolve_harness(&f.store, &f.session).unwrap();
    let specs = vec![
        json!({"name":"pipeline_probe","description":"Return the verification marker.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
    ];
    let mut process =
        transport::Process::start(&protocol::arguments(&effective, Some("haiku"), None))
            .await
            .unwrap();
    process.initialize(&specs).await.unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(90), async {
        process.write(&json!({"type":"user","message":{"role":"user","content":"Call the pipeline_probe tool once and reply with its exact verification marker. Do nothing else."},"parent_tool_use_id":null,"session_id":""})).await?;
        let mut called = false;
        loop {
            let frame = process.read().await?;
            match frame["type"].as_str() {
                Some("control_request") => {
                    let request = &frame["request"];
                    let response = if request["subtype"] == "mcp_message" && request["message"]["method"] == "tools/call" {
                        assert_eq!(request["server_name"], "pipeline");
                        assert_eq!(request["message"]["params"]["name"], "pipeline_probe");
                        called = true;
                        json!({"mcp_response":{"jsonrpc":"2.0","id":request["message"]["id"],"result":{"content":[{"type":"text","text":"PIPELINE_SUBSCRIPTION_OK"}],"isError":false}}})
                    } else {
                        tools::message(request, &specs, None).await?
                    };
                    process.write(&protocol::control_response(&frame["request_id"], response)).await?;
                }
                Some("result") => {
                    assert_eq!(frame["subtype"], "success", "{frame}");
                    assert_eq!(frame["is_error"], false, "{frame}");
                    assert!(called, "Model did not call the scoped MCP fixture");
                    assert!(frame["result"].as_str().is_some_and(|s| s.contains("PIPELINE_SUBSCRIPTION_OK")));
                    return Ok::<_, String>(());
                }
                _ => {}
            }
        }
    }).await;
    process.stop().await;
    result.unwrap().unwrap();
    println!("Subscription model turn and MCP request/result completed successfully.");
}
