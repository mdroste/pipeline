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
        None,
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

/// Accepts Crossref requests and never answers them.
fn silent_crossref() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept() {
            held.push(stream);
        }
    });
    format!("http://{address}")
}

struct WaitingCheck {
    _folder: tempfile::TempDir,
    paper: String,
    step: StepConfig,
    api: String,
}

/// A reference check waiting on its first Crossref request.
fn waiting_check() -> WaitingCheck {
    let folder = tempfile::tempdir().unwrap();
    let paper = folder.path().join("paper.txt");
    std::fs::write(
        &paper,
        "Body.\n\nReferences\n[1] Card, D. 1994. Minimum wages. AER.\n[2] Neumark, D. 2000. A comment. AER.\n[3] Autor, D. 2013. The China syndrome. AER.",
    )
    .unwrap();
    let mut step = analyzer("refs", Phase::Sequential, "reference_audit");
    step.analyzer.as_mut().unwrap().options = serde_json::json!({"network": true});
    WaitingCheck {
        paper: paper.to_str().unwrap().to_string(),
        _folder: folder,
        step,
        api: silent_crossref(),
    }
}

async fn run_check(
    check: &WaitingCheck,
) -> Result<Result<StepOutput, crate::models::StepFailure>, String> {
    let bus: crate::emit::EventBus = Arc::new(crate::emit::NullEvents);
    super::super::analyzer_step::analyzer_result(
        &bus,
        &check.step,
        &super::super::analyzer_step::AnalyzerInputs {
            document_bundle_path: "",
            paper_text_path: &check.paper,
            source_path: "",
            crossref_api: &check.api,
        },
    )
    .await
}

#[tokio::test]
async fn the_x_on_a_waiting_reference_check_fails_only_that_check() {
    let check = waiting_check();
    let run = crate::run_scope::ActiveScope::begin("analyzer-pass-cancel-test");
    let scope = run.scope().clone();
    let canceller = scope.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        canceller.cancel_pass("refs");
    });
    let started = std::time::Instant::now();
    let outcome = crate::run_scope::scoped(scope, run_check(&check)).await;
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    let failure = outcome.unwrap().unwrap_err();
    assert_eq!(failure.error, "Pass 'refs' cancelled");
}

#[tokio::test]
async fn stop_ends_a_waiting_reference_check_at_once() {
    let check = waiting_check();
    let run = crate::run_scope::ActiveScope::begin("analyzer-stop-test");
    let scope = run.scope().clone();
    let canceller = scope.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        canceller.cancel();
    });
    let started = std::time::Instant::now();
    let outcome = crate::run_scope::scoped(scope, run_check(&check)).await;
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert_eq!(outcome.err().as_deref(), Some("Pipeline cancelled"));
}
