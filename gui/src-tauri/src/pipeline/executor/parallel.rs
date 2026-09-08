//! Concurrent wave dispatch with per-unit selector isolation.
use super::artifact_context::{
    normalized_path, prepare_selected_shared_context, resolve_artifact_context, ArtifactRuntime,
};
use super::checkpoints::{checkpoint_outputs, OutputBudget};
use super::outputs::{effective_output_schema, step_output};
use super::paths::step_write_dir;
use super::prompts::{
    append_evidence_retrieval_guidance, append_shared_context_note, build_parallel_prompt,
    output_format_block, substitute_run_context, tools_with_write,
};
use super::step_call::{cancellation_error, execute_step_call, StepCallRequest};
use super::units::build_units;
use crate::models::{StepFailure, StepOutput};
use crate::output::{capitalize, new_report_nonce};
use crate::pipeline::claude::cli_parent_dir;
use crate::pipeline_config::StepConfig;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

pub(super) type ParallelTaskResult = Result<((usize, String), StepOutput), StepFailure>;

/// Run all parallel steps in a wave concurrently.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_parallel_wave(
    app: &crate::emit::EventBus,
    steps: &[&StepConfig],
    settings: &crate::settings::Settings,
    semaphore: &Arc<Semaphore>,
    orientation_path: &str,
    orientation_value: &serde_json::Value,
    paper_text_path: &str,
    document_bundle_path: &str,
    source_path: &str,
    paper_type: &str,
    survey_hint: &str,
    context_template: &str,
    variables: &std::collections::HashMap<String, String>,
    extra_inputs: &std::collections::HashMap<String, String>,
    extra_input_sources: &std::collections::HashMap<String, String>,
    prior_outputs: &[StepOutput],
    write_dir: Option<&str>,
    output_budget: &Arc<OutputBudget>,
    context_cache_enabled: bool,
    shared_context_pool: &crate::pipeline::context_cache::PreparedContextPool,
) -> Result<(Vec<StepOutput>, Vec<StepFailure>), String> {
    let mut tasks: JoinSet<ParallelTaskResult> = JoinSet::new();
    let mut immediate_results: Vec<((usize, String), StepOutput)> = Vec::new();

    for (idx, step) in steps.iter().enumerate() {
        let step_owned = (*step).clone();
        let settings_owned = settings.clone();
        let source_owned = source_path.to_string();
        let app_owned = app.clone();
        // Artifact fan-out reads its completed upstream product here, before
        // unit discovery moves onto a blocking thread.
        let artifact_source_owned = step
            .for_each
            .as_ref()
            .and_then(|for_each| for_each.artifact.as_ref())
            .and_then(|source| {
                prior_outputs
                    .iter()
                    .rev()
                    .find(|output| {
                        !output.skipped
                            && output.step_id.split('/').next().unwrap_or(&output.step_id)
                                == source.step
                    })
                    .map(|output| output.raw_text.clone())
            });
        let units = crate::commands::await_or_cancel(
            tokio::task::spawn_blocking(move || {
                build_units(
                    &step_owned,
                    &settings_owned,
                    &source_owned,
                    artifact_source_owned.as_deref(),
                    &app_owned,
                )
            }),
            Some(&step.id),
        )
        .await?
        .map_err(|e| format!("Fan-out discovery task failed for '{}': {e}", step.label))?
        .map_err(|e| format!("Fan-out discovery failed for '{}': {e}", step.label))?;
        if units.is_empty() {
            let _ = app.emit_event(
                "pipeline:pass",
                serde_json::json!({"name": step.id, "status": "skipped"}),
            );
            immediate_results.push((
                (idx, String::new()),
                StepOutput {
                    step_id: step.id.clone(),
                    step_label: step.label.clone(),
                    phase: "parallel".to_string(),
                    raw_text: "_(skipped: fan-out matched no files)_".to_string(),
                    skipped: true,
                    ..Default::default()
                },
            ));
            continue;
        }

        for unit in &units {
            let id = step.id.clone();
            let label = step.label.clone();
            let agent_name = unit.agent.clone();
            let model_selection = step.model_selection_for(settings, &agent_name);
            let effort_override = step.effort_for(settings, &agent_name);
            let output_schema = effective_output_schema(step)?;
            // Artifact fan-out records the element's display id, not the
            // element text, so manifests stay compact.
            let fan_out_item = unit
                .item
                .clone()
                .or_else(|| unit.inline_item.is_some().then(|| unit.display.clone()));
            let unit_lineage_ids = unit.lineage_ids.clone();
            let merge_group = if unit.merge_agents {
                if unit.item_suffix.is_empty() {
                    id.clone()
                } else {
                    format!("{}/{}", id, unit.item_suffix)
                }
            } else {
                String::new()
            };

            // Composite key when this is one of several units (multi-agent or
            // fan-out); the bare step id when it's a single plain run.
            let step_key = if unit.suffix.is_empty() {
                id.clone()
            } else {
                format!("{}/{}", id, unit.suffix)
            };
            // Publish every concrete provider/fan-out unit before it waits for
            // a worker permit. The progress view can then distinguish queued
            // work from a logical step that never expanded into provider calls.
            let _ = app.emit_event(
                "pipeline:pass",
                serde_json::json!({
                    "name": step_key,
                    "status": "pending"
                }),
            );

            let mut resolved = resolve_artifact_context(
                step,
                ArtifactRuntime {
                    orientation_path,
                    paper_text_path,
                    document_bundle_path,
                    source_path,
                    extra_inputs,
                    extra_input_sources,
                    outputs: prior_outputs,
                    run_artifact_dir: write_dir,
                },
            )?;
            // File fan-out stages the matched file into the artifact view and
            // binds its path; artifact fan-out binds the element text itself.
            let item_path = match unit.item.as_deref() {
                Some(item) => resolved.stage_current_item(item)?,
                None => unit.inline_item.clone().unwrap_or_default(),
            };
            let shared_context = prepare_selected_shared_context(
                app,
                context_cache_enabled,
                &resolved,
                orientation_value,
                shared_context_pool,
            );
            let run_artifact_dir = write_dir.map(str::to_string);
            let task_write_dir = step_write_dir(write_dir, &step_key)?;
            let tools = tools_with_write(
                &step.tools,
                !resolved.read_dirs.is_empty(),
                resolved.has_visuals,
                task_write_dir.as_deref(),
            );
            let report_rel = "report.md".to_string();
            let report_nonce = new_report_nonce()?;
            let output_format = output_format_block(
                task_write_dir.as_deref(),
                &report_nonce,
                output_schema.as_ref(),
            );
            let mut prompt = build_parallel_prompt(
                step,
                if resolved.includes_survey {
                    paper_type
                } else {
                    ""
                },
                &resolved.orientation_path,
                if resolved.includes_survey {
                    survey_hint
                } else {
                    ""
                },
                &resolved.paper_text_path,
                &resolved.document_bundle_path,
                &resolved.source_path,
                context_template,
                &output_format,
                resolved.artifact_root.as_deref(),
            )?;
            crate::safety::push_str_limited(
                &mut prompt,
                &format!("\n\n{}", resolved.manifest),
                crate::safety::MAX_EXPANDED_PROMPT_BYTES,
                "Parallel artifact context",
            )?;
            let prompt = substitute_run_context(&prompt, variables, &resolved.extra_inputs)?;
            // Fan-out: bind {item} to this unit's file (empty otherwise).
            let prompt = crate::safety::replace_all_limited(
                &prompt,
                "{item}",
                &item_path,
                crate::safety::MAX_EXPANDED_PROMPT_BYTES,
                "Parallel prompt",
            )?;
            let prompt = if shared_context.is_some() {
                append_shared_context_note(
                    prompt,
                    resolved.includes_primary_text,
                    resolved.includes_survey,
                )?
            } else {
                prompt
            };
            let prompt = append_evidence_retrieval_guidance(prompt, &tools)?;
            let task_read_dirs = resolved.read_dirs.clone();

            let app_handle = app.clone();
            let step_key_emit = step_key.clone();
            let log_label = if unit.suffix.is_empty() {
                format!("Step: {}", label)
            } else if unit.merge_agents {
                format!(
                    "Step: {} [{} · {}]",
                    label,
                    unit.display,
                    capitalize(&agent_name)
                )
            } else {
                format!("Step: {} [{}]", label, unit.display)
            };
            let display_label = if unit.suffix.is_empty() {
                label.clone()
            } else {
                format!("{} [{}]", label, unit.display)
            };
            let sort_key = (idx, unit.suffix.clone());
            let sem = semaphore.clone();
            let task_cwd = if resolved.source_path.is_empty() {
                Some(normalized_path(resolved._view.path()))
            } else {
                let source = std::path::Path::new(&resolved.source_path);
                if source.is_dir() {
                    Some(resolved.source_path.clone())
                } else {
                    cli_parent_dir(&resolved.source_path)
                }
            };
            // display_label is moved into the success StepOutput; keep a copy
            // for failure reporting.
            let fail_label = display_label.clone();
            let settings = settings.clone();
            let output_budget = output_budget.clone();
            // Keep the staged artifact view alive until the provider call and
            // all retries have completed.
            let context_view = resolved._view;
            let system_prompt = step.system_prompt.clone();

            tasks.spawn(async move {
                let _context_view = context_view;
                if let Some(error) = cancellation_error(&step_key_emit) {
                    return Err(StepFailure {
                        step_id: step_key_emit.clone(),
                        step_label: fail_label.clone(),
                        phase: "parallel".to_string(),
                        error,
                    });
                }
                let _permit = crate::commands::await_or_cancel(sem.acquire(), Some(&step_key_emit))
                    .await
                    .map_err(|error| StepFailure {
                        step_id: step_key_emit.clone(),
                        step_label: fail_label.clone(),
                        phase: "parallel".to_string(),
                        error,
                    })?
                    .map_err(|_| StepFailure {
                        step_id: step_key_emit.clone(),
                        step_label: fail_label.clone(),
                        phase: "parallel".to_string(),
                        error: "Semaphore closed".to_string(),
                    })?;
                // Cancellation can happen while this unit is queued for the
                // worker permit. Re-check after acquisition before starting a
                // provider call (and therefore before incurring cost).
                if let Some(error) = cancellation_error(&step_key_emit) {
                    return Err(StepFailure {
                        step_id: step_key_emit.clone(),
                        step_label: fail_label.clone(),
                        phase: "parallel".to_string(),
                        error,
                    });
                }
                let _ = app_handle.emit_event(
                    "pipeline:pass",
                    serde_json::json!({
                        "name": step_key_emit,
                        "status": "running"
                    }),
                );
                let provider = agent_name.clone();
                let resolution = crate::commands::await_or_cancel(
                    crate::model_catalog::resolve(&provider, &settings, model_selection.as_ref()),
                    Some(&step_key_emit),
                )
                .await
                .map_err(|error| StepFailure {
                    step_id: step_key_emit.clone(),
                    step_label: fail_label.clone(),
                    phase: "parallel".to_string(),
                    error,
                })?
                .map_err(|error| StepFailure {
                    step_id: step_key_emit.clone(),
                    step_label: fail_label.clone(),
                    phase: "parallel".to_string(),
                    error,
                })?;
                let model_policy = resolution.selection.label();
                let call = execute_step_call(StepCallRequest {
                    app: &app_handle,
                    pass_key: &step_key_emit,
                    log_label: &log_label,
                    prompt: &prompt,
                    system_prompt: (!system_prompt.is_empty()).then_some(system_prompt.as_str()),
                    tools: &tools,
                    agent: Some(&agent_name),
                    cwd: task_cwd.as_deref(),
                    read_dirs: &task_read_dirs,
                    run_artifact_dir: run_artifact_dir.as_deref(),
                    write_dir: task_write_dir.as_deref(),
                    report_rel: &report_rel,
                    report_nonce: &report_nonce,
                    output_schema: output_schema.as_ref(),
                    preserved_finding_ids: unit_lineage_ids.clone(),
                    command_model: resolution.command_model.as_deref(),
                    display_model: &resolution.resolved_model,
                    model_policy: &model_policy,
                    effort: &effort_override,
                    settings: &settings,
                    shared_context,
                })
                .await;

                match call {
                    Ok(call) => {
                        let mut output = step_output(
                            &step_key,
                            &display_label,
                            "parallel",
                            &provider,
                            &agent_name,
                            call,
                            &resolution,
                            &effort_override,
                            output_schema.is_some(),
                        );
                        output.merge_group = merge_group;
                        output.fan_out_item = fan_out_item;
                        if let Err(error) = output_budget.reserve(&output) {
                            let _ = app_handle.emit_event(
                                "pipeline:pass",
                                serde_json::json!({ "name": step_key_emit, "status": "error" }),
                            );
                            return Err(StepFailure {
                                step_id: step_key.clone(),
                                step_label: fail_label.clone(),
                                phase: "parallel".to_string(),
                                error,
                            });
                        }
                        let _ = app_handle.emit_event(
                            "pipeline:pass",
                            serde_json::json!({ "name": step_key_emit, "status": "done" }),
                        );
                        Ok((sort_key, output))
                    }
                    Err(error) => {
                        let _ = app_handle.emit_event(
                            "pipeline:pass",
                            serde_json::json!({ "name": step_key_emit, "status": "error" }),
                        );
                        Err(StepFailure {
                            step_id: step_key,
                            step_label: fail_label,
                            phase: "parallel".to_string(),
                            error,
                        })
                    }
                }
            });
        }
    }

    let mut results = immediate_results;
    let mut failures: Vec<StepFailure> = Vec::new();

    while let Some(res) = tasks.join_next().await {
        match res {
            Ok(Ok(report)) => {
                // Durably checkpoint each unit as it completes, so a crash
                // later in the wave cannot lose analyses that already
                // finished. The completion-order ordinal is provisional: the
                // caller clears this wave's ordinal range and rewrites it in
                // final order once the wave (and any merge) settles.
                checkpoint_outputs(
                    app,
                    write_dir,
                    prior_outputs.len() + results.len(),
                    std::slice::from_ref(&report.1),
                )
                .await?;
                results.push(report);
            }
            Ok(Err(failure)) => failures.push(failure),
            Err(e) => failures.push(StepFailure {
                step_id: "internal".to_string(),
                step_label: "Internal task".to_string(),
                phase: "parallel".to_string(),
                error: format!("Task panicked: {e}"),
            }),
        }
    }

    if !failures.is_empty() {
        for f in &failures {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!("WARNING: step failed: {}: {}", f.step_label, f.error) }),
            );
        }
        let _ = app.emit_event(
            "pipeline:log",
            serde_json::json!({ "line": format!(
                "WARNING: {}/{} parallel steps succeeded. Downstream steps will receive incomplete inputs.",
                results.len(), results.len() + failures.len()
            )}),
        );
    }

    // Sort by original config order, then agent name
    results.sort_by(|a, b| a.0.cmp(&b.0));
    Ok((results.into_iter().map(|(_, r)| r).collect(), failures))
}
