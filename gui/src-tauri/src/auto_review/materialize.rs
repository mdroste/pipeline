//! Host-only workflow skeleton construction and selected-role materialization.
use super::assets::{CORE_CONSISTENCY, CORE_CONTRIBUTION, CORE_EXPOSITION, SYNTHESIS, VALIDATE};
use super::catalog::{GENRES, METHODS, RESEARCH_ARTICLE_GENRE, SUBJECTS};
use super::prompts::{method_prompt, subject_prompt, with_document_genre, with_routing_context};
use super::schemas::specialist_schema;
use super::selection::validate_review_plan;
use super::AUTO_REVIEW_CONTRACT;
use crate::pipeline_config::{
    ArtifactSelector, Phase, PipelineConfig, StepArtifactPart, StepConfig,
};

fn base_step(id: &str, label: &str, prompt: String, tools: &[&str]) -> StepConfig {
    StepConfig {
        id: id.to_string(),
        label: label.to_string(),
        prompt,
        enabled: true,
        phase: Phase::Parallel,
        tools: tools.iter().map(|tool| (*tool).to_string()).collect(),
        agents: Vec::new(),
        ..Default::default()
    }
}

/// The saved Full profile is deliberately only a stable five-step skeleton.
/// Selected specialist steps are materialized from the allowlist after the
/// orientation call, so a profile never contains the whole catalog.
pub fn steps() -> Vec<StepConfig> {
    let mut steps = vec![
        base_step(
            "auto_contribution",
            "Contribution & Literature",
            CORE_CONTRIBUTION.to_string(),
            &["WebSearch"],
        ),
        base_step(
            "auto_consistency",
            "Claims & Consistency",
            CORE_CONSISTENCY.to_string(),
            &["WebSearch"],
        ),
        base_step(
            "auto_exposition",
            "Exposition & Architecture",
            CORE_EXPOSITION.to_string(),
            &["WebSearch"],
        ),
    ];
    for step in &mut steps {
        step.output_schema = Some(specialist_schema());
    }
    // Both terminal steps reference the live canonical findings contract
    // (`structured::SCHEMA_REFERENCE_KEY`), so saved profiles stay compact and
    // pick up host contract improvements without a migration.
    let mut synthesis = base_step(
        "auto_synthesis",
        "Consolidate Feedback",
        SYNTHESIS.to_string(),
        &["WebSearch"],
    );
    synthesis.phase = Phase::Sequential;
    synthesis.output_schema = Some(serde_json::json!({
        "type": "object",
        crate::pipeline::structured::SCHEMA_REFERENCE_KEY: "findings-v2",
        crate::pipeline::structured::FINDINGS_TAXONOMY_KEY: crate::findings::CATEGORIES,
    }));
    steps.push(synthesis);
    let mut validate = base_step(
        "auto_validate",
        "Validate Feedback",
        VALIDATE.to_string(),
        &["WebSearch"],
    );
    validate.phase = Phase::Sequential;
    // Validation filters and repairs the consolidated product; the host
    // enforces that every surviving finding keeps its exact input id and
    // relative order (see `structured::PRESERVE_FINDINGS_KEY`).
    validate.output_schema = Some(serde_json::json!({
        "type": "object",
        crate::pipeline::structured::SCHEMA_REFERENCE_KEY: "findings-v2-validation",
        crate::pipeline::structured::FINDINGS_TAXONOMY_KEY: crate::findings::CATEGORIES,
        crate::pipeline::structured::PRESERVE_FINDINGS_KEY: "auto_synthesis",
    }));
    steps.push(validate);
    steps
}

/// Quick keeps the Full workflow's consistency, exposition, consolidation,
/// and validation passes while omitting the contribution/literature pass.
pub fn quick_steps() -> Vec<StepConfig> {
    steps()
        .into_iter()
        .filter(|step| step.id != "auto_contribution")
        .collect()
}

fn current_specialist_step(id: &str) -> Option<StepConfig> {
    let mut step = if let Some(specialist) = SUBJECTS.iter().find(|specialist| specialist.id == id)
    {
        base_step(
            specialist.id,
            specialist.label,
            subject_prompt(specialist),
            &["WebSearch"],
        )
    } else {
        let specialist = METHODS.iter().find(|specialist| specialist.id == id)?;
        base_step(
            specialist.id,
            specialist.label,
            method_prompt(specialist),
            &["WebSearch"],
        )
    };
    step.output_schema = Some(specialist_schema());
    Some(step)
}

/// Return an owned copy of a current adaptive-review specialist for manual
/// insertion in an editable workflow. The caller receives no reference back
/// to the host-owned catalog, so later edits cannot mutate suite defaults.
pub fn copyable_specialist_step(id: &str) -> Option<StepConfig> {
    current_specialist_step(id)
}

fn specialist_step(id: &str) -> Option<StepConfig> {
    current_specialist_step(id)
}

pub(crate) fn uses_auto_review_contract(config: &PipelineConfig) -> bool {
    config
        .orientation_schema
        .as_ref()
        .and_then(|schema| schema.get("x-pipeline-contract"))
        .and_then(serde_json::Value::as_str)
        == Some(AUTO_REVIEW_CONTRACT)
}

/// Cheap structural preflight for auto-contract configs, run before the
/// orientation call: materialization needs one enabled Parallel core step as
/// the artifact/agent donor for specialists, and discovering that only after
/// a full orientation call wastes the call.
pub fn validate_auto_review_preflight(config: &PipelineConfig) -> Result<(), String> {
    if !uses_auto_review_contract(config) {
        return Ok(());
    }
    let has_parallel_donor = config
        .steps
        .iter()
        .any(|step| step.enabled && step.phase == Phase::Parallel);
    if has_parallel_donor {
        Ok(())
    } else {
        Err(
            "Auto Review needs at least one enabled parallel core step (it defines the \
             specialists' artifact access). Enable one of the core reviews in the workflow \
             editor before running."
                .to_string(),
        )
    }
}

/// Expand an Automatic Paper Review skeleton into one run-specific workflow
/// after the combined orientation/classification call has returned. The model
/// supplies IDs only; every executable property comes from the host-owned
/// catalog.
pub fn materialize_config(
    config: &PipelineConfig,
    orientation: &serde_json::Value,
) -> Result<PipelineConfig, String> {
    if !uses_auto_review_contract(config) {
        return Ok(config.clone());
    }
    let plan = orientation
        .get("review_plan")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "Auto-review orientation is missing review_plan".to_string())?;
    // The plan itself must be valid, but the configured exact agent count is
    // an orientation-time constraint. A saved plan remains resumable after
    // the user changes that setting.
    validate_review_plan(orientation)?;
    let selected_ids = plan
        .get("subject_specialist_ids")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .chain(
            plan.get("method_specialist_ids")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten(),
        )
        .filter_map(serde_json::Value::as_str)
        .collect::<Vec<_>>();
    // The validated per-selection reasons are foregrounded in each
    // materialized prompt so the specialist starts from the claim that
    // triggered its selection.
    let selection_reasons = plan
        .get("selection_notes")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|note| {
            Some((
                note.get("id").and_then(serde_json::Value::as_str)?,
                note.get("reason").and_then(serde_json::Value::as_str)?,
            ))
        })
        .collect::<std::collections::HashMap<_, _>>();

    let mut materialized = config.clone();
    let existing = materialized
        .steps
        .iter()
        .map(|step| step.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    if let Some(conflict) = selected_ids.iter().find(|id| existing.contains(*id)) {
        return Err(format!(
            "Auto-review specialist '{}' conflicts with a saved workflow step",
            conflict
        ));
    }

    // Pipeline checks are not reviewers: specialists copy a model step's
    // artifact access and agents.
    let donor = materialized
        .steps
        .iter()
        .find(|step| step.enabled && step.phase == Phase::Parallel && step.kind.is_model())
        .cloned()
        .ok_or_else(|| {
            "Auto Review needs at least one enabled parallel core step to define specialist artifact access"
                .to_string()
        })?;
    let core_ids = materialized
        .steps
        .iter()
        .filter(|step| step.enabled && step.phase == Phase::Parallel && step.kind.is_model())
        .map(|step| step.id.clone())
        .collect::<Vec<_>>();
    let mut selected_steps = Vec::with_capacity(selected_ids.len());
    for id in &selected_ids {
        let mut step = specialist_step(id)
            .ok_or_else(|| format!("Auto-review selected unknown specialist '{id}'"))?;
        if let Some(reason) = selection_reasons.get(*id) {
            step.prompt = with_routing_context(&step.prompt, reason);
        }
        step.context = donor.context.clone();
        // Specialists mirror the core reviewers' agent/model configuration, so
        // an explicit per-step multi-agent selection in the saved profile (or
        // the one-run Parallel override, which rewrites every Parallel step)
        // reaches the adaptive steps too. Empty lists still materialize from
        // the role-level Settings defaults afterwards.
        step.agents = donor.agents.clone();
        step.model = donor.model.clone();
        step.effort = donor.effort.clone();
        step.model_overrides = donor.model_overrides.clone();
        step.effort_overrides = donor.effort_overrides.clone();
        selected_steps.push(step);
    }

    let insertion = materialized
        .steps
        .iter()
        .position(|step| step.phase == Phase::Sequential)
        .unwrap_or(materialized.steps.len());
    materialized
        .steps
        .splice(insertion..insertion, selected_steps);

    let synthesis = materialized
        .steps
        .iter_mut()
        .find(|step| step.id == "auto_synthesis")
        .ok_or_else(|| "Auto Review needs the 'auto_synthesis' step".to_string())?;
    synthesis.dependency_policy.required = core_ids;
    synthesis.dependency_policy.quorum = selected_ids.iter().map(|id| (*id).to_string()).collect();
    synthesis.dependency_policy.minimum_successes =
        std::cmp::min(2, synthesis.dependency_policy.quorum.len()) as u32;
    for id in &selected_ids {
        synthesis.context.include.push(ArtifactSelector::Step {
            step: (*id).to_string(),
            parts: vec![StepArtifactPart::Report],
            glob: String::new(),
        });
    }
    let validate = materialized
        .steps
        .iter_mut()
        .find(|step| step.id == "auto_validate")
        .ok_or_else(|| "Auto Review needs the 'auto_validate' step".to_string())?;
    validate.dependency_policy.required = vec!["auto_synthesis".to_string()];

    // A validated non-article genre classification becomes shared context:
    // the host-owned genre paragraph is injected into every enabled step —
    // core reviewers, specialists, consolidation, and validation alike — so
    // the whole panel judges the manuscript as what it claims to be. Plans
    if let Some(genre_id) = plan
        .get("genre")
        .and_then(serde_json::Value::as_str)
        .filter(|id| *id != RESEARCH_ARTICLE_GENRE)
    {
        let genre = GENRES
            .iter()
            .find(|genre| genre.id == genre_id)
            .ok_or_else(|| format!("Auto-review orientation names unknown genre '{genre_id}'"))?;
        for step in materialized.steps.iter_mut().filter(|step| step.enabled) {
            step.prompt = with_document_genre(&step.prompt, genre);
        }
    }
    Ok(materialized)
}
