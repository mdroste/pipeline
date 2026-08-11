//! Exact shape of the first, static Auto Review catalog.
//!
//! This module exists only to recognize and migrate an untouched development
//! profile. New runs never execute this catalog-wide conditional workflow.

use super::{base_step, CORE_CONSISTENCY, CORE_CONTRIBUTION, CORE_EXPOSITION, SYNTHESIS};
use crate::pipeline_config::{Phase, RunCondition, StepConfig};

const ORIENTATION_TEMPLATE: &str =
    include_str!("../../../../prompts/auto_review/orientation_v1.md");

#[derive(Clone, Copy)]
enum Kind {
    Method,
    Field,
}

#[derive(Clone, Copy)]
struct Spec {
    id: &'static str,
    label: &'static str,
    description: &'static str,
    prompt: &'static str,
    kind: Kind,
}

macro_rules! method {
    ($id:literal, $label:literal, $description:literal, $file:literal) => {
        Spec {
            id: $id,
            label: $label,
            description: $description,
            prompt: include_str!($file),
            kind: Kind::Method,
        }
    };
}

macro_rules! field {
    ($id:literal, $label:literal, $description:literal, $file:literal) => {
        Spec {
            id: $id,
            label: $label,
            description: $description,
            prompt: include_str!($file),
            kind: Kind::Field,
        }
    };
}

const SPECIALISTS: &[Spec] = &[
    method!("formal_proofs", "Formal Proofs", "Verify central theorems, propositions, lemmas, and nontrivial formal derivations.", "../../../../prompts/auto_review/methods/formal_proofs.md"),
    method!("economic_model_logic", "Economic Model Logic", "Review economic assumptions, equilibrium, incentives, mechanisms, comparative statics, incidence, and welfare.", "../../../../prompts/auto_review/methods/economic_model_logic.md"),
    method!("causal_identification", "Causal Identification", "Review observational or quasi-experimental variation used for material causal claims.", "../../../../prompts/auto_review/methods/causal_identification.md"),
    method!("randomized_experiment", "Randomized Experiment", "Review random assignment, encouragement designs, interventions, compliance, outcomes, and inference.", "../../../../prompts/auto_review/methods/randomized_experiment.md"),
    method!("structural_estimation", "Structural Estimation", "Review estimated structural parameters, identification, fit, and model-based counterfactuals.", "../../../../prompts/auto_review/methods/structural_estimation.md"),
    method!("quantitative_computation", "Quantitative Computation", "Review calibration, numerical solution, dynamic computation, and model counterfactuals.", "../../../../prompts/auto_review/methods/quantitative_computation.md"),
    method!("statistical_validity", "Statistical Validity", "Review statistical inference, estimation, uncertainty, prediction, and sampling claims.", "../../../../prompts/auto_review/methods/statistical_validity.md"),
    method!("measurement_data", "Measurement & Data", "Review data construction, measurement, sampling frames, linkage, coding, and descriptive facts.", "../../../../prompts/auto_review/methods/measurement_data.md"),
    method!("simulation_numerics", "Simulation & Numerics", "Review Monte Carlo evidence, numerical approximation, discretization, and simulation accuracy.", "../../../../prompts/auto_review/methods/simulation_numerics.md"),
    method!("algorithmic_ml", "Algorithms & Machine Learning", "Review algorithms, prediction systems, learning procedures, benchmarks, and computational complexity.", "../../../../prompts/auto_review/methods/algorithmic_ml.md"),
    method!("qualitative_case_study", "Qualitative & Case Evidence", "Review interviews, ethnography, process tracing, qualitative coding, and comparative cases.", "../../../../prompts/auto_review/methods/qualitative_case_study.md"),
    method!("conceptual_argument", "Conceptual Argument", "Review conceptual, synthetic, normative, interpretive, or argumentative reasoning.", "../../../../prompts/auto_review/methods/conceptual_argument.md"),
    field!("field_economics_macro", "Macroeconomics", "Macroeconomics, monetary economics, growth, business cycles, labor macro, and international macro.", "../../../../prompts/auto_review/fields/economics_macro.md"),
    field!("field_economics_micro_theory", "Microeconomic Theory", "Microeconomic theory, mechanism design, information economics, game theory, and price theory.", "../../../../prompts/auto_review/fields/economics_micro_theory.md"),
    field!("field_economics_applied_micro", "Applied Microeconomics", "Applied microeconomics including labor, public, development, health, education, and urban economics.", "../../../../prompts/auto_review/fields/economics_applied_micro.md"),
    field!("field_economics_io", "Industrial Organization", "Industrial organization, market design, competition, demand, supply, and firm behavior.", "../../../../prompts/auto_review/fields/economics_io.md"),
    field!("field_economics_finance", "Finance", "Asset pricing, corporate finance, banking, household finance, and financial intermediation.", "../../../../prompts/auto_review/fields/economics_finance.md"),
    field!("field_economics_econometrics", "Econometrics", "Econometric theory, identification, estimation, inference, and statistical methodology for economics.", "../../../../prompts/auto_review/fields/economics_econometrics.md"),
    field!("field_mathematics", "Mathematics", "Pure and applied mathematics, including definitions, theorem scope, proof strategy, and relation to benchmark results.", "../../../../prompts/auto_review/fields/mathematics.md"),
    field!("field_statistics_probability", "Statistics & Probability", "Statistics, probability, stochastic processes, inference, prediction, and uncertainty quantification.", "../../../../prompts/auto_review/fields/statistics_probability.md"),
    field!("field_computer_science", "Computer Science", "Algorithms, machine learning, systems, programming languages, security, HCI, and computational evaluation.", "../../../../prompts/auto_review/fields/computer_science.md"),
    field!("field_biomedical_science", "Biomedical Science", "Biology, medicine, epidemiology, clinical research, and biomedical mechanisms and measurement.", "../../../../prompts/auto_review/fields/biomedical_science.md"),
    field!("field_natural_science", "Natural Science", "Physics, chemistry, Earth science, astronomy, and other natural-science domains.", "../../../../prompts/auto_review/fields/natural_science.md"),
    field!("field_social_science", "Social Science", "Political science, sociology, psychology, education, anthropology, and interdisciplinary social science.", "../../../../prompts/auto_review/fields/social_science.md"),
    field!("field_general_academic", "General Academic", "Fallback for humanities, law, interdisciplinary work, or a subject not covered by another field role.", "../../../../prompts/auto_review/fields/general_academic.md"),
];

pub(super) fn label_for(id: &str) -> Option<&'static str> {
    SPECIALISTS
        .iter()
        .find(|specialist| specialist.id == id)
        .map(|specialist| specialist.label)
}

pub(super) fn specialist_step(id: &str) -> Option<StepConfig> {
    SPECIALISTS
        .iter()
        .find(|specialist| specialist.id == id)
        .map(|specialist| {
            base_step(
                specialist.id,
                specialist.label,
                specialist.prompt.to_string(),
                &[],
            )
        })
}

pub(super) fn selected_ids(orientation: &serde_json::Value) -> Result<Vec<&str>, String> {
    let plan = orientation
        .get("review_plan")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "legacy Auto Review plan is missing review_plan".to_string())?;
    let selected = if plan.get("specialist_ids").is_some() {
        crate::pipeline::structured::validate(
            &orientation_schema_with_combined_specialist_ids_without_conceptual_argument(),
            orientation,
        )
        .map_err(|error| format!("legacy Auto Review plan is invalid: {error}"))?;
        plan.get("specialist_ids")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>()
    } else {
        crate::pipeline::structured::validate(&orientation_schema(), orientation)
            .map_err(|error| format!("legacy Auto Review plan is invalid: {error}"))?;
        let field = plan
            .get("field_specialist_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "legacy Auto Review plan is missing field_specialist_id".to_string())?;
        let mut selected = vec![field];
        selected.extend(
            plan.get("method_specialist_ids")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str),
        );
        selected
    };
    let selected_set = selected
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let mut noted = std::collections::HashSet::new();
    for note in plan
        .get("selection_notes")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let id = note
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "legacy Auto Review selection note needs an ID".to_string())?;
        let reason = note
            .get("reason")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                format!("legacy Auto Review selection note for '{id}' needs a reason")
            })?;
        if !selected_set.contains(id) || reason.trim().is_empty() || !noted.insert(id) {
            return Err(format!(
                "legacy Auto Review selection note for '{id}' does not match the selected plan"
            ));
        }
    }
    if noted != selected_set {
        return Err(
            "legacy Auto Review selection notes must cover every selected specialist".to_string(),
        );
    }
    Ok(selected)
}

pub(super) fn steps() -> Vec<StepConfig> {
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
    for specialist in SPECIALISTS {
        let mut step = base_step(
            specialist.id,
            specialist.label,
            specialist.prompt.to_string(),
            &[],
        );
        step.run_if = Some(match specialist.kind {
            Kind::Method => RunCondition::SurveyPath {
                pointer: "/review_plan/method_specialist_ids".to_string(),
                equals: None,
                exists: None,
                contains: Some(serde_json::Value::String(specialist.id.to_string())),
            },
            Kind::Field => RunCondition::SurveyPath {
                pointer: "/review_plan/field_specialist_id".to_string(),
                equals: Some(serde_json::Value::String(specialist.id.to_string())),
                exists: None,
                contains: None,
            },
        });
        steps.push(step);
    }
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

pub(super) fn orientation_prompt() -> String {
    let catalog = SPECIALISTS
        .iter()
        .map(|specialist| {
            let kind = match specialist.kind {
                Kind::Method => "method",
                Kind::Field => "field",
            };
            format!(
                "- `{}` ({kind}) — {}",
                specialist.id, specialist.description
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    ORIENTATION_TEMPLATE.replace("{specialist_catalog}", &catalog)
}

pub(super) fn orientation_schema() -> serde_json::Value {
    let field_ids = SPECIALISTS
        .iter()
        .filter(|specialist| matches!(specialist.kind, Kind::Field))
        .map(|specialist| serde_json::Value::String(specialist.id.to_string()))
        .collect::<Vec<_>>();
    let method_ids = SPECIALISTS
        .iter()
        .filter(|specialist| matches!(specialist.kind, Kind::Method))
        .map(|specialist| serde_json::Value::String(specialist.id.to_string()))
        .collect::<Vec<_>>();
    let all_ids = field_ids
        .iter()
        .chain(method_ids.iter())
        .cloned()
        .collect::<Vec<_>>();
    serde_json::json!({
        "type": "object",
        "required": ["metadata", "review_plan", "sections", "formal_results", "tables_figures", "notation", "stated_contribution", "key_references", "extraction_quality_notes"],
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
                "required": ["primary_domain", "subject", "paper_forms", "methods", "field_specialist_id", "method_specialist_ids", "selection_notes", "routing_uncertainty"],
                "properties": {
                    "primary_domain": {"type": "string"},
                    "subject": {"type": "string"},
                    "paper_forms": {"type": "array", "minItems": 1, "uniqueItems": true, "items": {"type": "string"}},
                    "methods": {"type": "array", "minItems": 1, "uniqueItems": true, "items": {"type": "string"}},
                    "field_specialist_id": {"type": "string", "enum": field_ids},
                    "method_specialist_ids": {"type": "array", "minItems": 1, "maxItems": 4, "uniqueItems": true, "items": {"type": "string", "enum": method_ids}},
                    "selection_notes": {
                        "type": "array", "minItems": 2, "maxItems": 5,
                        "items": {"type": "object", "required": ["id", "reason"], "properties": {"id": {"type": "string", "enum": all_ids}, "reason": {"type": "string"}}}
                    },
                    "routing_uncertainty": {"type": "array", "maxItems": 5, "items": {"type": "string"}}
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

pub(super) fn orientation_schema_without_conceptual_argument() -> serde_json::Value {
    let mut schema = orientation_schema();
    for pointer in [
        "/properties/review_plan/properties/method_specialist_ids/items/enum",
        "/properties/review_plan/properties/selection_notes/items/properties/id/enum",
    ] {
        if let Some(values) = schema
            .pointer_mut(pointer)
            .and_then(serde_json::Value::as_array_mut)
        {
            values.retain(|value| value.as_str() != Some("conceptual_argument"));
        }
    }
    schema
}

pub(super) fn orientation_schema_with_combined_specialist_ids_without_conceptual_argument(
) -> serde_json::Value {
    let mut schema = orientation_schema_without_conceptual_argument();
    let specialist_ids = SPECIALISTS
        .iter()
        .filter(|specialist| specialist.id != "conceptual_argument")
        .map(|specialist| serde_json::Value::String(specialist.id.to_string()))
        .collect::<Vec<_>>();
    let properties = schema
        .pointer_mut("/properties/review_plan/properties")
        .and_then(serde_json::Value::as_object_mut)
        .expect("legacy review-plan properties");
    properties.remove("field_specialist_id");
    properties.remove("method_specialist_ids");
    properties.insert(
        "specialist_ids".to_string(),
        serde_json::json!({
            "type": "array",
            "minItems": 2,
            "maxItems": 5,
            "uniqueItems": true,
            "items": {"type": "string", "enum": specialist_ids}
        }),
    );
    let required = schema
        .pointer_mut("/properties/review_plan/required")
        .and_then(serde_json::Value::as_array_mut)
        .expect("legacy review-plan required list");
    required.retain(|value| {
        !matches!(
            value.as_str(),
            Some("field_specialist_id" | "method_specialist_ids")
        )
    });
    required.push(serde_json::Value::String("specialist_ids".to_string()));
    schema
}
