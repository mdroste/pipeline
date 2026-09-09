use super::{
    definition::Chain,
    state::{Progress, Receipt},
    triggers::Trigger,
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, String>;
pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
pub fn id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("OS random source unavailable");
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}
pub fn hash(value: &impl Serialize) -> Result<String> {
    use sha2::{Digest, Sha256};
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).map_err(err)?)
    ))
}
pub fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    pub session_id: Option<String>,
    pub workspace_id: Option<String>,
    pub session_cursor: Option<String>,
    pub harness_fingerprint: Option<String>,
    pub runtime_root: Option<String>,
    #[serde(default)]
    pub root_identity: Option<String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, crate::commands::orchestration::PinnedReview>,
    #[serde(default)]
    pub checks: BTreeMap<String, String>,
    #[serde(default)]
    pub captured_checks: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRun {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mission_id: Option<String>,
    pub revision: i64,
    pub name: String,
    pub state: String,
    pub reason: Option<String>,
    pub chain: Chain,
    pub scope: Scope,
    pub inputs: Value,
    pub progress: Progress,
    pub created_at: i64,
    pub updated_at: i64,
    pub due_at: Option<i64>,
    pub deadline_at: i64,
    pub schedule_id: Option<String>,
    pub occurrence: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub revision: i64,
    pub name: String,
    pub state: String,
    pub reason: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub due_at: Option<i64>,
    pub session_id: Option<String>,
    pub schedule_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    pub id: String,
    pub revision: i64,
    pub name: String,
    pub enabled: bool,
    pub chain: Chain,
    pub scope: Scope,
    pub inputs: Value,
    pub trigger: Trigger,
    pub next_due_at: Option<i64>,
    pub created_at: i64,
    pub tzdb_version: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub sequence: i64,
    pub at: i64,
    pub kind: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attempts: Vec<Receipt>,
}
#[derive(Serialize, Deserialize)]
struct RetryEvent {
    detail: String,
    attempts: Vec<Receipt>,
}
#[derive(Debug, Clone)]
pub struct Store {
    pub root: PathBuf,
}
impl Store {
    pub fn default_root() -> Result<PathBuf> {
        #[cfg(feature = "e2e")]
        if let Some(root) = std::env::var_os("PIPELINE_E2E_TASK_ROOT") {
            return Ok(PathBuf::from(root));
        }
        Ok(crate::storage::data_root()?.join("orchestration"))
    }
    pub fn open(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root).map_err(err)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700)).map_err(err)?;
        }
        let store = Self { root: root.into() };
        let c = store.connection()?;
        let version: i64 = c
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(err)?;
        if version > 3 {
            return Err("Task store was written by a newer Pipeline version".into());
        }
        if version == 0 {
            c.execute_batch("BEGIN IMMEDIATE;
            CREATE TABLE runs(id TEXT PRIMARY KEY, operation TEXT UNIQUE NOT NULL, fingerprint TEXT NOT NULL, revision INTEGER NOT NULL, state TEXT NOT NULL, session_id TEXT, schedule_id TEXT, due_at INTEGER, updated_at INTEGER NOT NULL, body TEXT NOT NULL);
            CREATE INDEX run_ready ON runs(state,due_at); CREATE INDEX run_session ON runs(session_id,updated_at DESC); CREATE INDEX run_schedule ON runs(schedule_id,state);
            CREATE TABLE events(sequence INTEGER PRIMARY KEY AUTOINCREMENT,run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,at INTEGER NOT NULL,kind TEXT NOT NULL,detail TEXT NOT NULL);
            CREATE INDEX event_run ON events(run_id,sequence);
            CREATE TABLE schedules(id TEXT PRIMARY KEY,revision INTEGER NOT NULL,enabled INTEGER NOT NULL,next_due_at INTEGER,body TEXT NOT NULL);
            CREATE INDEX schedule_due ON schedules(enabled,next_due_at);
            CREATE TABLE occurrences(schedule_id TEXT NOT NULL,activation INTEGER NOT NULL,at INTEGER NOT NULL,run_id TEXT,disposition TEXT NOT NULL,PRIMARY KEY(schedule_id,activation,at));
            CREATE TABLE signals(id TEXT PRIMARY KEY,run_id TEXT NOT NULL REFERENCES runs(id),address TEXT NOT NULL,body TEXT NOT NULL,consumed INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE chains(id TEXT PRIMARY KEY,body TEXT NOT NULL);
            CREATE TABLE preferences(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            PRAGMA user_version=1; COMMIT;").map_err(err)?;
        }
        if version < 2 {
            if version == 1 {
                let backup = root.join("tasks-before-missions-v2.sqlite3");
                if !backup.exists() {
                    c.backup(rusqlite::MAIN_DB, &backup, None).map_err(err)?;
                }
            }
            c.execute_batch(include_str!("missions/schema.sql"))
                .map_err(err)?;
        }
        if version < 3 {
            if version > 0 {
                let backup = root.join("tasks-before-discovery-v3.sqlite3");
                if !backup.exists() {
                    c.backup(rusqlite::MAIN_DB, &backup, None).map_err(err)?;
                }
            }
            c.execute_batch(include_str!("discovery/schema.sql"))
                .map_err(err)?;
        }
        Ok(store)
    }
    pub fn connection(&self) -> Result<rusqlite::Connection> {
        let c = rusqlite::Connection::open(self.root.join("tasks.sqlite3")).map_err(err)?;
        c.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(err)?;
        c.pragma_update(None, "foreign_keys", true).map_err(err)?;
        c.pragma_update(None, "journal_mode", "WAL").map_err(err)?;
        Ok(c)
    }
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
        super::definition::validate(&chain)?;
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

    pub fn list(&self, view: &str, session: Option<&str>, offset: u32) -> Result<Vec<Summary>> {
        let c = self.connection()?;
        let predicate = match view {
            "session" => "1=1",
            "history" => "state IN ('finished','cancelled','failed')",
            "drafts" => "state='draft'",
            "ready" => "state IN ('queued','running','waiting')",
            _ => "state NOT IN ('finished','cancelled','failed')",
        };
        let sql=format!("SELECT id,revision,json_extract(body,'$.name'),state,json_extract(body,'$.reason'),json_extract(body,'$.createdAt'),updated_at,due_at,session_id,schedule_id FROM runs WHERE {predicate} AND NOT EXISTS (SELECT 1 FROM mission_children mc WHERE mc.task_id=runs.id) AND NOT EXISTS (SELECT 1 FROM discovery_children dc WHERE dc.task_id=runs.id) AND (?1 IS NULL OR session_id=?1) ORDER BY updated_at DESC,id LIMIT 50 OFFSET ?2");
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
        self.connection()?.query_row("SELECT MIN(t) FROM (SELECT due_at t FROM runs WHERE state IN ('queued','running','waiting') UNION ALL SELECT json_extract(body,'$.deadlineAt') t FROM runs WHERE state IN ('queued','running','waiting') UNION ALL SELECT due_at t FROM missions WHERE state IN ('queued','running','waiting','paused','stopping') UNION ALL SELECT json_extract(body,'$.deadlineAt') t FROM missions WHERE state IN ('queued','running','waiting') UNION ALL SELECT due_at t FROM discovery_runs WHERE state IN ('running','awaitingSelection','paused','stopping') UNION ALL SELECT deadline_at t FROM discovery_runs WHERE state IN ('running','awaitingSelection','paused','stopping') UNION ALL SELECT next_due_at t FROM schedules s WHERE enabled=1 AND NOT EXISTS(SELECT 1 FROM runs r WHERE r.schedule_id=s.id AND r.state NOT IN ('finished','cancelled','failed')))",[],|r|r.get(0)).map_err(err)
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
    pub fn recover(&self) -> Result<()> {
        let c = self.connection()?;
        let mut s=c.prepare("SELECT id FROM runs WHERE state NOT IN ('draft','finished','cancelled','failed') AND (state='cancelling' OR EXISTS(SELECT 1 FROM json_each(json_extract(body,'$.progress.receipts')) WHERE json_extract(value,'$.state')='running'))").map_err(err)?;
        let ids = s
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        for id in ids {
            let mut run = self.get(&id)?;
            for receipt in run.progress.receipts.values_mut() {
                if receipt.state == "running" {
                    receipt.state = "unknown".into();
                    receipt.error=Some("Pipeline stopped before the action outcome was recorded. Reconcile before retrying.".into());
                }
            }
            run.state = "attention".into();
            run.reason =
                Some("Interrupted task; inspect the recorded work before continuing".into());
            self.save(&mut run, "recovery", "Task interrupted")?;
        }
        Ok(())
    }
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
        if !["waiting", "paused", "running", "queued"].contains(&run.state.as_str()) {
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
    pub fn save_schedule(&self, mut schedule: Schedule, expected: Option<i64>) -> Result<Schedule> {
        schedule.trigger.validate()?;
        super::definition::validate(&schedule.chain)?;
        if !schedule.trigger.recurring() {
            return Err("Use a delayed task for one-time starts".into());
        }
        let mut c = self.connection()?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        if let Some(rev) = expected {
            schedule.revision = rev + 1;
            let changed=tx.execute("UPDATE schedules SET revision=?2,enabled=?3,next_due_at=?4,body=?5 WHERE id=?1 AND revision=?6",params![schedule.id,schedule.revision,schedule.enabled,schedule.next_due_at,serde_json::to_string(&schedule).map_err(err)?,rev]).map_err(err)?;
            if changed != 1 {
                return Err("Schedule changed; reload before editing".into());
            }
        } else {
            tx.execute("INSERT INTO schedules(id,revision,enabled,next_due_at,body) VALUES(?1,?2,?3,?4,?5)",params![schedule.id,schedule.revision,schedule.enabled,schedule.next_due_at,serde_json::to_string(&schedule).map_err(err)?]).map_err(err)?;
        }
        tx.commit().map_err(err)?;
        Ok(schedule)
    }
    pub fn activate_schedule(&self, run: &mut TaskRun, trigger: Trigger) -> Result<()> {
        if run.state != "draft" || !trigger.recurring() {
            return Err("Only a recurring draft can create a schedule".into());
        }
        let schedule = Schedule {
            id: run.id.clone(),
            revision: 0,
            name: run.name.clone(),
            enabled: true,
            chain: run.chain.clone(),
            scope: run.scope.clone(),
            inputs: run.inputs.clone(),
            next_due_at: trigger.next(now() - 1)?,
            trigger,
            created_at: now(),
            tzdb_version: chrono_tz::IANA_TZDB_VERSION.into(),
        };
        let mut c = self.connection()?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        let expected = run.revision;
        run.revision += 1;
        run.state = "finished".into();
        run.reason = Some("Schedule created".into());
        run.updated_at = now();
        run.due_at = None;
        let changed=tx.execute("UPDATE runs SET revision=?2,state=?3,updated_at=?4,body=?5,due_at=NULL WHERE id=?1 AND revision=?6",params![run.id,run.revision,run.state,run.updated_at,serde_json::to_string(run).map_err(err)?,expected]).map_err(err)?;
        if changed != 1 {
            return Err("Task changed; reload before starting".into());
        }
        tx.execute(
            "INSERT INTO schedules(id,revision,enabled,next_due_at,body) VALUES(?1,0,1,?2,?3)",
            params![
                schedule.id,
                schedule.next_due_at,
                serde_json::to_string(&schedule).map_err(err)?
            ],
        )
        .map_err(err)?;
        tx.execute("INSERT INTO events(run_id,at,kind,detail) VALUES(?1,?2,'scheduled','Schedule created')",params![run.id,now()]).map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(())
    }
    pub fn schedules(&self) -> Result<Vec<Schedule>> {
        let c = self.connection()?;
        let mut s = c
            .prepare("SELECT body FROM schedules ORDER BY id LIMIT 1000")
            .map_err(err)?;
        let bodies = s
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        bodies
            .iter()
            .map(|v| serde_json::from_str(v).map_err(err))
            .collect()
    }
    pub fn fire_schedules(&self, time: i64) -> Result<()> {
        let due = {
            let c = self.connection()?;
            let mut q=c.prepare("SELECT s.body FROM schedules s WHERE s.enabled=1 AND s.next_due_at<=?1 AND NOT EXISTS(SELECT 1 FROM runs r WHERE r.schedule_id=s.id AND r.state NOT IN ('finished','cancelled','failed')) ORDER BY s.next_due_at,s.id LIMIT 32").map_err(err)?;
            let rows = q
                .query_map([time], |r| r.get::<_, String>(0))
                .map_err(err)?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(err)?;
            rows.into_iter()
                .map(|v| serde_json::from_str::<Schedule>(&v).map_err(err))
                .collect::<Result<Vec<_>>>()?
        };
        for mut schedule in due {
            let mut c = self.connection()?;
            let tx = c
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(err)?;
            let revision: i64 = tx
                .query_row(
                    "SELECT revision FROM schedules WHERE id=?1",
                    [&schedule.id],
                    |r| r.get(0),
                )
                .map_err(err)?;
            if revision != schedule.revision {
                continue;
            }
            let first = schedule.next_due_at.ok_or("Missing schedule time")?;
            // Arithmetic interval coalescing and a bounded calendar scan avoid replay storms.
            let latest = match &schedule.trigger {
                Trigger::Interval { seconds, .. } => {
                    first + (time - first) / (*seconds as i64) * (*seconds as i64)
                }
                _ => {
                    let mut latest = first;
                    let mut cursor = first.max(time.saturating_sub(8 * 86400));
                    for _ in 0..10 {
                        match schedule.trigger.next(cursor)? {
                            Some(v) if v <= time => {
                                latest = v;
                                cursor = v;
                            }
                            _ => break,
                        }
                    }
                    latest
                }
            };
            let active:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM runs WHERE schedule_id=?1 AND state NOT IN ('finished','cancelled','failed'))",[&schedule.id],|r|r.get(0)).map_err(err)?;
            if active {
                continue;
            } // Keep the overdue cursor as the single coalesced pending occurrence.
            let operation = format!("schedule-{}-{}-{latest}", schedule.id, schedule.revision);
            let run = TaskRun {
                id: id(),
                mission_id: None,
                revision: 0,
                name: schedule.name.clone(),
                state: "queued".into(),
                reason: None,
                chain: schedule.chain.clone(),
                scope: schedule.scope.clone(),
                inputs: schedule.inputs.clone(),
                progress: Progress::default(),
                created_at: time,
                updated_at: time,
                due_at: Some(time),
                deadline_at: time + schedule.chain.limits.deadline_hours as i64 * 3600,
                schedule_id: Some(schedule.id.clone()),
                occurrence: Some(latest),
            };
            let inserted=tx.execute("INSERT OR IGNORE INTO occurrences(schedule_id,activation,at,run_id,disposition) VALUES(?1,?2,?3,?4,'queued')",params![schedule.id,schedule.revision,latest,run.id]).map_err(err)?;
            if inserted > 0 {
                insert_run(&tx, &run, &operation, &hash(&(&schedule.chain, latest))?)?;
                if latest != first {
                    tx.execute("INSERT OR IGNORE INTO occurrences(schedule_id,activation,at,disposition) VALUES(?1,?2,?3,'coalesced')",params![schedule.id,schedule.revision,first]).map_err(err)?;
                }
            }
            schedule.next_due_at = schedule.trigger.next(time)?;
            tx.execute(
                "UPDATE schedules SET next_due_at=?2,body=?3 WHERE id=?1",
                params![
                    schedule.id,
                    schedule.next_due_at,
                    serde_json::to_string(&schedule).map_err(err)?
                ],
            )
            .map_err(err)?;
            tx.commit().map_err(err)?;
        }
        Ok(())
    }
    pub fn preference(&self, key: &str) -> Result<Option<Value>> {
        let value: Option<String> = self
            .connection()?
            .query_row("SELECT value FROM preferences WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(err)?;
        value
            .map(|v| serde_json::from_str(&v).map_err(err))
            .transpose()
    }
    pub fn set_preference(&self, key: &str, value: &Value) -> Result<()> {
        self.connection()?.execute("INSERT INTO preferences(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value.to_string()]).map_err(err)?;
        Ok(())
    }
    pub fn save_chain(&self, chain: &Chain) -> Result<String> {
        super::definition::validate(chain)?;
        let key = hash(chain)?;
        self.connection()?
            .execute(
                "INSERT OR IGNORE INTO chains(id,body) VALUES(?1,?2)",
                params![key, serde_json::to_string(chain).map_err(err)?],
            )
            .map_err(err)?;
        Ok(key)
    }
    pub fn chains(&self) -> Result<Vec<Value>> {
        let c = self.connection()?;
        let mut s = c
            .prepare("SELECT id,body FROM chains ORDER BY rowid DESC LIMIT 200")
            .map_err(err)?;
        let result = s
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        result
            .into_iter()
            .map(|(id, body)| {
                Ok(json!({"id":id,"chain":serde_json::from_str::<Value>(&body).map_err(err)?}))
            })
            .collect()
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
