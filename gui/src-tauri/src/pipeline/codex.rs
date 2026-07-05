use std::process::Stdio;
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;
use tokio::io::{AsyncBufReadExt, BufReader};
use std::io::Write;
use tauri::AppHandle;

use super::claude::{build_silent_command, emit_stderr_tail, last_stderr_hint, LlmOverrides, STDERR_TAIL_LINES};

/// Maximum characters to pass as a direct CLI argument.
/// Beyond this we write to a temp file and tell Codex to read it.
const MAX_DIRECT_PROMPT_LENGTH: usize = 4000;

fn log(app: &AppHandle, line: impl Into<String>) {
    super::logging::emit(app, line.into());
}

fn verbose_log(app: &AppHandle, line: impl Into<String>) {
    if crate::settings::load().verbose_logging {
        log(app, line);
    }
}

/// Call `codex exec` and return the text output.
/// Streams stderr and stdout back to the frontend as `pipeline:log` events.
pub async fn call_codex(
    app: &AppHandle,
    prompt: &str,
    allowed_tools: &[&str],
    system_prompt: Option<&str>,
    _output_format: &str,
    timeout_secs: u64,
    label: &str,
    cwd: Option<&str>,
    overrides: &LlmOverrides<'_>,
) -> Result<String, String> {
    let mut cmd_args: Vec<String> = vec!["exec".to_string()];
    let mut _temp_file: Option<NamedTempFile> = None;

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
    let sandbox = if needs_write { "workspace-write" } else { "read-only" };
    cmd_args.push("--sandbox".to_string());
    cmd_args.push(sandbox.to_string());

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
    let settings = crate::settings::load();
    let model_src = overrides.model.unwrap_or(settings.codex_model.as_str());
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

    // Handle large prompts: write to temp file and tell Codex to read it
    if prompt.len() > MAX_DIRECT_PROMPT_LENGTH {
        let mut tmp = NamedTempFile::with_prefix("pipeline_prompt_")
            .map_err(|e| format!("Failed to create temp file: {e}"))?;
        tmp.write_all(prompt.as_bytes())
            .map_err(|e| format!("Failed to write temp file: {e}"))?;
        tmp.flush()
            .map_err(|e| format!("Failed to flush temp file: {e}"))?;

        let path = tmp.path().to_string_lossy().replace('\\', "/");
        let bytes = prompt.len();
        log(app, format!("Wrote {bytes} chars to temp file: {path}"));

        cmd_args.push(format!(
            "Read the instructions at {path} and follow them exactly."
        ));

        _temp_file = Some(tmp);
    } else {
        cmd_args.push(prompt.to_string());
    }

    // Log the command (truncated)
    let display_args: String = cmd_args.iter()
        .map(|a| if a.len() > 80 { format!("{}...", a.chars().take(80).collect::<String>()) } else { a.clone() })
        .collect::<Vec<_>>()
        .join(" ");
    verbose_log(app, format!("$ codex {display_args}"));

    // The cwd defines codex's writable workspace, so in write mode it must
    // be the artifact dir regardless of what the caller passed.
    let effective_cwd = if needs_write { overrides.write_dir.or(cwd) } else { cwd };
    let mut cmd = build_silent_command("codex", effective_cwd);
    cmd.args(&cmd_args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to spawn codex: {e}. Is Codex CLI installed?"))?;

    let pid = child.id().unwrap_or(0);
    if pid > 0 { crate::commands::register_child_pid(pid); }
    let start_time = Instant::now();
    log(app, format!("{label} started (PID {pid})"));

    // Forward stderr to the verbose log. In --json mode it's mostly TUI
    // noise, but auth and capacity warnings land here too — dropping them
    // made those failures undiagnosable.
    let stderr = child.stderr.take();
    let app_stderr = app.clone();
    let sess = super::logging::current();
    let stderr_task = tokio::spawn(super::logging::with_session_opt(sess.clone(), async move {
        let mut tail: std::collections::VecDeque<String> = std::collections::VecDeque::new();
        if let Some(stderr) = stderr {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if !line.trim().is_empty() {
                    verbose_log(&app_stderr, format!("[stderr] {line}"));
                    if tail.len() >= STDERR_TAIL_LINES { tail.pop_front(); }
                    tail.push_back(line);
                }
            }
        }
        tail.into_iter().collect::<Vec<String>>()
    }));

    // Parse JSONL stdout: collect agent_message text and usage info
    let stdout = child.stdout.take();
    let app_stdout = app.clone();
    let label_clone = label.to_string();
    let stdout_task = tokio::spawn(super::logging::with_session_opt(sess, async move {
        let mut agent_text = String::new();
        let mut total_input_tokens: u64 = 0;
        let mut total_output_tokens: u64 = 0;
        if let Some(stdout) = stdout {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if line.trim().is_empty() { continue; }
                if let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) {
                    match event.get("type").and_then(|t| t.as_str()) {
                        Some("item.completed") => {
                            if let Some(item) = event.get("item") {
                                if item.get("type").and_then(|t| t.as_str()) == Some("agent_message") {
                                    if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                                        agent_text.push_str(text);
                                    }
                                }
                            }
                        }
                        Some("turn.completed") => {
                            if let Some(usage) = event.get("usage") {
                                total_input_tokens += usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                                total_output_tokens += usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                            }
                        }
                        // Codex reports fatal problems (bad config, auth, model)
                        // as an error event on stdout — surface it, always.
                        Some(t) if t.contains("error") => {
                            let detail = event.get("message").and_then(|v| v.as_str())
                                .or_else(|| event.get("error").and_then(|e| e.as_str()))
                                .or_else(|| event.get("error").and_then(|e| e.get("message")).and_then(|v| v.as_str()))
                                .unwrap_or(t);
                            log(&app_stdout, format!("ERROR: [codex] {detail}"));
                        }
                        _ => {}
                    }
                } else {
                    verbose_log(&app_stdout, format!("[codex] {line}"));
                }
                if agent_text.len() > super::claude::MAX_STDOUT_BYTES {
                    log(&app_stdout, format!(
                        "WARNING: stdout exceeded {} MB, truncating",
                        super::claude::MAX_STDOUT_BYTES / 1_000_000
                    ));
                    break;
                }
            }
        }
        if total_input_tokens > 0 || total_output_tokens > 0 {
            verbose_log(&app_stdout, format!(
                "[codex] {}: tokens in={total_input_tokens} out={total_output_tokens}",
                label_clone
            ));
        }
        (agent_text, total_input_tokens, total_output_tokens)
    }));

    let status = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        child.wait(),
    )
    .await;

    let status = match status {
        Ok(s) => s,
        Err(_) => {
            let _ = child.kill().await;
            let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
            if pid > 0 { crate::commands::unregister_child_pid(pid); }
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            if crate::commands::is_cancelled() {
                log(app, format!("{label} cancelled"));
                return Err("Pipeline cancelled".into());
            }
            log(app, format!("ERROR: Codex call timed out after {timeout_secs}s (PID {pid}), killing process"));
            return Err(format!("Codex call timed out after {timeout_secs}s"));
        }
    }
        .map_err(|e| {
            if pid > 0 { crate::commands::unregister_child_pid(pid); }
            format!("Failed waiting for codex: {e}")
        })?;

    // Process has exited — unregister PID before joining I/O tasks
    // so cancel cleanup can't miss it if a join fails
    if pid > 0 { crate::commands::unregister_child_pid(pid); }

    let (agent_text, input_tokens, output_tokens) = stdout_task.await
        .map_err(|e| format!("stdout reader failed: {e}"))?;
    let text = agent_text.trim().to_string();

    let stderr_tail = stderr_task.await.unwrap_or_default();

    let exit_code = status.code().unwrap_or(-1);
    let elapsed = start_time.elapsed().as_secs();
    let token_info = if input_tokens > 0 || output_tokens > 0 {
        format!(", {input_tokens}+{output_tokens} tokens")
    } else {
        String::new()
    };
    log(app, format!("{label} finished ({elapsed}s, exit code {exit_code}, {} chars output{token_info})", text.len()));
    super::logging::emit_usage(app, input_tokens, output_tokens);

    if !status.success() {
        if crate::commands::is_cancelled() || exit_code == 143 || status.code().is_none() {
            log(app, format!("{label} cancelled"));
            return Err("Pipeline cancelled".into());
        }
        emit_stderr_tail(app, &stderr_tail);
        let hint = super::claude::extract_error_hint(&text).or_else(|| last_stderr_hint(&stderr_tail));
        let msg = if let Some(hint) = hint {
            format!("Codex call failed (exit {exit_code}): {hint}")
        } else {
            format!("Codex call failed (exit {exit_code}). See this call's session log for details.")
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

    Ok(text)
}
