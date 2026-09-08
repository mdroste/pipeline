use super::*;
use crate::settings::ModelSelection;

#[test]
fn role_and_attempt_saturation_survive_shared_record_construction() {
    let resolution = ResolvedModel {
        selection: ModelSelection::Automatic,
        command_model: None,
        resolved_model: "catalog-model".into(),
        transport: "cli".into(),
        source: "account".into(),
        catalog_updated_at: "fixture-version".into(),
        supported_efforts: vec![],
    };
    for role in [
        "step",
        "merge",
        "usage_limit_primary",
        "usage_limit_primary_merge",
        "usage_limit_fallback",
        "usage_limit_fallback_merge",
    ] {
        let record = call_record(
            role,
            "codex",
            "agent",
            &resolution,
            "high",
            9,
            CallUsage {
                provider_attempts: u64::MAX,
                ..Default::default()
            },
        );
        assert_eq!(record.role, role);
        assert_eq!(record.attempt_count, u32::MAX);
        assert_eq!(record.effort, "high");
        assert_eq!(record.model, "catalog-model");
        assert_eq!(record.model_transport, "cli");
        assert_eq!(record.model_catalog_updated_at, "fixture-version");
    }
}

#[test]
fn legacy_products_get_one_record_and_existing_records_remain_authoritative() {
    let mut output = StepOutput {
        phase: "sequential".into(),
        provider: "claude".into(),
        agent: "reviewer".into(),
        model: "legacy-model".into(),
        attempt_count: 3,
        input_tokens: 123,
        duration_secs: 12,
        ..Default::default()
    };
    let record = call_records_for_output(&output).remove(0);
    assert_eq!(record.role, "sequential");
    assert_eq!(record.model, "legacy-model");
    assert_eq!(record.input_tokens, 123);
    assert_eq!(record.attempt_count, 3);
    assert!(record.effort.is_empty()); // Legacy records did not retain effort.
    output.calls = vec![StepCallRecord {
        role: "failed_merge".into(),
        ..record
    }];
    let preserved = call_records_for_output(&output);
    assert_eq!(preserved.len(), 1);
    assert_eq!(preserved[0].role, "failed_merge");
}
