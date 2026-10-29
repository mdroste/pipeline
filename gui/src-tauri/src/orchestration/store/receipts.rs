//! Operation-scoped receipts, retry journals and attempt history.
use super::{err, now, Event, Result, Store, TaskRun};
use crate::orchestration::state::Receipt;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

impl Store {
    pub fn retry(&self, run: &mut TaskRun) -> Result<()> {
        if run.state != "attention" {
            return Err("Only a task needing attention can be retried".into());
        }
        let attempts: Vec<_> = run
            .progress
            .receipts
            .values()
            .filter(|r| matches!(r.state.as_str(), "unknown" | "failed"))
            .cloned()
            .collect();
        if attempts.is_empty() {
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
        next.state = "queued".into();
        next.due_at = Some(now());
        next.reason = None;
        // Removal and the complete prior receipts commit in the same transaction.
        self.save(&mut next, "retry", &event)?;
        *run = next;
        Ok(())
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

    pub fn events(&self, id: &str, after: i64) -> Result<Vec<Event>> {
        let c = self.connection()?;
        let mut s=c.prepare("SELECT sequence,at,kind,detail FROM events WHERE run_id=?1 AND sequence>?2 ORDER BY sequence LIMIT 100").map_err(err)?;
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

#[derive(Serialize, Deserialize)]
struct RetryEvent {
    detail: String,
    attempts: Vec<Receipt>,
}
