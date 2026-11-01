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

fn resume_step(id: &str, after: &[&str]) -> crate::pipeline_config::StepConfig {
    crate::pipeline_config::StepConfig {
        id: id.to_string(),
        enabled: true,
        after: after.iter().map(|id| id.to_string()).collect(),
        ..Default::default()
    }
}

fn resume_output(id: &str, text: &str, skipped: bool) -> crate::models::StepOutput {
    crate::models::StepOutput {
        step_id: id.to_string(),
        raw_text: text.to_string(),
        skipped,
        ..Default::default()
    }
}

fn parent_report(step_outputs: Vec<crate::models::StepOutput>) -> PipelineReport {
    PipelineReport {
        orientation: serde_json::Value::Null,
        step_outputs,
        failed_steps: Vec::new(),
        products: Default::default(),
        quality: Default::default(),
        referee_reports: Vec::new(),
        editor: None,
        report_date: chrono::Local::now().date_naive(),
        paper_hash: "test".into(),
    }
}

fn resume_selection(
    config: &PipelineConfig,
    report: &PipelineReport,
) -> std::collections::HashMap<String, Vec<crate::models::StepOutput>> {
    let app: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
    let parent = tempfile::tempdir().unwrap();
    select_reusable_outputs(
        &app,
        "parent",
        parent.path(),
        config,
        report,
        false,
        None,
        true,
    )
}

fn parent_with_a_skipped_step() -> PipelineReport {
    parent_report(vec![
        resume_output("optional", "_(skipped: run_if condition not met)_", true),
        resume_output("summary", "Completed summary", false),
    ])
}

#[test]
fn resume_reuses_the_dependents_of_a_skipped_step() {
    let mut config = empty_test_config();
    config.steps = vec![
        resume_step("optional", &[]),
        resume_step("summary", &["optional"]),
    ];

    let preloaded = resume_selection(&config, &parent_with_a_skipped_step());

    assert_eq!(preloaded["summary"][0].raw_text, "Completed summary");
    // The skipped step itself evaluates its guard again.
    assert!(!preloaded.contains_key("optional"));
}

#[test]
fn a_skipped_producer_needs_no_retained_files() {
    let mut config = empty_test_config();
    let mut summary = resume_step("summary", &["optional"]);
    summary.context.include = vec![crate::pipeline_config::ArtifactSelector::Step {
        step: "optional".to_string(),
        parts: vec![crate::pipeline_config::StepArtifactPart::Files],
        glob: String::new(),
    }];
    config.steps = vec![resume_step("optional", &[]), summary];

    let preloaded = resume_selection(&config, &parent_with_a_skipped_step());

    assert!(preloaded.contains_key("summary"));
}

#[test]
fn a_step_with_no_output_still_invalidates_its_dependents() {
    let mut config = empty_test_config();
    config.steps = vec![resume_step("lost", &[]), resume_step("summary", &["lost"])];
    let report = parent_report(vec![resume_output("summary", "Completed summary", false)]);

    assert!(resume_selection(&config, &report).is_empty());
}
