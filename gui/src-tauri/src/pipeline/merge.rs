use crate::models::StepOutput;
use crate::output::{capitalize, strip_to_report};
use crate::pipeline_config::MergeConfig;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

fn group_outputs(outputs: Vec<StepOutput>) -> Vec<(String, Vec<StepOutput>)> {
    let mut groups: Vec<(String, Vec<StepOutput>)> = Vec::new();
    let mut group_index: BTreeMap<String, usize> = BTreeMap::new();
    for output in outputs {
        // Non-merge outputs get a private key so even identical-looking IDs
        // cannot be collapsed accidentally.
        let group_id = if output.merge_group.is_empty() {
            format!("\0{}", groups.len())
        } else {
            output.merge_group.clone()
        };
        if let Some(&idx) = group_index.get(&group_id) {
            groups[idx].1.push(output);
        } else {
            let idx = groups.len();
            group_index.insert(group_id.clone(), idx);
            groups.push((group_id, vec![output]));
        }
    }
    groups
}

/// Merge multi-agent step outputs into single per-step outputs.
///
/// Groups only outputs carrying an explicit `merge_group`. This avoids
/// mistaking fan-out IDs for agent suffixes and permits one merge per fan-out
/// item when both features are enabled.
/// For groups with multiple agents, runs a parallel merge LLM call using
/// the merge prompt template. Single-agent outputs pass through unchanged.
///
/// When a merge fails, falls back to concatenated original agent outputs
/// rather than silently dropping the step.
///
/// Returns one StepOutput per step, in original config order.
pub async fn merge_step_outputs(
    app: &crate::emit::EventBus,
    outputs: Vec<StepOutput>,
    merge_config: &MergeConfig,
    semaphore: &Arc<Semaphore>,
    settings: &crate::settings::Settings,
) -> Result<Vec<StepOutput>, String> {
    // Validate merge prompt has required placeholders
    if !merge_config.prompt.contains("{topic}") || !merge_config.prompt.contains("{agent_reports}")
    {
        return Err(
            "Merge prompt is missing required placeholders ({topic} and/or {agent_reports})".into(),
        );
    }

    // Group by base step ID, preserving insertion order
    let groups = group_outputs(outputs);

    let total = groups.len();
    let mut results: Vec<Option<StepOutput>> = vec![None; total];
    // Save original multi-agent outputs for fallback on merge failure
    let mut originals: Vec<Option<Vec<StepOutput>>> = vec![None; total];
    type MergeError = (usize, String, crate::pipeline::logging::CallUsage, u64);
    let mut tasks: JoinSet<Result<(usize, StepOutput), MergeError>> = JoinSet::new();

    for (idx, (_base_id, group)) in groups.into_iter().enumerate() {
        if group.len() == 1 {
            // Single agent — pass through unchanged
            results[idx] = Some(group.into_iter().next().unwrap());
            continue;
        }

        // Save originals for fallback before building the merge task
        originals[idx] = Some(group.clone());

        // Multi-agent — build merge prompt and spawn task
        let topic = group[0].step_label.clone();

        let agent_reports_text: String = group
            .iter()
            .map(|o| format!("### Analysis by {}\n\n{}", capitalize(&o.agent), o.raw_text))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        let prompt = merge_config
            .prompt
            .replace("{topic}", &topic)
            .replace("{agent_reports}", &agent_reports_text);

        let agent_override = merge_config.agents.first().cloned();
        let agents_joined = group
            .iter()
            .map(|o| o.agent.clone())
            .collect::<Vec<_>>()
            .join("+");

        let base_id = group[0].merge_group.clone();
        let fan_out_item = group[0].fan_out_item.clone();
        let merge_key = format!("merge/{}", base_id);

        let app_handle = app.clone();
        let merge_key_done = merge_key.clone();
        let sem = semaphore.clone();
        let timeout = settings.step_timeout_secs.max(60);
        let run_settings = settings.clone();

        tasks.spawn(async move {
            let early_error = |error: String| {
                (
                    idx,
                    error,
                    crate::pipeline::logging::CallUsage::default(),
                    0,
                )
            };
            let cancellation_error = || {
                if crate::commands::is_cancelled() {
                    Some("Pipeline cancelled".to_string())
                } else if crate::commands::is_pass_cancelled(&merge_key_done) {
                    Some("Cancelled by user".to_string())
                } else {
                    None
                }
            };
            if let Some(error) = cancellation_error() {
                return Err(early_error(error));
            }
            let _permit = crate::commands::await_or_cancel(sem.acquire(), Some(&merge_key_done))
                .await
                .map_err(early_error)?
                .map_err(|_| early_error(format!("Semaphore closed during merge for {base_id}")))?;
            if let Some(error) = cancellation_error() {
                return Err(early_error(error));
            }
            let _ = app_handle.emit_event(
                "pipeline:pass",
                serde_json::json!({ "name": merge_key_done, "status": "running" }),
            );
            let log_label = format!("Merge: {}", topic);
            let agent_ref = agent_override.as_deref();
            let provider = agent_ref
                .unwrap_or(&run_settings.preferred_provider)
                .to_string();
            let resolution = crate::commands::await_or_cancel(
                crate::model_catalog::resolve(&provider, &run_settings, None),
                Some(&merge_key_done),
            )
            .await
            .map_err(early_error)?
            .map_err(|error| {
                early_error(format!(
                    "Model resolution failed for merge {base_id}: {error}"
                ))
            })?;
            if let Some(error) = cancellation_error() {
                return Err(early_error(error));
            }
            let effort = run_settings.model_effort(&provider).to_string();
            let call = super::call::execute(super::call::Request {
                app: &app_handle,
                pass_key: &merge_key_done,
                log_label: &log_label,
                prompt: &prompt,
                tools: &[],
                timeout_secs: timeout,
                agent: agent_ref,
                cwd: None,
                read_dirs: &[],
                write_dir: None,
                command_model: resolution.command_model.as_deref(),
                effort: &effort,
                settings: &run_settings,
            })
            .await;
            if let Some(error) = cancellation_error() {
                let _ = app_handle.emit_event(
                    "pipeline:pass",
                    serde_json::json!({ "name": merge_key_done, "status": "error" }),
                );
                return Err((idx, error, call.usage, call.duration_secs));
            }
            match call.output {
                Ok(raw_text) => {
                    let _ = app_handle.emit_event(
                        "pipeline:pass",
                        serde_json::json!({ "name": merge_key_done, "status": "done" }),
                    );
                    Ok((
                        idx,
                        StepOutput {
                            step_id: base_id,
                            step_label: topic,
                            fan_out_item,
                            phase: "parallel".to_string(),
                            provider,
                            agent: agents_joined,
                            raw_text: strip_to_report(&raw_text),
                            duration_secs: call.duration_secs,
                            input_tokens: call.usage.input_tokens,
                            output_tokens: call.usage.output_tokens,
                            attempt_count: 1,
                            model: resolution.resolved_model,
                            model_transport: resolution.transport,
                            model_policy: resolution.selection.label(),
                            model_source: resolution.source,
                            model_catalog_updated_at: resolution.catalog_updated_at,
                            ..Default::default()
                        },
                    ))
                }
                Err(e) => {
                    let _ = app_handle.emit_event(
                        "pipeline:pass",
                        serde_json::json!({ "name": merge_key_done, "status": "error" }),
                    );
                    Err((
                        idx,
                        format!("Merge for {base_id} failed: {e}"),
                        call.usage,
                        call.duration_secs,
                    ))
                }
            }
        });
    }

    // Collect merge results
    let mut errors = Vec::new();
    while let Some(res) = tasks.join_next().await {
        match res {
            Ok(Ok((idx, output))) => {
                results[idx] = Some(output);
            }
            Ok(Err(error)) => errors.push(error),
            Err(e) => errors.push((
                usize::MAX,
                format!("Merge task panicked: {e}"),
                Default::default(),
                0,
            )),
        }
    }

    if !errors.is_empty() {
        if let Some((_, error, _, _)) = errors
            .iter()
            .find(|(_, error, _, _)| error.to_ascii_lowercase().contains("cancelled"))
        {
            return Err(error.clone());
        }
        for (idx, err, merge_usage, merge_duration_secs) in &errors {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!("WARNING: merge failed: {err}") }),
            );
            // Fall back to concatenated original outputs instead of losing data
            if *idx < total {
                if let Some(original_outputs) = originals[*idx].take() {
                    let base_id = original_outputs[0].merge_group.clone();
                    let topic = original_outputs[0].step_label.clone();
                    let fan_out_item = original_outputs[0].fan_out_item.clone();
                    let agents_joined = original_outputs
                        .iter()
                        .map(|o| o.agent.clone())
                        .collect::<Vec<_>>()
                        .join("+");
                    let combined_text = format!(
                        "> **Note**: Multi-agent merge failed. Showing individual agent outputs.\n\n{}",
                        original_outputs
                            .iter()
                            .map(|o| {
                                format!(
                                    "### Analysis by {}\n\n{}",
                                    capitalize(&o.agent),
                                    &o.raw_text
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n\n---\n\n")
                    );
                    results[*idx] = Some(StepOutput {
                        step_id: base_id,
                        step_label: topic,
                        fan_out_item,
                        phase: "parallel".to_string(),
                        agent: agents_joined,
                        raw_text: combined_text,
                        duration_secs: *merge_duration_secs,
                        input_tokens: merge_usage.input_tokens,
                        output_tokens: merge_usage.output_tokens,
                        attempt_count: u32::from(
                            merge_usage.input_tokens > 0
                                || merge_usage.output_tokens > 0
                                || *merge_duration_secs > 0,
                        ),
                        ..Default::default()
                    });
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({ "line": format!(
                            "INFO: Using unmerged agent outputs for step '{}'",
                            results[*idx].as_ref().unwrap().step_label
                        )}),
                    );
                }
            }
        }

        let missing = results.iter().filter(|r| r.is_none()).count();
        if missing == total {
            return Err(format!(
                "All merges failed: {}",
                errors
                    .iter()
                    .map(|(_, e, _, _)| e.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
    }

    Ok(results.into_iter().flatten().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(id: &str, merge_group: &str) -> StepOutput {
        StepOutput {
            step_id: id.to_string(),
            merge_group: merge_group.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn slash_ids_do_not_imply_agent_merging() {
        let groups = group_outputs(vec![
            output("review/file_a", ""),
            output("review/file_b", ""),
        ]);
        assert_eq!(groups.len(), 2);
        assert!(groups.iter().all(|(_, group)| group.len() == 1));
    }

    #[test]
    fn agent_outputs_merge_only_within_their_item() {
        let groups = group_outputs(vec![
            output("review/a/claude", "review/a"),
            output("review/a/gemini", "review/a"),
            output("review/b/claude", "review/b"),
            output("review/b/gemini", "review/b"),
        ]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].0, "review/a");
        assert_eq!(groups[1].0, "review/b");
        assert!(groups.iter().all(|(_, group)| group.len() == 2));
    }
}
