//! Argument construction for the legacy Review CLI.

use super::CodexSessionMode;
use crate::pipeline::claude::normalize_cli_root;
use crate::pipeline::providers::LlmOverrides;

pub(super) fn codex_effective_cwd(
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

pub(super) fn codex_web_search_override(allowed_tools: &[&str]) -> String {
    let mode = if allowed_tools.contains(&"WebSearch") {
        "live"
    } else {
        "disabled"
    };
    format!("web_search=\"{mode}\"")
}

pub(super) fn append_codex_output_schema(cmd_args: &mut Vec<String>, schema_path: Option<&str>) {
    if let Some(path) = schema_path {
        cmd_args.push("--output-schema".to_string());
        cmd_args.push(path.to_string());
    }
}

/// Build the `codex exec` argument vector. Pure so the sandbox contract can
/// be asserted without spawning the CLI. Returns the argv and whether the
/// call runs in write mode.
pub(super) fn codex_exec_args(
    allowed_tools: &[&str],
    system_prompt: Option<&str>,
    overrides: &LlmOverrides<'_>,
    session_mode: CodexSessionMode<'_>,
    schema_path: Option<&str>,
    last_message_path: &str,
) -> Result<(Vec<String>, bool), String> {
    let mut cmd_args: Vec<String> = vec!["exec".to_string()];
    if matches!(session_mode, CodexSessionMode::Resume(_)) {
        cmd_args.push("resume".to_string());
    }
    // Use JSON mode for clean machine-readable output
    cmd_args.push("--json".to_string());
    // Do not inherit ambient search settings from the user's Codex config.
    // Contribution gets current/live search; every other review branch and
    // the shared-context primer keep search disabled.
    cmd_args.push("-c".to_string());
    cmd_args.push(codex_web_search_override(allowed_tools));

    // codex exec refuses to run outside a "trusted" git repo unless this is
    // set ("Not inside a trusted directory and --skip-git-repo-check was not
    // specified."). Our subprocess cwd is the system temp dir (read-only
    // calls) or the run's artifact dir (write calls) — neither is a git repo —
    // so without this flag codex exits without ever calling the model, leaving
    // empty output. Sandboxing is enforced independently via --sandbox, so
    // skipping the git-repo check costs nothing.
    cmd_args.push("--skip-git-repo-check".to_string());
    // Codex submits this file to OpenAI's strict Responses API, which
    // requires additionalProperties:false and all-required objects; the
    // strict projection widens optional fields to nullable and the host
    // strips the nulls back out. Inexpressible (yet valid) schemas proceed
    // without a native constraint rather than triggering a provider 400.
    append_codex_output_schema(&mut cmd_args, schema_path);

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
    } else {
        // `codex exec resume` has no --sandbox flag and does not restore the
        // forked thread's sandbox: without an override the user's global
        // ~/.codex/config.toml (often danger-full-access) would govern the
        // resumed review call. Pin it through the documented config override.
        cmd_args.push("-c".to_string());
        cmd_args.push(format!("sandbox_mode=\"{sandbox}\""));
        cmd_args.push("-c".to_string());
        cmd_args.push("approval_policy=\"never\"".to_string());
    }

    if needs_write {
        // workspace-write also opens /tmp and $TMPDIR by default; close
        // them so a step can't touch another step's temp prompt files.
        // (Reads stay unrestricted — the sandbox governs writes only.)
        cmd_args.push("-c".to_string());
        cmd_args.push("sandbox_workspace_write.network_access=false".to_string());
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

    // Reviewer instructions use the supported developer layer. `instructions`
    // is reserved/ignored by current Codex; quote literal text as a TOML string.
    if let Some(sys) = system_prompt {
        cmd_args.push("-c".to_string());
        cmd_args.push(format!(
            "developer_instructions={}",
            serde_json::to_string(sys).map_err(|error| error.to_string())?
        ));
    }

    // Apply Codex settings (model, reasoning effort) with optional per-step overrides.
    let model_src = overrides.model.unwrap_or("");
    let model = crate::settings::sanitize_cli_arg(model_src);
    if !model.is_empty() {
        cmd_args.push("--model".to_string());
        cmd_args.push(model);
    }
    let effort_src = overrides.effort.unwrap_or("");
    let effort = crate::settings::sanitize_cli_arg(effort_src);
    if !effort.is_empty() {
        cmd_args.push("-c".to_string());
        cmd_args.push(format!("model_reasoning_effort={}", effort));
    }

    // Codex reads the full prompt from a closed pipe. This avoids turning a
    // long prompt into a meta-instruction to use a Read tool, which can surface
    // as progress narration and makes prompt delivery tool-dependent.
    // JSONL contains progress/commentary messages as well as the terminal
    // response. Have Codex write the one terminal message separately so those
    // channels can never be flattened into a report.
    cmd_args.push("--output-last-message".to_string());
    cmd_args.push(last_message_path.to_string());
    if let CodexSessionMode::Resume(id) = session_mode {
        cmd_args.push(id.to_string());
    }
    cmd_args.push("-".to_string());
    Ok((cmd_args, needs_write))
}
