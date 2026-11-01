//! Stopping a Review through its run scope, with real provider calls that
//! hang at a local server.

use super::*;
use crate::run_scope::fixtures::{hanging_provider, until_called};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// The run scope each unit sees while it reports itself running.
#[derive(Default)]
struct UnitScopes(Mutex<Vec<String>>);

impl crate::emit::Events for UnitScopes {
    fn emit_event(
        &self,
        event: &str,
        payload: serde_json::Value,
    ) -> Result<(), crate::emit::EmitError> {
        if event == "pipeline:pass" && payload["status"] == "running" {
            let scope = crate::run_scope::current();
            self.0.lock().unwrap().push(scope.key().to_string());
        }
        Ok(())
    }
}

#[tokio::test]
async fn stopping_a_review_stops_its_parallel_wave_and_keeps_its_usage() {
    let (url, calls) = hanging_provider();
    let folder = tempfile::tempdir().unwrap();
    std::fs::write(folder.path().join("a.txt"), "first").unwrap();
    std::fs::write(folder.path().join("b.txt"), "second").unwrap();
    let mut fan = make_step("fan", Phase::Parallel);
    fan.agents = vec!["local".into()];
    fan.for_each = Some(crate::pipeline_config::ForEach {
        glob: "*.txt".into(),
        max: 20,
        artifact: None,
    });
    let settings = crate::settings::Settings {
        local_base_url: url,
        local_model: "fixture".into(),
        max_retries: 1,
        ..Default::default()
    };
    // One worker: the second unit waits for a permit and must never start.
    let semaphore = Arc::new(tokio::sync::Semaphore::new(1));
    let output_budget = Arc::new(OutputBudget::default());
    let shared_context_pool = crate::pipeline::context_cache::PreparedContextPool::default();
    let scopes = Arc::new(UnitScopes::default());
    let bus: crate::emit::EventBus = scopes.clone();
    let steps = [&fan];
    let run = crate::run_scope::ActiveScope::begin("parallel-wave-stop-test");
    let no_values = std::collections::HashMap::new();

    let wave = crate::run_scope::scoped(
        run.scope().clone(),
        run_parallel_wave(
            &bus,
            &steps,
            &settings,
            &semaphore,
            "",
            &serde_json::Value::Null,
            "",
            "",
            folder.path().to_str().unwrap(),
            "mixed",
            "",
            "{step_prompt}",
            &no_values,
            &no_values,
            &no_values,
            &[],
            None,
            &output_budget,
            false,
            &shared_context_pool,
        ),
    );
    let stop = async {
        until_called(&calls, 1).await;
        run.scope().cancel();
        Instant::now()
    };
    let (result, stopped_at) =
        tokio::time::timeout(Duration::from_secs(40), async { tokio::join!(wave, stop) })
            .await
            .expect("a stopped wave must return");

    assert!(
        stopped_at.elapsed() < super::super::UNIT_STOP_GRACE,
        "the wave took {:?} to stop",
        stopped_at.elapsed()
    );
    assert_eq!(
        *scopes.0.lock().unwrap(),
        vec!["parallel-wave-stop-test".to_string()],
        "only the unit that started runs, inside the run's scope"
    );
    assert_eq!(run.scope().usage().provider_attempts, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let (outputs, failures, _) = result.unwrap();
    assert!(outputs.is_empty());
    assert_eq!(failures.len(), 2);
    assert!(failures
        .iter()
        .all(|failure| crate::commands::is_pipeline_cancellation_error(&failure.error)));
}

#[tokio::test]
async fn stopping_a_review_stops_its_merge_and_keeps_its_usage() {
    let (url, calls) = hanging_provider();
    let settings = crate::settings::Settings {
        local_base_url: url,
        local_model: "fixture".into(),
        ..Default::default()
    };
    let unit = |agent: &str| StepOutput {
        step_id: format!("step/{agent}"),
        step_label: "Step".into(),
        merge_group: "step".into(),
        agent: agent.into(),
        raw_text: "analysis".into(),
        ..Default::default()
    };
    let merge = crate::pipeline_config::MergeConfig {
        enabled: true,
        prompt: "{topic}\n{agent_reports}".into(),
        agents: vec!["local".into()],
    };
    let semaphore = Arc::new(tokio::sync::Semaphore::new(1));
    let scopes = Arc::new(UnitScopes::default());
    let bus: crate::emit::EventBus = scopes.clone();
    let run = crate::run_scope::ActiveScope::begin("merge-stop-test");
    let no_schemas = std::collections::BTreeMap::new();

    let merged = crate::run_scope::scoped(
        run.scope().clone(),
        crate::pipeline::merge::merge_step_outputs(
            &bus,
            vec![unit("claude"), unit("codex")],
            &merge,
            &semaphore,
            None,
            &settings,
            &no_schemas,
        ),
    );
    let stop = async {
        until_called(&calls, 1).await;
        run.scope().cancel();
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(40), async {
        tokio::join!(merged, stop)
    })
    .await
    .expect("a stopped merge must return");

    assert!(crate::commands::is_pipeline_cancellation_error(
        &result.unwrap_err()
    ));
    assert_eq!(
        *scopes.0.lock().unwrap(),
        vec!["merge-stop-test".to_string()]
    );
    assert_eq!(run.scope().usage().provider_attempts, 1);
}

#[tokio::test]
async fn cancelling_one_pass_leaves_the_rest_of_the_wave_running() {
    let (url, calls) = hanging_provider();
    let folder = tempfile::tempdir().unwrap();
    let mut one = make_step("one", Phase::Parallel);
    one.agents = vec!["local".into()];
    let mut two = make_step("two", Phase::Parallel);
    two.agents = vec!["local".into()];
    let settings = crate::settings::Settings {
        local_base_url: url,
        local_model: "fixture".into(),
        max_retries: 1,
        ..Default::default()
    };
    let semaphore = Arc::new(tokio::sync::Semaphore::new(2));
    let output_budget = Arc::new(OutputBudget::default());
    let shared_context_pool = crate::pipeline::context_cache::PreparedContextPool::default();
    let bus: crate::emit::EventBus = Arc::new(crate::emit::NullEvents);
    let steps = [&one, &two];
    let run = crate::run_scope::ActiveScope::begin("parallel-pass-cancel-test");
    let no_values = std::collections::HashMap::new();

    let wave = crate::run_scope::scoped(
        run.scope().clone(),
        run_parallel_wave(
            &bus,
            &steps,
            &settings,
            &semaphore,
            "",
            &serde_json::Value::Null,
            "",
            "",
            folder.path().to_str().unwrap(),
            "mixed",
            "",
            "{step_prompt}",
            &no_values,
            &no_values,
            &no_values,
            &[],
            None,
            &output_budget,
            false,
            &shared_context_pool,
        ),
    );
    let stop = async {
        until_called(&calls, 2).await;
        // What the progress view's stop control sends for one running pass.
        crate::commands::cancel_pass("one".into(), Some(run.scope().key().to_string()))
            .await
            .unwrap();
        // The cancelled pass must not retry; its neighbor keeps waiting.
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        run.scope().cancel();
    };
    let (result, ()) =
        tokio::time::timeout(Duration::from_secs(40), async { tokio::join!(wave, stop) })
            .await
            .expect("a stopped wave must return");

    let (_, failures, _) = result.unwrap();
    let error = |step: &str| {
        failures
            .iter()
            .find(|failure| failure.step_id == step)
            .map(|failure| failure.error.as_str())
    };
    assert_eq!(error("one"), Some("Pass 'one' cancelled"));
    assert_eq!(error("two"), Some("Pipeline cancelled"));
}
