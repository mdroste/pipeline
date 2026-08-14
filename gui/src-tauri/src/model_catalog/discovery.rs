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

const ANTIGRAVITY_SIGN_IN_MESSAGE: &str = "Antigravity CLI is not signed in. Run `agy` in a terminal to sign in (or configure a Gemini API key), then refresh the model list.";

struct AntigravityProbeOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

/// Run one `agy models` invocation with bounded output on both streams.
/// stderr is retained so a signed-out failure can be mapped to an actionable
/// message instead of a raw exit code.
async fn run_antigravity_models(args: &[String]) -> Result<AntigravityProbeOutput, String> {
    let mut command = build_provider_command("agy", None, args)?;
    command
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to start agy model discovery: {e}"))?;
    let mut guard = DiscoveryProcessGuard::register(child.id().unwrap_or(0));
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_reader = tokio::spawn(async move {
        match stdout {
            Some(stream) => read_discovery_output(stream, MAX_DISCOVERY_LINE_BYTES).await,
            None => Ok((Vec::new(), false)),
        }
    });
    let stderr_reader = tokio::spawn(async move {
        match stderr {
            Some(stream) => read_discovery_output(stream, MAX_DISCOVERY_VERSION_BYTES).await,
            None => Ok((Vec::new(), false)),
        }
    });
    let status = match tokio::time::timeout(Duration::from_secs(15), child.wait()).await {
        Ok(Ok(status)) => status,
        _ => {
            stop_discovery_child(&mut child, &mut guard).await;
            let _ = stdout_reader.await;
            let _ = stderr_reader.await;
            return Err("agy model discovery timed out".to_string());
        }
    };
    guard.unregister();
    let (stdout_bytes, stdout_overflowed) = stdout_reader
        .await
        .map_err(|e| format!("agy model discovery reader failed: {e}"))??;
    let (stderr_bytes, _) = stderr_reader
        .await
        .map_err(|e| format!("agy model discovery reader failed: {e}"))??;
    if stdout_overflowed {
        return Err(format!(
            "agy model discovery output exceeded the {} MB safety limit",
            MAX_DISCOVERY_LINE_BYTES / 1024 / 1024
        ));
    }
    Ok(AntigravityProbeOutput {
        success: status.success(),
        stdout: String::from_utf8_lossy(&stdout_bytes).to_string(),
        stderr: String::from_utf8_lossy(&stderr_bytes).to_string(),
    })
}

/// Fetch the `agy models` listing. Newer agy releases support
/// `--output-format json` on the subcommand; 1.1.12 rejects the flag with a
/// usage error, so discovery falls back to the plain text listing. Signed-out
/// failures become an actionable sign-in message; `agy models` never starts
/// an interactive login.
async fn antigravity_models_output() -> Result<String, String> {
    let mut last_error = String::from("no diagnostic output");
    let attempts = [
        vec![
            "models".to_string(),
            "--output-format".to_string(),
            "json".to_string(),
        ],
        vec!["models".to_string()],
    ];
    for args in &attempts {
        let probe = run_antigravity_models(args).await?;
        if probe.success {
            return Ok(probe.stdout);
        }
        let combined = format!("{}\n{}", probe.stdout, probe.stderr);
        let lower = combined.to_ascii_lowercase();
        if lower.contains("sign in") || lower.contains("authentication") {
            return Err(ANTIGRAVITY_SIGN_IN_MESSAGE.to_string());
        }
        last_error = combined
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("no diagnostic output")
            .trim()
            .to_string();
        // Only an unrecognized-flag usage error justifies the plain retry.
        if !(lower.contains("not defined") || lower.contains("unknown flag")) {
            break;
        }
    }
    Err(format!("agy model discovery failed: {last_error}"))
}

fn collect_antigravity_json_models(catalog: &mut ModelCatalog, value: &serde_json::Value) {
    let items = value
        .as_array()
        .cloned()
        .or_else(|| value["models"].as_array().cloned())
        .or_else(|| value["data"].as_array().cloned())
        .unwrap_or_default();
    for item in &items {
        let Some(id) = item["id"]
            .as_str()
            .or_else(|| item["model"].as_str())
            .or_else(|| item["modelId"].as_str())
            .or_else(|| item.as_str())
        else {
            continue;
        };
        let mut model = entry(id);
        if let Some(display) = item["display_name"]
            .as_str()
            .or_else(|| item["displayName"].as_str())
            .or_else(|| item["name"].as_str())
        {
            model.display_name = display.to_string();
        }
        model.description = item["description"].as_str().unwrap_or("").to_string();
        model.is_default = item["is_default"]
            .as_bool()
            .or_else(|| item["isDefault"].as_bool())
            .or_else(|| item["default"].as_bool())
            .or_else(|| item["current"].as_bool())
            .unwrap_or(false);
        if model.is_default {
            catalog.default_model = Some(model.id.clone());
        }
        catalog.models.push(model);
    }
}

fn collect_antigravity_text_models(catalog: &mut ModelCatalog, output: &str) {
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        // Progress banners, section headers, and diagnostics are not rows.
        if lower.starts_with("fetching")
            || lower.starts_with("error")
            || lower.starts_with("available models")
            || lower.ends_with(':')
        {
            continue;
        }
        // Rows are either tab-separated records (id first) or a plain listing
        // with optional "*"/"-" markers and a "(default)" suffix. Model ids
        // may themselves contain spaces ("Gemini 3.5 Flash"), so columnar
        // separators — tabs, em dashes, 2+ spaces — split fields, not spaces.
        let mut fields = line.split('\t');
        let first = fields.next().unwrap_or_default().trim();
        let display = fields.next().map(str::trim).filter(|s| !s.is_empty());
        let is_default =
            lower.contains("(default)") || lower.contains("[default]") || first.starts_with('*');
        let mut token = first.trim_start_matches(['*', '-', '•']).trim();
        for separator in [" — ", "  "] {
            if let Some((head, _)) = token.split_once(separator) {
                token = head.trim();
            }
        }
        let token = token
            .replace("(default)", "")
            .replace("[default]", "")
            .trim()
            .trim_end_matches([',', ':'])
            .to_string();
        if token.is_empty() {
            continue;
        }
        let mut model = entry(token);
        if let Some(display) = display {
            model.display_name = display.to_string();
        }
        model.is_default = is_default;
        if is_default {
            catalog.default_model = Some(model.id.clone());
        }
        catalog.models.push(model);
    }
}

/// Parse `agy models` output into the catalog: a JSON document first, then
/// NDJSON lines, then the plain text listing. The parsed set is authoritative
/// for pinned-ID validation, so zero parsed models is an error rather than an
/// empty catalog.
pub(super) fn populate_antigravity_models(
    catalog: &mut ModelCatalog,
    output: &str,
) -> Result<(), String> {
    let trimmed = output.trim();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        collect_antigravity_json_models(catalog, &value);
    }
    if catalog.models.is_empty() {
        for line in trimmed.lines() {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) {
                collect_antigravity_json_models(catalog, &value);
            }
        }
    }
    if catalog.models.is_empty() {
        collect_antigravity_text_models(catalog, trimmed);
    }
    if catalog.models.is_empty() {
        return Err("agy model discovery returned an empty model list".into());
    }
    Ok(())
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
        "antigravity" => "agy",
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
        "antigravity" => {
            let output = antigravity_models_output().await?;
            populate_antigravity_models(&mut catalog, &output)?;
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
