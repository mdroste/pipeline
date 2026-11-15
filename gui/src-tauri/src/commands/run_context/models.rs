use super::{stable_snapshot_fingerprint, RunSnapshot};
use crate::pipeline_config::PipelineConfig;

/// The model a whole review runs on: provider, exact model (empty lets the
/// CLI or Pipeline choose), and thinking level.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq, Eq)]
pub struct ReviewModelChoice {
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub effort: String,
}

/// One-report model choices. `review` sets the model every call inherits
/// (orientation, parallel, merge, sequential, LLM extraction, revision
/// comparison); Settings stage exceptions and workflow-pinned steps keep
/// their own models unless the matching flag clears them. `agents` and the
/// merge fields choose Parallel agents and the Merge agent for this report;
/// a payload with only those (saved before `review` existed) keeps its old
/// meaning and replaces workflow-pinned Parallel agents.
///
/// `None` at the command boundary means use Settings and workflow choices
/// unchanged; a present value is fingerprinted into the immutable launch
/// snapshot, saved with the run, and never written to Settings.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct RunModelOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_method: Option<String>,
    // Defaults are skipped when serializing so payloads from before these
    // fields keep the same fingerprint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<ReviewModelChoice>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clear_settings_exceptions: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub override_workflow_pins: bool,
    #[serde(default)]
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

pub(in crate::commands) fn bind_model_overrides(
    mut snapshot: RunSnapshot,
    overrides: Option<&RunModelOverrides>,
) -> Result<RunSnapshot, String> {
    let Some(overrides) = overrides else {
        return Ok(snapshot);
    };
    apply_model_overrides(&mut snapshot.settings, &mut snapshot.config, overrides)?;
    snapshot.agent_overrides = Some(overrides.clone());
    snapshot.fingerprint = stable_snapshot_fingerprint(&(snapshot.fingerprint.as_str(), overrides))
        .map_err(|error| format!("Could not fingerprint Parallel agent options: {error}"))?;
    Ok(snapshot)
}

/// Folds a one-report model choice into a run's settings and workflow.
pub(in crate::commands) fn apply_model_overrides(
    settings: &mut crate::settings::Settings,
    config: &mut PipelineConfig,
    overrides: &RunModelOverrides,
) -> Result<(), String> {
    if let Some(review) = &overrides.review {
        apply_review_model(settings, config, review, overrides)?;
    } else if overrides.agents.is_empty() && overrides.extraction_method.is_none() {
        return Err("Select at least one Parallel agent".to_string());
    }
    if let Some(method) = overrides.extraction_method.as_deref() {
        if !matches!(method, "pdftotext" | "llm" | "paddleocr-vl-full") {
            return Err(format!("Unknown PDF reading method '{method}'"));
        }
        config.extraction.method = method.to_string();
    }
    if !overrides.agents.is_empty() {
        settings.default_parallel_agents = overrides.agents.clone();
        settings.default_parallel_model_overrides = overrides.model_overrides.clone();
        settings.default_parallel_effort_overrides = overrides.effort_overrides.clone();
        // Steps with no agents of their own take the list above. The older
        // payload also replaced workflow-pinned Parallel agents; a review
        // choice keeps those pins unless asked to override them.
        let replace_pins = overrides.review.is_none() || overrides.override_workflow_pins;
        for step in config.steps.iter_mut().filter(|step| {
            step.phase == crate::pipeline_config::Phase::Parallel
                && (replace_pins || step.agents.is_empty())
        }) {
            step.agents = overrides.agents.clone();
            step.model.clear();
            step.model_overrides = overrides.model_overrides.clone();
            step.effort.clear();
            step.effort_overrides = overrides.effort_overrides.clone();
        }
    }
    if let Some(merge_agent) = overrides.merge_agent.as_deref() {
        if merge_agent.trim().is_empty() {
            return Err("Select a Merge agent".to_string());
        }
        settings.default_merge_agent = merge_agent.to_string();
        settings.default_merge_model_overrides = overrides.merge_model_overrides.clone();
        settings.default_merge_effort_overrides = overrides.merge_effort_overrides.clone();
        // An explicit workflow Merge provider would otherwise bypass the
        // transient Settings policies above. Clearing it makes this one-run
        // selection the inherited, fully configured Merge default.
        config.merge.agents.clear();
    } else if !overrides.merge_model_overrides.is_empty()
        || !overrides.merge_effort_overrides.is_empty()
    {
        return Err("Merge model or thinking overrides require a Merge agent".to_string());
    }
    settings.validate()
}

/// Make `review` the default model of this run's settings snapshot. Every
/// call that names no model of its own inherits it, including orientation,
/// LLM extraction, and revision comparison, which read the default directly.
fn apply_review_model(
    settings: &mut crate::settings::Settings,
    config: &mut PipelineConfig,
    review: &ReviewModelChoice,
    overrides: &RunModelOverrides,
) -> Result<(), String> {
    let provider = review.provider.trim();
    if !matches!(provider, "claude" | "codex" | "antigravity" | "local") {
        return Err(format!("Unknown review model provider '{provider}'"));
    }
    settings.preferred_provider = provider.to_string();
    let model = review.model.trim().to_string();
    if provider == "local" {
        if !model.is_empty() {
            settings.local_model = model;
        }
        settings.default_model.clear();
    } else {
        settings.default_model = model;
    }
    settings.default_effort = review.effort.trim().to_string();
    if overrides.clear_settings_exceptions {
        settings.default_orientation_agent.clear();
        settings.default_orientation_model_overrides.clear();
        settings.default_orientation_effort_overrides.clear();
        settings.default_parallel_agents.clear();
        settings.default_parallel_model_overrides.clear();
        settings.default_parallel_effort_overrides.clear();
        settings.default_merge_agent.clear();
        settings.default_merge_model_overrides.clear();
        settings.default_merge_effort_overrides.clear();
        settings.default_sequential_agent.clear();
        settings.default_sequential_model_overrides.clear();
        settings.default_sequential_effort_overrides.clear();
    }
    if overrides.override_workflow_pins {
        for step in config.steps.iter_mut().filter(|step| step.kind.is_model()) {
            step.agents.clear();
            step.model.clear();
            step.model_overrides.clear();
            step.effort.clear();
            step.effort_overrides.clear();
        }
        config.merge.agents.clear();
    }
    Ok(())
}

/// One-run Review limits from the command line. `None` fields keep the
/// Settings value; a present value is fingerprinted and never persisted.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize)]
pub struct ReviewLimitOverrides {
    pub max_cost_usd: Option<f64>,
    pub max_calls: Option<u32>,
    pub max_minutes: Option<u32>,
}

impl ReviewLimitOverrides {
    pub fn is_empty(&self) -> bool {
        self.max_cost_usd.is_none() && self.max_calls.is_none() && self.max_minutes.is_none()
    }
}

pub(in crate::commands) fn bind_review_limits(
    mut snapshot: RunSnapshot,
    limits: &ReviewLimitOverrides,
) -> Result<RunSnapshot, String> {
    if limits.is_empty() {
        return Ok(snapshot);
    }
    if let Some(cost) = limits.max_cost_usd {
        if !cost.is_finite() || cost < 0.0 {
            return Err("The cost limit must be zero or a positive number".to_string());
        }
        snapshot.settings.review_max_cost_usd = cost;
    }
    if let Some(calls) = limits.max_calls {
        snapshot.settings.review_max_calls = calls;
    }
    if let Some(minutes) = limits.max_minutes {
        snapshot.settings.review_max_minutes = minutes;
    }
    snapshot.fingerprint = stable_snapshot_fingerprint(&(snapshot.fingerprint.as_str(), limits))
        .map_err(|error| format!("Could not fingerprint Review limits: {error}"))?;
    Ok(snapshot)
}
