//! Step products and schema-safe merge acceptance.
use super::step_call::StepCallResult;
use crate::models::StepOutput;
use crate::pipeline::provenance::{call_record, fallback_record};
use crate::pipeline_config::StepConfig;

#[allow(clippy::too_many_arguments)]
pub(super) fn step_output(
    step_key: &str,
    display_label: &str,
    phase: &str,
    provider: &str,
    agent: &str,
    call: StepCallResult,
    resolution: &crate::model_catalog::ResolvedModel,
    effort: &str,
    structured_json: bool,
) -> StepOutput {
    let fallback_usage = call.usage_limit_fallbacks.iter().fold(
        crate::pipeline::logging::CallUsage::default(),
        |mut total, fallback| {
            total.add_usage(fallback.fallback_usage);
            total
        },
    );
    let fallback_duration = call
        .usage_limit_fallbacks
        .iter()
        .fold(0u64, |total, fallback| {
            total.saturating_add(fallback.fallback_duration_secs)
        });
    let subtract = |total: u64, fallback: u64| total.saturating_sub(fallback);
    let primary_usage = crate::pipeline::logging::CallUsage {
        input_tokens: subtract(call.usage.input_tokens, fallback_usage.input_tokens),
        output_tokens: subtract(call.usage.output_tokens, fallback_usage.output_tokens),
        cached_input_tokens: subtract(
            call.usage.cached_input_tokens,
            fallback_usage.cached_input_tokens,
        ),
        cache_write_input_tokens: subtract(
            call.usage.cache_write_input_tokens,
            fallback_usage.cache_write_input_tokens,
        ),
        provider_attempts: subtract(
            call.usage.provider_attempts,
            fallback_usage.provider_attempts,
        ),
        model_round_trips: subtract(
            call.usage.model_round_trips,
            fallback_usage.model_round_trips,
        ),
        tool_calls: crate::models::ToolCallCounts {
            text_file: subtract(
                call.usage.tool_calls.text_file,
                fallback_usage.tool_calls.text_file,
            ),
            image: subtract(call.usage.tool_calls.image, fallback_usage.tool_calls.image),
            web: subtract(call.usage.tool_calls.web, fallback_usage.tool_calls.web),
            shell_or_other: subtract(
                call.usage.tool_calls.shell_or_other,
                fallback_usage.tool_calls.shell_or_other,
            ),
            unknown: subtract(
                call.usage.tool_calls.unknown,
                fallback_usage.tool_calls.unknown,
            ),
        },
    };
    let effective = call.effective_fallback.as_ref();
    let effective_provider = effective.map_or(provider, |fallback| fallback.provider.as_str());
    let effective_agent = effective.map_or(agent, |fallback| fallback.provider.as_str());
    let effective_resolution = effective.map_or(resolution, |fallback| &fallback.resolution);
    let mut call_records = vec![call_record(
        if effective.is_some() {
            "usage_limit_primary"
        } else {
            "step"
        },
        provider,
        agent,
        resolution,
        effort,
        call.duration_secs.saturating_sub(fallback_duration),
        primary_usage,
    )];
    call_records.extend(
        call.usage_limit_fallbacks
            .iter()
            .map(|fallback| fallback_record("usage_limit_fallback", fallback)),
    );
    StepOutput {
        step_id: step_key.to_string(),
        step_label: display_label.to_string(),
        phase: phase.to_string(),
        provider: effective_provider.to_string(),
        agent: effective_agent.to_string(),
        raw_text: call.text,
        structured_json,
        duration_secs: call.duration_secs,
        input_tokens: call.usage.input_tokens,
        output_tokens: call.usage.output_tokens,
        cached_input_tokens: call.usage.cached_input_tokens,
        cache_write_input_tokens: call.usage.cache_write_input_tokens,
        model_round_trips: call.usage.model_round_trips,
        tool_calls: call.usage.tool_calls,
        attempt_count: call.attempt_count,
        model: effective_resolution.resolved_model.clone(),
        model_transport: effective_resolution.transport.clone(),
        model_policy: effective_resolution.selection.label(),
        model_source: effective_resolution.source.clone(),
        model_catalog_updated_at: effective_resolution.catalog_updated_at.clone(),
        calls: call_records,
        ..Default::default()
    }
}

/// A merge report replaces schema-validated unit outputs, so it must satisfy
/// the producing step's `output_schema` too. A non-conforming merge falls back
/// to the first unit's already-valid output — never the concatenation banner,
/// which would also break JSON consumers. The fallback keeps every unit call
/// record, retains the merge call as `failed_merge` (matching the merge-failure
/// convention) so its spend stays visible, and takes the retained text's
/// provider/model provenance from the first unit.
pub(super) fn enforce_merge_output_schemas(
    app: &crate::emit::EventBus,
    steps: &[&StepConfig],
    unit_outputs: &[StepOutput],
    merged: &mut [StepOutput],
) {
    for output in merged {
        // A group replacement (synthesized merge or merge-failure banner
        // fallback) carries the merge group as its step_id — the step id, or
        // `{id}/{item}` for a fan-out item. Pass-through unit outputs never
        // match a merge group and were already validated by execute_step_call.
        let Some(first_unit) = unit_outputs
            .iter()
            .find(|unit| unit.merge_group == output.step_id)
        else {
            continue;
        };
        let Some(schema) = steps
            .iter()
            .find(|step| {
                step.id == output.step_id
                    || output
                        .step_id
                        .strip_prefix(&step.id)
                        .is_some_and(|rest| rest.starts_with('/'))
            })
            .and_then(|step| effective_output_schema(step).ok().flatten())
        else {
            continue;
        };
        let reason = if output.calls.iter().any(|call| call.role == "merge") {
            match crate::pipeline::structured::check(&schema, &output.raw_text) {
                Ok(()) => continue,
                Err(reason) => reason,
            }
        } else {
            // The merge already failed upstream; its banner concatenation is
            // never schema-shaped, so replace it without validating.
            "the merge failed and its concatenation fallback is not schema-shaped".to_string()
        };
        let _ = app.emit_event(
            "pipeline:log",
            serde_json::json!({ "line": format!(
                "WARNING: merged output for step '{}' did not satisfy the step's output schema ({reason}); keeping the first agent's validated output.",
                output.step_label
            )}),
        );
        output.raw_text = first_unit.raw_text.clone();
        output.structured_json = first_unit.structured_json;
        output.agent = first_unit.agent.clone();
        output.provider = first_unit.provider.clone();
        output.model = first_unit.model.clone();
        output.model_transport = first_unit.model_transport.clone();
        output.model_policy = first_unit.model_policy.clone();
        output.model_source = first_unit.model_source.clone();
        output.model_catalog_updated_at = first_unit.model_catalog_updated_at.clone();
        for call in &mut output.calls {
            if call.role == "merge" {
                call.role = "failed_merge".to_string();
            }
        }
    }
}

/// A step's dispatch-time output schema, with any live `x-pipeline-schema`
/// contract reference resolved. Saved profiles keep only the reference.
pub(super) fn effective_output_schema(
    step: &StepConfig,
) -> Result<Option<serde_json::Value>, String> {
    step.output_schema
        .as_ref()
        .map(|schema| {
            crate::pipeline::structured::resolve_schema_reference(schema)
                .map_err(|error| format!("Step '{}' output schema is invalid: {error}", step.id))
        })
        .transpose()
}
