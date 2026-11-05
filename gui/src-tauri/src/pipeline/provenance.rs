//! Stable call-record construction shared by unit execution and merging.
//! Callers retain their own accounting and explicitly choose product roles.
use super::{call::UsageLimitFallback, logging::CallUsage};
use crate::model_catalog::ResolvedModel;
use crate::models::{StepCallRecord, StepOutput};

/// The model to record for a call and how Pipeline knows it (see
/// `StepCallRecord::model_basis`). A provider's own report outranks the
/// request: a CLI alias such as `opus` is reported as the full model ID, and a
/// CLI Automatic call has no other record of its model.
pub(super) fn recorded_model(
    provider: &str,
    resolution: &ResolvedModel,
    reported: Option<&str>,
) -> (String, &'static str) {
    if let Some(model) = reported.map(str::trim).filter(|model| !model.is_empty()) {
        return (model.to_string(), "reported");
    }
    if resolution.command_model.is_some() || resolution.transport == "api" {
        return (resolution.resolved_model.clone(), "requested");
    }
    match crate::model_catalog::cli_default_model(provider) {
        Some(model) => (model, "expected"),
        None => (resolution.resolved_model.clone(), "unknown"),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn call_record(
    role: &str,
    provider: &str,
    agent: &str,
    resolution: &ResolvedModel,
    reported_model: Option<&str>,
    effort: &str,
    duration_secs: u64,
    usage: CallUsage,
) -> StepCallRecord {
    let (model, model_basis) = recorded_model(provider, resolution, reported_model);
    StepCallRecord {
        role: role.to_string(),
        provider: provider.to_string(),
        agent: agent.to_string(),
        model,
        model_transport: resolution.transport.clone(),
        model_policy: resolution.selection.label(),
        model_source: resolution.source.clone(),
        model_catalog_updated_at: resolution.catalog_updated_at.clone(),
        model_basis: model_basis.to_string(),
        // Empty means the provider's own default applied. Readers name that
        // default by provider instead of storing a bare "default".
        effort: effort.trim().to_string(),
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
        fallback.reported_model.as_deref(),
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
        model_basis: String::new(),
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
