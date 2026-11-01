//! Stopping, time limits and saved-result recovery at the edges of a run.
use super::coordinator::{coordinator_fixture, finished_run};
use super::*;
use std::time::Duration;

fn over_budget(m: &Coordinator, name: &str, receipt: &str) -> TaskRun {
    let mut run = m
        .store
        .create(
            name,
            chain(vec![delay("step")]),
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    let Advance::Ready(ready) = state::advance(&run.chain, &mut run.progress, &run.inputs) else {
        panic!()
    };
    done(&mut run.progress, &ready[0], json!({}));
    run.progress.receipts.get_mut("step").unwrap().state = receipt.into();
    run.progress.outputs["large"] = json!("x".repeat(state::MAX_PROGRESS_BYTES));
    run.state = "running".into();
    // Input could once pass the budget, and old records can hold it.
    m.store.write(&mut run, "fixture", "Over budget").unwrap();
    assert!(!state::context_within_budget(&run.progress));
    run
}

#[tokio::test]
async fn a_run_over_its_context_budget_can_be_stopped() {
    let (_temp, m) = coordinator_fixture();
    let idle = over_budget(&m, "idle", "input");
    let stopped = commands::control(&m, idle.id.clone(), idle.revision, "stop".into())
        .await
        .unwrap();
    assert_eq!(stopped.state, "cancelled");
    assert_eq!(m.store.get(&idle.id).unwrap().state, "cancelled");
    assert_eq!(stopped.reason.as_deref(), Some("Stopped by you"));
}

#[tokio::test]
async fn a_run_over_budget_with_an_action_running_ends_when_stopped() {
    let (_temp, m) = coordinator_fixture();
    let busy = over_budget(&m, "busy", "running");
    let stopped = commands::control(&m, busy.id.clone(), busy.revision, "stop".into())
        .await
        .unwrap();
    assert_eq!(stopped.state, "cancelled");
    assert_eq!(stopped.progress.receipts["step"].state, "unknown");
    // The action's late report changes nothing.
    m.complete(&busy.id, "step", Err("Task stopped".into()))
        .await
        .unwrap();
    assert_eq!(m.store.get(&busy.id).unwrap().state, "cancelled");
}

#[tokio::test]
async fn a_managed_action_over_budget_can_be_stopped_by_its_owner() {
    let (_temp, m) = coordinator_fixture();
    let child = over_budget(&m, "child", "running");
    missions::stop_child(&m, &child.id).await.unwrap();
    assert_eq!(m.store.get(&child.id).unwrap().state, "cancelling");
    m.complete(&child.id, "step", Err("Task stopped".into()))
        .await
        .unwrap();
    assert_eq!(m.store.get(&child.id).unwrap().state, "cancelled");
}

#[tokio::test]
async fn a_stop_stands_when_the_last_result_would_pass_the_budget() {
    let (_temp, m) = coordinator_fixture();
    let mut run = m
        .store
        .create(
            "late",
            chain(vec![delay("step")]),
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    let Advance::Ready(ready) = state::advance(&run.chain, &mut run.progress, &run.inputs) else {
        panic!()
    };
    done(&mut run.progress, &ready[0], json!({}));
    run.progress.receipts.get_mut("step").unwrap().state = "running".into();
    run.state = "cancelling".into();
    m.store.save(&mut run, "stop", "Stopping").unwrap();
    let large = json!({"text":"x".repeat(state::MAX_PROGRESS_BYTES)});
    m.complete(&run.id, "step", Ok(large)).await.unwrap();
    let saved = m.store.get(&run.id).unwrap();
    assert_eq!(saved.state, "cancelled");
    assert!(state::context_within_budget(&saved.progress));
}

#[tokio::test]
async fn an_expired_step_names_its_time_limit() {
    let (_temp, m) = coordinator_fixture();
    let mut run = m
        .store
        .create(
            "slow",
            chain(vec![delay("step")]),
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    let Advance::Ready(ready) = state::advance(&run.chain, &mut run.progress, &run.inputs) else {
        panic!()
    };
    done(&mut run.progress, &ready[0], json!({}));
    run.progress.receipts.get_mut("step").unwrap().state = "running".into();
    run.state = "running".into();
    m.store.save(&mut run, "fixture", "Running").unwrap();
    let (_parent, parent) = watch::channel(false);
    // The action sees only that it was stopped, as every action body does.
    let result =
        scheduler::time_limited(Duration::from_millis(20), parent, |mut stop| async move {
            stop.changed().await.unwrap();
            Err("Task stopped; Workspace turn interrupted".to_string())
        })
        .await;
    m.complete(&run.id, "step", result).await.unwrap();
    let saved = m.store.get(&run.id).unwrap();
    let error = saved.progress.receipts["step"].error.as_deref().unwrap();
    assert_eq!(error, "This step reached its time limit");
    assert_eq!(saved.reason.as_deref(), Some(error));
    assert_eq!(saved.state, "attention");
}

#[tokio::test]
async fn pressing_stop_is_still_reported_as_a_stop() {
    let (parent, receiver) = watch::channel(false);
    let result =
        scheduler::time_limited(Duration::from_secs(60), receiver, |mut stop| async move {
            parent.send(true).unwrap();
            stop.changed().await.unwrap();
            Err("Task review stopped".to_string())
        })
        .await;
    assert_eq!(result.unwrap_err(), "Task review stopped");
}

#[tokio::test]
async fn a_result_that_arrives_after_the_limit_is_kept() {
    let (_parent, parent) = watch::channel(false);
    let result = scheduler::time_limited(Duration::from_millis(5), parent, |mut stop| async move {
        stop.changed().await.unwrap();
        Ok(json!({"answer":"Finished as it was stopped"}))
    })
    .await;
    assert_eq!(result.unwrap()["answer"], "Finished as it was stopped");
    let (_parent, parent) = watch::channel(false);
    let busy = scheduler::time_limited(Duration::from_millis(5), parent, |mut stop| async move {
        stop.changed().await.unwrap();
        Err("task_busy: Workspace is running another turn".to_string())
    })
    .await;
    assert!(busy.unwrap_err().starts_with("task_busy:"));
}

fn pinned_review(m: &Coordinator, run: &TaskRun, reviews: &std::path::Path, id: &str) {
    let operation = &run.progress.receipts["step"].operation;
    adapters::atomic_json(
        &m.store
            .root
            .join("actions")
            .join(operation)
            .join("review-run.json"),
        &json!({"runId":id}),
    )
    .unwrap();
    let dir = reviews.join(id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".task-pin"), operation).unwrap();
    std::fs::write(
        dir.join("task-operation.json"),
        json!({"operation":operation}).to_string(),
    )
    .unwrap();
}

#[tokio::test]
async fn a_pin_left_by_a_crash_after_a_completed_review_is_released_on_restart() {
    let (_temp, m) = coordinator_fixture();
    let reviews = tempfile::tempdir().unwrap();
    let release = |id: &str, operation: &str| {
        crate::commands::orchestration::release_in(&reviews.path().join(id), "done", operation)
    };
    let adopted = finished_run(&m, "adopted", false, 0, &"a".repeat(64));
    pinned_review(&m, &adopted, reviews.path(), "abc_1");
    let mut uncertain = finished_run(&m, "uncertain", false, 0, &"b".repeat(64));
    pinned_review(&m, &uncertain, reviews.path(), "abc_2");
    uncertain.state = "attention".into();
    uncertain.progress.receipts.get_mut("step").unwrap().state = "unknown".into();
    m.store.save(&mut uncertain, "fixture", "Lost").unwrap();
    // A Review this run took from and later deleted can't stop the others.
    let deleted = finished_run(&m, "deleted", false, 0, &"c".repeat(64));
    let operation = deleted.progress.receipts["step"].operation.clone();
    adapters::atomic_json(
        &m.store
            .root
            .join("actions")
            .join(operation)
            .join("review-run.json"),
        &json!({"runId":"abc_3"}),
    )
    .unwrap();

    let outcome = adapters::release_adopted(&m.store, |id, operation| {
        if id == "abc_3" {
            return Err("Run manifest is missing".into());
        }
        release(id, operation)
    });
    assert!(outcome.unwrap_err().contains("manifest"));
    assert!(!reviews.path().join("abc_1/.task-pin").exists());
    assert!(reviews.path().join("abc_2/.task-pin").exists());
}

#[tokio::test]
async fn retention_keeps_a_run_whose_review_is_still_pinned() {
    const DAY: i64 = 86_400;
    let (_temp, m) = coordinator_fixture();
    let reviews = tempfile::tempdir().unwrap();
    let run = finished_run(&m, "pinned", true, 40 * DAY, &"a".repeat(64));
    pinned_review(&m, &run, reviews.path(), "abc_1");
    let journal = m
        .store
        .root
        .join("actions")
        .join(&run.progress.receipts["step"].operation);
    let held = |id: &str| -> Result<bool> {
        reviews
            .path()
            .join(id)
            .join(".task-pin")
            .try_exists()
            .map_err(err)
    };
    let removed = m
        .store
        .remove_unpinned_runs(std::slice::from_ref(&run), held)
        .unwrap();
    assert_eq!(removed, 0);
    assert!(m.store.get(&run.id).is_ok());
    assert!(journal.join("review-run.json").is_file());
    // The next pass, once the pin has gone.
    std::fs::remove_file(reviews.path().join("abc_1/.task-pin")).unwrap();
    let removed = m
        .store
        .remove_unpinned_runs(std::slice::from_ref(&run), held)
        .unwrap();
    assert_eq!(removed, 1);
    assert!(m.store.get(&run.id).is_err());
    assert!(!journal.exists());
}

/// A real conversation with a delivery saved and its receipt left uncertain.
async fn crashed_delivery(
    w: &crate::workbench::store::Store,
    m: &Coordinator,
    name: &str,
) -> TaskRun {
    let operation = store::id();
    let session = crate::workbench::discovery::source(w, &operation, "Return", false).unwrap();
    let plan = chain(vec![delay("step")]);
    let scope = adapters::prepare(&plan, Some(session.clone()), &json!({}))
        .await
        .unwrap();
    let mut run = m
        .store
        .create(name, plan, scope, json!({}), now(), true)
        .unwrap();
    let Advance::Ready(ready) = state::advance(&run.chain, &mut run.progress, &run.inputs) else {
        panic!()
    };
    done(&mut run.progress, &ready[0], json!({}));
    let receipt = run.progress.receipts.get_mut("step").unwrap();
    receipt.state = "unknown".into();
    crate::workbench::tasks::deliver(
        w,
        &session,
        &receipt.operation,
        json!({"report":"Delivered"}),
    )
    .unwrap();
    run
}

#[tokio::test]
async fn a_recovered_delivery_leaves_the_next_step_able_to_run() {
    let _turn = exclusive_turn_state().await;
    let w = disposable_workspace();
    let (_temp, m) = coordinator_fixture();
    let mut run = crashed_delivery(&w, &m, "delivery").await;
    let stale = adapters::task_binding(&run.scope).unwrap();
    assert!(crate::workbench::tasks::validate_binding(&w, &stale).is_err());
    m.recover_results(&mut run).await.unwrap();
    assert_eq!(run.progress.receipts["step"].state, "completed");
    let current = adapters::task_binding(&run.scope).unwrap();
    crate::workbench::tasks::validate_binding(&w, &current).unwrap();
}

#[tokio::test]
async fn a_recovered_delivery_does_not_accept_a_permission_change_made_meanwhile() {
    let _turn = exclusive_turn_state().await;
    let w = disposable_workspace();
    let (_temp, m) = coordinator_fixture();
    let mut run = crashed_delivery(&w, &m, "changed").await;
    let session = run.scope.session_id.clone().unwrap();
    let snapshot = w.session_snapshot(&session).unwrap().session;
    let mut overrides = snapshot.overrides;
    overrides["commandNetwork"] = json!(true);
    w.update_session(crate::workbench::store::UpdateSessionRequest {
        session_id: session,
        expected_revision: snapshot.revision,
        operation_id: format!("network-{}", run.id),
        title: None,
        draft: None,
        overrides: Some(overrides),
        archived: None,
        preset_id: None,
        paper_id: None,
        clear_paper: None,
    })
    .unwrap();
    m.recover_results(&mut run).await.unwrap();
    assert_eq!(run.progress.receipts["step"].state, "completed");
    let binding = adapters::task_binding(&run.scope).unwrap();
    assert!(crate::workbench::tasks::validate_binding(&w, &binding).is_err());
}
