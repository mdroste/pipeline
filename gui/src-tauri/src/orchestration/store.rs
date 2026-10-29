use serde::Serialize;
use std::path::{Path, PathBuf};

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
mod chains;
mod receipts;
mod records;
mod runs;
mod schedules;
mod signals;
pub use records::{Event, Schedule, Scope, Summary, TaskRun};
pub(crate) use runs::insert_run;

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
}
