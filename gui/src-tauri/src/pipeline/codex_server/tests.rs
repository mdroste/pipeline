use super::{
    connection::{Connection, PROFILE},
    invocation::run_connected,
};
use crate::agent_runtime::codex::AppServerClient;
use crate::pipeline::providers::LlmOverrides;
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
/// The next reply from the call, answering any thread/read reconciliation
/// a lagged call issues meanwhile with a still-running turn.
async fn receive_reply(
    reader: &mut BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
    writer: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>,
) -> Value {
    loop {
        let frame = receive(reader).await;
        if frame["method"] != "thread/read" {
            return frame;
        }
        send(writer, json!({"id":frame["id"],"result":{"thread":{"id":"thread-1","turns":[{"id":"turn-1","status":"inProgress","items":[]}]}}})).await;
    }
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
    let (ready, active_turn) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let mut ready = Some(ready);
        let (read, mut write) = tokio::io::split(server_io);
        let mut read = BufReader::new(read);
        let mut starts = 0;
        let mut completed = false;
        loop {
            let request = receive(&mut read).await;
            let result = match request["method"].as_str().unwrap() {
                "thread/read" => {
                    assert_eq!(mode, "lagged", "only a lagged call reconciles");
                    let turn = if completed {
                        json!({"id":"turn-1","status":"completed","items":[{"id":"comment","type":"agentMessage","phase":"commentary","text":"Working"},{"id":"final","type":"agentMessage","phase":"final_answer","text":"Report"}]})
                    } else {
                        json!({"id":"turn-1","status":"inProgress","items":[]})
                    };
                    json!({"thread":{"id":"thread-1","turns":[turn]}})
                }
                "account/read" => {
                    json!({"account":{"type":"chatgpt","email":"fixture@example.invalid","planType":"plus"},"requiresOpenaiAuth":true})
                }
                "model/list" => {
                    json!({"data":[{"id":"fixture","model":"fixture","displayName":"Fixture","description":"Test","isDefault":true,"defaultReasoningEffort":"high","supportedReasoningEfforts":[{"reasoningEffort":"high","description":"High"}]}],"nextCursor":null})
                }
                "thread/start" => {
                    if mode == "cancel_before_start" {
                        // Hold the reply: the call has a prepared receipt but
                        // cannot possibly submit a turn yet.
                        ready.take().unwrap().send(()).unwrap();
                        continue;
                    }
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
                    if mode == "lagged" {
                        // Sibling reviewer threads stream far more deltas than the
                        // shared event queue holds while this call is not draining
                        // (it is still awaiting this turn/start reply).
                        for index in 0..5_000 {
                            send(&mut write, json!({"method":"item/agentMessage/delta","params":{"threadId":"other","turnId":"other","itemId":"other","delta":format!("token {index}")}})).await;
                        }
                    }
                    send(
                        &mut write,
                        json!({"id":request["id"],"result":{"turn":{"id":"turn-1"}}}),
                    )
                    .await;
                    if mode == "disconnect" {
                        return;
                    }
                    if mode == "cancel" {
                        // A tool acknowledgement proves the caller processed
                        // turn/start and owns the exact active turn before it
                        // is dropped. Server-side send alone is not enough.
                        send(&mut write, json!({"id":"ready-tool","method":"item/tool/call","params":{"threadId":"thread-1","turnId":"turn-1","callId":"ready-call","tool":"Write","arguments":{"file_path":"note.txt","content":"Evidence"}}})).await;
                        assert_eq!(
                            receive_reply(&mut read, &mut write).await["result"]["success"],
                            true
                        );
                        ready.take().unwrap().send(()).unwrap();
                        continue;
                    }
                    // Replayed dynamic requests produce the same acknowledgement,
                    // and the handler never trusts another thread's final output.
                    let tool = json!({"id":"tool-1","method":"item/tool/call","params":{"threadId":"thread-1","turnId":"turn-1","callId":"call-1","tool":"Write","arguments":{"file_path":"note.txt","content":"Evidence"}}});
                    send(&mut write, tool.clone()).await;
                    let first = receive_reply(&mut read, &mut write).await;
                    assert_eq!(first["result"]["success"], true);
                    send(&mut write, tool).await;
                    assert_eq!(receive_reply(&mut read, &mut write).await, first);
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
                    completed = true;
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
    let result = if mode == "cancel" || mode == "cancel_before_start" {
        let mut operation = Box::pin(operation);
        tokio::select! {
            result = &mut operation => panic!("turn completed before cancellation: {result:?}"),
            ready = tokio::time::timeout(Duration::from_secs(20), active_turn) => {
                ready.expect("active turn never acknowledged").unwrap();
            }
        }
        drop(operation);
        None
    } else {
        Some(
            tokio::time::timeout(Duration::from_secs(20), operation)
                .await
                .unwrap(),
        )
    };
    match mode {
        "success" | "lagged" => {
            assert_eq!(result.unwrap().unwrap(), "Report");
            assert_eq!(
                std::fs::read_to_string(output.path().join("note.txt")).unwrap(),
                "Evidence"
            );
        }
        "disconnect" => assert!(crate::pipeline::provider_error::is_non_retryable_error(
            &result.unwrap().unwrap_err()
        )),
        "cancel" | "cancel_before_start" => assert!(result.is_none()),
        _ => unreachable!(),
    }
    if mode != "cancel_before_start" {
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap();
    } else {
        // Dropping a prepared call releases ownership without inventing a
        // turn/interrupt for a turn that has not been submitted.
        let _released = tokio::time::timeout(Duration::from_secs(5), c.activity.write())
            .await
            .unwrap();
        server.abort();
        assert!(server.await.unwrap_err().is_cancelled());
    }
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
    if mode == "cancel_before_start" {
        assert!(record["turn_id"].is_null());
        assert_eq!(record["state"], "not_submitted");
        return;
    }
    assert_eq!(record["turn_id"], "turn-1");
    if mode == "success" {
        assert_eq!(record["output"], "Report");
        assert_eq!(record["usage"]["input_tokens"], 100);
    } else if mode == "lagged" {
        assert_eq!(record["output"], "Report");
        assert!(
            record["state"] == "completed" || record["state"] == "completed_reconciled",
            "{}",
            record["state"]
        );
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
#[tokio::test]
async fn dropping_before_turn_submission_records_no_turn_to_interrupt() {
    scenario("cancel_before_start").await;
}
#[tokio::test]
async fn lagged_event_stream_reconciles_instead_of_failing_the_turn() {
    scenario("lagged").await;
}
