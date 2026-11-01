use super::*;
use definition::*;
use serde_json::json;
use triggers::*;
fn chain(steps: Vec<Step>) -> Chain {
    Chain {
        schema_version: 1,
        name: "Test".into(),
        description: String::new(),
        steps,
        limits: Limits::default(),
    }
}
fn step(id: &str, action: Action) -> Step {
    Step {
        id: id.into(),
        label: id.into(),
        action,
    }
}
fn delay(id: &str) -> Step {
    step(id, Action::Delay { seconds: 60 })
}
fn done(p: &mut state::Progress, ready: &state::Ready, value: Value) {
    p.receipts.insert(
        ready.address.clone(),
        Receipt {
            sequence: 0,
            address: ready.address.clone(),
            step_id: ready.step.id.clone(),
            label: ready.step.label.clone(),
            state: "completed".into(),
            operation: store::id(),
            started_at: 0,
            finished_at: Some(1),
            wake_at: None,
            output: Some(value.clone()),
            error: None,
            child: None,
        },
    );
    p.outputs[&ready.step.id] = value;
}
fn output(step: &str, pointer: &str) -> Binding {
    Binding::Output {
        step: step.into(),
        pointer: pointer.into(),
    }
}

fn failed_attempt(s: &Store, status: &str) -> TaskRun {
    let mut run = s
        .create(
            "retry-fixture",
            chain(vec![delay("action")]),
            Scope::default(),
            json!({}),
            0,
            true,
        )
        .unwrap();
    run.state = "attention".into();
    run.progress.actions = 1;
    run.progress.receipts.insert(
        "action".into(),
        Receipt {
            sequence: 1,
            address: "action".into(),
            step_id: "action".into(),
            label: "Prior action".into(),
            state: status.into(),
            operation: store::id(),
            started_at: 10,
            finished_at: Some(20),
            wake_at: None,
            output: Some(json!({"preview":"Original"})),
            error: Some("Interrupted action".into()),
            child: Some(json!({"runId":"original-child"})),
        },
    );
    s.save(&mut run, "attention", "Interrupted action").unwrap();
    run
}

/// `run_store` always opens the default Workspace store, so tests that cross
/// that boundary share one disposable root for the life of the test process.
/// Only a debug build honors the override, and a release test run must stop
/// here rather than reach real data.
pub(crate) fn disposable_workspace() -> crate::workbench::store::Store {
    static ROOT: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    if !cfg!(debug_assertions) {
        panic!("Only a debug build can redirect the Workspace store");
    }
    ROOT.get_or_init(|| {
        let root = tempfile::tempdir().unwrap();
        std::env::set_var("PIPELINE_WORKBENCH_DEV_ROOT", root.path());
        root
    });
    crate::workbench::store::Store::open_default().unwrap()
}

/// The turn permit and active turn state are process-wide, so tests that touch
/// them take turns.
pub(crate) async fn exclusive_turn_state() -> tokio::sync::MutexGuard<'static, ()> {
    static GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    GATE.lock().await
}

/// Waits for a dispatched action and returns its run once the action settles.
pub(super) async fn settled(c: &Coordinator, id: &str) -> TaskRun {
    for _ in 0..1000 {
        let run = c.store.get(id).unwrap();
        if run.progress.receipts.values().any(|r| r.state != "running") {
            return run;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("The dispatched action never settled");
}

mod coordinator;

mod availability;
mod interpreter;
mod receipts;
mod recovery;
mod schedules;
mod snapshots;
