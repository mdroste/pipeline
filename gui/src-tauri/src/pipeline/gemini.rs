use std::io::Write;
use std::process::Stdio;
use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt;

use super::claude::{build_provider_command, plan_cli_workspace, LlmOverrides};
use super::cli_process::{
    capture_stderr, capture_text_stdout, emit_stderr_tail, finish_streams, last_stderr_hint, log,
    track_child_started, verbose_log, wait_for_child,
};

/// `--include-directories` expands Gemini's editable workspace as well as its
/// readable roots. In write mode, this policy permits edits only under the
/// absolute artifact directory and denies edits everywhere else. The prompt
/// supplies an absolute artifact path; relative edit attempts are denied and
/// can still use the executor's stdout fallback.
const GEMINI_TRAVERSAL_PATTERN: &str = r#""file_path":"[^"]*\.\."#;
const GEMINI_READ_TOOLS: &[&str] = &[
    "read_file",
    "read_many_files",
    "list_directory",
    "glob",
    "grep_search",
];
const MAX_GEMINI_SYSTEM_SETTINGS_BYTES: u64 = 4 * 1024 * 1024;

fn gemini_allowed_write_pattern(write_dir: &str) -> String {
    let escaped_root = regex::escape(write_dir.trim_end_matches('/'));
    format!(r#""file_path":"{escaped_root}(?:/[^"]*)?""#)
}

fn gemini_core_tools(allowed_tools: &[&str], needs_write: bool) -> Vec<&'static str> {
    let mut tools = GEMINI_READ_TOOLS.to_vec();
    if allowed_tools.contains(&"WebSearch") {
        tools.push("google_web_search");
    }
    if needs_write {
        tools.extend(["write_file", "replace"]);
    }
    tools
}

fn gemini_tool_policy(write_dir: Option<&str>, core_tools: &[&str]) -> String {
    let read_tools: Vec<&str> = core_tools
        .iter()
        .copied()
        .filter(|tool| !matches!(*tool, "write_file" | "replace"))
        .collect();
    let read_tools =
        serde_json::to_string(&read_tools).expect("serializing Gemini tool names cannot fail");
    let mode = if write_dir.is_some() {
        "autoEdit"
    } else {
        "plan"
    };
    let mut policy = format!(
        "[[rule]]\n\
         toolName = {read_tools}\n\
         decision = \"allow\"\n\
         priority = 998\n\
         modes = [\"{mode}\"]\n\
         interactive = false\n\n"
    );
    if let Some(write_dir) = write_dir {
        let allowed_pattern = gemini_allowed_write_pattern(write_dir);
        // Match any `..` in the serialized file_path value. This deliberately
        // rejects benign double-dot filenames too: report handoff paths never
        // need them, and the conservative rule covers both slash styles after
        // JSON serialization.
        let pattern_literal = serde_json::to_string(&allowed_pattern)
            .expect("serializing a regex string cannot fail");
        let traversal_literal = serde_json::to_string(GEMINI_TRAVERSAL_PATTERN)
            .expect("serializing a regex string cannot fail");
        policy.push_str(&format!(
            "[[rule]]\n\
             toolName = [\"write_file\", \"replace\"]\n\
             argsPattern = {traversal_literal}\n\
             decision = \"deny\"\n\
             priority = 999\n\
             modes = [\"autoEdit\"]\n\
             interactive = false\n\
             denyMessage = \"Pipeline does not permit path traversal.\"\n\n\
             [[rule]]\n\
             toolName = [\"write_file\", \"replace\"]\n\
             argsPattern = {pattern_literal}\n\
             decision = \"allow\"\n\
             priority = 998\n\
             modes = [\"autoEdit\"]\n\
             interactive = false\n\n"
        ));
    }
    // A global deny removes every non-allowlisted tool from the model's
    // context. tools.core and the admin settings below independently prevent
    // ambient built-ins, extensions, MCP servers, skills, and hooks from
    // registering capabilities.
    policy.push_str(&format!(
        "[[rule]]\n\
         toolName = \"*\"\n\
         decision = \"deny\"\n\
         priority = 900\n\
         modes = [\"{mode}\"]\n\
         interactive = false\n\
         denyMessage = \"Pipeline did not grant this capability to the step.\"\n"
    ));
    policy
}

fn default_gemini_system_settings_path() -> Option<std::path::PathBuf> {
    if let Some(path) =
        std::env::var_os("GEMINI_CLI_SYSTEM_SETTINGS_PATH").filter(|path| !path.is_empty())
    {
        return Some(path.into());
    }
    #[cfg(target_os = "macos")]
    {
        Some(
            std::path::PathBuf::from("/Library/Application Support/GeminiCli")
                .join("settings.json"),
        )
    }
    #[cfg(target_os = "linux")]
    {
        Some(std::path::PathBuf::from("/etc/gemini-cli/settings.json"))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("ProgramData")
            .map(std::path::PathBuf::from)
            .map(|root| root.join("gemini-cli").join("settings.json"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

fn load_gemini_system_settings() -> Result<serde_json::Value, String> {
    use std::io::Read as _;
    let Some(path) = default_gemini_system_settings_path() else {
        return Ok(serde_json::json!({}));
    };
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let file = crate::safety::open_regular_file(&path).map_err(|error| {
        format!(
            "Cannot preserve existing Gemini system settings '{}': {error}",
            path.display()
        )
    })?;
    let mut bytes = Vec::with_capacity(64 * 1024);
    file.take(MAX_GEMINI_SYSTEM_SETTINGS_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            format!(
                "Cannot read existing Gemini system settings '{}': {error}",
                path.display()
            )
        })?;
    if bytes.len() as u64 > MAX_GEMINI_SYSTEM_SETTINGS_BYTES {
        return Err(format!(
            "Gemini system settings '{}' exceed the 4 MB safety limit",
            path.display()
        ));
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "Existing Gemini system settings '{}' are invalid JSON: {error}",
            path.display()
        )
    })?;
    if !value.is_object() {
        return Err(format!(
            "Existing Gemini system settings '{}' must contain a JSON object",
            path.display()
        ));
    }
    Ok(value)
}

fn set_gemini_setting(root: &mut serde_json::Value, path: &[&str], value: serde_json::Value) {
    let mut current = root;
    for key in &path[..path.len().saturating_sub(1)] {
        if !current.get(*key).is_some_and(serde_json::Value::is_object) {
            current[*key] = serde_json::json!({});
        }
        current = &mut current[*key];
    }
    current[path[path.len() - 1]] = value;
}

fn restricted_gemini_settings(
    mut settings: serde_json::Value,
    core_tools: &[&str],
) -> serde_json::Value {
    set_gemini_setting(
        &mut settings,
        &["tools", "core"],
        serde_json::json!(core_tools),
    );
    set_gemini_setting(
        &mut settings,
        &["tools", "discoveryCommand"],
        serde_json::json!(""),
    );
    set_gemini_setting(
        &mut settings,
        &["tools", "callCommand"],
        serde_json::json!(""),
    );
    set_gemini_setting(&mut settings, &["mcp", "allowed"], serde_json::json!([]));
    set_gemini_setting(&mut settings, &["mcpServers"], serde_json::json!({}));
    set_gemini_setting(&mut settings, &["policyPaths"], serde_json::json!([]));
    if settings.get("adminPolicyPaths").is_none() {
        set_gemini_setting(&mut settings, &["adminPolicyPaths"], serde_json::json!([]));
    }
    set_gemini_setting(
        &mut settings,
        &["skills", "enabled"],
        serde_json::json!(false),
    );
    set_gemini_setting(
        &mut settings,
        &["hooksConfig", "enabled"],
        serde_json::json!(false),
    );
    set_gemini_setting(&mut settings, &["useWriteTodos"], serde_json::json!(false));
    set_gemini_setting(
        &mut settings,
        &["security", "disableYoloMode"],
        serde_json::json!(true),
    );
    set_gemini_setting(
        &mut settings,
        &["security", "disableAlwaysAllow"],
        serde_json::json!(true),
    );
    set_gemini_setting(
        &mut settings,
        &["security", "enablePermanentToolApproval"],
        serde_json::json!(false),
    );
    set_gemini_setting(
        &mut settings,
        &["security", "blockGitExtensions"],
        serde_json::json!(true),
    );
    set_gemini_setting(
        &mut settings,
        &["security", "allowedExtensions"],
        serde_json::json!([]),
    );
    set_gemini_setting(
        &mut settings,
        &["admin", "secureModeEnabled"],
        serde_json::json!(true),
    );
    set_gemini_setting(
        &mut settings,
        &["admin", "extensions", "enabled"],
        serde_json::json!(false),
    );
    set_gemini_setting(
        &mut settings,
        &["admin", "mcp", "enabled"],
        serde_json::json!(false),
    );
    set_gemini_setting(
        &mut settings,
        &["admin", "mcp", "config"],
        serde_json::json!({}),
    );
    set_gemini_setting(
        &mut settings,
        &["admin", "mcp", "requiredConfig"],
        serde_json::json!({}),
    );
    set_gemini_setting(
        &mut settings,
        &["admin", "skills", "enabled"],
        serde_json::json!(false),
    );
    settings
}

fn create_gemini_system_settings(core_tools: &[&str]) -> Result<NamedTempFile, String> {
    let settings = restricted_gemini_settings(load_gemini_system_settings()?, core_tools);
    let mut file = tempfile::Builder::new()
        .prefix("pipeline_gemini_settings_")
        .suffix(".json")
        .tempfile()
        .map_err(|error| format!("Failed to create isolated Gemini settings: {error}"))?;
    serde_json::to_writer(&mut file, &settings)
        .map_err(|error| format!("Failed to write isolated Gemini settings: {error}"))?;
    file.flush()
        .map_err(|error| format!("Failed to flush isolated Gemini settings: {error}"))?;
    Ok(file)
}

fn create_gemini_policy(
    write_dir: Option<&str>,
    core_tools: &[&str],
) -> Result<NamedTempFile, String> {
    let policy_text = gemini_tool_policy(write_dir, core_tools);
    // Match any `..` in the serialized file_path value. This deliberately
    // rejects benign double-dot filenames too.
    let mut policy = tempfile::Builder::new()
        .prefix("pipeline_gemini_")
        .suffix(".toml")
        .tempfile()
        .map_err(|e| format!("Failed to create Gemini policy file: {e}"))?;
    policy
        .write_all(policy_text.as_bytes())
        .map_err(|e| format!("Failed to write Gemini policy file: {e}"))?;
    policy
        .flush()
        .map_err(|e| format!("Failed to flush Gemini policy file: {e}"))?;
    Ok(policy)
}

fn append_gemini_read_dirs(cmd_args: &mut Vec<String>, read_dirs: &[String]) {
    for dir in read_dirs {
        cmd_args.push("--include-directories".to_string());
        cmd_args.push(dir.clone());
    }
}

/// Parse Gemini CLI's one-shot JSON envelope. The `response` field is the
/// terminal assistant response; all other stdout fields are telemetry or
/// diagnostics and must not be flattened into the report.
fn parse_gemini_result(
    raw: &str,
) -> Result<(String, Option<crate::pipeline::logging::CallUsage>), String> {
    let value = serde_json::from_str::<serde_json::Value>(raw.trim())
        .map_err(|error| format!("invalid JSON: {error}"))?;
    if let Some(error) = value.get("error").filter(|error| !error.is_null()) {
        let detail = error
            .get("message")
            .and_then(|message| message.as_str())
            .unwrap_or("Gemini marked the result as an error");
        return Err(detail.to_string());
    }
    let text = value
        .get("response")
        .and_then(|response| response.as_str())
        .ok_or("missing string `response`")?
        .trim()
        .to_string();

    let mut input_tokens = 0u64;
    let mut output_tokens = 0u64;
    let mut cached_input_tokens = 0u64;
    let mut model_round_trips = 0u64;
    if let Some(models) = value
        .get("stats")
        .and_then(|stats| stats.get("models"))
        .and_then(|models| models.as_object())
    {
        for model in models.values() {
            model_round_trips = model_round_trips.saturating_add(
                model
                    .get("api")
                    .and_then(|api| {
                        api.get("totalRequests")
                            .or_else(|| api.get("total_requests"))
                    })
                    .and_then(|count| count.as_u64())
                    .unwrap_or(0),
            );
            if let Some(tokens) = model.get("tokens") {
                // Gemini CLI's `prompt` mirrors UsageMetadata.promptTokenCount,
                // which is the total effective prompt and already includes
                // cachedContentTokenCount. Treat `cached` as a subset of
                // logical input; only the separately reported tool-use prompt
                // tokens are additive here.
                input_tokens = input_tokens.saturating_add(
                    tokens
                        .get("prompt")
                        .and_then(|count| count.as_u64())
                        .unwrap_or(0),
                );
                input_tokens = input_tokens.saturating_add(
                    tokens
                        .get("tool")
                        .or_else(|| tokens.get("toolUsePrompt"))
                        .and_then(|count| count.as_u64())
                        .unwrap_or(0),
                );
                output_tokens = output_tokens.saturating_add(
                    tokens
                        .get("candidates")
                        .and_then(|count| count.as_u64())
                        .unwrap_or(0),
                );
                output_tokens = output_tokens.saturating_add(
                    tokens
                        .get("thoughts")
                        .and_then(|count| count.as_u64())
                        .unwrap_or(0),
                );
                cached_input_tokens = cached_input_tokens.saturating_add(
                    tokens
                        .get("cached")
                        .and_then(|count| count.as_u64())
                        .unwrap_or(0),
                );
            }
        }
    }
    let mut tool_calls = crate::models::ToolCallCounts::default();
    if let Some(tools) = value.get("stats").and_then(|stats| stats.get("tools")) {
        let reported_total = tools
            .get("totalCalls")
            .or_else(|| tools.get("total_calls"))
            .and_then(|count| count.as_u64())
            .unwrap_or(0);
        if let Some(by_name) = tools
            .get("byName")
            .or_else(|| tools.get("by_name"))
            .and_then(|counts| counts.as_object())
        {
            for (name, entry) in by_name {
                let count = entry.as_u64().or_else(|| {
                    entry.as_object().and_then(|detail| {
                        ["totalCalls", "total_calls", "calls", "count"]
                            .iter()
                            .find_map(|field| detail.get(*field).and_then(|value| value.as_u64()))
                    })
                });
                tool_calls.add_kind(super::logging::classify_tool_name(name), count.unwrap_or(0));
            }
        }
        tool_calls.unknown = tool_calls
            .unknown
            .saturating_add(reported_total.saturating_sub(tool_calls.total()));
    }
    let usage = (input_tokens > 0
        || output_tokens > 0
        || cached_input_tokens > 0
        || model_round_trips > 0
        || !tool_calls.is_empty())
    .then_some(crate::pipeline::logging::CallUsage {
        input_tokens,
        output_tokens,
        cached_input_tokens,
        cache_write_input_tokens: 0,
        model_round_trips,
        tool_calls,
        ..Default::default()
    });
    Ok((text, usage))
}

fn gemini_error_hint(raw: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(raw.trim())
        .ok()?
        .get("error")?
        .get("message")?
        .as_str()
        .map(str::to_string)
}

/// Call `gemini -p` and return the text output.
/// Streams stderr and stdout back to the frontend as `pipeline:log` events.
#[allow(clippy::too_many_arguments)]
pub async fn call_gemini(
    app: &crate::emit::EventBus,
    prompt: &str,
    allowed_tools: &[&str],
    _system_prompt: Option<&str>,
    _output_format: &str,
    timeout_secs: u64,
    label: &str,
    cwd: Option<&str>,
    extra_read_dirs: &[&str],
    overrides: &LlmOverrides<'_>,
) -> Result<String, String> {
    let mut cmd_args: Vec<String> = Vec::new();

    // Gemini's file tools confine writes to the workspace (cwd) at the
    // tool layer, symlink-resolved — so write mode means: cwd = artifact
    // dir, approval mode auto_edit (auto-approves file edits; shell stays
    // ask_user, which non-interactive mode treats as deny).
    // Read-only mode keeps "plan": reads auto-approved, writes blocked.
    let needs_write = allowed_tools.iter().any(|t| *t == "Write" || *t == "Edit")
        && overrides.write_dir.is_some();
    let workspace = plan_cli_workspace(
        cwd,
        extra_read_dirs,
        None,
        if needs_write {
            overrides.write_dir
        } else {
            None
        },
    )?;
    cmd_args.push("--approval-mode".to_string());
    cmd_args.push(if needs_write { "auto_edit" } else { "plan" }.to_string());
    let core_tools = gemini_core_tools(allowed_tools, needs_write);

    // Keep artifacts as cwd in write mode. Every source/input root remains
    // available through a repeated flag (rather than a comma-delimited value,
    // which would break valid directory names containing commas).
    append_gemini_read_dirs(&mut cmd_args, &workspace.read_dirs);

    let write_root = if needs_write {
        Some(
            workspace
                .cwd
                .as_deref()
                .ok_or("Gemini write mode requires an absolute artifact directory")?,
        )
    } else {
        None
    };
    let _tool_policy = create_gemini_policy(write_root, &core_tools)?;
    cmd_args.push("--admin-policy".to_string());
    cmd_args.push(_tool_policy.path().to_string_lossy().replace('\\', "/"));
    let _system_settings = create_gemini_system_settings(&core_tools)?;

    // Gemini's JSON envelope keeps the terminal response separate from
    // diagnostics and stats.
    cmd_args.push("-o".to_string());
    cmd_args.push("json".to_string());

    // Apply Gemini settings (model) with optional per-step override.
    // Gemini CLI doesn't expose an effort flag, so overrides.effort is ignored here.
    let settings = overrides
        .settings
        .cloned()
        .unwrap_or_else(crate::settings::load);
    let model_src = if overrides.model_resolved {
        overrides.model.unwrap_or("")
    } else {
        overrides.model.unwrap_or(settings.gemini_model.as_str())
    };
    let model = crate::settings::sanitize_cli_arg(model_src);
    if !model.is_empty() {
        cmd_args.push("-m".to_string());
        cmd_args.push(model);
    }

    // Gemini uses -p "prompt" for non-interactive mode.
    // Its CLI explicitly appends stdin to this value, so pipe the complete
    // prompt and close the stream instead of asking the model to read a temp
    // file as its first action.
    cmd_args.push("-p".to_string());
    cmd_args.push(String::new());

    // Log the command (truncated)
    let display_args: String = cmd_args
        .iter()
        .map(|a| {
            if a.len() > 80 {
                format!("{}...", a.chars().take(80).collect::<String>())
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    verbose_log(app, format!("$ gemini {display_args}"));

    // The cwd defines gemini's writable workspace, so in write mode it must
    // be the artifact dir regardless of what the caller passed.
    let effective_cwd = workspace.cwd.as_deref();
    let mut cmd = build_provider_command("gemini", effective_cwd, &cmd_args)?;
    cmd.env("GEMINI_CLI_SYSTEM_SETTINGS_PATH", _system_settings.path());
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn gemini: {e}. Is Gemini CLI installed?"))?;
    super::logging::record_provider_attempt();

    let (pid, start_time) = track_child_started(&child, app, label);
    let mut prompt_stdin = child
        .stdin
        .take()
        .ok_or("Gemini process did not expose stdin")?;
    let prompt_bytes = prompt.as_bytes().to_vec();
    let stdin_task = tokio::spawn(async move {
        prompt_stdin
            .write_all(&prompt_bytes)
            .await
            .map_err(|error| format!("Failed to send prompt to Gemini: {error}"))?;
        prompt_stdin
            .shutdown()
            .await
            .map_err(|error| format!("Failed to close Gemini prompt stream: {error}"))
    });

    // Stream stderr to the frontend, retaining the tail for diagnostics.
    let sess = super::logging::current();
    let stderr_task = capture_stderr(child.stderr.take(), app.clone(), sess.clone());

    // Stream stdout to the frontend
    let stdout_task = capture_text_stdout(child.stdout.take(), app.clone(), sess);

    let wait_result = wait_for_child(
        &mut child,
        pid,
        timeout_secs,
        "Gemini",
        "gemini",
        label,
        app,
    )
    .await;
    let streams = finish_streams(stdout_task, stderr_task, pid, "Gemini").await;
    let status = wait_result?;
    let stdin_result = stdin_task
        .await
        .map_err(|error| format!("Gemini prompt writer failed: {error}"))?;
    if status.success() {
        stdin_result?;
    }
    let ((raw_stdout, stdout_overflowed), stderr_tail) = streams?;
    if stdout_overflowed {
        emit_stderr_tail(app, &stderr_tail);
        return Err(format!(
            "Gemini stdout exceeded the {} MB safety limit",
            super::claude::MAX_STDOUT_BYTES / 1024 / 1024
        ));
    }
    let exit_code = status.code().unwrap_or(-1);
    if !status.success() {
        if crate::commands::is_cancelled() || exit_code == 143 || status.code().is_none() {
            log(app, format!("{label} cancelled"));
            return Err("Pipeline cancelled".into());
        }
        emit_stderr_tail(app, &stderr_tail);
        let hint = gemini_error_hint(&raw_stdout)
            .or_else(|| super::claude::extract_error_hint(&raw_stdout))
            .or_else(|| last_stderr_hint(&stderr_tail));
        let msg = if let Some(hint) = hint {
            format!("Gemini call failed (exit {exit_code}): {hint}")
        } else {
            format!(
                "Gemini call failed (exit {exit_code}). See this call's session log for details."
            )
        };
        log(app, format!("ERROR: {msg}"));
        return Err(msg);
    }

    let (text, gemini_usage) = parse_gemini_result(&raw_stdout).map_err(|error| {
        emit_stderr_tail(app, &stderr_tail);
        let msg = format!("Gemini returned an invalid JSON result envelope: {error}");
        log(app, format!("ERROR: {msg}"));
        msg
    })?;
    let elapsed = start_time.elapsed().as_secs();
    let token_info = match gemini_usage {
        Some(usage) if usage.input_tokens > 0 || usage.output_tokens > 0 => format!(
            ", {}+{} tokens, {} cached",
            usage.input_tokens, usage.output_tokens, usage.cached_input_tokens
        ),
        _ => String::new(),
    };
    log(
        app,
        format!(
            "{label} finished ({elapsed}s, exit code {exit_code}, {} chars output{token_info})",
            text.len()
        ),
    );
    if let Some(usage) = gemini_usage {
        super::logging::emit_usage(app, usage);
    }

    if text.is_empty() {
        emit_stderr_tail(app, &stderr_tail);
        let msg = "Gemini returned empty output.";
        log(app, format!("ERROR: {msg}"));
        return Err(msg.to_string());
    }

    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_workspace_retains_source_run_inputs_and_long_prompt() {
        let plan = plan_cli_workspace(
            Some("/Users/Mike/Documents/Paper"),
            &[
                "/Users/Mike/Documents/Paper",
                "/private/tmp/pipeline_run_inputs/paper-and-orientation",
                "/private/tmp/pipeline_run_inputs/paper-and-orientation/named-inputs",
            ],
            Some("/private/tmp/pipeline_prompt_1"),
            Some("/Users/Mike/.pipeline/runs/r1/artifacts"),
        )
        .unwrap();

        assert_eq!(
            plan.cwd.as_deref(),
            Some("/Users/Mike/.pipeline/runs/r1/artifacts")
        );
        assert_eq!(
            plan.read_dirs,
            vec![
                "/Users/Mike/Documents/Paper",
                "/private/tmp/pipeline_prompt_1",
                "/private/tmp/pipeline_run_inputs/paper-and-orientation",
            ]
        );

        let mut args = Vec::new();
        append_gemini_read_dirs(&mut args, &plan.read_dirs);
        assert_eq!(
            args,
            vec![
                "--include-directories",
                "/Users/Mike/Documents/Paper",
                "--include-directories",
                "/private/tmp/pipeline_prompt_1",
                "--include-directories",
                "/private/tmp/pipeline_run_inputs/paper-and-orientation",
            ]
        );
    }

    #[test]
    fn include_directory_flags_preserve_commas_and_spaces() {
        let roots = vec![
            "/Users/Mike/Paper, revised".to_string(),
            "C:/Users/Mike/Named Inputs".to_string(),
        ];
        let mut args = Vec::new();
        append_gemini_read_dirs(&mut args, &roots);
        assert_eq!(args[1], "/Users/Mike/Paper, revised");
        assert_eq!(args[3], "C:/Users/Mike/Named Inputs");
    }

    #[test]
    fn write_policy_allows_only_artifact_descendants() {
        let allowed = regex::Regex::new(&gemini_allowed_write_pattern(
            "/Users/Mike/.pipeline/runs/r1/artifacts",
        ))
        .unwrap();
        let inside = serde_json::json!({
            "content": "report",
            "file_path": "/Users/Mike/.pipeline/runs/r1/artifacts/steps/report.md"
        })
        .to_string();
        let outside = serde_json::json!({
            "content": "bad",
            "file_path": "/Users/Mike/Documents/Paper/source.tex"
        })
        .to_string();
        assert!(allowed.is_match(&inside));
        assert!(!allowed.is_match(&outside));
    }

    #[test]
    fn traversal_rule_catches_posix_and_windows_shapes_before_allow() {
        let traversal = regex::Regex::new(GEMINI_TRAVERSAL_PATTERN).unwrap();
        let posix = serde_json::json!({
            "file_path": "/Users/Mike/.pipeline/runs/r1/artifacts/../source.tex"
        })
        .to_string();
        let windows = serde_json::json!({
            "file_path": r"C:\Users\Mike\.pipeline\runs\r1\artifacts\..\source.tex"
        })
        .to_string();
        assert!(traversal.is_match(&posix));
        assert!(traversal.is_match(&windows));

        let tools = gemini_core_tools(&["WebSearch", "Write"], true);
        let policy = gemini_tool_policy(Some("C:/Users/Mike/.pipeline/runs/r1/artifacts"), &tools);
        let traversal_pos = policy.find("priority = 999").unwrap();
        let allow_pos = policy.find("priority = 998").unwrap();
        let fallback_deny_pos = policy.rfind("priority = 900").unwrap();
        assert!(traversal_pos < fallback_deny_pos && allow_pos < fallback_deny_pos);
        assert!(policy.contains("toolName = [\"write_file\", \"replace\"]"));
        assert!(policy.contains("argsPattern"));
        assert!(policy.contains("toolName = \"*\""));
    }

    #[test]
    fn windows_artifact_pattern_matches_normalized_prompt_path() {
        let allowed = regex::Regex::new(&gemini_allowed_write_pattern(
            "C:/Users/Mike/.pipeline/runs/r1/artifacts",
        ))
        .unwrap();
        let args = serde_json::json!({
            "file_path": "C:/Users/Mike/.pipeline/runs/r1/artifacts/steps/report.md",
            "content": "report"
        })
        .to_string();
        assert!(allowed.is_match(&args));
    }

    #[test]
    fn exact_capabilities_exclude_ambient_gemini_tools() {
        let read_only = gemini_core_tools(&[], false);
        assert_eq!(read_only, GEMINI_READ_TOOLS);
        for forbidden in [
            "run_shell_command",
            "web_fetch",
            "google_web_search",
            "write_file",
            "replace",
            "activate_skill",
            "list_mcp_resources",
        ] {
            assert!(!read_only.contains(&forbidden), "{forbidden}");
        }

        let web_and_write = gemini_core_tools(&["WebSearch", "Write"], true);
        assert!(web_and_write.contains(&"google_web_search"));
        assert!(web_and_write.contains(&"write_file"));
        assert!(web_and_write.contains(&"replace"));
        assert!(!web_and_write.contains(&"web_fetch"));
        assert!(!web_and_write.contains(&"run_shell_command"));
    }

    #[test]
    fn isolated_settings_disable_ambient_extension_points() {
        let settings = restricted_gemini_settings(
            serde_json::json!({
                "mcpServers": {"ambient": {"command": "dangerous"}},
                "skills": {"enabled": true},
                "hooksConfig": {"enabled": true},
                "tools": {"discoveryCommand": "discover-tools"},
                "adminPolicyPaths": ["/managed/policies"]
            }),
            GEMINI_READ_TOOLS,
        );
        assert_eq!(
            settings["tools"]["core"],
            serde_json::json!(GEMINI_READ_TOOLS)
        );
        assert_eq!(settings["tools"]["discoveryCommand"], "");
        assert_eq!(settings["tools"]["callCommand"], "");
        assert_eq!(settings["mcpServers"], serde_json::json!({}));
        assert_eq!(settings["mcp"]["allowed"], serde_json::json!([]));
        assert_eq!(settings["skills"]["enabled"], false);
        assert_eq!(settings["hooksConfig"]["enabled"], false);
        assert_eq!(settings["admin"]["extensions"]["enabled"], false);
        assert_eq!(settings["admin"]["mcp"]["enabled"], false);
        assert_eq!(settings["admin"]["skills"]["enabled"], false);
        assert_eq!(
            settings["adminPolicyPaths"],
            serde_json::json!(["/managed/policies"])
        );
    }

    #[test]
    fn json_envelope_returns_only_terminal_response_and_usage() {
        let raw = r#"{
          "session_id":"s1",
          "response":"  ## Final report\n\nClean.  ",
          "stats":{"models":{
            "gemini-3-pro":{"api":{"totalRequests":4},"tokens":{
              "prompt":120,"tool":7,"candidates":30,"thoughts":11,"cached":10
            }}
          },"tools":{"totalCalls":5,"byName":{
            "read_file":{"count":2},"google_web_search":{"calls":1},
            "future_tool":{"totalCalls":1}
          }}}
        }"#;
        let (text, usage) = parse_gemini_result(raw).unwrap();
        assert_eq!(text, "## Final report\n\nClean.");
        assert_eq!(
            usage,
            Some(crate::pipeline::logging::CallUsage {
                input_tokens: 127,
                output_tokens: 41,
                cached_input_tokens: 10,
                cache_write_input_tokens: 0,
                model_round_trips: 4,
                tool_calls: crate::models::ToolCallCounts {
                    text_file: 2,
                    web: 1,
                    unknown: 2,
                    ..Default::default()
                },
                ..Default::default()
            })
        );
        // Cached prompt tokens are already inside `prompt`: 120 prompt + 7
        // tool-use prompt = 127 logical input, not 137.
        assert_eq!(usage.unwrap().input_tokens, 127);
    }

    #[test]
    fn malformed_or_error_json_envelopes_are_rejected() {
        assert!(parse_gemini_result("narration before the report").is_err());
        assert!(parse_gemini_result(r#"{"error":{"message":"capacity exhausted"}}"#).is_err());
        assert!(parse_gemini_result(r#"{"session_id":"s1"}"#).is_err());
    }
}
