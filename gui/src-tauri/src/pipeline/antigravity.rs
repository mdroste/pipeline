use std::process::Stdio;

use super::claude::{
    build_provider_command, plan_cli_workspace, prepare_cli_prompt, CliWorkspacePlan, LlmOverrides,
};
use super::cli_process::{
    capture_stderr, capture_text_stdout, emit_stderr_tail, finish_streams, last_stderr_hint, log,
    track_child_started, verbose_log, wait_for_child,
};

/// Antigravity CLI (`agy`) wrapper — Google's successor to the retired Gemini
/// CLI. Contract pinned against agy 1.1.12 (deps enforces the minimum):
///
/// - `-p <prompt>` runs one non-interactive turn; `--output-format json`
///   prints exactly one stdout envelope `{conversation_id, status, response,
///   error, duration_seconds, num_turns, usage{input_tokens, output_tokens,
///   thinking_tokens, cache_read_tokens, total_tokens}}` (also on failure,
///   with exit 1). Human diagnostics go to stderr.
/// - `--disable-slash-commands` is mandatory: print mode otherwise expands
///   `/slash` commands and skills, and step prompts embed document text.
/// - `--print-timeout` defaults to 5m; it is raised past `timeout_secs` so
///   Pipeline's `wait_for_child` supervisor stays the authoritative timeout.
/// - Workspace: cwd is the writable root under `--sandbox`; every read root
///   is granted with a repeated `--add-dir`. Write mode uses
///   `--mode accept-edits`, read-only calls use `--mode plan`.
/// - Signed out, `-p` blocks ~60s on an interactive OAuth wait before failing
///   with an "authentication failed" envelope — deps preflight fails closed
///   on the `agy models` auth probe so runs never reach that stall.
/// - Long prompts go through a private temp file plus a Read instruction
///   (`prepare_cli_prompt`), which works independently of agy's stdin
///   handling; short prompts pass as the `-p` argument directly.
const ANTIGRAVITY_EFFORTS: &[&str] = &["low", "medium", "high"];

/// Margin added to `--print-timeout` so agy's own print deadline can only
/// fire after Pipeline's supervisor has already timed the call out.
const PRINT_TIMEOUT_MARGIN_SECS: u64 = 60;

fn append_antigravity_read_dirs(cmd_args: &mut Vec<String>, read_dirs: &[String]) {
    for dir in read_dirs {
        cmd_args.push("--add-dir".to_string());
        cmd_args.push(dir.clone());
    }
}

/// Build the complete agy argument list. Pure so tests can assert the exact
/// safety-relevant flag set without spawning a process.
fn build_antigravity_args(
    workspace: &CliWorkspacePlan,
    needs_write: bool,
    model: &str,
    effort: &str,
    timeout_secs: u64,
    prompt_argument: &str,
) -> Vec<String> {
    let mut cmd_args: Vec<String> = Vec::new();

    // The sandbox confines terminal use and file access to the workspace;
    // accept-edits auto-approves file edits there (non-interactive runs treat
    // unapproved actions as denied), while plan mode keeps reads available
    // and blocks edits for read-only steps.
    cmd_args.push("--sandbox".to_string());
    cmd_args.push("--mode".to_string());
    cmd_args.push(if needs_write { "accept-edits" } else { "plan" }.to_string());
    append_antigravity_read_dirs(&mut cmd_args, &workspace.read_dirs);

    cmd_args.push("--output-format".to_string());
    cmd_args.push("json".to_string());
    cmd_args.push("--disable-slash-commands".to_string());
    cmd_args.push("--print-timeout".to_string());
    cmd_args.push(format!(
        "{}s",
        timeout_secs.saturating_add(PRINT_TIMEOUT_MARGIN_SECS)
    ));

    if !model.is_empty() {
        cmd_args.push("--model".to_string());
        cmd_args.push(model.to_string());
    }
    // agy accepts exactly low|medium|high. The provider-agnostic step effort
    // chain can carry other providers' levels (for example Claude's "max");
    // those must not fail the call, so anything else is simply not sent.
    if ANTIGRAVITY_EFFORTS.contains(&effort) {
        cmd_args.push("--effort".to_string());
        cmd_args.push(effort.to_string());
    }

    cmd_args.push("-p".to_string());
    cmd_args.push(prompt_argument.to_string());
    cmd_args
}

#[derive(Debug, Default, serde::Deserialize)]
struct AntigravityUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    thinking_tokens: u64,
    #[serde(default)]
    cache_read_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
}

#[derive(Debug, Default, serde::Deserialize)]
struct AntigravityEnvelope {
    #[serde(default)]
    status: String,
    #[serde(default)]
    response: String,
    #[serde(default)]
    error: String,
    #[serde(default)]
    num_turns: u64,
    #[serde(default)]
    usage: Option<AntigravityUsage>,
}

fn sign_in_hint() -> String {
    "Antigravity CLI is not signed in. Run `agy` in a terminal to sign in, then retry.".to_string()
}

fn is_auth_error(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("authentication") || lower.contains("please sign in")
}

/// Parse agy's one-shot JSON envelope. The `response` field is the terminal
/// assistant response; everything else is telemetry or diagnostics and must
/// not be flattened into the report.
fn parse_antigravity_result(
    raw: &str,
) -> Result<(String, Option<crate::pipeline::logging::CallUsage>), String> {
    let envelope = serde_json::from_str::<AntigravityEnvelope>(raw.trim())
        .map_err(|error| format!("invalid JSON: {error}"))?;
    if !envelope.error.is_empty() {
        if is_auth_error(&envelope.error) {
            return Err(sign_in_hint());
        }
        return Err(envelope.error);
    }
    if envelope.status.eq_ignore_ascii_case("error") {
        return Err("Antigravity marked the result as an error".to_string());
    }
    let text = envelope.response.trim().to_string();
    if text.is_empty() {
        return Err("missing or empty `response`".to_string());
    }

    let usage = envelope.usage.and_then(|usage| {
        // Pipeline's convention treats cache-read tokens as a subset of
        // logical input. agy does not document whether `input_tokens` already
        // includes `cache_read_tokens`, so calibrate against its own
        // `total_tokens`: when the total only balances with cache reads added
        // separately, they are disjoint and logical input is their sum.
        let disjoint_total = usage
            .input_tokens
            .saturating_add(usage.output_tokens)
            .saturating_add(usage.thinking_tokens)
            .saturating_add(usage.cache_read_tokens);
        let input_tokens = if usage.total_tokens == disjoint_total && usage.cache_read_tokens > 0 {
            usage.input_tokens.saturating_add(usage.cache_read_tokens)
        } else {
            usage.input_tokens.max(usage.cache_read_tokens)
        };
        // Thinking tokens are billed and delivered as output.
        let output_tokens = usage.output_tokens.saturating_add(usage.thinking_tokens);
        (input_tokens > 0 || output_tokens > 0 || envelope.num_turns > 0).then_some(
            crate::pipeline::logging::CallUsage {
                input_tokens,
                output_tokens,
                cached_input_tokens: usage.cache_read_tokens,
                cache_write_input_tokens: 0,
                model_round_trips: envelope.num_turns,
                ..Default::default()
            },
        )
    });
    Ok((text, usage))
}

fn antigravity_error_hint(raw: &str) -> Option<String> {
    let envelope = serde_json::from_str::<AntigravityEnvelope>(raw.trim()).ok()?;
    if envelope.error.is_empty() {
        return None;
    }
    if is_auth_error(&envelope.error) {
        return Some(sign_in_hint());
    }
    Some(envelope.error)
}

/// Call `agy -p` and return the text output.
/// Streams stderr and stdout back to the frontend as `pipeline:log` events.
#[allow(clippy::too_many_arguments)]
pub async fn call_antigravity(
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
    let needs_write = allowed_tools.iter().any(|t| *t == "Write" || *t == "Edit")
        && overrides.write_dir.is_some();
    // Long prompts live in a private temp directory that becomes one of the
    // granted read roots; the argument then tells the model to read it.
    let prepared_prompt = prepare_cli_prompt(prompt)?;
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
    if needs_write && workspace.cwd.is_none() {
        return Err("Antigravity write mode requires an absolute artifact directory".to_string());
    }

    let model = crate::settings::sanitize_cli_arg(overrides.model.unwrap_or(""));
    let effort = crate::settings::sanitize_cli_arg(overrides.effort.unwrap_or(""));
    let cmd_args = build_antigravity_args(
        &workspace,
        needs_write,
        &model,
        &effort,
        timeout_secs,
        &prepared_prompt.argument,
    );

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
    verbose_log(app, format!("$ agy {display_args}"));

    // The cwd defines agy's writable workspace, so in write mode it must be
    // the artifact dir regardless of what the caller passed.
    let effective_cwd = workspace.cwd.as_deref();
    let mut cmd = build_provider_command("agy", effective_cwd, &cmd_args)?;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn agy: {e}. Is Antigravity CLI installed?"))?;
    super::logging::record_provider_attempt();

    let (pid, start_time) = track_child_started(&child, app, label);

    let sess = super::logging::current();
    let stderr_task = capture_stderr(child.stderr.take(), app.clone(), sess.clone());
    let stdout_task = capture_text_stdout(child.stdout.take(), app.clone(), sess);

    let wait_result = wait_for_child(
        &mut child,
        pid,
        timeout_secs,
        "Antigravity",
        "agy",
        label,
        app,
    )
    .await;
    let streams = finish_streams(stdout_task, stderr_task, pid, "Antigravity").await;
    let status = wait_result?;
    let ((raw_stdout, stdout_overflowed), stderr_tail) = streams?;
    if stdout_overflowed {
        emit_stderr_tail(app, &stderr_tail);
        return Err(format!(
            "Antigravity stdout exceeded the {} MB safety limit",
            super::claude::MAX_STDOUT_BYTES / 1024 / 1024
        ));
    }
    let exit_code = status.code().unwrap_or(-1);
    if !status.success() {
        if let Some(terminated) =
            super::claude::classify_terminated_exit("Antigravity", &status, exit_code)
        {
            log(app, format!("{label}: {terminated}"));
            return Err(terminated);
        }
        emit_stderr_tail(app, &stderr_tail);
        let auth_hint = stderr_tail
            .iter()
            .any(|line| is_auth_error(line))
            .then(sign_in_hint);
        let hint = auth_hint
            .or_else(|| antigravity_error_hint(&raw_stdout))
            .or_else(|| super::claude::extract_error_hint(&raw_stdout))
            .or_else(|| last_stderr_hint(&stderr_tail));
        let msg = if let Some(hint) = hint {
            format!("Antigravity call failed (exit {exit_code}): {hint}")
        } else {
            format!(
                "Antigravity call failed (exit {exit_code}). See this call's session log for details."
            )
        };
        log(app, format!("ERROR: {msg}"));
        return Err(msg);
    }

    let (text, antigravity_usage) = parse_antigravity_result(&raw_stdout).map_err(|error| {
        emit_stderr_tail(app, &stderr_tail);
        let msg = format!("Antigravity returned an invalid JSON result envelope: {error}");
        log(app, format!("ERROR: {msg}"));
        msg
    })?;
    let elapsed = start_time.elapsed().as_secs();
    let token_info = match &antigravity_usage {
        Some(usage) if usage.input_tokens > 0 || usage.output_tokens > 0 => format!(
            ", {}+{} tokens, {} cached",
            usage.input_tokens, usage.output_tokens, usage.cached_input_tokens
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
    if let Some(usage) = antigravity_usage {
        super::logging::emit_usage(app, usage);
    }

    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(read_dirs: &[&str], cwd: Option<&str>) -> CliWorkspacePlan {
        CliWorkspacePlan {
            cwd: cwd.map(str::to_string),
            read_dirs: read_dirs.iter().map(|dir| dir.to_string()).collect(),
        }
    }

    #[test]
    fn write_workspace_retains_source_run_inputs_and_prompt_root() {
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
        let mut args = Vec::new();
        append_antigravity_read_dirs(&mut args, &plan.read_dirs);
        assert_eq!(
            args,
            vec![
                "--add-dir",
                "/Users/Mike/Documents/Paper",
                "--add-dir",
                "/private/tmp/pipeline_prompt_1",
                "--add-dir",
                "/private/tmp/pipeline_run_inputs/paper-and-orientation",
            ]
        );
    }

    #[test]
    fn add_dir_flags_preserve_commas_and_spaces() {
        let roots = vec![
            "/Users/Mike/Paper, revised".to_string(),
            "C:/Users/Mike/Named Inputs".to_string(),
        ];
        let mut args = Vec::new();
        append_antigravity_read_dirs(&mut args, &roots);
        assert_eq!(args[1], "/Users/Mike/Paper, revised");
        assert_eq!(args[3], "C:/Users/Mike/Named Inputs");
    }

    #[test]
    fn mandatory_safety_flags_are_always_present() {
        for needs_write in [false, true] {
            let args = build_antigravity_args(
                &workspace(&["/tmp/inputs"], Some("/tmp/artifacts")),
                needs_write,
                "",
                "",
                900,
                "Review the paper.",
            );
            assert!(args.contains(&"--sandbox".to_string()));
            assert!(args.contains(&"--disable-slash-commands".to_string()));
            let format_at = args.iter().position(|a| a == "--output-format").unwrap();
            assert_eq!(args[format_at + 1], "json");
            // agy's own print deadline must sit beyond Pipeline's supervisor
            // timeout so wait_for_child stays the authoritative kill path.
            let timeout_at = args.iter().position(|a| a == "--print-timeout").unwrap();
            assert_eq!(args[timeout_at + 1], "960s");
        }
    }

    #[test]
    fn mode_follows_write_capability() {
        let read_only = build_antigravity_args(
            &workspace(&["/tmp/inputs"], Some("/tmp/inputs")),
            false,
            "",
            "",
            600,
            "p",
        );
        let mode_at = read_only.iter().position(|a| a == "--mode").unwrap();
        assert_eq!(read_only[mode_at + 1], "plan");

        let write = build_antigravity_args(
            &workspace(&["/tmp/inputs"], Some("/tmp/artifacts")),
            true,
            "",
            "",
            600,
            "p",
        );
        let mode_at = write.iter().position(|a| a == "--mode").unwrap();
        assert_eq!(write[mode_at + 1], "accept-edits");
    }

    #[test]
    fn model_and_effort_flags_are_conditional() {
        let args = build_antigravity_args(
            &workspace(&[], Some("/tmp")),
            false,
            "gemini-3.1-pro",
            "high",
            600,
            "p",
        );
        let model_at = args.iter().position(|a| a == "--model").unwrap();
        assert_eq!(args[model_at + 1], "gemini-3.1-pro");
        let effort_at = args.iter().position(|a| a == "--effort").unwrap();
        assert_eq!(args[effort_at + 1], "high");

        // Other providers' effort levels (for example Claude's "max") and the
        // empty default are dropped rather than failing the call.
        for effort in ["max", "", "xhigh"] {
            let args =
                build_antigravity_args(&workspace(&[], Some("/tmp")), false, "", effort, 600, "p");
            assert!(!args.contains(&"--effort".to_string()), "{effort:?}");
            assert!(!args.contains(&"--model".to_string()));
        }
    }

    #[test]
    fn prompt_argument_is_passed_to_print_flag_verbatim() {
        let args = build_antigravity_args(
            &workspace(&[], Some("/tmp")),
            false,
            "",
            "",
            600,
            "Read the instructions at /private/tmp/pipeline_prompt_1/prompt.txt and follow them exactly.",
        );
        let p_at = args.iter().position(|a| a == "-p").unwrap();
        assert!(args[p_at + 1].starts_with("Read the instructions at "));
        assert_eq!(p_at + 2, args.len());
    }

    #[test]
    fn json_envelope_returns_terminal_response_and_subset_usage() {
        // total_tokens balances only when cache reads are already inside
        // input_tokens, so logical input stays 120.
        let raw = r#"{
          "conversation_id":"c1",
          "status":"SUCCESS",
          "response":"  ## Final report\n\nClean.  ",
          "error":"",
          "duration_seconds":12.5,
          "num_turns":4,
          "usage":{"input_tokens":120,"output_tokens":30,"thinking_tokens":11,
                   "cache_read_tokens":10,"total_tokens":161}
        }"#;
        let (text, usage) = parse_antigravity_result(raw).unwrap();
        assert_eq!(text, "## Final report\n\nClean.");
        let usage = usage.unwrap();
        assert_eq!(usage.input_tokens, 120);
        assert_eq!(usage.output_tokens, 41);
        assert_eq!(usage.cached_input_tokens, 10);
        assert_eq!(usage.cache_write_input_tokens, 0);
        assert_eq!(usage.model_round_trips, 4);
    }

    #[test]
    fn json_envelope_calibrates_disjoint_cache_reads_into_logical_input() {
        // total_tokens balances only with cache reads added separately, so
        // they are disjoint from input_tokens and logical input is the sum.
        let raw = r#"{
          "status":"SUCCESS",
          "response":"Report body",
          "num_turns":1,
          "usage":{"input_tokens":110,"output_tokens":30,"thinking_tokens":11,
                   "cache_read_tokens":10,"total_tokens":161}
        }"#;
        let (_, usage) = parse_antigravity_result(raw).unwrap();
        let usage = usage.unwrap();
        assert_eq!(usage.input_tokens, 120);
        assert_eq!(usage.cached_input_tokens, 10);
    }

    #[test]
    fn unrecognized_success_status_literals_are_accepted() {
        // The success literal is not pinned by agy's docs; anything that is
        // not an explicit error with a non-empty response must pass.
        for status in ["SUCCESS", "OK", "completed", ""] {
            let raw = format!(r#"{{"status":"{status}","response":"Report body"}}"#);
            let (text, usage) = parse_antigravity_result(&raw).unwrap();
            assert_eq!(text, "Report body");
            assert!(usage.is_none());
        }
    }

    #[test]
    fn signed_out_error_envelope_is_rejected_with_sign_in_hint() {
        // Captured verbatim from agy 1.1.12 while signed out.
        let raw = r#"{"conversation_id":"","status":"ERROR","response":"","error":"authentication failed or timed out","duration_seconds":0,"num_turns":0,"usage":{"input_tokens":0,"output_tokens":0,"thinking_tokens":0,"cache_read_tokens":0,"total_tokens":0}}"#;
        let error = parse_antigravity_result(raw).unwrap_err();
        assert!(
            error.contains("Run `agy` in a terminal to sign in"),
            "{error}"
        );
        assert_eq!(antigravity_error_hint(raw).unwrap(), sign_in_hint());
    }

    #[test]
    fn malformed_or_error_envelopes_are_rejected() {
        assert!(parse_antigravity_result("narration before the report").is_err());
        assert!(parse_antigravity_result(r#"{"status":"ERROR","response":""}"#).is_err());
        assert!(
            parse_antigravity_result(r#"{"status":"SUCCESS","response":"","error":""}"#).is_err()
        );
        assert!(
            parse_antigravity_result(r#"{"error":"capacity exhausted"}"#)
                .unwrap_err()
                .contains("capacity exhausted")
        );
        assert!(parse_antigravity_result(r#"{"conversation_id":"c1"}"#).is_err());
    }
}
