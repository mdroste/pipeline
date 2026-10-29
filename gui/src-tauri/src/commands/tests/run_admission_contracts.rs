//! Admission, retained-input, and reuse contracts without live providers.

use super::*;

#[derive(Default)]
struct RecordedRunEvents(std::sync::Mutex<Vec<(String, serde_json::Value)>>);

impl crate::emit::Events for RecordedRunEvents {
    fn emit_event(
        &self,
        event: &str,
        payload: serde_json::Value,
    ) -> Result<(), crate::emit::EmitError> {
        self.0.lock().unwrap().push((event.to_string(), payload));
        Ok(())
    }
}

#[tokio::test]
async fn no_source_admission_preserves_validation_order_and_scope_cleanup() {
    let mut config = empty_test_config();
    config.extraction.input_mode = "none".into();
    config.variables = vec![crate::pipeline_config::VarSpec {
        key: "topic".into(),
        label: "Topic".into(),
        required: true,
        ..Default::default()
    }];
    let snapshot = RunSnapshot {
        settings: crate::settings::Settings::default(),
        config,
        profile_name: "Admission fixture".into(),
        config_fingerprint: "fixture".into(),
        fingerprint: "fixture".into(),
        workflow_source: "fixture".into(),
        workflow_fingerprint: "fixture".into(),
        workflow_json: String::new(),
        specialist_catalog_revision: String::new(),
    };
    let recorded = std::sync::Arc::new(RecordedRunEvents::default());
    let bus: crate::emit::EventBus = recorded.clone();
    let active = crate::run_scope::ActiveScope::begin("no-source-admission-characterization");
    active.scope().request_cancel();
    let error = crate::run_scope::scoped(active.scope().clone(), async {
        run_pipeline_inner_with_snapshot(
            &bus,
            "",
            None,
            false,
            Default::default(),
            Default::default(),
            Some(snapshot),
        )
        .await
        .unwrap_err()
    })
    .await;
    assert_eq!(error, "Missing required variable 'Topic'");
    assert!(active.scope().is_cancelled());
    assert!(active.scope().frozen_settings().is_none());
    assert!(recorded.0.lock().unwrap().is_empty());
}

#[test]
fn rerun_reuse_requires_every_non_skipped_output_and_ignores_disabled_steps() {
    let mut config = empty_test_config();
    config.steps = vec![
        crate::pipeline_config::StepConfig {
            id: "review".into(),
            output_schema: Some(serde_json::json!({
                "type": "object",
                "required": ["verdict"],
                "properties": {"verdict": {"type": "string"}}
            })),
            ..Default::default()
        },
        crate::pipeline_config::StepConfig {
            id: "plain".into(),
            ..Default::default()
        },
        crate::pipeline_config::StepConfig {
            id: "missing".into(),
            ..Default::default()
        },
        crate::pipeline_config::StepConfig {
            id: "skipped_only".into(),
            ..Default::default()
        },
        crate::pipeline_config::StepConfig {
            id: "disabled".into(),
            enabled: false,
            ..Default::default()
        },
    ];
    let outputs = [
        crate::models::StepOutput {
            step_id: "review/claude".into(),
            raw_text: r#"{"verdict":"accept"}"#.into(),
            structured_json: true,
            ..Default::default()
        },
        crate::models::StepOutput {
            step_id: "review/codex".into(),
            raw_text: r#"{"score":1}"#.into(),
            structured_json: true,
            ..Default::default()
        },
        crate::models::StepOutput {
            step_id: "plain/claude".into(),
            raw_text: "Retained report".into(),
            ..Default::default()
        },
        crate::models::StepOutput {
            step_id: "plain/codex".into(),
            skipped: true,
            ..Default::default()
        },
        crate::models::StepOutput {
            step_id: "skipped_only".into(),
            skipped: true,
            ..Default::default()
        },
    ];
    assert_eq!(
        incompatible_reuse_ids(&config, &outputs),
        ["review", "missing", "skipped_only"]
            .into_iter()
            .map(str::to_string)
            .collect()
    );
}

#[test]
fn rerun_reads_captured_bytes_and_rejects_paths_outside_the_run() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("run");
    std::fs::create_dir_all(root.join("context")).unwrap();
    std::fs::write(root.join("context/input.txt"), "Captured input\r\n").unwrap();
    assert_eq!(
        read_run_file_in(&root, "context/input.txt").unwrap(),
        "Captured input\r\n"
    );
    for relative in [
        "",
        "../outside.txt",
        "context/../outside.txt",
        "context\\..\\outside.txt",
    ] {
        assert_eq!(
            read_run_file_in(&root, relative).unwrap_err(),
            "Invalid run-relative path"
        );
    }
    assert_eq!(
        read_run_file_in(&root, root.join("context/input.txt").to_str().unwrap()).unwrap_err(),
        "Invalid run-relative path"
    );
    #[cfg(unix)]
    {
        let outside = directory.path().join("outside.txt");
        std::fs::write(&outside, "Outside content").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("context/escape.txt")).unwrap();
        assert_eq!(
            read_run_file_in(&root, "context/escape.txt").unwrap_err(),
            "Run-relative path resolves outside the run directory"
        );
    }
}
