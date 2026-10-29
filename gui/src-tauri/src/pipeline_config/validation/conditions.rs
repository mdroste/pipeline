//! Run-condition validation against effective dependency edges.

use super::super::{RunCondition, StepConfig, MAX_JSON_POINTER_BYTES, MAX_RUN_IF_PATTERN_BYTES};
use super::graph::resolve_dependencies;

fn validate_json_pointer(pointer: &str) -> bool {
    if pointer.is_empty() {
        return true;
    }
    if !pointer.starts_with('/') {
        return false;
    }
    let bytes = pointer.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'~' {
            if index + 1 >= bytes.len() || !matches!(bytes[index + 1], b'0' | b'1') {
                return false;
            }
            index += 2;
        } else {
            index += 1;
        }
    }
    true
}

/// Validate run guards against the effective dependency graph. An output guard
/// may only read a transitive upstream, which guarantees the referent has
/// completed before the scheduler evaluates the guard.
pub(in crate::pipeline_config) fn validate_run_conditions(
    steps: &[StepConfig],
) -> Result<(), String> {
    let enabled: Vec<&StepConfig> = steps.iter().filter(|step| step.enabled).collect();
    let deps = resolve_dependencies(&enabled);
    let index: std::collections::HashMap<&str, usize> = enabled
        .iter()
        .enumerate()
        .map(|(i, step)| (step.id.as_str(), i))
        .collect();

    for (step_index, step) in enabled.iter().enumerate() {
        let Some(condition) = &step.run_if else {
            continue;
        };
        match condition {
            RunCondition::OutputMatches {
                step: target,
                pattern,
                ..
            } => {
                if target.is_empty() {
                    return Err(format!(
                        "Step '{}' output condition must select an upstream step.",
                        step.id
                    ));
                }
                if pattern.len() > MAX_RUN_IF_PATTERN_BYTES {
                    return Err(format!(
                        "Step '{}' output condition exceeds the {MAX_RUN_IF_PATTERN_BYTES}-byte pattern limit.",
                        step.id
                    ));
                }
                regex::Regex::new(pattern).map_err(|error| {
                    format!(
                        "Step '{}' output condition has an invalid regular expression: {error}",
                        step.id
                    )
                })?;
                let Some(&target_index) = index.get(target.as_str()) else {
                    return Err(format!(
                        "Step '{}' output condition refers to an unknown or disabled step '{}'.",
                        step.id, target
                    ));
                };
                let mut pending: Vec<usize> = deps[step_index]
                    .iter()
                    .filter_map(|id| index.get(id.as_str()).copied())
                    .collect();
                let mut upstream = std::collections::HashSet::new();
                while let Some(current) = pending.pop() {
                    if upstream.insert(current) {
                        pending.extend(
                            deps[current]
                                .iter()
                                .filter_map(|id| index.get(id.as_str()).copied()),
                        );
                    }
                }
                if !upstream.contains(&target_index) {
                    return Err(format!(
                        "Step '{}' output condition reads '{}', but that step is not an upstream dependency.",
                        step.id, target
                    ));
                }
            }
            RunCondition::SurveyPath { pointer, .. } => {
                if pointer.len() > MAX_JSON_POINTER_BYTES {
                    return Err(format!(
                        "Step '{}' survey pointer exceeds the {MAX_JSON_POINTER_BYTES}-byte limit.",
                        step.id
                    ));
                }
                if !validate_json_pointer(pointer) {
                    return Err(format!(
                        "Step '{}' survey condition uses an invalid JSON pointer '{}'.",
                        step.id, pointer
                    ));
                }
            }
        }
    }
    Ok(())
}
