//! Findings regression coverage.

use super::*;

#[test]
fn finding_lineage_requires_an_ordered_id_subsequence() {
    let keep_all = r#"{"findings":[{"id":"a"},{"id":"b"},{"id":"c"}]}"#;
    let expected = finding_lineage(keep_all).unwrap();
    let drop_middle = r#"{"findings":[{"id":"a"},{"id":"c"}]}"#;
    let reordered = r#"{"findings":[{"id":"c"},{"id":"a"}]}"#;
    let invented = r#"{"findings":[{"id":"a"},{"id":"new"}]}"#;
    assert!(check_finding_lineage(&expected, keep_all).is_ok());
    assert!(check_finding_lineage(&expected, drop_middle).is_ok());
    assert!(check_finding_lineage(&expected, reordered).is_err());
    assert!(check_finding_lineage(&expected, invented).is_err());
    assert!(check_finding_lineage(&expected, "not json").is_err());
}

#[test]
fn validation_ledger_covers_every_input_and_receives_host_hashes() {
    let input =
        r#"{"findings":[{"rank":1,"id":"a","problem":"P"},{"rank":2,"id":"b","problem":"Q"}]}"#;
    let expected = finding_lineage(input).unwrap();
    let output = r#"{
      "findings":[{"rank":1,"id":"a","problem":"P"}],
      "validation_dispositions":[
        {"finding_id":"a","disposition":"retained","reason":"Confirmed"},
        {"finding_id":"b","disposition":"rejected_false_positive","reason":"Appendix resolves it"}
      ]
    }"#;
    let checked = check_validation_dispositions(&expected, output).unwrap();
    assert!(checked.contains("before_hash"));
    assert!(checked.contains("after_hash"));

    let incomplete = r#"{
      "findings":[{"rank":1,"id":"a","problem":"P"}],
      "validation_dispositions":[
        {"finding_id":"a","disposition":"retained","reason":"Confirmed"}
      ]
    }"#;
    assert!(check_validation_dispositions(&expected, incomplete)
        .unwrap_err()
        .contains("exactly one disposition"));
}

#[test]
fn finding_lineage_baseline_fails_closed_when_the_source_is_unavailable() {
    let schema = serde_json::json!({
        "type": "object",
        crate::pipeline::structured::PRESERVE_FINDINGS_KEY: "source",
    });
    let error = preserved_finding_ids("validate", Some(&schema), &[]).unwrap_err();
    assert!(
        error.contains("required artifact is unavailable"),
        "{error}"
    );

    let source = findings_output("source", r#"{"findings":[{"id":"stable"}]}"#);
    let preserved = preserved_finding_ids("validate", Some(&schema), &[source])
        .unwrap()
        .unwrap();
    assert_eq!(preserved[0].id, "stable");
}

#[test]
fn specialist_artifacts_render_for_sequential_context() {
    let mut specialist_steps = std::collections::HashSet::new();
    specialist_steps.insert("auto_exposition".to_string());
    let specialist = StepOutput {
        step_id: "auto_exposition".into(),
        step_label: "Exposition".into(),
        raw_text: r#"{"findings":[{"title":"T","in_the_paper":"P","problem":"Q","consequence":"C","what_would_help":"H","evidence":[{"page":3}]}]}"#.into(),
        structured_json: true,
        ..Default::default()
    };
    let synthesis = findings_output("auto_synthesis", r#"{"findings":[]}"#);
    let rendered = specialist_context_outputs(&[specialist, synthesis.clone()], &specialist_steps);
    assert!(rendered[0]
        .raw_text
        .starts_with("Report id: auto_exposition"));
    assert!(rendered[0].raw_text.contains("**#1. T**"));
    assert!(rendered[0].raw_text.contains("- **Location:** p. 3"));
    // Canonical findings JSON passes through verbatim for validation.
    assert_eq!(rendered[1].raw_text, synthesis.raw_text);
}

#[test]
fn artifact_fan_out_expands_bounded_elements_with_lineage() {
    let mut step = make_step("verify", Phase::Parallel);
    step.output_schema = Some(serde_json::json!({
        "type": "object",
        crate::pipeline::structured::SCHEMA_REFERENCE_KEY: "findings-v1",
        crate::pipeline::structured::PRESERVE_FINDINGS_KEY: "auto_synthesis",
    }));
    step.for_each = Some(crate::pipeline_config::ForEach {
        glob: String::new(),
        max: 2,
        artifact: Some(crate::pipeline_config::ForEachArtifact {
            step: "auto_synthesis".into(),
            pointer: "/findings".into(),
        }),
    });
    let artifact = r#"{"findings":[{"id":"first","title":"A"},{"id":"second","title":"B"},{"id":"third","title":"C"}]}"#;
    let settings = crate::settings::Settings::default();
    let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
    let units = build_units(&step, &settings, "", Some(artifact), &bus).unwrap();
    // The cap bounds expansion; suffixes are deterministic by index and
    // each unit carries its element's lineage id.
    assert_eq!(units.len(), 2);
    assert_eq!(units[0].suffix, "item_001");
    assert_eq!(units[1].suffix, "item_002");
    assert_eq!(units[0].display, "first");
    assert!(units[0]
        .inline_item
        .as_deref()
        .is_some_and(|item| item.contains("\"A\"")));
    assert_eq!(
        units[0]
            .lineage_ids
            .as_ref()
            .and_then(|lineage| lineage.first())
            .map(|finding| finding.id.as_str()),
        Some("first")
    );

    // A missing upstream artifact degrades to zero units with a warning.
    let missing = build_units(&step, &settings, "", None, &bus).unwrap();
    assert!(missing.is_empty());
}

#[test]
fn fan_out_collapse_assembles_a_contract_conformant_validation_artifact() {
    use crate::pipeline::executor::checkpoints::OutputBudget;
    use crate::pipeline::executor::findings::collapse_findings_fan_out;
    use crate::pipeline::executor::outputs::effective_output_schema;
    let taxonomy = crate::findings::CATEGORIES
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>();
    let mut step = make_step("validate", Phase::Parallel);
    step.output_schema = Some(serde_json::json!({
        "type": "object",
        crate::pipeline::structured::SCHEMA_REFERENCE_KEY: "findings-v2-validation",
        crate::pipeline::structured::FINDINGS_TAXONOMY_KEY: taxonomy,
        crate::pipeline::structured::PRESERVE_FINDINGS_KEY: "synthesis",
    }));
    step.for_each = Some(crate::pipeline_config::ForEach {
        glob: String::new(),
        max: 5,
        artifact: Some(crate::pipeline_config::ForEachArtifact {
            step: "synthesis".into(),
            pointer: "/findings".into(),
        }),
    });
    let finding = |id: &str, rank: u32| {
        serde_json::json!({
            "rank": rank, "id": id, "title": "T", "category": taxonomy[0],
            "severity": "high", "confidence": "high", "verification_status": "unverified",
            "reviewer_ids": ["r1"], "problem": "P", "consequence": "C",
            "recommended_action": "A",
            "evidence": [{"evidence_type": "document", "verification_status": "unverified", "description": "p. 3", "page": 3}]
        })
    };
    let synthesis = StepOutput {
        step_id: "synthesis".into(),
        raw_text: serde_json::json!({"schema_version": 2, "taxonomy": taxonomy, "findings": [finding("a", 1), finding("b", 2)]}).to_string(),
        structured_json: true,
        ..Default::default()
    };
    let unit = |suffix: &str, id: &str| {
        StepOutput {
        step_id: format!("validate/{suffix}"),
        raw_text: serde_json::json!({
            "schema_version": 2, "taxonomy": taxonomy, "findings": [finding(id, 1)],
            "validation_dispositions": [{"finding_id": id, "disposition": "retained", "reason": "Confirmed"}]
        })
        .to_string(),
        structured_json: true,
        ..Default::default()
    }
    };
    let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
    let budget = std::sync::Arc::new(OutputBudget::fresh());
    let outputs = collapse_findings_fan_out(
        &bus,
        &[&step],
        &[synthesis],
        vec![unit("item_001", "a"), unit("item_002", "b")],
        &budget,
    );
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].step_id, "validate");
    let schema = effective_output_schema(&step).unwrap().unwrap();
    crate::pipeline::structured::canonicalize(&schema, &outputs[0].raw_text).unwrap();
    let value: serde_json::Value = serde_json::from_str(&outputs[0].raw_text).unwrap();
    let ranks = value["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["rank"].as_u64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(ranks, [1, 2]);
    assert_eq!(
        value["validation_dispositions"].as_array().unwrap().len(),
        2
    );
    assert_eq!(value["schema_version"], 2);
}
