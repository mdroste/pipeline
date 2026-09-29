use super::*;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::test]
async fn shared_requests_preserve_owner_authority_and_rpc_rejections() {
    for (service, permissions, approval, roots, config, base, schema) in [
        (
            "pipeline_workflows",
            "review",
            "never",
            vec![],
            Some(json!({"web_search":"disabled"})),
            None,
            Some(json!({"type":"object"})),
        ),
        (
            "pipeline_workbench",
            "workbench-edit",
            "untrusted",
            vec!["/project".to_string()],
            None,
            Some("Literal base instructions"),
            None,
        ),
    ] {
        let (client_io, server_io) = tokio::io::duplex(64 * 1024);
        let (read, write) = tokio::io::split(client_io);
        let client = AppServerClient::from_io(1, read, write);
        let expected_roots = roots.clone();
        let expected_config = config.clone();
        let expected_schema = schema.clone();
        let server = tokio::spawn(async move {
            let (read, mut write) = tokio::io::split(server_io);
            let mut reader = BufReader::new(read);
            for method in ["thread/start", "turn/start"] {
                let mut line = String::new();
                reader.read_line(&mut line).await.unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                assert_eq!(request["method"], method);
                let params = &request["params"];
                if method == "thread/start" {
                    assert_eq!(params["modelProvider"], "openai");
                    assert_eq!(params["allowProviderModelFallback"], false);
                    assert_eq!(params["approvalsReviewer"], "user");
                    assert_eq!(params["serviceName"], service);
                    assert_eq!(params["permissions"], permissions);
                    assert_eq!(params["approvalPolicy"], approval);
                    assert_eq!(params["runtimeWorkspaceRoots"], json!(expected_roots));
                    assert_eq!(params["config"], json!(expected_config));
                    assert_eq!(params["baseInstructions"], json!(base));
                    assert_eq!(params["developerInstructions"], "Literal supplement");
                    assert_eq!(params["dynamicTools"], json!([{ "name":"owner_tool" }]));
                    assert_eq!(params["reasoningEffort"], "high");
                    write
                        .write_all(
                            format!(
                                "{}\n",
                                json!({"id":request["id"],"result":{"thread":{"id":"t"}}})
                            )
                            .as_bytes(),
                        )
                        .await
                        .unwrap();
                } else {
                    assert_eq!(params["threadId"], "t");
                    assert_eq!(params["input"], json!([{"type":"text", "text":"Task"}]));
                    assert_eq!(params["clientUserMessageId"], "message");
                    assert_eq!(params["effort"], "high");
                    assert_eq!(params["outputSchema"], json!(expected_schema));
                    write.write_all(format!("{}\n", json!({"id":request["id"],"error":{"code":-32602,"message":"rejected fixture"}})).as_bytes()).await.unwrap();
                }
            }
        });
        client
            .start_native_thread(NativeThreadStart {
                cwd: std::path::Path::new("/project"),
                runtime_workspace_roots: &roots,
                permissions,
                approval_policy: approval,
                developer_instructions: Some("Literal supplement"),
                base_instructions: base,
                dynamic_tools: &[json!({"name":"owner_tool"})],
                model: Some("fixture"),
                reasoning_effort: Some("high"),
                ephemeral: false,
                service_name: service,
                config,
            })
            .await
            .unwrap();
        let error = client
            .start_text_turn(TextTurnStart {
                thread_id: "t",
                text: "Task",
                client_user_message_id: Some("message"),
                model: None,
                effort: Some("high"),
                output_schema: schema.as_ref(),
            })
            .await
            .unwrap_err();
        assert_eq!(error.rpc_code, Some(-32602));
        assert_eq!(error.message, "rejected fixture");
        server.await.unwrap();
    }
}

#[test]
fn unresolved_paths_never_match() {
    use crate::agent_runtime::codex::session::same_existing_path;
    let root = tempfile::tempdir().unwrap();
    assert!(same_existing_path(root.path(), root.path()));
    assert!(!same_existing_path(
        &root.path().join("missing"),
        &root.path().join("missing")
    ));
    assert!(!same_existing_path(
        root.path(),
        &root.path().join("missing")
    ));
}
