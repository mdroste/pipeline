use std::process::Stdio;
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;
use tokio::io::{AsyncBufReadExt, BufReader};
use std::io::Write;

use super::claude::{build_silent_command, emit_stderr_tail, last_stderr_hint, LlmOverrides, STDERR_TAIL_LINES};

/// Maximum characters to pass as a direct CLI argument.
/// Beyond this we write to a temp file and tell Gemini to read it.
const MAX_DIRECT_PROMPT_LENGTH: usize = 4000;

fn log(app: &crate::emit::EventBus, line: impl Into<String>) {
    super::logging::emit(app, line.into());
}

fn verbose_log(app: &crate::emit::EventBus, line: impl Into<String>) {
    if crate::settings::load().verbose_logging {
        log(app, line);
    }
}

/// Call `gemini -p` and return the text output.
/// Streams stderr and stdout back to the frontend as `pipeline:log` events.
pub async fn call_gemini(
    app: &crate::emit::EventBus,
    prompt: &str,
    allowed_tools: &[&str],
    _system_prompt: Option<&str>,
    _output_format: &str,
    timeout_secs: u64,
    label: &str,
    cwd: Option<&str>,
    overrides: &LlmOverrides<'_>,
) -> Result<String, String> {
    let mut cmd_args: Vec<String> = Vec::new();
    let mut _temp_file: Option<NamedTempFile> = None;

    // Gemini's file tools confine writes to the workspace (cwd) at the
    // tool layer, symlink-resolved — so write mode means: cwd = artifact
    // dir, approval mode auto_edit (auto-approves file edits; shell stays
    // ask_user, which non-interactive mode treats as deny).
    // Read-only mode keeps "plan": reads auto-approved, writes blocked.
    let needs_write = allowed_tools.iter().any(|t| *t == "Write" || *t == "Edit")
        && overrides.write_dir.is_some();
    cmd_args.push("--approval-mode".to_string());
    cmd_args.push(if needs_write { "auto_edit" } else { "plan" }.to_string());

    // Output as plain text
    cmd_args.push("-o".to_string());
    cmd_args.push("text".to_string());

    // Apply Gemini settings (model) with optional per-step override.
    // Gemini CLI doesn't expose an effort flag, so overrides.effort is ignored here.
    let settings = crate::settings::load();
    let model_src = overrides.model.unwrap_or(settings.gemini_model.as_str());
    let model = crate::settings::sanitize_cli_arg(model_src);
    if !model.is_empty() {
        cmd_args.push("-m".to_string());
        cmd_args.push(model);
    }

    // Build the prompt argument. Gemini uses -p "prompt" for non-interactive mode.
    // Handle large prompts by writing to a temp file.
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

        cmd_args.push("-p".to_string());
        cmd_args.push(format!(
            "Read the instructions at {path} and follow them exactly."
        ));

        _temp_file = Some(tmp);
    } else {
        cmd_args.push("-p".to_string());
        cmd_args.push(prompt.to_string());
    }

    // Log the command (truncated)
    let display_args: String = cmd_args.iter()
        .map(|a| if a.len() > 80 { format!("{}...", a.chars().take(80).collect::<String>()) } else { a.clone() })
        .collect::<Vec<_>>()
        .join(" ");
    verbose_log(app, format!("$ gemini {display_args}"));

    // The cwd defines gemini's writable workspace, so in write mode it must
    // be the artifact dir regardless of what the caller passed.
    let effective_cwd = if needs_write { overrides.write_dir.or(cwd) } else { cwd };
    let mut cmd = build_silent_command("gemini", effective_cwd);
    cmd.args(&cmd_args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to spawn gemini: {e}. Is Gemini CLI installed?"))?;

    let pid = child.id().unwrap_or(0);
    if pid > 0 { crate::commands::register_child_pid(pid); }
    let start_time = Instant::now();
    log(app, format!("{label} started (PID {pid})"));

    // Stream stderr to the frontend, retaining the tail for diagnostics.
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

    // Stream stdout to the frontend
    let stdout = child.stdout.take();
    let app_stdout = app.clone();
    let stdout_task = tokio::spawn(super::logging::with_session_opt(sess, async move {
        let mut collected = String::new();
        if let Some(stdout) = stdout {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                collected.push_str(&line);
                collected.push('\n');
                if collected.len() > super::claude::MAX_STDOUT_BYTES {
                    log(&app_stdout, format!(
                        "WARNING: stdout exceeded {} MB, truncating",
                        super::claude::MAX_STDOUT_BYTES / 1_000_000
                    ));
                    break;
                }
                if collected.lines().count() <= 5 {
                    verbose_log(&app_stdout, format!("[out] {line}"));
                }
            }
        }
        if collected.lines().count() > 5 {
            verbose_log(&app_stdout, format!("[out] ... ({} total lines)", collected.lines().count()));
        }
        collected
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
            log(app, format!("ERROR: Gemini call timed out after {timeout_secs}s (PID {pid}), killing process"));
            return Err(format!("Gemini call timed out after {timeout_secs}s"));
        }
    }
        .map_err(|e| {
            if pid > 0 { crate::commands::unregister_child_pid(pid); }
            format!("Failed waiting for gemini: {e}")
        })?;

    // Process has exited — unregister PID before joining I/O tasks
    // so cancel cleanup can't miss it if a join fails
    if pid > 0 { crate::commands::unregister_child_pid(pid); }

    let text = stdout_task.await
        .map_err(|e| format!("stdout reader failed: {e}"))?
        .trim()
        .to_string();

    let stderr_tail = stderr_task.await.unwrap_or_default();

    let exit_code = status.code().unwrap_or(-1);
    let elapsed = start_time.elapsed().as_secs();
    log(app, format!("{label} finished ({elapsed}s, exit code {exit_code}, {} chars output)", text.len()));

    if !status.success() {
        if crate::commands::is_cancelled() || exit_code == 143 || status.code().is_none() {
            log(app, format!("{label} cancelled"));
            return Err("Pipeline cancelled".into());
        }
        emit_stderr_tail(app, &stderr_tail);
        let hint = super::claude::extract_error_hint(&text).or_else(|| last_stderr_hint(&stderr_tail));
        let msg = if let Some(hint) = hint {
            format!("Gemini call failed (exit {exit_code}): {hint}")
        } else {
            format!("Gemini call failed (exit {exit_code}). See this call's session log for details.")
        };
        log(app, format!("ERROR: {msg}"));
        return Err(msg);
    }

    if text.is_empty() {
        emit_stderr_tail(app, &stderr_tail);
        let msg = "Gemini returned empty output.";
        log(app, format!("ERROR: {msg}"));
        return Err(msg.to_string());
    }

    Ok(text)
}
