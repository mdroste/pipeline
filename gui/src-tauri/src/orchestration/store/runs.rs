//! Task creation, saved state, readiness and startup recovery.
use super::{err, hash, id, now, Result, Scope, Store, Summary, TaskRun, HEARTBEAT};
use crate::orchestration::{definition::Chain, state::Progress};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::Value;

impl Store {
    pub fn create(
        &self,
        operation: &str,
        chain: Chain,
        scope: Scope,
        inputs: Value,
        due: i64,
        approved: bool,
    ) -> Result<TaskRun> {
        if operation.is_empty()
            || operation.len() > 128
            || !inputs.is_object()
            || inputs.to_string().len() > 256 * 1024
        {
            return Err("Invalid task creation request".into());
        }
        crate::orchestration::definition::validate(&chain)?;
        let fingerprint = hash(&(&chain, &scope, &inputs, approved))?;
        let mut c = self.connection()?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT fingerprint,body FROM runs WHERE operation=?1",
                [operation],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(err)?;
        if let Some((previous, body)) = existing {
            if previous != fingerprint {
                return Err("Task operation already used with different inputs".into());
            }
            return serde_json::from_str(&body).map_err(err);
        }
        let timestamp = now();
        let run = TaskRun {
            id: id(),
            mission_id: None,
            revision: 0,
            name: chain.name.clone(),
            state: if approved { "queued" } else { "draft" }.into(),
            reason: None,
            deadline_at: due + chain.limits.deadline_hours as i64 * 3600,
            chain,
            scope,
            inputs,
            progress: Progress::default(),
            created_at: timestamp,
            updated_at: timestamp,
            due_at: Some(due),
            schedule_id: None,
            occurrence: None,
        };
        insert_run(&tx, &run, operation, &fingerprint)?;
        tx.commit().map_err(err)?;
        Ok(run)
    }

    pub fn get(&self, id: &str) -> Result<TaskRun> {
        let body: String = self
            .connection()?
            .query_row("SELECT body FROM runs WHERE id=?1", [id], |r| r.get(0))
            .map_err(err)?;
        serde_json::from_str(&body).map_err(err)
    }

    pub fn save(&self, run: &mut TaskRun, kind: &str, detail: &str) -> Result<()> {
        if !crate::orchestration::state::context_within_budget(&run.progress) {
            return Err(
                "Task context budget reached; reconcile durable action results before continuing"
                    .into(),
            );
        }
        self.write(run, kind, detail)
    }

    /// For a change that adds nothing to the run's context, such as stopping
    /// it or marking it for attention. A run over its context budget can still
    /// take these, so nothing is left unable to stop.
    pub fn write(&self, run: &mut TaskRun, kind: &str, detail: &str) -> Result<()> {
        let mut c = self.connection()?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        let expected = run.revision;
        run.revision += 1;
        run.updated_at = now();
        let changed=tx.execute("UPDATE runs SET revision=?2,state=?3,due_at=?4,updated_at=?5,body=?6,session_id=?8 WHERE id=?1 AND revision=?7",params![run.id,run.revision,run.state,run.due_at,run.updated_at,serde_json::to_string(run).map_err(err)?,expected,run.scope.session_id]).map_err(err)?;
        if changed != 1 {
            run.revision = expected;
            return Err("Task changed; reload before editing".into());
        }
        tx.execute(
            "INSERT INTO events(run_id,at,kind,detail) VALUES (?1,?2,?3,?4)",
            params![run.id, now(), kind, detail],
        )
        .map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(())
    }
    /// Refresh only the poll time of a run that is already recorded as
    /// waiting; no body rewrite, no event row.
    pub fn touch_due(&self, id: &str, due: Option<i64>) -> Result<()> {
        self.connection()?
            .execute("UPDATE runs SET due_at=?2 WHERE id=?1", params![id, due])
            .map_err(err)?;
        Ok(())
    }

    pub fn list(&self, view: &str, session: Option<&str>, offset: u32) -> Result<Vec<Summary>> {
        let c = self.connection()?;
        let predicate = match view {
            "session" => "1=1",
            "history" => "state IN ('finished','cancelled','failed')",
            "drafts" => "state='draft'",
            "ready" => "state IN ('queued','running','waiting')",
            _ => "state NOT IN ('finished','cancelled','failed')",
        };
        // Bodies are parsed only for the page, not for every candidate row.
        let sql=format!("SELECT r.id,r.revision,json_extract(r.body,'$.name'),r.state,json_extract(r.body,'$.reason'),json_extract(r.body,'$.createdAt'),r.updated_at,r.due_at,r.session_id,r.schedule_id FROM (SELECT id FROM runs WHERE {predicate} AND NOT EXISTS (SELECT 1 FROM mission_children mc WHERE mc.task_id=runs.id) AND NOT EXISTS (SELECT 1 FROM discovery_children dc WHERE dc.task_id=runs.id) AND (?1 IS NULL OR session_id=?1) ORDER BY updated_at DESC,id LIMIT 50 OFFSET ?2) page JOIN runs r ON r.id=page.id ORDER BY r.updated_at DESC,r.id");
        let mut s = c.prepare(&sql).map_err(err)?;
        let result = s
            .query_map(params![session, offset], |r| {
                Ok(Summary {
                    id: r.get(0)?,
                    revision: r.get(1)?,
                    name: r.get(2)?,
                    state: r.get(3)?,
                    reason: r.get(4)?,
                    created_at: r.get(5)?,
                    updated_at: r.get(6)?,
                    due_at: r.get(7)?,
                    session_id: r.get(8)?,
                    schedule_id: r.get(9)?,
                })
            })
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        Ok(result)
    }

    pub fn ready_ids(&self, time: i64) -> Result<Vec<String>> {
        let c = self.connection()?;
        let mut s=c.prepare("SELECT id FROM runs WHERE (state IN ('queued','running','waiting') AND due_at<=?1) OR (state IN ('queued','running','waiting') AND json_extract(body,'$.deadlineAt')<=?1) ORDER BY updated_at,id LIMIT 32").map_err(err)?;
        let result = s
            .query_map([time], |r| r.get(0))
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        Ok(result)
    }

    pub fn next_due(&self) -> Result<Option<i64>> {
        self.connection()?.query_row("SELECT MIN(t) FROM (SELECT due_at t FROM runs WHERE state IN ('queued','running','waiting') UNION ALL SELECT json_extract(body,'$.deadlineAt') t FROM runs WHERE state IN ('queued','running','waiting') UNION ALL SELECT due_at t FROM missions WHERE state IN ('queued','running','waiting','paused','stopping') UNION ALL SELECT json_extract(body,'$.deadlineAt') t FROM missions WHERE state IN ('queued','running','waiting') UNION ALL SELECT due_at t FROM discovery_runs WHERE state IN ('running','awaitingSelection','paused','stopping') UNION ALL SELECT deadline_at t FROM discovery_runs WHERE state IN ('running','awaitingSelection','paused','stopping') AND json_extract(body,'$.selectionSince') IS NULL UNION ALL SELECT next_due_at t FROM schedules s WHERE enabled=1 AND NOT EXISTS(SELECT 1 FROM runs r WHERE r.schedule_id=s.id AND r.state NOT IN ('finished','cancelled','failed')))",[],|r|r.get(0)).map_err(err)
    }

    /// One unreadable record must not keep the coordinator from starting, so
    /// each interrupted run is handled alone. Nothing here adds to a run, so a
    /// run already over its budget is still marked.
    pub fn recover(&self) -> Result<()> {
        let c = self.connection()?;
        let mut s=c.prepare("SELECT id FROM runs WHERE state NOT IN ('draft','finished','cancelled','failed') AND (state='cancelling' OR EXISTS(SELECT 1 FROM json_each(json_extract(body,'$.progress.receipts')) WHERE json_extract(value,'$.state')='running'))").map_err(err)?;
        let ids = s
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        // The app was not running after this, so a lost action isn't charged for
        // the time it stayed closed.
        let stopped = self
            .preference(HEARTBEAT)
            .ok()
            .flatten()
            .and_then(|v| v.as_i64());
        for id in ids {
            if let Err(error) = self.recover_run(&id, stopped) {
                eprintln!("Interrupted automation {id} needs attention: {error}");
            }
        }
        Ok(())
    }

    fn recover_run(&self, id: &str, stopped: Option<i64>) -> Result<()> {
        let mut run = self.get(id)?;
        for receipt in run.progress.receipts.values_mut() {
            if receipt.state == "running" {
                receipt.state = "unknown".into();
                receipt.finished_at = stopped.map(|t| t.max(receipt.started_at));
                receipt.error=Some("Pipeline stopped before the action outcome was recorded. Reconcile before retrying.".into());
            }
        }
        run.state = "attention".into();
        run.reason = Some("Interrupted task; inspect the recorded work before continuing".into());
        self.write(&mut run, "recovery", "Task interrupted")
    }
}

pub(crate) fn insert_run(
    tx: &rusqlite::Transaction<'_>,
    run: &TaskRun,
    operation: &str,
    fingerprint: &str,
) -> Result<()> {
    tx.execute("INSERT INTO runs(id,operation,fingerprint,revision,state,session_id,schedule_id,due_at,updated_at,body) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![run.id,operation,fingerprint,run.revision,run.state,run.scope.session_id,run.schedule_id,run.due_at,run.updated_at,serde_json::to_string(run).map_err(err)?]).map_err(err)?;
    tx.execute(
        "INSERT INTO events(run_id,at,kind,detail) VALUES(?1,?2,'created','Task created')",
        params![run.id, run.created_at],
    )
    .map_err(err)?;
    Ok(())
}
