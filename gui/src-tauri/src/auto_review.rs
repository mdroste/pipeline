//! Curated reviewer-role catalog for the built-in Paper Review (Auto) profile.
//!
//! The orientation model may select only stable IDs from this catalog. It
//! never creates executable steps, prompts, tools, providers, or model policy;
//! the host materializes only the selected `StepConfig`s from these static
//! entries and the existing executor handles the run.

mod legacy;
mod methods;
mod subjects;

use crate::pipeline_config::{
    ArtifactSelector, Phase, PipelineConfig, StepArtifactPart, StepConfig,
};

const ORIENTATION_TEMPLATE: &str = include_str!("../../../prompts/auto_review/orientation.md");
const SUBJECT_REVIEW_BASE: &str =
    include_str!("../../../prompts/auto_review/subjects/_review_contract.md");
const METHOD_REVIEW_BASE: &str =
    include_str!("../../../prompts/auto_review/methods/_review_contract.md");
const CORE_CONTRIBUTION: &str = include_str!("../../../prompts/auto_review/core/contribution.md");
const CORE_CONSISTENCY: &str = include_str!("../../../prompts/auto_review/core/consistency.md");
const CORE_EXPOSITION: &str = include_str!("../../../prompts/auto_review/core/exposition.md");
const SYNTHESIS: &str = include_str!("../../../prompts/auto_review/synthesis.md");

pub const AUTO_REVIEW_CONTRACT: &str = "auto-review-v2";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectLevel {
    Discipline,
    Subfield,
}

#[derive(Debug, Clone, Copy)]
pub struct SubjectSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub discipline_id: &'static str,
    pub discipline_label: &'static str,
    pub level: SubjectLevel,
    pub routing_description: &'static str,
    pub routing_exclusions: &'static str,
    pub discipline_prompt: &'static str,
    pub review_focus: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct MethodSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub routing_description: &'static str,
    pub routing_exclusions: &'static str,
    pub prompt: &'static str,
}

pub use methods::METHODS;
pub use subjects::SUBJECTS;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogRole {
    pub id: &'static str,
    pub label: &'static str,
    pub level: &'static str,
    pub description: &'static str,
    pub exclusions: &'static str,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogDiscipline {
    pub id: &'static str,
    pub label: &'static str,
    pub roles: Vec<CatalogRole>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoReviewCatalog {
    pub contract: &'static str,
    pub subject_count: usize,
    pub method_count: usize,
    pub disciplines: Vec<CatalogDiscipline>,
    pub methods: Vec<CatalogRole>,
}

/// Read-only metadata for explaining the router in the UI. This is generated
/// from the same static entries used by the classifier and materializer.
pub fn catalog() -> AutoReviewCatalog {
    let mut disciplines = Vec::<CatalogDiscipline>::new();
    for subject in SUBJECTS {
        if disciplines
            .last()
            .is_none_or(|discipline| discipline.id != subject.discipline_id)
        {
            disciplines.push(CatalogDiscipline {
                id: subject.discipline_id,
                label: subject.discipline_label,
                roles: Vec::new(),
            });
        }
        disciplines.last_mut().unwrap().roles.push(CatalogRole {
            id: subject.id,
            label: subject.label,
            level: match subject.level {
                SubjectLevel::Discipline => "discipline",
                SubjectLevel::Subfield => "subfield",
            },
            description: subject.routing_description,
            exclusions: subject.routing_exclusions,
        });
    }
    let methods = METHODS
        .iter()
        .map(|method| CatalogRole {
            id: method.id,
            label: method.label,
            level: "method",
            description: method.routing_description,
            exclusions: method.routing_exclusions,
        })
        .collect::<Vec<_>>();
    AutoReviewCatalog {
        contract: AUTO_REVIEW_CONTRACT,
        subject_count: SUBJECTS.len(),
        method_count: METHODS.len(),
        disciplines,
        methods,
    }
}

pub fn specialist_count() -> usize {
    SUBJECTS.len() + METHODS.len()
}

pub fn label_for(id: &str) -> Option<&'static str> {
    SUBJECTS
        .iter()
        .find(|specialist| specialist.id == id)
        .map(|specialist| specialist.label)
        .or_else(|| {
            METHODS
                .iter()
                .find(|specialist| specialist.id == id)
                .map(|specialist| specialist.label)
        })
        .or_else(|| legacy::label_for(id))
}

fn subject_prompt(specialist: &SubjectSpec) -> String {
    format!(
        "# {}\n\n## Discipline Lens\n\n{}\n\n## Specialist Focus\n\n{}\n\n## Scope Boundary\n\n{}\n\n{}",
        specialist.label,
        specialist.discipline_prompt.trim(),
        specialist.review_focus.trim(),
        specialist.routing_exclusions.trim(),
        SUBJECT_REVIEW_BASE.trim(),
    )
}

fn method_prompt(specialist: &MethodSpec) -> String {
    let raw = specialist.prompt.trim();
    let focus = raw
        .strip_prefix("# ")
        .and_then(|without_marker| without_marker.split_once("\n\n"))
        .map_or(raw, |(_, body)| body.trim());
    format!(
        "# {}\n\n## Specialist Focus\n\n{}\n\n{}",
        specialist.label,
        focus,
        METHOD_REVIEW_BASE.trim(),
    )
}

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

/// The saved Auto profile is deliberately only a stable four-step skeleton.
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
            &[],
        ),
        base_step(
            "auto_exposition",
            "Exposition & Architecture",
            CORE_EXPOSITION.to_string(),
            &[],
        ),
    ];
    let mut synthesis = base_step(
        "auto_synthesis",
        "Consolidate Auto Review",
        SYNTHESIS.to_string(),
        &[],
    );
    synthesis.phase = Phase::Sequential;
    steps.push(synthesis);
    steps
}

fn current_specialist_step(id: &str) -> Option<StepConfig> {
    if let Some(specialist) = SUBJECTS.iter().find(|specialist| specialist.id == id) {
        return Some(base_step(
            specialist.id,
            specialist.label,
            subject_prompt(specialist),
            &[],
        ));
    }
    METHODS
        .iter()
        .find(|specialist| specialist.id == id)
        .map(|specialist| {
            base_step(
                specialist.id,
                specialist.label,
                method_prompt(specialist),
                &[],
            )
        })
}

/// Return an owned copy of a current adaptive-review specialist for manual
/// insertion in an editable workflow. The caller receives no reference back
/// to the host-owned catalog, so later edits cannot mutate suite defaults.
pub fn copyable_specialist_step(id: &str) -> Option<StepConfig> {
    current_specialist_step(id)
}

fn specialist_step(id: &str) -> Option<StepConfig> {
    current_specialist_step(id).or_else(|| legacy::specialist_step(id))
}

pub(crate) fn uses_auto_review_contract(config: &PipelineConfig) -> bool {
    config
        .orientation_schema
        .as_ref()
        .and_then(|schema| schema.get("x-pipeline-contract"))
        .and_then(serde_json::Value::as_str)
        == Some(AUTO_REVIEW_CONTRACT)
}

/// Expand the four-step Auto profile into one run-specific workflow after the
/// combined orientation/classification call has returned. The model supplies IDs only; every executable
/// property comes from the host-owned catalog.
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
    let selected_ids = if plan.get("subject_specialist_ids").is_some() {
        validate_review_plan(orientation)?;
        plan.get("subject_specialist_ids")
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
            .collect::<Vec<_>>()
    } else {
        // Saved Auto v1 reports can still be rerun after the stock profile is
        // migrated. Keep their already-validated field/method plan intact.
        legacy::selected_ids(orientation)?
    };

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

    let context = materialized
        .steps
        .iter()
        .find(|step| step.enabled && step.phase == Phase::Parallel)
        .map(|step| step.context.clone())
        .ok_or_else(|| {
            "Auto Review needs at least one enabled parallel core step to define specialist artifact access"
                .to_string()
        })?;
    let mut selected_steps = Vec::with_capacity(selected_ids.len());
    for id in &selected_ids {
        let mut step = specialist_step(id)
            .ok_or_else(|| format!("Auto-review selected unknown specialist '{id}'"))?;
        step.context = context.clone();
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
    for id in selected_ids {
        synthesis.context.include.push(ArtifactSelector::Step {
            step: id.to_string(),
            parts: vec![StepArtifactPart::Report],
            glob: String::new(),
        });
    }
    Ok(materialized)
}

/// Orientation prompt with both catalogs injected from the same source of
/// truth used to build the run-specific specialist steps.
pub fn orientation_prompt() -> String {
    let mut subject_catalog = String::new();
    let mut current_discipline = "";
    for specialist in SUBJECTS {
        if current_discipline != specialist.discipline_id {
            if !subject_catalog.is_empty() {
                subject_catalog.push('\n');
            }
            subject_catalog.push_str(&format!("### {}\n", specialist.discipline_label));
            current_discipline = specialist.discipline_id;
        }
        subject_catalog.push_str(&format!(
            "- `{}` — {} Exclude when: {}\n",
            specialist.id, specialist.routing_description, specialist.routing_exclusions
        ));
    }
    let method_catalog = METHODS
        .iter()
        .map(|specialist| {
            format!(
                "- `{}` — {} Exclude when: {}",
                specialist.id, specialist.routing_description, specialist.routing_exclusions
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    ORIENTATION_TEMPLATE
        .replace("{subject_catalog}", subject_catalog.trim())
        .replace("{method_catalog}", &method_catalog)
}

/// Schema for the auto-review router. `subject_specialist_ids` is ordered: the
/// first element is primary and the optional second element is secondary.
pub fn orientation_schema() -> serde_json::Value {
    let subject_ids = SUBJECTS
        .iter()
        .map(|specialist| serde_json::Value::String(specialist.id.to_string()))
        .collect::<Vec<_>>();
    let method_ids = METHODS
        .iter()
        .map(|specialist| serde_json::Value::String(specialist.id.to_string()))
        .collect::<Vec<_>>();
    let all_ids = subject_ids
        .iter()
        .chain(method_ids.iter())
        .cloned()
        .collect::<Vec<_>>();
    serde_json::json!({
        "x-pipeline-contract": AUTO_REVIEW_CONTRACT,
        "type": "object",
        "required": [
            "metadata", "review_plan", "sections", "formal_results",
            "tables_figures", "notation", "stated_contribution",
            "key_references", "extraction_quality_notes"
        ],
        "properties": {
            "metadata": {
                "type": "object",
                "required": ["title", "authors", "paper_type", "has_appendix", "has_online_appendix"],
                "properties": {
                    "title": {"type": "string"},
                    "authors": {"type": "array", "items": {"type": "string"}},
                    "paper_type": {"type": "string", "enum": ["theory", "empirical", "mixed"]},
                    "has_appendix": {"type": "boolean"},
                    "has_online_appendix": {"type": "boolean"}
                }
            },
            "review_plan": {
                "type": "object",
                "required": [
                    "primary_domain", "subject", "paper_forms", "methods",
                    "subject_specialist_ids", "method_specialist_ids",
                    "selection_notes", "routing_uncertainty"
                ],
                "properties": {
                    "primary_domain": {"type": "string"},
                    "subject": {"type": "string"},
                    "paper_forms": {
                        "type": "array", "minItems": 1, "uniqueItems": true,
                        "items": {"type": "string"}
                    },
                    "methods": {
                        "type": "array", "minItems": 1, "uniqueItems": true,
                        "items": {"type": "string"}
                    },
                    "subject_specialist_ids": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 2,
                        "uniqueItems": true,
                        "items": {"type": "string", "enum": subject_ids}
                    },
                    "method_specialist_ids": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 4,
                        "uniqueItems": true,
                        "items": {"type": "string", "enum": method_ids}
                    },
                    "selection_notes": {
                        "type": "array",
                        "minItems": 2,
                        "maxItems": 6,
                        "items": {
                            "type": "object",
                            "required": ["id", "reason"],
                            "properties": {
                                "id": {"type": "string", "enum": all_ids},
                                "reason": {"type": "string"}
                            }
                        }
                    },
                    "routing_uncertainty": {
                        "type": "array", "maxItems": 5,
                        "items": {"type": "string"}
                    }
                }
            },
            "sections": {"type": "array"},
            "formal_results": {"type": "array"},
            "tables_figures": {"type": "array"},
            "notation": {"type": "array"},
            "stated_contribution": {"type": "string"},
            "key_references": {"type": "array", "items": {"type": "string"}},
            "extraction_quality_notes": {"type": "array"}
        }
    })
}

/// Apply the small semantic checks that ordinary JSON shape validation cannot
/// express. The schema marker makes the extension explicit and profile-local.
pub fn validate_contract_for_schema(
    schema: &serde_json::Value,
    orientation: &serde_json::Value,
) -> Result<(), String> {
    if schema
        .get("x-pipeline-contract")
        .and_then(serde_json::Value::as_str)
        != Some(AUTO_REVIEW_CONTRACT)
    {
        return Ok(());
    }
    validate_review_plan(orientation)
}

pub fn validate_review_plan(orientation: &serde_json::Value) -> Result<(), String> {
    let plan = orientation
        .get("review_plan")
        .ok_or_else(|| "$.review_plan: expected an object".to_string())?;
    validate_review_plan_object(plan)
}

pub fn validate_review_plan_object(plan: &serde_json::Value) -> Result<(), String> {
    let plan = plan
        .as_object()
        .ok_or_else(|| "$: expected a review-plan object".to_string())?;
    let subject_ids = plan
        .get("subject_specialist_ids")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "$.review_plan.subject_specialist_ids: expected an array".to_string())?;
    let method_ids = plan
        .get("method_specialist_ids")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "$.review_plan.method_specialist_ids: expected an array".to_string())?;

    if !(1..=2).contains(&subject_ids.len()) {
        return Err(format!(
            "subject_specialist_ids must contain one or two IDs, got {}",
            subject_ids.len()
        ));
    }
    if !(1..=4).contains(&method_ids.len()) {
        return Err(format!(
            "method_specialist_ids must contain one to four IDs, got {}",
            method_ids.len()
        ));
    }

    let mut selected = std::collections::HashSet::new();
    for value in subject_ids {
        let id = value
            .as_str()
            .ok_or_else(|| "subject specialist ID must be a string".to_string())?;
        if !SUBJECTS.iter().any(|specialist| specialist.id == id) {
            return Err(format!("unknown subject specialist '{id}'"));
        }
        if !selected.insert(id) {
            return Err(format!(
                "subject specialist '{id}' is selected more than once"
            ));
        }
    }
    if subject_ids.len() == 2 {
        let selected_subjects = subject_ids
            .iter()
            .filter_map(serde_json::Value::as_str)
            .filter_map(|id| SUBJECTS.iter().find(|specialist| specialist.id == id))
            .collect::<Vec<_>>();
        if selected_subjects.len() == 2
            && selected_subjects[0].discipline_id == selected_subjects[1].discipline_id
            && selected_subjects
                .iter()
                .any(|specialist| specialist.level == SubjectLevel::Discipline)
        {
            return Err(
                "do not select a discipline fallback together with one of its subfields"
                    .to_string(),
            );
        }
    }
    for value in method_ids {
        let id = value
            .as_str()
            .ok_or_else(|| "method specialist ID must be a string".to_string())?;
        if !METHODS.iter().any(|specialist| specialist.id == id) {
            return Err(format!("unknown method specialist '{id}'"));
        }
        if !selected.insert(id) {
            return Err(format!("specialist '{id}' is selected more than once"));
        }
    }

    let notes = plan
        .get("selection_notes")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "$.review_plan.selection_notes: expected an array".to_string())?;
    let mut noted = std::collections::HashSet::new();
    for note in notes {
        let note = note
            .as_object()
            .ok_or_else(|| "selection note must be an object".to_string())?;
        let id = note
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "selection note ID must be a string".to_string())?;
        let reason = note
            .get("reason")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("selection note for '{id}' needs a reason"))?;
        if reason.trim().is_empty() {
            return Err(format!("selection note for '{id}' has an empty reason"));
        }
        if !selected.contains(id) {
            return Err(format!("selection note names unselected specialist '{id}'"));
        }
        if !noted.insert(id) {
            return Err(format!("selection note for '{id}' appears more than once"));
        }
    }
    if noted != selected {
        let mut missing = selected.difference(&noted).copied().collect::<Vec<_>>();
        missing.sort_unstable();
        return Err(format!(
            "selection_notes must cover every selected specialist; missing {}",
            missing.join(", ")
        ));
    }
    Ok(())
}

pub(crate) fn legacy_steps() -> Vec<StepConfig> {
    legacy::steps()
}

pub(crate) fn legacy_orientation_prompt() -> String {
    legacy::orientation_prompt()
}

pub(crate) fn legacy_orientation_schema() -> serde_json::Value {
    legacy::orientation_schema()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured_auto_review() -> PipelineConfig {
        let mut config = PipelineConfig {
            steps: steps(),
            merge: Default::default(),
            context_cache: Default::default(),
            use_orientation: true,
            orientation_prompt: orientation_prompt(),
            orientation_schema: Some(orientation_schema()),
            extraction: Default::default(),
            parallel_context_template: String::new(),
            variables: Vec::new(),
        };
        for step in &mut config.steps {
            step.context.include = vec![ArtifactSelector::Survey];
        }
        let synthesis = config
            .steps
            .iter_mut()
            .find(|step| step.id == "auto_synthesis")
            .unwrap();
        synthesis.context.include.extend(
            ["auto_contribution", "auto_consistency", "auto_exposition"]
                .into_iter()
                .map(|step| ArtifactSelector::Step {
                    step: step.to_string(),
                    parts: vec![StepArtifactPart::Report],
                    glob: String::new(),
                }),
        );
        config
    }

    fn synthesis_report_inputs(config: &PipelineConfig) -> Vec<&str> {
        config
            .steps
            .iter()
            .find(|step| step.id == "auto_synthesis")
            .unwrap()
            .context
            .include
            .iter()
            .filter_map(|selector| match selector {
                ArtifactSelector::Step { step, parts, .. }
                    if parts == &[StepArtifactPart::Report] =>
                {
                    Some(step.as_str())
                }
                _ => None,
            })
            .collect()
    }

    fn valid_orientation() -> serde_json::Value {
        serde_json::json!({
            "metadata": {
                "title": "A paper", "authors": [], "paper_type": "theory",
                "has_appendix": true, "has_online_appendix": false
            },
            "review_plan": {
                "primary_domain": "mathematics",
                "subject": "partial differential equations",
                "paper_forms": ["formal_theory"],
                "methods": ["proof"],
                "subject_specialist_ids": ["subject_mathematics_pde"],
                "method_specialist_ids": ["formal_proofs"],
                "selection_notes": [
                    {"id": "subject_mathematics_pde", "reason": "The main result concerns a nonlinear PDE."},
                    {"id": "formal_proofs", "reason": "The theorem and proof carry the contribution."}
                ],
                "routing_uncertainty": []
            },
            "sections": [], "formal_results": [], "tables_figures": [],
            "notation": [], "stated_contribution": "A contribution.",
            "key_references": [], "extraction_quality_notes": []
        })
    }

    #[test]
    fn catalog_ids_prompts_and_fallbacks_are_complete() {
        assert_eq!(SUBJECTS.len(), 191);
        assert_eq!(METHODS.len(), 32);
        let mut ids = std::collections::HashSet::new();
        let mut disciplines = std::collections::HashMap::<&str, bool>::new();
        for specialist in SUBJECTS {
            assert!(ids.insert(specialist.id), "duplicate id {}", specialist.id);
            assert!(specialist.id.starts_with("subject_"));
            let prompt = subject_prompt(specialist);
            assert_specialist_prompt_contract(&prompt, specialist.label);
            let fallback = disciplines.entry(specialist.discipline_id).or_default();
            *fallback |= specialist.level == SubjectLevel::Discipline;
        }
        for specialist in METHODS {
            assert!(ids.insert(specialist.id), "duplicate id {}", specialist.id);
            assert!(specialist.prompt.len() > 200);
            let prompt = method_prompt(specialist);
            assert_specialist_prompt_contract(&prompt, specialist.label);
        }
        assert_eq!(disciplines.len(), 28);
        assert!(disciplines.values().all(|fallback| *fallback));

        let view = catalog();
        assert_eq!(view.subject_count, SUBJECTS.len());
        assert_eq!(view.method_count, METHODS.len());
        assert_eq!(view.disciplines.len(), 28);
        assert_eq!(
            view.disciplines
                .iter()
                .map(|discipline| discipline.roles.len())
                .sum::<usize>(),
            SUBJECTS.len()
        );
    }

    #[test]
    fn copyable_specialist_steps_are_owned_snapshots() {
        let id = "subject_economics_macro";
        let mut copied = copyable_specialist_step(id).unwrap();
        let default_prompt = copied.prompt.clone();

        copied.label = "My macro reviewer".to_string();
        copied.prompt.push_str("\n\nWorkflow-local addition.");

        let fresh = copyable_specialist_step(id).unwrap();
        assert_eq!(fresh.id, id);
        assert_eq!(fresh.label, "Economics — Macroeconomics");
        assert_eq!(fresh.prompt, default_prompt);
        assert_ne!(fresh.prompt, copied.prompt);
        assert!(copyable_specialist_step("not_in_the_catalog").is_none());
    }

    fn assert_specialist_prompt_contract(prompt: &str, label: &str) {
        assert!(prompt.starts_with(&format!("# {label}\n\n")), "{label}");
        assert_eq!(
            prompt.lines().filter(|line| line.starts_with("# ")).count(),
            1,
            "{label} has more than one top-level heading"
        );
        assert_report_output_fields(prompt, label);
        assert!(
            prompt.contains("Do not add operational instructions for harmful activity"),
            "{label} is missing the safety boundary"
        );
    }

    fn assert_report_output_fields(prompt: &str, label: &str) {
        for required in [
            "## Output",
            "**#1. Specific descriptive title**",
            "**Severity:**",
            "**In the paper:**",
            "**The problem:**",
            "**Consequence:**",
            "**What would help:**",
            "**Location:**",
            "Increment `N` sequentially from 1.",
            "No material issues identified.",
        ] {
            assert!(prompt.contains(required), "{label} is missing {required}");
        }
    }

    #[test]
    fn universal_reviewers_use_the_report_viewer_contract() {
        for (label, prompt) in [
            ("Contribution & Literature", CORE_CONTRIBUTION),
            ("Claims & Consistency", CORE_CONSISTENCY),
            ("Exposition & Architecture", CORE_EXPOSITION),
        ] {
            assert!(prompt.starts_with("# "), "{label}");
            assert_report_output_fields(prompt, label);
        }
        assert!(SYNTHESIS.contains("**#N. Descriptive title naming the specific issue**"));
        assert!(SYNTHESIS.contains("issue-navigation format used by the report viewer"));
    }

    #[test]
    fn generated_prompt_and_schema_cover_the_catalog() {
        let prompt = orientation_prompt();
        assert!(!prompt.contains("{subject_catalog}"));
        assert!(!prompt.contains("{method_catalog}"));
        for specialist in SUBJECTS {
            assert!(prompt.contains(&format!("`{}`", specialist.id)));
        }
        for specialist in METHODS {
            assert!(prompt.contains(&format!("`{}`", specialist.id)));
        }
        crate::pipeline::structured::validate_schema(&orientation_schema()).unwrap();
    }

    #[test]
    fn saved_profile_is_a_four_step_skeleton() {
        let steps = steps();
        assert_eq!(steps.len(), 4);
        assert!(steps.iter().all(|step| step.run_if.is_none()));
        assert_eq!(
            steps
                .iter()
                .filter(|step| step.phase == Phase::Sequential)
                .count(),
            1
        );
    }

    #[test]
    fn materialization_inserts_only_selected_specialists_before_synthesis() {
        let config = configured_auto_review();
        let materialized = materialize_config(&config, &valid_orientation()).unwrap();
        assert_eq!(materialized.steps.len(), 6);
        assert_eq!(
            materialized
                .steps
                .iter()
                .map(|step| step.id.as_str())
                .collect::<Vec<_>>(),
            [
                "auto_contribution",
                "auto_consistency",
                "auto_exposition",
                "subject_mathematics_pde",
                "formal_proofs",
                "auto_synthesis"
            ]
        );
        assert_eq!(
            synthesis_report_inputs(&materialized),
            [
                "auto_contribution",
                "auto_consistency",
                "auto_exposition",
                "subject_mathematics_pde",
                "formal_proofs",
            ]
        );
    }

    #[test]
    fn migrated_profile_can_rerun_a_saved_v1_orientation() {
        let mut orientation = valid_orientation();
        let plan = orientation["review_plan"].as_object_mut().unwrap();
        plan.remove("subject_specialist_ids");
        plan.insert(
            "field_specialist_id".to_string(),
            serde_json::json!("field_mathematics"),
        );
        plan.insert(
            "selection_notes".to_string(),
            serde_json::json!([
                {"id": "field_mathematics", "reason": "The paper is a mathematics contribution."},
                {"id": "formal_proofs", "reason": "The theorem and proof carry the contribution."}
            ]),
        );
        let mut config = PipelineConfig {
            steps: steps(),
            merge: Default::default(),
            context_cache: Default::default(),
            use_orientation: true,
            orientation_prompt: orientation_prompt(),
            orientation_schema: Some(orientation_schema()),
            extraction: Default::default(),
            parallel_context_template: String::new(),
            variables: Vec::new(),
        };
        for step in &mut config.steps {
            step.context.include = vec![ArtifactSelector::Survey];
        }
        let materialized = materialize_config(&config, &orientation).unwrap();
        assert_eq!(materialized.steps.len(), 6);
        assert!(materialized
            .steps
            .iter()
            .any(|step| step.id == "field_mathematics"));
    }

    #[test]
    fn migrated_profile_can_rerun_the_earliest_combined_specialist_plan() {
        let mut orientation = valid_orientation();
        let plan = orientation["review_plan"].as_object_mut().unwrap();
        plan.remove("subject_specialist_ids");
        plan.remove("method_specialist_ids");
        plan.insert(
            "specialist_ids".to_string(),
            serde_json::json!(["formal_proofs", "field_mathematics"]),
        );
        plan.insert(
            "selection_notes".to_string(),
            serde_json::json!([
                {"id": "formal_proofs", "reason": "The theorem and proof carry the contribution."},
                {"id": "field_mathematics", "reason": "The paper is a mathematics contribution."}
            ]),
        );
        let config = configured_auto_review();
        let materialized = materialize_config(&config, &orientation).unwrap();
        assert_eq!(materialized.steps.len(), 6);
        assert!(materialized
            .steps
            .iter()
            .any(|step| step.id == "formal_proofs"));
        assert!(materialized
            .steps
            .iter()
            .any(|step| step.id == "field_mathematics"));
        assert_eq!(
            synthesis_report_inputs(&materialized),
            [
                "auto_contribution",
                "auto_consistency",
                "auto_exposition",
                "formal_proofs",
                "field_mathematics",
            ]
        );
    }

    #[test]
    fn routing_schema_and_semantics_reject_invalid_plans() {
        let schema = orientation_schema();
        let valid = valid_orientation();
        crate::pipeline::structured::validate(&schema, &valid).unwrap();
        validate_contract_for_schema(&schema, &valid).unwrap();

        let mut wrong_category = valid.clone();
        wrong_category["review_plan"]["subject_specialist_ids"] =
            serde_json::json!(["formal_proofs"]);
        assert!(crate::pipeline::structured::validate(&schema, &wrong_category).is_err());

        let mut missing_note = valid.clone();
        missing_note["review_plan"]["selection_notes"] = serde_json::json!([
            {"id": "subject_mathematics_pde", "reason": "The paper studies PDEs."}
        ]);
        assert!(validate_contract_for_schema(&schema, &missing_note).is_err());

        let mut unselected_note = valid;
        unselected_note["review_plan"]["selection_notes"][1]["id"] =
            serde_json::json!("statistical_validity");
        assert!(validate_contract_for_schema(&schema, &unselected_note).is_err());

        let mut parent_and_child = valid_orientation();
        parent_and_child["review_plan"]["subject_specialist_ids"] =
            serde_json::json!(["subject_mathematics_general", "subject_mathematics_pde"]);
        parent_and_child["review_plan"]["selection_notes"] = serde_json::json!([
            {"id": "subject_mathematics_general", "reason": "Broad mathematics fallback."},
            {"id": "subject_mathematics_pde", "reason": "The main result concerns PDEs."},
            {"id": "formal_proofs", "reason": "Proofs carry the result."}
        ]);
        assert!(validate_contract_for_schema(&schema, &parent_and_child)
            .unwrap_err()
            .contains("discipline fallback"));
    }

    #[test]
    fn every_discipline_fallback_materializes_and_the_run_stays_under_ten_steps() {
        let config = configured_auto_review();

        for subject in SUBJECTS
            .iter()
            .filter(|subject| subject.level == SubjectLevel::Discipline)
        {
            let mut orientation = valid_orientation();
            orientation["review_plan"]["primary_domain"] =
                serde_json::json!(subject.discipline_label);
            orientation["review_plan"]["subject_specialist_ids"] = serde_json::json!([subject.id]);
            orientation["review_plan"]["selection_notes"] = serde_json::json!([
                {"id": subject.id, "reason": "The paper's central claim belongs to this discipline."},
                {"id": "formal_proofs", "reason": "Formal derivations carry the result."}
            ]);
            let materialized = materialize_config(&config, &orientation).unwrap();
            assert_eq!(materialized.steps.len(), 6, "{}", subject.id);
            assert!(materialized.steps.iter().any(|step| step.id == subject.id));
        }

        let mut maximum = valid_orientation();
        maximum["review_plan"]["subject_specialist_ids"] = serde_json::json!([
            "subject_physics_quantum_information",
            "subject_computer_science_algorithms"
        ]);
        maximum["review_plan"]["method_specialist_ids"] = serde_json::json!([
            "formal_proofs",
            "algorithmic_ml",
            "simulation_numerics",
            "reproducibility_software"
        ]);
        maximum["review_plan"]["selection_notes"] = serde_json::json!([
            {"id": "subject_physics_quantum_information", "reason": "The physical result is about quantum information."},
            {"id": "subject_computer_science_algorithms", "reason": "The computational result is an algorithm."},
            {"id": "formal_proofs", "reason": "Formal guarantees carry the claim."},
            {"id": "algorithmic_ml", "reason": "Algorithmic performance is central."},
            {"id": "simulation_numerics", "reason": "Simulations support the finite-size results."},
            {"id": "reproducibility_software", "reason": "A custom implementation produces the evidence."}
        ]);
        let materialized = materialize_config(&config, &maximum).unwrap();
        assert_eq!(materialized.steps.len(), 10);
        assert_eq!(
            synthesis_report_inputs(&materialized),
            [
                "auto_contribution",
                "auto_consistency",
                "auto_exposition",
                "subject_physics_quantum_information",
                "subject_computer_science_algorithms",
                "formal_proofs",
                "algorithmic_ml",
                "simulation_numerics",
                "reproducibility_software",
            ]
        );
    }
}
