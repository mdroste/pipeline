//! Model fixtures exercise the real Review loop, including its request catalog,
//! tool messages, usage and error paths. The ignored probe uses real public web
//! transport with fixture models, never credentials or paid model calls.
use super::*;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub(in crate::pipeline) async fn server(
    responses: Vec<(u16, Value)>,
) -> (String, tokio::task::JoinHandle<Vec<Value>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut requests = Vec::new();
        for (status, body) in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut buffer = [0u8; 4096];
                let count = socket.read(&mut buffer).await.unwrap();
                assert_ne!(count, 0);
                bytes.extend_from_slice(&buffer[..count]);
                assert!(bytes.len() < 2 * 1024 * 1024);
                let text = String::from_utf8_lossy(&bytes);
                if let Some((headers, body)) = text.split_once("\r\n\r\n") {
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|value| value.parse::<usize>().ok())
                        })
                        .unwrap();
                    if body.len() >= length {
                        requests.push(serde_json::from_str(body).unwrap());
                        break;
                    }
                }
            }
            let body = body.to_string();
            socket.write_all(format!("HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
        requests
    });
    (url, task)
}

fn call(name: &str, args: Value) -> Value {
    json!({"choices":[{"finish_reason":"tool_calls","message":{
        "role":"assistant", "content":null,"tool_calls":[{
            "id":format!("call-{name}"),"type":"function",
            "function":{"name":name,"arguments":args.to_string()}
        }]
    }}],"usage":{"prompt_tokens":10,"completion_tokens":4}})
}

fn answer() -> Value {
    json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Answer with source https://example.com/"}}],"usage":{"prompt_tokens":5,"completion_tokens":3}})
}

async fn run(url: &str, provider: &str, tools: Vec<Value>) -> Result<(String, Usage), String> {
    let app: crate::emit::EventBus = std::sync::Arc::new(crate::emit::CliEvents);
    let request = OpenAIRequest {
        model: "fixture".into(),
        messages: vec![OpenAIMessage {
            role: "user".into(),
            content: Some(json!("Review and cite sources")),
            tool_calls: None,
            tool_call_id: None,
        }],
        tools,
        reasoning_effort: None,
        max_completion_tokens: None,
        prompt_cache_key: None,
        response_format: None,
    };
    let scope = crate::run_scope::ActiveScope::begin(format!("web-fixture-{provider}"));
    crate::run_scope::scoped(
        scope.scope().clone(),
        openai_tool_loop(
            &app,
            &LOCAL_HTTP_CLIENT,
            url,
            "",
            provider,
            request,
            120,
            "web fixture",
            provider == "Local",
            false,
            &ToolAccess::default(),
        ),
    )
    .await
}

#[tokio::test]
async fn review_web_tool_errors_round_trip_for_both_providers() {
    for provider in ["OpenAI", "Local"] {
        let (url, task) = server(vec![
            (200, call("WebSearch", json!({"query":""}))),
            (200, call("WebFetch", json!({"url":"file:///private"}))),
            (200, answer()),
        ])
        .await;
        let (_, usage) = run(&url, provider, web::tools()).await.unwrap();
        assert_eq!(usage.tool_calls.web, 2);
        assert_eq!(usage.requests, 3);
        let requests = task.await.unwrap();
        assert_eq!(requests[0]["tools"], json!(web::tools()));
        assert_eq!(requests[1]["messages"][2]["role"], "tool");
        assert!(requests[1]["messages"][2]["content"]
            .as_str()
            .unwrap()
            .contains("query"));
        assert!(requests[2]["messages"][4]["content"]
            .as_str()
            .unwrap()
            .contains("HTTP"));
    }
}

#[tokio::test]
async fn disabled_search_and_fetch_are_denied_before_network() {
    for name in ["WebSearch", "WebFetch"] {
        let (url, task) = server(vec![
            (
                200,
                call(
                    name,
                    json!({"query":"macroeconomics", "url":"https://example.com"}),
                ),
            ),
            (200, answer()),
        ])
        .await;
        run(&url, "Local", Vec::new()).await.unwrap();
        let requests = task.await.unwrap();
        assert!(requests[0].get("tools").is_none());
        assert!(requests[1]["messages"][2]["content"]
            .as_str()
            .unwrap()
            .contains("not enabled"));
    }
}

#[tokio::test]
async fn local_tool_rejection_does_not_silently_remove_search() {
    let (url, task) = server(vec![(
        400,
        json!({"error":{"message":"model does not support tools"}}),
    )])
    .await;
    let error = run(&url, "Local", web::tools()).await.unwrap_err();
    assert!(error.contains("tool-capable model"), "{error}");
    assert_eq!(task.await.unwrap().len(), 1);
}

#[tokio::test]
#[ignore = "Requires public internet; fixture models, no paid model calls"]
async fn review_web_live_search_and_fetch_round_trip_both_providers() {
    for provider in ["OpenAI", "Local"] {
        let (url, task) = server(vec![
            (
                200,
                call("WebSearch", json!({"query":"macroeconomics research"})),
            ),
            (200, call("WebFetch", json!({"url":"https://example.com/"}))),
            (200, answer()),
        ])
        .await;
        let (text, usage) = run(&url, provider, web::tools()).await.unwrap();
        assert!(text.contains("https://example.com"));
        assert_eq!(usage.tool_calls.web, 2);
        let requests = task.await.unwrap();
        let search: Value =
            serde_json::from_str(requests[1]["messages"][2]["content"].as_str().unwrap()).unwrap();
        assert_eq!(search["provider"], "Bing");
        assert!(!search["results"].as_array().unwrap().is_empty());
        assert!(search["results"][0]["url"]
            .as_str()
            .unwrap()
            .starts_with("http"));
        let page: Value =
            serde_json::from_str(requests[2]["messages"][4]["content"].as_str().unwrap()).unwrap();
        assert!(page["text"].as_str().unwrap().contains("Example Domain"));
        assert!(page["authority"].as_str().unwrap().contains("Untrusted"));
    }
}
