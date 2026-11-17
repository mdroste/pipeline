use super::RunSnapshot;
use crate::pipeline::extract;
use crate::pipeline_config::{self, PipelineConfig};

pub(in crate::commands) async fn check_snapshot_dependencies(
    snapshot: &RunSnapshot,
    diff: bool,
    input_path: Option<&str>,
    extra_inputs: &std::collections::HashMap<String, String>,
) -> Result<crate::deps::DepsReport, String> {
    let settings = snapshot.settings.clone();
    let mut config = snapshot.config.clone();
    pipeline_config::apply_agent_defaults(&mut config, &settings);
    let input_path = input_path.map(str::to_string);
    let extra_inputs = extra_inputs.clone();
    let native_codex =
        settings.codex_backend == "app_server" && settings.model_transport("codex") == "cli";
    let mut report = crate::run_scope::spawn_blocking(move || {
        crate::deps::check_snapshot(settings, config, diff, input_path, extra_inputs)
    })
    .await
    .map_err(|error| format!("Dependency check failed: {error}"))?;
    if native_codex {
        if let Some(codex) = report.deps.iter_mut().find(|dep| dep.name == "Codex CLI") {
            apply_workflow_codex_status(
                codex,
                Box::pin(crate::pipeline::codex_server::workflow_codex_status()).await,
            );
        }
        report.ready = report.deps.iter().all(crate::deps::dependency_ready);
    }
    Ok(report)
}

pub(in crate::commands) fn apply_workflow_codex_status(
    codex: &mut crate::deps::DepStatus,
    status: Result<crate::pipeline::codex_server::connection::ConnectionStatus, String>,
) {
    use crate::agent_runtime::codex::AccountStatus;

    codex.name = "ChatGPT (reviews)".into();
    // This account belongs to the managed Workflow connection. Ambient CLI
    // authentication (and Workspace's separate account) cannot establish it.
    codex.cli_auth_status = None;
    codex.help_url = None;
    match status {
        Ok(status) => {
            // A live App Server handshake supersedes the earlier version
            // subprocess probe, which can fail even while this runtime works.
            codex.found = true;
            codex.authenticated = Some(
                status.account.status == AccountStatus::Chatgpt
                    && !status.login_in_progress
                    && status.unresolved_attempts.is_empty(),
            );
            codex.version = status.version;
            codex.hint = if !status.unresolved_attempts.is_empty() {
                "Review the unresolved ChatGPT attempt in Settings → Models → OpenAI before another run.".into()
            } else if status.login_in_progress {
                "Complete ChatGPT sign-in in your browser, or cancel it in Settings → Models → OpenAI.".into()
            } else if status.account.status != AccountStatus::Chatgpt {
                "Sign in with a ChatGPT account in Settings → Models → OpenAI. The legacy Codex CLI keeps its own sign-in.".into()
            } else {
                String::new()
            };
        }
        Err(error) => {
            codex.authenticated = Some(false);
            codex.hint = error;
        }
    }
}

pub(in crate::commands) fn require_snapshot_dependencies(
    report: &crate::deps::DepsReport,
) -> Result<(), String> {
    if report.ready {
        return Ok(());
    }
    let blockers = report
        .deps
        .iter()
        .filter(|dependency| !crate::deps::dependency_ready(dependency))
        .map(|dependency| format!("{}: {}", dependency.name, dependency.hint))
        .collect::<Vec<_>>();
    Err(format!(
        "The captured workflow is not ready to run. {}",
        blockers.join(" ")
    ))
}

/// Validate a concrete primary selection against the captured workflow before
/// dependency probing or model work starts. Only the legacy empty input mode
/// auto-detects files versus folders; explicit modes enforce their declared
/// path kind so a stale selection cannot silently change workflow semantics.
pub(in crate::commands) fn validate_primary_input_path(
    config: &PipelineConfig,
    input_path: Option<&str>,
) -> Result<(), String> {
    let Some(input_path) = input_path else {
        // Setup/schema reads intentionally omit the path. The launch preflight
        // and run command both call this again with the concrete selection.
        return Ok(());
    };
    let input_mode = config.extraction.input_mode.trim();
    if input_mode == "none" {
        if input_path.trim().is_empty() {
            return Ok(());
        }
        return Err("The active workflow does not accept a primary input".to_string());
    }
    if input_path.trim().is_empty() {
        return Err("The active workflow requires a primary input".to_string());
    }
    let metadata = std::fs::metadata(input_path)
        .map_err(|error| format!("The selected input is unavailable: {error}"))?;
    match input_mode {
        "document" if !metadata.is_file() => {
            return Err(
                "The active workflow requires a document, but the selected input is not a regular file"
                    .to_string(),
            );
        }
        "folder" if !metadata.is_dir() => {
            return Err(
                "The active workflow requires a folder, but the selected input is not a folder"
                    .to_string(),
            );
        }
        "" if !metadata.is_file() && !metadata.is_dir() => {
            return Err("The selected input is not a regular file or folder".to_string());
        }
        _ => {}
    }
    Ok(())
}

/// Validate the run-time meaning assigned to the primary selection. Path kind
/// and input semantics are deliberately separate: the same directory may be a
/// LaTeX document project or a browsable source tree. Calls from older clients
/// omit the interpretation and retain the legacy workflow/path validation.
pub(in crate::commands) fn validate_primary_input_selection(
    config: &PipelineConfig,
    input_path: Option<&str>,
    interpretation: Option<&str>,
) -> Result<(), String> {
    let Some(raw) = interpretation
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return validate_primary_input_path(config, input_path);
    };
    let Some(input_path) = input_path else {
        return Ok(());
    };
    if config.extraction.input_mode.trim() == "none" {
        return Err("The active workflow does not accept a primary input".to_string());
    }
    if input_path.trim().is_empty() {
        return Err("The active workflow requires a primary input".to_string());
    }
    let path = std::path::Path::new(input_path);
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("The selected input is unavailable: {error}"))?;
    match raw {
        "document" => {
            if !metadata.is_file() {
                return Err("A document input must be a regular file".to_string());
            }
            let supported = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    matches!(
                        extension.to_ascii_lowercase().as_str(),
                        "pdf" | "tex" | "docx" | "pptx" | "md" | "markdown" | "txt"
                    )
                });
            if !supported {
                return Err(
                    "A document input must be a PDF, TeX, DOCX, PPTX, Markdown, or text file"
                        .to_string(),
                );
            }
        }
        "latex_project" => {
            if !metadata.is_dir() {
                return Err("A LaTeX project input must be a folder".to_string());
            }
            if crate::pipeline::extract::find_main_tex(path).is_none() {
                return Err(
                    "The selected folder has no top-level TeX file containing \\documentclass"
                        .to_string(),
                );
            }
        }
        "source_tree" => {
            if !metadata.is_dir() {
                return Err("A browsable source-tree input must be a folder".to_string());
            }
            // The adaptive router prompt is used verbatim (it is not a stock
            // survey, so the folder-survey swap never applies) and wraps the
            // primary text as `<paper>…</paper>`. A source tree supplies only
            // a file inventory there, so routing would classify from file
            // names and every reviewer would read the inventory as the paper.
            if crate::auto_review::uses_auto_review_contract(config) {
                return Err(
                    "This review reads a single document, not a browsable source tree. \
                     Select the folder as a LaTeX project (if it contains the paper's TeX \
                     source), pick the paper file directly, or switch to a folder-oriented \
                     workflow."
                        .to_string(),
                );
            }
        }
        "batch" => {
            return Err("A batch selection must be launched through the batch runner".to_string());
        }
        _ => {
            return Err(format!(
                "Unknown input interpretation '{raw}'. Expected document, latex_project, source_tree, or batch"
            ));
        }
    }
    Ok(())
}

pub(in crate::commands) fn resolved_input_interpretation<'a>(
    configured: &str,
    input_path: &str,
    requested: Option<&'a str>,
) -> &'a str {
    if let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) {
        return requested;
    }
    match extract::effective_input_mode(configured, input_path) {
        "folder" => "source_tree",
        "none" => "none",
        _ => "document",
    }
}

pub(in crate::commands) fn validate_named_input_paths(
    config: &PipelineConfig,
    extra_inputs: &std::collections::HashMap<String, String>,
    require_required: bool,
) -> Result<(), String> {
    for slot in &config.extraction.extra_inputs {
        let path = extra_inputs
            .get(&slot.key)
            .map(|path| path.trim())
            .filter(|path| !path.is_empty());
        let Some(path) = path else {
            if require_required && slot.required {
                let label = if slot.label.trim().is_empty() {
                    slot.key.as_str()
                } else {
                    slot.label.as_str()
                };
                return Err(format!("Missing required input '{label}'"));
            }
            continue;
        };
        let label = if slot.label.trim().is_empty() {
            slot.key.as_str()
        } else {
            slot.label.as_str()
        };
        let metadata = std::fs::metadata(path)
            .map_err(|error| format!("Named input '{label}' is unavailable: {error}"))?;
        match slot.mode.as_str() {
            "folder" if !metadata.is_dir() => {
                return Err(format!(
                    "Named input '{label}' requires a folder, but the selected input is not a folder"
                ));
            }
            "document" if !metadata.is_file() => {
                return Err(format!(
                    "Named input '{label}' requires a document, but the selected input is not a regular file"
                ));
            }
            "document" => {
                let extension = std::path::Path::new(path)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .map(str::to_ascii_lowercase)
                    .unwrap_or_default();
                let supported = matches!(
                    extension.as_str(),
                    "pdf" | "tex" | "docx" | "pptx" | "md" | "markdown" | "txt"
                );
                if !supported {
                    return Err(format!(
                        "Named input '{label}' must be a PDF, TeX, DOCX, PPTX, Markdown, or text document"
                    ));
                }
                if !slot.extensions.is_empty() && !slot.extensions.contains(&extension) {
                    return Err(format!(
                        "Named input '{label}' does not accept .{extension}; allowed extensions: {}",
                        slot.extensions.join(", ")
                    ));
                }
                if slot.max_bytes > 0 && metadata.len() > slot.max_bytes {
                    return Err(format!(
                        "Named input '{label}' is {} bytes; its workflow limit is {} bytes",
                        metadata.len(),
                        slot.max_bytes
                    ));
                }
                if !slot.mime_types.is_empty() {
                    let mime = match extension.as_str() {
                        "pdf" => "application/pdf",
                        "tex" => "application/x-tex",
                        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
                        _ => "application/octet-stream",
                    };
                    if !slot.mime_types.iter().any(|allowed| allowed == mime) {
                        return Err(format!(
                            "Named input '{label}' has MIME type '{mime}', which this workflow does not accept"
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}
