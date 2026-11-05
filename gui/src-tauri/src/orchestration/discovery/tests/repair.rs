//! What happens to a run when a reply can't be taken up.
use super::*;
use crate::agent_runtime::events::NormalizedEvent;

/// Finishes the active child with this reply, as the coordinator would.
fn reply(c: &Coordinator, p: &Portfolio, text: &str) -> TaskRun {
    let mut child = c.store.get(p.run.active_child.as_ref().unwrap()).unwrap();
    let output = json!({"state":"completed","finalText":text,"cursor":child.scope.session_cursor});
    let receipt = child
        .progress
        .receipts
        .entry("action".into())
        .or_insert_with(|| crate::orchestration::state::Receipt {
            sequence: 1,
            address: "action".into(),
            step_id: "action".into(),
            label: "Investigate".into(),
            state: "completed".into(),
            operation: store::id(),
            started_at: now(),
            finished_at: None,
            wake_at: None,
            output: None,
            error: None,
            child: None,
        });
    receipt.state = "completed".into();
    receipt.finished_at = Some(now());
    child.progress.outputs = json!({"action":output});
    child.state = "finished".into();
    c.store.save(&mut child, "finished", "Finished").unwrap();
    child
}

/// The author's first turn ends, and the paper's scope moves with it.
fn first_turn_ended(c: &Coordinator, w: &crate::workbench::store::Store, child: &mut TaskRun) {
    let session = child.scope.session_id.clone().unwrap();
    let binding = w.active_binding(&session).unwrap().unwrap();
    w.project_codex_event(
        &binding.id,
        &NormalizedEvent::TurnCompleted {
            epoch: 1,
            thread_id: binding.provider_thread_id.clone(),
            turn_id: "turn-1".into(),
            status: "completed".into(),
            turn: json!({"id":"turn-1","status":"completed"}),
        },
    )
    .unwrap();
    child.scope.session_cursor = Some(crate::workbench::tasks::cursor(w, &session).unwrap());
    c.store.save(child, "fixture", "Ended").unwrap();
}

#[tokio::test]
async fn replies_that_keep_failing_wait_for_the_researcher_and_retry_tells_the_model_why() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let temp = tempfile::tempdir().unwrap();
    let c = coordinator(&temp);
    let (w, mut p, mut child, _) = author_turn(&c, crate::workbench::api::NAMESPACE);
    first_turn_ended(&c, &w, &mut child);
    for repairs in 1..=2 {
        reply(&c, &p, "Not the contract");
        advance(&c, &mut p).await.unwrap();
        assert_eq!(p.run.state, "running", "{}", p.run.reason);
        assert_eq!(p.run.repairs, repairs);
    }
    let dispatched = p.run.actions_reserved;
    reply(&c, &p, "Not the contract");
    advance(&c, &mut p).await.unwrap();
    assert_eq!(p.run.state, "attention");
    assert!(p
        .run
        .reason
        .starts_with("Research response could not be adopted"));
    assert!(p.run.active_child.is_none());
    assert!(!p.run.terminal());
    let session = p.papers[0].scope.as_ref().unwrap().session_id.clone();
    assert!(workspace::role_policy(&w, session.as_deref().unwrap())
        .unwrap()
        .is_some());

    assert_eq!(commands::retry(&c, &mut p).await.unwrap(), None);
    save(&c, &mut p, "retry", None).await.unwrap();
    assert_eq!(p.run.state, "running");
    assert_eq!(p.run.repairs, 1);
    let prompt = prompts::prompt(&p).unwrap();
    assert!(prompt.contains("Repair the response contract"), "{prompt}");
    advance(&c, &mut p).await.unwrap();
    assert_eq!(p.run.actions_reserved, dispatched + 1);
    assert!(p.run.active_child.is_some());
}

#[tokio::test]
async fn pausing_does_not_hide_a_reply_that_needs_the_researcher() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let temp = tempfile::tempdir().unwrap();
    let c = coordinator(&temp);
    let (_w, mut p, _child, _) = author_turn(&c, crate::workbench::api::NAMESPACE);
    p.run.repairs = 2;
    p.run.state = "paused".into();
    storage::save(&c.store, &mut p, "pause", None).unwrap();
    reply(&c, &p, "Not the contract");
    advance(&c, &mut p).await.unwrap();
    assert_eq!(p.run.state, "attention");
}

#[tokio::test]
async fn a_reply_naming_a_file_that_is_not_there_gets_a_repair() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let temp = tempfile::tempdir().unwrap();
    let c = coordinator(&temp);
    let (w, mut p, mut child, _) = author_turn(&c, crate::workbench::api::NAMESPACE);
    first_turn_ended(&c, &w, &mut child);
    let mut text = qualification::response(&p);
    text["files"] = json!(["results/missing.csv"]);
    reply(&c, &p, &text.to_string());
    advance(&c, &mut p).await.unwrap();
    assert_eq!(p.run.repairs, 1);
    assert_eq!(p.run.state, "running");
    assert!(p.papers[0].rounds.is_empty());
}

#[tokio::test]
async fn a_failure_to_save_costs_no_repair_and_retry_reads_the_same_reply_again() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let temp = tempfile::tempdir().unwrap();
    let c = coordinator(&temp);
    let (w, p, mut child, _) = author_turn(&c, crate::workbench::api::NAMESPACE);
    first_turn_ended(&c, &w, &mut child);
    let text = qualification::response(&p).to_string();
    let child = reply(&c, &p, &text);
    let reserved = p.run.actions_reserved;
    // A file where artifact storage belongs makes every save fail.
    let blobs = c.store.root.join("blobs");
    let _ = std::fs::remove_dir_all(&blobs);
    std::fs::write(&blobs, "").unwrap();

    tick_run(&c, &p.run.id).await.unwrap();
    let mut p = storage::load(&c.store, &p.run.id).unwrap();
    assert_eq!(p.run.state, "attention", "{}", p.run.reason);
    assert!(p.run.reason.contains("could not save"), "{}", p.run.reason);
    assert_eq!(p.run.repairs, 0);
    assert_eq!(p.run.active_child.as_ref(), Some(&child.id));
    assert_eq!(p.run.actions_reserved, reserved);

    std::fs::remove_file(&blobs).unwrap();
    assert_eq!(commands::retry(&c, &mut p).await.unwrap(), None);
    save(&c, &mut p, "retry", None).await.unwrap();
    tick_run(&c, &p.run.id).await.unwrap();
    let p = storage::load(&c.store, &p.run.id).unwrap();
    assert_eq!(p.papers[0].rounds.len(), 1, "{}", p.run.reason);
    assert_eq!(p.run.repairs, 0);
    assert_ne!(p.run.active_child.as_ref(), Some(&child.id));
}
