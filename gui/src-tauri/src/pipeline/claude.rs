use std::process::Stdio;
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use std::io::Write;
use tauri::AppHandle;

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
    /// The run's artifact directory (absolute, forward slashes). When set,
    /// the call is allowed to write files — confined to this directory by
    /// each provider's sandbox mechanism — and callers should also set the
    /// subprocess cwd to this directory. `None` = read-only call.
    pub write_dir: Option<&'a str>,
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
    super::logging::emit(app, line.into());
}

/// Log only when verbose_logging is enabled in settings.
fn verbose_log(app: &AppHandle, line: impl Into<String>) {
    if crate::settings::load().verbose_logging {
        log(app, line);
    }
}

/// Number of trailing stderr lines to retain per call for failure diagnostics.
pub const STDERR_TAIL_LINES: usize = 50;

/// The last non-empty captured stderr line, used as a hint in the error message
/// when stdout carried nothing useful.
pub fn last_stderr_hint(tail: &[String]) -> Option<String> {
    tail.iter()
        .rev()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
}

/// Surface captured stderr on failure, always (not gated on verbose_logging).
/// This is where CLIs report auth/config errors, and it is the only clue when
/// stdout is empty — dropping it made those failures undiagnosable.
pub fn emit_stderr_tail(app: &AppHandle, tail: &[String]) {
    if tail.is_empty() {
        return;
    }
    log(app, format!("ERROR: captured stderr from failed call ({} line(s)):", tail.len()));
    for l in tail {
        log(app, format!("[stderr] {l}"));
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
    // When file writes are enabled, scope them: replace any bare Write/Edit
    // with an Edit rule confined to the artifact dir. An Edit(path) rule
    // governs the Write, Edit, and NotebookEdit tools together, and `//`
    // anchors an absolute path in Claude Code's gitignore-style permission
    // syntax (a single `/` would be project-root-relative).
    if let Some(wd) = overrides.write_dir {
        tools.retain(|t| t != "Write" && t != "Edit");
        tools.push(format!("Edit({}/**)", absolute_rule_path(wd)));
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

    // Writes must not leak into the read directories: acceptEdits
    // auto-approves file edits in the cwd and --add-dir directories, so
    // when writing is enabled, explicitly deny edits there. Deny rules
    // outrank both allow rules and the permission mode.
    if overrides.write_dir.is_some() && !add_dirs.is_empty() {
        let denies: Vec<String> = add_dirs
            .iter()
            .map(|d| format!("Edit({}/**)", absolute_rule_path(d)))
            .collect();
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

    // In write mode, run from the artifact dir: with acceptEdits, edits are
    // auto-approved in the cwd, and the scoped allow/deny rules above keep
    // everything else closed.
    let effective_cwd = overrides.write_dir.or(cwd);
    let mut cmd = build_silent_command("claude", effective_cwd);
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

    // Stream stderr to the frontend, and retain the tail for failure
    // diagnostics regardless of the verbose setting.
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

    // Stream stdout to the frontend (Claude outputs result here)
    let stdout = child.stdout.take();
    let app_stdout = app.clone();
    let stdout_task = tokio::spawn(super::logging::with_session_opt(sess, async move {
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
    }));

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

    // Collect stdout and unwrap the JSON result envelope (see the
    // --output-format json note above). Falls back to the raw output when it
    // isn't the expected envelope, so callers/behavior are unchanged.
    let raw_stdout = stdout_task.await
        .map_err(|e| format!("stdout reader failed: {e}"))?;
    let (text, claude_usage) = parse_claude_result(&raw_stdout);

    // Let stderr finish and keep its tail for failure diagnostics.
    let stderr_tail = stderr_task.await.unwrap_or_default();

    let exit_code = status.code().unwrap_or(-1);
    let elapsed = start_time.elapsed().as_secs();
    let token_info = match claude_usage {
        Some((i, o)) if i > 0 || o > 0 => format!(", {i}+{o} tokens"),
        _ => String::new(),
    };
    log(app, format!("{label} finished ({elapsed}s, exit code {exit_code}, {} chars output{token_info})", text.len()));
    if let Some((i, o)) = claude_usage {
        super::logging::emit_usage(app, i, o);
    }

    if !status.success() {
        if crate::commands::is_cancelled() || exit_code == 143 || status.code().is_none() {
            log(app, format!("{label} cancelled"));
            return Err("Pipeline cancelled".into());
        }
        emit_stderr_tail(app, &stderr_tail);
        let hint = extract_error_hint(&text).or_else(|| last_stderr_hint(&stderr_tail));
        let msg = if let Some(hint) = hint {
            format!("Claude call failed (exit {exit_code}): {hint}")
        } else {
            format!("Claude call failed (exit {exit_code}). See this call's session log for details.")
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
fn absolute_rule_path(dir: &str) -> String {
    let d = dir.trim_end_matches('/');
    if let Some(stripped) = d.strip_prefix('/') {
        format!("//{stripped}")
    } else {
        format!("//{d}")
    }
}

/// Parse `claude -p --output-format json` output into the model's result text
/// and, when present, `(input_tokens, output_tokens)`. Input tokens include
/// cache-read/creation so the count reflects total tokens consumed. Falls back
/// to the trimmed raw output with no usage when the input isn't the expected
/// JSON envelope (older CLI, error text, or a caller-forced format).
fn parse_claude_result(raw: &str) -> (String, Option<(u64, u64)>) {
    let trimmed = raw.trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        // Only claude's own result envelope carries `"type":"result"`. Gating
        // on it means a model's *own* top-level JSON output (e.g. the
        // orientation map) is never mistaken for the envelope and unwrapped.
        if v.get("type").and_then(|t| t.as_str()) == Some("result") {
            let text = v
                .get("result")
                .and_then(|r| r.as_str())
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            let usage = v.get("usage").map(|u| {
                let field = |k: &str| u.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
                let input = field("input_tokens")
                    + field("cache_read_input_tokens")
                    + field("cache_creation_input_tokens");
                (input, field("output_tokens"))
            });
            return (text, usage);
        }
    }
    (trimmed.to_string(), None)
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

/// Dispatch an LLM call to the configured provider (Claude, Codex, Gemini, or
/// a local OpenAI-compatible server).
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
    // Tag every log line this call produces with a unique session id so the
    // frontend can separate concurrently running headless invocations.
    let session_id = super::logging::next_session_id();
    super::logging::with_session(session_id, label, async move {
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
        // Local OpenAI-compatible server (Ollama, LM Studio, llama.cpp, vLLM).
        // Always direct HTTP — there is no CLI fallback for this provider.
        "local" => {
            return super::api_openai::call_local_api(
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
    })
    .await
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

#[cfg(test)]
mod tests {
    use super::parse_claude_result;

    #[test]
    fn unwraps_result_envelope_and_sums_input_tokens() {
        let raw = r#"{"type":"result","subtype":"success","result":"hello world",
            "usage":{"input_tokens":100,"output_tokens":20,"cache_read_input_tokens":5,"cache_creation_input_tokens":3}}"#;
        let (text, usage) = parse_claude_result(raw);
        assert_eq!(text, "hello world");
        assert_eq!(usage, Some((108, 20)));
    }

    #[test]
    fn envelope_without_usage_yields_result_and_no_tokens() {
        let (text, usage) = parse_claude_result(r#"{"type":"result","result":"ok"}"#);
        assert_eq!(text, "ok");
        assert_eq!(usage, None);
    }

    #[test]
    fn plain_text_falls_back_unchanged() {
        let (text, usage) = parse_claude_result("  just some plain text  ");
        assert_eq!(text, "just some plain text");
        assert_eq!(usage, None);
    }

    #[test]
    fn model_json_output_is_not_mistaken_for_envelope() {
        // A model that returns its own JSON (e.g. the orientation map) has no
        // "type":"result" marker, so it passes through verbatim.
        let model = r#"{"metadata":{"title":"X"},"result":"not an envelope"}"#;
        let (text, usage) = parse_claude_result(model);
        assert_eq!(text, model);
        assert_eq!(usage, None);
    }
}
