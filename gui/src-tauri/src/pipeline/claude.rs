use std::process::Stdio;
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use std::io::Write;
use tauri::{AppHandle, Emitter};

/// Maximum characters to pass as a direct CLI argument.
/// Beyond this we write to a temp file and tell Claude to read it.
const MAX_DIRECT_PROMPT_LENGTH: usize = 4000;

/// Per-call model/effort overrides. When fields are `Some(non-empty)`, they
/// take precedence over the corresponding global settings for this single call.
/// Empty strings are treated as "no override" so callers can pass
/// step.model.as_str() directly.
#[derive(Default, Clone, Debug)]
pub struct LlmOverrides<'a> {
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    /// PDF to attach to the request on direct-API paths. CLI paths ignore
    /// this — there the prompt references the file path and the CLI's Read
    /// tool handles PDFs natively.
    pub pdf_attachment: Option<&'a std::path::Path>,
    /// Max output tokens on direct-API paths. The default (16384) suits
    /// analysis steps; full-document transcription needs more. CLI paths
    /// ignore this.
    pub max_output_tokens: Option<u32>,
}

impl<'a> LlmOverrides<'a> {
    pub fn from_step_strings(model: &'a str, effort: &'a str) -> Self {
        Self {
            model: if model.trim().is_empty() { None } else { Some(model) },
            effort: if effort.trim().is_empty() { None } else { Some(effort) },
            ..Default::default()
        }
    }
}

/// Safety cap on collected subprocess stdout (50 MB).
/// LLM outputs are bounded by token limits (~500 KB typical), so this
/// only guards against pathological cases (e.g. broken binary on PATH).
pub const MAX_STDOUT_BYTES: usize = 50_000_000;

fn log(app: &AppHandle, line: impl Into<String>) {
    app.emit("pipeline:log", serde_json::json!({ "line": line.into() })).ok();
}

/// Log only when verbose_logging is enabled in settings.
fn verbose_log(app: &AppHandle, line: impl Into<String>) {
    if crate::settings::load().verbose_logging {
        log(app, line);
    }
}

/// Call `claude -p` and return the text output.
/// Streams stderr and stdout back to the frontend as `pipeline:log` events.
///
/// `extra_read_dirs` are passed through as `--add-dir` flags so the Read
/// tool can reach paths outside the cwd.  The system temp dir is always
/// added because we routinely write prompts and orientation maps there.
/// Paths are normalized to forward slashes so Windows backslashes don't
/// confuse Claude's internal path normalization.
pub async fn call_claude(
    app: &AppHandle,
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
    let mut cmd_args: Vec<String> = vec!["-p".to_string()];
    let mut tools: Vec<String> = allowed_tools.iter().map(|s| s.to_string()).collect();
    // Read is always available — steps need it for paper/orientation files
    // and the long-prompt workaround writes to a temp file.
    if !tools.iter().any(|t| t == "Read") {
        tools.push("Read".to_string());
    }
    let mut _temp_file: Option<NamedTempFile> = None;

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

    // Always pass --allowedTools so Claude never gets default tools
    // (Edit, Write, Bash, etc.). When the list is empty, pass "none"
    // to explicitly disable all tools.
    cmd_args.push("--allowedTools".to_string());
    cmd_args.push(if tools.is_empty() { "none".to_string() } else { tools.join(",") });

    if let Some(sys) = system_prompt {
        cmd_args.push("--append-system-prompt".to_string());
        cmd_args.push(sys.to_string());
    }

    // These subprocesses run non-interactively (stdin=null, -p mode) and
    // only have read-only tools via --allowedTools. Auto-accept to avoid
    // permission prompts that would hang or fail without a TTY.
    cmd_args.push("--permission-mode".to_string());
    cmd_args.push("acceptEdits".to_string());

    // Grant Read access to the system temp dir (where temp prompt files,
    // extracted paper text, and orientation maps live) plus any caller-
    // provided directories (typically the paper's parent dir).  Forward
    // slashes only — Claude's internal path normalization mishandles raw
    // Windows backslash paths.
    let mut add_dirs: Vec<String> = Vec::with_capacity(extra_read_dirs.len() + 1);
    add_dirs.push(std::env::temp_dir().to_string_lossy().replace('\\', "/"));
    for dir in extra_read_dirs {
        add_dirs.push(dir.replace('\\', "/"));
    }
    add_dirs.sort();
    add_dirs.dedup();
    for dir in &add_dirs {
        cmd_args.push("--add-dir".to_string());
        cmd_args.push(dir.clone());
    }

    if output_format != "text" {
        cmd_args.push("--output-format".to_string());
        cmd_args.push(output_format.to_string());
    }

    // Apply Claude Code settings (model, effort) with optional per-step overrides.
    let settings = crate::settings::load();
    let model_src = overrides.model.unwrap_or(settings.claude_model.as_str());
    let model = crate::settings::sanitize_cli_arg(model_src);
    if !model.is_empty() {
        cmd_args.push("--model".to_string());
        cmd_args.push(model);
    }
    let effort_src = overrides.effort.unwrap_or(settings.claude_effort.as_str());
    let effort = crate::settings::sanitize_cli_arg(effort_src);
    if !effort.is_empty() {
        cmd_args.push("--effort".to_string());
        cmd_args.push(effort);
    }

    // Log the command (truncated)
    let display_args: String = cmd_args.iter()
        .map(|a| if a.len() > 80 { format!("{}...", a.chars().take(80).collect::<String>()) } else { a.clone() })
        .collect::<Vec<_>>()
        .join(" ");
    verbose_log(app, format!("$ claude {display_args}"));

    let mut cmd = build_silent_command("claude", cwd);
    cmd.args(&cmd_args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to spawn claude: {e}. Is Claude Code installed?"))?;

    let pid = child.id().unwrap_or(0);
    if pid > 0 {
        crate::commands::register_child_pid(pid);
    }
    let start_time = Instant::now();
    log(app, format!("{label} started (PID {pid})"));

    // Stream stderr to the frontend
    let stderr = child.stderr.take();
    let app_stderr = app.clone();
    let stderr_task = tokio::spawn(async move {
        if let Some(stderr) = stderr {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if !line.trim().is_empty() {
                    verbose_log(&app_stderr, format!("[stderr] {line}"));
                }
            }
        }
    });

    // Stream stdout to the frontend (Claude outputs result here)
    let stdout = child.stdout.take();
    let app_stdout = app.clone();
    let stdout_task = tokio::spawn(async move {
        let mut collected = String::new();
        if let Some(stdout) = stdout {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                collected.push_str(&line);
                collected.push('\n');
                if collected.len() > MAX_STDOUT_BYTES {
                    log(&app_stdout, format!(
                        "WARNING: stdout exceeded {} MB, truncating",
                        MAX_STDOUT_BYTES / 1_000_000
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
    });

    // Wait for the process to exit
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
            // Drain the reader tasks now that their pipes are closed, so they don't
            // linger after we return.
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            if crate::commands::is_cancelled() {
                log(app, format!("{label} cancelled"));
                return Err("Pipeline cancelled".into());
            }
            log(app, format!("ERROR: Claude call timed out after {timeout_secs}s (PID {pid}), killing process"));
            return Err(format!("Claude call timed out after {timeout_secs}s"));
        }
    }
        .map_err(|e| {
            if pid > 0 { crate::commands::unregister_child_pid(pid); }
            format!("Failed waiting for claude: {e}")
        })?;

    // Process has exited — unregister PID before joining I/O tasks
    // so cancel cleanup can't miss it if a join fails
    if pid > 0 { crate::commands::unregister_child_pid(pid); }

    // Collect stdout
    let text = stdout_task.await
        .map_err(|e| format!("stdout reader failed: {e}"))?
        .trim()
        .to_string();

    // Let stderr finish
    stderr_task.await.ok();

    let exit_code = status.code().unwrap_or(-1);
    let elapsed = start_time.elapsed().as_secs();
    log(app, format!("{label} finished ({elapsed}s, exit code {exit_code}, {} chars output)", text.len()));

    if !status.success() {
        if crate::commands::is_cancelled() || exit_code == 143 || status.code().is_none() {
            log(app, format!("{label} cancelled"));
            return Err("Pipeline cancelled".into());
        }
        let hint = extract_error_hint(&text);
        let msg = if let Some(hint) = hint {
            format!("Claude call failed (exit {exit_code}): {hint}")
        } else {
            format!("Claude call failed (exit {exit_code}). Check the console log for details.")
        };
        log(app, format!("ERROR: {msg}"));
        return Err(msg);
    }

    if text.is_empty() {
        let msg = "Claude returned empty output. This is a known issue with large prompts.";
        log(app, format!("ERROR: {msg}"));
        return Err(msg.to_string());
    }

    Ok(text)
}

/// Extract a user-facing error hint from CLI output.
/// Looks for common auth/config error patterns in stdout/stderr.
pub fn extract_error_hint(output: &str) -> Option<String> {
    let lower = output.to_lowercase();
    if lower.contains("not logged in") || lower.contains("not authenticated") || lower.contains("sign in") || lower.contains("log in") || lower.contains("auth") {
        Some("Not signed in. Run `claude auth login` to authenticate.".into())
    } else if lower.contains("api key") {
        Some("API key not configured. Check your API key settings.".into())
    } else if lower.contains("rate limit") || lower.contains("too many requests") {
        Some("Rate limited. Wait a moment and try again.".into())
    } else if lower.contains("overloaded") || lower.contains("capacity") {
        Some("Service overloaded. Try again in a few minutes.".into())
    } else {
        // Return first non-empty line of output as a generic hint
        output.lines()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
    }
}

/// Dispatch an LLM call to the configured provider (Claude, Codex, or Gemini).
/// All pipeline code should call this instead of provider-specific functions directly.
///
/// `cwd`: optional working directory for the subprocess. Pass the paper's source
/// directory when the LLM needs to read figures or other assets alongside the paper.
/// When `None`, the subprocess runs in the system temp dir.
pub async fn call_llm(
    app: &AppHandle,
    prompt: &str,
    allowed_tools: &[&str],
    system_prompt: Option<&str>,
    output_format: &str,
    timeout_secs: u64,
    label: &str,
    provider_override: Option<&str>,
    cwd: Option<&str>,
    extra_read_dirs: &[&str],
    overrides: &LlmOverrides<'_>,
) -> Result<String, String> {
    let settings = crate::settings::load();
    let provider = provider_override.unwrap_or(&settings.preferred_provider);

    // Direct API path: bypass CLI subprocess when an API key is configured
    match provider {
        "claude" | "" if !settings.anthropic_api_key.is_empty() => {
            return super::api_anthropic::call_anthropic_api(
                app, prompt, allowed_tools, system_prompt,
                timeout_secs, label, &settings, overrides,
            ).await;
        }
        "codex" if !settings.openai_api_key.is_empty() => {
            return super::api_openai::call_openai_api(
                app, prompt, allowed_tools, system_prompt,
                timeout_secs, label, &settings, overrides,
            ).await;
        }
        "gemini" if !settings.google_api_key.is_empty() => {
            return super::api_google::call_google_api(
                app, prompt, allowed_tools, system_prompt,
                timeout_secs, label, &settings, overrides,
            ).await;
        }
        _ => {}
    }

    // Subprocess fallback. extra_read_dirs is currently consumed only by
    // call_claude — codex and gemini sandbox via --sandbox / their own
    // mechanisms and don't accept --add-dir.
    match provider {
        "codex" => {
            super::codex::call_codex(app, prompt, allowed_tools, system_prompt, output_format, timeout_secs, label, cwd, overrides).await
        }
        "gemini" => {
            super::gemini::call_gemini(app, prompt, allowed_tools, system_prompt, output_format, timeout_secs, label, cwd, overrides).await
        }
        _ => {
            call_claude(app, prompt, allowed_tools, system_prompt, output_format, timeout_secs, label, cwd, extra_read_dirs, overrides).await
        }
    }
}

/// Build a tokio Command with the full user PATH and platform-specific flags.
///
/// `cwd` sets the subprocess working directory. When `None`, defaults to the
/// system temp dir to avoid macOS TCC permission prompts for protected folders
/// (~/Music, ~/Photos, etc.) that occur when the app inherits a broad CWD.
/// Pass the paper's source directory when the subprocess needs to read figures
/// or other assets alongside the paper.
pub fn build_silent_command(program: &str, cwd: Option<&str>) -> Command {
    #[allow(unused_mut)]
    let mut std_cmd = std::process::Command::new(program);
    std_cmd.env("PATH", crate::env::full_path());
    match cwd {
        Some(dir) => { std_cmd.current_dir(dir); }
        None => { std_cmd.current_dir(std::env::temp_dir()); }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    Command::from(std_cmd)
}
