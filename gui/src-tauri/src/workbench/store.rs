//! Versioned local persistence for Workbench-owned research state.

use fs2::FileExt as _;
use rusqlite::{params, Connection, OptionalExtension as _, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[cfg(not(unix))]
use sha2::{Digest as _, Sha256};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const CURRENT_SCHEMA_VERSION: u32 = 19;
const MAX_WORKSPACE_NAME_BYTES: usize = 300;
const MAX_SESSION_TITLE_BYTES: usize = 300;
const MAX_DRAFT_BYTES: usize = 1_000_000;
const MAX_OVERRIDES_BYTES: usize = 256_000;
const MAX_SESSIONS_PER_LIST: usize = 500;
const MAX_WORKSPACES_PER_LIST: usize = 500;
const MAX_PROJECTED_PAYLOAD_BYTES: usize = 8 * 1024 * 1024;

mod journal;
mod projection;
mod records;
mod schema;

use journal::{change_sequence, current_sequence, ensure_operation_unused, insert_change};
use projection::{required_json_string, upsert_projected_item, upsert_projected_turn};
use records::{
    load_active_binding, load_binding, load_session, load_workspace, read_session_row,
    read_workspace_row,
};
pub use records::{
    ClearWorkspaceRootRequest, ConversationSnapshot, ConversationTurn, CreateSessionRequest,
    CreateWorkspaceRequest, DeleteSessionRequest, MoveSessionRequest, Mutation, ReconcileResult,
    RegisterWorkspaceRootRequest, SessionBinding, SessionCursor, SessionList, SessionSnapshot,
    TranscriptItem, UpdateSessionRequest, UpdateWorkspaceRequest, WorkbenchSession, Workspace,
    WorkspaceList,
};
use schema::{
    configure_connection, configure_database, configure_timeout, database_has_objects,
    schema_version,
};
pub(super) use schema::{migrate, reference_schema};

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
    if let Some(runtime) = overrides.get("runtime") {
        if !matches!(
            runtime.as_str(),
            Some("codex" | "claude" | "openai-api" | "anthropic-api" | "google-api" | "local")
        ) {
            return Err(WorkbenchError::invalid(
                "Choose an available conversation provider",
            ));
        }
    }
    super::capabilities::ConversationCapabilities::resolve(overrides.as_object().unwrap(), false)?;
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

/// Held open between operations. When its last connection closes, SQLite folds
/// the write-ahead log into the database and deletes it, which would otherwise
/// happen after nearly every operation. Commits stay exactly as durable.
static IDLE_CONNECTION: std::sync::Mutex<Option<Connection>> = std::sync::Mutex::new(None);

impl Store {
    /// The first open creates, secures, and migrates the store. Later opens only
    /// confirm the data folder: its location is fixed for the process, and
    /// `connection()` checks the schema version every time.
    pub fn open_default() -> WorkbenchResult<Self> {
        static OPENED: std::sync::OnceLock<Store> = std::sync::OnceLock::new();
        let root = default_root()?;
        if let Some(store) = OPENED.get().filter(|store| store.root == root) {
            return Ok(store.clone());
        }
        let store = Self::open_default_at(&root)?;
        if OPENED.set(store.clone()).is_ok() {
            *IDLE_CONNECTION
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = store.connection().ok();
        }
        Ok(store)
    }

    /// Lets the last close fold the log into the database, so `research.sqlite3`
    /// is complete on its own whenever the app is not running.
    pub fn close_idle_connection() {
        IDLE_CONNECTION
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
    }

    fn open_default_at(root: &Path) -> WorkbenchResult<Self> {
        #[cfg(debug_assertions)]
        if std::env::var_os("PIPELINE_WORKBENCH_DEV_ROOT").is_some() {
            return Self::open_at(root);
        }
        let codex_home = crate::storage::local_root()
            .map_err(|error| WorkbenchError::storage("Cannot locate Workspace sign-in", error))?
            .join("workbench/codex");
        Self::open_with_codex_home(root, &codex_home)
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
        self.initialize_with(migrate)?;
        // A store that can't be repaired must still open. Its references stay
        // as they were, and Disk use refuses cleanup while any lies elsewhere.
        if let Err(error) = self.rebase_references() {
            eprintln!("Could not update stored file locations: {}", error.message);
        }
        Ok(())
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

fn validate_provider_id(label: &str, value: &str) -> WorkbenchResult<()> {
    if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        return Err(WorkbenchError::invalid(format!("Invalid {label}")));
    }
    Ok(())
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

mod roots;
pub(crate) use roots::{canonical_workspace_root, root_identity};

mod relocation;
pub(crate) use relocation::{
    is_left_out_reference, is_local_reference, left_out_file, left_out_reference,
    outside_folder_error, portable_reference, record_location, require_available,
    resolve_reference, REFERENCE_COLUMNS,
};

mod history;
pub use history::{TranscriptCursor, TranscriptPage};
