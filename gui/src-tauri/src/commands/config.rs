use super::*;

/// Open the active research data folder. The path is resolved server-side
/// (never passed from the frontend) so there is nothing to sanitize.
#[tauri::command]
pub async fn open_pipeline_dir() -> Result<(), String> {
    let dir = crate::storage::data_root()?;
    let path_str = dir.to_string_lossy().to_string();

    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg("--")
        .arg(&path_str)
        .spawn()
        .map_err(|e| format!("Failed to open folder: {e}"))?;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer")
            .arg(&path_str)
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open")
        .arg("--")
        .arg(&path_str)
        .spawn()
        .map_err(|e| format!("Failed to open folder: {e}"))?;

    Ok(())
}

// --- Settings ---

#[tauri::command]
pub async fn get_storage_settings() -> Result<crate::storage::StorageSettings, String> {
    tokio::task::spawn_blocking(crate::storage::settings)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn set_storage_directory(
    directory: String,
) -> Result<crate::storage::StorageSettings, String> {
    tokio::task::spawn_blocking(move || crate::storage::set_directory(&directory))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
pub struct SettingsResponse {
    pub settings: crate::settings::Settings,
    pub warnings: Vec<String>,
}

/// Opaque renderer-side marker for a credential that is already stored. The
/// marker carries no secret material; commands resolve it only against the
/// persisted settings and never send it to a provider.
const STORED_SECRET: &str = "__PIPELINE_STORED_SECRET__";

fn redact_settings(mut settings: crate::settings::Settings) -> crate::settings::Settings {
    for secret in [
        &mut settings.anthropic_api_key,
        &mut settings.openai_api_key,
        &mut settings.google_api_key,
        &mut settings.local_api_key,
    ] {
        if !secret.is_empty() {
            *secret = STORED_SECRET.to_string();
        }
    }
    settings
}

fn materialize_settings(
    mut candidate: crate::settings::Settings,
    persisted: &crate::settings::Settings,
) -> crate::settings::Settings {
    if candidate.anthropic_api_key == STORED_SECRET {
        candidate
            .anthropic_api_key
            .clone_from(&persisted.anthropic_api_key);
    }
    if candidate.openai_api_key == STORED_SECRET {
        candidate
            .openai_api_key
            .clone_from(&persisted.openai_api_key);
    }
    if candidate.google_api_key == STORED_SECRET {
        candidate
            .google_api_key
            .clone_from(&persisted.google_api_key);
    }
    if candidate.local_api_key == STORED_SECRET {
        // A saved bearer token belongs to one endpoint. Changing the endpoint
        // requires entering its credential again instead of silently forwarding
        // the old token to a new host.
        if candidate.local_base_url == persisted.local_base_url {
            candidate.local_api_key.clone_from(&persisted.local_api_key);
        } else {
            candidate.local_api_key.clear();
        }
    }
    candidate
}

#[tauri::command]
pub async fn get_settings() -> Result<SettingsResponse, String> {
    let (settings, warnings) = crate::settings::load_with_warnings();
    Ok(SettingsResponse {
        settings: redact_settings(settings),
        warnings,
    })
}

#[tauri::command]
pub async fn save_settings(settings: crate::settings::Settings) -> Result<(), String> {
    let persisted = crate::settings::load_persisted();
    let settings = materialize_settings(settings, &persisted);
    crate::settings::save_preserving_active(&settings)
}

/// Discover models for the provider's currently active transport. Settings are
/// accepted from the unsaved Settings screen so entering/removing an API key
/// immediately switches between CLI and API catalogs.
#[tauri::command]
pub async fn get_model_catalog(
    provider: String,
    settings: Option<crate::settings::Settings>,
    refresh: Option<bool>,
) -> Result<crate::model_catalog::ModelCatalog, String> {
    let persisted = crate::settings::load_persisted();
    let settings = settings
        .map(|candidate| materialize_settings(candidate, &persisted))
        .unwrap_or(persisted)
        .normalized();
    crate::model_catalog::discover(&provider, &settings, refresh.unwrap_or(false)).await
}

#[cfg(test)]
mod settings_response_tests {
    use super::*;

    #[test]
    fn renderer_settings_never_contain_saved_credentials() {
        let settings = crate::settings::Settings {
            anthropic_api_key: "anthropic-secret".into(),
            openai_api_key: "openai-secret".into(),
            google_api_key: "google-secret".into(),
            local_api_key: "local-secret".into(),
            ..Default::default()
        };
        let redacted = redact_settings(settings);
        for secret in [
            redacted.anthropic_api_key,
            redacted.openai_api_key,
            redacted.google_api_key,
            redacted.local_api_key,
        ] {
            assert_eq!(secret, STORED_SECRET);
        }
    }

    #[test]
    fn saved_local_secret_is_not_forwarded_to_a_changed_endpoint() {
        let persisted = crate::settings::Settings {
            local_base_url: "https://trusted.example/v1".into(),
            local_api_key: "trusted-secret".into(),
            ..Default::default()
        };
        let candidate = crate::settings::Settings {
            local_base_url: "https://other.example/v1".into(),
            local_api_key: STORED_SECRET.into(),
            ..persisted.clone()
        };
        assert!(materialize_settings(candidate, &persisted)
            .local_api_key
            .is_empty());
    }
}

// --- Pipeline config ---

#[tauri::command]
pub async fn get_pipeline_config() -> Result<PipelineConfig, String> {
    Ok(pipeline_config::load())
}

#[tauri::command]
pub async fn get_auto_review_catalog() -> crate::auto_review::AutoReviewCatalog {
    crate::auto_review::catalog()
}

/// Show the exact portable schema sent to providers after catalog expansion and
/// removal of host-owned constraints and Pipeline metadata.
#[tauri::command]
pub async fn resolve_orientation_schema_catalogs(
    schema: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let encoded = serde_json::to_vec(&schema)
        .map_err(|error| format!("Could not serialize orientation schema: {error}"))?;
    if encoded.len() > crate::pipeline_config::MAX_OUTPUT_SCHEMA_BYTES {
        return Err("Orientation schema exceeds Pipeline's size limit".to_string());
    }
    let resolved = crate::auto_review::resolve_schema_catalogs(&schema)?;
    crate::pipeline::structured::provider_schema(&resolved)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSetupResponse {
    pub profile_id: String,
    pub profile_config_snapshot_id: String,
    pub input_mode: String,
    pub variables: Vec<crate::pipeline_config::VarSpec>,
    pub input_slots: Vec<crate::pipeline_config::InputSlot>,
}

/// Return the inexpensive profile metadata needed to render New Run. This
/// deliberately skips path validation, dependency probes, runtime binding, and
/// scheduler planning; `get_execution_plan` performs those checks once the user
/// asks to review the report.
#[tauri::command]
pub async fn get_run_setup() -> Result<RunSetupResponse, String> {
    let snapshot = load_run_snapshot()?;
    Ok(RunSetupResponse {
        profile_id: snapshot.settings.active_profile.clone(),
        profile_config_snapshot_id: snapshot.config_fingerprint,
        input_mode: match snapshot.config.extraction.input_mode.trim() {
            "" => "document".to_string(),
            mode => mode.to_string(),
        },
        variables: snapshot.config.variables,
        input_slots: snapshot.config.extraction.extra_inputs,
    })
}

/// Copy one host-owned adaptive specialist into an ordinary editable step.
/// The returned StepConfig owns its prompt and has no durable template link.
#[tauri::command]
pub async fn get_auto_review_specialist_step(
    id: String,
) -> Result<crate::pipeline_config::StepConfig, String> {
    crate::auto_review::copyable_specialist_step(&id)
        .ok_or_else(|| format!("Unknown adaptive-review specialist '{id}'"))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPlanResponse {
    pub profile_id: String,
    pub profile_config_snapshot_id: String,
    pub profile_snapshot_id: String,
    pub configured_input_mode: String,
    pub input_mode: String,
    pub input_interpretation: String,
    pub variables: Vec<crate::pipeline_config::VarSpec>,
    pub input_slots: Vec<crate::pipeline_config::InputSlot>,
    pub readiness: crate::deps::DepsReport,
    pub stages: Vec<executor::ExecutionPlanStage>,
    pub parallel_agents: Vec<String>,
    pub merge_agent: Option<String>,
}

/// Return the canonical scheduler timeline for one immutable active-profile
/// snapshot. The frontend must not independently infer dependency waves.
#[tauri::command]
pub async fn get_execution_plan(
    variables: Option<std::collections::HashMap<String, String>>,
    extra_inputs: Option<std::collections::HashMap<String, String>>,
    expected_profile_config_snapshot_id: Option<String>,
    diff: Option<bool>,
    paper_path: Option<String>,
    input_interpretation: Option<String>,
    run_parallel_overrides: Option<RunParallelOverrides>,
) -> Result<ExecutionPlanResponse, String> {
    let variables = variables.unwrap_or_default();
    let extra_inputs = extra_inputs.unwrap_or_default();
    crate::safety::validate_runtime_context(&variables, "Planned run variables")?;
    crate::safety::validate_runtime_context(&extra_inputs, "Planned named input paths")?;
    let snapshot = load_run_snapshot()?;
    if expected_profile_config_snapshot_id
        .as_deref()
        .filter(|expected| !expected.is_empty())
        .is_some_and(|expected| expected != snapshot.config_fingerprint)
    {
        return Err(
            "The active profile or settings changed while run inputs were being collected. Reload the run setup and try again."
                .to_string(),
        );
    }
    let snapshot = bind_parallel_overrides(snapshot, run_parallel_overrides.as_ref())?;
    validate_primary_input_selection(
        &snapshot.config,
        paper_path.as_deref(),
        input_interpretation.as_deref(),
    )?;
    // This command is also used to discover the profile's input slots before
    // the user has filled them. Validate every concrete selection here; the
    // execution commands enforce that all required slots are present.
    validate_named_input_paths(&snapshot.config, &extra_inputs, false)?;
    let diff = diff.unwrap_or(false);
    let readiness =
        check_snapshot_dependencies(&snapshot, diff, paper_path.as_deref(), &extra_inputs).await?;
    let profile_config_snapshot_id = snapshot.config_fingerprint.clone();
    let resolved_interpretation = paper_path
        .as_deref()
        .map(|path| {
            resolved_input_interpretation(
                &snapshot.config.extraction.input_mode,
                path,
                input_interpretation.as_deref(),
            )
            .to_string()
        })
        .unwrap_or_else(|| match snapshot.config.extraction.input_mode.trim() {
            "folder" => "source_tree".to_string(),
            "none" => "none".to_string(),
            _ => input_interpretation
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "document".to_string()),
        });
    let input_mode = match resolved_interpretation.as_str() {
        "source_tree" => "folder".to_string(),
        "none" => "none".to_string(),
        _ => "document".to_string(),
    };
    let variable_specs = snapshot.config.variables.clone();
    let input_slots = snapshot.config.extraction.extra_inputs.clone();
    let snapshot = bind_runtime_snapshot(snapshot, &variables, &extra_inputs)?;
    let snapshot = bind_foreground_launch(
        snapshot,
        paper_path.as_deref().unwrap_or(""),
        input_interpretation.as_deref(),
        diff,
    )?;
    let mut planning_config = snapshot.config.clone();
    pipeline_config::apply_agent_defaults(&mut planning_config, &snapshot.settings);
    let merge_agent = (planning_config.merge.enabled
        && planning_config.steps.iter().any(|step| {
            step.enabled
                && step.phase == crate::pipeline_config::Phase::Parallel
                && step.agents.len() > 1
        }))
    .then(|| {
        planning_config
            .merge
            .agents
            .first()
            .cloned()
            .unwrap_or_else(|| snapshot.settings.merge_agent().to_string())
    });
    let mut stages = executor::execution_plan(&planning_config)?;
    if let Some(stage) = stages.iter_mut().find(|stage| stage.kind == "extracting") {
        stage.label = executor::input_processing_label(&resolved_interpretation).to_string();
    }
    Ok(ExecutionPlanResponse {
        profile_id: snapshot.settings.active_profile.clone(),
        profile_config_snapshot_id,
        profile_snapshot_id: snapshot.fingerprint,
        configured_input_mode: match snapshot.config.extraction.input_mode.trim() {
            "" => "document".to_string(),
            mode => mode.to_string(),
        },
        input_mode,
        input_interpretation: resolved_interpretation,
        variables: variable_specs,
        input_slots,
        readiness,
        stages,
        parallel_agents: snapshot.settings.parallel_agents(),
        merge_agent,
    })
}

#[tauri::command]
pub async fn save_pipeline_config(
    config: PipelineConfig,
    profile_id: Option<String>,
) -> Result<(), String> {
    let target = profile_id.unwrap_or_else(pipeline_config::get_active_profile_id);
    pipeline_config::save_for(&target, &config)
}

#[tauri::command]
pub async fn get_default_parallel_template() -> String {
    pipeline_config::default_parallel_template()
}

/// Compiled-in default text for a named prompt (no user overrides applied).
/// Backs the editor's "reset to …" actions, e.g. the generic vs paper-review
/// context templates and survey prompts.
#[tauri::command]
pub async fn get_default_prompt(name: String) -> Result<String, String> {
    crate::prompts::compiled_default(&name)
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Unknown prompt: {name}"))
}

#[derive(serde::Serialize)]
pub struct OrientationDefaults {
    pub prompt: String,
    pub schema: serde_json::Value,
}

/// A stock orientation prompt and its authoritative artifact contract. The
/// editor applies these as one operation so changing survey variants cannot
/// leave a stale schema behind.
#[tauri::command]
pub async fn get_orientation_defaults(name: String) -> Result<OrientationDefaults, String> {
    let prompt = crate::prompts::compiled_default(&name)
        .ok_or_else(|| format!("Unknown prompt: {name}"))?
        .to_string();
    let schema = crate::orientation_contract::schema_for_prompt_name(&name)
        .ok_or_else(|| format!("Prompt has no stock orientation schema: {name}"))?;
    Ok(OrientationDefaults { prompt, schema })
}

/// Compact Automatic Paper Review router template. Runtime expands its catalog
/// placeholders from the live manifest catalog.
#[tauri::command]
pub async fn get_auto_review_orientation_prompt() -> Result<String, String> {
    Ok(crate::auto_review::orientation_prompt())
}

/// Current-catalog router prompt and its matching orientation schema.
#[derive(serde::Serialize)]
pub struct AutoReviewOrientationDefaults {
    pub prompt: String,
    pub schema: serde_json::Value,
}

/// The router template and its compact catalog-backed orientation contract.
/// The editor restores both together; catalog rows are injected only at run
/// time and never stored in the profile prompt.
/// (`get_auto_review_orientation_prompt` above remains for compatibility.)
#[tauri::command]
pub async fn get_auto_review_orientation_defaults() -> Result<AutoReviewOrientationDefaults, String>
{
    Ok(AutoReviewOrientationDefaults {
        prompt: crate::auto_review::orientation_prompt(),
        schema: crate::auto_review::orientation_schema(),
    })
}

/// The canonical published-findings contract, served from the binary so the
/// editor's insertable template cannot drift from host validation.
#[tauri::command]
pub async fn get_findings_output_schema() -> serde_json::Value {
    crate::findings::output_schema()
}

/// What providers actually receive for a portable schema: live references and
/// catalogs resolved, host-only keywords removed, plus the projected size
/// against the CLI transport ceiling and the effective OpenAI strict mode.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSchemaPreview {
    pub projected: serde_json::Value,
    pub bytes: usize,
    pub limit: usize,
    pub openai_strict: bool,
}

#[tauri::command]
pub async fn preview_provider_schema(
    schema: serde_json::Value,
) -> Result<ProviderSchemaPreview, String> {
    let resolved = crate::pipeline::structured::resolve_schema_reference(&schema)?;
    let resolved = crate::auto_review::resolve_schema_catalogs(&resolved)?;
    let projected = crate::pipeline::structured::provider_schema(&resolved)?;
    let bytes = serde_json::to_vec(&projected)
        .map_err(|error| format!("Failed to serialize provider schema: {error}"))?
        .len();
    Ok(ProviderSchemaPreview {
        openai_strict: crate::pipeline::api_openai::strict_capable(&projected),
        projected,
        bytes,
        limit: crate::pipeline::structured::MAX_PROVIDER_SCHEMA_BYTES,
    })
}

#[tauri::command]
pub async fn reset_pipeline_config() -> Result<PipelineConfig, String> {
    Ok(pipeline_config::reset_defaults())
}

// --- Profiles ---

#[tauri::command]
pub async fn list_profiles() -> Result<Vec<ProfileSummary>, String> {
    pipeline_config::list_profiles()
}

#[tauri::command]
pub async fn get_active_profile() -> Result<String, String> {
    Ok(pipeline_config::get_active_profile_id())
}

#[tauri::command]
pub async fn create_profile(name: String) -> Result<ProfileSummary, String> {
    pipeline_config::create_profile(&name)
}

#[tauri::command]
pub async fn duplicate_profile(
    source_id: String,
    new_name: String,
) -> Result<ProfileSummary, String> {
    pipeline_config::duplicate_profile(&source_id, &new_name)
}

#[tauri::command]
pub async fn rename_profile(id: String, new_name: String) -> Result<ProfileSummary, String> {
    pipeline_config::rename_profile(&id, &new_name)
}

#[tauri::command]
pub async fn delete_profile(id: String) -> Result<(), String> {
    pipeline_config::delete_profile(&id)
}

#[tauri::command]
pub async fn switch_profile(id: String) -> Result<PipelineConfig, String> {
    pipeline_config::switch_profile(&id)
}

#[tauri::command]
pub async fn export_item(
    app: AppHandle,
    json: String,
    suggested_name: Option<String>,
) -> Result<Option<String>, String> {
    let name = suggested_name.as_deref().unwrap_or("pipeline-export.json");
    save_via_dialog(&app, name, "JSON", "json", json.into_bytes()).await
}

#[tauri::command]
pub async fn import_item(path: String) -> Result<serde_json::Value, String> {
    let content = read_import_file(&path)?;
    let envelope = pipeline_config::import_envelope(&content)?;
    serde_json::to_value(&envelope).map_err(|e| format!("Serialize error: {e}"))
}

#[tauri::command]
pub async fn export_profile(
    app: AppHandle,
    id: String,
    suggested_name: Option<String>,
) -> Result<Option<String>, String> {
    let json = pipeline_config::export_profile_data(&id)?;
    let name = suggested_name.as_deref().unwrap_or("pipeline-profile.json");
    save_via_dialog(&app, name, "JSON", "json", json.into_bytes()).await
}

/// Import a profile from a parsed envelope, checking the schema version.
/// Shared by file and URL import.
pub(super) fn import_profile_envelope(
    envelope: pipeline_config::ExportEnvelope,
) -> Result<ProfileSummary, String> {
    match envelope {
        pipeline_config::ExportEnvelope::Profile { schema_version, name, steps, merge, outputs, context_cache, use_orientation, orientation_prompt, orientation_schema, extraction, parallel_context_template, variables } => {
            if schema_version > pipeline_config::CURRENT_SCHEMA_VERSION {
                return Err(format!(
                    "This profile was made with a newer version of Pipeline (schema v{schema_version}). Update the app to import it."
                ));
            }
            pipeline_config::import_profile_data(&name, steps, merge, outputs, context_cache, use_orientation, orientation_prompt, orientation_schema, extraction, parallel_context_template, variables)
        }
        pipeline_config::ExportEnvelope::Step { .. } => {
            Err("This file contains a single step, not a profile. Use Import on the pipeline page to add it to the current profile.".into())
        }
        pipeline_config::ExportEnvelope::Bundle { .. } => {
            Err("This file is a full bundle. Use Import All to restore it.".into())
        }
    }
}

/// Fetch a profile JSON from a public HTTPS URL and import it. For sharing profiles
/// by link (a lab, a syllabus, a gist).
#[tauri::command]
pub async fn import_profile_from_url(url: String) -> Result<ProfileSummary, String> {
    let resp = fetch_public_profile_url(&url).await?;
    if !resp.status().is_success() {
        return Err(format!("Fetch failed: HTTP {}", resp.status()));
    }
    let bytes = read_response_limited(resp, MAX_IMPORT_SIZE as usize).await?;
    let content = String::from_utf8_lossy(&bytes).to_string();
    let envelope = pipeline_config::import_envelope(&content)
        .map_err(|e| format!("The URL did not contain a valid profile: {e}"))?;
    import_profile_envelope(envelope)
}

#[tauri::command]
pub async fn import_profile(path: String) -> Result<ProfileSummary, String> {
    let content = read_import_file(&path)?;
    let envelope = pipeline_config::import_envelope(&content)?;
    import_profile_envelope(envelope)
}

#[tauri::command]
pub async fn export_bundle(
    app: AppHandle,
    suggested_name: Option<String>,
) -> Result<Option<String>, String> {
    let json = pipeline_config::export_bundle()?;
    let name = suggested_name
        .as_deref()
        .unwrap_or("pipeline-settings-backup.json");
    save_via_dialog(&app, name, "JSON", "json", json.into_bytes()).await
}

#[tauri::command]
pub async fn import_bundle(path: String) -> Result<(), String> {
    let content = read_import_file(&path)?;
    pipeline_config::import_bundle(&content)
}

#[tauri::command]
pub async fn check_for_update() -> Result<crate::updates::UpdateInfo, String> {
    crate::updates::check().await
}

// --- Managed local engines ---

/// Status of every installable engine. The disk-usage walk can touch
/// multi-GB trees, so it runs off the async runtime.
#[tauri::command]
pub async fn list_engines() -> Result<Vec<crate::engines::EngineStatus>, String> {
    tokio::task::spawn_blocking(crate::engines::engine_statuses)
        .await
        .map_err(|e| format!("Engine status task failed: {e}"))
}

#[tauri::command]
pub async fn install_engine(app: AppHandle, engine_id: String) -> Result<(), String> {
    let bus = crate::emit::from_app(app);
    crate::engines::install_engine(&bus, &engine_id).await
}

#[tauri::command]
pub async fn uninstall_engine(app: AppHandle, engine_id: String) -> Result<(), String> {
    let bus = crate::emit::from_app(app);
    crate::engines::uninstall_engine(&bus, &engine_id).await
}

#[tauri::command]
pub fn cancel_engine_install() {
    crate::engines::cancel_install();
}
