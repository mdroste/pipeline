//! Claude CLI transport.

use super::*;

/// Call `claude -p` and return the text output.
/// Streams stderr and stdout back to the frontend as `pipeline:log` events.
///
/// `extra_read_dirs` are passed through as `--add-dir` flags so the Read
/// tool can reach paths outside the cwd. Long prompts add only their private
/// temp directory; run inputs are expected to arrive in an isolated run temp
/// directory supplied by the caller. Paths are canonicalized where possible
/// and normalized to forward slashes for Windows CLI compatibility.
#[derive(Clone, Copy)]
enum ClaudeSessionMode<'a> {
    Start(&'a str),
    ResumeFork { id: &'a str, workspace: &'a str },
}

pub(super) fn is_cli_session_capability_error(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    [
        "unknown option",
        "unexpected argument",
        "fork-session",
        "session not found",
        "cannot resume",
        "failed to resume",
        // Claude CLI resolves `--resume` against a session store scoped by
        // working directory; a session it cannot see fails with
        // "No conversation found with session ID: <uuid>".
        "no conversation found",
        "conversation not found",
        "no such session",
    ]
    .iter()
    .any(|needle| error.contains(needle))
}

/// Whether a failed forked call should be retried as a self-contained call.
/// Cancellation (run- or pass-level, matching `is_cancellation_error` in the
/// executor) must propagate, and a timeout would only double the step's wall
/// clock; every other failure of a fork whose warm-up already succeeded is
/// worth one self-contained attempt rather than failing the step.
pub(super) fn fork_failure_uses_fallback(error: &str) -> bool {
    !crate::commands::is_pipeline_cancellation_error(error)
        && !error.contains("timed out after")
        && !crate::pipeline::provider_error::is_usage_limit_error(error)
}

const SESSION_UNAVAILABLE_PREFIX: &str = "unavailable:";

/// Use one warmed Claude session per compatible model/configuration, then fork
/// it for every task. If the installed CLI cannot create the base session, the
/// call falls back to an ordinary self-contained prompt.
#[allow(clippy::too_many_arguments)]
pub async fn call_claude(
    app: &crate::emit::EventBus,
    prompt: &str,
    allowed_tools: &[&str],
    system_prompt: Option<&str>,
    output_format: &str,
    timeout_secs: u64,
    label: &str,
    cwd: Option<&str>,
    extra_read_dirs: &[&str],
    overrides: &LlmOverrides<'_>,
) -> Result<String, String> {
    let Some(context) = overrides.shared_context.as_ref() else {
        return call_claude_inner(
            app,
            prompt,
            allowed_tools,
            system_prompt,
            output_format,
            timeout_secs,
            label,
            cwd,
            extra_read_dirs,
            overrides,
            None,
        )
        .await;
    };

    let model = overrides.model.unwrap_or("");
    let effort = overrides.effort.unwrap_or("");
    let session_key =
        context.compatibility_key("claude-cli", [model, effort, system_prompt.unwrap_or("")]);
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
            match crate::pipeline::context_cache::new_session_id() {
                Err(error) => Err(error),
                Ok(id) => {
                    let primer = format!(
                        "{}\n\nReply with exactly: Context prepared.",
                        context.content()
                    );
                    let mut primer_overrides = overrides.clone();
                    primer_overrides.shared_context = None;
                    primer_overrides.write_dir = None;
                    primer_overrides.output_schema = None;
                    let primer_label = format!("{label} · cache warm-up");
                    match call_claude_inner(
                        app,
                        &primer,
                        &[],
                        system_prompt,
                        "text",
                        timeout_secs,
                        &primer_label,
                        Some(context.workspace_dir()),
                        &[],
                        &primer_overrides,
                        Some(ClaudeSessionMode::Start(&id)),
                    )
                    .await
                    {
                        Ok(_) => {
                            *base = Some(id.clone());
                            Ok(id)
                        }
                        Err(error) => {
                            // A cancelled warm-up says nothing about the CLI's
                            // session support; leave the slot empty so a later
                            // unit can warm the shared session again.
                            if !crate::commands::is_pipeline_cancellation_error(&error) {
                                *base = Some(format!("{SESSION_UNAVAILABLE_PREFIX}{error}"));
                            }
                            Err(error)
                        }
                    }
                }
            }
        }
    };

    match base_result {
        Ok(base_id) => {
            log(
                app,
                format!("{label}: using forked Claude shared-context session"),
            );
            let result = call_claude_inner(
                app,
                prompt,
                allowed_tools,
                system_prompt,
                output_format,
                timeout_secs,
                label,
                cwd,
                extra_read_dirs,
                overrides,
                Some(ClaudeSessionMode::ResumeFork {
                    id: &base_id,
                    workspace: context.workspace_dir(),
                }),
            )
            .await;
            if let Err(error) = &result {
                if fork_failure_uses_fallback(error) {
                    if is_cli_session_capability_error(error) {
                        // The base session is unusable for forking (for
                        // example, this CLI resolves `--resume` against a
                        // different per-directory session store than the one
                        // the warm-up wrote to). Record that so every sibling
                        // unit and retry goes straight to a self-contained
                        // call instead of re-forking the same dead session.
                        let mut state = slot.lock().await;
                        *state = Some(format!("{SESSION_UNAVAILABLE_PREFIX}{error}"));
                        drop(state);
                        log(
                            app,
                            format!(
                                "WARNING: {label}: installed Claude CLI cannot fork the warmed session ({error}); using a self-contained call"
                            ),
                        );
                    } else {
                        log(
                            app,
                            format!(
                                "WARNING: {label}: forked Claude call failed ({error}); retrying as a self-contained call"
                            ),
                        );
                    }
                    let fallback = context.prefixed_prompt(prompt)?;
                    let mut fallback_overrides = overrides.clone();
                    fallback_overrides.shared_context = None;
                    return call_claude_inner(
                        app,
                        &fallback,
                        allowed_tools,
                        system_prompt,
                        output_format,
                        timeout_secs,
                        label,
                        cwd,
                        extra_read_dirs,
                        &fallback_overrides,
                        None,
                    )
                    .await;
                }
            }
            result
        }
        Err(error) => {
            if crate::commands::is_pipeline_cancellation_error(&error) {
                return Err(error);
            }
            if crate::pipeline::provider_error::is_usage_limit_error(&error) {
                return Err(error);
            }
            log(
                app,
                format!(
                    "WARNING: {label}: Claude session cache was unavailable ({error}); using a self-contained call"
                ),
            );
            let fallback = context.prefixed_prompt(prompt)?;
            let mut fallback_overrides = overrides.clone();
            fallback_overrides.shared_context = None;
            call_claude_inner(
                app,
                &fallback,
                allowed_tools,
                system_prompt,
                output_format,
                timeout_secs,
                label,
                cwd,
                extra_read_dirs,
                &fallback_overrides,
                None,
            )
            .await
        }
    }
}

/// Compute the `--disallowedTools` deny rules for a Claude CLI call.
///
/// `--allowedTools` governs auto-approval, not availability: a tool omitted
/// from it can still be inherited from the user's own Claude Code configuration
/// and, under `--permission-mode acceptEdits`, silently auto-approved. So
/// anything that must never run has to be denied explicitly. Deny rules outrank
/// both allow rules and the permission mode.
pub(super) fn cli_disallowed_tools(
    allowed_tools: &[&str],
    read_dirs: &[String],
    write_enabled: bool,
) -> Vec<String> {
    let mut denies = Vec::new();
    // Pipeline profiles can derive Read, scoped edits, and WebSearch only.
    // Explicitly deny every ambient Claude Code capability that could be
    // inherited from a permissive personal configuration. The MCP wildcard is
    // defense in depth alongside --safe-mode.
    for tool in [
        "Agent",
        "AskUserQuestion",
        "Bash",
        "BashOutput",
        "EnterPlanMode",
        "ExitPlanMode",
        "Glob",
        "Grep",
        "KillShell",
        "Skill",
        "Task",
        "TaskCreate",
        "TaskGet",
        "TaskList",
        "TaskUpdate",
        "TodoWrite",
        "WebFetch",
        "mcp__*",
    ] {
        if !allowed_tools.contains(&tool) {
            denies.push(tool.to_string());
        }
    }
    if !allowed_tools.contains(&"WebSearch") {
        // A non-search review must not inherit search from the user's config.
        denies.push("WebSearch".to_string());
    }
    if write_enabled {
        // Writing is scoped to the producer-owned write root (the cwd). Deny
        // edits in the selected read roots so writes cannot leak there. An
        // Edit(path) rule governs the Write, Edit, and NotebookEdit tools
        // together.
        denies.extend(
            read_dirs
                .iter()
                .map(|d| format!("Edit({}/**)", absolute_rule_path(d))),
        );
    } else {
        // Read-only call: there is no producer write root, so the model must
        // not edit anything. Its cwd can be the user's real source folder (the
        // folder survey runs there), where acceptEdits would otherwise
        // auto-approve a prompt-injected write into the user's own files.
        denies.push("Edit".to_string());
        denies.push("Write".to_string());
        denies.push("NotebookEdit".to_string());
    }
    denies
}

pub(super) fn append_claude_output_schema(
    cmd_args: &mut Vec<String>,
    schema: Option<&serde_json::Value>,
) -> Result<(), String> {
    let Some(schema) = schema else {
        return Ok(());
    };
    let schema = crate::pipeline::structured::provider_schema_json(schema)?;
    cmd_args.push("--json-schema".to_string());
    cmd_args.push(schema);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn call_claude_inner(
    app: &crate::emit::EventBus,
    prompt: &str,
    allowed_tools: &[&str],
    system_prompt: Option<&str>,
    output_format: &str,
    timeout_secs: u64,
    label: &str,
    cwd: Option<&str>,
    extra_read_dirs: &[&str],
    overrides: &LlmOverrides<'_>,
    session_mode: Option<ClaudeSessionMode<'_>>,
) -> Result<String, String> {
    let mut cmd_args: Vec<String> = vec!["-p".to_string()];
    let mut tools: Vec<String> = allowed_tools
        .iter()
        // Direct APIs expose this as a first-class multimodal tool. Claude
        // Code's native Read tool already handles images and parallel reads,
        // so do not pass direct-API-only names to the CLI permission parser.
        .filter(|tool| {
            !matches!(
                **tool,
                "ReadDocumentAsset" | "ReadTextBatch" | "ReadDocumentAssetsBatch"
            )
        })
        .map(|tool| tool.to_string())
        .collect();
    // Read is always available — steps need it for paper/orientation files
    // and the long-prompt workaround writes to a temp file.
    if !tools.iter().any(|t| t == "Read") {
        tools.push("Read".to_string());
    }
    let prepared_prompt = prepare_cli_prompt(prompt)?;
    if let Some(path) = &prepared_prompt.path {
        log(
            app,
            format!("Wrote {} chars to temp file: {path}", prompt.len()),
        );
    }
    // A forked call must run from the warm-up workspace: the CLI resolves
    // `--resume` against a per-directory session store, so any other cwd
    // cannot see the warmed session and every unit would pay a full
    // self-contained fallback. The task's own cwd and write root are granted
    // as additional directories instead.
    let workspace = if let Some(ClaudeSessionMode::ResumeFork { workspace, .. }) = &session_mode {
        let mut fork_read_dirs: Vec<&str> = extra_read_dirs.to_vec();
        fork_read_dirs.extend(cwd);
        fork_read_dirs.extend(overrides.write_dir);
        plan_cli_workspace(
            Some(workspace),
            &fork_read_dirs,
            prepared_prompt.read_root.as_deref(),
            None,
        )?
    } else {
        plan_cli_workspace(
            cwd,
            extra_read_dirs,
            prepared_prompt.read_root.as_deref(),
            overrides.write_dir,
        )?
    };

    // When file writes are enabled, scope them: replace any bare Write/Edit
    // with an Edit rule confined to the artifact dir. An Edit(path) rule
    // governs the Write, Edit, and NotebookEdit tools together, and `//`
    // anchors an absolute path in Claude Code's gitignore-style permission
    // syntax (a single `/` would be project-root-relative). The rule is
    // derived from the write root itself, not the effective cwd — a forked
    // call's cwd is the session workspace, never its write root.
    if let Some(write_dir) = overrides.write_dir.and_then(normalize_cli_root) {
        tools.retain(|t| t != "Write" && t != "Edit");
        tools.push(format!("Edit({}/**)", absolute_rule_path(&write_dir)));
    }
    cmd_args.push(prepared_prompt.argument.clone());

    match session_mode {
        Some(ClaudeSessionMode::Start(id)) => {
            cmd_args.push("--session-id".to_string());
            cmd_args.push(id.to_string());
        }
        Some(ClaudeSessionMode::ResumeFork { id, .. }) => {
            cmd_args.push("--resume".to_string());
            cmd_args.push(id.to_string());
            cmd_args.push("--fork-session".to_string());
        }
        None => {}
    }

    // Always pass --allowedTools so Claude never gets default tools
    // (Edit, Write, Bash, etc.). When the list is empty, pass "none"
    // to explicitly disable all tools.
    cmd_args.push("--allowedTools".to_string());
    cmd_args.push(if tools.is_empty() {
        "none".to_string()
    } else {
        tools.join(",")
    });

    if let Some(sys) = system_prompt {
        cmd_args.push("--append-system-prompt".to_string());
        cmd_args.push(sys.to_string());
    }

    // These subprocesses run non-interactively (stdin=null, -p mode) and
    // only have read-only tools via --allowedTools. Auto-accept to avoid
    // permission prompts that would hang or fail without a TTY.
    cmd_args.push("--permission-mode".to_string());
    cmd_args.push("acceptEdits".to_string());
    // Do not load hooks, plugins, or MCP servers from the user's interactive
    // Claude configuration into an untrusted-document review call.
    cmd_args.push("--safe-mode".to_string());
    cmd_args.push("--disable-slash-commands".to_string());
    cmd_args.push("--no-chrome".to_string());

    // Grant Read access only to this run's explicit source/input roots and,
    // for a long prompt, that call's private prompt directory.
    for dir in &workspace.read_dirs {
        cmd_args.push("--add-dir".to_string());
        cmd_args.push(dir.clone());
    }

    let denies = cli_disallowed_tools(
        allowed_tools,
        &workspace.read_dirs,
        overrides.write_dir.is_some(),
    );
    if !denies.is_empty() {
        cmd_args.push("--disallowedTools".to_string());
        cmd_args.push(denies.join(","));
    }

    // Always request the JSON result envelope so we can read token usage from
    // it. We unwrap `.result` after the call, so callers still receive the raw
    // model text exactly as before. (No call site passes a non-text
    // output_format; the parameter is retained for API symmetry.)
    let _ = output_format;
    cmd_args.push("--output-format".to_string());
    cmd_args.push("json".to_string());
    append_claude_output_schema(&mut cmd_args, overrides.output_schema)?;

    // Apply Claude Code settings (model, effort) with optional per-step overrides.
    let model_src = overrides.model.unwrap_or("");
    let model = crate::settings::sanitize_cli_arg(model_src);
    if !model.is_empty() {
        cmd_args.push("--model".to_string());
        cmd_args.push(model);
    }
    let effort_src = overrides.effort.unwrap_or("");
    let effort = crate::settings::sanitize_cli_arg(effort_src);
    if !effort.is_empty() {
        cmd_args.push("--effort".to_string());
        cmd_args.push(effort);
    }

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
    verbose_log(app, format!("$ claude {display_args}"));

    // In write mode, run from the artifact dir: with acceptEdits, edits are
    // auto-approved in the cwd, and the scoped allow/deny rules above keep
    // everything else closed.
    let effective_cwd = workspace.cwd.as_deref();
    let mut cmd = build_provider_command("claude", effective_cwd, &cmd_args)?;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn claude: {e}. Is Claude Code installed?"))?;
    crate::pipeline::logging::record_provider_attempt();

    let (pid, start_time) = track_child_started(&child, app, label);

    // Stream stderr to the frontend, and retain the tail for failure
    // diagnostics regardless of the verbose setting.
    let sess = crate::pipeline::logging::current();
    let stderr_task = capture_stderr(child.stderr.take(), app.clone(), sess.clone());

    // Stream stdout to the frontend (Claude outputs result here)
    let stdout_task = capture_text_stdout(child.stdout.take(), app.clone(), sess);

    let wait_result = wait_for_child(
        &mut child,
        pid,
        timeout_secs,
        "Claude",
        "claude",
        label,
        app,
    )
    .await;

    let streams = finish_streams(stdout_task, stderr_task, pid, "Claude").await;
    let status = wait_result?;

    // Collect stdout and unwrap the JSON result envelope (see the
    // --output-format json note above). A successful process must produce the
    // envelope: accepting arbitrary stdout here would let CLI narration or
    // warnings become the report.
    let ((raw_stdout, stdout_overflowed), stderr_tail) = streams?;
    if stdout_overflowed {
        emit_stderr_tail(app, &stderr_tail);
        return Err(format!(
            "Claude stdout exceeded the {} MB safety limit",
            MAX_STDOUT_BYTES / 1024 / 1024
        ));
    }
    let exit_code = status.code().unwrap_or(-1);
    let parsed_result = parse_claude_result(&raw_stdout);
    let (text, claude_usage) = match parsed_result {
        Ok(result) => result,
        Err(error) if !status.success() => (
            error
                .strip_prefix("provider error: ")
                .unwrap_or_else(|| raw_stdout.trim())
                .to_string(),
            None,
        ),
        Err(error) if error.starts_with("provider error: ") => {
            emit_stderr_tail(app, &stderr_tail);
            let detail = error.trim_start_matches("provider error: ");
            let hint =
                claude_failure_hint(detail, &stderr_tail).unwrap_or_else(|| detail.to_string());
            let msg = format!("Claude call failed: {hint}");
            log(app, format!("ERROR: {msg}"));
            return Err(msg);
        }
        Err(error) => {
            emit_stderr_tail(app, &stderr_tail);
            let msg = format!("Claude returned an invalid JSON result envelope: {error}");
            log(app, format!("ERROR: {msg}"));
            return Err(msg);
        }
    };
    let elapsed = start_time.elapsed().as_secs();
    let token_info = match claude_usage {
        Some(usage) if usage.input_tokens > 0 || usage.output_tokens > 0 => format!(
            ", {}+{} tokens, {} cached/{} cache-write",
            usage.input_tokens,
            usage.output_tokens,
            usage.cached_input_tokens,
            usage.cache_write_input_tokens
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
    if let Some(usage) = claude_usage {
        crate::pipeline::logging::emit_usage(app, usage);
    }

    if !status.success() {
        if let Some(terminated) = classify_terminated_exit("Claude", &status, exit_code) {
            log(app, format!("{label}: {terminated}"));
            return Err(terminated);
        }
        emit_stderr_tail(app, &stderr_tail);
        let hint = claude_failure_hint(&text, &stderr_tail);
        let msg = if let Some(hint) = hint {
            format!("Claude call failed (exit {exit_code}): {hint}")
        } else {
            format!(
                "Claude call failed (exit {exit_code}). See this call's session log for details."
            )
        };
        log(app, format!("ERROR: {msg}"));
        return Err(msg);
    }

    if text.is_empty() {
        emit_stderr_tail(app, &stderr_tail);
        let msg = "Claude returned empty output. This is a known issue with large prompts.";
        log(app, format!("ERROR: {msg}"));
        return Err(msg.to_string());
    }

    Ok(text)
}

/// Convert an absolute directory path (forward slashes) into the absolute
/// form Claude Code's permission rules expect: `//` anchors the filesystem
/// root (a single `/` would mean project-root-relative). Windows drive
/// paths ("C:/Users/…") get the same `//` prefix, matching the CLI's
/// POSIX-style normalization of Windows paths.
pub(super) fn absolute_rule_path(dir: &str) -> String {
    let d = dir.trim_end_matches('/');
    if d.starts_with("//") {
        d.to_string()
    } else if let Some(stripped) = d.strip_prefix('/') {
        format!("//{stripped}")
    } else {
        format!("//{d}")
    }
}

/// Parse `claude -p --output-format json` output into the model's result text
/// and, when present, cache-aware token usage. `input_tokens` remains the
/// logical total and includes cache reads/creation. Successful CLI calls are
/// required to use this envelope so incidental stdout can never be promoted
/// to model output.
pub(super) fn parse_claude_result(
    raw: &str,
) -> Result<(String, Option<crate::pipeline::logging::CallUsage>), String> {
    let trimmed = raw.trim();
    let v = serde_json::from_str::<serde_json::Value>(trimmed)
        .map_err(|error| format!("invalid JSON: {error}"))?;
    if v.get("type").and_then(|t| t.as_str()) != Some("result") {
        return Err("missing `type: result`".to_string());
    }
    if v.get("is_error").and_then(|value| value.as_bool()) == Some(true) {
        let detail = claude_result_error_detail(&v)
            .unwrap_or_else(|| "Claude marked the result as an error".to_string());
        return Err(format!("provider error: {detail}"));
    }
    if let Some(subtype) = v.get("subtype").and_then(|value| value.as_str()) {
        if subtype != "success" {
            let detail = claude_result_error_detail(&v)
                .unwrap_or_else(|| format!("non-success result subtype `{subtype}`"));
            return Err(format!("provider error: {detail}"));
        }
    }
    let text = if let Some(structured) = v.get("structured_output").filter(|value| !value.is_null())
    {
        serde_json::to_string(structured)
            .map_err(|error| format!("invalid `structured_output`: {error}"))?
    } else {
        v.get("result")
            .and_then(|r| r.as_str())
            .ok_or("missing string `result`")?
            .trim()
            .to_string()
    };
    let model_round_trips = v
        .get("num_turns")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let mut tool_calls = crate::models::ToolCallCounts::default();
    if let Some(server_tools) = v
        .get("usage")
        .and_then(|usage| usage.get("server_tool_use"))
        .and_then(|value| value.as_object())
    {
        for (name, value) in server_tools {
            let count = value.as_u64().unwrap_or(0);
            tool_calls.add_kind(crate::pipeline::logging::classify_tool_name(name), count);
        }
    }
    let usage = v.get("usage").map(|u| {
        let field = |k: &str| u.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
        let cached = field("cache_read_input_tokens");
        let cache_write = field("cache_creation_input_tokens");
        crate::pipeline::logging::CallUsage {
            input_tokens: field("input_tokens")
                .saturating_add(cached)
                .saturating_add(cache_write),
            output_tokens: field("output_tokens"),
            cached_input_tokens: cached,
            cache_write_input_tokens: cache_write,
            model_round_trips,
            tool_calls,
            ..Default::default()
        }
    });
    let usage = usage.or_else(|| {
        (model_round_trips > 0 || !tool_calls.is_empty()).then_some(
            crate::pipeline::logging::CallUsage {
                model_round_trips,
                tool_calls,
                ..Default::default()
            },
        )
    });
    Ok((text, usage))
}

fn claude_result_error_detail(value: &serde_json::Value) -> Option<String> {
    value
        .get("result")
        .and_then(|item| item.as_str())
        .or_else(|| value.get("message").and_then(|item| item.as_str()))
        .or_else(|| {
            value.get("error").and_then(|error| {
                error
                    .as_str()
                    .or_else(|| error.get("message").and_then(|item| item.as_str()))
            })
        })
        .map(str::trim)
        .filter(|detail| !detail.is_empty())
        .map(str::to_string)
}

/// Classify an unsuccessful CLI exit. Returns the cancellation error when
/// Pipeline itself stopped the call (global cancel or this call's pass), and
/// a retryable signal-death error when the child was killed by something
/// else — the OOM killer, a crash, or an external kill. SIGTERM's
/// conventional 143 exit is grouped with signal deaths because Pipeline's
/// own kills also surface that way; the cancellation flags distinguish them.
pub fn classify_terminated_exit(
    provider: &str,
    status: &std::process::ExitStatus,
    exit_code: i32,
) -> Option<String> {
    let pass_cancelled = crate::pipeline::logging::current_pass()
        .is_some_and(|pass| crate::commands::is_pass_cancelled(&pass));
    if crate::commands::is_cancelled() || pass_cancelled {
        return Some("Pipeline cancelled".to_string());
    }
    if exit_code == 143 || status.code().is_none() {
        return Some(format!(
            "{provider} CLI terminated by a signal ({status}) that Pipeline did not send — \
             possibly killed by the system (out of memory) or crashed; retrying"
        ));
    }
    None
}

pub(super) const CLAUDE_LOGIN_HINT: &str =
    "Claude Code could not authenticate. Run `claude auth login` in Terminal, then try again.";

fn is_claude_auth_error(output: &str) -> bool {
    let lower = output.to_lowercase();
    [
        "not logged in",
        "not signed in",
        "not authenticated",
        "authentication required",
        "authentication failed",
        "please sign in",
        "please log in",
        "run /login",
        "401 unauthorized",
        "http 401",
        "status code 401",
    ]
    .iter()
    .any(|pattern| lower.contains(pattern))
}

/// Prefer one actionable authentication instruction over the CLI's raw
/// stdout/stderr diagnostics. Other failures retain the shared concise hint.
pub(super) fn claude_failure_hint(output: &str, stderr_tail: &[String]) -> Option<String> {
    if is_claude_auth_error(output) || stderr_tail.iter().any(|line| is_claude_auth_error(line)) {
        return Some(CLAUDE_LOGIN_HINT.to_string());
    }
    extract_error_hint(output).or_else(|| last_stderr_hint(stderr_tail))
}

/// Extract a provider-neutral user-facing error hint from CLI output.
pub fn extract_error_hint(output: &str) -> Option<String> {
    let lower = output.to_lowercase();
    if lower.contains("api key") {
        Some("API key not configured. Check your API key settings.".into())
    } else if lower.contains("rate limit") || lower.contains("too many requests") {
        Some("Rate limited. Wait a moment and try again.".into())
    } else if lower.contains("overloaded") || lower.contains("capacity") {
        Some("Service overloaded. Try again in a few minutes.".into())
    } else {
        // Return first non-empty line of output as a generic hint
        output
            .lines()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
    }
}
