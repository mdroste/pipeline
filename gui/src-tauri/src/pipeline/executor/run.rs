//! Run-level mutable execution state and completion ordering.
use super::artifact_context::{
    prepare_selected_shared_context, resolve_artifact_context, ArtifactRuntime,
};
use super::checkpoints::{
    checkpoint_failures, checkpoint_outputs, remove_checkpoints_from, OutputBudget,
};
use super::findings::collapse_findings_fan_out;
use super::outputs::{effective_output_schema, enforce_merge_output_schemas};
use super::parallel::run_parallel_wave;
use super::schedule::{
    base_id, dependency_policy_failure, emit_skip, mark_steps_done, ready_indices,
    resolve_dependencies, skip_output,
};
use super::sequential::run_sequential_step;
use super::step_call::is_cancellation_error;
use crate::models::{StepFailure, StepOutput};
use crate::pipeline::merge;
use crate::pipeline_config::{Phase, PipelineConfig, StepConfig};
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Result of pipeline execution: successful outputs and any step failures.
pub struct ExecutionResult {
    pub outputs: Vec<StepOutput>,
    pub failed_steps: Vec<StepFailure>,
}

/// Execute all enabled steps in the pipeline.
///
/// Steps run on an explicit dependency schedule: `after` contributes
/// order-only edges, while upstream step selectors contribute dataflow edges.
/// A step's `run_if` guard can skip it; a skipped step still "completes" so its
/// dependents proceed. A failed sequential step stops further execution but
/// does not discard prior outputs.
#[allow(clippy::too_many_arguments)]
pub async fn execute_steps(
    app: &crate::emit::EventBus,
    config: &PipelineConfig,
    orientation_path: &str,
    orientation_value: &serde_json::Value,
    paper_text_path: &str,
    document_bundle_path: &str,
    source_path: &str,
    paper_type: &str,
    survey_hint: &str,
    variables: &std::collections::HashMap<String, String>,
    extra_inputs: &std::collections::HashMap<String, String>,
    extra_input_sources: &std::collections::HashMap<String, String>,
    preloaded: &std::collections::HashMap<String, Vec<StepOutput>>,
    write_dir: Option<&str>,
    settings: &crate::settings::Settings,
) -> Result<ExecutionResult, String> {
    let semaphore = Arc::new(Semaphore::new(settings.max_workers.max(1) as usize));
    let output_budget = Arc::new(OutputBudget::default());
    let shared_context_pool = crate::pipeline::context_cache::PreparedContextPool::default();
    let mut all_outputs: Vec<StepOutput> = Vec::new();
    let mut failed_steps: Vec<StepFailure> = Vec::new();

    if let Some(plan) = orientation_value
        .pointer("/review_plan")
        .cloned()
        .and_then(|value| serde_json::from_value::<crate::models::ReviewPlan>(value).ok())
    {
        let subject_ids = plan
            .subject_specialist_ids
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let method_ids = plan
            .method_specialist_ids
            .iter()
            .map(String::as_str)
            .filter(|id| !id.is_empty())
            .collect::<Vec<_>>();
        let subject_labels = subject_ids
            .iter()
            .map(|id| crate::auto_review::label_for(id).unwrap_or(id).to_string())
            .collect::<Vec<_>>();
        let method_labels = method_ids
            .iter()
            .map(|id| crate::auto_review::label_for(id).unwrap_or(id).to_string())
            .collect::<Vec<_>>();
        let ids = subject_ids
            .iter()
            .chain(method_ids.iter())
            .copied()
            .collect::<Vec<_>>();
        let labels = ids
            .iter()
            .map(|id| crate::auto_review::label_for(id).unwrap_or(id).to_string())
            .collect::<Vec<_>>();
        let _ = app.emit_event(
            "pipeline:routing",
            serde_json::json!({
                "primaryDomain": plan.primary_domain,
                "subject": plan.subject,
                "subjectIds": subject_ids,
                "subjectLabels": subject_labels,
                "methodIds": method_ids,
                "methodLabels": method_labels,
                "specialistIds": ids,
                "specialistLabels": labels,
            }),
        );
        let _ = app.emit_event(
            "pipeline:log",
            serde_json::json!({
                "line": format!(
                    "Auto review detected {} / {}; selected {}",
                    plan.primary_domain,
                    plan.subject,
                    labels.join(", ")
                )
            }),
        );
    }

    let enabled: Vec<&StepConfig> = config.steps.iter().filter(|s| s.enabled).collect();
    let deps = resolve_dependencies(&enabled);

    // Producer steps whose structured artifact renders back to a readable
    // referee report when it enters downstream sequential context.
    let specialist_steps: std::collections::HashSet<String> = config
        .steps
        .iter()
        .filter(|step| {
            step.output_schema
                .as_ref()
                .is_some_and(crate::auto_review::is_specialist_schema)
        })
        .map(|step| step.id.clone())
        .collect();

    // Ids that have completed (including skipped) so dependents can start.
    let mut done: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Ids that produced a complete successful result. Unlike `done`, this
    // excludes skipped, failed, partially failed fan-out, and blocked steps.
    let mut successful: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut remaining: Vec<usize> = (0..enabled.len()).collect();
    let mut schedule_index = 0usize;
    let mut parallel_number = 0usize;
    let mut sequential_number = 0usize;

    while !remaining.is_empty() {
        let ready = ready_indices(&remaining, &deps, &done);
        if ready.is_empty() {
            let stuck: Vec<&str> = remaining
                .iter()
                .map(|&i| enabled[i].label.as_str())
                .collect();
            return Err(format!(
                "Pipeline stalled — steps have unsatisfiable dependencies (a cycle or a disabled upstream step): {}",
                stuck.join(", ")
            ));
        }
        schedule_index += 1;

        let ready_parallel: Vec<usize> = ready
            .iter()
            .copied()
            .filter(|&i| enabled[i].phase == Phase::Parallel)
            .collect();

        if !ready_parallel.is_empty() {
            parallel_number += 1;
            let wave_id = format!("wave-{schedule_index}-parallel");
            let wave_label = "Parallel agent wave";
            let wave_step_ids = ready_parallel
                .iter()
                .map(|index| enabled[*index].id.clone())
                .collect::<Vec<_>>();
            let wave_step_labels = ready_parallel
                .iter()
                .map(|index| enabled[*index].label.clone())
                .collect::<Vec<_>>();
            let merged_step_ids = ready_parallel
                .iter()
                .filter(|index| enabled[**index].agents.len() > 1)
                .map(|index| enabled[*index].id.clone())
                .collect::<Vec<_>>();
            let merged_step_labels = ready_parallel
                .iter()
                .filter(|index| enabled[**index].agents.len() > 1)
                .map(|index| enabled[*index].label.clone())
                .collect::<Vec<_>>();
            let planned_merge = config.merge.enabled && !merged_step_ids.is_empty();
            app.emit_event(
                "pipeline:stage",
                serde_json::json!({
                    "stage": "dispatching",
                    "id": wave_id,
                    "label": wave_label,
                    "stepIds": wave_step_ids,
                    "stepLabels": wave_step_labels,
                    "mergeStepIds": merged_step_ids,
                    "mergeStepLabels": merged_step_labels,
                }),
            )
            .ok();

            // Partition into steps whose run_if guard passes (dispatch) and
            // those it skips (record a placeholder so dependents proceed).
            let mut to_run: Vec<&StepConfig> = Vec::new();
            for &i in &ready_parallel {
                let step = enabled[i];
                if let Some(error) = dependency_policy_failure(step, &successful) {
                    let failure = StepFailure {
                        step_id: step.id.clone(),
                        step_label: step.label.clone(),
                        phase: "parallel".to_string(),
                        error,
                    };
                    let _ = app.emit_event(
                        "pipeline:pass",
                        serde_json::json!({"name": step.id, "status": "error"}),
                    );
                    checkpoint_failures(app, write_dir, std::slice::from_ref(&failure)).await?;
                    failed_steps.push(failure);
                    done.insert(step.id.clone());
                    continue;
                }
                // Resume: a preloaded step reuses the parent run's output.
                if let Some(cached) = preloaded.get(&step.id) {
                    for output in cached {
                        if let Err(error) = output_budget.reserve(output) {
                            let _ = app.emit_event(
                                "pipeline:pass",
                                serde_json::json!({"name": step.id, "status": "error"}),
                            );
                            return Err(error);
                        }
                        checkpoint_outputs(
                            app,
                            write_dir,
                            all_outputs.len(),
                            std::slice::from_ref(output),
                        )
                        .await?;
                        all_outputs.push(output.clone());
                    }
                    let _ = app.emit_event(
                        "pipeline:pass",
                        serde_json::json!({"name": step.id, "status": "done"}),
                    );
                    done.insert(step.id.clone());
                    if !cached.is_empty() {
                        successful.insert(step.id.clone());
                    }
                    continue;
                }
                if let Some(cond) = &step.run_if {
                    if !crate::pipeline::conditions::condition_met(
                        cond,
                        orientation_value,
                        &all_outputs,
                    ) {
                        emit_skip(app, step);
                        all_outputs.push(skip_output(step));
                        done.insert(step.id.clone());
                        continue;
                    }
                }
                to_run.push(step);
            }
            remaining.retain(|i| !ready_parallel.contains(i));

            if !to_run.is_empty() {
                let (mut wave_outputs, wave_failures) = run_parallel_wave(
                    app,
                    &to_run,
                    settings,
                    &semaphore,
                    orientation_path,
                    orientation_value,
                    paper_text_path,
                    document_bundle_path,
                    source_path,
                    paper_type,
                    survey_hint,
                    &config.parallel_context_template,
                    variables,
                    extra_inputs,
                    extra_input_sources,
                    &all_outputs,
                    write_dir,
                    &output_budget,
                    config.context_cache.enabled,
                    &shared_context_pool,
                )
                .await?;
                // Every dispatched step is terminal once its wave returns. A
                // failed unit and a fan-out with zero matching units both lack
                // a StepOutput, so deriving completion from outputs alone
                // leaves their dependents permanently blocked.
                let failed_bases = wave_failures
                    .iter()
                    .map(|failure| base_id(&failure.step_id))
                    .collect::<std::collections::HashSet<_>>();
                for step in &to_run {
                    let produced = wave_outputs
                        .iter()
                        .any(|output| !output.skipped && base_id(&output.step_id) == step.id);
                    if produced && !failed_bases.contains(step.id.as_str()) {
                        successful.insert(step.id.clone());
                    }
                }
                mark_steps_done(&mut done, &to_run);
                checkpoint_failures(app, write_dir, &wave_failures).await?;
                failed_steps.extend(wave_failures);

                let has_multi_agent = wave_outputs.iter().any(|o| !o.merge_group.is_empty());
                if planned_merge {
                    app.emit_event(
                        "pipeline:stage",
                        serde_json::json!({
                            "stage": "merging",
                            "id": format!("wave-{schedule_index}-merge"),
                            "label": format!("Merge parallel wave {parallel_number}"),
                            "stepIds": merged_step_ids,
                            "stepLabels": merged_step_labels,
                            "skipped": !has_multi_agent,
                        }),
                    )
                    .ok();
                }
                if has_multi_agent && config.merge.enabled {
                    let mut output_schemas = std::collections::BTreeMap::new();
                    for step in &to_run {
                        if let Some(schema) = effective_output_schema(step)? {
                            output_schemas.insert(step.id.clone(), schema);
                        }
                    }
                    match merge::merge_step_outputs(
                        app,
                        wave_outputs.clone(),
                        &config.merge,
                        &semaphore,
                        write_dir,
                        settings,
                        &output_schemas,
                    )
                    .await
                    {
                        Ok(mut merged) => {
                            enforce_merge_output_schemas(app, &to_run, &wave_outputs, &mut merged);
                            // `merged` holds clones of already-reserved
                            // pass-through outputs plus newly synthesized
                            // per-group merge reports; only the latter are new
                            // bytes. A merge result that would exceed the run
                            // budget degrades to the (already reserved)
                            // unmerged outputs instead of failing the run.
                            let already_reserved: std::collections::HashSet<&str> = wave_outputs
                                .iter()
                                .map(|output| output.raw_text.as_str())
                                .collect();
                            let mut reserve_failure = None;
                            for output in &merged {
                                if already_reserved.contains(output.raw_text.as_str()) {
                                    continue;
                                }
                                if let Err(error) = output_budget.reserve(output) {
                                    reserve_failure = Some(error);
                                    break;
                                }
                            }
                            match reserve_failure {
                                None => wave_outputs = merged,
                                Some(error) => {
                                    let _ = app.emit_event(
                                        "pipeline:log",
                                        serde_json::json!({ "line": format!(
                                            "WARNING: merged outputs exceed the run output budget ({error}); keeping the unmerged analyses."
                                        ) }),
                                    );
                                }
                            }
                        }
                        Err(e) if is_cancellation_error(&e) => return Err(e),
                        Err(e) => {
                            let _ = app.emit_event(
                                "pipeline:log",
                                serde_json::json!({ "line": format!("WARNING: merge failed: {e}. Using unmerged outputs.") }),
                            );
                        }
                    }
                }
                wave_outputs = collapse_findings_fan_out(
                    app,
                    &to_run,
                    &all_outputs,
                    wave_outputs,
                    &output_budget,
                );
                remove_checkpoints_from(write_dir, all_outputs.len()).await?;
                checkpoint_outputs(app, write_dir, all_outputs.len(), &wave_outputs).await?;
                all_outputs.extend(wave_outputs);
            } else if planned_merge {
                app.emit_event(
                    "pipeline:stage",
                    serde_json::json!({
                        "stage": "merging",
                        "id": format!("wave-{schedule_index}-merge"),
                        "label": format!("Merge parallel wave {parallel_number}"),
                        "stepIds": merged_step_ids,
                        "stepLabels": merged_step_labels,
                        "skipped": true,
                    }),
                )
                .ok();
            }
            continue;
        }

        // No parallel steps ready — run one sequential step (lowest index).
        let i = *ready.iter().min().unwrap();
        let step = enabled[i];
        remaining.retain(|&j| j != i);
        sequential_number += 1;
        app.emit_event(
            "pipeline:stage",
            serde_json::json!({
                "stage": "synthesizing",
                "id": format!("wave-{schedule_index}-sequential"),
                "label": "Sequential agent wave",
                "stepIds": [step.id.clone()],
                "stepLabels": [if step.label.is_empty() {
                    format!("Sequential step {sequential_number}")
                } else {
                    step.label.clone()
                }],
            }),
        )
        .ok();

        if let Some(error) = dependency_policy_failure(step, &successful) {
            let failure = StepFailure {
                step_id: step.id.clone(),
                step_label: step.label.clone(),
                phase: "sequential".to_string(),
                error,
            };
            let _ = app.emit_event(
                "pipeline:pass",
                serde_json::json!({"name": step.id, "status": "error"}),
            );
            checkpoint_failures(app, write_dir, std::slice::from_ref(&failure)).await?;
            failed_steps.push(failure);
            done.insert(step.id.clone());
            continue;
        }

        // Resume: a preloaded step reuses the parent run's output.
        if let Some(cached) = preloaded.get(&step.id) {
            for output in cached {
                if let Err(error) = output_budget.reserve(output) {
                    let _ = app.emit_event(
                        "pipeline:pass",
                        serde_json::json!({"name": step.id, "status": "error"}),
                    );
                    return Err(error);
                }
                checkpoint_outputs(
                    app,
                    write_dir,
                    all_outputs.len(),
                    std::slice::from_ref(output),
                )
                .await?;
                all_outputs.push(output.clone());
            }
            let _ = app.emit_event(
                "pipeline:pass",
                serde_json::json!({"name": step.id, "status": "done"}),
            );
            done.insert(step.id.clone());
            if !cached.is_empty() {
                successful.insert(step.id.clone());
            }
            continue;
        }

        if let Some(cond) = &step.run_if {
            if !crate::pipeline::conditions::condition_met(cond, orientation_value, &all_outputs) {
                emit_skip(app, step);
                all_outputs.push(skip_output(step));
                done.insert(step.id.clone());
                continue;
            }
        }

        let resolved = resolve_artifact_context(
            step,
            ArtifactRuntime {
                orientation_path,
                paper_text_path,
                document_bundle_path,
                source_path,
                extra_inputs,
                extra_input_sources,
                outputs: &all_outputs,
                run_artifact_dir: write_dir,
            },
        )?;
        let shared_context = prepare_selected_shared_context(
            app,
            config.context_cache.enabled,
            &resolved,
            orientation_value,
            &shared_context_pool,
        );
        match run_sequential_step(
            app,
            step,
            &resolved,
            if resolved.includes_survey {
                survey_hint
            } else {
                ""
            },
            variables,
            write_dir,
            settings,
            shared_context,
            &specialist_steps,
        )
        .await
        {
            Ok(output) => {
                output_budget.reserve(&output)?;
                checkpoint_outputs(
                    app,
                    write_dir,
                    all_outputs.len(),
                    std::slice::from_ref(&output),
                )
                .await?;
                let _ = app.emit_event(
                    "pipeline:pass",
                    serde_json::json!({"name": step.id, "status": "done"}),
                );
                done.insert(step.id.clone());
                successful.insert(step.id.clone());
                all_outputs.push(output);
            }
            Err(e) => {
                let _ = app.emit_event(
                    "pipeline:pass",
                    serde_json::json!({"name": step.id, "status": "error"}),
                );
                let _ = app.emit_event(
                    "pipeline:log",
                    serde_json::json!({ "line": format!("WARNING: step '{}' failed: {e}. Returning prior outputs.", step.label) }),
                );
                let failure = StepFailure {
                    step_id: step.id.clone(),
                    step_label: step.label.clone(),
                    phase: "sequential".to_string(),
                    error: e,
                };
                checkpoint_failures(app, write_dir, std::slice::from_ref(&failure)).await?;
                failed_steps.push(failure);
                break;
            }
        }
    }

    Ok(ExecutionResult {
        outputs: all_outputs,
        failed_steps,
    })
}
