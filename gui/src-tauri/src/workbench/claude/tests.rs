use super::super::store::{CreateSessionRequest, CreateWorkspaceRequest, UpdateSessionRequest};
use super::*;

#[test]
fn success_marks_only_the_last_answer_final_without_duplicate_transcript_items() {
    for with_assistant in [true, false] {
        let f = fixture();
        let bound = f
            .store
            .bind_session(&f.session, NAMESPACE, "claude-thread")
            .unwrap();
        f.store
            .record_turn_submission(&bound.id, "submission", "claude-turn")
            .unwrap();
        let turn = Turn {
            epoch: fresh_epoch(),
            thread: "claude-thread".into(),
            id: "claude-turn".into(),
            binding: bound.id.clone(),
            model: None,
        };
        let mut answers = super::answers::Answers::default();
        let answer = r#"{"summary":"The calibrated result"}"#;
        let mut frames = Vec::new();
        if with_assistant {
            frames.push(json!({"type":"assistant","message":{"content":[{"type":"text","text":"Checking the source"},{"type":"tool_use","name":"project_read"}]}}));
            frames.push(json!({"type":"assistant","parent_tool_use_id":"child","message":{"content":[{"type":"text","text":"Child output"}]}}));
            frames.push(
                json!({"type":"assistant","message":{"content":[{"type":"text","text":answer}]}}),
            );
        }
        frames.push(json!({"type":"result","is_error":false,"subtype":"success","result":answer}));
        for frame in frames {
            for event in answers.observe(&frame, &turn) {
                f.store.project_codex_event(&bound.id, &event).unwrap();
                f.store.project_codex_event(&bound.id, &event).unwrap();
            }
        }
        assert!(answers.final_received);
        let outcome = super::super::tasks::outcome(&f.store, &f.session, "submission")
            .unwrap()
            .unwrap();
        assert_eq!(outcome["finalText"], answer);
        let snapshot = f.store.conversation_snapshot(&f.session).unwrap();
        assert_eq!(snapshot.items.len(), if with_assistant { 2 } else { 1 });
        assert!(snapshot.items.iter().all(|item| item.is_final));
        assert!(!outcome["text"].as_str().unwrap().contains("Child output"));
    }
}

#[test]
fn failed_claude_result_never_promotes_partial_json_to_a_final_answer() {
    let f = fixture();
    let bound = f
        .store
        .bind_session(&f.session, NAMESPACE, "claude-thread")
        .unwrap();
    f.store
        .record_turn_submission(&bound.id, "submission", "claude-turn")
        .unwrap();
    let turn = Turn {
        epoch: fresh_epoch(),
        thread: "claude-thread".into(),
        id: "claude-turn".into(),
        binding: bound.id.clone(),
        model: None,
    };
    let mut answers = super::answers::Answers::default();
    assert_eq!(answers.stream_index(), 0);
    for frame in [
        json!({"type":"assistant","message":{"content":[{"type":"text","text":"{partial"}]}}),
        json!({"type":"result","is_error":true,"subtype":"error_max_turns","result":"limit reached"}),
    ] {
        for event in answers.observe(&frame, &turn) {
            f.store.project_codex_event(&bound.id, &event).unwrap();
        }
    }
    let outcome = super::super::tasks::outcome(&f.store, &f.session, "submission")
        .unwrap()
        .unwrap();
    assert!(outcome["finalText"].is_null());
    assert_eq!(outcome["text"], "{partial");
    assert!(!answers.final_received);
}

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
    let mut process = transport::Process::start(&args, None, None).await.unwrap();
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
        let mut clock = super::super::turn_loop::ModelClock::new(
            Duration::from_secs(1),
            Duration::from_secs(2),
        );
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

/// A stand-in CLI that reads the prompt and then runs `script`.
#[cfg(unix)]
fn fake_cli(script: &str) -> transport::Process {
    let mut command = tokio::process::Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(format!("read prompt\n{script}"))
        .kill_on_drop(true);
    transport::Process::attach(command, tempfile::tempdir().unwrap(), None, None).unwrap()
}

#[cfg(unix)]
fn recorded_turn(label: &str) -> Turn {
    let store = crate::orchestration::tests::disposable_workspace();
    let session = store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "Fake CLI".into(),
            operation_id: format!("fake-cli-{label}"),
        })
        .unwrap()
        .record
        .id;
    let thread = format!("claude-thread-{label}");
    let id = format!("claude-turn-{label}");
    let bound = store.bind_session(&session, NAMESPACE, &thread).unwrap();
    store
        .record_turn_submission(&bound.id, &format!("submission-{label}"), &id)
        .unwrap();
    Turn {
        epoch: fresh_epoch(),
        thread,
        id,
        binding: bound.id,
        model: None,
    }
}

#[cfg(unix)]
async fn drive_fake_cli(label: &str, script: &str, model_limit: u64) -> Result<(), String> {
    use std::time::Duration;
    let mut process = fake_cli(script);
    let (_cancel, receiver) = watch::channel(false);
    let result = transport::drive(
        &mut process,
        &super::super::turn_loop::RecordedEvents::default(),
        &recorded_turn(label),
        &protocol::Prompt {
            text: "Run the regressions".into(),
            user_text: "Run the regressions".into(),
        },
        Vec::new(),
        super::super::turn_loop::ModelClock::new(
            Duration::from_millis(model_limit),
            Duration::from_secs(30),
        ),
        receiver,
    )
    .await;
    process.stop().await;
    result
}

#[test]
fn a_sent_message_is_saved_before_anything_can_end_its_turn() {
    let source = include_str!("mod.rs");
    let submit = &source[source.find("pub(crate) async fn submit").unwrap()..];
    let saved = submit.find("\"userMessage\"").unwrap();
    for later in ["tokio::spawn", "clear_sent_draft"] {
        assert!(saved < submit.find(later).unwrap(), "{later}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn stop_before_the_worker_writes_keeps_the_saved_message() {
    let turn = recorded_turn("stopped-first");
    let events = super::super::turn_loop::RecordedEvents::default();
    turn.item(&events, "user", "userMessage", "Keep this message")
        .await
        .unwrap();
    let mut process = fake_cli("sleep 30");
    let (cancel, receiver) = watch::channel(false);
    cancel.send(true).unwrap();
    let result = transport::drive(
        &mut process,
        &events,
        &turn,
        &protocol::Prompt {
            text: "Keep this message".into(),
            user_text: "Keep this message".into(),
        },
        Vec::new(),
        transport::turn_clock(),
        receiver,
    )
    .await;
    process.stop().await;
    assert_eq!(result, Err("interrupted".to_string()));
    let thread = turn.thread.clone();
    let snapshot = run_store(move |store| {
        let session = store.session_for_thread(NAMESPACE, &thread)?.unwrap();
        store.conversation_snapshot(&session)
    })
    .await
    .unwrap();
    let saved = snapshot
        .items
        .iter()
        .filter(|item| item.item_kind == "userMessage")
        .count();
    assert_eq!(saved, 1);
}

#[cfg(unix)]
const NATIVE_COMMAND: &str = r#"
printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"tool_use","id":"bash-1","name":"Bash","input":{"command":"stata -b do models.do"}}]}}'
sleep 2
printf '%s\n' '{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"bash-1","content":"done"}]}}'
"#;
#[cfg(unix)]
const FINISH: &str = r#"
printf '%s\n' '{"type":"result","is_error":false,"subtype":"success","result":"The models ran."}'
"#;

#[cfg(unix)]
#[tokio::test]
async fn a_native_command_outlasting_the_model_limit_does_not_end_the_turn() {
    let script = format!("{NATIVE_COMMAND}{FINISH}");
    assert_eq!(drive_fake_cli("native-long", &script, 1000).await, Ok(()));
}

#[cfg(unix)]
#[tokio::test]
async fn silence_with_no_command_running_is_still_model_time() {
    let script = format!("sleep 5\n{FINISH}");
    let error = drive_fake_cli("model-silent", &script, 200)
        .await
        .unwrap_err();
    assert!(error.contains("15 minutes of model time"), "{error}");
    assert!(error.contains("commands"), "{error}");
}

#[cfg(unix)]
#[tokio::test]
async fn model_time_resumes_after_a_native_command_returns() {
    let script = format!("{NATIVE_COMMAND}sleep 5\n{FINISH}");
    let error = drive_fake_cli("model-after-command", &script, 1000)
        .await
        .unwrap_err();
    assert!(error.contains("model time"), "{error}");
}

#[tokio::test]
async fn a_tool_mistake_goes_back_to_claude_as_a_failed_result() {
    let specs = vec![json!({"name":"project_create","inputSchema":{"type":"object"}})];
    let f = fixture();
    let turn = Turn {
        epoch: fresh_epoch(),
        thread: "claude-thread".into(),
        id: "claude-turn".into(),
        binding: f
            .store
            .bind_session(&f.session, NAMESPACE, "claude-thread")
            .unwrap()
            .id,
        model: None,
    };
    let call = |id: u64, name: &str, arguments: Value| json!({"server_name":"pipeline","message":{"id":id,"method":"tools/call","params":{"name":name,"arguments":arguments}}});
    for (request, expected) in [
        (
            call(
                1,
                "project_create",
                json!({"content":"x".repeat(256 * 1024)}),
            ),
            "256 KiB",
        ),
        (call(2, "project_create", json!("not an object")), "256 KiB"),
        (call(3, "project_delete", json!({})), "not enabled"),
    ] {
        let answer = tools::message(&request, &specs, Some(&turn)).await.unwrap();
        let result = &answer["mcp_response"]["result"];
        assert_eq!(result["isError"], true);
        assert!(
            result["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains(expected),
            "{answer}"
        );
    }
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
    let sandbox = shell::sandbox(root, &Default::default(), false);
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
        shell::available(wsl::detect_async().await.environment().unwrap().as_ref()),
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
    let mut process = transport::Process::start(
        &args,
        Some(&shell::sandbox(&root, &Default::default(), false)),
        None,
    )
    .await
    .unwrap();
    process.initialize(&[]).await.unwrap();
    let commands = [
        format!("cat {}", root.join("marker.txt").display()),
        format!("cat {}", outside.path().join("secret.txt").display()),
        format!("echo changed > {}", root.join("written.txt").display()),
        "curl --fail -sS -m 5 https://example.org > /dev/null && echo PIPELINE_UNEXPECTED_NETWORK"
            .to_string(),
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
    assert!(
        !outputs.contains("PIPELINE_UNEXPECTED_NETWORK"),
        "Network was allowed"
    );
    println!(
        "Sandboxed shell read the root and was denied home reads, project writes and network."
    );
}

#[tokio::test]
#[ignore = "Uses subscription quota for one small model turn with writable sandbox and domain checks"]
async fn claude_working_copy_network_probe() {
    assert!(shell::available(
        wsl::detect_async().await.environment().unwrap().as_ref()
    ));
    let f = fixture();
    select(&f, "edit");
    let mut effective = super::super::research::resolve_harness(&f.store, &f.session).unwrap();
    effective.capabilities.shell_write = true;
    effective.capabilities.command_domains = vec!["example.org".into()];
    effective.command_network = true;
    let root = f.store.runtime_root(&f.session).unwrap();
    let args = protocol::arguments(&effective, Some("haiku"), None, Some(&root));
    let mut process = transport::Process::start(
        &args,
        Some(&shell::sandbox(&root, &effective.capabilities, true)),
        None,
    )
    .await
    .unwrap();
    process.initialize(&[]).await.unwrap();
    let commands=[format!("printf PIPELINE_COPY_WRITE > {}/created.txt",root.display()),"curl --fail -sS -m 10 https://example.org > /dev/null && echo PIPELINE_ALLOWED_DOMAIN".into(),"curl --fail -sS -m 5 https://example.com > /dev/null && echo PIPELINE_DENIED_DOMAIN".into(),"curl --noproxy '*' --fail -sS -m 5 https://example.org > /dev/null && echo PIPELINE_BYPASSED_PROXY".into()];
    let prompt=format!("Use Bash to run each command exactly once, one call each, even if one fails. Do not retry, change commands, or use other tools. Reply DONE.\n{}",commands.join("\n"));
    let result=tokio::time::timeout(std::time::Duration::from_secs(180),async {
        process.write(&json!({"type":"user","message":{"role":"user","content":prompt},"parent_tool_use_id":null,"session_id":""})).await?;
        let mut outputs=String::new();
        loop {
            let frame=process.read().await?;
            match frame["type"].as_str() {
                Some("control_request")=>process.control(&frame,&[],None).await?,
                Some("user")=>outputs.push_str(&frame["message"]["content"].to_string()),
                Some("result")=>return Ok::<_,String>(outputs),
                _=>{}
            }
        }
    }).await;
    process.stop().await;
    let outputs = result.unwrap().unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("created.txt")).unwrap(),
        "PIPELINE_COPY_WRITE"
    );
    assert!(
        outputs.contains("PIPELINE_ALLOWED_DOMAIN"),
        "Allowed site failed: {outputs}"
    );
    assert!(
        !outputs.contains("PIPELINE_DENIED_DOMAIN"),
        "Domain restriction failed: {outputs}"
    );
    assert!(
        !outputs.contains("PIPELINE_BYPASSED_PROXY"),
        "Direct network escaped the proxy: {outputs}"
    );
}

#[test]
fn private_folder_turns_are_told_to_work_there() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(temp.path()).unwrap();
    let session = store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "Unfiled".into(),
            operation_id: "session".into(),
        })
        .unwrap()
        .record
        .id;
    let effective = super::super::research::resolve_harness(&store, &session).unwrap();
    let root = store.runtime_root(&session).unwrap();
    let args = protocol::arguments(&effective, None, None, Some(&root));
    let prompt = &args[args
        .iter()
        .position(|a| a == "--append-system-prompt")
        .unwrap()
        + 1];
    assert!(prompt.contains("starts in this conversation's private working folder"));
    assert!(prompt.contains(&root.display().to_string()));
    assert!(!prompt.contains("File edits require Edit access"));
    let sandbox = shell::sandbox(&root, &effective.capabilities, false);
    assert_eq!(
        sandbox["filesystem"]["allowWrite"][0],
        root.to_string_lossy().as_ref()
    );
    assert_eq!(sandbox["filesystem"]["denyWrite"], json!([]));
}

#[tokio::test]
#[ignore = "Uses subscription quota for one small model turn in a private working folder"]
async fn claude_private_folder_probe() {
    assert!(shell::available(
        wsl::detect_async().await.environment().unwrap().as_ref()
    ));
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(temp.path()).unwrap();
    let session = store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "Unfiled".into(),
            operation_id: "session".into(),
        })
        .unwrap()
        .record
        .id;
    let effective = super::super::research::resolve_harness(&store, &session).unwrap();
    let root = store.runtime_root(&session).unwrap();
    let args = protocol::arguments(&effective, Some("haiku"), None, Some(&root));
    let mut process = transport::Process::start(
        &args,
        Some(&shell::sandbox(&root, &effective.capabilities, false)),
        Some(&root),
    )
    .await
    .unwrap();
    process.initialize(&[]).await.unwrap();
    let prompt = "Use Bash once to run exactly this command, then reply DONE.\nprintf PIPELINE_RELATIVE_WRITE > relative.txt";
    let result = tokio::time::timeout(std::time::Duration::from_secs(180), async {
        process.write(&json!({"type":"user","message":{"role":"user","content":prompt},"parent_tool_use_id":null,"session_id":""})).await?;
        loop {
            let frame = process.read().await?;
            match frame["type"].as_str() {
                Some("control_request") => process.control(&frame, &[], None).await?,
                Some("result") => return Ok::<_, String>(()),
                _ => {}
            }
        }
    })
    .await;
    process.stop().await;
    result.unwrap().unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("relative.txt")).unwrap(),
        "PIPELINE_RELATIVE_WRITE"
    );
}

#[test]
fn shared_web_access_does_not_grant_shell_network_or_main_project_writes() {
    let f = fixture();
    select(&f, "edit");
    super::super::research::save_config(
        &f.store,
        super::super::research::SaveWorkspaceConfigRequest {
            workspace_id: None,
            expected_revision: Some(0),
            body: json!({"webSearch":true,"browser":true}),
            operation_id: "web-access".into(),
        },
    )
    .unwrap();
    let effective = super::super::research::resolve_harness(&f.store, &f.session).unwrap();
    assert!(effective.web_search && effective.capabilities.browser);
    assert!(!effective.command_network && !effective.capabilities.shell_write);
    for name in ["project_create", "web_fetch", "web_download", "browser"] {
        assert!(effective
            .dynamic_tools
            .iter()
            .any(|spec| spec["name"] == name));
    }
    let args = protocol::arguments(&effective, None, None, Some(&f.root));
    assert_eq!(
        args[args.iter().position(|v| v == "--tools").unwrap() + 1],
        "Bash,WebSearch,WebFetch"
    );
    assert_eq!(
        shell::sandbox(&f.root, &effective.capabilities, effective.command_network)["network"]
            ["allowedDomains"],
        json!([])
    );
    let codex = effective.capabilities.codex_config(
        effective.web_search,
        effective.command_network,
        &effective.permission_profile,
    );
    assert_eq!(codex["web_search"], "live");
    assert!(codex.get("features.network_proxy").is_none());
    assert_eq!(effective.permission_profile, "workbench-inspect");
}
