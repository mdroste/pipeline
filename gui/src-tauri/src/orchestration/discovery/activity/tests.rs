use super::*;

#[test]
fn reads_the_active_step_and_recent_history_without_changing_the_run() {
    use crate::orchestration::{definition::*, state::Receipt};
    let temp = tempfile::tempdir().unwrap();
    let s = store::Store::open(temp.path()).unwrap();
    let mut p = super::super::tests::fixture(super::super::model::Mode::Unsupervised, 50);
    storage::create(&s, &p.run, "create", "f").unwrap();
    let chain = Chain {
        schema_version: 1,
        name: "Identify research standards".into(),
        description: String::new(),
        limits: Limits {
            max_api_cost_cents: None,
            max_actions: 1,
            deadline_hours: 1,
            action_timeout_secs: 30,
        },
        steps: vec![crate::orchestration::definition::Step {
            id: "action".into(),
            label: "Identify".into(),
            action: Action::Workspace {
                prompt: "Inspect".into(),
                model: None,
                effort: None,
            },
        }],
    };
    storage::admit(
        &s,
        &mut p,
        chain,
        store::Scope {
            session_id: Some("role-session".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let mut child = s.get(p.run.active_child.as_ref().unwrap()).unwrap();
    child.progress.receipts.insert(
        "action".into(),
        Receipt {
            action_kind: None,
            sequence: 1,
            address: "action".into(),
            step_id: "action".into(),
            label: "Identify".into(),
            state: "running".into(),
            operation: store::id(),
            started_at: 123,
            finished_at: None,
            wake_at: None,
            output: None,
            error: None,
            child: None,
        },
    );
    s.save(&mut child, "started", "Starting").unwrap();
    let revision = p.run.revision;
    let (activity, children) = context(&s, &p.run.id).unwrap();
    let step = activity.step.unwrap();
    assert_eq!(step.started_at, Some(123));
    assert_eq!(step.state, "running");
    assert_eq!(step.label, "Identify research standards");
    assert_eq!(children[0].session.as_deref(), Some("role-session"));
    assert!(!activity.entries.is_empty());
    assert_eq!(storage::get(&s, &p.run.id).unwrap().revision, revision);
}

#[test]
fn partial_team_counts_and_terminal_states_follow_observations() {
    let mut activity = Activity {
        revision: 1,
        observed_at: 20,
        last_activity_at: Some(1),
        step: Some(Step {
            id: "step".into(),
            label: "Ideas".into(),
            state: "running".into(),
            started_at: Some(10),
            finished_at: None,
        }),
        entries: vec![],
        agents: vec![],
    };
    let child = Child {
        id: "step".into(),
        label: "Ideas".into(),
        state: "failed".into(),
        session: None,
        operation: None,
        started: Some(10),
        finished: Some(20),
        updated: 20,
        team: None,
    };
    let snapshot = Snapshot {
        last_activity_at: Some(15),
        entries: vec![Entry {
            id: "a".into(),
            at: 15,
            label: "Idea agent 1".into(),
            state: "completed".into(),
        }],
        agents: vec![
            Agent {
                id: "a".into(),
                label: "Idea agent 1".into(),
                state: "completed".into(),
                updated_at: 15,
            },
            Agent {
                id: "b".into(),
                label: "Idea agent 2".into(),
                state: "running".into(),
                updated_at: 12,
            },
        ],
    };
    merge(&mut activity, &child, snapshot);
    assert_eq!(activity.last_activity_at, Some(20));
    assert_eq!(activity.agents[1].state, "completed");
    assert_eq!(activity.agents[2].state, "interrupted");
    assert!(activity.entries.iter().any(|e| e.id == "step-a"));
}

#[test]
fn progress_journals_survive_reopening_and_reject_oversized_or_invalid_paths() {
    let temp = tempfile::tempdir().unwrap();
    let store = store::Store::open(temp.path()).unwrap();
    let op = "a".repeat(32);
    let path = store.root.join("actions").join(&op).join("progress.json");
    let value = serde_json::to_value(Snapshot::default()).unwrap();
    super::super::adapters::atomic_json(&path, &value).unwrap();
    let reopened = store::Store::open(temp.path()).unwrap();
    assert!(super::super::adapters::idea_team::read(&reopened, &op)
        .unwrap()
        .is_some());
    assert!(super::super::adapters::idea_team::read(&reopened, "../outside").is_err());
    std::fs::write(path, vec![b' '; 65537]).unwrap();
    assert!(super::super::adapters::idea_team::read(&reopened, &op).is_err());
}
