use super::*;

#[test]
fn every_api_projects_one_structured_final_answer_after_tool_commentary() {
    let answer = r#"{"summary":"The calibrated result"}"#;
    for provider in [
        Provider::OpenAi,
        Provider::Local,
        Provider::Anthropic,
        Provider::Google,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open_at(temp.path()).unwrap();
        let session = store
            .create_session(super::super::store::CreateSessionRequest {
                workspace_id: None,
                title: "API answer".into(),
                operation_id: "create".into(),
            })
            .unwrap()
            .record
            .id;
        let bound = store
            .bind_session(&session, NAMESPACE, "api-thread")
            .unwrap();
        store
            .record_turn_submission(&bound.id, "submission", "api-turn")
            .unwrap();
        let turn = Turn {
            epoch: next_epoch(),
            thread: "api-thread".into(),
            id: "api-turn".into(),
            binding: bound.id.clone(),
            model: None,
            runtime: provider.runtime().into(),
            usage: Arc::new(Mutex::new(Vec::new())),
        };
        let (progress, final_reply) = match provider {
            Provider::Anthropic => (
                json!({"stop_reason":"tool_use","content":[{"type":"text","text":"Checking the source"},{"type":"tool_use","id":"call","name":"project_read","input":{"path":"paper.tex"}}]}),
                json!({"stop_reason":"end_turn","content":[{"type":"text","text":answer}]}),
            ),
            Provider::Google => (
                json!({"candidates":[{"finishReason":"STOP","content":{"parts":[{"text":"Checking the source"},{"functionCall":{"name":"project_read","args":{"path":"paper.tex"}}}]}}]}),
                json!({"candidates":[{"finishReason":"STOP","content":{"parts":[{"text":answer}]}}]}),
            ),
            _ => (
                json!({"choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","content":"Checking the source","tool_calls":[{"id":"call","type":"function","function":{"name":"project_read","arguments":"{\"path\":\"paper.tex\"}"}}]}}]}),
                json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":answer}}]}),
            ),
        };
        for (round, response) in [progress, final_reply].into_iter().enumerate() {
            let reply = wire::parse(provider, response).unwrap();
            let event = turn.reply_event(round, &reply);
            store.project_codex_event(&bound.id, &event).unwrap();
            store.project_codex_event(&bound.id, &event).unwrap(); // reconnect replay
        }
        let outcome = super::super::tasks::outcome(&store, &session, "submission")
            .unwrap()
            .unwrap();
        assert!(outcome["text"]
            .as_str()
            .unwrap()
            .contains("Checking the source"));
        assert_eq!(outcome["finalText"], answer);
        assert_eq!(
            serde_json::from_str::<Value>(outcome["finalText"].as_str().unwrap()).unwrap()
                ["summary"],
            "The calibrated result"
        );
        assert_eq!(
            store.conversation_snapshot(&session).unwrap().items.len(),
            2
        );
    }
}

#[test]
fn provider_selection_is_explicit_and_legacy_defaults_remain() {
    for (runtime, expected) in [
        ("openai-api", Provider::OpenAi),
        ("anthropic-api", Provider::Anthropic),
        ("google-api", Provider::Google),
        ("local", Provider::Local),
    ] {
        assert_eq!(
            selected(&json!({"runtime":runtime})).unwrap(),
            Some(expected)
        );
    }
    for value in [
        json!({}),
        json!({"runtime":"codex"}),
        json!({"runtime":"claude"}),
    ] {
        assert_eq!(selected(&value).unwrap(), None);
    }
    assert!(selected(&json!({"runtime":"unknown"})).is_err());
    assert!(selected(&json!({"runtime":null})).is_err());
}

fn connection(provider: Provider) -> Connection {
    Connection {
        provider,
        endpoint: "http://127.0.0.1:1/v1/chat/completions".into(),
        key: "test-key".into(),
        model: "test-model".into(),
        effort: None,
        thinking: None,
    }
}

#[test]
fn all_protocols_preserve_tool_calls_and_feed_results_back() {
    let fixtures = [
        (
            Provider::OpenAi,
            json!({"choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","content":null,"tool_calls":[{"id":"call-1","type":"function","function":{"name":"project_read","arguments":"{\"path\":\"paper.tex\"}"}}]}}],"usage":{"total_tokens":17}}),
        ),
        (
            Provider::Local,
            json!({"choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","content":"Reading","tool_calls":[{"id":"call-1","type":"function","function":{"name":"project_read","arguments":"{\"path\":\"paper.tex\"}"}}]}}]}),
        ),
        (
            Provider::Anthropic,
            json!({"stop_reason":"tool_use","content":[{"type":"text","text":"Reading"},{"type":"tool_use","id":"call-1","name":"project_read","input":{"path":"paper.tex"}}],"usage":{"input_tokens":17}}),
        ),
        (
            Provider::Google,
            json!({"candidates":[{"finishReason":"STOP","content":{"role":"model","parts":[{"thought":true,"text":"private"},{"text":"Reading"},{"functionCall":{"name":"project_read","args":{"path":"paper.tex"}},"thoughtSignature":"opaque-signature"}]}}],"usageMetadata":{"totalTokenCount":17}}),
        ),
    ];
    for (provider, response) in fixtures {
        let reply = wire::parse(provider, response).unwrap();
        assert!(!reply.text.contains("private"));
        assert_eq!(reply.calls.len(), 1);
        assert_eq!(reply.calls[0].arguments["path"], "paper.tex");
        let result =
            json!({"success":true,"contentItems":[{"type":"inputText","text":"paper contents"}]});
        let mut messages = wire::initial(provider, "instructions", "question");
        messages.push(reply.message.clone());
        messages.extend(wire::tool_results(
            provider,
            vec![(&reply.calls[0], result)],
        ));
        let body = wire::body(&connection(provider), "instructions", &messages, &[]);
        assert!(body.to_string().contains("paper contents"));
        if provider == Provider::Google {
            assert!(body.to_string().contains("opaque-signature"));
        }
        if provider == Provider::Anthropic {
            assert_eq!(body["messages"][2]["content"][0]["tool_use_id"], "call-1");
        }
        if provider == Provider::OpenAi || provider == Provider::Local {
            assert_eq!(body["messages"][3]["tool_call_id"], "call-1");
        }
    }
}

#[test]
fn incomplete_empty_and_malformed_responses_fail_before_tools() {
    for response in [
        json!({}),
        json!({"choices":[{"finish_reason":"length","message":{"content":"partial"}}]}),
        json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":""}}]}),
        json!({"choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","tool_calls":[{"id":"1","type":"function","function":{"name":"project_read","arguments":"[]"}}]}}]}),
    ] {
        assert!(wire::parse(Provider::OpenAi, response).is_err());
    }
    assert!(wire::parse(Provider::Google,json!({"candidates":[{"finishReason":"MAX_TOKENS","content":{"parts":[{"text":"partial"}]}}]})).is_err());
    assert!(wire::parse(
        Provider::Anthropic,
        json!({"stop_reason":"max_tokens","content":[{"type":"text","text":"partial"}]})
    )
    .is_err());
}

#[test]
fn tool_schemas_and_images_use_each_providers_wire_format() {
    let spec = json!({"name":"project_read","description":"Read","inputSchema":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}});
    let call = wire::Call {
        id: "call".into(),
        name: "browser".into(),
        response_id: None,
        arguments: json!({}),
    };
    let result = json!({"success":true,"contentItems":[{"type":"inputImage","imageUrl":"data:image/png;base64,aGVsbG8="}]});
    for provider in [
        Provider::OpenAi,
        Provider::Local,
        Provider::Anthropic,
        Provider::Google,
    ] {
        let body = wire::body(
            &connection(provider),
            "instructions",
            &[],
            std::slice::from_ref(&spec),
        );
        let schema = match provider {
            Provider::Anthropic => &body["tools"][0]["input_schema"],
            Provider::Google => {
                &body["tools"][0]["functionDeclarations"][0]["parametersJsonSchema"]
            }
            _ => &body["tools"][0]["function"]["parameters"],
        };
        assert_eq!(schema, &spec["inputSchema"]);
        let results = wire::tool_results(provider, vec![(&call, result.clone())]);
        assert!(serde_json::to_string(&results)
            .unwrap()
            .contains("aGVsbG8="));
    }
}

async fn server(status: &str, response: String) -> (String, tokio::task::JoinHandle<String>) {
    server_with_type(status, response, "application/json").await
}

async fn server_with_type(
    status: &str,
    response: String,
    content_type: &str,
) -> (String, tokio::task::JoinHandle<String>) {
    let content_type = content_type.to_owned();
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    let status = status.to_owned();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let n = socket.read(&mut buffer).await.unwrap();
            if n == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..n]);
            let text = String::from_utf8_lossy(&bytes);
            if let Some((headers, body)) = text.split_once("\r\n\r\n") {
                let len = headers
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|s| s.parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if body.len() >= len {
                    break;
                }
            }
        }
        let _=socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).as_bytes()).await;
        String::from_utf8(bytes).unwrap()
    });
    (endpoint, task)
}

#[tokio::test]
async fn http_transport_uses_selected_auth_and_never_retries_errors() {
    for provider in [
        Provider::OpenAi,
        Provider::Local,
        Provider::Anthropic,
        Provider::Google,
    ] {
        let (endpoint, task) = server("200 OK", "{\"ok\":true}".into()).await;
        let mut connection = connection(provider);
        connection.endpoint = endpoint;
        let response = wire::request(
            &wire::client().unwrap(),
            &connection,
            &json!({"test":"prompt"}),
        )
        .await
        .unwrap();
        assert_eq!(response["ok"], true);
        let request = task.await.unwrap().to_ascii_lowercase();
        assert!(request.contains(match provider {
            Provider::Anthropic => "x-api-key: test-key",
            Provider::Google => "x-goog-api-key: test-key",
            _ => "authorization: bearer test-key",
        }));
        assert!(request.contains("prompt"));
    }
    let (endpoint, task) = server("429 Too Many Requests", "secret provider error".into()).await;
    let mut connection = connection(Provider::Local);
    connection.endpoint = endpoint;
    let error = wire::request(&wire::client().unwrap(), &connection, &json!({}))
        .await
        .unwrap_err();
    assert!(error.contains("429"));
    assert!(!error.contains("secret"));
    task.await.unwrap();
}

#[test]
fn recovery_interrupts_api_turn_without_replay_and_preserves_history() {
    use super::super::store::CreateSessionRequest;
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(temp.path()).unwrap();
    let session = store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "Test".into(),
            operation_id: "create".into(),
        })
        .unwrap()
        .record
        .id;
    let binding = store.bind_session(&session, NAMESPACE, "api-test").unwrap();
    store
        .record_turn_submission(&binding.id, "submission", "api-turn-test")
        .unwrap();
    store
        .project_codex_event(
            &binding.id,
            &NormalizedEvent::ItemCompleted {
                epoch: next_epoch(),
                thread_id: "api-test".into(),
                turn_id: "api-turn-test".into(),
                item_id: "user".into(),
                item_kind: "userMessage".into(),
                item: json!({"text":"Prior question"}),
            },
        )
        .unwrap();
    assert!(reconcile(store.clone(), session.clone()).unwrap());
    let snapshot = store.conversation_snapshot(&session).unwrap();
    assert_eq!(snapshot.turns[0].state, "interrupted");
    assert!(super::super::history::recent_context(&store, &session)
        .unwrap()
        .contains("Prior question"));
    assert!(reconcile(store.clone(), session.clone()).unwrap());
    assert_eq!(
        store.conversation_snapshot(&session).unwrap().turns.len(),
        1
    );
}

#[tokio::test]
async fn cancellation_drops_inflight_work_without_waiting_for_the_server() {
    struct Guard(std::sync::Arc<std::sync::atomic::AtomicBool>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let dropped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let guard = Guard(dropped.clone());
    let (cancel, receiver) = watch::channel(false);
    let (started, ready) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(drive::cancellable(
        async move {
            let _guard = guard;
            let _ = started.send(());
            std::future::pending::<()>().await;
            Ok::<_, String>(())
        },
        receiver,
    ));
    ready.await.unwrap();
    cancel.send(true).unwrap();
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err(),
        "interrupted"
    );
    assert!(dropped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn connection_uses_only_selected_credentials_and_validates_custom_urls() {
    let settings = crate::settings::Settings {
        openai_api_key: "openai-key".into(),
        anthropic_api_key: "anthropic-key".into(),
        google_api_key: "google-key".into(),
        local_api_key: String::new(),
        local_base_url: "http://127.0.0.1:9999/v1/".into(),
        ..Default::default()
    };
    for (provider, key) in [
        (Provider::OpenAi, "openai-key"),
        (Provider::Anthropic, "anthropic-key"),
        (Provider::Google, "google-key"),
        (Provider::Local, ""),
    ] {
        let connection = Connection::from_settings(provider, Some("model"), None, &settings)
            .await
            .unwrap();
        assert_eq!(connection.key, key);
        if provider == Provider::Local {
            assert_eq!(
                connection.endpoint,
                "http://127.0.0.1:9999/v1/chat/completions"
            );
        }
        if provider == Provider::Google {
            assert!(connection
                .endpoint
                .ends_with("models/model:generateContent"));
        }
    }
    for url in [
        "http://remote.example/v1",
        "https://user:password@example.com/v1",
        "https://example.com/v1?key=secret",
    ] {
        let mut settings = settings.clone();
        settings.local_base_url = url.into();
        assert!(
            Connection::from_settings(Provider::Local, Some("model"), None, &settings)
                .await
                .is_err()
        );
    }
    assert!(
        Connection::from_settings(Provider::OpenAi, Some("model"), None, &Default::default())
            .await
            .is_err()
    );
    assert!(
        Connection::from_settings(Provider::Google, Some("model"), Some("high"), &settings)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn transport_bounds_payloads_and_rejects_redirects() {
    let client = wire::client().unwrap();
    let mut connection = connection(Provider::Local);
    let too_large = json!({"text":"x".repeat(wire::MAX_BODY)});
    assert!(wire::request(&client, &connection, &too_large)
        .await
        .unwrap_err()
        .contains("8 MB request"));
    let (endpoint, task) = server(
        "302 Found\r\nLocation: http://127.0.0.1:1/other",
        String::new(),
    )
    .await;
    connection.endpoint = endpoint;
    assert!(wire::request(&client, &connection, &json!({}))
        .await
        .unwrap_err()
        .contains("302"));
    task.await.unwrap();
    let (endpoint, task) = server("200 OK", "x".repeat(wire::MAX_BODY + 1)).await;
    connection.endpoint = endpoint;
    assert!(wire::request(&client, &connection, &json!({}))
        .await
        .unwrap_err()
        .contains("exceeded 8 MB"));
    task.await.unwrap();
}

#[test]
fn switching_api_providers_invalidates_the_automation_harness() {
    use super::super::store::{CreateSessionRequest, UpdateSessionRequest};
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(temp.path()).unwrap();
    let session = store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "Test".into(),
            operation_id: "create".into(),
        })
        .unwrap()
        .record
        .id;
    let mut fingerprints = std::collections::HashSet::new();
    for runtime in ["openai-api", "anthropic-api", "google-api", "local"] {
        let current = store.session_snapshot(&session).unwrap().session;
        store
            .update_session(UpdateSessionRequest {
                session_id: session.clone(),
                expected_revision: current.revision,
                operation_id: format!("select-{runtime}"),
                title: None,
                draft: None,
                overrides: Some(json!({"runtime":runtime})),
                archived: None,
                preset_id: None,
                paper_id: None,
                clear_paper: None,
            })
            .unwrap();
        let harness = super::super::research::resolve_harness(&store, &session).unwrap();
        assert!(!harness
            .developer_instructions
            .contains("Preserve Codex base instructions"));
        assert!(fingerprints.insert(harness.fingerprint));
    }
}

#[tokio::test]
async fn streaming_http_requests_and_json_fallback_use_one_exchange() {
    for provider in [
        Provider::OpenAi,
        Provider::Local,
        Provider::Anthropic,
        Provider::Google,
    ] {
        let event = match provider {
            Provider::Anthropic => "data: {\"type\":\"message_stop\"}\n\n",
            Provider::Google => "data: {\"candidates\":[{\"finishReason\":\"STOP\"}]}\n\n",
            _ => "data: [DONE]\n\n",
        };
        let (endpoint, server) =
            server_with_type("200 OK", event.into(), "text/event-stream; charset=utf-8").await;
        let mut conn = connection(provider);
        conn.endpoint = endpoint.replace(
            "/v1/chat/completions",
            "/v1beta/models/test:generateContent",
        );
        wire::exchange(
            &wire::client().unwrap(),
            &conn,
            &json!({}),
            Some(&mut |_| {}),
        )
        .await
        .unwrap();
        let request = server.await.unwrap();
        let body: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
        if provider == Provider::Google {
            assert!(request.contains(":streamGenerateContent?alt=sse"));
            assert!(body.get("stream").is_none());
        } else {
            assert_eq!(body["stream"], true);
        }
    }
    let (endpoint,server)=server("200 OK",json!({"choices":[{"message":{"role":"assistant","content":"complete"},"finish_reason":"stop"}]}).to_string()).await;
    let mut conn = connection(Provider::Local);
    conn.endpoint = endpoint;
    let value = wire::exchange(
        &wire::client().unwrap(),
        &conn,
        &json!({}),
        Some(&mut |_| {}),
    )
    .await
    .unwrap();
    assert_eq!(
        wire::parse(Provider::Local, value).unwrap().text,
        "complete"
    );
    server.await.unwrap();
}

#[test]
fn thinking_controls_use_each_providers_wire_format() {
    for provider in [
        Provider::OpenAi,
        Provider::Local,
        Provider::Anthropic,
        Provider::Google,
    ] {
        let mut conn = connection(provider);
        conn.effort = Some("high".into());
        conn.thinking = match provider {
            Provider::Anthropic => Some(json!({"type":"adaptive"})),
            Provider::Google => Some(json!({"thinkingLevel":"high"})),
            _ => None,
        };
        let body = wire::body(&conn, "instructions", &[], &[]);
        match provider {
            Provider::Anthropic => {
                assert_eq!(body["output_config"]["effort"], "high");
                assert_eq!(body["thinking"]["type"], "adaptive");
                assert!(body.get("reasoning_effort").is_none());
            }
            Provider::Google => {
                assert_eq!(
                    body["generationConfig"]["thinkingConfig"]["thinkingLevel"],
                    "high"
                );
                assert!(body.get("reasoning_effort").is_none());
            }
            _ => assert_eq!(body["reasoning_effort"], "high"),
        }
    }
}
