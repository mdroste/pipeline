//! Legacy Review CLI adapter and subprocess lifecycle.

use std::process::Stdio;
use tokio::io::{AsyncWriteExt, BufReader};

use super::cli_process::{
    capture_stderr, emit_stderr_tail, finish_streams, last_stderr_hint, log, track_child_started,
    verbose_log, wait_for_child,
};
use crate::agent_runtime::process::build_provider_command;
use crate::pipeline::providers::LlmOverrides;

mod args;
mod events;
mod failure;
mod session;

use args::{codex_effective_cwd, codex_exec_args};
use events::{codex_completed_tool_kind, read_last_message_file, update_last_agent_message};
use failure::{codex_failure_detail, codex_failure_hint};
#[cfg(test)]
use session::validate_codex_fork;
use session::{fork_codex_thread, CodexForkSpec};

#[derive(Clone, Copy)]
enum CodexSessionMode<'a> {
    New,
    Resume(&'a str),
}

struct CodexInvocation {
    text: String,
    thread_id: Option<String>,
}

const SESSION_UNAVAILABLE_PREFIX: &str = "unavailable:";

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

    let model = overrides.model.unwrap_or("");
    let effort = overrides.effort.unwrap_or("");
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
            primer_overrides.output_schema = None;
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
                    if !crate::cancellation::is_pipeline_cancellation_error(&error) {
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
            if crate::cancellation::is_pipeline_cancellation_error(&error) {
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
    let prepared_output_schema = overrides
        .output_schema
        .map(crate::pipeline::structured::prepare_codex_schema_file)
        .transpose()?
        .flatten();
    let last_message_dir = tempfile::Builder::new()
        .prefix("pipeline_codex_final_")
        .tempdir()
        .map_err(|error| format!("Failed to create Codex final-message directory: {error}"))?;
    let last_message_path = last_message_dir.path().join("final.txt");
    let (cmd_args, needs_write) = codex_exec_args(
        allowed_tools,
        system_prompt,
        overrides,
        session_mode,
        prepared_output_schema
            .as_ref()
            .map(|schema| schema.path.as_str()),
        &last_message_path.to_string_lossy().replace('\\', "/"),
    )?;

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

    crate::pipeline::executor::budget_dispatch::reserve(None, true)?;
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn codex: {e}. Is Codex CLI installed?"))?;

    let (pid, start_time) = track_child_started(&child, app, label);
    let mut prompt_stdin = child
        .stdin
        .take()
        .ok_or("Codex process did not expose stdin")?;
    let prompt_bytes = prompt.as_bytes().to_vec();
    let stdin_task = crate::run_scope::spawn(async move {
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
    let stdout_task = crate::run_scope::spawn(super::logging::with_session_opt(sess, async move {
        let mut last_agent_text = String::new();
        let mut total_input_tokens: u64 = 0;
        let mut total_output_tokens: u64 = 0;
        let mut total_cached_input_tokens: u64 = 0;
        let mut model_round_trips: u64 = 0;
        let mut tool_calls = crate::models::ToolCallCounts::default();
        let mut thread_id: Option<String> = None;
        let mut stdout_failure: Option<String> = None;
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
                        // A failed turn usually repeats the error before it.
                        Some(t) if t.contains("error") || t == "turn.failed" => {
                            let detail =
                                codex_failure_detail(&event).unwrap_or_else(|| t.to_string());
                            if stdout_failure.as_deref() != Some(detail.as_str()) {
                                log(&app_stdout, format!("ERROR: [codex] {detail}"));
                            }
                            stdout_failure = Some(detail);
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
            stdout_failure,
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
            stdout_failure,
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
        let hint = codex_failure_hint(stdout_failure.as_deref(), &text, &stderr_tail);
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
mod tests;
