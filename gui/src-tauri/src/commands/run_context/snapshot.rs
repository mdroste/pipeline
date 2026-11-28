use super::RunModelOverrides;
use crate::pipeline_config::{self, PipelineConfig};

#[derive(Clone)]
pub(in crate::commands) struct RunSnapshot {
    pub(in crate::commands) review_handoff_id: Option<String>,
    pub(in crate::commands) author_note: Option<String>,
    /// Match my writing: the author's style examples for Declaude's editors,
    /// from `pipeline::style_examples`.
    pub(in crate::commands) style_examples: Option<String>,
    pub(in crate::commands) settings: crate::settings::Settings,
    pub(in crate::commands) config: PipelineConfig,
    pub(in crate::commands) profile_name: String,
    pub(in crate::commands) config_fingerprint: String,
    pub(in crate::commands) fingerprint: String,
    pub(in crate::commands) workflow_source: String,
    pub(in crate::commands) workflow_fingerprint: String,
    pub(in crate::commands) workflow_json: String,
    pub(in crate::commands) specialist_catalog_revision: String,
    /// The one-report agent choice folded into `settings` and `config`, saved
    /// with the run so Resume and Regenerate can use it again.
    pub(in crate::commands) agent_overrides: Option<RunModelOverrides>,
}

pub(in crate::commands) fn settings_for_snapshot_fingerprint(
    settings: &crate::settings::Settings,
) -> crate::settings::Settings {
    use sha2::{Digest as _, Sha256};

    let mut fingerprint_settings = settings.clone();
    // Queue scheduling is not part of what a run executes; changing it must
    // not invalidate prepared launches.
    fingerprint_settings.max_concurrent_reviews = 0;
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
pub(in crate::commands) fn stable_snapshot_fingerprint<T: serde::Serialize>(
    value: &T,
) -> Result<String, serde_json::Error> {
    stable_content_fingerprint(value).map(|digest| digest[..16].to_owned())
}

pub(in crate::commands) fn stable_content_fingerprint<T: serde::Serialize>(
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
    Ok(digest)
}

pub(in crate::commands) fn load_run_snapshot() -> Result<RunSnapshot, String> {
    load_run_snapshot_for_profile(None)
}

/// Capture one executable settings/profile snapshot without changing the
/// persisted active profile. Headless callers use this for a per-invocation
/// `--profile`; the GUI continues to capture the active profile.
pub(in crate::commands) fn load_run_snapshot_for_profile(
    profile_id: Option<&str>,
) -> Result<RunSnapshot, String> {
    load_run_snapshot_for_workflow(profile_id, None)
}

/// Capture either an installed profile or an ephemeral portable workflow.
/// The two sources are intentionally exclusive and neither path changes the
/// persisted active profile.
pub(in crate::commands) fn load_run_snapshot_for_workflow(
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
        review_handoff_id: None,
        author_note: None,
        style_examples: None,
        settings,
        config,
        profile_name,
        config_fingerprint: fingerprint.clone(),
        fingerprint,
        workflow_source,
        workflow_fingerprint: document.fingerprint,
        workflow_json: document.canonical_json,
        specialist_catalog_revision,
        agent_overrides: None,
    })
}

pub(in crate::commands) fn bind_author_note(
    mut snapshot: RunSnapshot,
    note: Option<&str>,
) -> Result<RunSnapshot, String> {
    snapshot.author_note = crate::pipeline::author_note::normalize(note)?;
    if let Some(note) = &snapshot.author_note {
        snapshot.fingerprint =
            stable_snapshot_fingerprint(&(snapshot.fingerprint.as_str(), "author_note_v1", note))
                .map_err(|error| format!("Could not fingerprint the author note: {error}"))?;
    }
    Ok(snapshot)
}

/// Binds the author's style examples from About you when the researcher asked
/// to match their writing and the workflow has editors that use them. They
/// join the plan fingerprint, so a change between plan and launch is caught.
pub(in crate::commands) fn bind_style_examples(
    snapshot: RunSnapshot,
    match_writing: bool,
) -> Result<RunSnapshot, String> {
    // A data folder that can't be found has no About you to send.
    let folder = match_writing
        .then(crate::researcher::folder)
        .and_then(Result::ok);
    bind_style_examples_from(snapshot, folder.as_deref())
}

/// [`bind_style_examples`] with About you read from `folder`, or not at all.
pub(in crate::commands) fn bind_style_examples_from(
    mut snapshot: RunSnapshot,
    folder: Option<&std::path::Path>,
) -> Result<RunSnapshot, String> {
    snapshot.style_examples = folder
        .filter(|_| crate::pipeline::style_examples::accepts(&snapshot.config))
        .and_then(crate::pipeline::style_examples::render);
    if let Some(block) = &snapshot.style_examples {
        snapshot.fingerprint = stable_snapshot_fingerprint(&(
            snapshot.fingerprint.as_str(),
            "style_examples_v1",
            block,
        ))
        .map_err(|error| format!("Could not fingerprint the style examples: {error}"))?;
    }
    Ok(snapshot)
}

pub(in crate::commands) fn bind_foreground_launch(
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
