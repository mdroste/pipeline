//! Legacy profile records and ordered conversion into current workflow steps.

use super::{configure_artifact_flow, ExtractionConfig, MergeConfig, Phase, StepConfig};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
pub(super) struct LegacyRefereeConfig {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) prompt: String,
    pub(super) enabled: bool,
    #[serde(default)]
    pub(super) web_search: bool,
    #[serde(default)]
    pub(super) agents: Vec<String>,
}

#[derive(Deserialize)]
pub(super) struct LegacyPostStepConfig {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) prompt: String,
    pub(super) enabled: bool,
    #[serde(default)]
    pub(super) agents: Vec<String>,
}

#[derive(Deserialize)]
pub(super) struct LegacyProfileData {
    /// The oldest format has no name.
    #[serde(default = "migrated_name")]
    pub(super) name: String,
    #[serde(default)]
    pub(super) referees: Vec<LegacyRefereeConfig>,
    #[serde(default)]
    pub(super) post_steps: Vec<LegacyPostStepConfig>,
    #[serde(default)]
    pub(super) merge: MergeConfig,
}

fn migrated_name() -> String {
    "Migrated".to_string()
}

pub(super) fn referee_to_step(r: LegacyRefereeConfig) -> StepConfig {
    let tools = if r.web_search {
        vec!["WebSearch".to_string()]
    } else {
        vec![]
    };
    StepConfig {
        id: r.id,
        label: r.label,
        prompt: r.prompt,
        enabled: r.enabled,
        phase: Phase::Parallel,
        tools,
        agents: r.agents,
        ..Default::default()
    }
}

pub(super) fn post_step_to_step(p: LegacyPostStepConfig) -> StepConfig {
    StepConfig {
        id: p.id,
        label: p.label,
        prompt: p.prompt,
        enabled: p.enabled,
        phase: Phase::Sequential,
        tools: vec![],
        agents: p.agents,
        ..Default::default()
    }
}

pub(super) fn convert_legacy_steps(
    referees: Vec<LegacyRefereeConfig>,
    post_steps: Vec<LegacyPostStepConfig>,
) -> Vec<StepConfig> {
    let mut steps: Vec<StepConfig> = referees.into_iter().map(referee_to_step).collect();
    steps.extend(post_steps.into_iter().map(post_step_to_step));
    configure_artifact_flow(steps, &ExtractionConfig::default().input_mode, &[])
}

/// Steps written before contexts became explicit read what their phase
/// implied. An empty context would now mean an isolated step, so every save
/// since has written the key, and steps without it get the flow they implied.
/// A file with some of each cannot be read either way.
pub(super) fn wire_implicit_context(
    raw_steps: &Value,
    steps: Vec<StepConfig>,
    input_mode: &str,
) -> Result<Vec<StepConfig>, String> {
    let implicit = raw_steps
        .as_array()
        .into_iter()
        .flatten()
        .filter(|step| step.get("context").is_none())
        .count();
    if implicit == 0 {
        return Ok(steps);
    }
    if implicit < steps.len() {
        return Err(
            "Some steps in this file say what they read and others do not. Export it again, or add what each step reads.".to_string(),
        );
    }
    // A sequential step that declared Read also read the primary input.
    let readers: Vec<String> = steps
        .iter()
        .filter(|step| step.tools.iter().any(|tool| tool == "Read"))
        .map(|step| step.id.clone())
        .collect();
    let readers: Vec<&str> = readers.iter().map(String::as_str).collect();
    Ok(configure_artifact_flow(steps, input_mode, &readers))
}

/// One step taken from a file older than explicit contexts. A parallel step's
/// input is known. A sequential step read the earlier steps of a workflow that
/// is not here, so it cannot be added without cutting it off from them.
pub(super) fn older_step_with_context(step: StepConfig) -> Result<StepConfig, String> {
    if step.phase == Phase::Sequential {
        return Err(format!(
            "Step '{}' comes from an older file and read the steps before it automatically, which can't be carried over when it is added on its own. Import the whole file, or add a new step and copy its prompt.",
            step.label
        ));
    }
    let wired = configure_artifact_flow(vec![step], &ExtractionConfig::default().input_mode, &[]);
    wired
        .into_iter()
        .next()
        .ok_or_else(|| "The file holds no step".to_string())
}
