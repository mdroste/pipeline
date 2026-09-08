//! Ordered finding lineage, validation dispositions and fan-out collapse.
use super::checkpoints::OutputBudget;
use super::outputs::effective_output_schema;
use super::units::{artifact_fan_out_elements, artifact_item_suffix};
use crate::models::StepOutput;
use crate::pipeline_config::StepConfig;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PreservedFinding {
    pub(super) id: String,
    pub(super) before_hash: String,
}

/// Ordered finding ids of a canonical `{findings: [...]}` artifact. Returns
/// None when the text is not findings-shaped JSON.
pub(super) fn finding_ids(text: &str) -> Option<Vec<String>> {
    let value = serde_json::from_str::<serde_json::Value>(text.trim()).ok()?;
    let findings = value.get("findings")?.as_array()?;
    Some(
        findings
            .iter()
            .map(|finding| {
                finding
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            })
            .collect(),
    )
}

pub(super) fn normalized_finding_hash(value: &serde_json::Value) -> Result<String, String> {
    use sha2::{Digest as _, Sha256};

    fn sort(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(object) => {
                object.remove("rank");
                let mut entries = std::mem::take(object).into_iter().collect::<Vec<_>>();
                entries.sort_by(|(left, _), (right, _)| left.cmp(right));
                for (key, mut child) in entries {
                    sort(&mut child);
                    object.insert(key, child);
                }
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(sort),
            _ => {}
        }
    }

    let mut normalized = value.clone();
    sort(&mut normalized);
    let bytes = serde_json::to_vec(&normalized)
        .map_err(|error| format!("could not hash finding: {error}"))?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

pub(super) fn finding_lineage(text: &str) -> Option<Vec<PreservedFinding>> {
    let value = serde_json::from_str::<serde_json::Value>(text.trim()).ok()?;
    let findings = value.get("findings")?.as_array()?;
    findings
        .iter()
        .map(|finding| {
            Some(PreservedFinding {
                id: finding.get("id")?.as_str()?.to_string(),
                before_hash: normalized_finding_hash(finding).ok()?,
            })
        })
        .collect()
}

pub(super) fn preserved_finding_ids(
    step_id: &str,
    output_schema: Option<&serde_json::Value>,
    prior_outputs: &[StepOutput],
) -> Result<Option<Vec<PreservedFinding>>, String> {
    let Some(target) = output_schema
        .and_then(|schema| schema.get(crate::pipeline::structured::PRESERVE_FINDINGS_KEY))
        .and_then(serde_json::Value::as_str)
    else {
        return Ok(None);
    };
    let output = prior_outputs
        .iter()
        .rev()
        .find(|output| {
            !output.skipped
                && output.step_id.split('/').next().unwrap_or(&output.step_id) == target
        })
        .ok_or_else(|| {
            format!(
                "Step '{step_id}' preserves findings from '{target}', but that required artifact is unavailable"
            )
        })?;
    let ids = finding_lineage(&output.raw_text).ok_or_else(|| {
        format!(
            "Step '{step_id}' preserves findings from '{target}', but that artifact is not canonical findings JSON"
        )
    })?;
    Ok(Some(ids))
}

/// Host-owned lineage invariant for findings-filtering steps: every returned
/// finding must keep its exact input id, in the input's relative order, with
/// no additions. This is the contract the validate prompt states; enforcing it
/// here keeps id lineage (annotations, the Projects ledger) intact even when a
/// model renumbers or invents findings.
pub(super) fn check_finding_lineage(
    expected: &[PreservedFinding],
    canonical: &str,
) -> Result<(), String> {
    let Some(actual) = finding_ids(canonical) else {
        return Err("the response is not a findings artifact, so its finding ids cannot be verified against the upstream product".to_string());
    };
    let mut remaining = expected.iter().map(|finding| &finding.id);
    for id in &actual {
        if !remaining.any(|candidate| candidate == id) {
            return Err(format!(
                "finding id '{id}' does not continue the upstream findings product in order; \
                 keep every surviving finding's exact input id and original order, and do not add findings"
            ));
        }
    }
    Ok(())
}

pub(super) fn check_validation_dispositions(
    expected: &[PreservedFinding],
    canonical: &str,
) -> Result<String, String> {
    let mut value = serde_json::from_str::<serde_json::Value>(canonical)
        .map_err(|error| format!("validation artifact is not valid JSON: {error}"))?;
    let output_findings = value
        .get("findings")
        .and_then(serde_json::Value::as_array)
        .ok_or("validation artifact has no findings array")?;
    let output_by_id = output_findings
        .iter()
        .filter_map(|finding| Some((finding.get("id")?.as_str()?.to_string(), finding.clone())))
        .collect::<std::collections::HashMap<_, _>>();
    let dispositions = value
        .get_mut("validation_dispositions")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or("validation artifact has no validation_dispositions array")?;
    if dispositions.len() != expected.len() {
        return Err(format!(
            "validation ledger must contain exactly one disposition for each of {} input findings; it contains {}",
            expected.len(),
            dispositions.len()
        ));
    }

    let mut seen = std::collections::HashSet::new();
    for (index, (entry, input)) in dispositions.iter_mut().zip(expected).enumerate() {
        let entry = entry
            .as_object_mut()
            .ok_or_else(|| format!("validation_dispositions[{index}] must be an object"))?;
        let id = entry
            .get("finding_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if id != input.id {
            return Err(format!(
                "validation_dispositions[{index}].finding_id must be '{}' to cover every input finding in order",
                input.id
            ));
        }
        if !seen.insert(id.to_string()) {
            return Err(format!("validation ledger repeats finding id '{id}'"));
        }
        let disposition = entry
            .get("disposition")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let output = output_by_id.get(id);
        match disposition {
            "retained" | "revised" => {
                let output = output.ok_or_else(|| {
                    format!("finding '{id}' is marked {disposition} but is absent from findings")
                })?;
                let after_hash = normalized_finding_hash(output)?;
                if disposition == "retained" && after_hash != input.before_hash {
                    return Err(format!(
                        "finding '{id}' changed but is marked retained; use revised"
                    ));
                }
                if disposition == "revised" && after_hash == input.before_hash {
                    return Err(format!(
                        "finding '{id}' is unchanged but is marked revised; use retained"
                    ));
                }
                entry.insert(
                    "after_hash".to_string(),
                    serde_json::Value::String(after_hash),
                );
            }
            "merged_into" => {
                if output.is_some() {
                    return Err(format!(
                        "finding '{id}' is marked merged_into but still appears in findings"
                    ));
                }
                let target = entry
                    .get("merged_into")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                if target == id || !output_by_id.contains_key(target) {
                    return Err(format!(
                        "finding '{id}' must merge into a different surviving finding id"
                    ));
                }
            }
            "rejected_false_positive"
            | "unverified_missing_evidence"
            | "deferred_manual_review" => {
                if output.is_some() {
                    return Err(format!(
                        "finding '{id}' is marked {disposition} but still appears in findings"
                    ));
                }
            }
            _ => return Err(format!("finding '{id}' has an unsupported disposition")),
        }
        entry.insert(
            "before_hash".to_string(),
            serde_json::Value::String(input.before_hash.clone()),
        );
    }
    serde_json::to_string_pretty(&value)
        .map_err(|error| format!("failed to publish validation ledger: {error}"))
}

/// Deterministically reassemble a findings-preserving artifact fan-out. Each
/// unit judged exactly one upstream element, so the step's product is the
/// ordered concatenation of unit results — no merge model involved. An
/// element whose unit failed or returned an unusable product keeps its
/// original, unvalidated finding rather than silently disappearing.
pub(super) fn collapse_findings_fan_out(
    app: &crate::emit::EventBus,
    steps: &[&StepConfig],
    upstream_outputs: &[StepOutput],
    mut outputs: Vec<StepOutput>,
    output_budget: &Arc<OutputBudget>,
) -> Vec<StepOutput> {
    for step in steps {
        let Some(for_each) = &step.for_each else {
            continue;
        };
        let Some(source) = &for_each.artifact else {
            continue;
        };
        let Ok(Some(schema)) = effective_output_schema(step) else {
            continue;
        };
        if schema
            .get(crate::pipeline::structured::PRESERVE_FINDINGS_KEY)
            .and_then(serde_json::Value::as_str)
            != Some(source.step.as_str())
        {
            continue;
        }
        let Some(source_text) = upstream_outputs
            .iter()
            .rev()
            .find(|output| {
                !output.skipped
                    && output.step_id.split('/').next().unwrap_or(&output.step_id) == source.step
            })
            .map(|output| output.raw_text.clone())
        else {
            continue;
        };
        let Ok(elements) =
            artifact_fan_out_elements(step, source, for_each, Some(&source_text), app)
        else {
            continue;
        };
        if elements.is_empty() {
            continue;
        }

        let (units, mut kept): (Vec<StepOutput>, Vec<StepOutput>) =
            outputs.into_iter().partition(|output| {
                output.step_id.split('/').next().unwrap_or(&output.step_id) == step.id
            });

        let mut combined = Vec::new();
        let mut unvalidated = 0usize;
        for (index, element) in elements.iter().enumerate() {
            let key = format!("{}/{}", step.id, artifact_item_suffix(index));
            // Exact key first (single agent, or a merged multi-agent group);
            // otherwise the lexically first per-agent unit for determinism.
            let unit = units
                .iter()
                .filter(|unit| {
                    !unit.skipped
                        && (unit.step_id == key
                            || unit
                                .step_id
                                .strip_prefix(&key)
                                .is_some_and(|rest| rest.starts_with('/')))
                })
                .min_by(|a, b| a.step_id.cmp(&b.step_id));
            let unit_findings = unit.and_then(|unit| {
                serde_json::from_str::<serde_json::Value>(unit.raw_text.trim())
                    .ok()?
                    .get("findings")?
                    .as_array()
                    .cloned()
            });
            match unit_findings {
                Some(findings) => combined.extend(findings),
                None => {
                    if element.is_object() {
                        combined.push(element.clone());
                        unvalidated += 1;
                    }
                }
            }
        }
        if unvalidated > 0 {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!(
                    "WARNING: {unvalidated} of {} fan-out unit(s) for step '{}' produced no usable verdict; their original findings are kept unvalidated.",
                    elements.len(),
                    step.label
                )}),
            );
        }
        let combined_value = serde_json::json!({ "findings": combined });
        let raw_text = match serde_json::to_string_pretty(&combined_value) {
            Ok(text) => text,
            Err(error) => {
                let _ = app.emit_event(
                    "pipeline:log",
                    serde_json::json!({ "line": format!(
                        "WARNING: could not assemble fan-out findings for step '{}' ({error}); keeping per-unit outputs.",
                        step.label
                    )}),
                );
                kept.extend(units);
                outputs = kept;
                continue;
            }
        };

        let mut assembled = StepOutput {
            step_id: step.id.clone(),
            step_label: step.label.clone(),
            phase: "parallel".to_string(),
            raw_text,
            structured_json: true,
            ..Default::default()
        };
        for unit in &units {
            assembled.duration_secs = assembled.duration_secs.saturating_add(unit.duration_secs);
            assembled.input_tokens = assembled.input_tokens.saturating_add(unit.input_tokens);
            assembled.output_tokens = assembled.output_tokens.saturating_add(unit.output_tokens);
            assembled.cached_input_tokens = assembled
                .cached_input_tokens
                .saturating_add(unit.cached_input_tokens);
            assembled.cache_write_input_tokens = assembled
                .cache_write_input_tokens
                .saturating_add(unit.cache_write_input_tokens);
            assembled.model_round_trips = assembled
                .model_round_trips
                .saturating_add(unit.model_round_trips);
            assembled.tool_calls.add_counts(unit.tool_calls);
            assembled.attempt_count = assembled.attempt_count.saturating_add(unit.attempt_count);
            assembled.calls.extend(unit.calls.iter().cloned());
        }
        if let Some(first) = units.first() {
            assembled.agent = first.agent.clone();
            assembled.provider = first.provider.clone();
            assembled.model = first.model.clone();
            assembled.model_transport = first.model_transport.clone();
            assembled.model_policy = first.model_policy.clone();
            assembled.model_source = first.model_source.clone();
            assembled.model_catalog_updated_at = first.model_catalog_updated_at.clone();
        }
        if let Err(error) = output_budget.reserve(&assembled) {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!(
                    "WARNING: assembled fan-out findings for step '{}' exceed the run output budget ({error}); keeping per-unit outputs.",
                    step.label
                )}),
            );
            kept.extend(units);
            outputs = kept;
            continue;
        }
        let _ = app.emit_event(
            "pipeline:log",
            serde_json::json!({ "line": format!(
                "Assembled {} finding(s) from {} fan-out unit(s) for step '{}'.",
                combined_value["findings"].as_array().map_or(0, Vec::len),
                elements.len(),
                step.label
            )}),
        );
        kept.push(assembled);
        outputs = kept;
    }
    outputs
}
