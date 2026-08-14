use std::process::Stdio;
use tokio::io::{AsyncWriteExt, BufReader};

use super::claude::{build_provider_command, normalize_cli_root, LlmOverrides};
use super::cli_process::{
    capture_stderr, emit_stderr_tail, finish_streams, last_stderr_hint, log, track_child_started,
    verbose_log, wait_for_child,
};

fn codex_effective_cwd(
    needs_write: bool,
    cwd: Option<&str>,
    write_dir: Option<&str>,
    prompt_root: Option<&str>,
) -> Result<Option<String>, String> {
    (if needs_write { write_dir.or(cwd) } else { cwd })
        .or(prompt_root)
        .map(|path| {
            normalize_cli_root(path)
                .ok_or_else(|| format!("Codex working directory must be absolute: {path}"))
        })
        .transpose()
}

#[derive(Clone, Copy)]
enum CodexSessionMode<'a> {
    New,
    Resume(&'a str),
}

struct CodexInvocation {
    text: String,
    thread_id: Option<String>,
}

#[derive(Debug)]
struct CodexForkSpec<'a> {
    cwd: &'a str,
    sandbox: &'static str,
    model: Option<&'a str>,
}

const SESSION_UNAVAILABLE_PREFIX: &str = "unavailable:";

fn read_last_message_file(path: &std::path::Path) -> Result<Option<String>, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("Could not inspect Codex final-message file: {error}"))?;
    if metadata.len() > super::claude::MAX_STDOUT_BYTES as u64 {
        return Err(format!(
            "Codex final message exceeded the {} MB safety limit",
            super::claude::MAX_STDOUT_BYTES / 1_000_000
        ));
    }
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("Could not read Codex final-message file: {error}"))?;
    let trimmed = text.trim();
    Ok((!trimmed.is_empty()).then(|| trimmed.to_string()))
}

fn update_last_agent_message(
    event: &serde_json::Value,
    last_agent_text: &mut String,
) -> Result<bool, String> {
    let Some(item) = event.get("item") else {
        return Ok(false);
    };
    if item.get("type").and_then(|kind| kind.as_str()) != Some("agent_message") {
        return Ok(false);
    }
    let Some(text) = item.get("text").and_then(|text| text.as_str()) else {
        return Ok(false);
    };
    if text.len() > super::claude::MAX_STDOUT_BYTES {
        return Err(format!(
            "Codex agent message exceeded the {} MB safety limit",
            super::claude::MAX_STDOUT_BYTES / 1_000_000
        ));
    }
    last_agent_text.clear();
    last_agent_text.push_str(text);
    Ok(true)
}

fn codex_completed_tool_kind(event: &serde_json::Value) -> Option<crate::models::ToolCallKind> {
    use crate::models::ToolCallKind;

    if event.get("type").and_then(|kind| kind.as_str()) != Some("item.completed") {
        return None;
    }
    let item = event.get("item")?;
    let item_type = item.get("type").and_then(|kind| kind.as_str())?;
    match item_type {
        "file_change" | "file_read" | "file_write" => Some(ToolCallKind::TextFile),
        "command_execution" | "computer_call" | "computer_action" => {
            Some(ToolCallKind::ShellOrOther)
        }
        "web_search" | "web_fetch" => Some(ToolCallKind::Web),
        "image_generation" | "image_view" | "view_image" => Some(ToolCallKind::Image),
        "mcp_tool_call" | "function_call" | "tool_call" => {
            let name = item
                .get("tool")
                .or_else(|| item.get("name"))
                .or_else(|| {
                    item.get("function")
                        .and_then(|function| function.get("name"))
                })
                .and_then(|name| name.as_str())
                .unwrap_or("");
            Some(super::logging::classify_tool_name(name))
        }
        // Preserve future tool-like item kinds in an explicit unknown bucket.
        other if other.contains("tool") || other.ends_with("_call") => Some(ToolCallKind::Unknown),
        _ => None,
    }
}

fn is_session_resume_error(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    [
        "unknown option",
        "unexpected argument",
        "session not found",
        "thread not found",
        "failed to resume",
        "failed to load rollout",
    ]
    .iter()
    .any(|needle| error.contains(needle))
}

fn codex_web_search_override(allowed_tools: &[&str]) -> String {
    let mode = if allowed_tools.contains(&"WebSearch") {
        "live"
    } else {
        "disabled"
    };
    format!("web_search=\"{mode}\"")
}

/// Call `codex exec` and return the text output.
/// Streams stderr and stdout back to the frontend as `pipeline:log` events.
#[allow(clippy::too_many_arguments)]
pub async fn call_codex(
    app: &crate::emit::EventBus,
    prompt: &str,
    allowed_tools: &[&str],
    system_prompt: Option<&str>,
    _output_format: &str,
    timeout_secs: u64,
    label: &str,
    cwd: Option<&str>,
    overrides: &LlmOverrides<'_>,
) -> Result<String, String> {
    let Some(context) = overrides.shared_context.as_ref() else {
        return call_codex_inner(
            app,
            prompt,
            allowed_tools,
            system_prompt,
            timeout_secs,
            label,
            cwd,
            overrides,
            CodexSessionMode::New,
        )
        .await
        .map(|result| result.text);
    };

    let settings = overrides
        .settings
        .cloned()
        .unwrap_or_else(crate::settings::load);
    let model = if overrides.model_resolved {
        overrides.model.unwrap_or("")
    } else {
        overrides.model.unwrap_or(settings.codex_model.as_str())
    };
    let effort = overrides.effort.unwrap_or(settings.codex_effort.as_str());
    let session_key =
        context.compatibility_key("codex-cli", [model, effort, system_prompt.unwrap_or("")]);
    let slot = context.slot(session_key).await;

    let base_result = {
        let mut base = slot.lock().await;
        if let Some(id) = base.as_ref() {
            if let Some(reason) = id.strip_prefix(SESSION_UNAVAILABLE_PREFIX) {
                Err(reason.to_string())
            } else {
                Ok(id.clone())
            }
        } else {
            let primer = format!(
                "{}\n\nReply with exactly: Context prepared.",
                context.content()
            );
            let mut primer_overrides = overrides.clone();
            primer_overrides.shared_context = None;
            primer_overrides.write_dir = None;
            let primer_label = format!("{label} · cache warm-up");
            match call_codex_inner(
                app,
                &primer,
                &[],
                system_prompt,
                timeout_secs,
                &primer_label,
                Some(context.workspace_dir()),
                &primer_overrides,
                CodexSessionMode::New,
            )
            .await
            {
                Ok(result) => match result.thread_id {
                    Some(id) => {
                        *base = Some(id.clone());
                        Ok(id)
                    }
                    None => {
                        let error = "Codex did not report a reusable thread id".to_string();
                        *base = Some(format!("{SESSION_UNAVAILABLE_PREFIX}{error}"));
                        Err(error)
                    }
                },
                Err(error) => {
                    // A cancelled warm-up says nothing about the CLI's
                    // session support; leave the slot empty so a later unit
                    // can warm the shared session again.
                    if !error.to_ascii_lowercase().contains("cancelled") {
                        *base = Some(format!("{SESSION_UNAVAILABLE_PREFIX}{error}"));
                    }
                    Err(error)
                }
            }
        }
    };

    let needs_write = allowed_tools
        .iter()
        .any(|tool| *tool == "Write" || *tool == "Edit")
        && overrides.write_dir.is_some();
    let branch_cwd = codex_effective_cwd(
        needs_write,
        cwd,
        overrides.write_dir,
        Some(context.workspace_dir()),
    )?
    .ok_or_else(|| "Codex shared-context fork has no working directory".to_string())?;
    let fork_spec = CodexForkSpec {
        cwd: &branch_cwd,
        sandbox: if needs_write {
            "workspace-write"
        } else {
            "read-only"
        },
        model: (!model.is_empty()).then_some(model),
    };
    let branch_result = match base_result {
        Ok(base_id) => fork_codex_thread(&base_id, &fork_spec)
            .await
            .map(|id| (base_id, id)),
        Err(error) => Err(error),
    };

    match branch_result {
        Ok((_base_id, branch_id)) => {
            log(
                app,
                format!("{label}: using forked Codex shared-context session"),
            );
            let result = call_codex_inner(
                app,
                prompt,
                allowed_tools,
                system_prompt,
                timeout_secs,
                label,
                cwd,
                overrides,
                CodexSessionMode::Resume(&branch_id),
            )
            .await;
            match result {
                Ok(result) => Ok(result.text),
                Err(error) if is_session_resume_error(&error) => {
                    let mut base = slot.lock().await;
                    *base = Some(format!("{SESSION_UNAVAILABLE_PREFIX}{error}"));
                    drop(base);
                    log(
                        app,
                        format!(
                            "WARNING: {label}: installed Codex CLI could not resume the forked thread ({error}); using a self-contained call"
                        ),
                    );
                    let fallback = context.prefixed_prompt(prompt)?;
                    let mut fallback_overrides = overrides.clone();
                    fallback_overrides.shared_context = None;
                    call_codex_inner(
                        app,
                        &fallback,
                        allowed_tools,
                        system_prompt,
                        timeout_secs,
                        label,
                        cwd,
                        &fallback_overrides,
                        CodexSessionMode::New,
                    )
                    .await
                    .map(|result| result.text)
                }
                Err(error) => Err(error),
            }
        }
        Err(error) => {
            // A cancelled warm-up or fork must not disable caching for the
            // remaining units, and dispatching a fallback call mid-cancel
            // would spawn a fresh Codex process; propagate it instead.
            if error.to_ascii_lowercase().contains("cancelled") {
                return Err(error);
            }
            let mut base = slot.lock().await;
            *base = Some(format!("{SESSION_UNAVAILABLE_PREFIX}{error}"));
            drop(base);
            log(
                app,
                format!(
                    "WARNING: {label}: Codex session cache was unavailable ({error}); using a self-contained call"
                ),
            );
            let fallback = context.prefixed_prompt(prompt)?;
            let mut fallback_overrides = overrides.clone();
            fallback_overrides.shared_context = None;
            call_codex_inner(
                app,
                &fallback,
                allowed_tools,
                system_prompt,
                timeout_secs,
                label,
                cwd,
                &fallback_overrides,
                CodexSessionMode::New,
            )
            .await
            .map(|result| result.text)
        }
    }
}

fn validate_codex_fork(
    result: &serde_json::Value,
    spec: &CodexForkSpec<'_>,
) -> Result<String, String> {
    let returned_cwd = result
        .get("cwd")
        .and_then(serde_json::Value::as_str)
        .and_then(normalize_cli_root)
        .ok_or_else(|| "Codex thread/fork returned no valid working directory".to_string())?;
    if returned_cwd != spec.cwd {
        return Err(format!(
            "Codex thread/fork did not preserve the requested working directory (requested {}, received {returned_cwd})",
            spec.cwd
        ));
    }

    let expected_policy = if spec.sandbox == "workspace-write" {
        "workspaceWrite"
    } else {
        "readOnly"
    };
    let sandbox = result
        .get("sandbox")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "Codex thread/fork returned no sandbox policy".to_string())?;
    let returned_policy = sandbox
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if returned_policy != expected_policy {
        return Err(format!(
            "Codex thread/fork returned sandbox '{returned_policy}', expected '{expected_policy}'"
        ));
    }
    if sandbox.get("networkAccess").and_then(|v| v.as_bool()) == Some(true) {
        return Err("Codex thread/fork unexpectedly granted network access".to_string());
    }
    if spec.sandbox == "workspace-write" {
        if sandbox.get("excludeSlashTmp").and_then(|v| v.as_bool()) != Some(true)
            || sandbox.get("excludeTmpdirEnvVar").and_then(|v| v.as_bool()) != Some(true)
        {
            return Err(
                "Codex thread/fork did not preserve the requested workspace-write restrictions"
                    .to_string(),
            );
        }
        let writable_roots = sandbox
            .get("writableRoots")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "Codex thread/fork returned no writable-root policy".to_string())?;
        if !writable_roots.is_empty() {
            return Err(
                "Codex thread/fork unexpectedly granted additional writable roots".to_string(),
            );
        }
    }

    let roots = result
        .get("runtimeWorkspaceRoots")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "Codex thread/fork returned no runtime workspace roots".to_string())?;
    let returned_roots = roots
        .iter()
        .filter_map(serde_json::Value::as_str)
        .filter_map(normalize_cli_root)
        .collect::<Vec<_>>();
    if returned_roots != [spec.cwd.to_string()] {
        return Err(format!(
            "Codex thread/fork returned unexpected runtime workspace roots: {returned_roots:?}"
        ));
    }
    if let Some(model) = spec.model {
        if result.get("model").and_then(serde_json::Value::as_str) != Some(model) {
            return Err("Codex thread/fork did not preserve the requested model".to_string());
        }
    }

    result
        .get("thread")
        .and_then(|thread| thread.get("id"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "Codex thread/fork returned no child thread id".to_string())
}

async fn fork_codex_thread(thread_id: &str, spec: &CodexForkSpec<'_>) -> Result<String, String> {
    let mut fork_params = serde_json::json!({
        "threadId": thread_id,
        "cwd": spec.cwd,
        "sandbox": spec.sandbox,
        "runtimeWorkspaceRoots": [spec.cwd],
    });
    if spec.sandbox == "workspace-write" {
        fork_params["config"] = serde_json::json!({
            "sandbox_workspace_write": {
                "exclude_slash_tmp": true,
                "exclude_tmpdir_env_var": true,
                "network_access": false
            }
        });
    }
    if let Some(model) = spec.model {
        fork_params["model"] = serde_json::Value::String(model.to_string());
    }
    let requests = [
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": {
                    "name": "pipeline",
                    "title": "Pipeline",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": { "experimentalApi": true }
            }
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "thread/fork",
            "params": fork_params
        }),
    ];
    let replies = crate::model_catalog::rpc_exchange(
        "codex",
        &["app-server".into(), "--listen".into(), "stdio://".into()],
        &requests,
    )
    .await?;
    let result = replies
        .get(1)
        .and_then(|reply| reply.get("result"))
        .ok_or_else(|| "Codex thread/fork returned no result".to_string())?;
    validate_codex_fork(result, spec)
}

#[allow(clippy::too_many_arguments)]
async fn call_codex_inner(
    app: &crate::emit::EventBus,
    prompt: &str,
    allowed_tools: &[&str],
    system_prompt: Option<&str>,
    timeout_secs: u64,
    label: &str,
    cwd: Option<&str>,
    overrides: &LlmOverrides<'_>,
    session_mode: CodexSessionMode<'_>,
) -> Result<CodexInvocation, String> {
    let mut cmd_args: Vec<String> = vec!["exec".to_string()];
    if matches!(session_mode, CodexSessionMode::Resume(_)) {
        cmd_args.push("resume".to_string());
    }
    // Use JSON mode for clean machine-readable output
    cmd_args.push("--json".to_string());
    // Do not inherit ambient search settings from the user's Codex config.
    // Contribution gets current/live search; every other review branch and
    // the shared-context primer keep search disabled.
    cmd_args.push("-c".to_string());
    cmd_args.push(codex_web_search_override(allowed_tools));

    // codex exec refuses to run outside a "trusted" git repo unless this is
    // set ("Not inside a trusted directory and --skip-git-repo-check was not
    // specified."). Our subprocess cwd is the system temp dir (read-only
    // calls) or the run's artifact dir (write calls) — neither is a git repo —
    // so without this flag codex exits without ever calling the model, leaving
    // empty output. Sandboxing is enforced independently via --sandbox, so
    // skipping the git-repo check costs nothing.
    cmd_args.push("--skip-git-repo-check".to_string());

    // Determine sandbox mode: workspace-write only when the step may write
    // AND we have an artifact dir to confine writes to (the caller sets the
    // subprocess cwd there, which is what defines codex's workspace).
    let needs_write = allowed_tools.iter().any(|t| *t == "Write" || *t == "Edit")
        && overrides.write_dir.is_some();
    let sandbox = if needs_write {
        "workspace-write"
    } else {
        "read-only"
    };
    if matches!(session_mode, CodexSessionMode::New) {
        cmd_args.push("--sandbox".to_string());
        cmd_args.push(sandbox.to_string());
    }

    if needs_write {
        // workspace-write also opens /tmp and $TMPDIR by default; close
        // them so a step can't touch another step's temp prompt files.
        // (Reads stay unrestricted — the sandbox governs writes only.)
        cmd_args.push("-c".to_string());
        cmd_args.push("sandbox_workspace_write.exclude_slash_tmp=true".to_string());
        cmd_args.push("-c".to_string());
        cmd_args.push("sandbox_workspace_write.exclude_tmpdir_env_var=true".to_string());

        // On native Windows, workspace-write silently downgrades to
        // read-only unless codex's (experimental) Windows sandbox is
        // enabled. The unelevated restricted-token backend needs no admin
        // setup and no feature flag; enabling it per-invocation keeps the
        // user's ~/.codex/config.toml untouched. If the model still can't
        // write, the executor falls back to stdout output.
        #[cfg(windows)]
        {
            cmd_args.push("-c".to_string());
            cmd_args.push("windows.sandbox=\"unelevated\"".to_string());
        }
    }

    // System prompt via config override
    if let Some(sys) = system_prompt {
        cmd_args.push("-c".to_string());
        cmd_args.push(format!("instructions={}", sys));
    }

    // Apply Codex settings (model, reasoning effort) with optional per-step overrides.
    let settings = overrides
        .settings
        .cloned()
        .unwrap_or_else(crate::settings::load);
    let model_src = if overrides.model_resolved {
        overrides.model.unwrap_or("")
    } else {
        overrides.model.unwrap_or(settings.codex_model.as_str())
    };
    let model = crate::settings::sanitize_cli_arg(model_src);
    if !model.is_empty() {
        cmd_args.push("--model".to_string());
        cmd_args.push(model);
    }
    let effort_src = overrides.effort.unwrap_or(settings.codex_effort.as_str());
    let effort = crate::settings::sanitize_cli_arg(effort_src);
    if !effort.is_empty() {
        cmd_args.push("-c".to_string());
        cmd_args.push(format!("model_reasoning_effort={}", effort));
    }

    // Codex reads the full prompt from a closed pipe. This avoids turning a
    // long prompt into a meta-instruction to use a Read tool, which can surface
    // as progress narration and makes prompt delivery tool-dependent.
    // JSONL contains progress/commentary messages as well as the terminal
    // response. Have Codex write the one terminal message separately so those
    // channels can never be flattened into a report.
    let last_message_dir = tempfile::Builder::new()
        .prefix("pipeline_codex_final_")
        .tempdir()
        .map_err(|error| format!("Failed to create Codex final-message directory: {error}"))?;
    let last_message_path = last_message_dir.path().join("final.txt");
    cmd_args.push("--output-last-message".to_string());
    cmd_args.push(last_message_path.to_string_lossy().replace('\\', "/"));
    if let CodexSessionMode::Resume(id) = session_mode {
        cmd_args.push(id.to_string());
    }
    cmd_args.push("-".to_string());

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
    verbose_log(app, format!("$ codex {display_args}"));

    // The cwd defines codex's writable workspace, so in write mode it must
    // be the artifact dir regardless of what the caller passed.
    let effective_cwd = codex_effective_cwd(needs_write, cwd, overrides.write_dir, None)?;
    let mut cmd = build_provider_command("codex", effective_cwd.as_deref(), &cmd_args)?;
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn codex: {e}. Is Codex CLI installed?"))?;
    super::logging::record_provider_attempt();

    let (pid, start_time) = track_child_started(&child, app, label);
    let mut prompt_stdin = child
        .stdin
        .take()
        .ok_or("Codex process did not expose stdin")?;
    let prompt_bytes = prompt.as_bytes().to_vec();
    let stdin_task = tokio::spawn(async move {
        prompt_stdin
            .write_all(&prompt_bytes)
            .await
            .map_err(|error| format!("Failed to send prompt to Codex: {error}"))?;
        prompt_stdin
            .shutdown()
            .await
            .map_err(|error| format!("Failed to close Codex prompt stream: {error}"))
    });

    // Forward stderr to the verbose log. In --json mode it's mostly TUI
    // noise, but auth and capacity warnings land here too — dropping them
    // made those failures undiagnosable.
    let sess = super::logging::current();
    let stderr_task = capture_stderr(child.stderr.take(), app.clone(), sess.clone());

    // Parse JSONL stdout for telemetry. Keep only the latest agent_message as
    // a compatibility fallback for older CLIs that fail to populate
    // --output-last-message; never concatenate progress and final messages.
    let stdout = child.stdout.take();
    let app_stdout = app.clone();
    let label_clone = label.to_string();
    let stdout_task = tokio::spawn(super::logging::with_session_opt(sess, async move {
        let mut last_agent_text = String::new();
        let mut total_input_tokens: u64 = 0;
        let mut total_output_tokens: u64 = 0;
        let mut total_cached_input_tokens: u64 = 0;
        let mut model_round_trips: u64 = 0;
        let mut tool_calls = crate::models::ToolCallCounts::default();
        let mut thread_id: Option<String> = None;
        let mut overflowed = false;
        let mut stdout_bytes = 0usize;
        if let Some(stdout) = stdout {
            let mut reader = BufReader::new(stdout);
            while let Ok(Some(record)) =
                super::logging::next_bounded_line(&mut reader, super::logging::MAX_CLI_LINE_BYTES)
                    .await
            {
                if record.truncated {
                    if !overflowed {
                        log(&app_stdout, "ERROR: Codex stdout exceeded its safety limit");
                    }
                    overflowed = true;
                    continue;
                }
                stdout_bytes = stdout_bytes.saturating_add(record.text.len() + 1);
                if stdout_bytes > super::claude::MAX_STDOUT_BYTES {
                    if !overflowed {
                        log(
                            &app_stdout,
                            format!(
                                "ERROR: stdout exceeded {} MB",
                                super::claude::MAX_STDOUT_BYTES / 1_000_000
                            ),
                        );
                    }
                    overflowed = true;
                }
                if overflowed {
                    // Continue draining until EOF so the provider cannot
                    // deadlock on a full stdout pipe.
                    continue;
                }
                let line = record.text;
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) {
                    match event.get("type").and_then(|t| t.as_str()) {
                        Some("thread.started") => {
                            thread_id = event
                                .get("thread_id")
                                .or_else(|| event.get("threadId"))
                                .and_then(|value| value.as_str())
                                .map(str::to_string);
                        }
                        Some("item.completed") => {
                            if let Some(kind) = codex_completed_tool_kind(&event) {
                                tool_calls.add_kind(kind, 1);
                            }
                            if let Err(error) =
                                update_last_agent_message(&event, &mut last_agent_text)
                            {
                                log(&app_stdout, format!("ERROR: {error}"));
                                overflowed = true;
                            }
                        }
                        Some("turn.completed") => {
                            model_round_trips = model_round_trips.saturating_add(1);
                            if let Some(usage) = event.get("usage") {
                                total_input_tokens += usage
                                    .get("input_tokens")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);
                                total_output_tokens += usage
                                    .get("output_tokens")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);
                                total_cached_input_tokens += usage
                                    .get("cached_input_tokens")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);
                            }
                        }
                        // Codex reports fatal problems (bad config, auth, model)
                        // as an error event on stdout — surface it, always.
                        Some(t) if t.contains("error") => {
                            let detail = event
                                .get("message")
                                .and_then(|v| v.as_str())
                                .or_else(|| event.get("error").and_then(|e| e.as_str()))
                                .or_else(|| {
                                    event
                                        .get("error")
                                        .and_then(|e| e.get("message"))
                                        .and_then(|v| v.as_str())
                                })
                                .unwrap_or(t);
                            log(&app_stdout, format!("ERROR: [codex] {detail}"));
                        }
                        _ => {}
                    }
                } else {
                    verbose_log(&app_stdout, format!("[codex] {line}"));
                }
            }
        }
        if total_input_tokens > 0 || total_output_tokens > 0 {
            verbose_log(
                &app_stdout,
                format!(
                    "[codex] {}: tokens in={total_input_tokens} out={total_output_tokens} cached={total_cached_input_tokens}",
                    label_clone,
                ),
            );
        }
        (
            last_agent_text,
            total_input_tokens,
            total_output_tokens,
            total_cached_input_tokens,
            model_round_trips,
            tool_calls,
            thread_id,
            overflowed,
        )
    }));

    let wait_result =
        wait_for_child(&mut child, pid, timeout_secs, "Codex", "codex", label, app).await;
    let streams = finish_streams(stdout_task, stderr_task, pid, "Codex").await;
    let status = wait_result?;
    let stdin_result = stdin_task
        .await
        .map_err(|error| format!("Codex prompt writer failed: {error}"))?;
    if status.success() {
        stdin_result?;
    }
    let (
        (
            last_agent_text,
            input_tokens,
            output_tokens,
            cached_input_tokens,
            model_round_trips,
            tool_calls,
            thread_id,
            stdout_overflowed,
        ),
        stderr_tail,
    ) = streams?;
    if stdout_overflowed {
        emit_stderr_tail(app, &stderr_tail);
        return Err(format!(
            "Codex stdout exceeded the {} MB safety limit",
            super::claude::MAX_STDOUT_BYTES / 1024 / 1024
        ));
    }
    let text = match read_last_message_file(&last_message_path) {
        Ok(Some(text)) => text,
        Ok(None) => last_agent_text.trim().to_string(),
        Err(error) => {
            log(
                app,
                format!("WARNING: {label}: {error}; using the final JSONL message"),
            );
            last_agent_text.trim().to_string()
        }
    };

    let exit_code = status.code().unwrap_or(-1);
    let elapsed = start_time.elapsed().as_secs();
    let token_info = if input_tokens > 0 || output_tokens > 0 {
        format!(", {input_tokens}+{output_tokens} tokens, {cached_input_tokens} cached")
    } else {
        String::new()
    };
    log(
        app,
        format!(
            "{label} finished ({elapsed}s, exit code {exit_code}, {} chars output{token_info})",
            text.len()
        ),
    );
    super::logging::emit_usage(
        app,
        super::logging::CallUsage {
            input_tokens,
            output_tokens,
            cached_input_tokens,
            cache_write_input_tokens: 0,
            model_round_trips,
            tool_calls,
            ..Default::default()
        },
    );

    if !status.success() {
        if let Some(terminated) =
            super::claude::classify_terminated_exit("Codex", &status, exit_code)
        {
            log(app, format!("{label}: {terminated}"));
            return Err(terminated);
        }
        emit_stderr_tail(app, &stderr_tail);
        let hint =
            super::claude::extract_error_hint(&text).or_else(|| last_stderr_hint(&stderr_tail));
        let msg = if let Some(hint) = hint {
            format!("Codex call failed (exit {exit_code}): {hint}")
        } else {
            format!(
                "Codex call failed (exit {exit_code}). See this call's session log for details."
            )
        };
        log(app, format!("ERROR: {msg}"));
        return Err(msg);
    }

    if text.is_empty() {
        emit_stderr_tail(app, &stderr_tail);
        let hint = last_stderr_hint(&stderr_tail);
        let msg = match hint {
            Some(h) => format!("Codex returned empty output. Last stderr: {h}"),
            None => "Codex returned empty output.".to_string(),
        };
        log(app, format!("ERROR: {msg}"));
        return Err(msg);
    }

    Ok(CodexInvocation { text, thread_id })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_mode_keeps_windows_artifact_directory_as_workspace() {
        let cwd = codex_effective_cwd(
            true,
            Some(r"C:\Users\Mike\Documents\Paper"),
            Some(r"C:\Users\Mike\.pipeline\runs\r1\artifacts"),
            Some(r"C:\Users\Mike\AppData\Local\Temp\pipeline_prompt"),
        )
        .unwrap();
        assert_eq!(
            cwd.as_deref(),
            Some("C:/Users/Mike/.pipeline/runs/r1/artifacts")
        );
    }

    #[test]
    fn long_prompt_root_is_workspace_when_no_other_cwd_exists() {
        let cwd =
            codex_effective_cwd(false, None, None, Some("/private/tmp/pipeline_prompt")).unwrap();
        assert_eq!(cwd.as_deref(), Some("/private/tmp/pipeline_prompt"));
    }

    #[test]
    fn invalid_write_workspace_fails_closed() {
        assert!(codex_effective_cwd(true, None, Some("relative/artifacts"), None).is_err());
    }

    #[test]
    fn resume_errors_are_distinguished_from_provider_failures() {
        assert!(is_session_resume_error("failed to load rollout for thread"));
        assert!(is_session_resume_error("session not found"));
        assert!(!is_session_resume_error("rate limited"));
    }

    #[test]
    fn reads_only_the_terminal_message_file() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), "  ## Final report\n\nClean.  \n").unwrap();
        assert_eq!(
            read_last_message_file(file.path()).unwrap().as_deref(),
            Some("## Final report\n\nClean.")
        );
    }

    #[test]
    fn jsonl_fallback_replaces_commentary_with_the_terminal_message() {
        let events = [
            serde_json::json!({
                "type": "item.completed",
                "item": {
                    "type": "agent_message",
                    "text": "I’ll read the instructions and use the requested skill."
                }
            }),
            serde_json::json!({
                "type": "item.completed",
                "item": {
                    "type": "agent_message",
                    "text": "## Final report\n\nClean."
                }
            }),
        ];
        let mut fallback = String::new();
        for event in events {
            update_last_agent_message(&event, &mut fallback).unwrap();
        }
        assert_eq!(fallback, "## Final report\n\nClean.");
        assert!(!fallback.contains("instructions"));
    }

    #[test]
    fn completed_codex_items_keep_stable_tool_categories_and_unknowns() {
        use crate::models::ToolCallKind;

        let cases = [
            ("file_change", None, ToolCallKind::TextFile),
            ("command_execution", None, ToolCallKind::ShellOrOther),
            ("web_search", None, ToolCallKind::Web),
            ("image_generation", None, ToolCallKind::Image),
            (
                "mcp_tool_call",
                Some("ReadDocumentAsset"),
                ToolCallKind::Image,
            ),
            ("future_tool_call", None, ToolCallKind::Unknown),
        ];
        for (item_type, name, expected) in cases {
            let mut item = serde_json::json!({ "type": item_type });
            if let Some(name) = name {
                item["tool"] = serde_json::Value::String(name.to_string());
            }
            let event = serde_json::json!({
                "type": "item.completed",
                "item": item
            });
            assert_eq!(codex_completed_tool_kind(&event), Some(expected));
        }

        let message = serde_json::json!({
            "type": "item.completed",
            "item": { "type": "agent_message", "text": "done" }
        });
        assert_eq!(codex_completed_tool_kind(&message), None);
    }

    #[test]
    fn codex_web_search_is_explicitly_scoped_per_turn() {
        assert_eq!(
            codex_web_search_override(&["Read", "WebSearch"]),
            "web_search=\"live\""
        );
        assert_eq!(
            codex_web_search_override(&["Read"]),
            "web_search=\"disabled\""
        );
        assert_eq!(codex_web_search_override(&[]), "web_search=\"disabled\"");
    }

    #[test]
    fn validates_a_scoped_workspace_write_fork() {
        let result = serde_json::json!({
            "cwd": "/tmp/pipeline/run/steps/referee",
            "model": "gpt-5.6-sol",
            "runtimeWorkspaceRoots": ["/tmp/pipeline/run/steps/referee"],
            "sandbox": {
                "type": "workspaceWrite",
                "excludeSlashTmp": true,
                "excludeTmpdirEnvVar": true,
                "networkAccess": false,
                "writableRoots": []
            },
            "thread": { "id": "child-thread" }
        });
        let spec = CodexForkSpec {
            cwd: "/tmp/pipeline/run/steps/referee",
            sandbox: "workspace-write",
            model: Some("gpt-5.6-sol"),
        };

        assert_eq!(validate_codex_fork(&result, &spec).unwrap(), "child-thread");
    }

    #[test]
    fn rejects_a_fork_that_inherits_a_broader_workspace() {
        let result = serde_json::json!({
            "cwd": "/tmp/pipeline/run/steps/referee",
            "model": "gpt-5.6-sol",
            "runtimeWorkspaceRoots": ["/tmp/pipeline/run"],
            "sandbox": {
                "type": "workspaceWrite",
                "excludeSlashTmp": true,
                "excludeTmpdirEnvVar": true,
                "networkAccess": false,
                "writableRoots": []
            },
            "thread": { "id": "child-thread" }
        });
        let spec = CodexForkSpec {
            cwd: "/tmp/pipeline/run/steps/referee",
            sandbox: "workspace-write",
            model: Some("gpt-5.6-sol"),
        };

        assert!(validate_codex_fork(&result, &spec)
            .unwrap_err()
            .contains("unexpected runtime workspace roots"));
    }
}
