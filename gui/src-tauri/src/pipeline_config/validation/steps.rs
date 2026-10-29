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

/// Analyzer steps run one registered host check; every model-call field must
/// be empty so a saved analyzer step can never silently become a model call.
fn validate_analyzer_step(step: &StepConfig) -> Result<(), String> {
    use super::super::StepKind;
    match (step.kind, step.analyzer.as_ref()) {
        (StepKind::Model, None) => Ok(()),
        (StepKind::Model, Some(_)) => Err(format!(
            "Step '{}' names an analyzer but is not an analyzer step",
            step.id
        )),
        (StepKind::Analyzer, None) => Err(format!(
            "Analyzer step '{}' must name an analyzer and version",
            step.id
        )),
        (StepKind::Analyzer, Some(spec)) => {
            let info = crate::pipeline::analyzers::registry::find(&spec.id, spec.version)
                .ok_or_else(|| {
                    format!(
                        "Step '{}' uses unknown analyzer '{}' version {}",
                        step.id, spec.id, spec.version
                    )
                })?;
            let model_fields = [
                (!step.prompt.trim().is_empty(), "a prompt"),
                (!step.system_prompt.trim().is_empty(), "system instructions"),
                (!step.agents.is_empty(), "providers"),
                (
                    !step.model.trim().is_empty() || !step.model_overrides.is_empty(),
                    "a model",
                ),
                (
                    !step.effort.trim().is_empty() || !step.effort_overrides.is_empty(),
                    "an effort level",
                ),
                (step.output_schema.is_some(), "an output schema"),
                (step.for_each.is_some(), "fan-out"),
            ];
            if let Some((_, field)) = model_fields.iter().find(|(set, _)| *set) {
                return Err(format!(
                    "Analyzer step '{}' cannot have {field}; it makes no model call",
                    step.id
                ));
            }
            crate::pipeline::analyzers::registry::validate_options(info, &spec.options)
                .map_err(|error| format!("Analyzer step '{}': {error}", step.id))
        }
    }
}

/// Artifact fan-out reads structured JSON, so its source must declare an
/// output schema (or be an analyzer, which always returns JSON). With a
/// schema, the pointer must be able to select an array.
fn validate_fan_out_source(
    step: &StepConfig,
    source: &StepConfig,
    pointer: &str,
) -> Result<(), String> {
    let Some(schema) = &source.output_schema else {
        if source.kind == super::super::StepKind::Analyzer {
            return Ok(());
        }
        return Err(format!(
            "Step '{}' fans out over step '{}', which has no output schema; fan-out needs a step that returns structured JSON",
            step.id, source.id
        ));
    };
    // An unresolvable schema is reported by the source step's own checks.
    let Ok(resolved) = crate::pipeline::structured::resolve_schema_reference(schema) else {
        return Ok(());
    };
    if schema_selects_array(&resolved, pointer) {
        Ok(())
    } else {
        Err(format!(
            "Step '{}' fan-out pointer '{}' does not select an array in the output schema of step '{}'",
            step.id, pointer, source.id
        ))
    }
}

/// Whether an RFC 6901 `pointer` can select an array in a value conforming to
/// the resolved `schema`. Untyped nodes and objects without declared
/// properties pass; an undeclared field, a scalar, or a final non-array type
/// cannot hold an array.
fn schema_selects_array(schema: &serde_json::Value, pointer: &str) -> bool {
    let mut node = schema;
    for token in pointer.split('/').skip(1) {
        let token = token.replace("~1", "/").replace("~0", "~");
        let child = match node.get("type").and_then(serde_json::Value::as_str) {
            None => return true,
            Some("object") => match node.get("properties").and_then(|p| p.as_object()) {
                Some(properties) if !properties.is_empty() => properties.get(&token),
                _ => return true,
            },
            Some("array") => match node.get("items") {
                None => return true,
                Some(items) => token.parse::<usize>().is_ok().then_some(items),
            },
            Some(_) => None,
        };
        match child {
            Some(child) => node = child,
            None => return false,
        }
    }
    node.get("type")
        .and_then(serde_json::Value::as_str)
        .is_none_or(|kind| kind == "array")
}

pub fn validate_workflow_semantics(steps: &[StepConfig]) -> Result<(), String> {
    for step in steps {
        validate_analyzer_step(step)?;
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

/// Require a terminal-capable workflow, and fan-out sources that return
/// structured JSON, without making legacy invalid profiles impossible to open
/// and repair. Call this at explicit save/import and run boundaries, rather
/// than from the structural load validator.
pub fn validate_enabled_sequential_step(steps: &[StepConfig]) -> Result<(), String> {
    if !steps
        .iter()
        .any(|step| step.enabled && step.phase == Phase::Sequential)
    {
        return Err(
            "A workflow must have at least one enabled Sequential step to produce a final report."
                .to_string(),
        );
    }
    for step in steps {
        let Some(artifact) = step.for_each.as_ref().and_then(|fe| fe.artifact.as_ref()) else {
            continue;
        };
        // Unknown sources are reported by the structural validator.
        if let Some(source) = steps
            .iter()
            .find(|candidate| candidate.id == artifact.step.trim() && candidate.id != step.id)
        {
            validate_fan_out_source(step, source, &artifact.pointer)?;
        }
    }
    Ok(())
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
