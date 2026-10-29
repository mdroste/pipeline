//! Analyzer steps through the real scheduler, without any provider.
use super::*;
use crate::pipeline_config::{AnalyzerSpec, StepKind};

fn analyzer(id: &str, phase: Phase, analyzer: &str) -> StepConfig {
    StepConfig {
        id: id.into(),
        label: id.into(),
        phase,
        kind: StepKind::Analyzer,
        analyzer: Some(AnalyzerSpec {
            id: analyzer.into(),
            version: 1,
            options: serde_json::Value::Null,
        }),
        ..Default::default()
    }
}

fn config(steps: Vec<StepConfig>) -> PipelineConfig {
    PipelineConfig {
        steps,
        merge: Default::default(),
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

async fn run(
    config: &PipelineConfig,
    paper_text_path: &str,
    source_path: &str,
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
        paper_text_path,
        "",
        source_path,
        "",
        "",
        &empty,
        &empty,
        &empty,
        &preloaded,
        None,
        &crate::settings::Settings::default(),
    )
    .await
    .unwrap();
    let events = recorder.0.lock().unwrap().clone();
    (result, events)
}

#[tokio::test]
async fn analyzer_steps_run_on_the_host_in_both_phases() {
    let package = tempfile::tempdir().unwrap();
    std::fs::write(package.path().join("main.do"), "use \"data.dta\"\n").unwrap();
    let paper = package.path().join("paper.txt");
    std::fs::write(
        &paper,
        "Body.\n\nReferences\n[1] Card, D. 1994. Minimum wages. AER.\n[2] Neumark, D. 2000. A comment. AER.\n[3] Autor, D. 2013. The China syndrome. AER.",
    )
    .unwrap();
    let mut refs = analyzer("refs", Phase::Sequential, "reference_audit");
    refs.after = vec!["package".into()];
    let workflow = config(vec![
        analyzer("package", Phase::Parallel, "replication_static"),
        refs,
    ]);
    let (result, events) = run(
        &workflow,
        paper.to_str().unwrap(),
        package.path().to_str().unwrap(),
    )
    .await;
    assert!(
        result.failed_steps.is_empty(),
        "{:?}",
        result.failed_steps.first().map(|f| &f.error)
    );
    assert!(result.budget_stop.is_none());
    assert_eq!(result.outputs.len(), 2);
    for output in &result.outputs {
        assert_eq!(output.provider, "pipeline");
        assert!(output.structured_json);
        assert!(output.calls.is_empty());
        assert_eq!(output.input_tokens, 0);
    }
    let package_output: serde_json::Value =
        serde_json::from_str(&result.outputs[0].raw_text).unwrap();
    assert_eq!(package_output["analyzer"], "replication_static");
    let refs_output: serde_json::Value = serde_json::from_str(&result.outputs[1].raw_text).unwrap();
    assert_eq!(refs_output["reference_count"], 3);
    assert_eq!(result.outputs[1].phase, "sequential");
    let done: Vec<&str> = events
        .iter()
        .filter(|(event, payload)| event == "pipeline:pass" && payload["status"] == "done")
        .filter_map(|(_, payload)| payload["name"].as_str())
        .collect();
    assert_eq!(done, vec!["package", "refs"]);
}

#[tokio::test]
async fn a_failed_parallel_check_is_recorded_and_the_run_continues() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("paper.pdf");
    std::fs::write(&file, "not a folder").unwrap();
    let mut refs = analyzer("refs", Phase::Sequential, "reference_audit");
    refs.after = vec!["package".into()];
    let workflow = config(vec![
        analyzer("package", Phase::Parallel, "replication_static"),
        refs,
    ]);
    let (result, _) = run(&workflow, "", file.to_str().unwrap()).await;
    assert_eq!(result.failed_steps.len(), 1);
    assert_eq!(result.failed_steps[0].step_id, "package");
    assert!(result.failed_steps[0].error.contains("needs a folder"));
    assert_eq!(result.outputs.len(), 1);
    assert_eq!(result.outputs[0].step_id, "refs");
}
