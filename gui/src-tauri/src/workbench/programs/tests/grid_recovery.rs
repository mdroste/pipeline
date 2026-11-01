//! A forced quit can leave a grid's attempt with an unknown outcome. The grid can
//! always be ended, or moved past that attempt by an explicit decision. A follow-up
//! left the same way can be dismissed.
use super::*;

fn stranded(f: &Fixture, outcome: &str) -> DeskRecord {
    let p = profile(f, "print('fixture')", 10);
    let base = capture(f, &p);
    let grid = experiments::prepare(&f.store, grid_request(f, &base)).unwrap();
    let c = f.store.connection().unwrap();
    c.execute("INSERT INTO research_executions(id,workspace_id,adapter,command_json,cwd,input_manifest_json,dependency_hash,outcome,output_manifest_json,validation_json,snapshot_consistency,created_at) VALUES('execution_lost',?1,'command','[]',?2,'{}','fixture',?3,'{}','{}','uncertain','2026-09-07')",params![f.ws,f.root.to_string_lossy(),outcome]).unwrap();
    c.execute("INSERT INTO experiment_attempts(plan_id,ordinal,operation_id,execution_id,state) VALUES(?1,0,'attempt','execution_lost','queued')",[&grid.id]).unwrap();
    c.execute("UPDATE experiment_runs SET state='attention',active_execution_id='execution_lost',reason='The active attempt has an unknown outcome' WHERE plan_id=?1",[&grid.id]).unwrap();
    grid
}
fn control(f: &Fixture, grid: &DeskRecord, action: &str) -> WorkbenchResult<Value> {
    experiments::control(&f.store, &f.ws, &grid.id, action, &grid.content_hash)
}

#[test]
fn a_grid_stuck_on_an_unknown_attempt_can_be_cancelled_and_stays_cancelled() {
    let f = fixture(true);
    let grid = stranded(&f, "outcome_unknown");
    let cancelled = control(&f, &grid, "cancel").unwrap();
    assert_eq!(cancelled["run"]["state"], "cancelled");
    for _ in 0..3 {
        experiments::tick(&f.store, &f.ws, &grid.id).unwrap();
    }
    let status = experiments::status(&f.store, &f.ws, &grid.id).unwrap();
    assert_eq!(status["run"]["state"], "cancelled");
    assert!(status["run"]["activeExecutionId"].is_null());
    assert_eq!(status["attempts"][0]["state"], "outcome_unknown");
}

#[test]
fn reconcile_points_to_the_decision_and_continuing_keeps_the_attempt_unknown() {
    let f = fixture(true);
    let grid = stranded(&f, "outcome_unknown");
    let refused = control(&f, &grid, "start").unwrap_err();
    assert!(refused.message.contains("reconcile"), "{refused:?}");
    let refused = control(&f, &grid, "reconcile").unwrap_err();
    assert!(
        refused.message.contains("continue without this run"),
        "{refused:?}"
    );

    let paused = control(&f, &grid, "continue_unknown").unwrap();
    assert_eq!(paused["run"]["state"], "paused");
    assert_eq!(paused["run"]["nextIndex"], 1);
    assert!(paused["run"]["activeExecutionId"].is_null());
    assert_eq!(
        paused["run"]["reason"],
        "Specification 1 was set aside with an unknown outcome."
    );
    assert_eq!(paused["attempts"][0]["state"], "outcome_unknown");
    assert_eq!(paused["attempts"][0]["executionId"], "execution_lost");
    assert_eq!(
        control(&f, &grid, "start").unwrap()["run"]["state"],
        "running"
    );
}

#[test]
fn only_an_unknown_attempt_can_be_set_aside() {
    let f = fixture(true);
    let grid = stranded(&f, "completed");
    assert!(control(&f, &grid, "continue_unknown").is_err());
    let status = experiments::status(&f.store, &f.ws, &grid.id).unwrap();
    assert_eq!(status["run"]["state"], "attention");
    assert_eq!(status["run"]["nextIndex"], 0);
}

/// Twenty follow-ups whose submissions can't be traced, as after a restore.
fn stranded_followups(f: &Fixture) -> Vec<Value> {
    for n in 0..20 {
        let rows = followups::enqueue(
            &f.store,
            &f.session,
            &format!("Request {n}"),
            None,
            None,
            &format!("queue-{n}"),
        )
        .unwrap();
        let row = rows.as_array().unwrap().last().unwrap();
        let (id, fingerprint) = (
            row["id"].as_str().unwrap(),
            row["fingerprint"].as_str().unwrap(),
        );
        followups::prepare_dispatch(&f.store, &f.session, id, fingerprint).unwrap();
        followups::reconcile(&f.store, &f.session, id, None).unwrap();
    }
    followups::list(&f.store, &f.session)
        .unwrap()
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn dismissing_a_stranded_followup_frees_the_queue_without_retrying_it() {
    let f = fixture(false);
    let rows = stranded_followups(&f);
    assert!(rows.iter().all(|r| r["state"] == "attention"));
    assert!(followups::enqueue(&f.store, &f.session, "One more", None, None, "more").is_err());
    let (id, revision) = (
        rows[0]["id"].as_str().unwrap(),
        rows[0]["revision"].as_i64(),
    );
    let rows = followups::control(&f.store, &f.session, id, revision.unwrap(), "dismiss").unwrap();
    let row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap();
    assert_eq!(row["state"], "cancelled");
    assert!(row["result"]["notice"]
        .as_str()
        .unwrap()
        .contains("not retried"));
    assert!(row["result"].get("transport").is_some());
    followups::enqueue(&f.store, &f.session, "One more", None, None, "more").unwrap();
    assert!(followups::prepare_dispatch(&f.store, &f.session, id, "any").is_err());
    assert!(f
        .store
        .conversation_snapshot(&f.session)
        .unwrap()
        .turns
        .is_empty());
}

#[test]
fn only_a_followup_in_attention_can_be_dismissed() {
    let f = fixture(false);
    let rows = followups::enqueue(&f.store, &f.session, "Waiting", None, None, "a").unwrap();
    let error = followups::control(
        &f.store,
        &f.session,
        rows[0]["id"].as_str().unwrap(),
        rows[0]["revision"].as_i64().unwrap(),
        "dismiss",
    )
    .unwrap_err();
    assert!(error.message.contains("attention"), "{error:?}");
}
