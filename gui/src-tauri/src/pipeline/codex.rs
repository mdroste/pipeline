use std::process::Stdio;
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;
use tokio::io::{AsyncBufReadExt, BufReader};
use std::io::Write;
use tauri::{AppHandle, Emitter};

use super::claude::{build_silent_command, LlmOverrides};

/// Maximum characters to pass as a direct CLI argument.
/// Beyond this we write to a temp file and tell Codex to read it.
const MAX_DIRECT_PROMPT_LENGTH: usize = 4000;

fn log(app: &AppHandle, line: impl Into<String>) {
    app.emit("pipeline:log", serde_json::json!({ "line": line.into() })).ok();
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

    // Determine sandbox mode based on allowed tools
    let needs_write = allowed_tools.iter().any(|t| *t == "Write" || *t == "Edit");
    let sandbox = if needs_write { "workspace-write" } else { "read-only" };
    cmd_args.push("--sandbox".to_string());
    cmd_args.push(sandbox.to_string());

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

    let mut cmd = build_silent_command("codex", cwd);
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

    // Drain stderr silently (in --json mode it's just TUI noise)
    let stderr = child.stderr.take();
    let stderr_task = tokio::spawn(async move {
        if let Some(stderr) = stderr {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(_)) = reader.next_line().await {}
        }
    });

    // Parse JSONL stdout: collect agent_message text and usage info
    let stdout = child.stdout.take();
    let app_stdout = app.clone();
    let label_clone = label.to_string();
    let stdout_task = tokio::spawn(async move {
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
    });

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

    stderr_task.await.ok();

    let exit_code = status.code().unwrap_or(-1);
    let elapsed = start_time.elapsed().as_secs();
    let token_info = if input_tokens > 0 || output_tokens > 0 {
        format!(", {input_tokens}+{output_tokens} tokens")
    } else {
        String::new()
    };
    log(app, format!("{label} finished ({elapsed}s, exit code {exit_code}, {} chars output{token_info})", text.len()));

    if !status.success() {
        if crate::commands::is_cancelled() || exit_code == 143 || status.code().is_none() {
            log(app, format!("{label} cancelled"));
            return Err("Pipeline cancelled".into());
        }
        let hint = super::claude::extract_error_hint(&text);
        let msg = if let Some(hint) = hint {
            format!("Codex call failed (exit {exit_code}): {hint}")
        } else {
            format!("Codex call failed (exit {exit_code}). Check the console log for details.")
        };
        log(app, format!("ERROR: {msg}"));
        return Err(msg);
    }

    if text.is_empty() {
        let msg = "Codex returned empty output.";
        log(app, format!("ERROR: {msg}"));
        return Err(msg.to_string());
    }

    Ok(text)
}
