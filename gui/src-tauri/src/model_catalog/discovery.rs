use super::*;

pub(super) async fn read_discovery_output<R: AsyncRead + Unpin>(
    mut reader: R,
    limit: usize,
) -> Result<(Vec<u8>, bool), String> {
    let mut kept = Vec::with_capacity(limit.min(16 * 1024));
    let mut overflowed = false;
    let mut chunk = [0u8; 16 * 1024];
    loop {
        let count = reader
            .read(&mut chunk)
            .await
            .map_err(|e| format!("Failed to read provider discovery output: {e}"))?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(kept.len());
        let take = remaining.min(count);
        kept.extend_from_slice(&chunk[..take]);
        overflowed |= take < count;
    }
    Ok((kept, overflowed))
}

struct DiscoveryProcessGuard {
    pid: u32,
    armed: bool,
}

impl DiscoveryProcessGuard {
    fn register(pid: u32) -> Self {
        if pid > 0 {
            crate::commands::register_child_pid(pid);
        }
        Self {
            pid,
            armed: pid > 0,
        }
    }

    fn unregister(&mut self) {
        if self.armed {
            crate::commands::unregister_child_pid(self.pid);
            self.armed = false;
        }
    }
}

impl Drop for DiscoveryProcessGuard {
    fn drop(&mut self) {
        if self.armed {
            crate::commands::kill_process(self.pid);
            crate::commands::unregister_child_pid(self.pid);
        }
    }
}

async fn stop_discovery_child(
    child: &mut tokio::process::Child,
    guard: &mut DiscoveryProcessGuard,
) {
    if guard.pid > 0 {
        crate::commands::kill_process(guard.pid);
    }
    let _ = child.kill().await;
    let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    guard.unregister();
}

async fn cli_version(program: &str) -> String {
    let args = vec!["--version".to_string()];
    let Ok(mut command) = build_provider_command(program, None, &args) else {
        return String::new();
    };
    command
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let Ok(mut child) = command.spawn() else {
        return String::new();
    };
    let mut guard = DiscoveryProcessGuard::register(child.id().unwrap_or(0));
    let Some(stdout) = child.stdout.take() else {
        stop_discovery_child(&mut child, &mut guard).await;
        return String::new();
    };
    let reader = tokio::spawn(read_discovery_output(stdout, MAX_DISCOVERY_VERSION_BYTES));
    let status = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    if !matches!(status, Ok(Ok(status)) if status.success()) {
        stop_discovery_child(&mut child, &mut guard).await;
        let _ = reader.await;
        return String::new();
    }
    guard.unregister();
    match reader.await {
        Ok(Ok((bytes, false))) => String::from_utf8_lossy(&bytes).trim().to_string(),
        _ => String::new(),
    }
}

async fn rpc_exchange_impl(
    program: &str,
    args: &[String],
    requests: &[serde_json::Value],
    accept_error_reply: bool,
) -> Result<Vec<serde_json::Value>, String> {
    let mut command = build_provider_command(program, None, args)?;
    command
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to start {program} model discovery: {e}"))?;
    let mut guard = DiscoveryProcessGuard::register(child.id().unwrap_or(0));
    let result = async {
        let mut stdin = child
            .stdin
            .take()
            .ok_or("Provider discovery has no stdin")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("Provider discovery has no stdout")?;
        let mut reader = BufReader::new(stdout);
        let mut results = Vec::new();
        for request in requests {
            let id = request["id"].clone();
            let response = tokio::time::timeout(Duration::from_secs(12), async {
                stdin
                    .write_all(
                        serde_json::to_string(request)
                            .map_err(|e| e.to_string())?
                            .as_bytes(),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                stdin.write_all(b"\n").await.map_err(|e| e.to_string())?;
                stdin.flush().await.map_err(|e| e.to_string())?;
                while let Some(record) = crate::pipeline::logging::next_bounded_line(
                    &mut reader,
                    MAX_DISCOVERY_LINE_BYTES,
                )
                .await
                .map_err(|e| e.to_string())?
                {
                    if record.truncated {
                        return Err(format!(
                            "{program} model discovery emitted a response line larger than {} MB",
                            MAX_DISCOVERY_LINE_BYTES / 1024 / 1024
                        ));
                    }
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&record.text) {
                        if value.get("id") == Some(&id) {
                            return Ok(value);
                        }
                    }
                }
                Err("Provider discovery ended before replying".to_string())
            })
            .await
            .map_err(|_| format!("{program} model discovery timed out"))??;
            if response.get("error").is_some() && !accept_error_reply {
                return Err(provider_discovery_error(program, &response["error"]));
            }
            let is_error = response.get("error").is_some();
            results.push(response);
            if is_error {
                break;
            }
            if program == "codex" && id == serde_json::json!(1) {
                tokio::time::timeout(Duration::from_secs(12), async {
                    stdin
                        .write_all(
                            b"{\"jsonrpc\":\"2.0\",\"method\":\"initialized\",\"params\":{}}\n",
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                    stdin.flush().await.map_err(|e| e.to_string())
                })
                .await
                .map_err(|_| format!("{program} model discovery timed out"))??;
            }
        }
        Ok(results)
    }
    .await;
    stop_discovery_child(&mut child, &mut guard).await;
    result
}

pub(crate) async fn rpc_exchange(
    program: &str,
    args: &[String],
    requests: &[serde_json::Value],
) -> Result<Vec<serde_json::Value>, String> {
    rpc_exchange_impl(program, args, requests, false).await
}

pub(super) fn provider_discovery_error(program: &str, error: &serde_json::Value) -> String {
    format!("{program} discovery returned {error}")
}

/// Ask Claude Code for the same account-aware model list exposed by its
/// interactive `/model` picker. This is the documented Agent SDK initialize
/// handshake: it does not submit a user prompt or make a model call.
async fn claude_sdk_initialize() -> Result<serde_json::Value, String> {
    let args = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--safe-mode",
        "--permission-mode",
        "dontAsk",
        "--tools",
        "",
        "--disable-slash-commands",
        "--no-chrome",
        "--no-session-persistence",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();
    let mut command = build_provider_command("claude", None, &args)?;
    command
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to start Claude model discovery: {e}"))?;
    let mut guard = DiscoveryProcessGuard::register(child.id().unwrap_or(0));
    let result = async {
        let mut stdin = child
            .stdin
            .take()
            .ok_or("Claude model discovery has no stdin")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("Claude model discovery has no stdout")?;
        let request_id = "pipeline-model-list";
        let request = serde_json::json!({
            "type": "control_request",
            "request_id": request_id,
            "request": {
                "subtype": "initialize",
                "hooks": {}
            }
        });
        stdin
            .write_all(
                serde_json::to_string(&request)
                    .map_err(|e| e.to_string())?
                    .as_bytes(),
            )
            .await
            .map_err(|e| e.to_string())?;
        stdin.write_all(b"\n").await.map_err(|e| e.to_string())?;
        stdin.flush().await.map_err(|e| e.to_string())?;

        let mut reader = BufReader::new(stdout);
        tokio::time::timeout(Duration::from_secs(20), async {
            while let Some(record) =
                crate::pipeline::logging::next_bounded_line(&mut reader, MAX_DISCOVERY_LINE_BYTES)
                    .await
                    .map_err(|e| e.to_string())?
            {
                if record.truncated {
                    return Err(format!(
                        "Claude model discovery emitted a response line larger than {} MB",
                        MAX_DISCOVERY_LINE_BYTES / 1024 / 1024
                    ));
                }
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&record.text) else {
                    continue;
                };
                if value["type"] != "control_response"
                    || value["response"]["request_id"] != request_id
                {
                    continue;
                }
                if value["response"]["subtype"] == "error" {
                    return Err(format!(
                        "Claude model discovery returned {}",
                        value["response"]["error"]
                    ));
                }
                return value["response"]["response"]
                    .as_object()
                    .map(|_| value["response"]["response"].clone())
                    .ok_or_else(|| {
                        "Claude model discovery returned no initialization data".to_string()
                    });
            }
            Err("Claude model discovery ended before replying".to_string())
        })
        .await
        .map_err(|_| "Claude model discovery timed out".to_string())?
    }
    .await;
    stop_discovery_child(&mut child, &mut guard).await;
    result
}

pub(super) fn populate_claude_models(
    catalog: &mut ModelCatalog,
    initialization: &serde_json::Value,
) -> Result<(), String> {
    let models = initialization["models"]
        .as_array()
        .ok_or("Claude model discovery returned no model list")?;
    let default_resolved = models
        .iter()
        .find(|model| model["value"] == "default")
        .and_then(|model| model["resolvedModel"].as_str())
        .map(str::to_string);
    catalog.default_model.clone_from(&default_resolved);

    for item in models {
        let Some(id) = item["value"].as_str() else {
            continue;
        };
        // Pipeline already provides an Automatic option which deliberately
        // omits --model, so do not duplicate Claude's `default` sentinel as a
        // pinnable exact model.
        if id == "default" {
            continue;
        }
        let mut model = entry(id);
        model.display_name = item["displayName"].as_str().unwrap_or(id).to_string();
        model.description = item["description"].as_str().unwrap_or("").to_string();
        model.supported_efforts = item["supportedEffortLevels"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_string)
            .collect();
        model.is_default = default_resolved
            .as_deref()
            .is_some_and(|default| item["resolvedModel"].as_str() == Some(default));
        catalog.models.push(model);
    }
    if catalog.models.is_empty() {
        return Err("Claude model discovery returned an empty model list".into());
    }
    Ok(())
}

pub(super) async fn cli_catalog(provider: &str) -> Result<ModelCatalog, String> {
    let program = match provider {
        "claude" => "claude",
        "codex" => "codex",
        _ => return Err(format!("Unknown CLI provider: {provider}")),
    };
    let mut catalog = base_catalog(provider, "cli", "installed_cli");
    catalog.source_version = cli_version(program).await;
    match provider {
        "codex" => {
            let requests = [
                serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"pipeline","title":"Pipeline","version":env!("CARGO_PKG_VERSION")}}}),
                serde_json::json!({"jsonrpc":"2.0","id":2,"method":"model/list","params":{"limit":100}}),
            ];
            let replies = rpc_exchange(
                "codex",
                &["app-server".into(), "--listen".into(), "stdio://".into()],
                &requests,
            )
            .await?;
            let result = &replies[1]["result"];
            for item in result["data"].as_array().into_iter().flatten() {
                let Some(id) = item["model"].as_str().or_else(|| item["id"].as_str()) else {
                    continue;
                };
                let mut model = entry(id);
                model.display_name = item["displayName"].as_str().unwrap_or(id).to_string();
                model.description = item["description"].as_str().unwrap_or("").to_string();
                model.is_default = item["isDefault"].as_bool().unwrap_or(false);
                model.supported_efforts = item["supportedReasoningEfforts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|effort| {
                        effort["reasoningEffort"]
                            .as_str()
                            .or_else(|| effort.as_str())
                    })
                    .map(str::to_string)
                    .collect();
                if model.is_default {
                    catalog.default_model = Some(id.to_string());
                }
                catalog.models.push(model);
            }
        }
        "claude" => {
            let initialization = claude_sdk_initialize().await?;
            populate_claude_models(&mut catalog, &initialization)?;
        }
        _ => unreachable!(),
    }
    catalog.roles = infer_roles(provider, &catalog.models);
    Ok(catalog)
}
