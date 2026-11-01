//! API turn loop with Workspace-owned tools and durable events. Each response
//! is processed once, and call IDs are host-assigned from the accepted turn.
use super::super::turn_loop::{ModelClock, HOST_TIME_LIMIT};
use super::*;
use std::{collections::HashSet, time::Duration};

/// Model time allowed for one turn. Tools don't count: they have the turn's
/// host allowance, and a research run has its own profile timeout as well.
const MODEL_TIME_LIMIT: Duration = Duration::from_secs(60 * 60);
const MODEL_TIMEOUT: &str = "The API turn timed out after 60 minutes of model time; it was not retried. Time spent in project tools and local jobs is not counted.";
const HOST_TIMEOUT: &str =
    "The API turn stopped after 4 hours in project tools and local jobs; it was not retried.";

pub(super) fn turn_clock() -> ModelClock {
    ModelClock::new(MODEL_TIME_LIMIT, HOST_TIME_LIMIT)
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run(
    client: &reqwest::Client,
    connection: &Connection,
    app: &impl TurnEvents,
    turn: &Turn,
    instructions: &str,
    prompt: &str,
    specs: &[Value],
    mut clock: ModelClock,
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
        let mut ceiling = 0;
        for round in 0..32 {
            let mut body = wire::body(connection, instructions, &messages, specs);
            let reply = wire::parse(
                connection.provider,
                clock
                    .model(model_call(
                        client,
                        connection,
                        &mut body,
                        &mut ceiling,
                        Some(&mut |text| {
                            let _ = app.publish(&NormalizedEvent::AgentMessageDelta {
                                epoch: turn.epoch,
                                thread_id: turn.thread.clone(),
                                turn_id: turn.id.clone(),
                                item_id: format!("{}-assistant-{round}", turn.id),
                                delta: text.into(),
                            });
                        }),
                    ))
                    .await
                    .ok_or(MODEL_TIMEOUT)??,
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
            if !has_tool_calls(&reply)? {
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
                let result = clock
                    .host(execute(app, turn, &id, call, specs))
                    .await
                    .ok_or(HOST_TIMEOUT)??;
                results.push((call, result));
            }
            messages.extend(wire::tool_results(connection.provider, results));
        }
        Err("The conversation reached its model-call limit".into())
    };
    cancellable(work, cancel).await
}

/// One model call. A refused reply ceiling is the only resend: the server did
/// no work and streamed nothing, so nothing can run or appear twice. `ceiling`
/// carries the accepted limit into the turn's later calls.
pub(super) async fn model_call(
    client: &reqwest::Client,
    connection: &Connection,
    body: &mut Value,
    ceiling: &mut usize,
    mut emit: Option<&mut (dyn FnMut(&str) + Send + '_)>,
) -> Result<Value, String> {
    loop {
        wire::limit_output(connection.provider, body, wire::OUTPUT_LIMITS[*ceiling]);
        let attempt = wire::exchange(client, connection, body, emit.as_deref_mut()).await;
        match attempt {
            Err(error)
                if error == wire::OUTPUT_LIMIT_REFUSED
                    && *ceiling + 1 < wire::OUTPUT_LIMITS.len() =>
            {
                *ceiling += 1;
            }
            result => return result,
        }
    }
}

/// Whether the turn continues into this reply's tool calls. A reply cut off at
/// the length limit ends the turn after its partial text is saved.
pub(super) fn has_tool_calls(reply: &wire::Reply) -> Result<bool, String> {
    if reply.truncated {
        return Err(wire::LENGTH_LIMIT.into());
    }
    Ok(!reply.calls.is_empty())
}

pub(super) async fn cancellable<T>(
    work: impl std::future::Future<Output = Result<T, String>>,
    mut cancel: watch::Receiver<bool>,
) -> Result<T, String> {
    tokio::select! {
        biased;
        _ = cancel.changed() => Err("interrupted".into()),
        result = work => result,
    }
}

/// Why this call can't run, in terms the model can correct. Only a broken tool
/// schema is the host's fault and ends the turn.
fn refusal(call: &wire::Call, specs: &[Value]) -> Result<Option<String>, String> {
    let Some(spec) = specs.iter().find(|s| s["name"] == call.name) else {
        return Ok(Some(
            "This tool is not enabled for this conversation. Use only the tools listed for it."
                .into(),
        ));
    };
    let validator = jsonschema::validator_for(&spec["inputSchema"])
        .map_err(|_| "Invalid conversation tool schema")?;
    Ok(validator.validate(&call.arguments).err().map(|error| {
        let detail = error.to_string();
        format!(
            "The tool arguments do not match its schema: {}",
            super::super::search::prefix(&detail, 1024)
        )
    }))
}

async fn execute(
    app: &impl TurnEvents,
    turn: &Turn,
    id: &str,
    call: &wire::Call,
    specs: &[Value],
) -> Result<Value, String> {
    let refused = refusal(call, specs)?;
    let name = super::super::search::prefix(&call.name, 256);
    let item = json!({"id":id,"type":"toolCall","runtime":turn.runtime,"tool":name,"input":super::super::search::prefix(&call.arguments.to_string(),32*1024),"status":"inProgress"});
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
    let response = if let Some(message) = refused {
        Err(WorkbenchError::invalid(message))
    } else if super::super::conversation_tools::owns(&call.name) {
        // Boxed because this future is too large for a thread's stack unoptimized.
        Box::pin(super::super::conversation_tools::handle(params)).await
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
    let encoded = response.to_string();
    if encoded.len() > wire::MAX_BODY / 2 {
        return Err("The tool result exceeded 4 MB".into());
    }
    let mut completed = item;
    completed["status"] = json!(if response["success"] == true {
        "completed"
    } else {
        "failed"
    });
    completed["output"] = json!(super::super::search::prefix(&encoded, 32 * 1024));
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
    let mut body = wire::body(connection, instructions, &messages, &[]);
    let response = tokio::time::timeout(
        Duration::from_secs(60),
        model_call(&client, connection, &mut body, &mut 0, None),
    )
    .await
    .map_err(|_| "Title request timed out")??;
    let reply = wire::parse(connection.provider, response)?;
    has_tool_calls(&reply)?;
    Ok(reply.text)
}
