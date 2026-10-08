use super::*;
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn newer_servers_must_echo_the_security_contract() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary
        .path()
        .canonicalize()
        .unwrap()
        .display()
        .to_string();
    let expected_roots = vec![root.clone()];
    let connection = ThreadConnection {
        workbench_binding_id: None,
        thread_id: "thread-1".to_string(),
        provider_session_id: "session-1".to_string(),
        model: "gpt-test".to_string(),
        model_provider: "openai".to_string(),
        cwd: root.clone(),
        instruction_sources: Vec::new(),
        runtime_workspace_roots: expected_roots.clone(),
        active_permission_profile: Some("pipeline-workbench".to_string()),
        approval_policy: Some("untrusted".to_string()),
        approvals_reviewer: Some("user".to_string()),
    };
    assert!(validate_thread_contract(
        &connection,
        Some(&root),
        &expected_roots,
        "pipeline-workbench",
        Some("gpt-test"),
    )
    .is_ok());

    for incompatible in [
        ThreadConnection {
            model_provider: "other".to_string(),
            ..connection.clone()
        },
        ThreadConnection {
            runtime_workspace_roots: Vec::new(),
            ..connection.clone()
        },
        ThreadConnection {
            active_permission_profile: None,
            ..connection.clone()
        },
        ThreadConnection {
            approval_policy: Some("never".to_string()),
            ..connection.clone()
        },
        ThreadConnection {
            model: "substituted".to_string(),
            ..connection.clone()
        },
    ] {
        assert!(validate_thread_contract(
            &incompatible,
            Some(&root),
            &expected_roots,
            "pipeline-workbench",
            Some("gpt-test"),
        )
        .is_err());
    }
}

#[test]
fn thread_lifecycle_uses_qualified_methods_and_exact_safe_defaults() {
    runtime().block_on(async {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().canonicalize().unwrap();
        let (client_io, server_io) = tokio::io::duplex(64 * 1024);
        let (client_reader, client_writer) = tokio::io::split(client_io);
        let (server_reader, mut server_writer) = tokio::io::split(server_io);
        let mut server_reader = BufReader::new(server_reader);
        let workspace_for_server = workspace.clone();
        let server = tokio::spawn(async move {
            for expected in [
                "thread/start",
                "turn/start",
                "turn/interrupt",
                "thread/read",
                "thread/resume",
            ] {
                let mut line = String::new();
                server_reader.read_line(&mut line).await.unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                assert_eq!(request["method"], expected);
                if expected == "thread/resume" {
                    assert!(request["params"].get("baseInstructions").is_none(), "resume retains the persisted base prompt");
                }
                let result = match expected {
                    "thread/start" => {
                        assert_eq!(request["params"]["baseInstructions"], "You are an economics research partner.");
                        assert_eq!(request["params"]["modelProvider"], "openai");
                        assert_eq!(request["params"]["allowProviderModelFallback"], false);
                        assert_eq!(request["params"]["approvalPolicy"], "untrusted");
                        assert_eq!(request["params"]["dynamicTools"][0]["type"], "function");
                        assert_eq!(request["params"]["dynamicTools"][0]["name"], "fixture_read");
                        json!({
                            "thread":{"id":"thread-1","sessionId":"provider-session-1"},
                            "model":"gpt-test","modelProvider":"openai",
                            "activePermissionProfile":{"id":"pipeline-workbench"},
                            "approvalPolicy":"untrusted","approvalsReviewer":"user",
                            "cwd":workspace_for_server,
                            "instructionSources":[],
                            "runtimeWorkspaceRoots":[workspace_for_server]
                        })
                    }
                    "turn/start" => json!({"turn":{"id":"turn-1"}}),
                    "thread/resume" => json!({
                        "thread":{"id":"thread-1","sessionId":"provider-session-1"},
                        "model":"gpt-test","modelProvider":"openai",
                        "activePermissionProfile":{"id":"pipeline-workbench"},
                        "approvalPolicy":"untrusted","approvalsReviewer":"user",
                        "cwd":workspace_for_server,
                        "instructionSources":[],
                        "runtimeWorkspaceRoots":[workspace_for_server]
                    }),
                    _ => json!({}),
                };
                server_writer
                    .write_all(
                        format!("{}\n", json!({"id":request["id"],"result":result})).as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        let initialize = InitializeResult {
            user_agent: "codex_cli_rs/0.147.0".to_string(),
            platform_family: "unix".to_string(),
            platform_os: "macos".to_string(),
            codex_home: temporary.path().display().to_string(),
        };
        let client = AppServerClient::from_io(8, client_reader, client_writer);
        let supervisor = AppServerSupervisor::from_test_client(8, client, initialize);
        let root = workspace.display().to_string();
        let thread = supervisor
            .start_thread(StartThreadRequest {
                base_instructions: Some("You are an economics research partner.".into()),
                workbench_session_id: "session-test".to_string(),
                cwd: root.clone(),
                runtime_workspace_roots: vec![root.clone()],
                permissions: "pipeline-workbench".to_string(),
                developer_instructions: "Use only selected research context.".to_string(),
                model: None,
                effort: Some("high".to_string()),
                dynamic_tools: vec![json!({"type":"function","name":"fixture_read","description":"Read a fixture.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})],
            })
            .await
            .unwrap();
        assert_eq!(thread.provider_session_id, "provider-session-1");
        assert_eq!(
            supervisor
                .start_turn(StartTurnRequest {
                    workbench_binding_id: "binding-test".to_string(),
                    thread_id: thread.thread_id.clone(),
                    text: "Analyze the result.".to_string(),
                    client_user_message_id: "submission-1".to_string(),
                    model: None,
                    effort: None,
                })
                .await
                .unwrap(),
            "turn-1"
        );
        supervisor
            .interrupt_turn(&thread.thread_id, "turn-1")
            .await
            .unwrap();
        supervisor.read_thread(&thread.thread_id).await.unwrap();
        supervisor
            .resume_thread(
                "session-test",
                &thread.thread_id,
                "pipeline-workbench",
                vec![root],
            )
            .await
            .unwrap();
        server.await.unwrap();
    });
}

/// A scripted server for side-turn tests: answers thread/start and
/// turn/start, then plays the given notification frames.
async fn side_turn_server(
    reader: tokio::io::ReadHalf<tokio::io::DuplexStream>,
    mut writer: tokio::io::WriteHalf<tokio::io::DuplexStream>,
    workspace: PathBuf,
    after_turn_start: Vec<Value>,
    expect_interrupt: bool,
) -> Vec<Value> {
    let mut reader = BufReader::new(reader);
    let mut observed = Vec::new();
    for expected in ["thread/start", "turn/start"] {
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let request: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(request["method"], expected);
        observed.push(request.clone());
        let result = if expected == "thread/start" {
            json!({
                "thread":{"id":"thread-side","sessionId":"provider-side"},
                "model":"gpt-mini","modelProvider":"openai",
                "activePermissionProfile":{"id":"workbench-inspect"},
                "approvalPolicy":"untrusted","approvalsReviewer":"user",
                "cwd":workspace,
                "instructionSources":[],
                "runtimeWorkspaceRoots":[workspace]
            })
        } else {
            json!({"turn":{"id":"turn-side"}})
        };
        writer
            .write_all(format!("{}\n", json!({"id":request["id"],"result":result})).as_bytes())
            .await
            .unwrap();
    }
    for frame in after_turn_start {
        writer
            .write_all(format!("{frame}\n").as_bytes())
            .await
            .unwrap();
    }
    if expect_interrupt {
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let response: Value = serde_json::from_str(&line).unwrap();
        assert!(
            response.get("error").is_some(),
            "side call must decline the request"
        );
        observed.push(response);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let request: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(request["method"], "turn/interrupt");
        observed.push(request.clone());
        writer
            .write_all(format!("{}\n", json!({"id":request["id"],"result":{}})).as_bytes())
            .await
            .unwrap();
    }
    observed
}

fn side_turn_request(root: &Path) -> SideTurnRequest {
    SideTurnRequest {
        cwd: root.display().to_string(),
        permissions: "workbench-inspect".to_string(),
        developer_instructions: "Reply with only the title.".to_string(),
        text: "Title this.".to_string(),
        model: None,
        effort: Some("low".to_string()),
        timeout: Duration::from_secs(5),
    }
}

#[test]
fn side_turns_run_ephemeral_tool_free_threads_and_return_the_final_message() {
    runtime().block_on(async {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().canonicalize().unwrap();
        let (client_io, server_io) = tokio::io::duplex(64 * 1024);
        let (client_reader, client_writer) = tokio::io::split(client_io);
        let (server_reader, server_writer) = tokio::io::split(server_io);
        let frames = vec![
            json!({"method":"item/agentMessage/delta","params":{"threadId":"thread-side","turnId":"turn-side","itemId":"item-side","delta":"Ident"}}),
            json!({"method":"item/completed","params":{"threadId":"thread-side","turnId":"turn-side","item":{"id":"item-side","type":"agentMessage","text":"Identification in panels"}}}),
            json!({"method":"turn/completed","params":{"threadId":"thread-side","turn":{"id":"turn-side","status":"completed","items":[]}}}),
        ];
        let server = tokio::spawn(side_turn_server(
            server_reader,
            server_writer,
            workspace.clone(),
            frames,
            false,
        ));
        let client = AppServerClient::from_io(11, client_reader, client_writer);
        let supervisor = AppServerSupervisor::from_test_client(
            11,
            client,
            InitializeResult {
                user_agent: "codex_cli_rs/0.147.0".to_string(),
                platform_family: "unix".to_string(),
                platform_os: "macos".to_string(),
                codex_home: temporary.path().display().to_string(),
            },
        );
        let text = supervisor
            .run_owned_side_turn(side_turn_request(&workspace), None)
            .await
            .unwrap();
        assert_eq!(text, "Identification in panels");
        assert!(!supervisor.is_side_thread("thread-side"));
        let observed = server.await.unwrap();
        assert_eq!(observed[0]["params"]["ephemeral"], true);
        assert!(observed[0]["params"]["baseInstructions"].is_null());
        assert_eq!(observed[0]["params"]["dynamicTools"], json!([]));
        assert_eq!(observed[0]["params"]["permissions"], "workbench-inspect");
        assert_eq!(observed[0]["params"]["allowProviderModelFallback"], false);
        assert_eq!(observed[1]["params"]["threadId"], "thread-side");
        assert_eq!(observed[1]["params"]["effort"], "low");
        assert!(observed[1]["params"].get("clientUserMessageId").is_none());
    });
}

#[test]
fn side_turns_decline_server_requests_and_stop() {
    runtime().block_on(async {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().canonicalize().unwrap();
        let (client_io, server_io) = tokio::io::duplex(64 * 1024);
        let (client_reader, client_writer) = tokio::io::split(client_io);
        let (server_reader, server_writer) = tokio::io::split(server_io);
        let frames = vec![json!({
            "id":"approval-side",
            "method":"item/commandExecution/requestApproval",
            "params":{"threadId":"thread-side","turnId":"turn-side","command":"ls"}
        })];
        let server = tokio::spawn(side_turn_server(
            server_reader,
            server_writer,
            workspace.clone(),
            frames,
            true,
        ));
        let client = AppServerClient::from_io(12, client_reader, client_writer);
        let supervisor = AppServerSupervisor::from_test_client(
            12,
            client,
            InitializeResult {
                user_agent: "codex_cli_rs/0.147.0".to_string(),
                platform_family: "unix".to_string(),
                platform_os: "macos".to_string(),
                codex_home: temporary.path().display().to_string(),
            },
        );
        let error = supervisor
            .run_owned_side_turn(side_turn_request(&workspace), None)
            .await
            .unwrap_err();
        assert!(
            error.message.contains("tried to use a tool"),
            "{}",
            error.message
        );
        assert!(!supervisor.is_side_thread("thread-side"));
        let observed = server.await.unwrap();
        assert_eq!(observed[2]["id"], "approval-side");
        assert_eq!(observed[3]["params"]["turnId"], "turn-side");
    });
}

#[test]
fn account_login_catalog_and_quota_use_the_pinned_protocol_contract() {
    runtime().block_on(async {
        let temporary = tempfile::tempdir().unwrap();
        let (client_io, server_io) = tokio::io::duplex(64 * 1024);
        let (client_reader, client_writer) = tokio::io::split(client_io);
        let (server_reader, mut server_writer) = tokio::io::split(server_io);
        let mut server_reader = BufReader::new(server_reader);
        let server = tokio::spawn(async move {
            for (index, expected) in [
                "account/read",
                "account/login/start",
                "account/login/cancel",
                "account/logout",
                "model/list",
                "model/list",
                "account/rateLimits/read",
            ]
            .into_iter()
            .enumerate()
            {
                let mut line = String::new();
                server_reader.read_line(&mut line).await.unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                assert_eq!(request["method"], expected);
                let result = match index {
                    0 => {
                        assert_eq!(request["params"], json!({"refreshToken":false}));
                        json!({
                            "account":{"type":"chatgpt","email":"researcher@example.test","planType":"plus"},
                            "requiresOpenaiAuth":true
                        })
                    }
                    1 => {
                        assert_eq!(request["params"]["type"], "chatgpt");
                        assert_eq!(request["params"]["appBrand"], "chatgpt");
                        assert_eq!(request["params"]["useHostedLoginSuccessPage"], true);
                        assert!(request["params"].get("apiKey").is_none());
                        json!({"type":"chatgpt","loginId":"login-1","authUrl":"https://auth.openai.test/login"})
                    }
                    2 => {
                        assert_eq!(request["params"], json!({"loginId":"login-1"}));
                        json!({"status":"canceled"})
                    }
                    3 => {
                        assert!(request["params"].is_null());
                        json!({})
                    }
                    4 => {
                        assert_eq!(request["params"]["cursor"], Value::Null);
                        assert_eq!(request["params"]["includeHidden"], false);
                        assert_eq!(request["params"]["limit"], MODEL_PAGE_SIZE);
                        json!({
                            "data":[model_fixture("model-a", true)],
                            "nextCursor":"page-2"
                        })
                    }
                    5 => {
                        assert_eq!(request["params"]["cursor"], "page-2");
                        json!({"data":[model_fixture("model-b", false)],"nextCursor":null})
                    }
                    6 => {
                        assert!(request["params"].is_null());
                        json!({
                            "rateLimits": {"primary":{"usedPercent":91}},
                            "rateLimitsByLimitId": {
                                "codex": {
                                    "limitName":"Codex",
                                    "planType":"plus",
                                    "primary":{"usedPercent":42,"windowDurationMins":300,"resetsAt":1234}
                                }
                            }
                        })
                    }
                    _ => unreachable!(),
                };
                server_writer
                    .write_all(
                        format!("{}\n", json!({"id":request["id"],"result":result})).as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        let client = AppServerClient::from_io(9, client_reader, client_writer);
        let supervisor = AppServerSupervisor::from_test_client(
            9,
            client,
            InitializeResult {
                user_agent: "codex_cli_rs/0.147.0".to_string(),
                platform_family: "unix".to_string(),
                platform_os: "macos".to_string(),
                codex_home: temporary.path().display().to_string(),
            },
        );

        let account = supervisor.account_state(false).await.unwrap();
        assert_eq!(account.status, AccountStatus::Chatgpt);
        assert_eq!(account.plan_type.as_deref(), Some("plus"));
        let login = supervisor.login_start().await.unwrap();
        assert_eq!(login.login_id, "login-1");
        assert!(supervisor.login_cancel(&login.login_id).await.unwrap());
        supervisor.logout().await.unwrap();
        let catalog = supervisor.model_catalog().await.unwrap();
        assert_eq!(catalog.models.len(), 2);
        assert_eq!(catalog.models[1].model, "model-b");
        assert_eq!(
            catalog
                .validate_exact("model-a", Some("medium"))
                .unwrap()
                .model,
            "model-a"
        );
        let unavailable = catalog.validate_exact("model-missing", None).unwrap_err();
        assert_eq!(unavailable.code, "invalid_input");
        assert!(catalog.validate_exact("model-a", Some("high")).is_err());
        let limits = supervisor.rate_limits().await.unwrap();
        assert_eq!(limits.source, RateLimitSource::PerBucket);
        assert_eq!(limits.buckets[0].limit_id.as_deref(), Some("codex"));
        assert_eq!(limits.buckets[0].primary.as_ref().unwrap().used_percent, 42);
        assert_eq!(
            limits.buckets[0].primary.as_ref().unwrap().remaining_percent,
            58
        );
        server.await.unwrap();
    });
}

#[test]
fn account_parser_distinguishes_signed_out_and_unsupported_modes() {
    assert_eq!(
        parse_account_state(json!({"account":null,"requiresOpenaiAuth":true}))
            .unwrap()
            .status,
        AccountStatus::SignedOut
    );
    let unsupported = parse_account_state(json!({
        "account":{"type":"apiKey"},
        "requiresOpenaiAuth":true
    }))
    .unwrap();
    assert_eq!(unsupported.status, AccountStatus::Unsupported);
    assert_eq!(
        unsupported.unsupported_account_type.as_deref(),
        Some("apiKey")
    );
}

fn model_fixture(id: &str, is_default: bool) -> Value {
    json!({
        "id":id,
        "model":id,
        "displayName":id,
        "description":"Test model",
        "isDefault":is_default,
        "defaultReasoningEffort":"medium",
        "supportedReasoningEfforts":[{
            "reasoningEffort":"medium",
            "description":"Balanced"
        }]
    })
}

#[test]
#[ignore = "requires a locally installed capability-compatible Codex CLI"]
fn live_supervisor_starts_in_an_isolated_temporary_home_and_shuts_down_cleanly() {
    runtime().block_on(async {
        let temporary = tempfile::tempdir().unwrap();
        let codex_home = temporary.path().join("codex");
        let supervisor = AppServerSupervisor::launch_with_codex_home(73, codex_home.clone())
            .await
            .unwrap();
        let status = supervisor.status();
        assert!(status.connected);
        assert_eq!(status.epoch, 73);
        assert!(same_path(
            Path::new(status.codex_home.as_deref().unwrap()),
            &codex_home
        ));
        supervisor.shutdown().await;
        assert!(!supervisor.status().connected);
    });
}
