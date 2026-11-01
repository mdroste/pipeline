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
        json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":""}}]}),
        json!({"choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","tool_calls":[{"id":"1","type":"function","function":{"name":"project_read","arguments":"[]"}}]}}]}),
    ] {
        assert!(wire::parse(Provider::OpenAi, response).is_err());
    }
}

#[test]
fn refusals_and_content_blocks_stay_errors_and_are_not_called_length_stops() {
    for (provider, response) in [
        (
            Provider::OpenAi,
            json!({"choices":[{"finish_reason":"content_filter","message":{"role":"assistant","content":"partial"}}]}),
        ),
        (
            Provider::Google,
            json!({"candidates":[{"finishReason":"SAFETY","content":{"parts":[{"text":"partial"}]}}]}),
        ),
        (
            Provider::Anthropic,
            json!({"stop_reason":"refusal","content":[{"type":"text","text":"partial"}]}),
        ),
    ] {
        let error = wire::parse(provider, response).err().unwrap();
        assert!(error.contains("blocked"), "{error}");
        assert!(!error.contains("length limit"), "{error}");
    }
}

/// Serves one scripted reply per connection and returns each request's body.
async fn scripted_server(
    replies: Vec<(&'static str, String)>,
) -> (String, tokio::task::JoinHandle<Vec<Value>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    let task = tokio::spawn(async move {
        let mut requests = Vec::new();
        for (status, reply) in replies {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0u8; 4096];
            let body = loop {
                let n = socket.read(&mut buffer).await.unwrap();
                bytes.extend_from_slice(&buffer[..n]);
                let text = String::from_utf8_lossy(&bytes);
                if let Some((headers, body)) = text.split_once("\r\n\r\n") {
                    let length = headers.lines().find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")?
                            .parse::<usize>()
                            .ok()
                    });
                    if body.len() >= length.unwrap_or(0) {
                        break body.to_owned();
                    }
                }
                assert!(n > 0, "the client closed the connection early");
            };
            requests.push(serde_json::from_str(&body).unwrap());
            let kind = if status.starts_with("200") {
                "text/event-stream"
            } else {
                "application/json"
            };
            socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",reply.len()).as_bytes()).await.unwrap();
        }
        requests
    });
    (endpoint, task)
}

fn sse(events: &[Value]) -> String {
    events.iter().map(|v| format!("data: {v}\n\n")).collect()
}

fn length_stop_stream(provider: Provider) -> String {
    match provider {
        Provider::Anthropic => sse(&[
            json!({"type":"message_start","message":{"usage":{"input_tokens":4}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"The long draft"}}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"call","name":"project_read","input":{}}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"pa"}}),
            json!({"type":"message_delta","delta":{"stop_reason":"max_tokens"},"usage":{"output_tokens":8}}),
            json!({"type":"message_stop"}),
        ]),
        Provider::Google => sse(&[
            json!({"candidates":[{"content":{"parts":[{"text":"The long draft"},{"functionCall":{"name":"project_read","args":{}}}]},"finishReason":"MAX_TOKENS"}]}),
        ]),
        _ => format!(
            "{}data: [DONE]\n\n",
            sse(&[
                json!({"choices":[{"index":0,"delta":{"content":"The long draft","tool_calls":[{"index":0,"id":"call","type":"function","function":{"name":"project_read","arguments":"{\"pa"}}]}}]}),
                json!({"choices":[{"index":0,"delta":{},"finish_reason":"length"}]}),
            ])
        ),
    }
}

#[tokio::test]
async fn a_length_stop_saves_the_partial_reply_and_never_runs_its_tool_calls() {
    for provider in [
        Provider::OpenAi,
        Provider::Local,
        Provider::Anthropic,
        Provider::Google,
    ] {
        let (endpoint, server) =
            scripted_server(vec![("200 OK", length_stop_stream(provider))]).await;
        let mut conn = connection(provider);
        conn.endpoint = endpoint.replace(
            "/v1/chat/completions",
            "/v1beta/models/test:generateContent",
        );
        let mut streamed = String::new();
        let mut body = wire::body(&conn, "instructions", &[], &[]);
        let value = drive::model_call(
            &wire::client().unwrap(),
            &conn,
            &mut body,
            &mut 0,
            Some(&mut |text| streamed.push_str(text)),
        )
        .await
        .unwrap();
        server.await.unwrap();
        let reply = wire::parse(provider, value).unwrap();
        assert_eq!(reply.text, streamed);
        assert!(reply.calls.is_empty());

        let temp = tempfile::tempdir().unwrap();
        let store = Store::open_at(temp.path()).unwrap();
        let session = store
            .create_session(super::super::store::CreateSessionRequest {
                workspace_id: None,
                title: "Long draft".into(),
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
        store
            .project_codex_event(&bound.id, &turn.reply_event(0, &reply))
            .unwrap();
        let error = drive::has_tool_calls(&reply).unwrap_err();
        assert!(error.contains("This reply reached the model's length limit"));

        let items = store.conversation_snapshot(&session).unwrap().items;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].payload["text"], "The long draft");
        assert_eq!(items[0].payload["phase"], "incomplete");
        assert!(items[0].is_final);
        assert!(super::super::history::recent_context(&store, &session)
            .unwrap()
            .contains("The long draft"));
        let outcome = super::super::tasks::outcome(&store, &session, "submission")
            .unwrap()
            .unwrap();
        assert!(outcome["finalText"].is_null());
    }
}

#[test]
fn a_length_stop_without_text_still_names_the_limit() {
    for (provider, response) in [
        (
            Provider::Google,
            json!({"candidates":[{"finishReason":"MAX_TOKENS","content":{"role":"model"}}]}),
        ),
        (
            Provider::OpenAi,
            json!({"choices":[{"finish_reason":"length","message":{"role":"assistant","content":null}}]}),
        ),
        (
            Provider::Anthropic,
            json!({"stop_reason":"max_tokens","content":[]}),
        ),
    ] {
        let reply = wire::parse(provider, response).ok().unwrap();
        assert!(reply.text.is_empty());
        assert!(drive::has_tool_calls(&reply)
            .unwrap_err()
            .contains("length limit"));
    }
}

#[test]
fn reply_ceilings_follow_each_providers_format() {
    let mut anthropic = connection(Provider::Anthropic);
    anthropic.effort = Some("high".into());
    let body = wire::body(&anthropic, "instructions", &[], &[]);
    assert_eq!(body["max_tokens"], wire::OUTPUT_LIMITS[0]);
    let mut google = connection(Provider::Google);
    google.effort = Some("high".into());
    google.thinking = Some(json!({"thinkingLevel":"high"}));
    let body = wire::body(&google, "instructions", &[], &[]);
    assert_eq!(
        body["generationConfig"]["maxOutputTokens"],
        wire::OUTPUT_LIMITS[0]
    );
    // OpenAI and local servers apply each model's own maximum.
    for provider in [Provider::OpenAi, Provider::Local] {
        let body = wire::body(&connection(provider), "instructions", &[], &[]);
        assert!(body.get("max_tokens").is_none());
        assert!(body.get("max_completion_tokens").is_none());
    }
}

#[tokio::test]
async fn a_refused_reply_ceiling_steps_down_and_stays_down_for_the_turn() {
    let refusal = r#"{"error":{"message":"max_tokens: 32000 > 16384, which is the maximum allowed number of output tokens for test-model"}}"#;
    let done = sse(&[
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":"Answer"}}),
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{}}),
        json!({"type":"message_stop"}),
    ]);
    let (endpoint, server) = scripted_server(vec![
        ("400 Bad Request", refusal.into()),
        ("200 OK", done.clone()),
        ("200 OK", done),
    ])
    .await;
    let mut conn = connection(Provider::Anthropic);
    conn.endpoint = endpoint;
    let client = wire::client().unwrap();
    let mut ceiling = 0;
    let mut text = String::new();
    for _ in 0..2 {
        let mut body = wire::body(&conn, "instructions", &[], &[]);
        drive::model_call(
            &client,
            &conn,
            &mut body,
            &mut ceiling,
            Some(&mut |delta| text.push_str(delta)),
        )
        .await
        .unwrap();
    }
    assert_eq!(text, "AnswerAnswer");
    let sent: Vec<_> = server
        .await
        .unwrap()
        .iter()
        .map(|request| request["max_tokens"].as_u64().unwrap())
        .collect();
    assert_eq!(sent, [32_000, 16_384, 16_384]);
}

#[tokio::test]
async fn an_unrelated_400_is_not_resent_and_does_not_echo_its_body() {
    let (endpoint, server) = scripted_server(vec![(
        "400 Bad Request",
        r#"{"error":{"message":"secret max_tokens must be positive"}}"#.into(),
    )])
    .await;
    let mut conn = connection(Provider::Anthropic);
    conn.endpoint = endpoint;
    let mut body = wire::body(&conn, "instructions", &[], &[]);
    let error = drive::model_call(&wire::client().unwrap(), &conn, &mut body, &mut 0, None)
        .await
        .unwrap_err();
    assert!(error.contains("HTTP 400"));
    assert!(!error.contains("secret"));
    assert_eq!(server.await.unwrap().len(), 1);
}

#[tokio::test]
async fn every_ceiling_refused_ends_with_a_plain_message() {
    let refusal = r#"{"error":{"message":"maxOutputTokens is out of range"}}"#;
    let (endpoint, server) = scripted_server(vec![
        ("400 Bad Request", refusal.into());
        wire::OUTPUT_LIMITS.len()
    ])
    .await;
    let mut conn = connection(Provider::Google);
    conn.endpoint = endpoint;
    let mut body = wire::body(&conn, "instructions", &[], &[]);
    let error = drive::model_call(&wire::client().unwrap(), &conn, &mut body, &mut 0, None)
        .await
        .unwrap_err();
    assert_eq!(error, wire::OUTPUT_LIMIT_REFUSED);
    let sent: Vec<_> = server
        .await
        .unwrap()
        .iter()
        .map(|request| {
            request["generationConfig"]["maxOutputTokens"]
                .as_u64()
                .unwrap() as u32
        })
        .collect();
    assert_eq!(sent, wire::OUTPUT_LIMITS);
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

/// Answers one streamed request, writing each chunk after `pause`, and holds
/// the connection for `linger` before it ends the reply.
async fn trickling_server(
    chunks: Vec<String>,
    pause: std::time::Duration,
    linger: std::time::Duration,
) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 4096];
        while !String::from_utf8_lossy(&bytes).contains("test-model") {
            let n = socket.read(&mut buffer).await.unwrap();
            assert!(n > 0, "the client closed the connection early");
            bytes.extend_from_slice(&buffer[..n]);
        }
        let head =
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";
        socket.write_all(head.as_bytes()).await.unwrap();
        for chunk in chunks {
            tokio::time::sleep(pause).await;
            socket.write_all(chunk.as_bytes()).await.unwrap();
        }
        tokio::time::sleep(linger).await;
    });
    endpoint
}

#[tokio::test]
async fn a_slow_model_keeps_going_while_its_reply_keeps_arriving() {
    use std::time::Duration;
    let word = |text: &str| sse(&[json!({"choices":[{"delta":{"content":text}}]})]);
    let mut chunks: Vec<String> = (0..8).map(|_| word("word ")).collect();
    chunks.push(sse(&[
        json!({"choices":[{"delta":{},"finish_reason":"stop"}]}),
    ]));
    chunks.push("data: [DONE]\n\n".into());
    let mut conn = connection(Provider::Local);
    // The whole reply takes about 1.5 seconds, three times the silence limit.
    let pause = Duration::from_millis(150);
    conn.endpoint = trickling_server(chunks, pause, Duration::ZERO).await;
    let client = wire::client_with_silence(Duration::from_millis(500)).unwrap();
    let mut text = String::new();
    let body = wire::body(&conn, "instructions", &[], &[]);
    wire::exchange(
        &client,
        &conn,
        &body,
        Some(&mut |delta: &str| text.push_str(delta)),
    )
    .await
    .unwrap();
    assert_eq!(text, "word ".repeat(8));
}

#[tokio::test]
async fn a_silent_model_is_reported_as_silent_and_not_as_a_dropped_connection() {
    use std::time::Duration;
    let first = sse(&[json!({"choices":[{"delta":{"content":"word "}}]})]);
    let mut conn = connection(Provider::Local);
    let pause = Duration::from_millis(50);
    conn.endpoint = trickling_server(vec![first], pause, Duration::from_secs(5)).await;
    let client = wire::client_with_silence(Duration::from_millis(300)).unwrap();
    let body = wire::body(&conn, "instructions", &[], &[]);
    let error = wire::exchange(&client, &conn, &body, Some(&mut |_: &str| ()))
        .await
        .unwrap_err();
    assert!(error.contains("sent nothing for 10 minutes"), "{error}");
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

/// One streamed reply with `calls` as (name, arguments), or text when empty.
fn scripted_reply(calls: &[(&str, &str)]) -> String {
    let tool_calls: Vec<Value> = calls
        .iter()
        .enumerate()
        .map(|(index, (name, arguments))| {
            json!({"index":index,"id":format!("call-{name}-{index}"),"type":"function","function":{"name":name,"arguments":arguments}})
        })
        .collect();
    let (delta, finish) = if calls.is_empty() {
        (json!({"content":"Finished"}), "stop")
    } else {
        (json!({"tool_calls":tool_calls}), "tool_calls")
    };
    format!(
        "{}data: [DONE]\n\n",
        sse(&[
            json!({"choices":[{"index":0,"delta":delta}]}),
            json!({"choices":[{"index":0,"delta":{},"finish_reason":finish}]}),
        ])
    )
}

/// A saved conversation with a recorded API turn, on the store turns write to.
fn scripted_turn(label: &str) -> (Store, String, Turn) {
    let store = crate::orchestration::tests::disposable_workspace();
    let session = store
        .create_session(super::super::store::CreateSessionRequest {
            workspace_id: None,
            title: "Scripted turn".into(),
            operation_id: format!("scripted-turn-{label}"),
        })
        .unwrap()
        .record
        .id;
    let thread = format!("api-thread-{label}");
    let id = format!("api-turn-{label}");
    let bound = store.bind_session(&session, NAMESPACE, &thread).unwrap();
    store
        .record_turn_submission(&bound.id, &format!("submission-{label}"), &id)
        .unwrap();
    let turn = Turn {
        epoch: next_epoch(),
        thread,
        id,
        binding: bound.id,
        model: None,
        runtime: Provider::Local.runtime().into(),
        usage: Arc::new(Mutex::new(Vec::new())),
    };
    (store, session, turn)
}

/// Runs through the host's research tools, which turn this test's unbound
/// caller away, so reaching that refusal shows a call passed its checks.
fn read_tool() -> Value {
    json!({"name":"scripted_read","description":"Read a file","inputSchema":{
        "type":"object",
        "properties":{"path":{"type":"string"}},
        "required":["path"],
        "additionalProperties":false
    }})
}

fn tool_items(store: &Store, session: &str) -> Vec<Value> {
    store
        .conversation_snapshot(session)
        .unwrap()
        .items
        .into_iter()
        .filter(|item| item.payload["type"] == "toolCall")
        .map(|item| item.payload)
        .collect()
}

#[tokio::test]
async fn a_tool_mistake_goes_back_to_the_model_and_the_turn_continues() {
    let (endpoint, server) = scripted_server(vec![
        (
            "200 OK",
            scripted_reply(&[
                ("scripted_read", r#"{"path":"a.tex","extra":true}"#),
                ("scripted_read", "{}"),
                ("project_delete_everything", r#"{"path":"a.tex"}"#),
                ("scripted_read", r#"{"path":"a.tex"}"#),
            ]),
        ),
        ("200 OK", scripted_reply(&[])),
    ])
    .await;
    let mut conn = connection(Provider::Local);
    conn.endpoint = endpoint;
    let (store, session, turn) = scripted_turn("tool-mistake");
    let (_cancel, receiver) = watch::channel(false);
    let events = super::super::turn_loop::RecordedEvents::default();
    Box::pin(drive::run(
        &wire::client().unwrap(),
        &conn,
        &events,
        &turn,
        "instructions",
        "question",
        &[read_tool()],
        drive::turn_clock(),
        receiver,
    ))
    .await
    .unwrap();

    // The model saw each failure and could correct its call.
    let requests = server.await.unwrap();
    let sent = requests[1].to_string();
    assert!(sent.contains("do not match its schema"), "{sent}");
    assert!(sent.contains("not enabled for this conversation"), "{sent}");
    assert!(sent.contains("not bound to an active Workspace"), "{sent}");

    let items = tool_items(&store, &session);
    assert_eq!(items.len(), 4);
    assert!(items.iter().all(|item| item["status"] == "failed"));
    assert_eq!(items[2]["tool"], "project_delete_everything");
}

#[tokio::test]
async fn a_tool_outlasting_the_model_limit_does_not_end_the_turn() {
    let (endpoint, server) = scripted_server(vec![
        (
            "200 OK",
            scripted_reply(&[("scripted_read", r#"{"path":"a.tex"}"#)]),
        ),
        ("200 OK", scripted_reply(&[])),
    ])
    .await;
    let mut conn = connection(Provider::Local);
    conn.endpoint = endpoint;
    let (_store, _session, turn) = scripted_turn("slow-tool");
    let (_cancel, receiver) = watch::channel(false);
    let events = super::super::turn_loop::RecordedEvents {
        tool_delay: std::time::Duration::from_millis(1500),
        ..Default::default()
    };
    let result = Box::pin(drive::run(
        &wire::client().unwrap(),
        &conn,
        &events,
        &turn,
        "instructions",
        "question",
        &[read_tool()],
        crate::workbench::turn_loop::ModelClock::new(
            std::time::Duration::from_secs(1),
            std::time::Duration::from_secs(60),
        ),
        receiver,
    ))
    .await;
    assert_eq!(result, Ok(()));
    assert_eq!(server.await.unwrap().len(), 2);
}

#[tokio::test]
async fn a_tool_past_the_host_allowance_ends_the_turn_in_plain_words() {
    let (endpoint, server) = scripted_server(vec![(
        "200 OK",
        scripted_reply(&[("scripted_read", r#"{"path":"a.tex"}"#)]),
    )])
    .await;
    let mut conn = connection(Provider::Local);
    conn.endpoint = endpoint;
    let (_store, _session, turn) = scripted_turn("host-limit");
    let (_cancel, receiver) = watch::channel(false);
    let events = super::super::turn_loop::RecordedEvents::default();
    let error = Box::pin(drive::run(
        &wire::client().unwrap(),
        &conn,
        &events,
        &turn,
        "instructions",
        "question",
        &[read_tool()],
        crate::workbench::turn_loop::ModelClock::new(
            std::time::Duration::from_secs(30),
            std::time::Duration::ZERO,
        ),
        receiver,
    ))
    .await
    .unwrap_err();
    assert!(error.contains("4 hours"), "{error}");
    server.await.unwrap();
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
