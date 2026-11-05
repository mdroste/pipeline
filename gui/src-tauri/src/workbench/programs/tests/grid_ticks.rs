//! The background pass over experiment grids. One grid's failure stays on that
//! grid: the others advance, and the researcher reads the error on the grid. A busy
//! store is no failure at all: the grid tries again on the next pass.
use super::*;

/// A running grid whose active attempt has no execution record.
fn broken(f: &Fixture, base: &DeskRecord, title: &str) -> DeskRecord {
    let mut request = grid_request(f, base);
    request.title = title.into();
    request.operation_id = title.into();
    let grid = experiments::prepare(&f.store, request).unwrap();
    f.store.connection().unwrap().execute("UPDATE experiment_runs SET state='running',active_execution_id='execution_gone',process_instance=?2,updated_at='2020-01-01' WHERE plan_id=?1",params![grid.id,research::jobs::process_instance()]).unwrap();
    grid
}
fn run(f: &Fixture, grid: &DeskRecord) -> Value {
    experiments::status(&f.store, &f.ws, &grid.id).unwrap()["run"].clone()
}

#[test]
fn a_failing_grid_shows_its_error_and_the_next_grid_still_advances() {
    let f = fixture(true);
    let p = profile(&f, "print('fixture')", 10);
    let base = capture(&f, &p);
    let failing = broken(&f, &base, "failing");
    let mut request = grid_request(&f, &base);
    request.title = "healthy".into();
    request.operation_id = "healthy".into();
    let healthy = experiments::prepare(&f.store, request).unwrap();
    // A grid left running by an earlier launch pauses on its next pass.
    f.store.connection().unwrap().execute("UPDATE experiment_runs SET state='running',process_instance='earlier-launch',updated_at='2021-01-01' WHERE plan_id=?1",[&healthy.id]).unwrap();

    experiments::tick_due(&f.store).unwrap();

    let stopped = run(&f, &failing);
    assert_eq!(stopped["state"], "attention");
    let reason = stopped["reason"].as_str().unwrap();
    assert!(reason.starts_with("This grid stopped: "), "{reason}");
    let advanced = run(&f, &healthy);
    assert_eq!(advanced["state"], "paused");
    assert!(advanced["reason"]
        .as_str()
        .unwrap()
        .contains("Pipeline restarted"));
}

#[test]
fn a_stopped_grid_leaves_the_pass_and_can_still_be_cancelled() {
    let f = fixture(true);
    let p = profile(&f, "print('fixture')", 10);
    let base = capture(&f, &p);
    let grid = broken(&f, &base, "failing");
    experiments::tick_due(&f.store).unwrap();
    let revision = run(&f, &grid)["revision"].clone();
    experiments::tick_due(&f.store).unwrap();
    assert_eq!(run(&f, &grid)["revision"], revision);

    let cancelled =
        experiments::control(&f.store, &f.ws, &grid.id, "cancel", &grid.content_hash).unwrap();
    assert_eq!(cancelled["run"]["state"], "cancelled");

    // Cancelled with its attempt still gone: it stays cancelled and stops repeating.
    experiments::tick_due(&f.store).unwrap();
    let ended = run(&f, &grid);
    assert_eq!(ended["state"], "cancelled");
    assert!(ended["activeExecutionId"].is_null());
    assert!(ended["reason"].as_str().unwrap().contains("stopped"));
}

/// A running grid that pauses the moment it next gets to write.
fn restarted(f: &Fixture, base: &DeskRecord, title: &str, updated: &str) -> DeskRecord {
    let mut request = grid_request(f, base);
    request.title = title.into();
    request.operation_id = title.into();
    let grid = experiments::prepare(&f.store, request).unwrap();
    f.store.connection().unwrap().execute("UPDATE experiment_runs SET state='running',process_instance='earlier-launch',updated_at=?2 WHERE plan_id=?1",params![grid.id,updated]).unwrap();
    grid
}
/// Another writer holds the store, as a long save does, until this is dropped.
fn hold_store(f: &Fixture) -> rusqlite::Connection {
    let c = f.store.connection().unwrap();
    c.execute_batch("BEGIN IMMEDIATE").unwrap();
    c
}
/// A cancelled grid whose attempt ended without the grid hearing of it.
fn cancelled_with_ended_attempt(f: &Fixture, base: &DeskRecord) -> DeskRecord {
    let grid = experiments::prepare(&f.store, grid_request(f, base)).unwrap();
    let c = f.store.connection().unwrap();
    c.execute("INSERT INTO research_executions(id,workspace_id,adapter,command_json,cwd,input_manifest_json,dependency_hash,outcome,output_manifest_json,validation_json,snapshot_consistency,created_at) VALUES('execution_ended',?1,'command','[]',?2,'{}','fixture','interrupted','{}','{}','uncertain','2026-09-07')",params![f.ws,f.root.to_string_lossy()]).unwrap();
    c.execute("INSERT INTO experiment_attempts(plan_id,ordinal,operation_id,execution_id,state) VALUES(?1,0,'attempt','execution_ended','queued')",[&grid.id]).unwrap();
    c.execute("UPDATE experiment_runs SET state='cancelled',active_execution_id='execution_ended' WHERE plan_id=?1",[&grid.id]).unwrap();
    grid
}

#[test]
fn a_busy_store_is_tried_again_and_the_next_grid_still_advances() {
    let f = fixture(true);
    let p = profile(&f, "print('fixture')", 10);
    let base = capture(&f, &p);
    let first = restarted(&f, &base, "first", "2020-01-01");
    let second = restarted(&f, &base, "second", "2021-01-01");
    let due = vec![
        (f.ws.clone(), first.id.clone()),
        (f.ws.clone(), second.id.clone()),
    ];
    experiments::tick_each(&f.store, due, |ws, id| {
        if id != first.id {
            return experiments::tick(&f.store, ws, id);
        }
        let held = hold_store(&f);
        let result = experiments::tick(&f.store, ws, id);
        drop(held);
        let error = result.as_ref().unwrap_err();
        assert!(error.message.contains("locked"), "{error:?}");
        result
    });

    let waiting = run(&f, &first);
    assert_eq!(waiting["state"], "running");
    assert!(waiting["reason"].is_null());
    assert_eq!(run(&f, &second)["state"], "paused");

    experiments::tick_due(&f.store).unwrap();
    assert_eq!(run(&f, &first)["state"], "paused");
}

#[test]
fn a_cancelled_grid_waits_out_a_busy_store_and_keeps_its_receipt() {
    let f = fixture(true);
    let p = profile(&f, "print('fixture')", 10);
    let base = capture(&f, &p);
    let grid = cancelled_with_ended_attempt(&f, &base);

    // The store frees up as soon as the tick gives up, so a wrong response could
    // still be written.
    experiments::tick_each(&f.store, vec![(f.ws.clone(), grid.id.clone())], |ws, id| {
        let held = hold_store(&f);
        let result = experiments::tick(&f.store, ws, id);
        drop(held);
        result
    });
    let waiting = run(&f, &grid);
    assert_eq!(waiting["activeExecutionId"], "execution_ended");
    assert!(waiting["reason"].is_null());

    experiments::tick_due(&f.store).unwrap();
    let status = experiments::status(&f.store, &f.ws, &grid.id).unwrap();
    assert_eq!(status["run"]["state"], "cancelled");
    assert!(status["run"]["activeExecutionId"].is_null());
    assert_eq!(status["attempts"][0]["state"], "interrupted");
    assert_eq!(status["attempts"][0]["receipt"]["id"], "execution_ended");
}

#[test]
fn an_unreadable_plan_stops_a_running_grid_and_says_so() {
    let f = fixture(true);
    let p = profile(&f, "print('fixture')", 10);
    let base = capture(&f, &p);
    let grid = restarted(&f, &base, "unreadable", "2020-01-01");
    let c = f.store.connection().unwrap();
    c.execute(
        "UPDATE desk_records SET body_json='{}' WHERE id=?1",
        [&grid.id],
    )
    .unwrap();
    experiments::tick_due(&f.store).unwrap();
    let (state, reason): (String, String) = c
        .query_row(
            "SELECT state,reason FROM experiment_runs WHERE plan_id=?1",
            [&grid.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, "attention");
    assert!(reason.contains("can't be read"), "{reason}");
}

#[test]
fn a_cancelled_grid_with_an_unreadable_plan_still_keeps_its_receipt() {
    let f = fixture(true);
    let p = profile(&f, "print('fixture')", 10);
    let base = capture(&f, &p);
    let grid = cancelled_with_ended_attempt(&f, &base);
    let c = f.store.connection().unwrap();
    c.execute(
        "UPDATE desk_records SET body_json='{}' WHERE id=?1",
        [&grid.id],
    )
    .unwrap();

    experiments::tick_due(&f.store).unwrap();

    let (state, active, reason): (String, Option<String>, String) = c
        .query_row(
            "SELECT state,active_execution_id,reason FROM experiment_runs WHERE plan_id=?1",
            [&grid.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(state, "cancelled");
    assert!(active.is_none());
    assert!(reason.contains("can't be read"), "{reason}");
    let (attempt, receipt): (String, String) = c
        .query_row(
            "SELECT state,result_json FROM experiment_attempts WHERE plan_id=?1",
            [&grid.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(attempt, "interrupted");
    assert!(receipt.contains("execution_ended"), "{receipt}");
}

#[test]
fn a_brief_project_lock_leaves_the_grid_running_for_the_next_pass() {
    let f = fixture(true);
    let p = profile(&f, "print('fixture')", 10);
    let base = capture(&f, &p);
    let grid = experiments::prepare(&f.store, grid_request(&f, &base)).unwrap();
    f.store
        .connection()
        .unwrap()
        .execute(
            "UPDATE experiment_runs SET state='running',process_instance=?2 WHERE plan_id=?1",
            params![grid.id, research::jobs::process_instance()],
        )
        .unwrap();
    let busy = project::execution_lock(&f.store, &f.ws).unwrap();

    experiments::tick(&f.store, &f.ws, &grid.id).unwrap();

    let waiting = run(&f, &grid);
    assert_eq!(waiting["state"], "running");
    assert!(waiting["reason"].is_null());
    assert!(waiting["activeExecutionId"].is_null());
    drop(busy);
}

#[test]
fn a_refused_job_that_will_not_clear_up_still_stops_the_grid() {
    let f = fixture(true);
    let p = profile(&f, "print('fixture')", 10);
    let base = capture(&f, &p);
    let grid = experiments::prepare(&f.store, grid_request(&f, &base)).unwrap();
    f.store
        .connection()
        .unwrap()
        .execute(
            "UPDATE experiment_runs SET state='running',process_instance=?2 WHERE plan_id=?1",
            params![grid.id, research::jobs::process_instance()],
        )
        .unwrap();
    fs::remove_file(f.root.join("analysis.py")).unwrap();

    experiments::tick(&f.store, &f.ws, &grid.id).unwrap();

    assert_eq!(run(&f, &grid)["state"], "attention");
}

#[test]
fn what_makes_an_error_transient_stays_out_of_what_the_app_sees() {
    let error = WorkbenchError::conflict("Busy").transient();
    assert_eq!(error.code, "storage_conflict");
    assert!(error.retryable);
    let sent = serde_json::to_value(&error).unwrap();
    assert!(sent.get("transient").is_none(), "{sent}");
    let received: WorkbenchError = serde_json::from_value(sent).unwrap();
    assert!(!received.transient);
}

/// The lock another open Pipeline would hold on its own file.
fn alive_elsewhere(f: &Fixture, instance: &str) -> fs::File {
    use fs2::FileExt as _;
    let path = research::jobs::instance_lock_path(&f.store, instance).unwrap();
    let file = fs::File::create(path).unwrap();
    file.try_lock_exclusive().unwrap();
    file
}

#[test]
fn a_grid_running_in_another_open_pipeline_is_left_to_it() {
    let f = fixture(true);
    let p = profile(&f, "print('fixture')", 10);
    let base = capture(&f, &p);
    let grid = experiments::prepare(&f.store, grid_request(&f, &base)).unwrap();
    let held = alive_elsewhere(&f, "instance_other");
    f.store
        .connection()
        .unwrap()
        .execute(
            "UPDATE experiment_runs SET state='running',process_instance='instance_other' WHERE plan_id=?1",
            [&grid.id],
        )
        .unwrap();

    experiments::tick(&f.store, &f.ws, &grid.id).unwrap();
    assert_eq!(run(&f, &grid)["state"], "running");

    drop(held);
    experiments::tick(&f.store, &f.ws, &grid.id).unwrap();
    assert_eq!(run(&f, &grid)["state"], "paused");
}

#[test]
fn an_attempt_alive_in_another_open_pipeline_needs_no_attention() {
    let f = fixture(true);
    let p = profile(&f, "print('fixture')", 10);
    let base = capture(&f, &p);
    let grid = experiments::prepare(&f.store, grid_request(&f, &base)).unwrap();
    numeric(&f, "elsewhere", 1.0, "sample");
    let held = alive_elsewhere(&f, "instance_other");
    let c = f.store.connection().unwrap();
    c.execute("INSERT INTO execution_jobs(execution_id,operation_id,request_hash,owner,process_instance,created_at) VALUES('execution_elsewhere','elsewhere','hash','detached','instance_other','now')",[]).unwrap();
    c.execute(
        "UPDATE research_executions SET outcome='running' WHERE id='execution_elsewhere'",
        [],
    )
    .unwrap();
    c.execute(
        "UPDATE experiment_runs SET state='paused',active_execution_id='execution_elsewhere' WHERE plan_id=?1",
        [&grid.id],
    )
    .unwrap();

    experiments::tick(&f.store, &f.ws, &grid.id).unwrap();
    assert_eq!(run(&f, &grid)["state"], "paused");

    drop(held);
    experiments::tick(&f.store, &f.ws, &grid.id).unwrap();
    assert_eq!(run(&f, &grid)["state"], "attention");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_still_running_here_is_not_called_unknown() {
    let f = fixture(true);
    let p = profile(&f, "print('fixture')", 10);
    let base = capture(&f, &p);
    let grid = experiments::prepare(&f.store, grid_request(&f, &base)).unwrap();
    experiments::control(&f.store, &f.ws, &grid.id, "start", &grid.content_hash).unwrap();
    experiments::tick(&f.store, &f.ws, &grid.id).unwrap();
    let active = run(&f, &grid)["activeExecutionId"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(research::jobs::execution_active(&active));
    f.store
        .connection()
        .unwrap()
        .execute(
            "UPDATE research_executions SET outcome='outcome_unknown' WHERE id=?1",
            [&active],
        )
        .unwrap();

    experiments::tick(&f.store, &f.ws, &grid.id).unwrap();
    assert_eq!(run(&f, &grid)["state"], "running");

    research::jobs::launch_pending();
    for _ in 0..500 {
        if !research::jobs::execution_active(&active) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(!research::jobs::execution_active(&active));
}
