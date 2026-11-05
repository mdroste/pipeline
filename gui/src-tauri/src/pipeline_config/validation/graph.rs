//! Effective dependency edges and runnable-graph validation.

use super::super::{ArtifactSelector, StepConfig};

impl StepConfig {
    /// Step ids whose artifacts this step consumes.
    pub fn artifact_dependencies(&self) -> impl Iterator<Item = &str> {
        self.context
            .include
            .iter()
            .filter_map(|selector| match selector {
                ArtifactSelector::Step { step, .. } => Some(step.as_str()),
                _ => None,
            })
            .chain(
                // Fanning out over an upstream artifact is dataflow: the step
                // becomes ready only after its item source completes.
                self.for_each
                    .as_ref()
                    .and_then(|for_each| for_each.artifact.as_ref())
                    .map(|artifact| artifact.step.as_str()),
            )
    }

    /// Dependencies whose successful completion is part of this step's
    /// execution contract. They also establish scheduling edges.
    pub fn policy_dependencies(&self) -> impl Iterator<Item = &str> {
        self.dependency_policy
            .required
            .iter()
            .chain(self.dependency_policy.quorum.iter())
            .map(String::as_str)
    }
}

/// Compute each enabled step's dependency set. `after` contributes order-only
/// edges; upstream artifact selections contribute data edges.
pub(crate) fn resolve_dependencies(
    enabled: &[&StepConfig],
) -> Vec<std::collections::HashSet<String>> {
    enabled
        .iter()
        .map(|step| {
            step.after
                .iter()
                .map(String::as_str)
                .chain(step.artifact_dependencies())
                .chain(step.policy_dependencies())
                .map(str::to_string)
                .collect()
        })
        .collect()
}

/// Reject enabled dependency graphs that cannot run. Disabled steps may retain
/// stale settings while being edited, but an enabled step may never wait on a
/// disabled or unknown upstream.
pub fn validate_dependencies(steps: &[StepConfig]) -> Result<(), String> {
    use std::collections::{HashMap, HashSet};
    let ids: HashSet<&str> = steps.iter().map(|s| s.id.as_str()).collect();
    let enabled_ids: HashSet<&str> = steps
        .iter()
        .filter(|step| step.enabled)
        .map(|step| step.id.as_str())
        .collect();

    // Unknown / self dependencies.
    for s in steps.iter().filter(|step| step.enabled) {
        for dep in s
            .after
            .iter()
            .map(String::as_str)
            .chain(s.artifact_dependencies())
            .chain(s.policy_dependencies())
        {
            if dep == s.id {
                return Err(format!("Step '{}' lists itself as a dependency.", s.id));
            }
            if !ids.contains(dep) {
                return Err(format!(
                    "Step '{}' depends on unknown step '{}'.",
                    s.id, dep
                ));
            }
            if !enabled_ids.contains(dep) {
                return Err(format!(
                    "Step '{}' depends on disabled step '{}'. Enable it or remove the dependency.",
                    s.id, dep
                ));
            }
        }

        let mut required = HashSet::new();
        for dependency in &s.dependency_policy.required {
            if !required.insert(dependency.as_str()) {
                return Err(format!(
                    "Step '{}' lists required dependency '{}' more than once.",
                    s.id, dependency
                ));
            }
        }
        let mut quorum = HashSet::new();
        for dependency in &s.dependency_policy.quorum {
            if !quorum.insert(dependency.as_str()) {
                return Err(format!(
                    "Step '{}' lists quorum dependency '{}' more than once.",
                    s.id, dependency
                ));
            }
            if required.contains(dependency.as_str()) {
                return Err(format!(
                    "Step '{}' lists dependency '{}' as both required and quorum.",
                    s.id, dependency
                ));
            }
        }
        let minimum = s.dependency_policy.minimum_successes as usize;
        if quorum.is_empty() && minimum != 0 {
            return Err(format!(
                "Step '{}' sets a quorum minimum without quorum dependencies.",
                s.id
            ));
        }
        if !quorum.is_empty() && (minimum == 0 || minimum > quorum.len()) {
            return Err(format!(
                "Step '{}' quorum minimum must be between 1 and {}.",
                s.id,
                quorum.len()
            ));
        }
    }

    // Cycle detection over the complete effective graph (DFS with a colour
    // map), including both order-only and artifact-dataflow dependencies.
    let enabled: Vec<&StepConfig> = steps.iter().filter(|step| step.enabled).collect();
    let resolved = resolve_dependencies(&enabled);
    let graph: HashMap<&str, Vec<&str>> = enabled
        .iter()
        .zip(&resolved)
        .map(|(step, deps)| (step.id.as_str(), deps.iter().map(String::as_str).collect()))
        .collect();
    #[derive(PartialEq, Clone, Copy)]
    enum Mark {
        Visiting,
        Done,
    }
    let mut marks: HashMap<&str, Mark> = HashMap::new();
    // Iterative DFS so deep graphs can't blow the stack.
    for start in graph.keys().copied() {
        if marks.contains_key(start) {
            continue;
        }
        let mut stack: Vec<(&str, usize)> = vec![(start, 0)];
        marks.insert(start, Mark::Visiting);
        while let Some((node, idx)) = stack.last().copied() {
            let neighbours = graph.get(node).map(|v| v.as_slice()).unwrap_or(&[]);
            if idx < neighbours.len() {
                stack.last_mut().unwrap().1 += 1;
                let next = neighbours[idx];
                match marks.get(next) {
                    Some(Mark::Visiting) => {
                        return Err(format!(
                            "Step dependencies form a cycle involving '{next}'."
                        ));
                    }
                    Some(Mark::Done) => {}
                    None => {
                        marks.insert(next, Mark::Visiting);
                        stack.push((next, 0));
                    }
                }
            } else {
                marks.insert(node, Mark::Done);
                stack.pop();
            }
        }
    }
    Ok(())
}
