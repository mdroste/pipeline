//! Ordered structural step validation and terminal workflow requirements.

use super::super::{
    Phase, StepConfig, ALLOWED_AGENTS, ALLOWED_TOOLS, MAX_FAN_OUT_ITEMS, MAX_OUTPUT_SCHEMA_BYTES,
    MAX_PROFILE_STEPS, MAX_STEP_LABEL_CHARS, MAX_STEP_PROMPT_BYTES,
};
use super::conditions::validate_run_conditions;
use super::graph::validate_dependencies;

/// Step ids key the executor's pass events and the merge grouping
/// (`{id}/{agent}`), so duplicates silently collide. Reject them at save and
/// import time. Deliberately not enforced on load/migration, so an existing
/// profile with duplicates can still be opened and repaired in the editor.
pub fn validate_unique_step_ids(steps: &[StepConfig]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for step in steps {
        if step.id.is_empty()
            || step.id.len() > 64
            || !step
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return Err(format!(
                "Invalid step id '{}'. Use 1–64 ASCII letters, numbers, '.', '-', or '_'.",
                step.id
            ));
        }
        if !seen.insert(step.id.as_str()) {
            return Err(format!(
                "Duplicate step id '{}' — step ids must be unique",
                step.id
            ));
        }
    }
    Ok(())
}

pub fn validate_workflow_semantics(steps: &[StepConfig]) -> Result<(), String> {
    for step in steps {
        if step.phase == Phase::Sequential && step.agents.len() > 1 {
            return Err(format!(
                "Sequential step '{}' selects multiple agents, but sequential multi-agent execution is unsupported.",
                step.id
            ));
        }
        if step.phase == Phase::Sequential && step.for_each.is_some() {
            return Err(format!(
                "Sequential step '{}' uses fan-out, but sequential fan-out is unsupported.",
                step.id
            ));
        }
    }
    Ok(())
}

/// Require a terminal-capable workflow without making legacy invalid profiles
/// impossible to open and repair. Call this at explicit save/import and run
/// boundaries, rather than from the structural load validator.
pub fn validate_enabled_sequential_step(steps: &[StepConfig]) -> Result<(), String> {
    if steps
        .iter()
        .any(|step| step.enabled && step.phase == Phase::Sequential)
    {
        Ok(())
    } else {
        Err(
            "A workflow must have at least one enabled Sequential step to produce a final report."
                .to_string(),
        )
    }
}

pub(in crate::pipeline_config) fn validate_profile_steps(
    steps: &[StepConfig],
) -> Result<(), String> {
    if steps.len() > MAX_PROFILE_STEPS {
        return Err(format!(
            "Profile has {} steps; the safety limit is {MAX_PROFILE_STEPS}",
            steps.len()
        ));
    }
    validate_unique_step_ids(steps)?;
    validate_dependencies(steps)?;
    validate_workflow_semantics(steps)?;
    validate_run_conditions(steps)?;
    for step in steps {
        if step.label.chars().count() > MAX_STEP_LABEL_CHARS {
            return Err(format!(
                "Step '{}' label exceeds {MAX_STEP_LABEL_CHARS} characters",
                step.id
            ));
        }
        if step.prompt.len() > MAX_STEP_PROMPT_BYTES
            || step.system_prompt.len() > MAX_STEP_PROMPT_BYTES
        {
            return Err(format!(
                "Step '{}' prompt exceeds the {} MB safety limit",
                step.id,
                MAX_STEP_PROMPT_BYTES / 1024 / 1024
            ));
        }
        if step.tools.len() > ALLOWED_TOOLS.len() {
            return Err(format!("Step '{}' declares too many tools", step.id));
        }
        let mut tools = std::collections::HashSet::new();
        for tool in &step.tools {
            if !ALLOWED_TOOLS.contains(&tool.as_str()) {
                return Err(format!(
                    "Step '{}' requests unsupported tool '{}'. Allowed tools: {}",
                    step.id,
                    tool,
                    ALLOWED_TOOLS.join(", ")
                ));
            }
            if !tools.insert(tool.as_str()) {
                return Err(format!(
                    "Step '{}' lists tool '{}' more than once",
                    step.id, tool
                ));
            }
        }
        if step.agents.len() > ALLOWED_AGENTS.len() {
            return Err(format!("Step '{}' declares too many agents", step.id));
        }
        let mut agents = std::collections::HashSet::new();
        for agent in &step.agents {
            if !ALLOWED_AGENTS.contains(&agent.as_str()) {
                return Err(format!(
                    "Step '{}' names unsupported agent '{}'. Allowed agents: {}",
                    step.id,
                    agent,
                    ALLOWED_AGENTS.join(", ")
                ));
            }
            if !agents.insert(agent.as_str()) {
                return Err(format!(
                    "Step '{}' lists agent '{}' more than once",
                    step.id, agent
                ));
            }
        }
        if let Some(for_each) = &step.for_each {
            if let Some(artifact) = &for_each.artifact {
                if !for_each.glob.trim().is_empty() {
                    return Err(format!(
                        "Step '{}' fan-out must use either a glob or an upstream artifact, not both",
                        step.id
                    ));
                }
                let target = artifact.step.trim();
                if target.is_empty() || target == step.id {
                    return Err(format!(
                        "Step '{}' fan-out artifact must name another step in this workflow",
                        step.id
                    ));
                }
                if !steps.iter().any(|candidate| candidate.id == target) {
                    return Err(format!(
                        "Step '{}' fans out over unknown step '{}'",
                        step.id, target
                    ));
                }
                if !artifact.pointer.is_empty() && !artifact.pointer.starts_with('/') {
                    return Err(format!(
                        "Step '{}' fan-out pointer must be an RFC 6901 JSON pointer starting with '/'",
                        step.id
                    ));
                }
                if artifact.pointer.len() > 512 {
                    return Err(format!(
                        "Step '{}' fan-out pointer must contain at most 512 bytes",
                        step.id
                    ));
                }
            } else {
                if for_each.glob.trim().is_empty() || for_each.glob.len() > 1024 {
                    return Err(format!(
                        "Step '{}' fan-out glob must contain 1–1024 bytes",
                        step.id
                    ));
                }
                if for_each.glob.contains('\\') {
                    return Err(format!(
                        "Step '{}' has an invalid fan-out glob; use '/' as the path separator",
                        step.id
                    ));
                }
            }
            if !(1..=MAX_FAN_OUT_ITEMS).contains(&for_each.max) {
                return Err(format!(
                    "Step '{}' fan-out maximum must be between 1 and {MAX_FAN_OUT_ITEMS}",
                    step.id
                ));
            }
        }
        if let Some(schema) = &step.output_schema {
            if schema.get("x-pipeline-contract").is_some()
                || schema
                    .get(crate::auto_review::ADAPTIVE_AGENT_COUNT_KEY)
                    .is_some()
            {
                return Err(format!(
                    "Step '{}' output schema uses an orientation-only x-pipeline contract setting",
                    step.id
                ));
            }
            let bytes = serde_json::to_vec(schema)
                .map_err(|e| format!("Step '{}' output schema is invalid: {e}", step.id))?;
            if bytes.len() > MAX_OUTPUT_SCHEMA_BYTES {
                return Err(format!(
                    "Step '{}' output schema exceeds the {} MB safety limit",
                    step.id,
                    MAX_OUTPUT_SCHEMA_BYTES / 1024 / 1024
                ));
            }
            // Live contract references resolve before the provider preflight,
            // so an unknown reference fails at save rather than dispatch.
            let resolved = crate::pipeline::structured::resolve_schema_reference(schema)
                .map_err(|e| format!("Step '{}' output schema is invalid: {e}", step.id))?;
            crate::pipeline::structured::provider_schema(&resolved)
                .map_err(|e| format!("Step '{}' output schema is invalid: {e}", step.id))?;
            if let Some(target) = schema.get(crate::pipeline::structured::PRESERVE_FINDINGS_KEY) {
                let target = target.as_str().unwrap_or_default().trim();
                if target.is_empty()
                    || target == step.id
                    || !steps.iter().any(|candidate| candidate.id == target)
                {
                    return Err(format!(
                        "Step '{}' preserves findings from '{}', which is not another step in this workflow",
                        step.id, target
                    ));
                }
                if !step
                    .artifact_dependencies()
                    .any(|dependency| dependency == target)
                {
                    return Err(format!(
                        "Step '{}' preserves findings from '{}', but does not select that step as an artifact source",
                        step.id, target
                    ));
                }
            }
        }
    }
    Ok(())
}
