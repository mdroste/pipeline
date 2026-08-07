use crate::models::{StepCallRecord, StepOutput};
use crate::output::{
    capitalize, extract_report_envelope, new_report_nonce, normalize_math_delimiters,
    report_output_format,
};
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

fn call_records_for_output(output: &StepOutput) -> Vec<StepCallRecord> {
    if !output.calls.is_empty() {
        return output.calls.clone();
    }

    vec![StepCallRecord {
        role: output.phase.clone(),
        provider: output.provider.clone(),
        agent: output.agent.clone(),
        model: output.model.clone(),
        model_transport: output.model_transport.clone(),
        model_policy: output.model_policy.clone(),
        model_source: output.model_source.clone(),
        model_catalog_updated_at: output.model_catalog_updated_at.clone(),
        duration_secs: output.duration_secs,
        input_tokens: output.input_tokens,
        output_tokens: output.output_tokens,
        cached_input_tokens: output.cached_input_tokens,
        cache_write_input_tokens: output.cache_write_input_tokens,
        model_round_trips: output.model_round_trips,
        tool_calls: output.tool_calls,
        attempt_count: output.attempt_count,
    }]
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
    type MergeError = (
        usize,
        String,
        crate::pipeline::logging::CallUsage,
        u64,
        Option<StepCallRecord>,
    );
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

        let limit = crate::safety::MAX_EXPANDED_PROMPT_BYTES;
        let mut agent_reports_text = String::new();
        for (report_index, output) in group.iter().enumerate() {
            if report_index > 0 {
                crate::safety::push_str_limited(
                    &mut agent_reports_text,
                    "\n\n---\n\n",
                    limit,
                    "Merge context",
                )?;
            }
            let capitalized_agent = capitalize(&output.agent);
            for value in [
                "### Analysis by ",
                capitalized_agent.as_str(),
                "\n\n",
                output.raw_text.as_str(),
            ] {
                crate::safety::push_str_limited(
                    &mut agent_reports_text,
                    value,
                    limit,
                    "Merge context",
                )?;
            }
        }

        let prompt = crate::safety::replace_all_limited(
            &merge_config.prompt,
            "{topic}",
            &topic,
            limit,
            "Merge prompt",
        )?;
        let prompt = crate::safety::replace_all_limited(
            &prompt,
            "{agent_reports}",
            &agent_reports_text,
            limit,
            "Merge prompt",
        )?;
        let report_nonce = new_report_nonce()?;
        let mut prompt = prompt;
        crate::safety::push_str_limited(&mut prompt, "\n\n", limit, "Merge prompt")?;
        crate::safety::push_str_limited(
            &mut prompt,
            &report_output_format(None, &report_nonce),
            limit,
            "Merge prompt",
        )?;
        let original_calls: Vec<StepCallRecord> =
            group.iter().flat_map(call_records_for_output).collect();
        let original_duration = original_calls
            .iter()
            .fold(0u64, |total, call| total.saturating_add(call.duration_secs));
        let original_input = original_calls
            .iter()
            .fold(0u64, |total, call| total.saturating_add(call.input_tokens));
        let original_output = original_calls
            .iter()
            .fold(0u64, |total, call| total.saturating_add(call.output_tokens));
        let original_cached = original_calls.iter().fold(0u64, |total, call| {
            total.saturating_add(call.cached_input_tokens)
        });
        let original_cache_write = original_calls.iter().fold(0u64, |total, call| {
            total.saturating_add(call.cache_write_input_tokens)
        });
        let original_model_round_trips = original_calls.iter().fold(0u64, |total, call| {
            total.saturating_add(call.model_round_trips)
        });
        let original_tool_calls = original_calls.iter().fold(
            crate::models::ToolCallCounts::default(),
            |mut total, call| {
                total.add_counts(call.tool_calls);
                total
            },
        );
        let original_attempts = original_calls
            .iter()
            .fold(0u32, |total, call| total.saturating_add(call.attempt_count));

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
                    None,
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
            let model_policy = resolution.selection.label();
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
                display_model: &resolution.resolved_model,
                model_policy: &model_policy,
                effort: &effort,
                settings: &run_settings,
                shared_context: None,
            })
            .await;
            let merge_call = StepCallRecord {
                role: "merge".to_string(),
                provider: provider.clone(),
                agent: provider.clone(),
                model: resolution.resolved_model.clone(),
                model_transport: resolution.transport.clone(),
                model_policy: resolution.selection.label(),
                model_source: resolution.source.clone(),
                model_catalog_updated_at: resolution.catalog_updated_at.clone(),
                duration_secs: call.duration_secs,
                input_tokens: call.usage.input_tokens,
                output_tokens: call.usage.output_tokens,
                cached_input_tokens: call.usage.cached_input_tokens,
                cache_write_input_tokens: call.usage.cache_write_input_tokens,
                model_round_trips: call.usage.model_round_trips,
                tool_calls: call.usage.tool_calls,
                attempt_count: 1,
            };
            if let Some(error) = cancellation_error() {
                let _ = app_handle.emit_event(
                    "pipeline:pass",
                    serde_json::json!({ "name": merge_key_done, "status": "error" }),
                );
                let mut failed_call = merge_call;
                failed_call.role = "failed_merge".to_string();
                return Err((
                    idx,
                    error,
                    call.usage,
                    call.duration_secs,
                    Some(failed_call),
                ));
            }
            match call.output {
                Ok(raw_text) => {
                    let report = match extract_report_envelope(&raw_text, &report_nonce) {
                        Ok(report) => report,
                        Err(error) => {
                            let _ = app_handle.emit_event(
                                "pipeline:pass",
                                serde_json::json!({ "name": merge_key_done, "status": "error" }),
                            );
                            let mut failed = merge_call;
                            failed.role = "failed_merge".to_string();
                            return Err((
                                idx,
                                format!("Merge for {base_id} returned an invalid report: {error}"),
                                call.usage,
                                call.duration_secs,
                                Some(failed),
                            ));
                        }
                    };
                    let _ = app_handle.emit_event(
                        "pipeline:pass",
                        serde_json::json!({ "name": merge_key_done, "status": "done" }),
                    );
                    let mut calls = original_calls;
                    calls.push(merge_call);
                    Ok((
                        idx,
                        StepOutput {
                            step_id: base_id,
                            step_label: topic,
                            fan_out_item,
                            phase: "parallel".to_string(),
                            provider,
                            agent: agents_joined,
                            raw_text: normalize_math_delimiters(&report),
                            duration_secs: original_duration.saturating_add(call.duration_secs),
                            input_tokens: original_input.saturating_add(call.usage.input_tokens),
                            output_tokens: original_output.saturating_add(call.usage.output_tokens),
                            cached_input_tokens: original_cached
                                .saturating_add(call.usage.cached_input_tokens),
                            cache_write_input_tokens: original_cache_write
                                .saturating_add(call.usage.cache_write_input_tokens),
                            model_round_trips: original_model_round_trips
                                .saturating_add(call.usage.model_round_trips),
                            tool_calls: {
                                let mut total = original_tool_calls;
                                total.add_counts(call.usage.tool_calls);
                                total
                            },
                            attempt_count: original_attempts.saturating_add(1),
                            model: resolution.resolved_model,
                            model_transport: resolution.transport,
                            model_policy: resolution.selection.label(),
                            model_source: resolution.source,
                            model_catalog_updated_at: resolution.catalog_updated_at,
                            calls,
                            ..Default::default()
                        },
                    ))
                }
                Err(e) => {
                    let _ = app_handle.emit_event(
                        "pipeline:pass",
                        serde_json::json!({ "name": merge_key_done, "status": "error" }),
                    );
                    let mut failed_call = merge_call;
                    failed_call.role = "failed_merge".to_string();
                    Err((
                        idx,
                        format!("Merge for {base_id} failed: {e}"),
                        call.usage,
                        call.duration_secs,
                        Some(failed_call),
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
                None,
            )),
        }
    }

    if !errors.is_empty() {
        if crate::commands::is_cancelled() {
            if let Some((_, error, _, _, _)) = errors
                .iter()
                .find(|(_, error, _, _, _)| error.to_ascii_lowercase().contains("cancelled"))
            {
                return Err(error.clone());
            }
        }
        for (idx, err, merge_usage, merge_duration_secs, merge_call) in &errors {
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
                    let mut combined_text = String::new();
                    crate::safety::push_str_limited(
                        &mut combined_text,
                        "> **Note**: Multi-agent merge failed. Showing individual agent outputs.\n\n",
                        crate::safety::MAX_RUN_OUTPUT_BYTES,
                        "Merged fallback output",
                    )?;
                    for (report_index, output) in original_outputs.iter().enumerate() {
                        if report_index > 0 {
                            crate::safety::push_str_limited(
                                &mut combined_text,
                                "\n\n---\n\n",
                                crate::safety::MAX_RUN_OUTPUT_BYTES,
                                "Merged fallback output",
                            )?;
                        }
                        let capitalized_agent = capitalize(&output.agent);
                        for value in [
                            "### Analysis by ",
                            capitalized_agent.as_str(),
                            "\n\n",
                            output.raw_text.as_str(),
                        ] {
                            crate::safety::push_str_limited(
                                &mut combined_text,
                                value,
                                crate::safety::MAX_RUN_OUTPUT_BYTES,
                                "Merged fallback output",
                            )?;
                        }
                    }
                    let mut calls: Vec<StepCallRecord> = original_outputs
                        .iter()
                        .flat_map(call_records_for_output)
                        .collect();
                    if let Some(merge_call) = merge_call {
                        calls.push(merge_call.clone());
                    } else if merge_usage.input_tokens > 0
                        || merge_usage.output_tokens > 0
                        || merge_usage.model_round_trips > 0
                        || !merge_usage.tool_calls.is_empty()
                        || *merge_duration_secs > 0
                    {
                        calls.push(StepCallRecord {
                            role: "failed_merge".to_string(),
                            duration_secs: *merge_duration_secs,
                            input_tokens: merge_usage.input_tokens,
                            output_tokens: merge_usage.output_tokens,
                            cached_input_tokens: merge_usage.cached_input_tokens,
                            cache_write_input_tokens: merge_usage.cache_write_input_tokens,
                            model_round_trips: merge_usage.model_round_trips,
                            tool_calls: merge_usage.tool_calls,
                            attempt_count: 1,
                            ..Default::default()
                        });
                    }
                    let duration_secs = calls
                        .iter()
                        .fold(0u64, |total, call| total.saturating_add(call.duration_secs));
                    let input_tokens = calls
                        .iter()
                        .fold(0u64, |total, call| total.saturating_add(call.input_tokens));
                    let output_tokens = calls
                        .iter()
                        .fold(0u64, |total, call| total.saturating_add(call.output_tokens));
                    let cached_input_tokens = calls.iter().fold(0u64, |total, call| {
                        total.saturating_add(call.cached_input_tokens)
                    });
                    let cache_write_input_tokens = calls.iter().fold(0u64, |total, call| {
                        total.saturating_add(call.cache_write_input_tokens)
                    });
                    let model_round_trips = calls.iter().fold(0u64, |total, call| {
                        total.saturating_add(call.model_round_trips)
                    });
                    let tool_calls = calls.iter().fold(
                        crate::models::ToolCallCounts::default(),
                        |mut total, call| {
                            total.add_counts(call.tool_calls);
                            total
                        },
                    );
                    let attempt_count = calls
                        .iter()
                        .fold(0u32, |total, call| total.saturating_add(call.attempt_count));
                    results[*idx] = Some(StepOutput {
                        step_id: base_id,
                        step_label: topic,
                        fan_out_item,
                        phase: "parallel".to_string(),
                        agent: agents_joined,
                        raw_text: combined_text,
                        duration_secs,
                        input_tokens,
                        output_tokens,
                        cached_input_tokens,
                        cache_write_input_tokens,
                        model_round_trips,
                        tool_calls,
                        attempt_count,
                        calls,
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
                    .map(|(_, e, _, _, _)| e.as_str())
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
