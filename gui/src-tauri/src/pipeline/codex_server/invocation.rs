use super::connection::{connect, Connection, PROFILE};
use super::tools::HostTools;
use crate::agent_runtime::codex::invocation::{NativeThreadStart, TextTurnStart};
use crate::agent_runtime::codex::session::{private_write, same_existing_path};
use crate::agent_runtime::codex::{NormalizedEvent, DEFAULT_REQUEST_TIMEOUT};
use crate::pipeline::{claude::LlmOverrides, logging};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, OwnedRwLockReadGuard};

const MAX_OUTPUT: usize = 50 * 1024 * 1024;
const MAX_TOOL_CALLS: usize = 64;
const UNKNOWN: &str = "[codex-outcome-unknown]";

struct Attempt {
    connection: Arc<Connection>,
    lease: Option<OwnedRwLockReadGuard<()>>,
    account_lease: Option<OwnedRwLockReadGuard<()>>,
    path: PathBuf,
    record: Value,
    thread: Option<String>,
    turn: Option<String>,
    terminal: bool,
}

impl Attempt {
    fn save(&self) -> Result<(), String> {
        private_write(
            &self.path,
            &serde_json::to_vec_pretty(&self.record).map_err(|e| e.to_string())?,
        )
    }
}

impl Drop for Attempt {
    fn drop(&mut self) {
        if self.terminal {
            return;
        }
        let c = self.connection.clone();
        let thread = self.thread.clone();
        let turn = self.turn.clone();
        let lease = self.lease.take();
        let account_lease = self.account_lease.take();
        let path = self.path.clone();
        let mut record = self.record.clone();
        // A cleanup owner outlives the dropped call task. Native tools have no
        // artifact roots, and host tools stop polling with the call future.
        tokio::spawn(async move {
            let _lease = lease;
            let _account_lease = account_lease;
            record["state"] = json!("outcome_unknown");
            let _ = private_write(
                &path,
                &serde_json::to_vec_pretty(&record).unwrap_or_default(),
            );
            if let (Some(thread), Some(turn)) = (thread, turn) {
                let mut events = c.native.client.subscribe();
                let interrupted = c
                    .native
                    .client
                    .request(
                        "turn/interrupt",
                        json!({"threadId":thread,"turnId":turn}),
                        Duration::from_secs(3),
                    )
                    .await;
                if interrupted.is_ok() {
                    let terminal = tokio::time::timeout(Duration::from_secs(3), async {
                        loop {
                            match events.recv().await {
                                Ok(NormalizedEvent::TurnCompleted {
                                    thread_id, turn_id, ..
                                }) if thread_id == thread && turn_id == turn => return true,
                                Ok(NormalizedEvent::ConnectionClosed { .. }) | Err(_) => {
                                    return false
                                }
                                _ => {}
                            }
                        }
                    })
                    .await
                    .unwrap_or(false);
                    if terminal {
                        record["cleanup"] = json!("interrupted_terminal_observed");
                        let _ = private_write(
                            &path,
                            &serde_json::to_vec_pretty(&record).unwrap_or_default(),
                        );
                        return;
                    }
                }
                // Completion can precede subscription or an interrupt response.
                if let Ok(snapshot) = c
                    .native
                    .client
                    .request(
                        "thread/read",
                        json!({"threadId":thread,"includeTurns":true}),
                        Duration::from_secs(3),
                    )
                    .await
                {
                    if snapshot["thread"]["turns"].as_array().is_some_and(|turns| {
                        turns.iter().any(|t| {
                            t["id"] == turn
                                && matches!(
                                    t["status"].as_str(),
                                    Some("completed" | "failed" | "interrupted")
                                )
                        })
                    }) {
                        record["cleanup"] = json!("terminal_reconciled");
                        let _ = private_write(
                            &path,
                            &serde_json::to_vec_pretty(&record).unwrap_or_default(),
                        );
                        return;
                    }
                }
            } else if record["state_before_drop"] == "prepared" {
                record["state"] = json!("not_submitted");
                let _ = private_write(
                    &path,
                    &serde_json::to_vec_pretty(&record).unwrap_or_default(),
                );
                return;
            }
            c.native.terminate().await;
        });
    }
}

fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

#[allow(clippy::too_many_arguments)] // Matches the existing provider dispatcher contract.
pub async fn call_codex(
    app: &crate::emit::EventBus,
    prompt: &str,
    allowed_tools: &[&str],
    instructions: Option<&str>,
    timeout_secs: u64,
    label: &str,
    roots: &[&str],
    overrides: &LlmOverrides<'_>,
) -> Result<String, String> {
    let pass = logging::current_pass();
    let operation = run(
        app,
        prompt,
        allowed_tools,
        instructions,
        label,
        roots,
        overrides,
    );
    let result = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        crate::commands::await_or_cancel(operation, pass.as_deref()),
    )
    .await;
    match result {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => Err(error),
        Err(_) => Err(format!("{UNKNOWN} Workflow Codex deadline elapsed; do not retry until the recorded attempt is reconciled")),
    }
}

async fn run(
    app: &crate::emit::EventBus,
    prompt: &str,
    allowed_tools: &[&str],
    instructions: Option<&str>,
    label: &str,
    roots: &[&str],
    overrides: &LlmOverrides<'_>,
) -> Result<String, String> {
    run_connected(
        connect().await?,
        app,
        prompt,
        allowed_tools,
        instructions,
        label,
        roots,
        overrides,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_connected(
    c: Arc<Connection>,
    app: &crate::emit::EventBus,
    prompt: &str,
    allowed_tools: &[&str],
    instructions: Option<&str>,
    label: &str,
    roots: &[&str],
    overrides: &LlmOverrides<'_>,
) -> Result<String, String> {
    let prompt = match overrides.shared_context.as_ref() {
        Some(context) => context.prefixed_prompt(prompt)?,
        None => prompt.to_string(),
    };
    if prompt.len() > 2 * 1024 * 1024 {
        return Err("[codex-capability] Expanded Workflow prompt exceeds the App Server 2 MiB submission limit".into());
    }
    let mut tools = HostTools::new(allowed_tools, roots, overrides.write_dir)?;
    let schema = overrides
        .output_schema
        .map(crate::pipeline::structured::codex_schema)
        .transpose()?
        .flatten();
    let lease = c.activity.clone().read_owned().await;
    let account_lease = match &c.account {
        Some(account) => Some(
            account
                .lease(&c.native.client)
                .await
                .map_err(|e| format!("[codex-auth] {e}"))?,
        ),
        None => None,
    };
    if !c.recovery.lock().await.is_empty() {
        return Err("[codex-outcome-unknown] An earlier Workflow attempt requires review in Settings → API Keys → ChatGPT before another run".into());
    }
    if c.native
        .client
        .account_state(false)
        .await
        .map_err(|e| e.to_string())?
        .status
        != crate::agent_runtime::codex::AccountStatus::Chatgpt
    {
        return Err(
            "[codex-auth] Sign in to the Workflow ChatGPT App Server connection in Settings".into(),
        );
    }
    let model = overrides.model.filter(|s| !s.is_empty());
    let effort = overrides.effort.filter(|s| !s.is_empty());
    if let Some(model) = model {
        c.native
            .client
            .validate_model_selection(model, effort)
            .await
            .map_err(|e| format!("[codex-capability] {e}"))?;
    }
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    let id: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let mut attempt = Attempt {
        connection: c.clone(),
        lease: Some(lease),
        account_lease,
        path: c.attempts.join(format!("{id}.json")),
        record: json!({"schema_version":1,"id":id,"state":"prepared","state_before_drop":"prepared","pass":logging::current_pass(),"label":label,"backend":"codex_app_server","runtime_version":c.native.version,"epoch":c.native.client.epoch(),"model_requested":model,"effort":effort,"instructions":instructions,"instructions_sha256":hash(instructions.unwrap_or("")),"prompt_sha256":hash(&prompt),"prompt":prompt,"read_roots":roots,"write_root":overrides.write_dir,"allowed_tools":allowed_tools,"output_schema":overrides.output_schema,"native_schema":schema.is_some(),"native_context_reuse":false,"created_at":chrono::Utc::now().to_rfc3339()}),
        thread: None,
        turn: None,
        terminal: false,
    };
    attempt.save()?;
    let mut events = c.native.client.subscribe();
    let start = c.native.client.start_native_thread(NativeThreadStart {
        cwd: &c.cwd,
        runtime_workspace_roots: &[],
        permissions: PROFILE,
        approval_policy: "never",
        developer_instructions: instructions,
        base_instructions: None,
        dynamic_tools: &tools.declarations(),
        model,
        reasoning_effort: effort,
        ephemeral: false,
        service_name: "pipeline_workflows",
        config: Some(json!({"web_search":if allowed_tools.contains(&"WebSearch") {"live"} else {"disabled"}})),
    }).await.map_err(|e| format!("[codex-capability] Cannot start Workflow thread: {e}"))?;
    validate_thread(&start, &c.cwd, model)?;
    let thread = start["thread"]["id"]
        .as_str()
        .ok_or("[codex-capability] Missing thread identity")?
        .to_string();
    let effective_model = start["model"].as_str().unwrap_or_default();
    c.native
        .client
        .validate_model_selection(effective_model, effort)
        .await
        .map_err(|e| format!("[codex-capability] {e}"))?;
    attempt.thread = Some(thread.clone());
    attempt.record["thread_id"] = json!(thread);
    attempt.record["model_reported"] = json!(effective_model);
    attempt.record["state"] = json!("submitting");
    attempt.record["state_before_drop"] = json!("submitting");
    attempt.save()?;
    logging::record_provider_attempt();
    logging::emit(app, format!("[App Server] {label}: {effective_model}; attempt {id}; selected context supplied in a fresh thread"));
    let turn = match c
        .native
        .client
        .start_text_turn(TextTurnStart {
            thread_id: &thread,
            text: &prompt,
            client_user_message_id: Some(&id),
            model: None,
            effort,
            output_schema: schema.as_ref(),
        })
        .await
    {
        Ok(value) => value["turn"]["id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| format!("{UNKNOWN} Missing turn identity after submission"))?,
        Err(error) if error.rpc_code.is_some() => {
            attempt.terminal = true;
            attempt.record["state"] = json!("rejected");
            attempt.save()?;
            return Err(format!("[codex-rejected] {}", error.message));
        }
        Err(error) => return Err(format!("{UNKNOWN} {error}")),
    };
    attempt.turn = Some(turn.clone());
    attempt.record["turn_id"] = json!(turn);
    attempt.record["state"] = json!("accepted");
    attempt.save()?;
    let mut items = BTreeMap::new();
    let mut completed_tools = HashSet::new();
    let mut tool_results: HashMap<String, (String, Value)> = HashMap::new();
    let mut usage = logging::CallUsage::default();
    let mut usage_available = false;
    let mut output_bytes = 0usize;
    loop {
        match events.recv().await {
            Ok(NormalizedEvent::ItemCompleted {
                thread_id,
                turn_id,
                item_id,
                item_kind,
                item,
                ..
            }) if thread_id == thread && turn_id == turn => {
                output_bytes = output_bytes.saturating_add(item.to_string().len());
                if output_bytes > MAX_OUTPUT {
                    return Err(format!(
                        "{UNKNOWN} App Server output exceeded its safety budget"
                    ));
                }
                if item_kind == "agentMessage" {
                    items.insert(item_id.clone(), item.clone());
                }
                if completed_tools.insert(item_id)
                    && matches!(
                        item_kind.as_str(),
                        "webSearch" | "dynamicToolCall" | "mcpToolCall"
                    )
                {
                    let mut counts = crate::models::ToolCallCounts::default();
                    counts.add_kind(
                        logging::classify_tool_name(
                            item.get("tool")
                                .and_then(Value::as_str)
                                .unwrap_or(&item_kind),
                        ),
                        1,
                    );
                    logging::emit_tool_calls(app, counts);
                }
            }
            Ok(NormalizedEvent::ServerRequest {
                request_id,
                method,
                params,
                ..
            }) if params["threadId"] == thread => {
                if params["turnId"] != turn || method != "item/tool/call" {
                    let _ = c.native.client.respond_to_server_request(request_id, Err(crate::agent_runtime::codex::RequestError::invalid("Unattended Workflow cannot approve native actions or answer questions"))).await;
                    return Err(
                        "[codex-capability] Native approval or unsupported tool request was denied"
                            .into(),
                    );
                }
                let call_id = params["callId"]
                    .as_str()
                    .ok_or("[codex-capability] Tool request omitted call identity")?
                    .to_string();
                let fingerprint = hash(&json!([params["tool"], params["arguments"]]).to_string());
                let response = if let Some((previous, result)) = tool_results.get(&call_id) {
                    if previous != &fingerprint {
                        return Err("[codex-capability] Tool call identity was reused with different arguments".into());
                    }
                    result.clone()
                } else {
                    if tool_results.len() >= MAX_TOOL_CALLS {
                        return Err("[codex-capability] Workflow tool budget exhausted".into());
                    }
                    let name = params["tool"].as_str().unwrap_or_default();
                    attempt.record["pending_tool"] = json!({"call_id":call_id,"tool":name,"arguments_sha256":hash(&params["arguments"].to_string())});
                    attempt.save()?;
                    let result = tools.execute(app, name, &params["arguments"]).await;
                    attempt.record["pending_tool"] = Value::Null;
                    attempt.save()?;
                    tool_results.insert(call_id, (fingerprint, result.clone()));
                    result
                };
                c.native
                    .client
                    .respond_to_server_request(request_id, Ok(response))
                    .await
                    .map_err(|e| format!("{UNKNOWN} {e}"))?;
            }
            Ok(NormalizedEvent::UnknownNotification { method, params, .. })
                if method == "thread/tokenUsage/updated"
                    && params["threadId"] == thread
                    && params["turnId"] == turn =>
            {
                usage = parse_usage(&params["tokenUsage"]["total"]);
                usage_available = true;
            }
            Ok(NormalizedEvent::TurnCompleted {
                thread_id,
                turn_id,
                status,
                turn: terminal,
                ..
            }) if thread_id == thread && turn_id == turn => {
                attempt.terminal = true;
                attempt.record["state"] = json!(status);
                attempt.record["usage"] = if usage_available {
                    json!(usage)
                } else {
                    Value::Null
                };
                attempt.record["finished_at"] = json!(chrono::Utc::now().to_rfc3339());
                attempt.save()?;
                logging::emit_usage(app, usage);
                let result = if status != "completed" {
                    attempt.record["native_error"] = terminal["error"].clone();
                    Err(native_error(
                        terminal.get("error").filter(|e| !e.is_null()),
                        &status,
                    ))
                } else {
                    final_answer(items.values())
                };
                if let Ok(output) = &result {
                    attempt.record["output"] = json!(output);
                }
                attempt.save()?;
                let _ = c
                    .native
                    .client
                    .request(
                        "thread/unsubscribe",
                        json!({"threadId":thread}),
                        Duration::from_secs(3),
                    )
                    .await;
                return result;
            }
            Ok(NormalizedEvent::ConnectionClosed { .. })
            | Err(broadcast::error::RecvError::Closed) => {
                logging::emit_usage(app, usage);
                return Err(format!("{UNKNOWN} Workflow App Server disconnected after submission; inspect attempt {id}"));
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {
                // Never lose terminal output silently. Fresh threads contain exactly
                // one submitted turn; read is reconciliation, never a new submission.
                let snapshot = c
                    .native
                    .client
                    .request(
                        "thread/read",
                        json!({"threadId":thread,"includeTurns":true}),
                        DEFAULT_REQUEST_TIMEOUT,
                    )
                    .await
                    .map_err(|e| format!("{UNKNOWN} Reconciliation failed: {e}"))?;
                if let Some(terminal) = snapshot["thread"]["turns"].as_array().and_then(|turns| {
                    turns
                        .iter()
                        .find(|t| t["id"] == turn && t["status"] == "completed")
                }) {
                    let values = terminal["items"]
                        .as_array()
                        .ok_or_else(|| format!("{UNKNOWN} Reconciliation omitted items"))?;
                    let result = final_answer(values.iter())?;
                    attempt.terminal = true;
                    attempt.record["state"] = json!("completed_reconciled");
                    attempt.record["output"] = json!(result);
                    attempt.record["usage"] = if usage_available {
                        json!(usage)
                    } else {
                        Value::Null
                    };
                    attempt.save()?;
                    logging::emit_usage(app, usage);
                    return Ok(result);
                }
                return Err(format!(
                    "{UNKNOWN} Event stream lagged while a turn was active; it was not repeated"
                ));
            }
            _ => {}
        }
    }
}

pub(super) fn validate_thread(
    value: &Value,
    cwd: &std::path::Path,
    model: Option<&str>,
) -> Result<(), String> {
    let valid = value["modelProvider"] == "openai"
        && value["cwd"]
            .as_str()
            .is_some_and(|p| same_existing_path(std::path::Path::new(p), cwd))
        && value["runtimeWorkspaceRoots"]
            .as_array()
            .is_some_and(Vec::is_empty)
        && value["instructionSources"]
            .as_array()
            .is_some_and(Vec::is_empty)
        && value["activePermissionProfile"]["id"] == PROFILE
        && value["approvalPolicy"] == "never"
        && value["approvalsReviewer"] == "user"
        && value["sandbox"]["type"] == "readOnly"
        && model.is_none_or(|m| value["model"] == m);
    if valid {
        Ok(())
    } else {
        Err("[codex-capability] App Server did not honor the Workflow provider, instruction, model or filesystem contract".into())
    }
}

fn final_answer<'a>(items: impl Iterator<Item = &'a Value>) -> Result<String, String> {
    let final_items: Vec<_> = items
        .filter(|item| item["type"] == "agentMessage" && item["phase"] == "final_answer")
        .collect();
    if final_items.len() != 1 {
        return Err("[codex-capability] App Server did not return exactly one explicitly marked final answer".into());
    }
    final_items[0]["text"]
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| "[codex-capability] App Server final answer is empty".into())
}

fn native_error(error: Option<&Value>, status: &str) -> String {
    let code = error
        .and_then(|e| e["codexErrorInfo"].as_str())
        .unwrap_or("");
    let message = error.and_then(|e| e["message"].as_str()).unwrap_or(status);
    match code {
        "usageLimitExceeded" => format!("[codex-terminal] usage limit exceeded: {message}"),
        "unauthorized" => format!("[codex-auth] {message}"),
        "contextWindowExceeded" | "sessionBudgetExceeded" | "badRequest" | "sandboxError" => {
            format!("[codex-capability] {code}: {message}")
        }
        _ => format!("[codex-terminal] {message}"),
    }
}

fn parse_usage(value: &Value) -> logging::CallUsage {
    logging::CallUsage {
        input_tokens: value["inputTokens"].as_u64().unwrap_or(0),
        output_tokens: value["outputTokens"].as_u64().unwrap_or(0),
        cached_input_tokens: value["cachedInputTokens"].as_u64().unwrap_or(0),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_provider_failures_preserve_retry_and_fallback_policy() {
        let quota = native_error(
            Some(&json!({"codexErrorInfo":"usageLimitExceeded","message":"Unavailable"})),
            "failed",
        );
        assert!(crate::pipeline::provider_error::is_usage_limit_error(
            &quota
        ));
        assert!(!crate::pipeline::provider_error::is_non_retryable_error(
            &quota
        ));
        let auth = native_error(
            Some(&json!({"codexErrorInfo":"unauthorized","message":"Sign in"})),
            "failed",
        );
        assert!(crate::pipeline::provider_error::is_non_retryable_error(
            &auth
        ));
        let transient = native_error(
            Some(&json!({"codexErrorInfo":"serverOverloaded","message":"Busy"})),
            "failed",
        );
        assert!(!crate::pipeline::provider_error::is_usage_limit_error(
            &transient
        ));
        assert!(!crate::pipeline::provider_error::is_non_retryable_error(
            &transient
        ));
    }
    #[test]
    fn commentary_is_never_a_report_and_cumulative_usage_is_not_added() {
        let items = [
            json!({"type":"agentMessage","phase":"commentary","text":"Working"}),
            json!({"type":"agentMessage","phase":"final_answer","text":"{\"content\":\"Report\"}"}),
        ];
        assert_eq!(
            final_answer(items.iter()).unwrap(),
            "{\"content\":\"Report\"}"
        );
        assert!(final_answer(items[..1].iter()).is_err());
        assert!(final_answer([json!({"type":"agentMessage","text":"Unmarked"})].iter()).is_err());
        let usage =
            parse_usage(&json!({"inputTokens":120,"cachedInputTokens":100,"outputTokens":20}));
        assert_eq!(usage.input_tokens, 120);
        assert_eq!(usage.cached_input_tokens, 100);
    }
}
