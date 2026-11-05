//! Once-only input signals and receipt updates.
use super::{err, now, Result, Store, TaskRun};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::Value;

impl Store {
    pub fn signal(
        &self,
        id: &str,
        address: &str,
        operation: &str,
        value: Value,
    ) -> Result<TaskRun> {
        if value.to_string().len() > 65536 || operation.is_empty() || operation.len() > 128 {
            return Err("Invalid task input".into());
        }
        let mut c = self.connection()?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        let body: String = tx
            .query_row("SELECT body FROM runs WHERE id=?1", [id], |r| r.get(0))
            .map_err(err)?;
        let mut run: TaskRun = serde_json::from_str(&body).map_err(err)?;
        let previous: Option<(String, String, String)> = tx
            .query_row(
                "SELECT run_id,address,body FROM signals WHERE id=?1",
                [operation],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(err)?;
        if let Some((old_id, old_address, old_value)) = previous {
            if old_id != id
                || old_address != address
                || serde_json::from_str::<Value>(&old_value).map_err(err)? != value
            {
                return Err("Input operation already used".into());
            }
            return Ok(run);
        }
        // Past its deadline a run sits in attention with the wait still open.
        // The researcher's answer is the decision to continue, so it starts a
        // new deadline rather than sending the run straight back.
        let late = run.state == "attention"
            && run.deadline_at <= now()
            && !run
                .progress
                .receipts
                .values()
                .any(|r| matches!(r.state.as_str(), "unknown" | "failed"));
        if !late && !["waiting", "paused", "running", "queued"].contains(&run.state.as_str()) {
            return Err("Task is not waiting for input".into());
        }
        let receipt = run
            .progress
            .receipts
            .get_mut(address)
            .ok_or("Input wait was not found")?;
        if receipt.state != "input" {
            return Err("This input has already been resolved".into());
        }
        receipt.state = "completed".into();
        receipt.finished_at = Some(now());
        receipt.output = Some(value.clone());
        run.progress.outputs[&receipt.step_id] = value.clone();
        if !crate::orchestration::state::context_within_budget(&run.progress) {
            return Err("This input would exceed the automation's context budget".into());
        }
        if late {
            run.deadline_at = now() + run.chain.limits.deadline_hours as i64 * 3600;
        }
        if run.state != "paused" {
            run.state = "queued".into();
        }
        run.due_at = Some(now());
        run.reason = None;
        run.revision += 1;
        run.updated_at = now();
        tx.execute(
            "INSERT INTO signals(id,run_id,address,body,consumed) VALUES(?1,?2,?3,?4,1)",
            params![operation, id, address, value.to_string()],
        )
        .map_err(err)?;
        tx.execute(
            "UPDATE runs SET revision=?2,state=?3,due_at=?4,updated_at=?4,body=?5 WHERE id=?1",
            params![
                id,
                run.revision,
                run.state,
                now(),
                serde_json::to_string(&run).map_err(err)?
            ],
        )
        .map_err(err)?;
        tx.execute(
            "INSERT INTO events(run_id,at,kind,detail) VALUES(?1,?2,'input','Input received')",
            params![id, now()],
        )
        .map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(run)
    }
}
