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
    let args = protocol::arguments(&effective, None, None, None);
    let mut process = transport::Process::start(&args, None).await.unwrap();
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
    let mut process = transport::Process::start(
        &protocol::arguments(&effective, Some("haiku"), None, None),
        None,
    )
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
                // Shows what the usage line would be built from.
                Some("rate_limit_event") => {
                    println!("Usage report: {}", frame["rate_limit_info"]);
                    println!("Parsed: {:?}", usage::parse(&frame, 0));
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

#[test]
fn host_work_does_not_use_the_turn_model_time() {
    use std::time::Duration;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut clock = transport::ModelClock::new(Duration::from_secs(1), Duration::from_secs(2));
        // A research run waiting on a local job outlasts the whole model limit.
        assert_eq!(
            clock
                .host(async {
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                    "result"
                })
                .await,
            Some("result")
        );
        assert_eq!(
            clock
                .model(async {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    "frame"
                })
                .await,
            Some("frame")
        );
        assert_eq!(
            clock
                .model(tokio::time::sleep(Duration::from_secs(5)))
                .await,
            None
        );
        // Host work is bounded by what is left of the turn's host time.
        let started = tokio::time::Instant::now();
        assert_eq!(
            clock.host(tokio::time::sleep(Duration::from_secs(5))).await,
            None
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    });
}

#[test]
fn usage_reports_are_validated_and_normalized_to_percentages() {
    let frame = |info: Value| json!({"type":"rate_limit_event","rate_limit_info":info});
    let report = usage::parse(
        &frame(json!({"status":"allowed_warning","resetsAt":1_900_000_000,"rateLimitType":"five_hour","utilization":0.82})),
        7,
    )
    .unwrap();
    assert_eq!(report.status, "allowed_warning");
    assert_eq!(report.resets_at, Some(1_900_000_000));
    assert_eq!(report.rate_limit_type.as_deref(), Some("five_hour"));
    assert!((report.used_percent.unwrap() - 82.0).abs() < 1e-9);
    assert_eq!(report.observed_at, 7);
    // The shape Claude Code 2.1.312 sends: utilization per window, not top level.
    let windows = usage::parse(
        &frame(
            json!({"status":"allowed","rateLimitType":"five_hour","resetsAt":1_793_266_200,
            "isUsingOverage":false,"overageStatus":"rejected",
            "unifiedWindows":{"five_hour":{"status":"allowed","utilization":0.69},
                "seven_day":{"status":"allowed","utilization":0}}}),
        ),
        0,
    )
    .unwrap();
    assert!((windows.used_percent.unwrap() - 69.0).abs() < 1e-9);
    assert!(!windows.is_using_overage);
    // A percentage is tolerated and bounded.
    let percent = usage::parse(&frame(json!({"status":"allowed","utilization":140})), 0).unwrap();
    assert_eq!(percent.used_percent, Some(100.0));
    let bare = usage::parse(
        &frame(json!({"status":"rejected","rateLimitType":"<b>"})),
        0,
    )
    .unwrap();
    assert_eq!(
        (bare.used_percent, bare.rate_limit_type, bare.resets_at),
        (None, None, None)
    );
    for invalid in [
        frame(json!({"status":"unknown"})),
        frame(json!({})),
        json!({"type":"result","rate_limit_info":{"status":"allowed"}}),
    ] {
        assert!(usage::parse(&invalid, 0).is_none(), "{invalid}");
    }
}

#[test]
fn turns_offer_bash_only_with_a_sandbox_root() {
    let f = fixture();
    select(&f, "edit");
    let effective = super::super::research::resolve_harness(&f.store, &f.session).unwrap();
    let tools =
        |args: &[String]| args[args.iter().position(|a| a == "--tools").unwrap() + 1].clone();
    let prompt = |args: &[String]| {
        args[args
            .iter()
            .position(|a| a == "--append-system-prompt")
            .unwrap()
            + 1]
        .clone()
    };
    let without = protocol::arguments(&effective, None, None, None);
    assert_eq!(tools(&without), "");
    assert!(prompt(&without).contains("only through authorized research profiles"));
    let with = protocol::arguments(&effective, None, None, Some(&f.root));
    assert_eq!(tools(&with), "Bash");
    assert!(prompt(&with).contains(&f.root.display().to_string()));
    // Bash is never pre-approved; only Claude Code's sandbox auto-approves it.
    let allowed = &with[with.iter().position(|a| a == "--allowedTools").unwrap() + 1];
    assert_eq!(allowed, "mcp__pipeline__*");
    for flag in [
        "--no-chrome",
        "--strict-mcp-config",
        "--disable-slash-commands",
    ] {
        assert!(with.iter().any(|a| a == flag), "{flag}");
    }
}

#[test]
fn shell_sandbox_is_read_only_offline_and_fails_closed() {
    let root = std::path::Path::new("/research/project");
    let sandbox = shell::sandbox(root);
    assert_eq!(sandbox["enabled"], true);
    assert_eq!(sandbox["failIfUnavailable"], true);
    assert_eq!(sandbox["allowUnsandboxedCommands"], false);
    assert_eq!(sandbox["excludedCommands"], json!([]));
    let fs = &sandbox["filesystem"];
    assert_eq!(fs["allowRead"], json!(["/research/project"]));
    assert_eq!(fs["denyWrite"], json!(["/research/project"]));
    let denied = fs["denyRead"].as_array().unwrap();
    for path in ["~/", "~/.ssh", "~/.claude", "~/.config"] {
        assert!(denied.iter().any(|d| d == path), "{path}");
    }
    let store = crate::storage::local_root().unwrap();
    assert!(denied.iter().any(|d| d == store.to_string_lossy().as_ref()));
    let network = &sandbox["network"];
    assert_eq!(network["allowedDomains"], json!([]));
    assert_eq!(network["strictAllowlist"], true);
    assert_eq!(network["allowAllUnixSockets"], false);
}

#[tokio::test]
#[ignore = "Uses subscription quota for one small model turn with sandboxed Bash"]
async fn claude_sandboxed_shell_probe() {
    assert!(
        shell::available(),
        "Claude Code's sandbox is unavailable here"
    );
    let f = fixture();
    select(&f, "edit");
    std::fs::write(f.root.join("marker.txt"), "PIPELINE_ROOT_MARKER").unwrap();
    let home = dirs::home_dir().unwrap();
    let outside = tempfile::Builder::new()
        .prefix(".pipeline-sandbox-probe-")
        .tempdir_in(&home)
        .unwrap();
    std::fs::write(outside.path().join("secret.txt"), "PIPELINE_HOME_SECRET").unwrap();
    let effective = super::super::research::resolve_harness(&f.store, &f.session).unwrap();
    let root = f.store.runtime_root(&f.session).unwrap();
    let args = protocol::arguments(&effective, Some("haiku"), None, Some(&root));
    let mut process = transport::Process::start(&args, Some(&shell::sandbox(&root)))
        .await
        .unwrap();
    process.initialize(&[]).await.unwrap();
    let commands = [
        format!("cat {}", root.join("marker.txt").display()),
        format!("cat {}", outside.path().join("secret.txt").display()),
        format!("echo changed > {}", root.join("written.txt").display()),
        "curl -sS -m 5 https://example.com".to_string(),
    ];
    let prompt = format!("Use the Bash tool to run each of these commands exactly, one Bash call each, even if one fails. Do not retry or change them. Then reply DONE.\n{}", commands.join("\n"));
    let result = tokio::time::timeout(std::time::Duration::from_secs(180), async {
        process.write(&json!({"type":"user","message":{"role":"user","content":prompt},"parent_tool_use_id":null,"session_id":""})).await?;
        let (mut calls, mut outputs) = (0, String::new());
        loop {
            let frame = process.read().await?;
            match frame["type"].as_str() {
                Some("control_request") => process.control(&frame, &[], None).await?,
                Some("assistant") => {
                    calls += frame["message"]["content"].as_array().into_iter().flatten()
                        .filter(|part| part["type"] == "tool_use" && part["name"] == "Bash").count();
                }
                Some("user") => outputs.push_str(&frame["message"]["content"].to_string()),
                Some("result") => return Ok::<_, String>((calls, outputs)),
                _ => {}
            }
        }
    })
    .await;
    process.stop().await;
    let (calls, outputs) = result.unwrap().unwrap();
    assert!(calls >= 4, "Model made {calls} Bash calls");
    assert!(
        outputs.contains("PIPELINE_ROOT_MARKER"),
        "Root read failed: {outputs}"
    );
    assert!(
        !outputs.contains("PIPELINE_HOME_SECRET"),
        "Home read was allowed"
    );
    assert!(
        !root.join("written.txt").exists(),
        "Project write was allowed"
    );
    assert!(!outputs.contains("Example Domain"), "Network was allowed");
    println!(
        "Sandboxed shell read the root and was denied home reads, project writes and network."
    );
}
