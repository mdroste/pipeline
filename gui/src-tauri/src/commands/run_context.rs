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
    validate_runtime_bindings(&snapshot.config, variables, extra_inputs, false)?;
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

pub(super) fn validate_runtime_bindings(
    config: &PipelineConfig,
    variables: &std::collections::HashMap<String, String>,
    extra_inputs: &std::collections::HashMap<String, String>,
    require_required: bool,
) -> Result<(), String> {
    let declared_variables = config
        .variables
        .iter()
        .map(|spec| (spec.key.as_str(), spec))
        .collect::<std::collections::HashMap<_, _>>();
    for key in variables.keys() {
        if !declared_variables.contains_key(key.as_str()) {
            return Err(format!("Run supplied undeclared variable '{key}'"));
        }
    }
    for spec in &config.variables {
        let value = variables
            .get(&spec.key)
            .map(String::as_str)
            .unwrap_or(&spec.default);
        let label = if spec.label.trim().is_empty() {
            spec.key.as_str()
        } else {
            spec.label.as_str()
        };
        if require_required && spec.required && value.trim().is_empty() {
            return Err(format!("Missing required variable '{label}'"));
        }
        if require_required && spec.secret && value == "[redacted secret; re-enter to rerun]" {
            return Err(format!(
                "Secret variable '{label}' must be entered again before this run"
            ));
        }
        if value.is_empty() {
            continue;
        }
        let length = value.chars().count() as u32;
        if spec
            .validation
            .min_length
            .is_some_and(|minimum| length < minimum)
        {
            return Err(format!(
                "Variable '{label}' must contain at least {} characters",
                spec.validation.min_length.unwrap_or_default()
            ));
        }
        if spec
            .validation
            .max_length
            .is_some_and(|maximum| length > maximum)
        {
            return Err(format!(
                "Variable '{label}' must contain at most {} characters",
                spec.validation.max_length.unwrap_or_default()
            ));
        }
        if !spec.validation.pattern.is_empty()
            && !regex::Regex::new(&spec.validation.pattern)
                .map_err(|error| format!("Variable '{label}' validation is invalid: {error}"))?
                .is_match(value)
        {
            return Err(format!(
                "Variable '{label}' does not match its required format"
            ));
        }
        if spec.kind == "choice" && !spec.choices.iter().any(|choice| choice == value) {
            return Err(format!(
                "Variable '{label}' is not one of its allowed choices"
            ));
        }
    }

    let declared_inputs = config
        .extraction
        .extra_inputs
        .iter()
        .map(|slot| slot.key.as_str())
        .collect::<std::collections::HashSet<_>>();
    for key in extra_inputs.keys() {
        if !declared_inputs.contains(key.as_str()) {
            return Err(format!("Run supplied undeclared named input '{key}'"));
        }
    }
    Ok(())
}

/// Values safe to retain in manifests. Secret values are deliberately not
/// recoverable; a re-run must ask for them again.
pub(super) fn durable_variables(
    config: &PipelineConfig,
    variables: &std::collections::HashMap<String, String>,
) -> std::collections::HashMap<String, String> {
    variables
        .iter()
        .map(|(key, value)| {
            let secret = config
                .variables
                .iter()
                .any(|spec| spec.key == *key && spec.secret);
            (
                key.clone(),
                if secret {
                    "[redacted secret; re-enter to rerun]".to_string()
                } else {
                    value.clone()
                },
            )
        })
        .collect()
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
    let native_codex =
        settings.codex_backend == "app_server" && settings.model_transport("codex") == "cli";
    let mut report = tokio::task::spawn_blocking(move || {
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

pub(super) fn apply_workflow_codex_status(
    codex: &mut crate::deps::DepStatus,
    status: Result<crate::pipeline::codex_server::connection::ConnectionStatus, String>,
) {
    use crate::agent_runtime::codex::AccountStatus;

    codex.name = "Workflow ChatGPT".into();
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
                "Review the unresolved Workflow ChatGPT attempt in Settings → Providers → ChatGPT before another run.".into()
            } else if status.login_in_progress {
                "Complete Workflow ChatGPT sign-in in your browser, or cancel it in Settings → Providers → ChatGPT.".into()
            } else if status.account.status != AccountStatus::Chatgpt {
                "Sign in with a ChatGPT account in Settings → Providers → ChatGPT. Workspace and the legacy Codex CLI keep separate sign-ins.".into()
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
                let extension = std::path::Path::new(path)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .map(str::to_ascii_lowercase)
                    .unwrap_or_default();
                let supported = matches!(extension.as_str(), "pdf" | "tex" | "docx");
                if !supported {
                    return Err(format!(
                        "Named input '{label}' must be a PDF, TeX, or DOCX document"
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
