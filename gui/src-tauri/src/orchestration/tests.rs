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

mod coordinator;

mod availability;
mod interpreter;
mod receipts;
mod schedules;
mod snapshots;
