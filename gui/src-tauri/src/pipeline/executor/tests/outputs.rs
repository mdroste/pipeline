//! Outputs regression coverage.

use super::*;

#[test]
fn non_conforming_merge_falls_back_to_first_unit_output() {
    let mut step = make_step("s", Phase::Parallel);
    step.output_schema = Some(serde_json::json!({"type": "object", "required": ["issues"]}));
    let units = vec![
        schema_unit("claude", "{\"issues\": []}"),
        schema_unit("codex", "{\"issues\": [1]}"),
    ];
    let mut merged = vec![merge_output("A narrative merge, not issues JSON.")];
    let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
    enforce_merge_output_schemas(&bus, &[&step], &units, &mut merged);
    assert_eq!(merged[0].raw_text, "{\"issues\": []}");
    assert_eq!(merged[0].agent, "claude");
    assert!(merged[0].calls.iter().any(|c| c.role == "failed_merge"));
    assert!(merged[0].calls.iter().all(|c| c.role != "merge"));
}

#[test]
fn banner_fallback_for_schema_step_is_replaced_by_first_unit() {
    let mut step = make_step("s", Phase::Parallel);
    step.output_schema = Some(serde_json::json!({"type": "object", "required": ["issues"]}));
    let units = vec![
        schema_unit("claude", "{\"issues\": []}"),
        schema_unit("codex", "{\"issues\": [1]}"),
    ];
    // merge.rs's merge-failure fallback: a banner plus concatenated unit
    // outputs (which contain extractable JSON) and no successful merge call.
    let mut banner = merge_output(
        "> **Note**: Multi-agent merge failed. Showing individual agent outputs.\n\n{\"issues\": []}\n\n---\n\n{\"issues\": [1]}",
    );
    banner.calls[1].role = "failed_merge".into();
    let mut merged = vec![banner];
    let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
    enforce_merge_output_schemas(&bus, &[&step], &units, &mut merged);
    assert_eq!(merged[0].raw_text, "{\"issues\": []}");
    assert_eq!(merged[0].agent, "claude");
}

#[test]
fn conforming_merge_and_schemaless_step_are_untouched() {
    let mut schema_step = make_step("s", Phase::Parallel);
    schema_step.output_schema = Some(serde_json::json!({"type": "object", "required": ["issues"]}));
    let plain_step = make_step("p", Phase::Parallel);
    let units = vec![
        schema_unit("claude", "{\"issues\": []}"),
        StepOutput {
            step_id: "p/claude".into(),
            merge_group: "p".into(),
            agent: "claude".into(),
            raw_text: "plain unit".into(),
            ..Default::default()
        },
    ];
    let mut plain_merge = merge_output("Narrative merge of a schemaless step.");
    plain_merge.step_id = "p".into();
    let mut merged = vec![merge_output("{\"issues\": [1, 2]}"), plain_merge];
    let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
    enforce_merge_output_schemas(&bus, &[&schema_step, &plain_step], &units, &mut merged);
    assert_eq!(merged[0].raw_text, "{\"issues\": [1, 2]}");
    assert!(merged[0].calls.iter().any(|c| c.role == "merge"));
    assert_eq!(merged[1].raw_text, "Narrative merge of a schemaless step.");
}

#[test]
fn fallback_records_partition_usage_without_changing_effective_provenance() {
    use crate::model_catalog::ResolvedModel;
    use crate::pipeline::{call::UsageLimitFallback, logging::CallUsage};
    use crate::settings::ModelSelection;
    let resolution = |model: &str| ResolvedModel {
        selection: ModelSelection::Pinned {
            model: model.into(),
        },
        command_model: Some(model.into()),
        resolved_model: model.into(),
        transport: "api".into(),
        source: "test-catalog".into(),
        catalog_updated_at: "fixture-version".into(),
        supported_efforts: vec![],
    };
    let primary = resolution("primary-model");
    let fallback_usage = CallUsage {
        input_tokens: 11,
        output_tokens: 7,
        cached_input_tokens: 3,
        cache_write_input_tokens: 2,
        provider_attempts: 2,
        model_round_trips: 4,
        tool_calls: crate::models::ToolCallCounts {
            image: 2,
            web: 3,
            ..Default::default()
        },
    };
    let primary_usage = CallUsage {
        input_tokens: 19,
        output_tokens: 5,
        cached_input_tokens: 4,
        cache_write_input_tokens: 1,
        provider_attempts: 3,
        model_round_trips: 6,
        tool_calls: crate::models::ToolCallCounts {
            text_file: 4,
            unknown: 1,
            ..Default::default()
        },
    };
    let mut usage = primary_usage;
    usage.add_usage(fallback_usage);
    let fallback = UsageLimitFallback {
        provider: "codex".into(),
        resolution: resolution("fallback-model"),
        effort: "high".into(),
        primary_usage,
        primary_duration_secs: 13,
        fallback_usage,
        fallback_duration_secs: 17,
    };
    let output = step_output(
        "step/claude",
        "Step",
        "parallel",
        "claude",
        "reviewer",
        StepCallResult {
            text: "report".into(),
            duration_secs: 30,
            usage,
            attempt_count: 2,
            usage_limit_fallbacks: vec![fallback.clone()],
            effective_fallback: Some(fallback),
        },
        &primary,
        "  ",
        false,
    );
    assert_eq!(output.provider, "codex");
    assert_eq!(output.model, "fallback-model");
    assert_eq!(output.attempt_count, 2); // Logical attempts differ from provider invocations.
    assert_eq!(
        output
            .calls
            .iter()
            .map(|call| call.role.as_str())
            .collect::<Vec<_>>(),
        vec!["usage_limit_primary", "usage_limit_fallback"]
    );
    assert_eq!(output.calls[0].agent, "reviewer");
    assert_eq!(output.calls[0].effort, "default");
    assert_eq!(output.calls[0].duration_secs, 13);
    assert_eq!(output.calls[1].duration_secs, 17);
    let records = &output.calls;
    assert_eq!(
        records.iter().map(|call| call.input_tokens).sum::<u64>(),
        usage.input_tokens
    );
    assert_eq!(
        records.iter().map(|call| call.output_tokens).sum::<u64>(),
        usage.output_tokens
    );
    assert_eq!(
        records
            .iter()
            .map(|call| call.cached_input_tokens)
            .sum::<u64>(),
        usage.cached_input_tokens
    );
    assert_eq!(
        records
            .iter()
            .map(|call| call.cache_write_input_tokens)
            .sum::<u64>(),
        usage.cache_write_input_tokens
    );
    assert_eq!(
        records
            .iter()
            .map(|call| call.model_round_trips)
            .sum::<u64>(),
        usage.model_round_trips
    );
    assert_eq!(
        records
            .iter()
            .map(|call| u64::from(call.attempt_count))
            .sum::<u64>(),
        usage.provider_attempts
    );
    assert_eq!(records[0].tool_calls, primary_usage.tool_calls);
    assert_eq!(records[1].tool_calls, fallback_usage.tool_calls);
}
