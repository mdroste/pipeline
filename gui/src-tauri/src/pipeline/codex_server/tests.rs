use super::{
    connection::{Connection, PROFILE},
    invocation::run_connected,
};
use crate::agent_runtime::codex::AppServerClient;
use crate::pipeline::claude::LlmOverrides;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

async fn receive(reader: &mut BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>) -> Value {
    let mut line = String::new();
    assert!(reader.read_line(&mut line).await.unwrap() > 0);
    serde_json::from_str(&line).unwrap()
}
async fn send(writer: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>, value: Value) {
    writer
        .write_all(format!("{value}\n").as_bytes())
        .await
        .unwrap();
}

// Runs the production submission/event/host-tool code against a scripted
// transport. No account, installed CLI, network or billable turn is used.
async fn scenario(mode: &'static str) {
    let root = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let (client_io, server_io) = tokio::io::duplex(256 * 1024);
    let (read, write) = tokio::io::split(client_io);
    let c = Connection::simulated(root.path(), AppServerClient::from_io(7, read, write));
    let cwd = c.cwd.clone();
    let server = tokio::spawn(async move {
        let (read, mut write) = tokio::io::split(server_io);
        let mut read = BufReader::new(read);
        let mut starts = 0;
        loop {
            let request = receive(&mut read).await;
            let result = match request["method"].as_str().unwrap() {
                "account/read" => {
                    json!({"account":{"type":"chatgpt","email":"fixture@example.invalid","planType":"plus"},"requiresOpenaiAuth":true})
                }
                "model/list" => {
                    json!({"data":[{"id":"fixture","model":"fixture","displayName":"Fixture","description":"Test","isDefault":true,"defaultReasoningEffort":"high","supportedReasoningEfforts":[{"reasoningEffort":"high","description":"High"}]}],"nextCursor":null})
                }
                "thread/start" => {
                    assert_eq!(
                        request["params"]["developerInstructions"],
                        "Review evidence carefully."
                    );
                    assert_eq!(request["params"]["runtimeWorkspaceRoots"], json!([]));
                    assert_eq!(request["params"]["config"]["web_search"], "disabled");
                    assert_eq!(
                        request["params"]["dynamicTools"].as_array().unwrap().len(),
                        1
                    );
                    json!({"thread":{"id":"thread-1"},"model":"fixture","modelProvider":"openai","cwd":cwd,"runtimeWorkspaceRoots":[],"instructionSources":[],"activePermissionProfile":{"id":PROFILE},"approvalPolicy":"never","approvalsReviewer":"user","sandbox":{"type":"readOnly"}})
                }
                "turn/start" => {
                    starts += 1;
                    assert_eq!(starts, 1, "an accepted turn must never be submitted twice");
                    assert_eq!(request["params"]["effort"], "high");
                    assert_eq!(request["params"]["input"][0]["text"], "Review the fixture.");
                    send(
                        &mut write,
                        json!({"id":request["id"],"result":{"turn":{"id":"turn-1"}}}),
                    )
                    .await;
                    if mode == "disconnect" {
                        return;
                    }
                    if mode == "cancel" {
                        continue;
                    }
                    // Replayed dynamic requests produce the same acknowledgement,
                    // and the handler never trusts another thread's final output.
                    let tool = json!({"id":"tool-1","method":"item/tool/call","params":{"threadId":"thread-1","turnId":"turn-1","callId":"call-1","tool":"Write","arguments":{"file_path":"note.txt","content":"Evidence"}}});
                    send(&mut write, tool.clone()).await;
                    let first = receive(&mut read).await;
                    assert_eq!(first["result"]["success"], true);
                    send(&mut write, tool).await;
                    assert_eq!(receive(&mut read).await, first);
                    send(&mut write, json!({"method":"item/completed","params":{"threadId":"other","turnId":"other","item":{"id":"foreign","type":"agentMessage","phase":"final_answer","text":"WRONG"}}})).await;
                    for (id, phase, text) in [
                        ("comment", "commentary", "Working"),
                        ("final", "final_answer", "Report"),
                    ] {
                        send(&mut write, json!({"method":"item/completed","params":{"threadId":"thread-1","turnId":"turn-1","item":{"id":id,"type":"agentMessage","phase":phase,"text":text}}})).await;
                    }
                    for input in [50, 100] {
                        send(&mut write, json!({"method":"thread/tokenUsage/updated","params":{"threadId":"thread-1","turnId":"turn-1","tokenUsage":{"total":{"inputTokens":input,"outputTokens":20,"cachedInputTokens":10}}}})).await;
                    }
                    send(&mut write, json!({"method":"turn/completed","params":{"threadId":"thread-1","turn":{"id":"turn-1","status":"completed"}}})).await;
                    continue;
                }
                "turn/interrupt" => {
                    assert_eq!(mode, "cancel");
                    assert_eq!(
                        request["params"],
                        json!({"threadId":"thread-1","turnId":"turn-1"})
                    );
                    send(&mut write, json!({"id":request["id"],"result":{}})).await;
                    send(&mut write, json!({"method":"turn/completed","params":{"threadId":"thread-1","turn":{"id":"turn-1","status":"interrupted"}}})).await;
                    return;
                }
                "thread/unsubscribe" => {
                    send(&mut write, json!({"id":request["id"],"result":{}})).await;
                    return;
                }
                method => panic!("Unexpected request {method}"),
            };
            send(&mut write, json!({"id":request["id"],"result":result})).await;
        }
    });
    let app: crate::emit::EventBus = Arc::new(crate::emit::NullEvents);
    let overrides = LlmOverrides {
        model: Some("fixture"),
        effort: Some("high"),
        write_dir: output.path().to_str(),
        ..Default::default()
    };
    let operation = run_connected(
        c.clone(),
        &app,
        "Review the fixture.",
        &["Write"],
        Some("Review evidence carefully."),
        "Fixture",
        &[],
        &overrides,
    );
    let result = tokio::time::timeout(
        if mode == "cancel" {
            Duration::from_millis(100)
        } else {
            Duration::from_secs(5)
        },
        operation,
    )
    .await;
    match mode {
        "success" => {
            assert_eq!(result.unwrap().unwrap(), "Report");
            assert_eq!(
                std::fs::read_to_string(output.path().join("note.txt")).unwrap(),
                "Evidence"
            );
        }
        "disconnect" => assert!(crate::pipeline::provider_error::is_non_retryable_error(
            &result.unwrap().unwrap_err()
        )),
        "cancel" => assert!(result.is_err()),
        _ => unreachable!(),
    }
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
    // Cleanup retains ownership until interruption/reconciliation is finished.
    let _released = tokio::time::timeout(Duration::from_secs(5), c.activity.write())
        .await
        .unwrap();
    let records: Vec<_> = std::fs::read_dir(&c.attempts).unwrap().collect();
    assert_eq!(records.len(), 1);
    let record: Value =
        serde_json::from_slice(&std::fs::read(records[0].as_ref().unwrap().path()).unwrap())
            .unwrap();
    assert_eq!(record["instructions"], "Review evidence carefully.");
    assert_eq!(record["turn_id"], "turn-1");
    if mode == "success" {
        assert_eq!(record["output"], "Report");
        assert_eq!(record["usage"]["input_tokens"], 100);
    } else {
        assert_eq!(record["state"], "outcome_unknown");
    }
}

#[tokio::test]
async fn native_invocation_preserves_tools_instructions_identity_and_usage() {
    scenario("success").await;
}
#[tokio::test]
async fn accepted_disconnect_is_durable_and_not_retryable() {
    scenario("disconnect").await;
}
#[tokio::test]
async fn dropping_a_call_interrupts_its_exact_turn() {
    scenario("cancel").await;
}
