//! Graph regression coverage.

use super::*;

#[test]
fn unsupported_sequential_multi_unit_shapes_are_rejected() {
    let multi_agent = StepConfig {
        id: "synthesis".to_string(),
        phase: Phase::Sequential,
        agents: vec!["claude".to_string(), "antigravity".to_string()],
        ..Default::default()
    };
    assert!(validate_workflow_semantics(&[multi_agent]).is_err());

    let fan_out = StepConfig {
        id: "fanout".to_string(),
        phase: Phase::Sequential,
        for_each: Some(ForEach {
            glob: "*.tex".to_string(),
            max: 10,
            artifact: None,
        }),
        ..Default::default()
    };
    assert!(validate_workflow_semantics(&[fan_out]).is_err());
}

#[test]
fn runnable_workflows_require_an_enabled_sequential_step() {
    let parallel = step_with_id("analysis");
    assert_eq!(
        validate_enabled_sequential_step(std::slice::from_ref(&parallel)).unwrap_err(),
        "A workflow must have at least one enabled Sequential step to produce a final report."
    );

    let mut sequential = StepConfig {
        id: "synthesis".to_string(),
        phase: Phase::Sequential,
        enabled: false,
        ..Default::default()
    };
    assert!(validate_enabled_sequential_step(&[parallel.clone(), sequential.clone()]).is_err());

    sequential.enabled = true;
    assert!(validate_enabled_sequential_step(&[parallel, sequential]).is_ok());
}

#[test]
fn runtime_validation_rejects_parallel_only_workflows() {
    let config: PipelineConfig = ProfileData::new(
        "Parallel only",
        vec![step_with_id("analysis")],
        MergeConfig::default(),
    )
    .into();

    assert!(validate_profile_data(&ProfileData::from_config("Editable", &config)).is_ok());
    assert_eq!(
        validate_runtime_config(&config).unwrap_err(),
        "A workflow must have at least one enabled Sequential step to produce a final report."
    );
}

#[test]
fn published_outputs_require_a_runnable_sequential_producer() {
    let mut profile = ProfileData::new(
        "Published",
        vec![StepConfig {
            id: "final".into(),
            label: "Final".into(),
            phase: Phase::Sequential,
            enabled: true,
            ..Default::default()
        }],
        MergeConfig::default(),
    );
    profile.outputs.primary_step = "final".into();
    profile.outputs.findings_step = "final".into();
    assert_eq!(
        validate_profile_data(&profile).unwrap_err(),
        "Published findings step 'final' must define an output JSON schema"
    );

    profile.steps[0].output_schema = Some(crate::findings::output_schema());
    validate_profile_data(&profile).unwrap();

    profile.steps[0].phase = Phase::Parallel;
    assert_eq!(
        validate_profile_data(&profile).unwrap_err(),
        "Published primary report step 'final' must be Sequential"
    );
}

#[test]
fn findings_lineage_requires_a_dataflow_edge_to_its_source() {
    let source = StepConfig {
        id: "source".into(),
        label: "Source".into(),
        phase: Phase::Parallel,
        enabled: true,
        output_schema: Some(crate::findings::output_schema()),
        ..Default::default()
    };
    let mut final_step = StepConfig {
        id: "final".into(),
        label: "Final".into(),
        phase: Phase::Sequential,
        enabled: true,
        after: vec!["source".into()],
        output_schema: Some(serde_json::json!({
            "type": "object",
            crate::pipeline::structured::SCHEMA_REFERENCE_KEY: "findings-v1",
            crate::pipeline::structured::PRESERVE_FINDINGS_KEY: "source",
        })),
        ..Default::default()
    };
    let mut profile = ProfileData::new(
        "Lineage",
        vec![source, final_step.clone()],
        MergeConfig::default(),
    );
    profile.outputs.primary_step = "final".into();
    profile.outputs.findings_step = "final".into();

    let error = validate_profile_data(&profile).unwrap_err();
    assert!(
        error.contains("does not select that step as an artifact source"),
        "{error}"
    );

    final_step.context.include.push(ArtifactSelector::Step {
        step: "source".into(),
        parts: vec![StepArtifactPart::Report],
        glob: String::new(),
    });
    profile.steps[1] = final_step;
    validate_profile_data(&profile).unwrap();
}

#[test]
fn steps_without_dependency_edges_are_valid() {
    let steps = vec![step_with_id("a"), step_with_id("b"), step_with_id("c")];
    assert!(validate_dependencies(&steps).is_ok());
}

#[test]
fn deps_valid_dag_passes() {
    let steps = vec![
        step_dep("a", &[]),
        step_dep("b", &["a"]),
        step_dep("c", &["a", "b"]),
    ];
    assert!(validate_dependencies(&steps).is_ok());
}

#[test]
fn selected_step_artifacts_create_dataflow_dependencies() {
    let producer = step_with_id("producer");
    let mut consumer = step_with_id("consumer");
    consumer.phase = Phase::Sequential;
    consumer.context.include = vec![ArtifactSelector::Step {
        step: "producer".into(),
        parts: vec![StepArtifactPart::Report],
        glob: String::new(),
    }];
    let steps = [producer, consumer];
    assert!(validate_dependencies(&steps).is_ok());
    let enabled: Vec<&StepConfig> = steps.iter().collect();
    let dependencies = resolve_dependencies(&enabled);
    assert_eq!(
        dependencies[1],
        ["producer".to_string()].into_iter().collect()
    );
}

#[test]
fn dependency_success_policies_create_edges_and_validate_quorum() {
    let core = step_with_id("core");
    let specialist_a = step_with_id("specialist-a");
    let specialist_b = step_with_id("specialist-b");
    let mut synthesis = step_with_id("synthesis");
    synthesis.phase = Phase::Sequential;
    synthesis.dependency_policy = DependencyPolicy {
        required: vec!["core".into()],
        quorum: vec!["specialist-a".into(), "specialist-b".into()],
        minimum_successes: 1,
    };
    let steps = [core, specialist_a, specialist_b, synthesis];
    validate_dependencies(&steps).unwrap();
    let enabled = steps.iter().collect::<Vec<_>>();
    let dependencies = resolve_dependencies(&enabled);
    assert_eq!(dependencies[3].len(), 3);

    let mut invalid = steps[3].clone();
    invalid.dependency_policy.minimum_successes = 3;
    assert!(validate_dependencies(&[
        steps[0].clone(),
        steps[1].clone(),
        steps[2].clone(),
        invalid,
    ])
    .unwrap_err()
    .contains("quorum minimum"));
}

#[test]
fn mixed_order_and_artifact_cycle_is_rejected() {
    let mut a = step_dep("a", &["b"]);
    a.context.include.clear();
    let mut b = step_with_id("b");
    b.phase = Phase::Sequential;
    b.context.include = vec![ArtifactSelector::Step {
        step: "a".into(),
        parts: vec![StepArtifactPart::Files],
        glob: "**/*.csv".into(),
    }];
    assert!(validate_dependencies(&[a, b]).is_err());
}

#[test]
fn deps_unknown_id_rejected() {
    let steps = vec![step_dep("a", &["ghost"])];
    let err = validate_dependencies(&steps).unwrap_err();
    assert!(err.contains("unknown step 'ghost'"), "{err}");
}

#[test]
fn deps_self_dependency_rejected() {
    let steps = vec![step_dep("a", &["a"])];
    let err = validate_dependencies(&steps).unwrap_err();
    assert!(err.contains("itself"), "{err}");
}

#[test]
fn deps_cycle_rejected() {
    let steps = vec![step_dep("a", &["b"]), step_dep("b", &["a"])];
    let err = validate_dependencies(&steps).unwrap_err();
    assert!(err.contains("cycle"), "{err}");
}

#[test]
fn deps_longer_cycle_rejected() {
    let steps = vec![
        step_dep("a", &["c"]),
        step_dep("b", &["a"]),
        step_dep("c", &["b"]),
    ];
    assert!(validate_dependencies(&steps).is_err());
}

#[test]
fn disabled_dependencies_have_explicit_semantics() {
    let mut disabled = step_dep("off", &["ghost"]);
    disabled.enabled = false;
    assert!(validate_dependencies(&[disabled.clone()]).is_ok());

    let enabled = step_dep("on", &["off"]);
    let error = validate_dependencies(&[disabled, enabled]).unwrap_err();
    assert!(error.contains("disabled step 'off'"), "{error}");
}

#[test]
fn disabled_cycles_are_ignored() {
    let mut a = step_dep("a", &["b"]);
    let mut b = step_dep("b", &["a"]);
    a.enabled = false;
    b.enabled = false;
    assert!(validate_dependencies(&[a, b]).is_ok());
}

#[test]
fn output_conditions_require_valid_completed_upstreams() {
    let a = step_with_id("a");
    let mut same_wave = step_with_id("b");
    same_wave.run_if = Some(RunCondition::OutputMatches {
        step: "a".into(),
        pattern: "high".into(),
        negate: false,
    });
    assert!(validate_run_conditions(&[a.clone(), same_wave.clone()])
        .unwrap_err()
        .contains("not an upstream dependency"));

    same_wave.after = vec!["a".into()];
    assert!(validate_run_conditions(&[a.clone(), same_wave.clone()]).is_ok());

    if let Some(RunCondition::OutputMatches { pattern, .. }) = &mut same_wave.run_if {
        *pattern = "(".into();
    }
    assert!(validate_run_conditions(&[a, same_wave])
        .unwrap_err()
        .contains("invalid regular expression"));
}

#[test]
fn survey_conditions_require_valid_json_pointers() {
    let mut step = step_with_id("survey");
    step.run_if = Some(RunCondition::SurveyPath {
        pointer: "metadata/type".into(),
        equals: None,
        exists: Some(true),
        contains: None,
    });
    assert!(validate_run_conditions(&[step.clone()]).is_err());
    step.run_if = Some(RunCondition::SurveyPath {
        pointer: "/metadata/a~1b".into(),
        equals: None,
        exists: Some(true),
        contains: None,
    });
    assert!(validate_run_conditions(&[step]).is_ok());
}

#[test]
fn profile_validation_rejects_incoherent_artifact_selectors() {
    let mut no_input = ProfileData::new(
        "No input",
        vec![step_with_id("reader")],
        MergeConfig::default(),
    );
    no_input.extraction.input_mode = "none".into();
    no_input.steps[0].context.include = vec![ArtifactSelector::Primary {
        parts: vec![PrimaryArtifactPart::Text],
    }];
    assert!(validate_profile_data(&no_input)
        .unwrap_err()
        .contains("workflow has no input"));

    let mut invalid_glob = ProfileData::new(
        "Invalid glob",
        vec![step_with_id("producer"), step_with_id("consumer")],
        MergeConfig::default(),
    );
    invalid_glob.steps[1].phase = Phase::Sequential;
    invalid_glob.steps[1].context.include = vec![ArtifactSelector::Step {
        step: "producer".into(),
        parts: vec![StepArtifactPart::Report],
        glob: "*.csv".into(),
    }];
    assert!(validate_profile_data(&invalid_glob)
        .unwrap_err()
        .contains("without selecting files"));

    let mut parallel_consumer = ProfileData::new(
        "Parallel consumer",
        vec![step_with_id("producer"), step_with_id("consumer")],
        MergeConfig::default(),
    );
    parallel_consumer.steps[1].context.include = vec![ArtifactSelector::Step {
        step: "producer".into(),
        parts: vec![StepArtifactPart::Report],
        glob: String::new(),
    }];
    assert!(validate_profile_data(&parallel_consumer)
        .unwrap_err()
        .contains("Parallel step 'consumer' cannot select output"));
}

#[test]
fn profile_validation_bounds_fan_out_cost() {
    let step = StepConfig {
        id: "map".to_string(),
        label: "Map".to_string(),
        prompt: "Review {item}".to_string(),
        enabled: true,
        phase: Phase::Parallel,
        for_each: Some(ForEach {
            glob: "**/*".to_string(),
            max: MAX_FAN_OUT_ITEMS + 1,
            artifact: None,
        }),
        ..Default::default()
    };
    let profile = ProfileData::new("Imported".to_string(), vec![step], Default::default());
    assert!(validate_profile_data(&profile)
        .unwrap_err()
        .contains("fan-out maximum"));
}

#[test]
fn profile_validation_rejects_backslash_fan_out_glob() {
    let step = StepConfig {
        id: "map".to_string(),
        label: "Map".to_string(),
        prompt: "Review {item}".to_string(),
        enabled: true,
        phase: Phase::Parallel,
        for_each: Some(ForEach {
            glob: "tables\\*.csv".to_string(),
            max: 4,
            artifact: None,
        }),
        ..Default::default()
    };
    let profile = ProfileData::new("Imported".to_string(), vec![step], Default::default());
    assert!(validate_profile_data(&profile)
        .unwrap_err()
        .contains("invalid fan-out glob"));
}
