//! Stable call-record construction shared by unit execution and merging.
//! Callers retain their own accounting and explicitly choose product roles.
use super::{call::UsageLimitFallback, logging::CallUsage};
use crate::model_catalog::ResolvedModel;
use crate::models::{StepCallRecord, StepOutput};

pub(super) fn call_record(
    role: &str,
    provider: &str,
    agent: &str,
    resolution: &ResolvedModel,
    effort: &str,
    duration_secs: u64,
    usage: CallUsage,
) -> StepCallRecord {
    StepCallRecord {
        role: role.to_string(),
        provider: provider.to_string(),
        agent: agent.to_string(),
        model: resolution.resolved_model.clone(),
        model_transport: resolution.transport.clone(),
        model_policy: resolution.selection.label(),
        model_source: resolution.source.clone(),
        model_catalog_updated_at: resolution.catalog_updated_at.clone(),
        effort: if effort.trim().is_empty() {
            "default".to_string()
        } else {
            effort.to_string()
        },
        duration_secs,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cached_input_tokens: usage.cached_input_tokens,
        cache_write_input_tokens: usage.cache_write_input_tokens,
        model_round_trips: usage.model_round_trips,
        tool_calls: usage.tool_calls,
        attempt_count: u32::try_from(usage.provider_attempts).unwrap_or(u32::MAX),
    }
}

pub(super) fn fallback_record(role: &str, fallback: &UsageLimitFallback) -> StepCallRecord {
    call_record(
        role,
        &fallback.provider,
        &fallback.provider,
        &fallback.resolution,
        &fallback.effort,
        fallback.fallback_duration_secs,
        fallback.fallback_usage,
    )
}

pub(super) fn call_records_for_output(output: &StepOutput) -> Vec<StepCallRecord> {
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
        effort: String::new(),
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

#[cfg(test)]
mod tests;
