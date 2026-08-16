use super::*;

#[derive(Clone)]
pub(super) struct RunSnapshot {
    pub(super) settings: crate::settings::Settings,
    pub(super) config: PipelineConfig,
    pub(super) profile_name: String,
    pub(super) config_fingerprint: String,
    pub(super) fingerprint: String,
    pub(super) workflow_source: String,
    pub(super) workflow_fingerprint: String,
    pub(super) workflow_json: String,
    pub(super) specialist_catalog_revision: String,
}

/// One-report override for Parallel steps and, when selected, their Merge
/// calls. `None` at the command boundary means use Settings and workflow
/// choices unchanged; a present value is fingerprinted into the immutable
/// launch snapshot and never persisted.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct RunParallelOverrides {
    pub agents: Vec<String>,
    #[serde(default)]
    pub model_overrides: std::collections::HashMap<String, crate::settings::ModelSelection>,
    #[serde(default)]
    pub effort_overrides: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub merge_agent: Option<String>,
    #[serde(default)]
    pub merge_model_overrides: std::collections::HashMap<String, crate::settings::ModelSelection>,
    #[serde(default)]
    pub merge_effort_overrides: std::collections::HashMap<String, String>,
}

pub(super) fn settings_for_snapshot_fingerprint(
    settings: &crate::settings::Settings,
) -> crate::settings::Settings {
    use sha2::{Digest as _, Sha256};

    let mut fingerprint_settings = settings.clone();
    for (label, secret) in [
        ("anthropic", &mut fingerprint_settings.anthropic_api_key),
        ("openai", &mut fingerprint_settings.openai_api_key),
        ("google", &mut fingerprint_settings.google_api_key),
        ("local", &mut fingerprint_settings.local_api_key),
    ] {
        if !secret.is_empty() {
            let mut secret_hash = Sha256::new();
            secret_hash.update(b"pipeline run snapshot secret v1\0");
            secret_hash.update(label.as_bytes());
            secret_hash.update(b"\0");
            secret_hash.update(secret.as_bytes());
            *secret = format!("<digest:{:x}>", secret_hash.finalize());
        }
    }
    fingerprint_settings
}

/// Hash serialized launch state with recursively sorted JSON object keys.
/// Settings, profiles, and one-run overrides all contain `HashMap` fields;
/// serializing those maps directly can assign different fingerprints to the
/// same values on separate plan and launch IPC calls.
pub(super) fn stable_snapshot_fingerprint<T: serde::Serialize>(
    value: &T,
) -> Result<String, serde_json::Error> {
    use sha2::{Digest as _, Sha256};

    fn sort_object_keys(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(object) => {
                let mut entries = std::mem::take(object).into_iter().collect::<Vec<_>>();
                entries.sort_by(|(left, _), (right, _)| left.cmp(right));
                for (key, mut child) in entries {
                    sort_object_keys(&mut child);
                    object.insert(key, child);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    sort_object_keys(item);
                }
            }
            _ => {}
        }
    }

    let mut canonical = serde_json::to_value(value)?;
    sort_object_keys(&mut canonical);
    let encoded = serde_json::to_vec(&canonical)?;
    let digest = format!("{:x}", Sha256::digest(encoded));
    Ok(digest[..16].to_string())
}

pub(super) fn load_run_snapshot() -> Result<RunSnapshot, String> {
    load_run_snapshot_for_profile(None)
}

/// Capture one executable settings/profile snapshot without changing the
/// persisted active profile. Headless callers use this for a per-invocation
/// `--profile`; the GUI continues to capture the active profile.
pub(super) fn load_run_snapshot_for_profile(
    profile_id: Option<&str>,
) -> Result<RunSnapshot, String> {
    load_run_snapshot_for_workflow(profile_id, None)
}

/// Capture either an installed profile or an ephemeral portable workflow.
/// The two sources are intentionally exclusive and neither path changes the
/// persisted active profile.
pub(super) fn load_run_snapshot_for_workflow(
    profile_id: Option<&str>,
    workflow: Option<&crate::commands::HeadlessWorkflow>,
) -> Result<RunSnapshot, String> {
    if profile_id.is_some() && workflow.is_some() {
        return Err(
            "Select either an installed profile or a workflow document, not both".to_string(),
        );
    }
    let mut settings = crate::settings::load_persisted_required().map_err(|e| {
        format!("Cannot start run because settings could not be loaded safely: {e}")
    })?;
    let (selected_profile, document, workflow_source) = if let Some(workflow) = workflow {
        let short = workflow
            .document
            .fingerprint
            .strip_prefix("sha256:")
            .unwrap_or(&workflow.document.fingerprint)
            .chars()
            .take(16)
            .collect::<String>();
        (
            format!("ephemeral-{short}"),
            workflow.document.clone(),
            workflow.source.clone(),
        )
    } else {
        let selected = profile_id
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| settings.active_profile.clone());
        let document = pipeline_config::load_required_workflow_for(&selected)?;
        let source = format!("profile:{selected}");
        (selected, document, source)
    };
    let config = document.config.clone();
    let profile_name = document.name.clone();
    pipeline_config::validate_enabled_sequential_step(&config.steps)?;
    crate::auto_review::validate_auto_review_preflight(&config)?;
    // The snapshot and run manifest should identify the selected profile, but
    // a one-off CLI run must not rewrite the desktop app's settings.json.
    settings.active_profile = selected_profile;
    let fingerprint_settings = settings_for_snapshot_fingerprint(&settings);
    let specialist_catalog_revision = if crate::auto_review::uses_auto_review_contract(&config) {
        crate::auto_review::catalog_revision().to_string()
    } else {
        String::new()
    };
    let fingerprint = stable_snapshot_fingerprint(&(
        &fingerprint_settings,
        &config,
        specialist_catalog_revision.as_str(),
    ))
    .map_err(|e| format!("Could not fingerprint run configuration: {e}"))?;
    Ok(RunSnapshot {
        settings,
        config,
        profile_name,
        config_fingerprint: fingerprint.clone(),
        fingerprint,
        workflow_source,
        workflow_fingerprint: document.fingerprint,
        workflow_json: document.canonical_json,
        specialist_catalog_revision,
    })
}

pub(super) fn bind_runtime_snapshot(
    mut snapshot: RunSnapshot,
    variables: &std::collections::HashMap<String, String>,
    extra_inputs: &std::collections::HashMap<String, String>,
) -> Result<RunSnapshot, String> {
    let variables = variables
        .iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    let extra_inputs = extra_inputs
        .iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    // Chain from the current fingerprint, not config_fingerprint: the
    // one-run agent override has already been folded in by
    // bind_parallel_overrides, and rebasing on the config fingerprint would
    // silently drop it from the plan/launch mismatch check.
    snapshot.fingerprint =
        stable_snapshot_fingerprint(&(snapshot.fingerprint.as_str(), variables, extra_inputs))
            .map_err(|error| format!("Could not fingerprint runtime options: {error}"))?;
    Ok(snapshot)
}

pub(super) fn bind_parallel_overrides(
    mut snapshot: RunSnapshot,
    overrides: Option<&RunParallelOverrides>,
) -> Result<RunSnapshot, String> {
    let Some(overrides) = overrides else {
        return Ok(snapshot);
    };
    if overrides.agents.is_empty() {
        return Err("Select at least one Parallel agent".to_string());
    }
    snapshot.settings.default_parallel_agents = overrides.agents.clone();
    snapshot.settings.default_parallel_model_overrides = overrides.model_overrides.clone();
    snapshot.settings.default_parallel_effort_overrides = overrides.effort_overrides.clone();
    for step in snapshot
        .config
        .steps
        .iter_mut()
        .filter(|step| step.phase == crate::pipeline_config::Phase::Parallel)
    {
        step.agents = overrides.agents.clone();
        step.model.clear();
        step.model_overrides = overrides.model_overrides.clone();
        step.effort.clear();
        step.effort_overrides = overrides.effort_overrides.clone();
    }
    if let Some(merge_agent) = overrides.merge_agent.as_deref() {
        if merge_agent.trim().is_empty() {
            return Err("Select a Merge agent".to_string());
        }
        snapshot.settings.default_merge_agent = merge_agent.to_string();
        snapshot.settings.default_merge_model_overrides = overrides.merge_model_overrides.clone();
        snapshot.settings.default_merge_effort_overrides = overrides.merge_effort_overrides.clone();
        // An explicit workflow Merge provider would otherwise bypass the
        // transient Settings policies above. Clearing it makes this one-run
        // selection the inherited, fully configured Merge default.
        snapshot.config.merge.agents.clear();
    } else if !overrides.merge_model_overrides.is_empty()
        || !overrides.merge_effort_overrides.is_empty()
    {
        return Err("Merge model or thinking overrides require a Merge agent".to_string());
    }
    snapshot.settings.validate()?;
    snapshot.fingerprint = stable_snapshot_fingerprint(&(snapshot.fingerprint.as_str(), overrides))
        .map_err(|error| format!("Could not fingerprint Parallel agent options: {error}"))?;
    Ok(snapshot)
}

pub(super) fn bind_foreground_launch(
    mut snapshot: RunSnapshot,
    paper_path: &str,
    input_interpretation: Option<&str>,
    diff: bool,
) -> Result<RunSnapshot, String> {
    snapshot.fingerprint = stable_snapshot_fingerprint(&(
        snapshot.fingerprint.as_str(),
        paper_path,
        input_interpretation,
        diff,
    ))
    .map_err(|error| format!("Could not fingerprint launch options: {error}"))?;
    Ok(snapshot)
}

pub(super) async fn check_snapshot_dependencies(
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
    tokio::task::spawn_blocking(move || {
        crate::deps::check_snapshot(settings, config, diff, input_path, extra_inputs)
    })
    .await
    .map_err(|error| format!("Dependency check failed: {error}"))
}

pub(super) fn require_snapshot_dependencies(
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
pub(super) fn validate_primary_input_path(
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
pub(super) fn validate_primary_input_selection(
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
                        "pdf" | "tex" | "docx"
                    )
                });
            if !supported {
                return Err("A document input must be a PDF, TeX, or DOCX file".to_string());
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
                    "Automatic Paper Review reviews a document, not a browsable source tree. \
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

pub(super) fn resolved_input_interpretation<'a>(
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

pub(super) fn validate_named_input_paths(
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
                let supported = std::path::Path::new(path)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| {
                        matches!(
                            extension.to_ascii_lowercase().as_str(),
                            "pdf" | "tex" | "docx"
                        )
                    });
                if !supported {
                    return Err(format!(
                        "Named input '{label}' must be a PDF, TeX, or DOCX document"
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(())
}
