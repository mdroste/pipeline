//! Schedule regression coverage.

use super::*;

#[test]
fn required_and_quorum_dependencies_block_incomplete_synthesis() {
    let mut step = make_step("synthesis", Phase::Sequential);
    step.dependency_policy.required = vec!["core-a".into(), "core-b".into()];
    step.dependency_policy.quorum = vec!["specialist-a".into(), "specialist-b".into()];
    step.dependency_policy.minimum_successes = 1;

    let successful = ["core-a".to_string(), "specialist-a".to_string()]
        .into_iter()
        .collect();
    let error = dependency_policy_failure(&step, &successful).unwrap();
    assert!(error.contains("core-b"), "{error}");

    let successful = [
        "core-a".to_string(),
        "core-b".to_string(),
        "specialist-a".to_string(),
    ]
    .into_iter()
    .collect();
    assert!(dependency_policy_failure(&step, &successful).is_none());
}

#[test]
fn input_processing_label_matches_the_declared_input_meaning() {
    assert_eq!(
        input_processing_label("document"),
        "Creating document bundle"
    );
    assert_eq!(
        input_processing_label("latex_project"),
        "Creating document bundle"
    );
    assert_eq!(
        input_processing_label("folder"),
        "Creating source-tree inventory"
    );
    assert_eq!(
        input_processing_label("source_tree"),
        "Creating source-tree inventory"
    );
    assert_eq!(input_processing_label("none"), "Preparing workflow context");
}

#[test]
fn execution_plan_uses_the_runtime_readiness_scheduler() {
    let mut first = make_step("first", Phase::Parallel);
    first.agents = vec!["claude".into(), "codex".into()];
    let second = make_step("second", Phase::Parallel);
    let mut synthesis = make_step("synthesis", Phase::Sequential);
    synthesis.label = "Synthesize evidence".into();
    synthesis.after = vec!["first".into(), "second".into()];
    let mut follow_up = make_step("follow-up", Phase::Parallel);
    follow_up.after = vec!["synthesis".into()];
    follow_up.run_if = Some(crate::pipeline_config::RunCondition::SurveyPath {
        pointer: "/paper/type".into(),
        equals: Some(serde_json::json!("theory")),
        exists: None,
        contains: None,
    });
    let mut disabled = make_step("disabled", Phase::Sequential);
    disabled.enabled = false;
    let config = PipelineConfig {
        steps: vec![first, second, synthesis, follow_up, disabled],
        merge: Default::default(),
        outputs: Default::default(),
        context_cache: Default::default(),
        use_orientation: true,
        orientation_prompt: String::new(),
        orientation_schema: None,
        extraction: Default::default(),
        parallel_context_template: String::new(),
        variables: Vec::new(),
    };

    let plan = execution_plan(&config).unwrap();
    assert_eq!(
        plan.iter()
            .map(|stage| stage.kind.as_str())
            .collect::<Vec<_>>(),
        vec![
            "extracting",
            "orienting",
            "dispatching",
            "merging",
            "synthesizing",
            "dispatching",
            "done"
        ]
    );
    assert_eq!(plan[2].step_ids, vec!["first", "second"]);
    assert_eq!(plan[2].label, "Parallel agent wave");
    assert_eq!(plan[2].step_labels, vec!["first", "second"]);
    assert_eq!(plan[3].step_ids, vec!["first"]);
    assert_eq!(plan[4].label, "Sequential agent wave");
    assert_eq!(plan[4].step_labels, vec!["Synthesize evidence"]);
    assert_eq!(plan[5].step_ids, vec!["follow-up"]);
    assert!(plan
        .iter()
        .all(|stage| !stage.step_ids.contains(&"disabled".to_string())));

    let mut auto_config = config;
    auto_config.orientation_schema = Some(serde_json::json!({
        "x-pipeline-contract": crate::auto_review::AUTO_REVIEW_CONTRACT
    }));
    let auto_plan = execution_plan(&auto_config).unwrap();
    assert_eq!(auto_plan[1].label, "Creating orientation map & review plan");
}

#[test]
fn resumed_runtime_emits_every_planned_scheduler_stage_id() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(assert_resumed_runtime_emits_every_planned_scheduler_stage_id());
}

#[test]
fn parallel_steps_without_edges_have_no_dependencies() {
    let steps = [
        make_step("a", Phase::Parallel),
        make_step("b", Phase::Parallel),
    ];
    let deps = deps_of(&steps);
    assert!(deps[0].is_empty());
    assert!(deps[1].is_empty());
}

#[test]
fn isolated_sequential_has_no_implicit_dependencies() {
    let steps = [
        make_step("a", Phase::Parallel),
        make_step("s", Phase::Sequential),
    ];
    let deps = deps_of(&steps);
    assert!(deps[1].is_empty());
}

#[test]
fn order_and_artifact_dependencies_are_unioned() {
    let a = make_step("a", Phase::Parallel);
    let c = make_step("c", Phase::Parallel);
    let mut b = make_step("b", Phase::Sequential);
    b.after = vec!["a".into()];
    b.context
        .include
        .push(crate::pipeline_config::ArtifactSelector::Step {
            step: "c".into(),
            parts: vec![crate::pipeline_config::StepArtifactPart::Report],
            glob: String::new(),
        });
    let deps = deps_of(&[a, c, b]);
    assert!(deps[0].is_empty());
    assert_eq!(
        deps[2],
        ["a".to_string(), "c".to_string()].into_iter().collect()
    );
}

#[test]
fn resolve_dependencies_empty() {
    let deps = deps_of(&[]);
    assert!(deps.is_empty());
}

#[test]
fn base_id_strips_agent() {
    assert_eq!(base_id("technical"), "technical");
    assert_eq!(base_id("technical/claude"), "technical");
}

#[test]
fn terminal_parallel_failure_unblocks_dependent_step() {
    let mut downstream = make_step("downstream", Phase::Sequential);
    downstream.after = vec!["ok".into(), "failed".into()];
    let steps = [
        make_step("ok", Phase::Parallel),
        make_step("failed", Phase::Parallel),
        downstream,
    ];
    let refs: Vec<&StepConfig> = steps.iter().collect();
    let deps = resolve_dependencies(&refs);
    let mut done = std::collections::HashSet::new();

    // A returned wave is terminal even when only one member produced an
    // output. Completion must follow dispatch, not StepOutput presence.
    mark_steps_done(&mut done, &refs[..2]);

    assert_eq!(ready_indices(&[2], &deps, &done), vec![2]);
}

#[test]
fn zero_match_fan_out_is_terminal_and_unblocks_dependent_step() {
    let temp = tempfile::tempdir().unwrap();
    let mut fan = make_step("fan", Phase::Parallel);
    fan.for_each = Some(crate::pipeline_config::ForEach {
        glob: "**/*.does-not-exist".into(),
        max: 20,
        artifact: None,
    });
    let mut downstream = make_step("downstream", Phase::Sequential);
    downstream.after = vec!["fan".into()];
    let steps = [fan, downstream];
    let refs: Vec<&StepConfig> = steps.iter().collect();
    let settings = crate::settings::Settings::default();
    let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);

    assert!(build_units(
        &steps[0],
        &settings,
        temp.path().to_str().unwrap(),
        None,
        &bus
    )
    .unwrap()
    .is_empty());

    let deps = resolve_dependencies(&refs);
    let mut done = std::collections::HashSet::new();
    mark_steps_done(&mut done, &refs[..1]);
    assert_eq!(ready_indices(&[1], &deps, &done), vec![1]);
}

#[test]
fn zero_match_fan_out_returns_visible_skipped_output() {
    let temp = tempfile::tempdir().unwrap();
    let mut fan = make_step("fan", Phase::Parallel);
    fan.for_each = Some(crate::pipeline_config::ForEach {
        glob: "**/*.does-not-exist".into(),
        max: 20,
        artifact: None,
    });
    let settings = crate::settings::Settings::default();
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(1));
    let output_budget = std::sync::Arc::new(OutputBudget::default());
    let shared_context_pool = crate::pipeline::context_cache::PreparedContextPool::default();
    let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
    let steps = [&fan];
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (outputs, failures, failed_calls) = runtime
        .block_on(run_parallel_wave(
            &bus,
            &steps,
            &settings,
            &semaphore,
            "",
            &serde_json::Value::Null,
            "",
            "",
            temp.path().to_str().unwrap(),
            "mixed",
            "",
            "{step_prompt}",
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &[],
            None,
            &output_budget,
            false,
            &shared_context_pool,
        ))
        .unwrap();
    assert!(failures.is_empty());
    assert!(failed_calls.is_empty());
    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].skipped);
    assert_eq!(outputs[0].step_id, "fan");
    assert!(outputs[0].raw_text.contains("matched no files"));
}

type WaveResult = Result<
    (
        Vec<StepOutput>,
        Vec<crate::models::StepFailure>,
        Vec<crate::models::StepCallRecord>,
    ),
    String,
>;

/// Runs one parallel wave with no provider calls possible: the worker
/// semaphore is closed when `closed` is set, so a dispatched unit can only
/// fail before resolving a model. Returns the wave result and its pass events.
fn run_wave_recording(
    steps: &[&StepConfig],
    prior_outputs: &[StepOutput],
    source: &std::path::Path,
    closed: bool,
) -> (WaveResult, Vec<serde_json::Value>) {
    let settings = crate::settings::Settings::default();
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(1));
    if closed {
        semaphore.close();
    }
    let output_budget = std::sync::Arc::new(OutputBudget::default());
    let shared_context_pool = crate::pipeline::context_cache::PreparedContextPool::default();
    let recorder = Arc::new(RecordingEvents::default());
    let bus: crate::emit::EventBus = recorder.clone();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = runtime.block_on(run_parallel_wave(
        &bus,
        steps,
        &settings,
        &semaphore,
        "",
        &serde_json::Value::Null,
        "",
        "",
        source.to_str().unwrap(),
        "mixed",
        "",
        "{step_prompt}",
        &Default::default(),
        &Default::default(),
        &Default::default(),
        prior_outputs,
        None,
        &output_budget,
        false,
        &shared_context_pool,
    ));
    let passes = recorder
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|(event, _)| event == "pipeline:pass")
        .map(|(_, payload)| payload.clone())
        .collect();
    (result, passes)
}

#[test]
fn fan_out_discovery_error_fails_only_its_own_step() {
    let temp = tempfile::tempdir().unwrap();
    let mut fan = make_step("fan", Phase::Parallel);
    fan.for_each = Some(crate::pipeline_config::ForEach {
        glob: String::new(),
        max: 20,
        artifact: Some(crate::pipeline_config::ForEachArtifact {
            step: "notes".into(),
            pointer: "/items".into(),
        }),
    });
    let mut sibling = make_step("sibling", Phase::Parallel);
    sibling.for_each = Some(crate::pipeline_config::ForEach {
        glob: "**/*.does-not-exist".into(),
        max: 20,
        artifact: None,
    });
    let notes = StepOutput {
        step_id: "notes".into(),
        step_label: "Notes".into(),
        raw_text: "## Notes\n\nPlain Markdown, not JSON.".into(),
        ..Default::default()
    };

    let (result, passes) = run_wave_recording(&[&fan, &sibling], &[notes], temp.path(), false);
    let (outputs, failures, _) = result.expect("a discovery error must not abort the wave");

    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].step_id, "sibling");
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].step_id, "fan");
    assert!(
        failures[0].error.contains("needs structured JSON"),
        "{}",
        failures[0].error
    );
    assert!(passes.contains(&serde_json::json!({"name": "fan", "status": "error"})));
}

#[test]
fn unit_that_stops_before_its_provider_call_reports_an_error_status() {
    let temp = tempfile::tempdir().unwrap();
    let solo = make_step("solo", Phase::Parallel);

    let (result, passes) = run_wave_recording(&[&solo], &[], temp.path(), true);
    let (outputs, failures, failed_calls) = result.unwrap();

    assert!(outputs.is_empty());
    assert!(failed_calls.is_empty(), "no provider call was made");
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].step_id, "solo");
    assert_eq!(
        passes,
        [
            serde_json::json!({"name": "solo", "status": "pending"}),
            serde_json::json!({"name": "solo", "status": "error"}),
        ]
    );
}

#[test]
fn unit_preparation_error_fails_only_its_own_unit() {
    let temp = tempfile::tempdir().unwrap();
    let first = make_step("first", Phase::Parallel);
    let mut second = make_step("second", Phase::Parallel);
    second.context.include.push(ArtifactSelector::Step {
        step: "first".into(),
        parts: Vec::new(),
        glob: String::new(),
    });

    let (result, passes) = run_wave_recording(&[&first, &second], &[], temp.path(), true);
    let (outputs, mut failures, _) =
        result.expect("a unit that cannot be prepared must not abort the wave");

    assert!(outputs.is_empty());
    failures.sort_by(|a, b| a.step_id.cmp(&b.step_id));
    let failed: Vec<_> = failures.iter().map(|f| f.step_id.as_str()).collect();
    assert_eq!(failed, ["first", "second"]);
    assert!(
        failures[1]
            .error
            .contains("cannot consume another step's output"),
        "{}",
        failures[1].error
    );
    // The unit dispatched earlier still settles; the unprepared one is
    // reported failed without being announced as queued.
    assert!(passes.contains(&serde_json::json!({"name": "first", "status": "error"})));
    assert!(passes.contains(&serde_json::json!({"name": "second", "status": "error"})));
    assert!(!passes.contains(&serde_json::json!({"name": "second", "status": "pending"})));
}

#[test]
fn abandoned_wave_settles_every_unfinished_unit() {
    use super::super::parallel::{abandon, Announced, ParallelTaskResult};
    let recorder = Arc::new(RecordingEvents::default());
    let bus: crate::emit::EventBus = recorder.clone();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut tasks: tokio::task::JoinSet<ParallelTaskResult> = tokio::task::JoinSet::new();
        let mut announced = Announced::new();
        let mut track = |task: &tokio::task::AbortHandle, key: &str| {
            announced.insert(task.id(), (key.to_string(), key.to_uppercase()));
        };
        track(&tasks.spawn(std::future::pending()), "waiting");
        let broken = tasks.spawn(async { panic!("unit bug") });
        track(&broken, "broken");
        // A unit that already reported its own terminal status.
        let finished = tasks.spawn(async {
            Err(crate::models::StepFailure {
                step_id: "finished".into(),
                step_label: "Finished".into(),
                phase: "parallel".into(),
                error: "Provider failed".into(),
            }
            .into())
        });
        track(&finished, "finished");
        // Let the units that can end do so before the wave returns early.
        while !(broken.is_finished() && finished.is_finished()) {
            tokio::task::yield_now().await;
        }
        abandon(&bus, &mut tasks, &announced).await;
        assert!(tasks.is_empty());
    });
    let passes: Vec<_> = recorder
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|(event, _)| event == "pipeline:pass")
        .map(|(_, payload)| payload.clone())
        .collect();
    assert_eq!(passes.len(), 2, "{passes:?}");
    assert!(passes.contains(&serde_json::json!({"name": "waiting", "status": "error"})));
    assert!(passes.contains(&serde_json::json!({"name": "broken", "status": "error"})));
}

#[test]
fn dependents_of_finds_transitive_downstream() {
    use crate::pipeline_config::{MergeConfig, PipelineConfig};
    let mut synthesis = make_step("s", Phase::Sequential);
    synthesis.context.include = vec![ArtifactSelector::Step {
        step: "a".into(),
        parts: vec![StepArtifactPart::Report],
        glob: String::new(),
    }];
    let config = PipelineConfig {
        steps: vec![
            make_step("a", Phase::Parallel),
            make_step("b", Phase::Parallel),
            synthesis,
        ],
        merge: MergeConfig::default(),
        outputs: Default::default(),
        context_cache: Default::default(),
        use_orientation: true,
        orientation_prompt: String::new(),
        orientation_schema: None,
        extraction: Default::default(),
        parallel_context_template: String::new(),
        variables: Vec::new(),
    };
    // Re-running 'a' means 's' (which consumes it) must re-run; 'b' need not.
    let seeds: std::collections::HashSet<String> = ["a".to_string()].into_iter().collect();
    let deps = dependents_of(&config, &seeds);
    assert!(deps.contains("s"));
    assert!(!deps.contains("b"));
    assert!(!deps.contains("a")); // seeds excluded
}
