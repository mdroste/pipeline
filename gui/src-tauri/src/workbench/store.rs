//! Versioned local persistence for Workbench-owned research state.

use fs2::FileExt as _;
use rusqlite::{params, Connection, OptionalExtension as _, Row, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[cfg(not(unix))]
use sha2::{Digest as _, Sha256};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const CURRENT_SCHEMA_VERSION: u32 = 15;
const MAX_WORKSPACE_NAME_BYTES: usize = 300;
const MAX_SESSION_TITLE_BYTES: usize = 300;
const MAX_DRAFT_BYTES: usize = 1_000_000;
const MAX_OVERRIDES_BYTES: usize = 256_000;
const MAX_SESSIONS_PER_LIST: usize = 500;
const MAX_WORKSPACES_PER_LIST: usize = 500;
const MAX_PROJECTED_PAYLOAD_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub recovery: Option<String>,
}

impl WorkbenchError {
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_input".to_string(),
            message: message.into(),
            retryable: false,
            recovery: None,
        }
    }

    pub(crate) fn conflict(message: impl Into<String>) -> Self {
        Self {
            code: "storage_conflict".to_string(),
            message: message.into(),
            retryable: true,
            recovery: Some("Refresh the Workbench record and retry your edit.".to_string()),
        }
    }

    pub(crate) fn storage(context: &str, error: impl std::fmt::Display) -> Self {
        Self {
            code: "storage_error".to_string(),
            message: format!("{context}: {error}"),
            retryable: false,
            recovery: Some("Retry after checking local storage availability.".to_string()),
        }
    }

    fn newer(found: u32) -> Self {
        Self {
            code: "newer_store_schema".to_string(),
            message: format!(
                "This Workbench store uses schema version {found}; this Pipeline build supports through version {CURRENT_SCHEMA_VERSION}."
            ),
            retryable: false,
            recovery: Some("Open the store with the newer Pipeline version that created it.".to_string()),
        }
    }

    pub(crate) fn worker(message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: "storage_worker_failed".to_string(),
            message: message.into(),
            retryable,
            recovery: Some(
                "Retry the operation. Restart Pipeline if database workers remain unavailable."
                    .to_string(),
            ),
        }
    }
}

pub type WorkbenchResult<T> = Result<T, WorkbenchError>;

#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
    database: PathBuf,
    backups: PathBuf,
    codex_home: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub root: Option<String>,
    pub root_identity: Option<String>,
    pub settings_revision: i64,
    pub revision: i64,
    pub archived_at: Option<String>,
    pub missing_root_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchSession {
    pub id: String,
    pub workspace_id: Option<String>,
    pub paper_id: Option<String>,
    pub title: String,
    pub preset_id: Option<String>,
    pub overrides: Value,
    pub draft: String,
    pub revision: i64,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mutation<T> {
    pub record: T,
    pub sequence: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionList {
    pub sessions: Vec<WorkbenchSession>,
    pub sequence: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceList {
    pub workspaces: Vec<Workspace>,
    pub sequence: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub workspace: Option<Workspace>,
    pub session: WorkbenchSession,
    pub sequence: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationTurn {
    pub id: String,
    pub binding_id: String,
    pub client_submission_id: String,
    pub provider_turn_id: Option<String>,
    pub state: String,
    pub error: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
    pub terminal_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptItem {
    pub id: String,
    pub turn_id: Option<String>,
    pub provider_item_id: String,
    pub item_kind: String,
    pub payload: Value,
    pub is_final: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSnapshot {
    pub workspace: Option<Workspace>,
    pub session: WorkbenchSession,
    pub active_binding: Option<SessionBinding>,
    pub turns: Vec<ConversationTurn>,
    pub items: Vec<TranscriptItem>,
    pub sequence: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileResult {
    pub changed: usize,
    pub missing: usize,
    pub restored: usize,
    pub sequence: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionBinding {
    pub id: String,
    pub session_id: String,
    pub runtime_namespace: String,
    pub provider_thread_id: String,
    pub incarnation: i64,
    pub created_at: String,
    pub retired_at: Option<String>,
    pub retirement_reason: Option<String>,
    #[serde(default)]
    pub harness_fingerprint: Option<String>,
    #[serde(default)]
    pub instruction_sources: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub root: Option<String>,
    pub operation_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterWorkspaceRootRequest {
    pub workspace_id: String,
    pub root: String,
    pub operation_id: String,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearWorkspaceRootRequest {
    pub workspace_id: String,
    pub operation_id: String,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    pub workspace_id: Option<String>,
    pub title: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionRequest {
    pub session_id: String,
    pub expected_revision: i64,
    pub operation_id: String,
    pub title: Option<String>,
    pub draft: Option<String>,
    pub overrides: Option<Value>,
    pub archived: Option<bool>,
    pub preset_id: Option<String>,
    pub paper_id: Option<String>,
    pub clear_paper: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveSessionRequest {
    pub session_id: String,
    pub expected_revision: i64,
    pub operation_id: String,
    /// `None` files the conversation as unfiled.
    pub workspace_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSessionRequest {
    pub session_id: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWorkspaceRequest {
    pub workspace_id: String,
    pub expected_revision: i64,
    pub operation_id: String,
    pub name: Option<String>,
    pub archived: Option<bool>,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn new_id(prefix: &str) -> WorkbenchResult<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| WorkbenchError::storage("Failed to generate object id", error))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let body = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("{prefix}_{body}"))
}

/// A fresh random suffix for host-originated operation ids.
pub(crate) fn operation_suffix() -> WorkbenchResult<String> {
    new_id("op")
}

fn validate_id(label: &str, value: &str) -> WorkbenchResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(WorkbenchError::invalid(format!("Invalid {label}")));
    }
    Ok(())
}

fn validate_title(title: &str) -> WorkbenchResult<String> {
    let title = title.trim();
    if title.is_empty() {
        return Err(WorkbenchError::invalid("Session title cannot be empty"));
    }
    if title.len() > MAX_SESSION_TITLE_BYTES {
        return Err(WorkbenchError::invalid(format!(
            "Session title cannot exceed {MAX_SESSION_TITLE_BYTES} bytes"
        )));
    }
    Ok(title.to_string())
}

fn validate_workspace_name(name: &str) -> WorkbenchResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(WorkbenchError::invalid("Workspace name cannot be empty"));
    }
    if name.len() > MAX_WORKSPACE_NAME_BYTES {
        return Err(WorkbenchError::invalid(format!(
            "Workspace name cannot exceed {MAX_WORKSPACE_NAME_BYTES} bytes"
        )));
    }
    Ok(name.to_string())
}

fn validate_draft(draft: &str) -> WorkbenchResult<()> {
    if draft.len() > MAX_DRAFT_BYTES {
        return Err(WorkbenchError::invalid(format!(
            "Session draft cannot exceed {MAX_DRAFT_BYTES} bytes"
        )));
    }
    Ok(())
}

fn validate_overrides(overrides: &Value) -> WorkbenchResult<String> {
    if !overrides.is_object() {
        return Err(WorkbenchError::invalid(
            "Session overrides must be a JSON object",
        ));
    }
    let encoded = serde_json::to_string(overrides)
        .map_err(|error| WorkbenchError::invalid(format!("Invalid session overrides: {error}")))?;
    if encoded.len() > MAX_OVERRIDES_BYTES {
        return Err(WorkbenchError::invalid(format!(
            "Session overrides cannot exceed {MAX_OVERRIDES_BYTES} bytes"
        )));
    }
    Ok(encoded)
}

fn secure_directory(path: &Path) -> WorkbenchResult<()> {
    std::fs::create_dir_all(path)
        .map_err(|error| WorkbenchError::storage("Failed to create Workbench storage", error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).map_err(
            |error| WorkbenchError::storage("Failed to secure Workbench storage", error),
        )?;
    }
    Ok(())
}

fn secure_file(path: &Path) -> WorkbenchResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(
            |error| WorkbenchError::storage("Failed to secure Workbench database", error),
        )?;
    }
    Ok(())
}

fn default_root() -> WorkbenchResult<PathBuf> {
    // Disposable GUI qualification stores, deliberately unavailable in release builds.
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("PIPELINE_WORKBENCH_DEV_ROOT") {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err(WorkbenchError::invalid(
                "Development Workspace store must be an absolute path",
            ));
        }
        return Ok(path);
    }
    crate::storage::data_root()
        .map(|root| root.join("workbench"))
        .map_err(|error| WorkbenchError::storage("Cannot locate Workspace data", error))
}

impl Store {
    pub fn open_default() -> WorkbenchResult<Self> {
        let root = default_root()?;
        #[cfg(debug_assertions)]
        if std::env::var_os("PIPELINE_WORKBENCH_DEV_ROOT").is_some() {
            return Self::open_at(&root);
        }
        let codex_home = crate::storage::local_root()
            .map_err(|error| WorkbenchError::storage("Cannot locate Workspace sign-in", error))?
            .join("workbench/codex");
        Self::open_with_codex_home(&root, &codex_home)
    }

    pub fn open_at(root: &Path) -> WorkbenchResult<Self> {
        Self::open_with_codex_home(root, &root.join("codex"))
    }

    fn open_with_codex_home(root: &Path, codex_home: &Path) -> WorkbenchResult<Self> {
        secure_directory(root)?;
        let initialization_lock_path = root.join(".initialize.lock");
        let initialization_lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&initialization_lock_path)
            .map_err(|error| {
                WorkbenchError::storage("Failed to open Workbench initialization lock", error)
            })?;
        secure_file(&initialization_lock_path)?;
        initialization_lock.lock_exclusive().map_err(|error| {
            WorkbenchError::storage("Failed to acquire Workbench initialization lock", error)
        })?;
        for child in ["blobs", "jobs", "context", "backups"] {
            secure_directory(&root.join(child))?;
        }
        secure_directory(codex_home)?;
        let store = Self {
            root: root.to_path_buf(),
            database: root.join("research.sqlite3"),
            backups: root.join("backups"),
            codex_home: codex_home.to_path_buf(),
        };
        store.initialize()?;
        Ok(store)
    }

    pub fn database_path(&self) -> &Path {
        &self.database
    }

    pub(crate) fn codex_home_path(&self) -> PathBuf {
        self.codex_home.clone()
    }

    pub(crate) fn connection(&self) -> WorkbenchResult<Connection> {
        let connection = Connection::open(&self.database)
            .map_err(|error| WorkbenchError::storage("Failed to open Workbench database", error))?;
        configure_timeout(&connection)?;
        let version = schema_version(&connection)?;
        if version > CURRENT_SCHEMA_VERSION {
            return Err(WorkbenchError::newer(version));
        }
        if version != CURRENT_SCHEMA_VERSION {
            return Err(WorkbenchError::storage(
                "Workbench migration did not complete",
                format!("expected schema {CURRENT_SCHEMA_VERSION}, found {version}"),
            ));
        }
        configure_connection(&connection)?;
        Ok(connection)
    }

    pub(crate) fn root_path(&self) -> &Path {
        &self.root
    }

    fn initialize(&self) -> WorkbenchResult<()> {
        self.initialize_with(migrate)
    }

    fn initialize_with<F>(&self, migrator: F) -> WorkbenchResult<()>
    where
        F: FnOnce(&mut Connection, u32) -> WorkbenchResult<()>,
    {
        let existed = self.database.exists();
        let mut connection = Connection::open(&self.database).map_err(|error| {
            WorkbenchError::storage("Failed to create Workbench database", error)
        })?;
        secure_file(&self.database)?;
        configure_timeout(&connection)?;
        let version = schema_version(&connection)?;
        if version > CURRENT_SCHEMA_VERSION {
            return Err(WorkbenchError::newer(version));
        }
        configure_database(&connection)?;
        if version < CURRENT_SCHEMA_VERSION {
            if existed && database_has_objects(&connection)? {
                self.backup(&connection, version)?;
            }
            migrator(&mut connection, version)?;
        }
        Ok(())
    }

    fn backup(&self, source: &Connection, version: u32) -> WorkbenchResult<PathBuf> {
        let suffix = new_id("backup")?;
        let name = format!(
            "research-{}-v{version}-{suffix}.sqlite3",
            chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
        );
        let path = self.backups.join(name);
        let mut destination = Connection::open(&path)
            .map_err(|error| WorkbenchError::storage("Failed to create migration backup", error))?;
        {
            let backup =
                rusqlite::backup::Backup::new(source, &mut destination).map_err(|error| {
                    WorkbenchError::storage("Failed to start migration backup", error)
                })?;
            backup
                .run_to_completion(100, Duration::from_millis(10), None)
                .map_err(|error| {
                    WorkbenchError::storage("Failed to write migration backup", error)
                })?;
        }
        secure_file(&path)?;
        Ok(path)
    }
}

fn configure_timeout(connection: &Connection) -> WorkbenchResult<()> {
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| WorkbenchError::storage("Failed to configure database timeout", error))
}

fn configure_database(connection: &Connection) -> WorkbenchResult<()> {
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;",
        )
        .map_err(|error| WorkbenchError::storage("Failed to configure Workbench database", error))
}

fn configure_connection(connection: &Connection) -> WorkbenchResult<()> {
    connection
        .execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = FULL;")
        .map_err(|error| WorkbenchError::storage("Failed to configure Workbench connection", error))
}

fn schema_version(connection: &Connection) -> WorkbenchResult<u32> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| WorkbenchError::storage("Failed to read Workbench schema version", error))
}

fn database_has_objects(connection: &Connection) -> WorkbenchResult<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name NOT LIKE 'sqlite_%')",
            [],
            |row| row.get(0),
        )
        .map_err(|error| WorkbenchError::storage("Failed to inspect Workbench database", error))
}

pub(super) fn migrate(connection: &mut Connection, from: u32) -> WorkbenchResult<()> {
    connection
        .pragma_update(None, "foreign_keys", false)
        .map_err(|error| {
            WorkbenchError::storage("Failed to suspend migration constraints", error)
        })?;
    let migration = (|| {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                WorkbenchError::storage("Failed to start Workbench migration", error)
            })?;
        if from < 1 {
            transaction
                .execute_batch(include_str!("migrations/001_initial.sql"))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 1", error)
                })?;
        }
        if from < 2 {
            transaction
                .execute_batch(include_str!("migrations/002_independent_workspaces.sql"))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 2", error)
                })?;
        }
        if from < 3 {
            transaction
                .execute_batch(include_str!("migrations/003_research_harness.sql"))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 3", error)
                })?;
        }
        if from < 4 {
            transaction
                .execute_batch(include_str!(
                    "migrations/004_native_instruction_sources.sql"
                ))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 4", error)
                })?;
        }
        if from < 5 {
            transaction
                .execute_batch(include_str!("migrations/005_release_readiness.sql"))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 5", error)
                })?;
        }
        if from < 6 {
            transaction
                .execute_batch(include_str!("migrations/006_project_surface.sql"))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 6", error)
                })?;
        }
        if from < 7 {
            transaction
                .execute_batch(include_str!("migrations/007_research_studio.sql"))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 7", error)
                })?;
        }
        if from < 8 {
            transaction
                .execute_batch(include_str!("migrations/008_theory_notes.sql"))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 8", error)
                })?;
        }
        if from < 9 {
            transaction
                .execute_batch(include_str!("migrations/009_project_exchange.sql"))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 9", error)
                })?;
        }
        if from < 10 {
            transaction
                .execute_batch(include_str!("migrations/010_preferences.sql"))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 10", error)
                })?;
        }
        if from < 11 {
            transaction
                .execute_batch(include_str!("migrations/011_research_desk.sql"))
                .map_err(|error| {
                    WorkbenchError::storage("Failed to apply Workbench migration 11", error)
                })?;
        }
        if from < 12 {
            transaction
                .execute_batch(include_str!("migrations/012_task_exchanges.sql"))
                .map_err(|e| WorkbenchError::storage("Failed to migrate task exchanges", e))?;
        }
        if from < 13 {
            transaction
                .execute_batch(include_str!("migrations/013_research_programs.sql"))
                .map_err(|e| WorkbenchError::storage("Failed to migrate research programs", e))?;
        }
        if from < 14 {
            transaction
                .execute_batch(include_str!("migrations/014_agent_base_prompts.sql"))
                .map_err(|e| WorkbenchError::storage("Failed to migrate agent base prompts", e))?;
        }
        if from < 15 {
            transaction
                .execute_batch(include_str!("migrations/015_self_discovery.sql"))
                .map_err(|e| {
                    WorkbenchError::storage("Failed to migrate self-discovery roles", e)
                })?;
        }
        transaction
            .pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
            .map_err(|error| {
                WorkbenchError::storage("Failed to record Workbench schema version", error)
            })?;
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit Workbench migration", error))
    })();
    let constraints = connection
        .pragma_update(None, "foreign_keys", true)
        .map_err(|error| WorkbenchError::storage("Failed to restore migration constraints", error));
    migration?;
    constraints?;
    let violation: Option<String> = connection
        .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
        .optional()
        .map_err(|error| {
            WorkbenchError::storage("Failed to validate migrated references", error)
        })?;
    if let Some(table) = violation {
        return Err(WorkbenchError::storage(
            "Migrated store contains an invalid reference",
            table,
        ));
    }
    Ok(())
}

fn require_revision(current: i64, expected: i64, label: &str) -> WorkbenchResult<()> {
    if current != expected {
        return Err(WorkbenchError::conflict(format!(
            "{label} revision changed (expected {expected}, found {current})"
        )));
    }
    Ok(())
}

fn resolve_optional_root(
    root: Option<&str>,
    store_root: &Path,
) -> WorkbenchResult<(Option<String>, Option<String>)> {
    root.map(|root| {
        let canonical = canonical_workspace_root(Path::new(root), store_root)?;
        let identity = root_identity(&canonical)?;
        Ok((
            Some(canonical.to_string_lossy().into_owned()),
            Some(identity),
        ))
    })
    .unwrap_or(Ok((None, None)))
}

pub(crate) fn canonical_workspace_root(root: &Path, store_root: &Path) -> WorkbenchResult<PathBuf> {
    if !root.is_absolute() {
        return Err(WorkbenchError::invalid("Workspace root must be absolute"));
    }
    let canonical = root.canonicalize().map_err(|error| {
        WorkbenchError::invalid(format!("Workspace root is unavailable: {error}"))
    })?;
    if !canonical.is_dir() {
        return Err(WorkbenchError::invalid(
            "Workspace root must be a directory",
        ));
    }
    if canonical.parent().is_none() {
        return Err(WorkbenchError::invalid(
            "Filesystem root cannot be a workspace",
        ));
    }
    let canonical_store = store_root.canonicalize().map_err(|error| {
        WorkbenchError::storage("Failed to resolve Workbench storage root", error)
    })?;
    if canonical.starts_with(&canonical_store) || canonical_store.starts_with(&canonical) {
        return Err(WorkbenchError::invalid(
            "Workspace root cannot overlap Workbench-owned storage",
        ));
    }
    // A custom research directory no longer encloses the local credential
    // store. Keep that store protected when registering project folders.
    if let Ok(local) =
        crate::storage::local_root().and_then(|p| p.canonicalize().map_err(|e| e.to_string()))
    {
        if canonical.starts_with(&local) || local.starts_with(&canonical) {
            return Err(WorkbenchError::invalid(
                "Workspace root cannot overlap Pipeline's local settings and credentials",
            ));
        }
    }
    if let Some(home) = dirs::home_dir().and_then(|path| path.canonicalize().ok()) {
        if canonical == home || home.starts_with(&canonical) {
            return Err(WorkbenchError::invalid(
                "Home directory or one of its ancestors is too broad for a workspace",
            ));
        }
    }
    Ok(canonical)
}

pub(crate) fn root_identity(root: &Path) -> WorkbenchResult<String> {
    let metadata = std::fs::metadata(root).map_err(|error| {
        WorkbenchError::invalid(format!("Cannot inspect workspace root: {error}"))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        Ok(format!("unix:{}:{}", metadata.dev(), metadata.ino()))
    }
    #[cfg(not(unix))]
    {
        let mut digest = Sha256::new();
        digest.update(root.to_string_lossy().as_bytes());
        Ok(format!("path-sha256:{:x}", digest.finalize()))
    }
}

fn parse_json(value: String, column: usize) -> rusqlite::Result<Value> {
    serde_json::from_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn read_workspace_row(row: &Row<'_>) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: row.get(0)?,
        name: row.get(1)?,
        root: row.get(2)?,
        root_identity: row.get(3)?,
        settings_revision: row.get(4)?,
        revision: row.get(5)?,
        archived_at: row.get(6)?,
        missing_root_at: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn load_workspace(connection: &Connection, workspace_id: &str) -> WorkbenchResult<Workspace> {
    connection
        .query_row(
            "SELECT id, name, root, root_identity, settings_revision, revision, archived_at, missing_root_at, created_at, updated_at FROM workspaces WHERE id = ?1",
            [workspace_id],
            read_workspace_row,
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to load workspace", error))?
        .ok_or_else(|| WorkbenchError::invalid("Workbench workspace does not exist"))
}

fn read_session_row(row: &Row<'_>) -> rusqlite::Result<WorkbenchSession> {
    let overrides: String = row.get(5)?;
    Ok(WorkbenchSession {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        paper_id: row.get(2)?,
        title: row.get(3)?,
        preset_id: row.get(4)?,
        overrides: parse_json(overrides, 5)?,
        draft: row.get(6)?,
        revision: row.get(7)?,
        archived_at: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn validate_provider_id(label: &str, value: &str) -> WorkbenchResult<()> {
    if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        return Err(WorkbenchError::invalid(format!("Invalid {label}")));
    }
    Ok(())
}

fn read_binding_row(row: &Row<'_>) -> rusqlite::Result<SessionBinding> {
    let instruction_sources: String = row.get(9)?;
    Ok(SessionBinding {
        id: row.get(0)?,
        session_id: row.get(1)?,
        runtime_namespace: row.get(2)?,
        provider_thread_id: row.get(3)?,
        incarnation: row.get(4)?,
        created_at: row.get(5)?,
        retired_at: row.get(6)?,
        retirement_reason: row.get(7)?,
        harness_fingerprint: row.get(8)?,
        instruction_sources: serde_json::from_str(&instruction_sources).unwrap_or_default(),
    })
}

fn load_binding(connection: &Connection, binding_id: &str) -> WorkbenchResult<SessionBinding> {
    connection
        .query_row(
            "SELECT id, session_id, runtime_namespace, provider_thread_id, incarnation, created_at, retired_at, retirement_reason, harness_fingerprint, instruction_sources_json FROM session_bindings WHERE id = ?1",
            [binding_id],
            read_binding_row,
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to load session binding", error))?
        .ok_or_else(|| WorkbenchError::invalid("Workbench session binding does not exist"))
}

fn load_active_binding(
    connection: &Connection,
    session_id: &str,
) -> WorkbenchResult<Option<SessionBinding>> {
    connection
        .query_row(
            "SELECT id, session_id, runtime_namespace, provider_thread_id, incarnation, created_at, retired_at, retirement_reason, harness_fingerprint, instruction_sources_json FROM session_bindings WHERE session_id = ?1 AND retired_at IS NULL ORDER BY incarnation DESC LIMIT 1",
            [session_id],
            read_binding_row,
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to load active session binding", error))
}

fn required_json_string<'a>(
    value: &'a Value,
    key: &str,
    context: &str,
) -> WorkbenchResult<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| WorkbenchError::invalid(format!("{context} omitted string field {key}")))
}

fn encode_projected_payload(value: &Value, context: &str) -> WorkbenchResult<String> {
    let encoded =
        serde_json::to_string(value).map_err(|error| WorkbenchError::storage(context, error))?;
    if encoded.len() > MAX_PROJECTED_PAYLOAD_BYTES {
        return Err(WorkbenchError::invalid(format!(
            "{context} exceeded {MAX_PROJECTED_PAYLOAD_BYTES} bytes"
        )));
    }
    Ok(encoded)
}

fn ensure_projected_turn(
    connection: &Connection,
    binding_id: &str,
    provider_turn_id: &str,
    timestamp: &str,
) -> WorkbenchResult<String> {
    if let Some(id) = connection
        .query_row(
            "SELECT id FROM turns WHERE binding_id = ?1 AND provider_turn_id = ?2",
            params![binding_id, provider_turn_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to inspect projected turn", error))?
    {
        return Ok(id);
    }
    let id = new_id("turn")?;
    let provisional_submission_id = format!("provider:{provider_turn_id}");
    connection
        .execute(
            "INSERT INTO turns (id, binding_id, client_submission_id, provider_turn_id, state, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'inProgress', ?5, ?5)",
            params![id, binding_id, provisional_submission_id, provider_turn_id, timestamp],
        )
        .map_err(|error| map_constraint("Failed to create projected turn", error))?;
    Ok(id)
}

fn upsert_projected_turn(
    connection: &Connection,
    binding_id: &str,
    provider_turn_id: &str,
    turn: &Value,
    timestamp: &str,
    terminal: bool,
) -> WorkbenchResult<()> {
    validate_provider_id("provider turn id", provider_turn_id)?;
    encode_projected_payload(turn, "Projected turn")?;
    let id = ensure_projected_turn(connection, binding_id, provider_turn_id, timestamp)?;
    let state = required_json_string(turn, "status", "projected turn")?;
    let usage = turn
        .get("usage")
        .filter(|value| !value.is_null())
        .map(|value| encode_projected_payload(value, "Projected turn usage"))
        .transpose()?;
    let error = turn
        .get("error")
        .filter(|value| !value.is_null())
        .map(|value| encode_projected_payload(value, "Projected turn error"))
        .transpose()?;
    connection
        .execute(
            "UPDATE turns SET state = ?1, usage_json = ?2, error_json = ?3, updated_at = ?4, terminal_at = CASE WHEN ?5 THEN ?4 ELSE terminal_at END WHERE id = ?6",
            params![state, usage, error, timestamp, terminal, id],
        )
        .map_err(|error| WorkbenchError::storage("Failed to update projected turn", error))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn upsert_projected_item(
    connection: &Connection,
    binding_id: &str,
    provider_turn_id: &str,
    provider_item_id: &str,
    item_kind: &str,
    item: &Value,
    is_final: bool,
    timestamp: &str,
) -> WorkbenchResult<()> {
    validate_provider_id("provider item id", provider_item_id)?;
    let payload = encode_projected_payload(item, "Projected transcript item")?;
    let turn_id = ensure_projected_turn(connection, binding_id, provider_turn_id, timestamp)?;
    let id = new_id("item")?;
    connection
        .execute(
            "INSERT INTO transcript_items (id, binding_id, turn_id, provider_item_id, item_kind, payload_json, is_final, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8) ON CONFLICT(binding_id, provider_item_id) DO UPDATE SET turn_id = excluded.turn_id, item_kind = excluded.item_kind, payload_json = excluded.payload_json, is_final = MAX(transcript_items.is_final, excluded.is_final), updated_at = excluded.updated_at",
            params![id, binding_id, turn_id, provider_item_id, item_kind, payload, is_final, timestamp],
        )
        .map_err(|error| WorkbenchError::storage("Failed to project transcript item", error))?;
    Ok(())
}

fn change_sequence(connection: &Connection, operation_id: &str) -> WorkbenchResult<Option<i64>> {
    connection
        .query_row(
            "SELECT sequence FROM change_log WHERE operation_id = ?1",
            [operation_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to inspect projected event", error))
}

fn load_session(connection: &Connection, session_id: &str) -> WorkbenchResult<WorkbenchSession> {
    connection
        .query_row(
            "SELECT id, workspace_id, paper_id, title, preset_id, overrides_json, draft, revision, archived_at, created_at, updated_at FROM sessions WHERE id = ?1",
            [session_id],
            read_session_row,
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to load session", error))?
        .ok_or_else(|| WorkbenchError::invalid("Workbench session does not exist"))
}

fn ensure_operation_unused(connection: &Connection, operation_id: &str) -> WorkbenchResult<()> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM change_log WHERE operation_id = ?1)",
            [operation_id],
            |row| row.get(0),
        )
        .map_err(|error| WorkbenchError::storage("Failed to inspect operation id", error))?;
    if exists {
        return Err(WorkbenchError::conflict(
            "This client operation id has already been committed",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn insert_change(
    connection: &Connection,
    operation_id: &str,
    entity_type: &str,
    entity_id: &str,
    action: &str,
    workspace_id: Option<&str>,
    session_id: Option<&str>,
    details: &Value,
    created_at: &str,
) -> WorkbenchResult<i64> {
    let details = serde_json::to_string(details)
        .map_err(|error| WorkbenchError::storage("Failed to encode change record", error))?;
    connection
        .execute(
            "INSERT INTO change_log (operation_id, entity_type, entity_id, action, workspace_id, session_id, details_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![operation_id, entity_type, entity_id, action, workspace_id, session_id, details, created_at],
        )
        .map_err(|error| map_constraint("Failed to append change record", error))?;
    Ok(connection.last_insert_rowid())
}

fn current_sequence(connection: &Connection) -> WorkbenchResult<i64> {
    connection
        .query_row(
            "SELECT COALESCE(MAX(sequence), 0) FROM change_log",
            [],
            |row| row.get(0),
        )
        .map_err(|error| WorkbenchError::storage("Failed to read Workbench sequence", error))
}

fn map_constraint(context: &str, error: rusqlite::Error) -> WorkbenchError {
    if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
        WorkbenchError::conflict(format!(
            "{context}: the requested record conflicts with existing data"
        ))
    } else {
        WorkbenchError::storage(context, error)
    }
}

#[cfg(test)]
#[path = "store/tests.rs"]
mod tests;

mod runtime;
mod sessions;
mod views;
mod workspaces;
