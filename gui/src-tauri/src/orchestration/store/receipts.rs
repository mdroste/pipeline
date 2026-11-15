//! Operation-scoped receipts, retry journals and attempt history.
use super::{err, now, Event, Result, Store, TaskRun};
use crate::orchestration::state::Receipt;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

impl Store {
    pub(crate) fn completion_pending(&self, id: &str, operation: &str) -> Result<bool> {
        self.connection()?.query_row("SELECT EXISTS(SELECT 1 FROM runs r,json_each(CASE WHEN json_valid(r.body) THEN r.body ELSE '{}' END,'$.progress.receipts') a WHERE r.id=?1 AND r.state<>'unreadable' AND json_extract(a.value,'$.state')='running' AND json_extract(a.value,'$.operation')=?2)",params![id,operation],|r|r.get(0)).map_err(err)
    }
    pub fn retry(&self, run: &mut TaskRun) -> Result<()> {
        if run.state != "attention" {
            return Err("Only a task needing attention can be retried".into());
        }
        self.restore_group_clocks(run)?;
        if let Some(group) = run.progress.expired_group(now()) {
            return Err(format!(
                "Group {group} reached its elapsed deadline. Start a new run to give it more time."
            ));
        }
        let attempts: Vec<_> = run
            .progress
            .receipts
            .values()
            .filter(|r| matches!(r.state.as_str(), "unknown" | "failed"))
            .cloned()
            .collect();
        let expired = run.deadline_at <= now();
        if attempts.is_empty() && !expired {
            return Err("No failed action to retry; inspect the condition inputs".into());
        }
        let event = serde_json::to_string(&RetryEvent {
            detail: "Retry requested; previous attempts retained".into(),
            attempts,
        })
        .map_err(err)?;
        let mut next = run.clone();
        next.progress
            .receipts
            .retain(|_, r| !matches!(r.state.as_str(), "unknown" | "failed"));
        if expired {
            // Choosing Retry is the decision to continue past the deadline. The
            // same deadline would only send the run straight back here.
            next.deadline_at = now() + next.chain.limits.deadline_hours as i64 * 3600;
        }
        next.state = "queued".into();
        next.due_at = Some(now());
        next.reason = None;
        // Removal and the complete prior receipts commit in the same transaction.
        self.save(&mut next, "retry", &event)?;
        *run = next;
        Ok(())
    }

    /// Locate the action that actually delivered a legacy exchange, including
    /// an attempt later moved into retry history. Operation IDs are not paths.
    pub(crate) fn delivery_producer(
        &self,
        session: &str,
        operation: &str,
    ) -> Result<Option<(TaskRun, String)>> {
        let c = self.connection()?;
        let source:Option<(String,String)>=c.query_row(
            "SELECT r.id,json_extract(j.value,'$.stepId') FROM runs r, json_each(CASE WHEN json_valid(r.body) THEN r.body ELSE '{}' END,'$.progress.receipts') j
             WHERE r.session_id=?1 AND r.state<>'unreadable' AND json_extract(j.value,'$.operation')=?2
             UNION ALL SELECT r.id,json_extract(j.value,'$.stepId') FROM runs r JOIN events e ON e.run_id=r.id,
             json_each(CASE WHEN json_valid(e.detail) THEN e.detail ELSE '{}' END,'$.attempts') j
             WHERE r.session_id=?1 AND e.kind='retry' AND json_extract(j.value,'$.operation')=?2 LIMIT 1",
            params![session,operation],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(err)?;
        source
            .map(|(id, step)| Ok((self.get(&id)?, step)))
            .transpose()
    }

    /// Retry moves an old receipt into the append-only event in the same transaction
    /// that frees its address. It still counts against its enclosing group instances.
    pub fn retried_addresses(&self, run: &str) -> Result<Vec<String>> {
        let c = self.connection()?;
        let mut q=c.prepare("SELECT json_extract(j.value,'$.address') FROM events e, json_each(CASE WHEN json_valid(e.detail) THEN e.detail ELSE '{}' END,'$.attempts') j WHERE e.run_id=?1 AND e.kind='retry'").map_err(err)?;
        let rows = q.query_map([run], |r| r.get(0)).map_err(err)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)
    }

    pub fn step_receipt(
        &self,
        id: &str,
        address: &str,
        operation: Option<&str>,
    ) -> Result<Receipt> {
        let run = self.get(id)?;
        if let Some(receipt) = run.progress.receipts.get(address) {
            if operation.is_none_or(|op| op == receipt.operation) {
                return Ok(receipt.clone());
            }
        }
        if let Some(operation) = operation {
            let body: Option<String> = self.connection()?.query_row(
                "SELECT attempt.value FROM events e, json_each(CASE WHEN json_valid(e.detail) THEN e.detail ELSE '{}' END, '$.attempts') attempt
                 WHERE e.run_id=?1 AND e.kind='retry' AND json_extract(attempt.value,'$.address')=?2 AND json_extract(attempt.value,'$.operation')=?3 LIMIT 1",
                params![id, address, operation], |r| r.get(0),
            ).optional().map_err(err)?;
            if let Some(body) = body {
                return serde_json::from_str(&body).map_err(err);
            }
        }
        Err("Step attempt was not found in this task".into())
    }

    pub fn step_output(&self, id: &str, address: &str, operation: Option<&str>) -> Result<Value> {
        let receipt = self.step_receipt(id, address, operation)?;
        let path = self
            .root
            .join("actions")
            .join(&receipt.operation)
            .join("result.json");
        if path.is_file() {
            serde_json::from_slice(&std::fs::read(path).map_err(err)?).map_err(err)
        } else {
            Ok(receipt.output.unwrap_or(Value::Null))
        }
    }

    pub fn abandoned_operations(&self) -> Result<Vec<String>> {
        let c = self.connection()?;
        let mut q = c.prepare("SELECT DISTINCT json_extract(j.value,'$.operation') FROM runs r, json_each(r.body,'$.progress.receipts') j WHERE r.state='cancelled' UNION SELECT DISTINCT json_extract(j.value,'$.operation') FROM events e, json_each(CASE WHEN json_valid(e.detail) THEN e.detail ELSE '{}' END,'$.attempts') j WHERE e.kind='retry'").map_err(err)?;
        let operations = q
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err);
        operations
    }

    /// Operations whose result a run has durably taken.
    pub fn adopted_operations(&self) -> Result<Vec<String>> {
        let c = self.connection()?;
        let mut q = c.prepare("SELECT DISTINCT json_extract(j.value,'$.operation') FROM runs r, json_each(r.body,'$.progress.receipts') j WHERE r.state<>'unreadable' AND json_valid(r.body) AND json_extract(j.value,'$.state')='completed' AND json_extract(j.value,'$.operation') IS NOT NULL").map_err(err)?;
        let operations = q
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err);
        operations
    }

    pub fn events(&self, id: &str, after: i64) -> Result<Vec<Event>> {
        let c = self.connection()?;
        let mut s=c.prepare("SELECT sequence,at,kind,detail FROM events WHERE run_id=?1 AND sequence>?2 AND kind<>'seen' ORDER BY sequence LIMIT 100").map_err(err)?;
        let result = s
            .query_map(params![id, after], |r| {
                Ok(Event {
                    sequence: r.get(0)?,
                    at: r.get(1)?,
                    kind: r.get(2)?,
                    detail: r.get(3)?,
                    attempts: Vec::new(),
                })
            })
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        result
            .into_iter()
            .map(|mut event| {
                // Legacy retry events contain plain text and remain readable.
                if event.kind == "retry" && event.detail.starts_with('{') {
                    let retry: RetryEvent = serde_json::from_str(&event.detail).map_err(err)?;
                    event.detail = retry.detail;
                    event.attempts = retry.attempts;
                }
                Ok(event)
            })
            .collect()
    }
}

impl TaskRun {
    /// Past its deadline a requeued run would only come straight back to
    /// attention, so continuing is left to Retry, which starts a new deadline.
    pub fn requeue_after_reconcile(&mut self) {
        if self
            .progress
            .receipts
            .values()
            .any(|r| r.state == "unknown")
        {
            return;
        }
        if let Some(failed) = self
            .progress
            .receipts
            .values()
            .find(|r| r.state == "failed")
        {
            self.reason = failed.error.clone();
        } else if let Some(group) = self.progress.expired_group(now()) {
            self.reason = Some(format!(
                "Group {group} reached its elapsed deadline. Start a new run to give it more time."
            ));
        } else if self.deadline_at <= now() {
            self.reason = Some(
                "Saved results are recorded and the deadline has passed. Choose Retry unfinished actions to continue with a new deadline.".into(),
            );
        } else {
            self.state = "queued".into();
            self.due_at = Some(now());
            self.reason = None;
        }
    }
}

#[derive(Serialize, Deserialize)]
struct RetryEvent {
    detail: String,
    attempts: Vec<Receipt>,
}
