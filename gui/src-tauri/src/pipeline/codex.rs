use std::process::Stdio;
use tokio::io::BufReader;

use super::claude::{build_provider_command, normalize_cli_root, prepare_cli_prompt, LlmOverrides};
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

    let settings = overrides
        .settings
        .cloned()
        .unwrap_or_else(crate::settings::load);
    let model = overrides.model.unwrap_or(settings.codex_model.as_str());
    let effort = overrides.effort.unwrap_or(settings.codex_effort.as_str());
    let tools_key = allowed_tools.join(",");
    let session_key = context.compatibility_key(
        "codex-cli",
        [
            model,
            effort,
            system_prompt.unwrap_or(""),
            tools_key.as_str(),
            cwd.unwrap_or(""),
            overrides.write_dir.unwrap_or(""),
        ],
    );
    let slot = context.slot(session_key).await;

    let base_result = {
        let mut base = slot.lock().await;
        if let Some(id) = base.as_ref() {
            Ok(id.clone())
        } else {
            let primer = format!(
                "{}\n\nReply with exactly: Context prepared.",
                context.content()
            );
            let mut primer_overrides = overrides.clone();
            primer_overrides.shared_context = None;
            let primer_label = format!("{label} · cache warm-up");
            call_codex_inner(
                app,
                &primer,
                allowed_tools,
                system_prompt,
                timeout_secs,
                &primer_label,
                cwd,
                &primer_overrides,
                CodexSessionMode::New,
            )
            .await
            .and_then(|result| {
                let id = result
                    .thread_id
                    .ok_or_else(|| "Codex did not report a reusable thread id".to_string())?;
                *base = Some(id.clone());
                Ok(id)
            })
        }
    };

    let branch_result = match base_result {
        Ok(base_id) => fork_codex_thread(&base_id).await.map(|id| (base_id, id)),
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

async fn fork_codex_thread(thread_id: &str) -> Result<String, String> {
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
                }
            }
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "thread/fork",
            "params": { "threadId": thread_id }
        }),
    ];
    let replies = crate::model_catalog::rpc_exchange(
        "codex",
        &["app-server".into(), "--listen".into(), "stdio://".into()],
        &requests,
    )
    .await?;
    replies
        .get(1)
        .and_then(|reply| reply.get("result"))
        .and_then(|result| result.get("thread"))
        .and_then(|thread| thread.get("id"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "Codex thread/fork returned no child thread id".to_string())
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
    let prepared_prompt = prepare_cli_prompt(prompt)?;

    // Use JSON mode for clean machine-readable output
    cmd_args.push("--json".to_string());

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

    // Codex's sandbox constrains writes but permits reads, so it needs no
    // equivalent of Claude/Gemini's read-root flags. The private prompt
    // directory nevertheless prevents unrelated temp files sharing a root.
    if let Some(path) = &prepared_prompt.path {
        log(
            app,
            format!("Wrote {} chars to temp file: {path}", prompt.len()),
        );
    }
    if let CodexSessionMode::Resume(id) = session_mode {
        cmd_args.push(id.to_string());
    }
    cmd_args.push(prepared_prompt.argument.clone());

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
    let effective_cwd = codex_effective_cwd(
        needs_write,
        cwd,
        overrides.write_dir,
        prepared_prompt.read_root.as_deref(),
    )?;
    let mut cmd = build_provider_command("codex", effective_cwd.as_deref(), &cmd_args)?;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn codex: {e}. Is Codex CLI installed?"))?;

    let (pid, start_time) = track_child_started(&child, app, label);

    // Forward stderr to the verbose log. In --json mode it's mostly TUI
    // noise, but auth and capacity warnings land here too — dropping them
    // made those failures undiagnosable.
    let sess = super::logging::current();
    let stderr_task = capture_stderr(child.stderr.take(), app.clone(), sess.clone());

    // Parse JSONL stdout: collect agent_message text and usage info
    let stdout = child.stdout.take();
    let app_stdout = app.clone();
    let label_clone = label.to_string();
    let stdout_task = tokio::spawn(super::logging::with_session_opt(sess, async move {
        let mut agent_text = String::new();
        let mut total_input_tokens: u64 = 0;
        let mut total_output_tokens: u64 = 0;
        let mut total_cached_input_tokens: u64 = 0;
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
                            if let Some(item) = event.get("item") {
                                if item.get("type").and_then(|t| t.as_str())
                                    == Some("agent_message")
                                {
                                    if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                                        if text.len()
                                            > super::claude::MAX_STDOUT_BYTES
                                                .saturating_sub(agent_text.len())
                                        {
                                            log(
                                                &app_stdout,
                                                format!(
                                                    "ERROR: stdout exceeded {} MB",
                                                    super::claude::MAX_STDOUT_BYTES / 1_000_000
                                                ),
                                            );
                                            overflowed = true;
                                        } else {
                                            agent_text.push_str(text);
                                        }
                                    }
                                }
                            }
                        }
                        Some("turn.completed") => {
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
            agent_text,
            total_input_tokens,
            total_output_tokens,
            total_cached_input_tokens,
            thread_id,
            overflowed,
        )
    }));

    let wait_result =
        wait_for_child(&mut child, pid, timeout_secs, "Codex", "codex", label, app).await;
    let streams = finish_streams(stdout_task, stderr_task, pid, "Codex").await;
    let status = wait_result?;
    let (
        (
            agent_text,
            input_tokens,
            output_tokens,
            cached_input_tokens,
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
    let text = agent_text.trim().to_string();

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
        },
    );

    if !status.success() {
        if crate::commands::is_cancelled() || exit_code == 143 || status.code().is_none() {
            log(app, format!("{label} cancelled"));
            return Err("Pipeline cancelled".into());
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
}
