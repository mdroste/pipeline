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
            Some("catalog-model"),
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
        assert_eq!(record.model_basis, "reported");
        assert_eq!(record.model_transport, "cli");
        assert_eq!(record.model_catalog_updated_at, "fixture-version");
    }
}

#[test]
fn recorded_model_prefers_the_report_and_says_how_it_knows() {
    let cli_automatic = ResolvedModel {
        selection: ModelSelection::Automatic,
        command_model: None,
        resolved_model: "Provider default".into(),
        transport: "cli".into(),
        source: "provider_default".into(),
        catalog_updated_at: String::new(),
        supported_efforts: vec![],
    };
    // A provider name no other test remembers a default for.
    let provider = "provenance-test";
    assert_eq!(
        recorded_model(provider, &cli_automatic, Some("claude-opus-5-5")),
        ("claude-opus-5-5".to_string(), "reported")
    );
    assert_eq!(
        recorded_model(provider, &cli_automatic, None),
        ("Provider default".to_string(), "unknown")
    );
    crate::model_catalog::remember_cli_default(provider, "listed-default");
    assert_eq!(
        recorded_model(provider, &cli_automatic, Some(" ")),
        ("listed-default".to_string(), "expected")
    );
    let pinned = ResolvedModel {
        command_model: Some("opus".into()),
        resolved_model: "opus".into(),
        ..cli_automatic.clone()
    };
    assert_eq!(
        recorded_model(provider, &pinned, None),
        ("opus".to_string(), "requested")
    );
    assert_eq!(
        recorded_model(provider, &pinned, Some("claude-opus-4-7")),
        ("claude-opus-4-7".to_string(), "reported")
    );
}

#[test]
fn an_unset_effort_is_recorded_empty_rather_than_as_default() {
    let resolution = ResolvedModel {
        selection: ModelSelection::Automatic,
        command_model: Some("gpt-5.6-terra".into()),
        resolved_model: "gpt-5.6-terra".into(),
        transport: "api".into(),
        source: "provider_api".into(),
        catalog_updated_at: String::new(),
        supported_efforts: vec![],
    };
    let record = call_record(
        "step",
        "codex",
        "codex",
        &resolution,
        None,
        "  ",
        1,
        CallUsage::default(),
    );
    assert_eq!(record.effort, "");
    assert_eq!(record.model_basis, "requested");
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
