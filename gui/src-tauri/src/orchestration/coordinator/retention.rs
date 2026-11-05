//! Bounded removal of old finished runs and of the files only they named.
//!
//! Only plain finished runs go, and only when no one can still need them: a run
//! with a result stays until the researcher has opened it, and runs that belong
//! to a research automation or seed a schedule are never candidates. Blobs are
//! shared by content, so one goes only when no surviving record names it.
use super::store::{err, now, Result, Store, TaskRun};
use super::{adapters, Coordinator};
use rusqlite::params;
use serde_json::json;
use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, SystemTime};

const PASS_INTERVAL_SECS: i64 = 86_400;
const KEEP_SECS: i64 = 30 * 86_400;
const RUNS_PER_PASS: i64 = 200;
const LAST_PASS: &str = "retentionAt";
/// A blob this new may belong to work that isn't recorded yet.
const BLOB_GRACE: Duration = Duration::from_secs(86_400);
const JOURNAL_LIMIT: u64 = 16 * 1024 * 1024;

fn is_operation(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Every 64-digit hex token in `text`. Anything that merely looks like a digest
/// only keeps a blob longer.
fn collect_digests(text: &str, named: &mut HashSet<String>) {
    for token in text.split(|c: char| !c.is_ascii_hexdigit()) {
        if token.len() == 64 {
            named.insert(token.to_string());
        }
    }
}

impl Store {
    /// The researcher opened this finished run, so its result has been seen.
    pub fn mark_seen(&self, id: &str) -> Result<()> {
        self.connection()?.execute("INSERT INTO events(run_id,at,kind,detail) SELECT id,?2,'seen','Result opened' FROM runs WHERE id=?1 AND state IN ('finished','cancelled','failed') AND NOT EXISTS(SELECT 1 FROM events WHERE run_id=?1 AND kind='seen')",params![id,now()]).map_err(err)?;
        Ok(())
    }

    fn retention_due(&self, time: i64) -> Result<bool> {
        let last = self.preference(LAST_PASS)?.and_then(|v| v.as_i64());
        Ok(last.is_none_or(|t| t + PASS_INTERVAL_SECS <= time))
    }

    /// Finished runs past the retention age that nothing still depends on.
    fn old_runs(&self, time: i64) -> Result<Vec<TaskRun>> {
        let c = self.connection()?;
        let mut q=c.prepare("SELECT r.body FROM runs r WHERE r.state IN ('finished','cancelled','failed') AND r.updated_at<?1 AND NOT EXISTS(SELECT 1 FROM mission_children m WHERE m.task_id=r.id) AND NOT EXISTS(SELECT 1 FROM discovery_children d WHERE d.task_id=r.id) AND NOT EXISTS(SELECT 1 FROM schedules s WHERE s.id=r.id) AND (EXISTS(SELECT 1 FROM events e WHERE e.run_id=r.id AND e.kind='seen') OR NOT EXISTS(SELECT 1 FROM json_each(json_extract(r.body,'$.progress.receipts')) j WHERE json_extract(j.value,'$.state')='completed')) ORDER BY r.updated_at,r.id LIMIT ?2").map_err(err)?;
        let bodies = q
            .query_map(params![time - KEEP_SECS, RUNS_PER_PASS], |r| {
                r.get::<_, String>(0)
            })
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        // A record that can't be read stays where it is.
        Ok(bodies
            .iter()
            .filter_map(|body| serde_json::from_str(body).ok())
            .collect())
    }

    /// Journals of this run, including attempts that a Retry replaced.
    fn run_operations(&self, run: &TaskRun) -> Result<Vec<String>> {
        let c = self.connection()?;
        let mut q=c.prepare("SELECT DISTINCT json_extract(j.value,'$.operation') FROM events e, json_each(CASE WHEN json_valid(e.detail) THEN e.detail ELSE '{}' END,'$.attempts') j WHERE e.run_id=?1 AND e.kind='retry'").map_err(err)?;
        let mut operations = q
            .query_map([&run.id], |r| r.get::<_, String>(0))
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        operations.extend(run.progress.receipts.values().map(|r| r.operation.clone()));
        Ok(operations)
    }

    /// Files go first: a crash partway leaves a run to remove next time, never
    /// a journal that no record owns.
    fn remove_runs(&self, runs: &[TaskRun]) -> Result<usize> {
        self.remove_unpinned_runs(runs, crate::commands::orchestration::is_pinned)
    }

    /// A pin is released through the journal, and nothing else knows which
    /// Review it holds, so a run with a pinned Review waits for a later pass.
    pub(super) fn remove_unpinned_runs(
        &self,
        runs: &[TaskRun],
        pinned: impl Fn(&str) -> Result<bool>,
    ) -> Result<usize> {
        let mut removed = Vec::new();
        for run in runs {
            let clear = || -> Result<()> {
                let operations = self.run_operations(run)?;
                if operations.iter().any(|o| !is_operation(o)) {
                    return Err("Invalid task operation".into());
                }
                for operation in &operations {
                    if let Some(review) = adapters::journaled_review(self, operation)? {
                        if pinned(&review)? {
                            return Err("Its Review is still pinned".into());
                        }
                    }
                }
                for operation in operations {
                    let path = self.root.join("actions").join(operation);
                    if path.try_exists().map_err(err)? {
                        std::fs::remove_dir_all(path).map_err(err)?;
                    }
                }
                Ok(())
            };
            match clear() {
                Ok(()) => removed.push(run.id.as_str()),
                Err(error) => eprintln!("Old automation {} stays: {error}", run.id),
            }
        }
        let mut c = self.connection()?;
        let tx = c.transaction().map_err(err)?;
        for id in &removed {
            tx.execute("DELETE FROM signals WHERE run_id=?1", [id])
                .map_err(err)?;
            tx.execute("DELETE FROM occurrences WHERE run_id=?1", [id])
                .map_err(err)?;
            tx.execute("DELETE FROM runs WHERE id=?1", [id])
                .map_err(err)?;
        }
        tx.commit().map_err(err)?;
        Ok(removed.len())
    }

    fn sweep_blobs(&self) -> Result<()> {
        let root = self.root.join("blobs");
        if !root.is_dir() {
            return Ok(());
        }
        let mut named = HashSet::new();
        let c = self.connection()?;
        for sql in [
            "SELECT body FROM runs",
            "SELECT detail FROM events WHERE kind='retry'",
            "SELECT body FROM schedules",
            "SELECT body FROM chains",
            "SELECT body FROM missions",
            "SELECT body FROM discovery_runs",
            "SELECT body FROM discovery_records",
        ] {
            let mut q = c.prepare(sql).map_err(err)?;
            let mut rows = q.query([]).map_err(err)?;
            while let Some(row) = rows.next().map_err(err)? {
                collect_digests(&row.get::<_, String>(0).map_err(err)?, &mut named);
            }
        }
        self.scan_journals(&mut named)?;
        let cutoff = SystemTime::now() - BLOB_GRACE;
        for entry in std::fs::read_dir(&root).map_err(err)? {
            let entry = entry.map_err(err)?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let digest = name.strip_prefix("tree-").unwrap_or(&name);
            let old = entry
                .metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|t| t < cutoff);
            if digest.len() == 64
                && digest.bytes().all(|b| b.is_ascii_hexdigit())
                && !named.contains(digest)
                && old
            {
                std::fs::remove_dir_all(entry.path()).map_err(err)?;
            }
        }
        Ok(())
    }

    fn scan_journals(&self, named: &mut HashSet<String>) -> Result<()> {
        let root = self.root.join("actions");
        if !root.is_dir() {
            return Ok(());
        }
        for action in std::fs::read_dir(root).map_err(err)? {
            for file in std::fs::read_dir(action.map_err(err)?.path()).map_err(err)? {
                let path = file.map_err(err)?.path();
                if path.extension().is_some_and(|e| e == "json") {
                    collect_digests(&read_journal(&path)?, named);
                }
            }
        }
        Ok(())
    }
}

fn read_journal(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut text = Vec::new();
    crate::safety::open_regular_file(path)?
        .take(JOURNAL_LIMIT)
        .read_to_end(&mut text)
        .map_err(err)?;
    Ok(String::from_utf8_lossy(&text).into_owned())
}

impl Coordinator {
    /// Called under the gate with nothing dispatched, so no snapshot can sit
    /// between its blob and the record that names it.
    pub(super) async fn retain(&self) {
        if !self
            .active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
        {
            return;
        }
        if let Err(error) = self.retention_pass().await {
            eprintln!("Automation history cleanup needs attention: {error}");
        }
    }

    async fn retention_pass(&self) -> Result<()> {
        let time = now();
        if !self.db(move |s| s.retention_due(time)).await? {
            return Ok(());
        }
        // A pin is released through the journal this pass may remove.
        let old = self
            .db(move |s| {
                adapters::reconcile_abandoned_reviews(&s)?;
                s.old_runs(time)
            })
            .await?;
        for run in &old {
            if !run.progress.decisions.contains_key("occurrenceSession") {
                continue;
            }
            let Some(session) = run.scope.session_id.clone() else {
                continue;
            };
            if let Err(error) = crate::workbench::commands::run_store(move |s| {
                crate::workbench::tasks::archive_occurrence(&s, &session)
            })
            .await
            {
                eprintln!("Automation conversation stays open: {}", error.message);
            }
        }
        self.db(move |s| {
            if s.remove_runs(&old)? > 0 {
                s.sweep_blobs()?;
            }
            s.set_preference(LAST_PASS, &json!(time))
        })
        .await
    }
}
