//! Cached-output compatibility and rerun selection.

use crate::models::PipelineReport;
use crate::pipeline::executor;

pub(in crate::commands) fn base_step_id(step_id: &str) -> String {
    step_id.split('/').next().unwrap_or(step_id).to_string()
}

pub(in crate::commands) fn failed_base_ids(
    failures: &[crate::models::StepFailure],
) -> std::collections::HashSet<String> {
    failures
        .iter()
        .map(|failure| base_step_id(&failure.step_id))
        .collect()
}

pub(in crate::commands) fn resume_seed_ids(
    enabled_ids: &[String],
    report: &PipelineReport,
) -> std::collections::HashSet<String> {
    let mut seeds = failed_base_ids(&report.failed_steps);
    let completed: std::collections::HashSet<String> = report
        .step_outputs
        .iter()
        .map(|output| base_step_id(&output.step_id))
        .collect();
    // Recovery reports use a synthetic run-level failure when the process
    // stopped without returning an ExecutionResult. Missing enabled steps are
    // therefore the authoritative restart point.
    seeds.extend(
        enabled_ids
            .iter()
            .filter(|step_id| !completed.contains(*step_id))
            .cloned(),
    );
    seeds
}

pub(in crate::commands) fn collect_preloaded_outputs(
    outputs: &[crate::models::StepOutput],
    rerun: &std::collections::HashSet<String>,
) -> std::collections::HashMap<String, Vec<crate::models::StepOutput>> {
    let mut preloaded = std::collections::HashMap::<String, Vec<crate::models::StepOutput>>::new();
    for output in outputs.iter().filter(|output| !output.skipped) {
        let base = base_step_id(&output.step_id);
        if !rerun.contains(&base) {
            preloaded.entry(base).or_default().push(output.clone());
        }
    }
    preloaded
}

pub(super) fn orientation_contract_error(
    config: &crate::pipeline_config::PipelineConfig,
    orientation: &serde_json::Value,
    input_mode: &str,
) -> Result<(), String> {
    let contract = crate::pipeline::orient::resolve_survey_schema(
        &config.orientation_prompt,
        config.orientation_schema.as_ref(),
        input_mode,
    )
    .unwrap_or_else(|| serde_json::json!({"type": "object"}));
    let resolved = crate::auto_review::resolve_schema_catalogs(&contract)?;
    crate::pipeline::structured::validate(&resolved, orientation)?;
    crate::auto_review::validate_contract_for_schema(&contract, orientation)?;
    Ok(())
}

/// Steps the parent deliberately skipped. They have no output to validate, and
/// that is not a contract failure: each one re-evaluates its guard when the
/// re-run reaches it, and its completed dependents stay reusable.
fn skipped_step_ids(outputs: &[crate::models::StepOutput]) -> std::collections::HashSet<String> {
    let ran: std::collections::HashSet<String> = outputs
        .iter()
        .filter(|output| !output.skipped)
        .map(|output| base_step_id(&output.step_id))
        .collect();
    outputs
        .iter()
        .filter(|output| output.skipped)
        .map(|output| base_step_id(&output.step_id))
        .filter(|id| !ran.contains(id))
        .collect()
}

pub(in crate::commands) fn incompatible_reuse_ids(
    config: &crate::pipeline_config::PipelineConfig,
    outputs: &[crate::models::StepOutput],
) -> std::collections::HashSet<String> {
    let skipped = skipped_step_ids(outputs);
    let mut by_base = std::collections::HashMap::<&str, Vec<&crate::models::StepOutput>>::new();
    for output in outputs.iter().filter(|output| !output.skipped) {
        by_base
            .entry(output.step_id.split('/').next().unwrap_or(&output.step_id))
            .or_default()
            .push(output);
    }
    config
        .steps
        .iter()
        .filter(|step| step.enabled && !skipped.contains(&step.id))
        .filter_map(|step| {
            let compatible = by_base.get(step.id.as_str()).is_some_and(|prior| {
                // A live schema reference must resolve before a cached
                // artifact can be judged against it; an unresolvable
                // reference simply reruns the step.
                let resolved_schema = step
                    .output_schema
                    .as_ref()
                    .map(crate::pipeline::structured::resolve_schema_reference);
                !prior.is_empty()
                    && prior.iter().all(|output| match resolved_schema.as_ref() {
                        Some(Ok(schema)) => {
                            output.structured_json
                                && crate::pipeline::structured::canonicalize(
                                    schema,
                                    &output.raw_text,
                                )
                                .is_ok()
                        }
                        Some(Err(_)) => false,
                        None => !output.structured_json && !output.raw_text.trim().is_empty(),
                    })
            });
            (!compatible).then(|| step.id.clone())
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub(in crate::commands) fn select_reusable_outputs(
    app: &crate::emit::EventBus,
    parent_run_id: &str,
    parent_dir: &std::path::Path,
    execution_config: &crate::pipeline_config::PipelineConfig,
    parent_report: &PipelineReport,
    orientation_rebuilt: bool,
    from_step: Option<String>,
    only_failed: bool,
) -> std::collections::HashMap<String, Vec<crate::models::StepOutput>> {
    // Determine which steps to re-run vs. reuse.
    let enabled_ids: Vec<String> = execution_config
        .steps
        .iter()
        .filter(|s| s.enabled)
        .map(|s| s.id.clone())
        .collect();
    let mut rerun: std::collections::HashSet<String> = if orientation_rebuilt {
        enabled_ids.iter().cloned().collect()
    } else if let Some(fs) = &from_step {
        match enabled_ids.iter().position(|id| id == fs) {
            Some(k) => {
                // Steps may be listed in any order while the executor schedules
                // by dependency graph, so a dependent of a re-run step can sit
                // earlier in the list. Extend the positional set with graph
                // dependents so no step is preloaded with stale upstream output.
                let seeds: std::collections::HashSet<String> =
                    enabled_ids[k..].iter().cloned().collect();
                let mut set = seeds.clone();
                set.extend(crate::pipeline::executor::dependents_of(
                    execution_config,
                    &seeds,
                ));
                set
            }
            None => enabled_ids.iter().cloned().collect(), // unknown step → full re-run
        }
    } else if only_failed {
        let seeds = resume_seed_ids(&enabled_ids, parent_report);
        let mut set = seeds.clone();
        set.extend(crate::pipeline::executor::dependents_of(
            execution_config,
            &seeds,
        ));
        set
    } else {
        enabled_ids.iter().cloned().collect()
    };

    // Even when the user asks for a narrow partial re-run, never preload an
    // artifact produced under a different output mode or one that fails the
    // step's current schema. Its graph dependents are stale as well.
    let mut incompatible = incompatible_reuse_ids(execution_config, &parent_report.step_outputs);
    let skipped = skipped_step_ids(&parent_report.step_outputs);
    // Missing retained producer trees cannot satisfy a downstream file selector.
    // A skipped producer never had one, so nothing was lost.
    for step in execution_config.steps.iter().filter(|s| s.enabled) {
        for selector in &step.context.include {
            if let crate::pipeline_config::ArtifactSelector::Step {
                step: producer,
                parts,
                ..
            } = selector
            {
                if parts.contains(&crate::pipeline_config::StepArtifactPart::Files)
                    && !skipped.contains(producer)
                    && !parent_dir
                        .join("artifacts/by-step")
                        .join(executor::step_slug(producer))
                        .is_dir()
                {
                    incompatible.insert(producer.clone());
                }
            }
        }
    }
    if !incompatible.is_empty() {
        rerun.extend(incompatible.iter().cloned());
        rerun.extend(crate::pipeline::executor::dependents_of(
            execution_config,
            &incompatible,
        ));
        app.emit_event("pipeline:log", serde_json::json!({
            "line": format!(
                "Re-run: invalidating {} cached step artifact(s) that do not satisfy the current output contracts",
                incompatible.len()
            )
        })).ok();
    }

    // Preload the reused steps' outputs (keyed by base id).
    let mut preloaded = collect_preloaded_outputs(&parent_report.step_outputs, &rerun);
    let enabled: std::collections::HashSet<&str> = enabled_ids.iter().map(String::as_str).collect();
    preloaded.retain(|id, _| enabled.contains(id.as_str()));
    app.emit_event("pipeline:log", serde_json::json!({
        "line": format!("Re-run of {parent_run_id}: reusing {} step(s), re-running the rest", preloaded.len())
    })).ok();

    preloaded
}
