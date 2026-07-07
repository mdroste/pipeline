use super::claude::{call_llm, LlmOverrides};
use crate::models::StepOutput;
use crate::output::{capitalize, strip_to_report};
use crate::pipeline_config::MergeConfig;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

/// Merge multi-agent step outputs into single per-step outputs.
///
/// Groups outputs by base step ID (stripping the "/agent" suffix).
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
) -> Result<Vec<StepOutput>, String> {
    // Validate merge prompt has required placeholders
    if !merge_config.prompt.contains("{topic}")
        || !merge_config.prompt.contains("{agent_reports}")
    {
        return Err(
            "Merge prompt is missing required placeholders ({topic} and/or {agent_reports})"
                .into(),
        );
    }

    // Group by base step ID, preserving insertion order
    let mut groups: Vec<(String, Vec<StepOutput>)> = Vec::new();
    let mut group_index: BTreeMap<String, usize> = BTreeMap::new();

    for output in outputs {
        let base_id = output
            .step_id
            .split('/')
            .next()
            .unwrap_or(&output.step_id)
            .to_string();
        if let Some(&idx) = group_index.get(&base_id) {
            groups[idx].1.push(output);
        } else {
            let idx = groups.len();
            group_index.insert(base_id.clone(), idx);
            groups.push((base_id, vec![output]));
        }
    }

    let total = groups.len();
    let mut results: Vec<Option<StepOutput>> = vec![None; total];
    // Save original multi-agent outputs for fallback on merge failure
    let mut originals: Vec<Option<Vec<StepOutput>>> = vec![None; total];
    let mut tasks: JoinSet<Result<(usize, StepOutput), (usize, String)>> = JoinSet::new();

    for (idx, (_base_id, group)) in groups.into_iter().enumerate() {
        if group.len() == 1 {
            // Single agent — pass through unchanged
            results[idx] = Some(group.into_iter().next().unwrap());
            continue;
        }

        // Save originals for fallback before building the merge task
        originals[idx] = Some(group.clone());

        // Multi-agent — build merge prompt and spawn task
        let topic = group[0]
            .step_label
            .split(" (")
            .next()
            .unwrap_or(&group[0].step_label)
            .to_string();

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

        let base_id = group[0]
            .step_id
            .split('/')
            .next()
            .unwrap_or(&group[0].step_id)
            .to_string();
        let merge_key = format!("merge/{}", base_id);

        let _ = app.emit_event(
            "pipeline:pass",
            serde_json::json!({ "name": merge_key, "status": "running" }),
        );

        let app_handle = app.clone();
        let merge_key_done = merge_key.clone();
        let sem = semaphore.clone();

        tasks.spawn(async move {
            let _permit = sem
                .acquire()
                .await
                .map_err(|_| (idx, format!("Semaphore closed during merge for {base_id}")))?;
            let log_label = format!("Merge: {}", topic);
            let agent_ref = agent_override.as_deref();
            let timeout = crate::settings::load().step_timeout_secs.max(60);
            match call_llm(
                &app_handle,
                &prompt,
                &[],
                None,
                "text",
                timeout,
                &log_label,
                agent_ref,
                None,
                &[],
                &LlmOverrides::default(),
            )
            .await
            {
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
                            phase: "parallel".to_string(),
                            agent: agents_joined,
                            raw_text: strip_to_report(&raw_text),
                            ..Default::default()
                        },
                    ))
                }
                Err(e) => {
                    let _ = app_handle.emit_event(
                        "pipeline:pass",
                        serde_json::json!({ "name": merge_key_done, "status": "error" }),
                    );
                    Err((idx, format!("Merge for {base_id} failed: {e}")))
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
            Ok(Err((idx, e))) => errors.push((idx, e)),
            Err(e) => errors.push((usize::MAX, format!("Merge task panicked: {e}"))),
        }
    }

    if !errors.is_empty() {
        for (idx, err) in &errors {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!("WARNING: merge failed: {err}") }),
            );
            // Fall back to concatenated original outputs instead of losing data
            if *idx < total {
                if let Some(original_outputs) = originals[*idx].take() {
                    let base_id = original_outputs[0]
                        .step_id
                        .split('/')
                        .next()
                        .unwrap_or(&original_outputs[0].step_id)
                        .to_string();
                    let topic = original_outputs[0]
                        .step_label
                        .split(" (")
                        .next()
                        .unwrap_or(&original_outputs[0].step_label)
                        .to_string();
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
                        phase: "parallel".to_string(),
                        agent: agents_joined,
                        raw_text: combined_text,
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
            return Err(format!("All merges failed: {}", errors.iter().map(|(_, e)| e.as_str()).collect::<Vec<_>>().join("; ")));
        }
    }

    Ok(results.into_iter().flatten().collect())
}

