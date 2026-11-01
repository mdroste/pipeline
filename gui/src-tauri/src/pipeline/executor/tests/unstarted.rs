//! Waves the scheduler never starts are reported as skipped stages, with the
//! preflight plan's ids, before any pass event for their steps.
use super::*;
use crate::pipeline_config::{AnalyzerSpec, StepKind};

fn analyzer(id: &str, phase: Phase) -> StepConfig {
    StepConfig {
        id: id.into(),
        label: id.into(),
        phase,
        enabled: true,
        kind: StepKind::Analyzer,
        analyzer: Some(AnalyzerSpec {
            id: "replication_static".into(),
            version: 1,
            options: serde_json::Value::Null,
        }),
        ..Default::default()
    }
}

fn config(steps: Vec<StepConfig>) -> PipelineConfig {
    PipelineConfig {
        steps,
        merge: crate::pipeline_config::MergeConfig {
            enabled: true,
            ..Default::default()
        },
        outputs: Default::default(),
        context_cache: Default::default(),
        use_orientation: true,
        orientation_prompt: String::new(),
        orientation_schema: None,
        extraction: Default::default(),
        parallel_context_template: String::new(),
        variables: Vec::new(),
    }
}

/// `package` (sequential) -> `refs` (sequential) -> `late` (parallel, two
/// providers, so its wave plans a merge).
fn chain() -> PipelineConfig {
    let mut refs = analyzer("refs", Phase::Sequential);
    refs.after = vec!["package".into()];
    let late = StepConfig {
        id: "late".into(),
        label: "Late".into(),
        phase: Phase::Parallel,
        enabled: true,
        agents: vec!["claude".into(), "codex".into()],
        after: vec!["refs".into()],
        ..Default::default()
    };
    config(vec![analyzer("package", Phase::Sequential), refs, late])
}

async fn run(
    config: &PipelineConfig,
    source_path: &str,
    settings: &crate::settings::Settings,
) -> (ExecutionResult, Vec<(String, serde_json::Value)>) {
    let recorder = Arc::new(RecordingEvents::default());
    let bus: crate::emit::EventBus = recorder.clone();
    let empty = std::collections::HashMap::new();
    let preloaded = std::collections::HashMap::new();
    let result = execute_steps(
        &bus,
        config,
        "",
        &serde_json::Value::Null,
        "",
        "",
        source_path,
        "",
        "",
        &empty,
        &empty,
        &empty,
        &preloaded,
        None,
        settings,
    )
    .await
    .unwrap();
    let events = recorder.0.lock().unwrap().clone();
    (result, events)
}

fn skipped_stages(events: &[(String, serde_json::Value)]) -> Vec<(String, Vec<String>)> {
    events
        .iter()
        .filter(|(event, payload)| event == "pipeline:stage" && payload["skipped"] == true)
        .map(|(_, payload)| {
            let ids = payload["stepIds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_str().unwrap().to_string())
                .collect();
            (payload["id"].as_str().unwrap().to_string(), ids)
        })
        .collect()
}

fn planned_waves(config: &PipelineConfig) -> Vec<(String, Vec<String>)> {
    execution_plan(config)
        .unwrap()
        .into_iter()
        .filter(|stage| stage.id.starts_with("wave-"))
        .map(|stage| (stage.id, stage.step_ids))
        .collect()
}

#[tokio::test]
async fn a_failed_sequential_step_reports_later_waves_as_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let not_a_folder = dir.path().join("paper.pdf");
    std::fs::write(&not_a_folder, "not a folder").unwrap();
    let workflow = chain();
    let (result, events) = run(
        &workflow,
        not_a_folder.to_str().unwrap(),
        &crate::settings::Settings::default(),
    )
    .await;

    assert_eq!(result.failed_steps.len(), 1);
    assert_eq!(result.failed_steps[0].step_id, "package");
    // Every wave after the failed one, under the plan's own ids.
    assert_eq!(
        skipped_stages(&events),
        planned_waves(&workflow)[1..].to_vec()
    );
    assert!(!events
        .iter()
        .any(|(event, payload)| event == "pipeline:pass"
            && payload["status"] == "done"
            && payload["name"] != "package"));
}

#[tokio::test]
async fn a_review_limit_reached_before_any_wave_skips_every_wave_first() {
    let scope = crate::run_scope::ActiveScope::begin("unstarted-waves-test");
    let workflow = chain();
    let settings = crate::settings::Settings {
        review_max_calls: 2,
        ..Default::default()
    };
    let (result, events) = crate::run_scope::scoped(scope.scope().clone(), async {
        // Orientation and one retry happen before the first wave.
        crate::pipeline::logging::record_provider_attempt();
        crate::pipeline::logging::record_provider_attempt();
        run(&workflow, "", &settings).await
    })
    .await;

    assert_eq!(
        result.budget_stop.as_deref(),
        Some("limit of 2 model calls (2 made)")
    );
    assert!(result.outputs.is_empty());
    assert_eq!(result.failed_steps.len(), 3);
    assert!(result.failed_steps.iter().all(|failure| failure
        .error
        .starts_with(super::super::budget::BUDGET_STOP_PREFIX)));
    assert_eq!(skipped_stages(&events), planned_waves(&workflow));
    // The not-started errors follow every skipped stage, so the progress view
    // cannot attach them to an earlier wave.
    let last_stage = events
        .iter()
        .rposition(|(event, _)| event == "pipeline:stage")
        .unwrap();
    let first_pass = events
        .iter()
        .position(|(event, _)| event == "pipeline:pass")
        .unwrap();
    assert!(last_stage < first_pass);
}

#[tokio::test]
async fn a_skipped_step_is_checkpointed_for_recovery() {
    let skipped = |id: &str, phase: Phase| StepConfig {
        run_if: Some(crate::pipeline_config::RunCondition::OutputMatches {
            step: "absent".into(),
            pattern: "x".into(),
            negate: false,
        }),
        ..analyzer(id, phase)
    };
    let workflow = config(vec![
        skipped("parallel-step", Phase::Parallel),
        skipped("sequential-step", Phase::Sequential),
    ]);
    let run_dir = tempfile::tempdir().unwrap();
    let bus: crate::emit::EventBus = Arc::new(crate::emit::NullEvents);
    let empty = std::collections::HashMap::new();
    let preloaded = std::collections::HashMap::new();
    execute_steps(
        &bus,
        &workflow,
        "",
        &serde_json::Value::Null,
        "",
        "",
        "",
        "",
        "",
        &empty,
        &empty,
        &empty,
        &preloaded,
        run_dir.path().to_str(),
        &crate::settings::Settings::default(),
    )
    .await
    .unwrap();

    let mut recorded = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(run_dir.path().join("checkpoints")).unwrap() {
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(entry.unwrap().path()).unwrap()).unwrap();
        recorded.insert(
            value["steps"][0].as_str().unwrap().to_string(),
            value["outputs"][0]["skipped"].clone(),
        );
    }
    assert_eq!(recorded["parallel-step"], true);
    assert_eq!(recorded["sequential-step"], true);
}
