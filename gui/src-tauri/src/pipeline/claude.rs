use std::io::Write;
use std::process::Stdio;
use std::time::Duration;
use tempfile::TempDir;
use tokio::process::Command;

pub use super::cli_process::MAX_STDOUT_BYTES;
use super::cli_process::{
    capture_stderr, capture_text_stdout, emit_stderr_tail, finish_streams, last_stderr_hint, log,
    track_child_started, verbose_log, wait_for_child,
};

/// Maximum characters to pass as a direct CLI argument.
/// Beyond this we write to a temp file and tell Claude to read it.
pub(crate) const MAX_DIRECT_PROMPT_LENGTH: usize = 4000;
const MAX_LIVE_ARTIFACT_FILES: usize = 1_000;
const MAX_LIVE_ARTIFACT_BYTES: u64 = 300 * 1024 * 1024;
const MAX_LIVE_ARTIFACT_FILE_BYTES: u64 = 50 * 1024 * 1024;

fn check_live_artifact_quota(root: &std::path::Path) -> Result<(), String> {
    let mut stack = vec![root.to_path_buf()];
    let mut files = 0usize;
    let mut bytes = 0u64;
    let mut walk = crate::safety::WalkBudget::new_cancellable("Artifact directory scan");
    while let Some(directory) = stack.pop() {
        let entries = std::fs::read_dir(&directory)
            .map_err(|e| format!("Cannot inspect artifact directory: {e}"))?;
        for entry in entries.flatten() {
            walk.entry()?;
            let file_type = entry
                .file_type()
                .map_err(|e| format!("Cannot inspect artifact entry: {e}"))?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                walk.directory()?;
                stack.push(entry.path());
                continue;
            }
            if !file_type.is_file() {
                return Err("Artifact directory contains a non-regular file".to_string());
            }
            let length = entry
                .metadata()
                .map_err(|e| format!("Cannot inspect artifact size: {e}"))?
                .len();
            files += 1;
            bytes = bytes.saturating_add(length);
            if files > MAX_LIVE_ARTIFACT_FILES
                || length > MAX_LIVE_ARTIFACT_FILE_BYTES
                || bytes > MAX_LIVE_ARTIFACT_BYTES
            {
                return Err(format!(
                    "Artifact safety quota exceeded ({files} files, {} MB)",
                    bytes / 1024 / 1024
                ));
            }
        }
    }
    Ok(())
}

async fn supervise_artifact_writes<F>(
    future: F,
    write_dir: &str,
    pass_key: Option<String>,
) -> Result<String, String>
where
    F: std::future::Future<Output = Result<String, String>>,
{
    let mut operation = std::pin::pin!(future);
    let directory = std::path::PathBuf::from(write_dir);
    loop {
        if let Ok(result) =
            tokio::time::timeout(Duration::from_millis(250), operation.as_mut()).await
        {
            // A provider can finish between monitor ticks. Always scan once
            // after completion so a last-millisecond oversized or special file
            // cannot bypass the live quota.
            let path = directory.clone();
            let final_check = tokio::task::spawn_blocking(move || check_live_artifact_quota(&path))
                .await
                .map_err(|e| format!("Artifact monitor failed: {e}"))?;
            final_check?;
            return result;
        }
        let path = directory.clone();
        let check = tokio::task::spawn_blocking(move || check_live_artifact_quota(&path))
            .await
            .map_err(|e| format!("Artifact monitor failed: {e}"))?;
        if let Err(error) = check {
            if let Some(key) = &pass_key {
                let _ = crate::commands::cancel_pass(key.clone()).await;
            } else {
                crate::commands::kill_all_children();
            }
            return Err(error);
        }
    }
}

/// A CLI-safe prompt argument. Long prompts live in a private temporary
/// directory rather than directly under the process-wide temp directory, so
/// providers can grant access to only this call's prompt file.
pub(crate) struct PreparedCliPrompt {
    pub argument: String,
    pub path: Option<String>,
    pub read_root: Option<String>,
    _temp_dir: Option<TempDir>,
}

pub(crate) fn prepare_cli_prompt(prompt: &str) -> Result<PreparedCliPrompt, String> {
    if prompt.len() <= MAX_DIRECT_PROMPT_LENGTH {
        return Ok(PreparedCliPrompt {
            argument: prompt.to_string(),
            path: None,
            read_root: None,
            _temp_dir: None,
        });
    }

    let temp_dir = tempfile::Builder::new()
        .prefix("pipeline_prompt_")
        .tempdir()
        .map_err(|e| format!("Failed to create prompt temp directory: {e}"))?;
    let prompt_path = temp_dir.path().join("prompt.txt");
    let mut file = std::fs::File::create(&prompt_path)
        .map_err(|e| format!("Failed to create prompt temp file: {e}"))?;
    file.write_all(prompt.as_bytes())
        .map_err(|e| format!("Failed to write prompt temp file: {e}"))?;
    file.flush()
        .map_err(|e| format!("Failed to flush prompt temp file: {e}"))?;

    let path = normalize_cli_root(&prompt_path.to_string_lossy())
        .ok_or_else(|| "Failed to resolve prompt temp file".to_string())?;
    let read_root = normalize_cli_root(&temp_dir.path().to_string_lossy())
        .ok_or_else(|| "Failed to resolve prompt temp directory".to_string())?;
    Ok(PreparedCliPrompt {
        argument: format!("Read the instructions at {path} and follow them exactly."),
        path: Some(path),
        read_root: Some(read_root),
        _temp_dir: Some(temp_dir),
    })
}

/// Normalize a path for CLI arguments. Existing paths are canonicalized
/// (which resolves macOS `/var` aliases and Windows extended-path prefixes);
/// absolute paths with another platform's syntax are normalized lexically so
/// request construction can be tested deterministically on every host.
pub(crate) fn normalize_cli_root(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }

    let slash_path = trimmed.replace('\\', "/");
    for candidate in [
        std::path::Path::new(trimmed),
        std::path::Path::new(&slash_path),
    ] {
        if let Ok(canonical) = candidate.canonicalize() {
            return Some(normalize_canonical_cli_path(&canonical.to_string_lossy()));
        }
    }

    if is_cli_absolute(&slash_path) {
        Some(normalize_absolute_lexically(&slash_path))
    } else {
        None
    }
}

/// Platform-independent parent extraction for absolute prompt paths. Rust's
/// host `Path` parser does not recognize Windows drive paths when tests run on
/// macOS/Linux, so normalize separators before finding the parent.
pub(crate) fn cli_parent_dir(path: &str) -> Option<String> {
    let normalized = path.trim().replace('\\', "/");
    let (parent, _) = normalized.rsplit_once('/')?;
    let parent = if parent.is_empty() { "/" } else { parent };
    normalize_cli_root(parent)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliWorkspacePlan {
    pub cwd: Option<String>,
    pub read_dirs: Vec<String>,
}

/// Select the provider cwd and the additional read-only roots. The artifact
/// directory wins as cwd in write mode. Roots already covered by cwd, or by a
/// less-specific root in the same list, are omitted.
pub(crate) fn plan_cli_workspace(
    cwd: Option<&str>,
    extra_read_dirs: &[&str],
    transient_read_root: Option<&str>,
    write_dir: Option<&str>,
) -> Result<CliWorkspacePlan, String> {
    let mut effective_cwd = write_dir
        .or(cwd)
        .or(transient_read_root)
        .map(|path| {
            normalize_cli_root(path)
                .ok_or_else(|| format!("Provider working directory must be absolute: {path}"))
        })
        .transpose()?;
    let mut roots: Vec<String> = extra_read_dirs
        .iter()
        .copied()
        .chain(transient_read_root)
        .filter(|path| !path.trim().is_empty())
        .map(|path| {
            normalize_cli_root(path)
                .ok_or_else(|| format!("Provider read directory must be absolute: {path}"))
        })
        .collect::<Result<_, _>>()?;
    // When the caller has no distinct cwd, use one of its explicit readable
    // roots instead of falling back to the process-wide temp directory.
    if effective_cwd.is_none() {
        effective_cwd = roots.first().cloned();
    }
    roots.retain(|root| {
        effective_cwd
            .as_deref()
            .is_none_or(|base| !same_or_descendant(root, base))
    });

    roots.sort_by(|a, b| {
        path_depth(a)
            .cmp(&path_depth(b))
            .then_with(|| path_sort_key(a).cmp(&path_sort_key(b)))
    });
    let mut minimal: Vec<String> = Vec::new();
    for root in roots {
        if !minimal.iter().any(|base| same_or_descendant(&root, base)) {
            minimal.push(root);
        }
    }
    minimal.sort_by_key(|p| path_sort_key(p));

    Ok(CliWorkspacePlan {
        cwd: effective_cwd,
        read_dirs: minimal,
    })
}

fn normalize_canonical_cli_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if let Some(rest) = normalized.strip_prefix("//?/UNC/") {
        format!("//{rest}")
    } else if let Some(rest) = normalized.strip_prefix("//?/") {
        rest.to_string()
    } else {
        normalized
    }
}

fn is_cli_absolute(path: &str) -> bool {
    path.starts_with('/')
        || (path.as_bytes().get(1) == Some(&b':')
            && path.as_bytes().get(2) == Some(&b'/')
            && path.as_bytes()[0].is_ascii_alphabetic())
}

fn normalize_absolute_lexically(path: &str) -> String {
    let (prefix, rest) = if path.starts_with("//") {
        ("//".to_string(), path.trim_start_matches('/'))
    } else if path.as_bytes().get(1) == Some(&b':') {
        (path[..2].to_string(), path[2..].trim_start_matches('/'))
    } else {
        ("/".to_string(), path.trim_start_matches('/'))
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    match prefix.as_str() {
        "/" => format!("/{}", parts.join("/")),
        "//" => format!("//{}", parts.join("/")),
        drive => format!("{drive}/{}", parts.join("/")),
    }
}

fn path_sort_key(path: &str) -> String {
    if path.as_bytes().get(1) == Some(&b':') || path.starts_with("//") {
        path.to_ascii_lowercase()
    } else {
        path.to_string()
    }
}

fn path_depth(path: &str) -> usize {
    path.split('/').filter(|part| !part.is_empty()).count()
}

fn same_or_descendant(path: &str, root: &str) -> bool {
    let path = path_sort_key(path.trim_end_matches('/'));
    let root = path_sort_key(root.trim_end_matches('/'));
    path == root
        || path
            .strip_prefix(&root)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Per-call model/effort overrides. When fields are `Some(non-empty)`, they
/// take precedence over the corresponding global settings for this single call.
/// Empty strings are treated as "no override" so callers can pass
/// step.model.as_str() directly.
#[derive(Default, Clone, Debug)]
pub struct LlmOverrides<'a> {
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    /// The caller already resolved the structured model policy. When true,
    /// `model: None` intentionally means CLI Automatic and must not fall back
    /// to a legacy settings string.
    pub model_resolved: bool,
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
    /// Immutable settings snapshot captured at run start. When absent (for
    /// standalone helper calls), settings are loaded normally.
    pub settings: Option<&'a crate::settings::Settings>,
    /// Run-local paper/orientation prefix. Direct APIs place it before the
    /// task-specific prompt; supported CLIs fork a warmed base session.
    pub shared_context: Option<std::sync::Arc<crate::pipeline::context_cache::PreparedContext>>,
}

impl<'a> LlmOverrides<'a> {
    pub fn from_step_strings(model: &'a str, effort: &'a str) -> Self {
        Self {
            model: if model.trim().is_empty() {
                None
            } else {
                Some(model)
            },
            effort: if effort.trim().is_empty() {
                None
            } else {
                Some(effort)
            },
            ..Default::default()
        }
    }
}

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
    ResumeFork(&'a str),
}

fn is_cli_session_capability_error(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    [
        "unknown option",
        "unexpected argument",
        "fork-session",
        "session not found",
        "cannot resume",
        "failed to resume",
    ]
    .iter()
    .any(|needle| error.contains(needle))
}

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

    let settings = overrides
        .settings
        .cloned()
        .unwrap_or_else(crate::settings::load);
    let model = overrides.model.unwrap_or(settings.claude_model.as_str());
    let effort = overrides.effort.unwrap_or(settings.claude_effort.as_str());
    let tools_key = allowed_tools.join(",");
    let write_key = overrides.write_dir.unwrap_or("");
    let session_key = context.compatibility_key(
        "claude-cli",
        [
            model,
            effort,
            system_prompt.unwrap_or(""),
            tools_key.as_str(),
            cwd.unwrap_or(""),
            write_key,
        ],
    );
    let slot = context.slot(session_key).await;

    let base_result = {
        let mut base = slot.lock().await;
        if let Some(id) = base.as_ref() {
            Ok(id.clone())
        } else {
            match super::context_cache::new_session_id() {
                Err(error) => Err(error),
                Ok(id) => {
                    let primer = format!(
                        "{}\n\nReply with exactly: Context prepared.",
                        context.content()
                    );
                    let mut primer_overrides = overrides.clone();
                    primer_overrides.shared_context = None;
                    let primer_label = format!("{label} · cache warm-up");
                    call_claude_inner(
                        app,
                        &primer,
                        &[],
                        system_prompt,
                        "text",
                        timeout_secs,
                        &primer_label,
                        cwd,
                        extra_read_dirs,
                        &primer_overrides,
                        Some(ClaudeSessionMode::Start(&id)),
                    )
                    .await
                    .map(|_| {
                        *base = Some(id.clone());
                        id
                    })
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
                Some(ClaudeSessionMode::ResumeFork(&base_id)),
            )
            .await;
            if let Err(error) = &result {
                if is_cli_session_capability_error(error) {
                    log(
                        app,
                        format!(
                            "WARNING: {label}: installed Claude CLI cannot fork the warmed session ({error}); using a self-contained call"
                        ),
                    );
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
    let mut tools: Vec<String> = allowed_tools.iter().map(|s| s.to_string()).collect();
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
    let workspace = plan_cli_workspace(
        cwd,
        extra_read_dirs,
        prepared_prompt.read_root.as_deref(),
        overrides.write_dir,
    )?;

    // When file writes are enabled, scope them: replace any bare Write/Edit
    // with an Edit rule confined to the artifact dir. An Edit(path) rule
    // governs the Write, Edit, and NotebookEdit tools together, and `//`
    // anchors an absolute path in Claude Code's gitignore-style permission
    // syntax (a single `/` would be project-root-relative).
    if let Some(wd) = workspace
        .cwd
        .as_deref()
        .filter(|_| overrides.write_dir.is_some())
    {
        tools.retain(|t| t != "Write" && t != "Edit");
        tools.push(format!("Edit({}/**)", absolute_rule_path(wd)));
    }
    cmd_args.push(prepared_prompt.argument.clone());

    match session_mode {
        Some(ClaudeSessionMode::Start(id)) => {
            cmd_args.push("--session-id".to_string());
            cmd_args.push(id.to_string());
        }
        Some(ClaudeSessionMode::ResumeFork(id)) => {
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

    // Grant Read access only to this run's explicit source/input roots and,
    // for a long prompt, that call's private prompt directory.
    for dir in &workspace.read_dirs {
        cmd_args.push("--add-dir".to_string());
        cmd_args.push(dir.clone());
    }

    // Writes must not leak into the read directories: acceptEdits
    // auto-approves file edits in the cwd and --add-dir directories, so
    // when writing is enabled, explicitly deny edits there. Deny rules
    // outrank both allow rules and the permission mode.
    if overrides.write_dir.is_some() && !workspace.read_dirs.is_empty() {
        let denies: Vec<String> = workspace
            .read_dirs
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
    let settings = overrides
        .settings
        .cloned()
        .unwrap_or_else(crate::settings::load);
    let model_src = if overrides.model_resolved {
        overrides.model.unwrap_or("")
    } else {
        overrides.model.unwrap_or(settings.claude_model.as_str())
    };
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
    super::logging::record_provider_attempt();

    let (pid, start_time) = track_child_started(&child, app, label);

    // Stream stderr to the frontend, and retain the tail for failure
    // diagnostics regardless of the verbose setting.
    let sess = super::logging::current();
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
    // --output-format json note above). Falls back to the raw output when it
    // isn't the expected envelope, so callers/behavior are unchanged.
    let ((raw_stdout, stdout_overflowed), stderr_tail) = streams?;
    if stdout_overflowed {
        emit_stderr_tail(app, &stderr_tail);
        return Err(format!(
            "Claude stdout exceeded the {} MB safety limit",
            MAX_STDOUT_BYTES / 1024 / 1024
        ));
    }
    let (text, claude_usage) = parse_claude_result(&raw_stdout);

    let exit_code = status.code().unwrap_or(-1);
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
        super::logging::emit_usage(app, usage);
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
fn absolute_rule_path(dir: &str) -> String {
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
/// logical total and includes cache reads/creation. Falls back
/// to the trimmed raw output with no usage when the input isn't the expected
/// JSON envelope (older CLI, error text, or a caller-forced format).
fn parse_claude_result(raw: &str) -> (String, Option<crate::pipeline::logging::CallUsage>) {
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
                let cached = field("cache_read_input_tokens");
                let cache_write = field("cache_creation_input_tokens");
                crate::pipeline::logging::CallUsage {
                    input_tokens: field("input_tokens")
                        .saturating_add(cached)
                        .saturating_add(cache_write),
                    output_tokens: field("output_tokens"),
                    cached_input_tokens: cached,
                    cache_write_input_tokens: cache_write,
                    ..Default::default()
                }
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
    if lower.contains("not logged in")
        || lower.contains("not authenticated")
        || lower.contains("sign in")
        || lower.contains("log in")
        || lower.contains("auth")
    {
        Some("Not signed in. Run `claude auth login` to authenticate.".into())
    } else if lower.contains("api key") {
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

/// Dispatch an LLM call to the configured provider (Claude, Codex, Gemini, or
/// a local OpenAI-compatible server).
/// All pipeline code should call this instead of provider-specific functions directly.
///
/// `cwd`: optional working directory for the subprocess. Pass the paper's source
/// directory when the LLM needs to read figures or other assets alongside the paper.
/// When `None`, the subprocess runs in the system temp dir.
#[allow(clippy::too_many_arguments)]
pub async fn call_llm(
    app: &crate::emit::EventBus,
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
        let settings = overrides
            .settings
            .cloned()
            .unwrap_or_else(crate::settings::load);
        let provider = provider_override.unwrap_or(&settings.preferred_provider);

        // Resolve a durable policy at the dispatch boundary. API Automatic
        // becomes a concrete available ID; CLI Automatic intentionally omits
        // the model flag so an older installed CLI keeps using its own default.
        let requested = overrides
            .model
            .map(crate::settings::ModelSelection::from_legacy);
        let resolution = if overrides.model_resolved {
            None
        } else {
            Some(
                crate::commands::await_or_cancel(
                    crate::model_catalog::resolve(provider, &settings, requested.as_ref()),
                    None,
                )
                .await??,
            )
        };
        let mut effective_overrides = overrides.clone();
        if let Some(resolution) = &resolution {
            effective_overrides.model = resolution.command_model.as_deref();
            effective_overrides.model_resolved = true;
            if let Some(effort) = effective_overrides.effort {
                if !resolution.supported_efforts.is_empty()
                    && !resolution
                        .supported_efforts
                        .iter()
                        .any(|item| item == effort)
                {
                    effective_overrides.effort = None;
                }
            }
        }
        let overrides = &effective_overrides;

        // Direct API path: bypass CLI subprocess when an API key is configured
        match provider {
            "claude" | "" if !settings.anthropic_api_key.is_empty() => {
                return super::api_anthropic::call_anthropic_api(
                    app,
                    prompt,
                    allowed_tools,
                    system_prompt,
                    timeout_secs,
                    label,
                    &settings,
                    overrides,
                )
                .await;
            }
            "codex" if !settings.openai_api_key.is_empty() => {
                return super::api_openai::call_openai_api(
                    app,
                    prompt,
                    allowed_tools,
                    system_prompt,
                    timeout_secs,
                    label,
                    &settings,
                    overrides,
                )
                .await;
            }
            "gemini" if !settings.google_api_key.is_empty() => {
                return super::api_google::call_google_api(
                    app,
                    prompt,
                    allowed_tools,
                    system_prompt,
                    timeout_secs,
                    label,
                    &settings,
                    overrides,
                )
                .await;
            }
            // Local OpenAI-compatible server (Ollama, LM Studio, llama.cpp, vLLM).
            // Always direct HTTP — there is no CLI fallback for this provider.
            "local" => {
                return super::api_openai::call_local_api(
                    app,
                    prompt,
                    allowed_tools,
                    system_prompt,
                    timeout_secs,
                    label,
                    &settings,
                    overrides,
                )
                .await;
            }
            _ => {}
        }

        // Subprocess fallback. Claude uses --add-dir and Gemini uses
        // --include-directories for the same explicit read-root set. Codex's
        // sandbox restricts writes, not reads, so it needs no equivalent flag.
        let fallback_prompt;
        let cli_prompt = if provider == "gemini" {
            if let Some(context) = overrides.shared_context.as_ref() {
                fallback_prompt = context.prefixed_prompt(prompt)?;
                fallback_prompt.as_str()
            } else {
                prompt
            }
        } else {
            prompt
        };
        let cli_call = async {
            match provider {
                "codex" => {
                    let codex_cwd = cwd.or_else(|| {
                        extra_read_dirs
                            .iter()
                            .copied()
                            .find(|path| !path.trim().is_empty())
                    });
                    super::codex::call_codex(
                        app,
                        prompt,
                        allowed_tools,
                        system_prompt,
                        output_format,
                        timeout_secs,
                        label,
                        codex_cwd,
                        overrides,
                    )
                    .await
                }
                "gemini" => {
                    super::gemini::call_gemini(
                        app,
                        cli_prompt,
                        allowed_tools,
                        system_prompt,
                        output_format,
                        timeout_secs,
                        label,
                        cwd,
                        extra_read_dirs,
                        overrides,
                    )
                    .await
                }
                _ => {
                    call_claude(
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
                    )
                    .await
                }
            }
        };
        if let Some(write_dir) = overrides.write_dir {
            supervise_artifact_writes(cli_call, write_dir, super::logging::current_pass()).await
        } else {
            cli_call.await
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
    configure_silent_command(&mut std_cmd);
    match cwd {
        Some(dir) => {
            std_cmd.current_dir(dir);
        }
        None => {
            std_cmd.current_dir(std::env::temp_dir());
        }
    }
    Command::from(std_cmd)
}

/// Resolve an installed provider CLI and build a tokio command without
/// passing provider arguments through a shell. On Windows, npm `.cmd` shims
/// are represented as `node.exe <validated-entrypoint>` by the resolver.
pub(crate) fn build_provider_command(
    program: &str,
    cwd: Option<&str>,
    args: &[String],
) -> Result<Command, String> {
    let resolved = crate::deps::resolve_command(program).ok_or_else(|| {
        format!(
            "No launchable {program} CLI was found on PATH. On Windows, reinstall it with npm if its command shim is missing or damaged."
        )
    })?;
    let mut std_cmd = resolved.command(args);
    configure_silent_command(&mut std_cmd);
    match cwd {
        Some(dir) => {
            std_cmd.current_dir(dir);
        }
        None => {
            std_cmd.current_dir(std::env::temp_dir());
        }
    }
    Ok(Command::from(std_cmd))
}

/// Apply the shared environment and process-isolation flags to a standard
/// command. Extraction uses this directly because it runs on blocking worker
/// threads; async provider and installer commands use `build_silent_command`.
pub fn configure_silent_command(std_cmd: &mut std::process::Command) {
    std_cmd.env("PATH", crate::env::full_path());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Make the child the leader of a new process group so cancellation can
        // terminate every tool it spawns without signalling the app itself.
        std_cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn configured_child_owns_a_dedicated_process_group() {
        let mut command = std::process::Command::new("sleep");
        super::configure_silent_command(&mut command);
        let mut child = command.arg("5").spawn().unwrap();
        let pid = child.id();
        let process_group = unsafe { libc::getpgid(pid as i32) };
        assert_eq!(process_group, pid as i32);
        crate::commands::kill_process(pid);
        let _ = child.wait();
    }

    #[test]
    fn unwraps_result_envelope_and_sums_input_tokens() {
        let raw = r#"{"type":"result","subtype":"success","result":"hello world",
            "usage":{"input_tokens":100,"output_tokens":20,"cache_read_input_tokens":5,"cache_creation_input_tokens":3}}"#;
        let (text, usage) = parse_claude_result(raw);
        assert_eq!(text, "hello world");
        assert_eq!(
            usage,
            Some(crate::pipeline::logging::CallUsage {
                input_tokens: 108,
                output_tokens: 20,
                cached_input_tokens: 5,
                cache_write_input_tokens: 3,
                ..Default::default()
            })
        );
    }

    #[test]
    fn session_capability_errors_are_distinguished_from_model_failures() {
        assert!(is_cli_session_capability_error(
            "error: unknown option '--fork-session'"
        ));
        assert!(is_cli_session_capability_error(
            "failed to resume: session not found"
        ));
        assert!(!is_cli_session_capability_error(
            "service overloaded; try again"
        ));
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

    #[test]
    fn folder_orientation_uses_folder_as_cwd_without_broadening_roots() {
        let plan = plan_cli_workspace(
            Some("/Users/Mike/Documents/Paper Folder"),
            &["/Users/Mike/Documents/Paper Folder"],
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            plan.cwd.as_deref(),
            Some("/Users/Mike/Documents/Paper Folder")
        );
        assert!(plan.read_dirs.is_empty());
    }

    #[test]
    fn windows_write_workspace_keeps_all_read_roots_and_artifacts_as_cwd() {
        let plan = plan_cli_workspace(
            Some(r"C:\Users\Mike\Documents\Paper"),
            &[
                r"C:\Users\Mike\Documents\Paper",
                r"C:\Users\Mike\AppData\Local\Temp\pipeline_run",
                r"C:\Users\Mike\AppData\Local\Temp\pipeline_run\named",
                r"D:\Shared Inputs\Rubric",
            ],
            Some(r"C:\Users\Mike\AppData\Local\Temp\pipeline_prompt"),
            Some(r"C:\Users\Mike\.pipeline\runs\r1\artifacts"),
        )
        .unwrap();

        assert_eq!(
            plan.cwd.as_deref(),
            Some("C:/Users/Mike/.pipeline/runs/r1/artifacts")
        );
        assert_eq!(
            plan.read_dirs,
            vec![
                "C:/Users/Mike/AppData/Local/Temp/pipeline_prompt",
                "C:/Users/Mike/AppData/Local/Temp/pipeline_run",
                "C:/Users/Mike/Documents/Paper",
                "D:/Shared Inputs/Rubric",
            ]
        );
    }

    #[test]
    fn invalid_write_workspace_fails_closed() {
        let error = plan_cli_workspace(Some("/safe/source"), &[], None, Some("relative/artifacts"))
            .unwrap_err();
        assert!(error.contains("working directory must be absolute"));
    }

    #[test]
    fn long_prompt_gets_its_own_read_root() {
        let prompt = "x".repeat(MAX_DIRECT_PROMPT_LENGTH + 1);
        let prepared = prepare_cli_prompt(&prompt).unwrap();
        let path = prepared.path.as_deref().unwrap();
        let root = prepared.read_root.as_deref().unwrap();
        assert_eq!(cli_parent_dir(path).as_deref(), Some(root));
        assert_eq!(std::fs::read_to_string(path).unwrap(), prompt);

        let isolated = plan_cli_workspace(None, &[], Some(root), None).unwrap();
        assert_eq!(isolated.cwd.as_deref(), Some(root));
        assert!(isolated.read_dirs.is_empty());

        let alongside_source =
            plan_cli_workspace(Some("/Users/Mike/Paper"), &[], Some(root), None).unwrap();
        assert_eq!(alongside_source.read_dirs, vec![root.to_string()]);
        assert!(!alongside_source
            .read_dirs
            .contains(&std::env::temp_dir().to_string_lossy().replace('\\', "/")));
    }

    #[test]
    fn cli_paths_normalize_mac_windows_and_permission_rules() {
        assert_eq!(
            normalize_cli_root("/Users/Mike/Documents/../Paper").as_deref(),
            Some("/Users/Mike/Paper")
        );
        assert_eq!(
            normalize_cli_root(r"C:\Users\Mike\Documents\..\Paper\").as_deref(),
            Some("C:/Users/Mike/Paper")
        );
        assert_eq!(
            absolute_rule_path("/Users/Mike/Paper"),
            "//Users/Mike/Paper"
        );
        assert_eq!(
            absolute_rule_path("C:/Users/Mike/Paper"),
            "//C:/Users/Mike/Paper"
        );
    }

    #[test]
    fn live_artifact_monitor_rejects_oversized_files() {
        let dir = tempfile::tempdir().unwrap();
        let file = std::fs::File::create(dir.path().join("too-large.bin")).unwrap();
        file.set_len(MAX_LIVE_ARTIFACT_FILE_BYTES + 1).unwrap();
        assert!(check_live_artifact_quota(dir.path())
            .unwrap_err()
            .contains("quota exceeded"));
    }

    #[cfg(unix)]
    #[test]
    fn live_artifact_monitor_rejects_special_files() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt as _;

        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("fifo");
        let fifo_c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
        assert!(check_live_artifact_quota(dir.path())
            .unwrap_err()
            .contains("non-regular"));
    }
}
