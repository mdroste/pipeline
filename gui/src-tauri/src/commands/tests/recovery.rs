//! Recovery regression coverage.

use super::*;

#[test]
fn rerun_cache_preserves_every_output_for_a_base_step() {
    let outputs = vec![
        crate::models::StepOutput {
            step_id: "review/claude".into(),
            raw_text: "one".into(),
            ..Default::default()
        },
        crate::models::StepOutput {
            step_id: "review/antigravity".into(),
            raw_text: "two".into(),
            ..Default::default()
        },
    ];
    let cache = collect_preloaded_outputs(&outputs, &Default::default());
    assert_eq!(cache["review"].len(), 2);
}

#[test]
fn rerun_cache_invalidates_artifacts_when_the_output_contract_changes() {
    let mut config = empty_test_config();
    config.steps = vec![crate::pipeline_config::StepConfig {
        id: "review".into(),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "required": ["verdict"],
            "properties": {"verdict": {"type": "string"}}
        })),
        ..Default::default()
    }];
    let markdown_parent = crate::models::StepOutput {
        step_id: "review".into(),
        raw_text: "# Old report".into(),
        structured_json: false,
        ..Default::default()
    };
    assert!(incompatible_reuse_ids(&config, &[markdown_parent]).contains("review"));

    let valid_json = crate::models::StepOutput {
        step_id: "review".into(),
        raw_text: r#"{"verdict":"accept"}"#.into(),
        structured_json: true,
        ..Default::default()
    };
    assert!(!incompatible_reuse_ids(&config, &[valid_json]).contains("review"));

    let invalid_json = crate::models::StepOutput {
        step_id: "review".into(),
        raw_text: r#"{"score":1}"#.into(),
        structured_json: true,
        ..Default::default()
    };
    assert!(incompatible_reuse_ids(&config, &[invalid_json]).contains("review"));
}

#[test]
fn composite_failure_ids_normalize_to_the_base_step() {
    let failures = vec![crate::models::StepFailure {
        step_id: "review/antigravity".into(),
        step_label: "Review (Antigravity)".into(),
        phase: "parallel".into(),
        error: "failed".into(),
    }];
    let seeds = failed_base_ids(&failures);
    assert_eq!(seeds, ["review".to_string()].into_iter().collect());
}

#[test]
fn resume_starts_at_the_first_missing_step_after_recovery() {
    let report = PipelineReport {
        orientation: serde_json::Value::Null,
        step_outputs: vec![crate::models::StepOutput {
            step_id: "technical".into(),
            raw_text: "complete".into(),
            ..Default::default()
        }],
        failed_steps: vec![crate::models::StepFailure {
            step_id: "__run_cancelled__".into(),
            step_label: "Run cancelled".into(),
            phase: String::new(),
            error: "cancelled".into(),
        }],
        products: Default::default(),
        quality: Default::default(),
        referee_reports: Vec::new(),
        editor: None,
        report_date: chrono::Local::now().date_naive(),
        paper_hash: "test".into(),
    };
    let enabled = vec![
        "technical".to_string(),
        "empirical".to_string(),
        "synthesis".to_string(),
    ];
    let seeds = resume_seed_ids(&enabled, &report);
    assert!(!seeds.contains("technical"));
    assert!(seeds.contains("empirical"));
    assert!(seeds.contains("synthesis"));
}
