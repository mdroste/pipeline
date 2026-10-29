//! Legacy profile records and ordered conversion into current workflow steps.

use super::{MergeConfig, Phase, StepConfig};
use serde::Deserialize;

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
    pub(super) name: String,
    #[serde(default)]
    pub(super) referees: Vec<LegacyRefereeConfig>,
    #[serde(default)]
    pub(super) post_steps: Vec<LegacyPostStepConfig>,
    #[serde(default)]
    pub(super) merge: MergeConfig,
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
    steps
}
