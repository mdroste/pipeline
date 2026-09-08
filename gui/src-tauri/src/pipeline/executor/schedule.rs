//! One dependency algorithm for preview and runtime readiness.
use crate::models::StepOutput;
use crate::pipeline_config::{Phase, PipelineConfig, StepConfig};

/// One authoritative row in the profile's planned execution timeline.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPlanStage {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub step_ids: Vec<String>,
    pub step_labels: Vec<String>,
}

/// Human-facing work item for the input-preparation stage. The selected
/// container and its declared interpretation are distinct, so a folder used
/// as a source tree must not be described as a document bundle.
pub fn input_processing_label(input_interpretation: &str) -> &'static str {
    match input_interpretation.trim() {
        "folder" | "source_tree" => "Creating source-tree inventory",
        "none" => "Preparing workflow context",
        _ => "Creating document bundle",
    }
}

/// Simulate the exact readiness scheduler used by [`execute_steps`] without
/// running dynamic `run_if` predicates. The resulting waves are the canonical
/// preflight timeline exposed to the UI.
pub fn execution_plan(config: &PipelineConfig) -> Result<Vec<ExecutionPlanStage>, String> {
    let mut plan = vec![ExecutionPlanStage {
        id: "extracting".to_string(),
        kind: "extracting".to_string(),
        label: input_processing_label(&config.extraction.input_mode).to_string(),
        step_ids: Vec::new(),
        step_labels: Vec::new(),
    }];
    plan.push(ExecutionPlanStage {
        id: "orienting".to_string(),
        kind: "orienting".to_string(),
        label: if crate::auto_review::uses_auto_review_contract(config) {
            "Creating orientation map & review plan"
        } else {
            "Creating orientation map"
        }
        .to_string(),
        step_ids: Vec::new(),
        step_labels: Vec::new(),
    });

    let enabled: Vec<&StepConfig> = config.steps.iter().filter(|step| step.enabled).collect();
    let dependencies = resolve_dependencies(&enabled);
    let mut done = std::collections::HashSet::new();
    let mut remaining: Vec<usize> = (0..enabled.len()).collect();
    let mut schedule_index = 0usize;
    let mut parallel_number = 0usize;
    let mut sequential_number = 0usize;
    while !remaining.is_empty() {
        let ready = ready_indices(&remaining, &dependencies, &done);
        if ready.is_empty() {
            let stuck = remaining
                .iter()
                .map(|index| enabled[*index].label.as_str())
                .collect::<Vec<_>>();
            return Err(format!(
                "Pipeline plan has unsatisfiable dependencies: {}",
                stuck.join(", ")
            ));
        }
        schedule_index += 1;
        let ready_parallel = ready
            .iter()
            .copied()
            .filter(|index| enabled[*index].phase == Phase::Parallel)
            .collect::<Vec<_>>();
        if !ready_parallel.is_empty() {
            parallel_number += 1;
            let wave_steps = ready_parallel
                .iter()
                .map(|index| enabled[*index])
                .collect::<Vec<_>>();
            plan.push(ExecutionPlanStage {
                id: format!("wave-{schedule_index}-parallel"),
                kind: "dispatching".to_string(),
                label: "Parallel agent wave".to_string(),
                step_ids: wave_steps.iter().map(|step| step.id.clone()).collect(),
                step_labels: wave_steps.iter().map(|step| step.label.clone()).collect(),
            });
            let merged_step_ids = wave_steps
                .iter()
                .filter(|step| step.agents.len() > 1)
                .map(|step| step.id.clone())
                .collect::<Vec<_>>();
            if config.merge.enabled && !merged_step_ids.is_empty() {
                plan.push(ExecutionPlanStage {
                    id: format!("wave-{schedule_index}-merge"),
                    kind: "merging".to_string(),
                    label: format!("Merge parallel wave {parallel_number}"),
                    step_ids: merged_step_ids,
                    step_labels: wave_steps
                        .iter()
                        .filter(|step| step.agents.len() > 1)
                        .map(|step| step.label.clone())
                        .collect(),
                });
            }
            mark_steps_done(&mut done, &wave_steps);
            remaining.retain(|index| !ready_parallel.contains(index));
            continue;
        }

        let index = *ready.iter().min().expect("ready list is non-empty");
        let step = enabled[index];
        sequential_number += 1;
        plan.push(ExecutionPlanStage {
            id: format!("wave-{schedule_index}-sequential"),
            kind: "synthesizing".to_string(),
            label: "Sequential agent wave".to_string(),
            step_ids: vec![step.id.clone()],
            step_labels: vec![if step.label.is_empty() {
                format!("Sequential step {sequential_number}")
            } else {
                step.label.clone()
            }],
        });
        done.insert(step.id.clone());
        remaining.retain(|candidate| *candidate != index);
    }
    plan.push(ExecutionPlanStage {
        id: "done".to_string(),
        kind: "done".to_string(),
        label: "Complete".to_string(),
        step_ids: Vec::new(),
        step_labels: Vec::new(),
    });
    Ok(plan)
}

pub(super) fn ready_indices(
    remaining: &[usize],
    deps: &[std::collections::HashSet<String>],
    done: &std::collections::HashSet<String>,
) -> Vec<usize> {
    remaining
        .iter()
        .copied()
        .filter(|&i| deps[i].iter().all(|d| done.contains(d)))
        .collect()
}

pub(super) fn dependency_policy_failure(
    step: &StepConfig,
    successful: &std::collections::HashSet<String>,
) -> Option<String> {
    let missing_required = step
        .dependency_policy
        .required
        .iter()
        .filter(|dependency| !successful.contains(dependency.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let quorum_succeeded = step
        .dependency_policy
        .quorum
        .iter()
        .filter(|dependency| successful.contains(dependency.as_str()))
        .count();
    let quorum_minimum = step.dependency_policy.minimum_successes as usize;
    if missing_required.is_empty() && quorum_succeeded >= quorum_minimum {
        return None;
    }

    let mut reasons = Vec::new();
    if !missing_required.is_empty() {
        reasons.push(format!(
            "required dependencies did not succeed: {}",
            missing_required.join(", ")
        ));
    }
    if quorum_succeeded < quorum_minimum {
        let missing_quorum = step
            .dependency_policy
            .quorum
            .iter()
            .filter(|dependency| !successful.contains(dependency.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        reasons.push(format!(
            "dependency quorum was not met ({quorum_succeeded}/{quorum_minimum}); unavailable: {}",
            missing_quorum.join(", ")
        ));
    }
    Some(format!(
        "Step was blocked because {}; partial upstream reports remain available for resume.",
        reasons.join("; ")
    ))
}

pub(super) fn mark_steps_done(done: &mut std::collections::HashSet<String>, steps: &[&StepConfig]) {
    done.extend(steps.iter().map(|step| step.id.clone()));
}

/// Base id of a (possibly composite) step key: "technical/claude" → "technical".
pub(super) fn base_id(step_key: &str) -> &str {
    step_key.split('/').next().unwrap_or(step_key)
}

/// Delegate to the configuration module so validation and scheduling use the
/// exact same effective dependency graph.
pub(super) fn resolve_dependencies(
    enabled: &[&StepConfig],
) -> Vec<std::collections::HashSet<String>> {
    crate::pipeline_config::resolve_dependencies(enabled)
}

/// The set of enabled steps that transitively depend on any of `seeds`
/// (i.e. everything downstream of the seeds, seeds excluded). Used by resume to
/// decide what must re-run when some upstream steps are re-executed.
pub fn dependents_of(
    config: &PipelineConfig,
    seeds: &std::collections::HashSet<String>,
) -> std::collections::HashSet<String> {
    let enabled: Vec<&StepConfig> = config.steps.iter().filter(|s| s.enabled).collect();
    let deps = resolve_dependencies(&enabled);
    // Reverse edges: dep -> steps that depend on it.
    let mut dependents: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();
    for (i, s) in enabled.iter().enumerate() {
        for d in &deps[i] {
            dependents
                .entry(d.as_str())
                .or_default()
                .push(s.id.as_str());
        }
    }
    let mut out: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut stack: Vec<String> = seeds.iter().cloned().collect();
    while let Some(node) = stack.pop() {
        if let Some(children) = dependents.get(node.as_str()) {
            for c in children {
                if out.insert(c.to_string()) {
                    stack.push(c.to_string());
                }
            }
        }
    }
    out
}

/// Placeholder output for a step skipped by its run_if guard.
pub(super) fn skip_output(step: &StepConfig) -> StepOutput {
    StepOutput {
        step_id: step.id.clone(),
        step_label: step.label.clone(),
        phase: match step.phase {
            Phase::Parallel => "parallel".into(),
            Phase::Sequential => "sequential".into(),
        },
        raw_text: "_(skipped: run_if condition not met)_".to_string(),
        skipped: true,
        ..Default::default()
    }
}

pub(super) fn emit_skip(app: &crate::emit::EventBus, step: &StepConfig) {
    let _ = app.emit_event(
        "pipeline:pass",
        serde_json::json!({"name": step.id, "status": "skipped"}),
    );
    let _ = app.emit_event(
        "pipeline:log",
        serde_json::json!({ "line": format!("Step '{}' skipped (run_if condition not met)", step.label) }),
    );
}
