//! Research task records, isolated copies, and the application journal.

mod application;
mod copies;
mod records;

pub(super) use application::{accept_changes, apply, recover, undo};
#[cfg(all(test, unix))]
pub(super) use copies::git;
pub(super) use copies::{
    capture_changes, checkpoint, discard_copy, git_fetch, reclaim_replaced_copy, reject,
};
pub use copies::{session_task_root, task_session};
pub use records::{Application, AppliedFile, Checkpoint, ResearchTask};

use super::{
    bounded, decode, id, put, record, ProjectRecord, Store, WorkbenchError, WorkbenchResult,
};

pub(super) fn create_task(
    store: &Store,
    ws: &str,
    objective: String,
    anchor_id: Option<String>,
    expected_outputs: Vec<String>,
    expected_checks: Vec<String>,
) -> WorkbenchResult<ProjectRecord> {
    bounded(&objective, 16000)?;
    if expected_outputs.len() > 100 || expected_checks.len() > 100 {
        return Err(WorkbenchError::invalid(
            "At most 100 outputs/checks per task",
        ));
    }
    for value in expected_outputs.iter().chain(expected_checks.iter()) {
        bounded(value, 1000)?;
    }
    if let Some(anchor) = &anchor_id {
        record(store, ws, anchor, "anchor")?;
    }
    put(
        store,
        ws,
        &id("task")?,
        "task",
        0,
        &ResearchTask {
            objective,
            anchor_id,
            expected_outputs,
            expected_checks,
            status: "open".into(),
        },
    )
}
pub(super) fn update_task(
    store: &Store,
    ws: &str,
    task_id: &str,
    expected: i64,
    status: &str,
) -> WorkbenchResult<ProjectRecord> {
    if !["open", "investigating", "deferred", "completed", "rejected"].contains(&status) {
        return Err(WorkbenchError::invalid("Invalid task disposition"));
    }
    let r = record(store, ws, task_id, "task")?;
    let mut task: ResearchTask = decode(&r)?;
    task.status = status.into();
    put(store, ws, task_id, "task", expected, &task)
}
