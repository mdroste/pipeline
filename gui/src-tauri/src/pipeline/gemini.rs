use std::io::Write;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;
use tokio::io::BufReader;

use super::claude::{
    build_provider_command, emit_stderr_tail, last_stderr_hint, plan_cli_workspace,
    prepare_cli_prompt, LlmOverrides, STDERR_TAIL_LINES,
};

fn log(app: &crate::emit::EventBus, line: impl Into<String>) {
    super::logging::emit(app, line.into());
}

fn verbose_log(app: &crate::emit::EventBus, line: impl Into<String>) {
    if crate::settings::load().verbose_logging {
        log(app, line);
    }
}

/// `--include-directories` expands Gemini's editable workspace as well as its
/// readable roots. In write mode, this policy permits edits only under the
/// absolute artifact directory and denies edits everywhere else. The prompt
/// supplies an absolute artifact path; relative edit attempts are denied and
/// can still use the executor's stdout fallback.
const GEMINI_TRAVERSAL_PATTERN: &str = r#""file_path":"[^"]*\.\."#;

fn gemini_allowed_write_pattern(write_dir: &str) -> String {
    let escaped_root = regex::escape(write_dir.trim_end_matches('/'));
    format!(r#""file_path":"{escaped_root}(?:/[^"]*)?""#)
}

fn gemini_write_policy(write_dir: &str) -> String {
    let allowed_pattern = gemini_allowed_write_pattern(write_dir);
    // Match any `..` in the serialized file_path value. This deliberately
    // rejects benign double-dot filenames too: report handoff paths never
    // need them, and the conservative rule covers both slash styles after
    // JSON serialization.
    let pattern_literal =
        serde_json::to_string(&allowed_pattern).expect("serializing a regex string cannot fail");
    let traversal_literal = serde_json::to_string(GEMINI_TRAVERSAL_PATTERN)
        .expect("serializing a regex string cannot fail");
    format!(
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
         interactive = false\n\n\
         [[rule]]\n\
         toolName = [\"write_file\", \"replace\"]\n\
         decision = \"deny\"\n\
         priority = 997\n\
         modes = [\"autoEdit\"]\n\
         interactive = false\n\
         denyMessage = \"Pipeline only permits writes inside this run's artifact directory.\"\n"
    )
}

fn append_gemini_read_dirs(cmd_args: &mut Vec<String>, read_dirs: &[String]) {
    for dir in read_dirs {
        cmd_args.push("--include-directories".to_string());
        cmd_args.push(dir.clone());
    }
}

fn create_gemini_write_policy(write_dir: &str) -> Result<NamedTempFile, String> {
    let mut policy = tempfile::Builder::new()
        .prefix("pipeline_gemini_")
        .suffix(".toml")
        .tempfile()
        .map_err(|e| format!("Failed to create Gemini policy file: {e}"))?;
    policy
        .write_all(gemini_write_policy(write_dir).as_bytes())
        .map_err(|e| format!("Failed to write Gemini policy file: {e}"))?;
    policy
        .flush()
        .map_err(|e| format!("Failed to flush Gemini policy file: {e}"))?;
    Ok(policy)
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
    let prepared_prompt = prepare_cli_prompt(prompt)?;
    if let Some(path) = &prepared_prompt.path {
        log(
            app,
            format!("Wrote {} chars to temp file: {path}", prompt.len()),
        );
    }

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
        prepared_prompt.read_root.as_deref(),
        if needs_write {
            overrides.write_dir
        } else {
            None
        },
    )?;
    cmd_args.push("--approval-mode".to_string());
    cmd_args.push(if needs_write { "auto_edit" } else { "plan" }.to_string());

    // Keep artifacts as cwd in write mode. Every source/input root remains
    // available through a repeated flag (rather than a comma-delimited value,
    // which would break valid directory names containing commas).
    append_gemini_read_dirs(&mut cmd_args, &workspace.read_dirs);

    let mut _write_policy: Option<NamedTempFile> = None;
    if needs_write {
        let write_root = workspace
            .cwd
            .as_deref()
            .ok_or("Gemini write mode requires an absolute artifact directory")?;
        let policy = create_gemini_write_policy(write_root)?;
        cmd_args.push("--policy".to_string());
        cmd_args.push(policy.path().to_string_lossy().replace('\\', "/"));
        _write_policy = Some(policy);
    }

    // Output as plain text
    cmd_args.push("-o".to_string());
    cmd_args.push("text".to_string());

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
    cmd_args.push("-p".to_string());
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
    verbose_log(app, format!("$ gemini {display_args}"));

    // The cwd defines gemini's writable workspace, so in write mode it must
    // be the artifact dir regardless of what the caller passed.
    let effective_cwd = workspace.cwd.as_deref();
    let mut cmd = build_provider_command("gemini", effective_cwd, &cmd_args)?;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn gemini: {e}. Is Gemini CLI installed?"))?;

    let pid = child.id().unwrap_or(0);
    if pid > 0 {
        crate::commands::register_child_pid(pid);
    }
    let start_time = Instant::now();
    log(app, format!("{label} started (PID {pid})"));

    // Stream stderr to the frontend, retaining the tail for diagnostics.
    let stderr = child.stderr.take();
    let app_stderr = app.clone();
    let sess = super::logging::current();
    let stderr_task = tokio::spawn(super::logging::with_session_opt(sess.clone(), async move {
        let mut tail: std::collections::VecDeque<String> = std::collections::VecDeque::new();
        if let Some(stderr) = stderr {
            let mut reader = BufReader::new(stderr);
            while let Ok(Some(record)) =
                super::logging::next_bounded_line(&mut reader, super::logging::MAX_CLI_LINE_BYTES)
                    .await
            {
                let line = if record.truncated {
                    format!("{}… [line truncated]", record.text)
                } else {
                    record.text
                };
                if !line.trim().is_empty() {
                    verbose_log(&app_stderr, format!("[stderr] {line}"));
                    if tail.len() >= STDERR_TAIL_LINES {
                        tail.pop_front();
                    }
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
            let mut reader = BufReader::new(stdout);
            while let Ok(Some(record)) =
                super::logging::next_bounded_line(&mut reader, super::logging::MAX_CLI_LINE_BYTES)
                    .await
            {
                let line = record.text;
                if record.truncated {
                    log(&app_stdout, "WARNING: provider emitted an oversized stdout record; record was truncated");
                }
                collected.push_str(&line);
                collected.push('\n');
                if collected.len() > super::claude::MAX_STDOUT_BYTES {
                    log(
                        &app_stdout,
                        format!(
                            "WARNING: stdout exceeded {} MB, truncating",
                            super::claude::MAX_STDOUT_BYTES / 1_000_000
                        ),
                    );
                    break;
                }
                if collected.lines().count() <= 5 {
                    verbose_log(&app_stdout, format!("[out] {line}"));
                }
            }
        }
        if collected.lines().count() > 5 {
            verbose_log(
                &app_stdout,
                format!("[out] ... ({} total lines)", collected.lines().count()),
            );
        }
        collected
    }));

    let status = tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait()).await;

    let status = match status {
        Ok(s) => s,
        Err(_) => {
            if pid > 0 { crate::commands::kill_process(pid); }
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
    if pid > 0 {
        crate::commands::unregister_child_pid(pid);
    }

    let text = stdout_task
        .await
        .map_err(|e| format!("stdout reader failed: {e}"))?
        .trim()
        .to_string();

    let stderr_tail = stderr_task.await.unwrap_or_default();

    let exit_code = status.code().unwrap_or(-1);
    let elapsed = start_time.elapsed().as_secs();
    log(
        app,
        format!(
            "{label} finished ({elapsed}s, exit code {exit_code}, {} chars output)",
            text.len()
        ),
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
            format!("Gemini call failed (exit {exit_code}): {hint}")
        } else {
            format!(
                "Gemini call failed (exit {exit_code}). See this call's session log for details."
            )
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

        let policy = gemini_write_policy("C:/Users/Mike/.pipeline/runs/r1/artifacts");
        let traversal_pos = policy.find("priority = 999").unwrap();
        let allow_pos = policy.find("priority = 998").unwrap();
        let fallback_deny_pos = policy.find("priority = 997").unwrap();
        assert!(traversal_pos < allow_pos && allow_pos < fallback_deny_pos);
        assert!(policy.contains("toolName = [\"write_file\", \"replace\"]"));
        assert!(policy.contains("argsPattern"));
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
}
