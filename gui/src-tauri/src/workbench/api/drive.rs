//! API turn loop with Workspace-owned tools and durable events. Each response
//! is processed once, and call IDs are host-assigned from the accepted turn.
use super::*;
use std::{collections::HashSet, time::Duration};

#[allow(clippy::too_many_arguments)]
pub(super) async fn run(
    client: &reqwest::Client,
    connection: &Connection,
    app: &tauri::AppHandle,
    turn: &Turn,
    instructions: &str,
    prompt: &str,
    specs: &[Value],
    cancel: watch::Receiver<bool>,
) -> Result<(), String> {
    let work = async {
        turn.emit(
            app,
            NormalizedEvent::TurnStarted {
                epoch: turn.epoch,
                thread_id: turn.thread.clone(),
                turn_id: turn.id.clone(),
                turn: json!({"id":turn.id,"status":"inProgress"}),
            },
        )
        .await?;
        let mut messages = wire::initial(connection.provider, instructions, prompt);
        let mut calls = 0;
        let mut seen = HashSet::new();
        for round in 0..32 {
            let body = wire::body(connection, instructions, &messages, specs);
            let reply = wire::parse(
                connection.provider,
                wire::exchange(
                    client,
                    connection,
                    &body,
                    Some(&mut |text| {
                        let _ = app.emit(
                            "workbench:event",
                            NormalizedEvent::AgentMessageDelta {
                                epoch: turn.epoch,
                                thread_id: turn.thread.clone(),
                                turn_id: turn.id.clone(),
                                item_id: format!("{}-assistant-{round}", turn.id),
                                delta: text.into(),
                            },
                        );
                    }),
                )
                .await?,
            )?;
            turn.usage
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(reply.usage.clone());
            let binding = turn.binding.clone();
            let usage_event = NormalizedEvent::ItemCompleted {
                epoch: turn.epoch,
                thread_id: turn.thread.clone(),
                turn_id: turn.id.clone(),
                item_id: format!("{}-usage-{round}", turn.id),
                item_kind: "apiUsage".into(),
                item: json!({"usage":reply.usage,"runtime":turn.runtime,"model":connection.model}),
            };
            run_store(move |store| store.project_codex_event(&binding, &usage_event))
                .await
                .map_err(|e| e.message)?;
            if !reply.text.is_empty() {
                turn.emit(app, turn.reply_event(round, &reply)).await?;
            }
            if reply.calls.is_empty() {
                return Ok(());
            }
            calls += reply.calls.len();
            if calls > 128 {
                return Err("The conversation reached its tool-call limit".into());
            }
            messages.push(reply.message);
            let mut results = Vec::new();
            for (index, call) in reply.calls.iter().enumerate() {
                // Gemini may omit IDs. Our round-scoped IDs remain unique while
                // repeated provider IDs on other APIs fail without replay.
                if connection.provider != Provider::Google && !seen.insert(call.id.clone()) {
                    return Err(
                        "The model repeated a tool call identifier; it was not run again".into(),
                    );
                }
                let id = format!("{}-tool-{round}-{index}", turn.id);
                let result = execute(app, turn, &id, call, specs).await?;
                results.push((call, result));
            }
            messages.extend(wire::tool_results(connection.provider, results));
        }
        Err("The conversation reached its model-call limit".into())
    };
    cancellable(work, cancel).await
}

pub(super) async fn cancellable<T>(
    work: impl std::future::Future<Output = Result<T, String>>,
    mut cancel: watch::Receiver<bool>,
) -> Result<T, String> {
    tokio::select! {
        biased;
        _ = cancel.changed() => Err("interrupted".into()),
        result = tokio::time::timeout(Duration::from_secs(3600),work) => result.unwrap_or_else(|_|Err("The API turn timed out; it was not retried".into())),
    }
}

async fn execute(
    app: &tauri::AppHandle,
    turn: &Turn,
    id: &str,
    call: &wire::Call,
    specs: &[Value],
) -> Result<Value, String> {
    let Some(spec) = specs.iter().find(|s| s["name"] == call.name) else {
        return Err("The model requested a tool that is not enabled".into());
    };
    let validator = jsonschema::validator_for(&spec["inputSchema"])
        .map_err(|_| "Invalid conversation tool schema")?;
    if !validator.is_valid(&call.arguments) {
        return Err("The model supplied invalid tool arguments".into());
    }
    let item = json!({"id":id,"type":"toolCall","runtime":turn.runtime,"tool":call.name,"input":super::super::search::prefix(&call.arguments.to_string(),32*1024),"status":"inProgress"});
    turn.emit(
        app,
        NormalizedEvent::ItemStarted {
            epoch: turn.epoch,
            thread_id: turn.thread.clone(),
            turn_id: turn.id.clone(),
            item_id: id.into(),
            item_kind: "toolCall".into(),
            item: item.clone(),
        },
    )
    .await?;
    let params = json!({"threadId":turn.thread,"turnId":turn.id,"callId":id,"tool":call.name,"arguments":call.arguments});
    let response = if super::super::conversation_tools::owns(&call.name) {
        super::super::conversation_tools::handle(params).await
    } else {
        run_store(move |store| super::super::research::handle_dynamic_tool_call(&store, &params))
            .await
    };
    super::super::research::jobs::launch_pending();
    let mut response = response.unwrap_or_else(
        |e| json!({"success":false,"contentItems":[{"type":"inputText","text":e.message}]}),
    );
    if call.name == "workbench_research_run" && response["success"] == true {
        if let Some(id) = response["contentItems"][0]["text"]
            .as_str()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .and_then(|v| v["id"].as_str().map(str::to_owned))
        {
            while super::super::research::jobs::execution_active(&id) {
                tokio::time::sleep(Duration::from_millis(75)).await;
            }
            response = run_store(move |store| {
                super::super::research::jobs::complete_tool_result(&store, &id)
            })
            .await
            .map_err(|e| e.message)?;
        }
    }
    if response.to_string().len() > wire::MAX_BODY / 2 {
        return Err("The tool result exceeded 4 MB".into());
    }
    let mut completed = item;
    completed["status"] = json!(if response["success"] == true {
        "completed"
    } else {
        "failed"
    });
    completed["output"] = json!(super::super::search::prefix(
        &response.to_string(),
        32 * 1024
    ));
    turn.emit(
        app,
        NormalizedEvent::ItemCompleted {
            epoch: turn.epoch,
            thread_id: turn.thread.clone(),
            turn_id: turn.id.clone(),
            item_id: id.into(),
            item_kind: "toolCall".into(),
            item: completed,
        },
    )
    .await?;
    Ok(response)
}

pub(crate) async fn title(connection: &Connection, prompt: &str) -> Result<String, String> {
    let client = wire::client_for(connection)?;
    let instructions = "Return only a short, descriptive conversation title.";
    let messages = wire::initial(connection.provider, instructions, prompt);
    let body = wire::body(connection, instructions, &messages, &[]);
    let response = tokio::time::timeout(
        Duration::from_secs(60),
        wire::request(&client, connection, &body),
    )
    .await
    .map_err(|_| "Title request timed out")??;
    Ok(wire::parse(connection.provider, response)?.text)
}
