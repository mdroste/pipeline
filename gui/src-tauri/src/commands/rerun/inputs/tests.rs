use super::*;

#[test]
fn missing_captured_named_input_preserves_required_and_optional_admission() {
    let directory = tempfile::tempdir().unwrap();
    let parent: crate::runs::RunManifest = serde_json::from_value(serde_json::json!({
        "run_id": "old-parent",
        "created": "2026-10-29T00:00:00Z",
        "input_path": "",
        "input_mode": "none",
        "profile_id": "fixture",
        "profile_name": "Fixture",
        "provider": "claude",
        "artifacts": []
    }))
    .unwrap();
    let mut config: crate::pipeline_config::PipelineConfig =
        crate::pipeline_config::ProfileData::new(
            "Restore fixture",
            Vec::new(),
            crate::pipeline_config::MergeConfig {
                enabled: false,
                prompt: String::new(),
                agents: Vec::new(),
            },
        )
        .into();
    config.extraction.extra_inputs = vec![crate::pipeline_config::InputSlot {
        key: "rubric".into(),
        label: "Rubric".into(),
        required: true,
        ..Default::default()
    }];
    let mut writer = None;
    let result = restore_named_inputs(
        "old-parent",
        directory.path(),
        &parent,
        &config,
        &config,
        &directory,
        &mut writer,
    );
    match result {
        Err(error) => assert_eq!(
            error,
            "Re-run requires named input 'Rubric', but the parent run did not capture it. Start a new run and select the input again."
        ),
        Ok(_) => panic!("missing required captured input must fail"),
    }
    config.extraction.extra_inputs[0].required = false;
    let restored = restore_named_inputs(
        "old-parent",
        directory.path(),
        &parent,
        &config,
        &config,
        &directory,
        &mut writer,
    )
    .unwrap();
    assert!(restored.runtime_extra_sources.is_empty());
    assert!(restored.resolved_inputs.is_empty());
    assert!(restored.persisted_inputs.is_empty());
    assert!(restored.extra_tmps.is_empty());
    assert!(writer.is_none());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}
