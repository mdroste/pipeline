//! Ordered schema/count and review-plan admission checks.
use super::catalog::{SubjectLevel, GENRES, METHODS, RESEARCH_ARTICLE_GENRE, SUBJECTS};
use super::schema_contract::validate_auto_review_schema_contract;
use super::{ADAPTIVE_AGENT_COUNT_KEY, AUTO_REVIEW_CONTRACT};
use crate::pipeline_config::StepConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdaptiveAgentBounds {
    pub subject_min: usize,
    pub subject_max: usize,
    pub method_min: usize,
    pub method_max: usize,
}

impl AdaptiveAgentBounds {
    pub fn total_min(self) -> usize {
        self.subject_min + self.method_min
    }

    pub fn total_max(self) -> usize {
        self.subject_max + self.method_max
    }
}

fn schema_array_bound(
    schema: &serde_json::Value,
    property: &str,
    keyword: &str,
    default: usize,
) -> Result<usize, String> {
    let Some(value) = schema.pointer(&format!(
        "/properties/review_plan/properties/{property}/{keyword}"
    )) else {
        return Ok(default);
    };
    value
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| format!("review_plan.{property}.{keyword} must be a nonnegative integer"))
}

pub fn adaptive_agent_bounds(schema: &serde_json::Value) -> Result<AdaptiveAgentBounds, String> {
    let bounds = AdaptiveAgentBounds {
        subject_min: schema_array_bound(schema, "subject_specialist_ids", "minItems", 1)?,
        subject_max: schema_array_bound(schema, "subject_specialist_ids", "maxItems", 2)?,
        method_min: schema_array_bound(schema, "method_specialist_ids", "minItems", 1)?,
        method_max: schema_array_bound(schema, "method_specialist_ids", "maxItems", 4)?,
    };
    if !(1..=2).contains(&bounds.subject_min)
        || !(1..=2).contains(&bounds.subject_max)
        || bounds.subject_min > bounds.subject_max
    {
        return Err("subject specialist bounds must stay within one to two agents".to_string());
    }
    if !(1..=4).contains(&bounds.method_min)
        || !(1..=4).contains(&bounds.method_max)
        || bounds.method_min > bounds.method_max
    {
        return Err("method specialist bounds must stay within one to four agents".to_string());
    }
    Ok(bounds)
}

pub fn configured_agent_count(schema: &serde_json::Value) -> Result<Option<usize>, String> {
    if schema
        .get("x-pipeline-contract")
        .and_then(serde_json::Value::as_str)
        != Some(AUTO_REVIEW_CONTRACT)
    {
        return Ok(None);
    }
    let Some(value) = schema.get(ADAPTIVE_AGENT_COUNT_KEY) else {
        return Ok(None);
    };
    let bounds = adaptive_agent_bounds(schema)?;
    let count = value.as_u64().ok_or_else(|| {
        format!(
            "{ADAPTIVE_AGENT_COUNT_KEY} must be an integer from {} to {}",
            bounds.total_min(),
            bounds.total_max()
        )
    })? as usize;
    if !(bounds.total_min()..=bounds.total_max()).contains(&count) {
        return Err(format!(
            "{ADAPTIVE_AGENT_COUNT_KEY} must be from {} to {}, got {count}",
            bounds.total_min(),
            bounds.total_max()
        ));
    }
    Ok(Some(count))
}

pub fn validate_schema_settings(schema: &serde_json::Value) -> Result<(), String> {
    if schema.get(ADAPTIVE_AGENT_COUNT_KEY).is_some() && schema.get("x-pipeline-contract").is_none()
    {
        return Err(format!(
            "{ADAPTIVE_AGENT_COUNT_KEY} requires x-pipeline-contract: '{AUTO_REVIEW_CONTRACT}'"
        ));
    }
    if let Some(contract) = schema.get("x-pipeline-contract") {
        let contract = contract
            .as_str()
            .ok_or_else(|| "x-pipeline-contract must be a string".to_string())?;
        if contract != AUTO_REVIEW_CONTRACT {
            return Err(format!(
                "Unsupported x-pipeline-contract '{contract}'; expected '{AUTO_REVIEW_CONTRACT}'"
            ));
        }
        validate_auto_review_schema_contract(schema)?;
        adaptive_agent_bounds(schema)?;
    }
    configured_agent_count(schema).map(|_| ())
}

pub fn validate_profile_contract_identity(
    steps: &[StepConfig],
    schema: Option<&serde_json::Value>,
) -> Result<(), String> {
    let has_auto_skeleton = steps.iter().any(|step| {
        matches!(
            step.id.as_str(),
            "auto_contribution"
                | "auto_consistency"
                | "auto_exposition"
                | "auto_synthesis"
                | "auto_validate"
        )
    });
    let has_contract = schema
        .and_then(|schema| schema.get("x-pipeline-contract"))
        .and_then(serde_json::Value::as_str)
        == Some(AUTO_REVIEW_CONTRACT);
    if has_auto_skeleton && !has_contract {
        Err(format!(
            "Auto Review skeleton steps require x-pipeline-contract: '{AUTO_REVIEW_CONTRACT}'"
        ))
    } else {
        Ok(())
    }
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
    validate_review_plan(orientation)?;
    if let Some(expected) = configured_agent_count(schema)? {
        let plan = orientation
            .get("review_plan")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| "$.review_plan: expected an object".to_string())?;
        let subject_count = plan
            .get("subject_specialist_ids")
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len);
        let method_count = plan
            .get("method_specialist_ids")
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len);
        let actual = subject_count + method_count;
        if actual != expected {
            return Err(format!(
                "review_plan must select exactly {expected} adaptive agents, got {actual}"
            ));
        }
    }
    Ok(())
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

    // Genre is a document classification, not a reviewer selection.
    if let Some(value) = plan.get("genre") {
        let genre = value
            .as_str()
            .ok_or_else(|| "$.review_plan.genre: expected a string".to_string())?;
        if genre != RESEARCH_ARTICLE_GENRE && !GENRES.iter().any(|entry| entry.id == genre) {
            return Err(format!("unknown document genre '{genre}'"));
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
