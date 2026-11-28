//! Review setup, execution previews, and editor defaults.

use super::run_context::{
    bind_author_note, bind_foreground_launch, bind_model_overrides, bind_runtime_snapshot,
    bind_style_examples, check_snapshot_dependencies, load_run_snapshot,
    resolved_input_interpretation, validate_named_input_paths, validate_primary_input_selection,
    RunModelOverrides,
};
use crate::pipeline::executor;
use crate::pipeline_config;

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
    /// Whether New review offers Match my writing: the workflow has editors
    /// that use style examples, and About you has some to send.
    pub match_writing_available: bool,
}

/// Return the inexpensive profile metadata needed to render New Run. This
/// deliberately skips path validation, dependency probes, runtime binding, and
/// scheduler planning; `get_execution_plan` performs those checks once the user
/// asks to review the report.
#[tauri::command]
pub async fn get_run_setup() -> Result<RunSetupResponse, String> {
    let snapshot = load_run_snapshot()?;
    let match_writing_available = bind_style_examples(snapshot.clone(), true)?
        .style_examples
        .is_some();
    Ok(RunSetupResponse {
        profile_id: snapshot.settings.active_profile.clone(),
        profile_config_snapshot_id: snapshot.config_fingerprint,
        input_mode: match snapshot.config.extraction.input_mode.trim() {
            "" => "document".to_string(),
            mode => mode.to_string(),
        },
        variables: snapshot.config.variables,
        input_slots: snapshot.config.extraction.extra_inputs,
        match_writing_available,
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
    /// Every provider this run may call, including PDF extraction, comparison
    /// with the previous report, and the usage-limit backup.
    pub providers: Vec<String>,
    pub orientation_agent: String,
    pub parallel_agents: Vec<String>,
    pub sequential_agent: String,
    pub merge_agent: Option<String>,
    /// The usage-limit backup, when it couldn't run right now.
    pub backup_unavailable: Option<String>,
    /// Rough preflight range of calls, tokens, and API list-price cost, with
    /// the Review limits that will apply.
    pub usage_estimate: executor::budget::UsageEstimate,
    /// Every model the run will call, and whether each follows the review
    /// model, the default model, a Settings exception, or a workflow pin.
    pub model_roster: Vec<super::model_roster::RosterEntry>,
    /// Analyzer steps, with what each reads and any service it contacts.
    pub host_checks: Vec<crate::pipeline::analyzers::registry::HostCheck>,
    /// Files in the selected folders that won't fit in the Review's copy.
    pub input_notices: Vec<String>,
    /// Whether Match my writing can apply: the workflow has editors that use
    /// style examples and About you has style notes or examples to send.
    pub match_writing_available: bool,
}

/// Return the canonical scheduler timeline for one immutable active-profile
/// snapshot. The frontend must not independently infer dependency waves.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn get_execution_plan(
    variables: Option<std::collections::HashMap<String, String>>,
    extra_inputs: Option<std::collections::HashMap<String, String>>,
    expected_profile_config_snapshot_id: Option<String>,
    diff: Option<bool>,
    paper_path: Option<String>,
    input_interpretation: Option<String>,
    run_parallel_overrides: Option<RunModelOverrides>,
    author_note: Option<String>,
    match_writing: Option<bool>,
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
    let raw_config = snapshot.config.clone();
    let snapshot = bind_model_overrides(snapshot, run_parallel_overrides.as_ref())?;
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
    let backup_unavailable = crate::deps::unavailable_backup(&snapshot.settings, &readiness);
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
    let snapshot = bind_author_note(snapshot, author_note.as_deref())?;
    let snapshot = bind_style_examples(snapshot, match_writing.unwrap_or(false))?;
    let match_writing_available = snapshot.style_examples.is_some()
        || bind_style_examples(snapshot.clone(), true)?
            .style_examples
            .is_some();
    let snapshot = bind_foreground_launch(
        snapshot,
        paper_path.as_deref().unwrap_or(""),
        input_interpretation.as_deref(),
        diff,
    )?;
    let mut planning_config = snapshot.config.clone();
    pipeline_config::apply_agent_defaults(&mut planning_config, &snapshot.settings);
    crate::pipeline::style_examples::apply(
        &mut planning_config,
        snapshot.style_examples.as_deref(),
    );
    // Show a design the run would refuse here, before anything is billed.
    if !crate::auto_review::uses_auto_review_contract(&planning_config) {
        crate::pipeline_config::validate_runtime_config(&planning_config)?;
    }
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
    let usage_estimate = executor::budget::preview_estimate(
        &planning_config,
        &snapshot.settings,
        paper_path.as_deref(),
        snapshot.author_note.as_deref(),
    )
    .await;
    let input_notices = super::reuse_artifacts::folder_capture_notices(
        &planning_config,
        &snapshot.settings,
        paper_path.as_deref(),
        input_mode == "folder",
        &extra_inputs,
    )
    .await;
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
        providers: crate::deps::run_providers(
            &snapshot.settings,
            &planning_config,
            diff,
            paper_path.as_deref(),
            Some(&extra_inputs),
        ),
        orientation_agent: snapshot.settings.orientation_agent().to_string(),
        parallel_agents: snapshot.settings.parallel_agents(),
        sequential_agent: snapshot.settings.sequential_agent().to_string(),
        merge_agent,
        backup_unavailable,
        usage_estimate,
        model_roster: super::model_roster::model_roster(
            &snapshot.settings,
            &planning_config,
            &raw_config,
            run_parallel_overrides
                .as_ref()
                .is_some_and(|overrides| overrides.review.is_some()),
            run_parallel_overrides
                .as_ref()
                .is_some_and(|overrides| !overrides.agents.is_empty()),
            diff,
        ),
        host_checks: crate::pipeline::analyzers::registry::host_checks(&planning_config),
        input_notices,
        match_writing_available,
    })
}

/// The models a review would call with these one-report choices, without the
/// dependency probes `get_execution_plan` runs. New review shows this list
/// while the researcher changes the review model.
#[tauri::command]
pub async fn get_model_roster(
    run_parallel_overrides: Option<RunModelOverrides>,
    diff: Option<bool>,
) -> Result<Vec<super::model_roster::RosterEntry>, String> {
    let snapshot = load_run_snapshot()?;
    let raw_config = snapshot.config.clone();
    let snapshot = bind_model_overrides(snapshot, run_parallel_overrides.as_ref())?;
    let mut config = snapshot.config.clone();
    pipeline_config::apply_agent_defaults(&mut config, &snapshot.settings);
    Ok(super::model_roster::model_roster(
        &snapshot.settings,
        &config,
        &raw_config,
        run_parallel_overrides
            .as_ref()
            .is_some_and(|overrides| overrides.review.is_some()),
        run_parallel_overrides
            .as_ref()
            .is_some_and(|overrides| !overrides.agents.is_empty()),
        diff.unwrap_or(false),
    ))
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

/// Current-catalog router prompt and its matching orientation schema.
#[derive(serde::Serialize)]
pub struct AutoReviewOrientationDefaults {
    pub prompt: String,
    pub schema: serde_json::Value,
}

/// The router template and its compact catalog-backed orientation contract.
/// The editor restores both together; catalog rows are injected only at run
/// time and never stored in the profile prompt. A built-in adaptive profile
/// gets its own stock pair; any other or omitted profile gets Paper Review's.
#[tauri::command]
pub async fn get_auto_review_orientation_defaults(
    profile_id: Option<String>,
) -> Result<AutoReviewOrientationDefaults, String> {
    let (prompt, schema) =
        pipeline_config::adaptive_orientation_defaults(profile_id.as_deref().unwrap_or_default());
    Ok(AutoReviewOrientationDefaults { prompt, schema })
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
