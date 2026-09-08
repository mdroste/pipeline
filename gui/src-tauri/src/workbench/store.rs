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

pub const CURRENT_SCHEMA_VERSION: u32 = 13;
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

    pub fn create_workspace(
        &self,
        request: CreateWorkspaceRequest,
    ) -> WorkbenchResult<Mutation<Workspace>> {
        validate_id("operation id", &request.operation_id)?;
        let name = validate_workspace_name(&request.name)?;
        let (root, root_identity) = resolve_optional_root(request.root.as_deref(), &self.root)?;
        let id = new_id("workspace")?;
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                WorkbenchError::storage("Failed to start workspace creation", error)
            })?;
        ensure_operation_unused(&transaction, &request.operation_id)?;
        transaction
            .execute(
                "INSERT INTO workspaces (id, name, root, root_identity, settings_revision, revision, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 0, 1, ?5, ?5)",
                params![id, name, root, root_identity, timestamp],
            )
            .map_err(|error| map_constraint("Failed to create workspace", error))?;
        let workspace = load_workspace(&transaction, &id)?;
        let sequence = insert_change(
            &transaction,
            &request.operation_id,
            "workspace",
            &id,
            "workspace_created",
            Some(&id),
            None,
            &json!({ "revision": 1, "hasRoot": workspace.root.is_some() }),
            &timestamp,
        )?;
        transaction.commit().map_err(|error| {
            WorkbenchError::storage("Failed to commit workspace creation", error)
        })?;
        Ok(Mutation {
            record: workspace,
            sequence,
        })
    }

    pub fn register_workspace_root(
        &self,
        request: RegisterWorkspaceRootRequest,
    ) -> WorkbenchResult<Mutation<Workspace>> {
        validate_id("workspace id", &request.workspace_id)?;
        validate_id("operation id", &request.operation_id)?;
        let canonical = canonical_workspace_root(Path::new(&request.root), &self.root)?;
        let root = canonical.to_string_lossy().into_owned();
        let identity = root_identity(&canonical)?;
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WorkbenchError::storage("Failed to start root registration", error))?;
        ensure_operation_unused(&transaction, &request.operation_id)?;
        let current = load_workspace(&transaction, &request.workspace_id)?;
        require_revision(current.revision, request.expected_revision, "Workspace")?;
        let next_revision = current.revision + 1;
        transaction
            .execute(
                "UPDATE workspaces SET root = ?1, root_identity = ?2, missing_root_at = NULL, revision = ?3, updated_at = ?4 WHERE id = ?5",
                params![root, identity, next_revision, timestamp, request.workspace_id],
            )
            .map_err(|error| map_constraint("Failed to register workspace root", error))?;
        let workspace = load_workspace(&transaction, &request.workspace_id)?;
        let sequence = insert_change(
            &transaction,
            &request.operation_id,
            "workspace",
            &request.workspace_id,
            "workspace_root_registered",
            Some(&request.workspace_id),
            None,
            &json!({ "revision": next_revision, "rootIdentity": workspace.root_identity }),
            &timestamp,
        )?;
        transaction.commit().map_err(|error| {
            WorkbenchError::storage("Failed to commit root registration", error)
        })?;
        Ok(Mutation {
            record: workspace,
            sequence,
        })
    }

    pub fn clear_workspace_root(
        &self,
        request: ClearWorkspaceRootRequest,
    ) -> WorkbenchResult<Mutation<Workspace>> {
        validate_id("workspace id", &request.workspace_id)?;
        validate_id("operation id", &request.operation_id)?;
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WorkbenchError::storage("Failed to start root removal", error))?;
        ensure_operation_unused(&transaction, &request.operation_id)?;
        let current = load_workspace(&transaction, &request.workspace_id)?;
        require_revision(current.revision, request.expected_revision, "Workspace")?;
        let next_revision = current.revision + 1;
        transaction
            .execute(
                "UPDATE workspaces SET root = NULL, root_identity = NULL, missing_root_at = NULL, revision = ?1, updated_at = ?2 WHERE id = ?3",
                params![next_revision, timestamp, request.workspace_id],
            )
            .map_err(|error| WorkbenchError::storage("Failed to remove workspace root", error))?;
        let workspace = load_workspace(&transaction, &request.workspace_id)?;
        let sequence = insert_change(
            &transaction,
            &request.operation_id,
            "workspace",
            &request.workspace_id,
            "workspace_root_cleared",
            Some(&request.workspace_id),
            None,
            &json!({ "revision": next_revision }),
            &timestamp,
        )?;
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit root removal", error))?;
        Ok(Mutation {
            record: workspace,
            sequence,
        })
    }

    pub fn workspace(&self, workspace_id: &str) -> WorkbenchResult<Workspace> {
        validate_id("workspace id", workspace_id)?;
        load_workspace(&self.connection()?, workspace_id)
    }

    pub fn list_workspaces(&self, include_archived: bool) -> WorkbenchResult<WorkspaceList> {
        let connection = self.connection()?;
        let filter = if include_archived {
            ""
        } else {
            " WHERE archived_at IS NULL"
        };
        let sql = format!(
            "SELECT id, name, root, root_identity, settings_revision, revision, archived_at, missing_root_at, created_at, updated_at FROM workspaces{filter} ORDER BY updated_at DESC, id LIMIT ?1"
        );
        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| WorkbenchError::storage("Failed to prepare workspace list", error))?;
        let workspaces = statement
            .query_map([(MAX_WORKSPACES_PER_LIST + 1) as i64], read_workspace_row)
            .map_err(|error| WorkbenchError::storage("Failed to list workspaces", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| WorkbenchError::storage("Failed to decode workspaces", error))?;
        if workspaces.len() > MAX_WORKSPACES_PER_LIST {
            return Err(WorkbenchError::storage(
                "Workspace list exceeds its safety limit",
                MAX_WORKSPACES_PER_LIST,
            ));
        }
        Ok(WorkspaceList {
            workspaces,
            sequence: current_sequence(&connection)?,
        })
    }

    pub fn update_workspace(
        &self,
        request: UpdateWorkspaceRequest,
    ) -> WorkbenchResult<Mutation<Workspace>> {
        validate_id("workspace id", &request.workspace_id)?;
        validate_id("operation id", &request.operation_id)?;
        let name = request
            .name
            .as_deref()
            .map(validate_workspace_name)
            .transpose()?;
        if name.is_none() && request.archived.is_none() {
            return Err(WorkbenchError::invalid(
                "Workspace update must change its name or archived state",
            ));
        }
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WorkbenchError::storage("Failed to start workspace update", error))?;
        ensure_operation_unused(&transaction, &request.operation_id)?;
        let current = load_workspace(&transaction, &request.workspace_id)?;
        require_revision(current.revision, request.expected_revision, "Workspace")?;
        let archived_at = match request.archived {
            Some(true) => current.archived_at.or_else(|| Some(timestamp.clone())),
            Some(false) => None,
            None => current.archived_at,
        };
        let next_revision = current.revision + 1;
        transaction
            .execute(
                "UPDATE workspaces SET name = ?1, archived_at = ?2, revision = ?3, updated_at = ?4 WHERE id = ?5",
                params![name.unwrap_or(current.name), archived_at, next_revision, timestamp, request.workspace_id],
            )
            .map_err(|error| WorkbenchError::storage("Failed to update workspace", error))?;
        let workspace = load_workspace(&transaction, &request.workspace_id)?;
        let sequence = insert_change(
            &transaction,
            &request.operation_id,
            "workspace",
            &request.workspace_id,
            "workspace_updated",
            Some(&request.workspace_id),
            None,
            &json!({ "revision": next_revision, "archived": workspace.archived_at.is_some() }),
            &timestamp,
        )?;
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit workspace update", error))?;
        Ok(Mutation {
            record: workspace,
            sequence,
        })
    }

    pub fn create_session(
        &self,
        request: CreateSessionRequest,
    ) -> WorkbenchResult<Mutation<WorkbenchSession>> {
        if let Some(workspace_id) = &request.workspace_id {
            validate_id("workspace id", workspace_id)?;
        }
        validate_id("operation id", &request.operation_id)?;
        let title = validate_title(&request.title)?;
        let id = new_id("session")?;
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WorkbenchError::storage("Failed to start session creation", error))?;
        ensure_operation_unused(&transaction, &request.operation_id)?;
        if let Some(workspace_id) = &request.workspace_id {
            let workspace = load_workspace(&transaction, workspace_id)?;
            if workspace.archived_at.is_some() {
                return Err(WorkbenchError::invalid(
                    "Cannot create a session for an archived workspace",
                ));
            }
        }
        transaction
            .execute(
                "INSERT INTO sessions (id, workspace_id, title, overrides_json, draft, revision, created_at, updated_at) VALUES (?1, ?2, ?3, '{}', '', 1, ?4, ?4)",
                params![id, request.workspace_id, title, timestamp],
            )
            .map_err(|error| WorkbenchError::storage("Failed to create session", error))?;
        let session = load_session(&transaction, &id)?;
        let sequence = insert_change(
            &transaction,
            &request.operation_id,
            "session",
            &id,
            "session_created",
            request.workspace_id.as_deref(),
            Some(&id),
            &json!({ "revision": 1 }),
            &timestamp,
        )?;
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit session creation", error))?;
        Ok(Mutation {
            record: session,
            sequence,
        })
    }

    pub fn update_session(
        &self,
        request: UpdateSessionRequest,
    ) -> WorkbenchResult<Mutation<WorkbenchSession>> {
        validate_id("session id", &request.session_id)?;
        validate_id("operation id", &request.operation_id)?;
        let title = request.title.as_deref().map(validate_title).transpose()?;
        if let Some(draft) = &request.draft {
            validate_draft(draft)?;
        }
        let overrides = request
            .overrides
            .as_ref()
            .map(validate_overrides)
            .transpose()?;
        if let Some(preset_id) = request.preset_id.as_deref() {
            validate_id("preset id", preset_id)?;
        }
        if let Some(paper_id) = request.paper_id.as_deref() {
            validate_id("paper id", paper_id)?;
        }
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WorkbenchError::storage("Failed to start session update", error))?;
        ensure_operation_unused(&transaction, &request.operation_id)?;
        let current = load_session(&transaction, &request.session_id)?;
        if current.revision != request.expected_revision {
            return Err(WorkbenchError::conflict(format!(
                "Session revision changed (expected {}, found {})",
                request.expected_revision, current.revision
            )));
        }
        let next_revision = current.revision + 1;
        let archived_at = match request.archived {
            Some(true) => current.archived_at.or_else(|| Some(timestamp.clone())),
            Some(false) => None,
            None => current.archived_at,
        };
        let paper_id = if request.clear_paper == Some(true) {
            None
        } else {
            request.paper_id.or(current.paper_id)
        };
        transaction
            .execute(
                "UPDATE sessions SET title = ?1, overrides_json = ?2, draft = ?3, revision = ?4, archived_at = ?5, updated_at = ?6, preset_id = ?7, paper_id = ?8 WHERE id = ?9",
                params![
                    title.unwrap_or(current.title),
                    overrides.unwrap_or_else(|| current.overrides.to_string()),
                    request.draft.unwrap_or(current.draft),
                    next_revision,
                    archived_at,
                    timestamp,
                    request.preset_id.or(current.preset_id),
                    paper_id,
                    request.session_id
                ],
            )
            .map_err(|error| WorkbenchError::storage("Failed to update session", error))?;
        let session = load_session(&transaction, &request.session_id)?;
        let sequence = insert_change(
            &transaction,
            &request.operation_id,
            "session",
            &request.session_id,
            "session_updated",
            session.workspace_id.as_deref(),
            Some(&request.session_id),
            &json!({ "revision": next_revision, "archived": session.archived_at.is_some() }),
            &timestamp,
        )?;
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit session update", error))?;
        Ok(Mutation {
            record: session,
            sequence,
        })
    }

    pub fn list_sessions(
        &self,
        workspace_id: Option<&str>,
        include_archived: bool,
    ) -> WorkbenchResult<SessionList> {
        if let Some(workspace_id) = workspace_id {
            validate_id("workspace id", workspace_id)?;
        }
        let connection = self.connection()?;
        let sql = if include_archived {
            "SELECT id, workspace_id, paper_id, title, preset_id, overrides_json, draft, revision, archived_at, created_at, updated_at FROM sessions WHERE ((?1 IS NULL AND workspace_id IS NULL) OR workspace_id = ?1) ORDER BY updated_at DESC, id LIMIT ?2"
        } else {
            "SELECT id, workspace_id, paper_id, title, preset_id, overrides_json, draft, revision, archived_at, created_at, updated_at FROM sessions WHERE ((?1 IS NULL AND workspace_id IS NULL) OR workspace_id = ?1) AND archived_at IS NULL ORDER BY updated_at DESC, id LIMIT ?2"
        };
        let mut statement = connection
            .prepare(sql)
            .map_err(|error| WorkbenchError::storage("Failed to prepare session list", error))?;
        let sessions = statement
            .query_map(
                params![workspace_id, (MAX_SESSIONS_PER_LIST + 1) as i64],
                read_session_row,
            )
            .map_err(|error| WorkbenchError::storage("Failed to list sessions", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| WorkbenchError::storage("Failed to decode sessions", error))?;
        if sessions.len() > MAX_SESSIONS_PER_LIST {
            return Err(WorkbenchError::storage(
                "Session list exceeds its safety limit",
                MAX_SESSIONS_PER_LIST,
            ));
        }
        Ok(SessionList {
            sessions,
            sequence: current_sequence(&connection)?,
        })
    }

    /// Files a conversation under another Workspace, or unfiles it.
    ///
    /// The effective harness fingerprints the Workspace, so the next turn
    /// starts a successor native thread through the ordinary handoff path.
    /// Records that belong to the current Workspace (recipe runs, tool
    /// receipts, executions, curated context) pin the conversation there.
    pub fn move_session(
        &self,
        request: MoveSessionRequest,
    ) -> WorkbenchResult<Mutation<WorkbenchSession>> {
        validate_id("session id", &request.session_id)?;
        validate_id("operation id", &request.operation_id)?;
        if let Some(workspace_id) = &request.workspace_id {
            validate_id("workspace id", workspace_id)?;
        }
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WorkbenchError::storage("Failed to start session move", error))?;
        ensure_operation_unused(&transaction, &request.operation_id)?;
        let current = load_session(&transaction, &request.session_id)?;
        require_revision(current.revision, request.expected_revision, "Session")?;
        if current.workspace_id == request.workspace_id {
            return Err(WorkbenchError::invalid(
                "The conversation is already filed there",
            ));
        }
        if current.overrides.get("projectCheckpointId").is_some() {
            return Err(WorkbenchError::invalid(
                "Task conversations stay with the project that owns their task copy",
            ));
        }
        if let Some(workspace_id) = &request.workspace_id {
            let workspace = load_workspace(&transaction, workspace_id)?;
            if workspace.archived_at.is_some() {
                return Err(WorkbenchError::invalid(
                    "Cannot move a conversation into an archived workspace",
                ));
            }
        }
        let bound_records: i64 = transaction
            .query_row(
                "SELECT (SELECT COUNT(*) FROM recipe_runs WHERE session_id = ?1) + (SELECT COUNT(*) FROM tool_receipts WHERE session_id = ?1) + (SELECT COUNT(*) FROM research_executions WHERE session_id = ?1) + (SELECT COUNT(*) FROM session_context_items WHERE session_id = ?1)",
                [&request.session_id],
                |row| row.get(0),
            )
            .map_err(|error| WorkbenchError::storage("Failed to inspect conversation records", error))?;
        if bound_records > 0 {
            return Err(WorkbenchError::invalid(
                "This conversation has research records in its current Workspace and cannot be moved",
            ));
        }
        let next_revision = current.revision + 1;
        transaction
            .execute(
                "UPDATE sessions SET workspace_id = ?1, paper_id = NULL, revision = ?2, updated_at = ?3 WHERE id = ?4",
                params![request.workspace_id, next_revision, timestamp, request.session_id],
            )
            .map_err(|error| map_constraint("Failed to move session", error))?;
        let session = load_session(&transaction, &request.session_id)?;
        let sequence = insert_change(
            &transaction,
            &request.operation_id,
            "session",
            &request.session_id,
            "session_moved",
            session.workspace_id.as_deref(),
            Some(&request.session_id),
            &json!({
                "revision": next_revision,
                "fromWorkspaceId": current.workspace_id,
                "toWorkspaceId": session.workspace_id,
                "paperCleared": current.paper_id.is_some()
            }),
            &timestamp,
        )?;
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit session move", error))?;
        Ok(Mutation {
            record: session,
            sequence,
        })
    }

    /// Permanently removes a conversation with its native bindings, turns,
    /// transcript, snapshots, receipts, and recipe runs. Executions and Review
    /// handoffs keep their records with the conversation reference cleared.
    pub fn delete_session(&self, request: DeleteSessionRequest) -> WorkbenchResult<i64> {
        validate_id("session id", &request.session_id)?;
        validate_id("operation id", &request.operation_id)?;
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WorkbenchError::storage("Failed to start session deletion", error))?;
        ensure_operation_unused(&transaction, &request.operation_id)?;
        let current = load_session(&transaction, &request.session_id)?;
        let running: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM research_executions WHERE session_id = ?1 AND outcome IN ('queued', 'running')",
                [&request.session_id],
                |row| row.get(0),
            )
            .map_err(|error| WorkbenchError::storage("Failed to inspect conversation jobs", error))?;
        if running > 0 {
            return Err(WorkbenchError::conflict(
                "Stop this conversation's local jobs before deleting it",
            ));
        }
        for sql in [
            "DELETE FROM transcript_items WHERE binding_id IN (SELECT id FROM session_bindings WHERE session_id = ?1)",
            "DELETE FROM turns WHERE binding_id IN (SELECT id FROM session_bindings WHERE session_id = ?1)",
            "DELETE FROM session_bindings WHERE session_id = ?1",
            "DELETE FROM config_snapshots WHERE session_id = ?1",
            "DELETE FROM context_snapshots WHERE session_id = ?1",
            "DELETE FROM sessions WHERE id = ?1",
        ] {
            transaction
                .execute(sql, [&request.session_id])
                .map_err(|error| map_constraint("Failed to delete session", error))?;
        }
        let sequence = insert_change(
            &transaction,
            &request.operation_id,
            "session",
            &request.session_id,
            "session_deleted",
            current.workspace_id.as_deref(),
            Some(&request.session_id),
            &json!({ "title": current.title }),
            &timestamp,
        )?;
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit session deletion", error))?;
        Ok(sequence)
    }

    /// The conversation currently bound to a live native thread, if any.
    pub(crate) fn session_for_thread(
        &self,
        runtime_namespace: &str,
        provider_thread_id: &str,
    ) -> WorkbenchResult<Option<String>> {
        validate_id("runtime namespace", runtime_namespace)?;
        validate_provider_id("provider thread id", provider_thread_id)?;
        self.connection()?
            .query_row(
                "SELECT session_id FROM session_bindings WHERE runtime_namespace = ?1 AND provider_thread_id = ?2 AND retired_at IS NULL ORDER BY incarnation DESC LIMIT 1",
                params![runtime_namespace, provider_thread_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| WorkbenchError::storage("Failed to resolve thread binding", error))
    }

    /// Reads one app-owned preference. Preferences never enter harness
    /// fingerprints or archives' research semantics.
    pub fn preference(&self, key: &str) -> WorkbenchResult<Option<Value>> {
        validate_id("preference key", key)?;
        let raw: Option<String> = self
            .connection()?
            .query_row(
                "SELECT value_json FROM preferences WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| WorkbenchError::storage("Failed to read preference", error))?;
        Ok(raw.and_then(|value| serde_json::from_str(&value).ok()))
    }

    pub fn set_preference(&self, key: &str, value: &Value) -> WorkbenchResult<()> {
        validate_id("preference key", key)?;
        let encoded = serde_json::to_string(value)
            .map_err(|error| WorkbenchError::invalid(format!("Invalid preference: {error}")))?;
        if encoded.len() > 64 * 1024 {
            return Err(WorkbenchError::invalid("Preference exceeds 64 KiB"));
        }
        self.connection()?
            .execute(
                "INSERT INTO preferences (key, value_json, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
                params![key, encoded, now()],
            )
            .map_err(|error| WorkbenchError::storage("Failed to save preference", error))?;
        Ok(())
    }

    pub fn session_snapshot(&self, session_id: &str) -> WorkbenchResult<SessionSnapshot> {
        validate_id("session id", session_id)?;
        let connection = self.connection()?;
        let session = load_session(&connection, session_id)?;
        let workspace = session
            .workspace_id
            .as_deref()
            .map(|workspace_id| load_workspace(&connection, workspace_id))
            .transpose()?;
        Ok(SessionSnapshot {
            workspace,
            session,
            sequence: current_sequence(&connection)?,
        })
    }

    pub fn conversation_snapshot(&self, session_id: &str) -> WorkbenchResult<ConversationSnapshot> {
        const MAX_HYDRATED_TURNS: usize = 500;
        const MAX_HYDRATED_ITEMS: usize = 2_000;
        validate_id("session id", session_id)?;
        let connection = self.connection()?;
        let session = load_session(&connection, session_id)?;
        let workspace = session
            .workspace_id
            .as_deref()
            .map(|workspace_id| load_workspace(&connection, workspace_id))
            .transpose()?;
        let active_binding = load_active_binding(&connection, session_id)?;
        let mut turns_statement = connection
            .prepare(
                "SELECT t.id, t.binding_id, t.client_submission_id, t.provider_turn_id, t.state, t.error_json, t.created_at, t.updated_at, t.terminal_at FROM turns t JOIN session_bindings b ON b.id = t.binding_id WHERE b.session_id = ?1 AND (t.provider_turn_id IS NULL OR NOT EXISTS (SELECT 1 FROM turns newer JOIN session_bindings nb ON nb.id = newer.binding_id WHERE nb.session_id = b.session_id AND newer.provider_turn_id = t.provider_turn_id AND nb.incarnation > b.incarnation)) ORDER BY t.created_at, t.id LIMIT ?2",
            )
            .map_err(|error| WorkbenchError::storage("Failed to prepare conversation turns", error))?;
        let turns = turns_statement
            .query_map(
                params![session_id, (MAX_HYDRATED_TURNS + 1) as i64],
                |row| {
                    let error: Option<String> = row.get(5)?;
                    Ok(ConversationTurn {
                        id: row.get(0)?,
                        binding_id: row.get(1)?,
                        client_submission_id: row.get(2)?,
                        provider_turn_id: row.get(3)?,
                        state: row.get(4)?,
                        error: error.and_then(|value| serde_json::from_str(&value).ok()),
                        created_at: row.get(6)?,
                        updated_at: row.get(7)?,
                        terminal_at: row.get(8)?,
                    })
                },
            )
            .map_err(|error| WorkbenchError::storage("Failed to load conversation turns", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                WorkbenchError::storage("Failed to decode conversation turns", error)
            })?;
        if turns.len() > MAX_HYDRATED_TURNS {
            return Err(WorkbenchError::invalid(
                "Conversation turn history exceeds its hydration limit",
            ));
        }
        let mut items_statement = connection
            .prepare(
                "SELECT i.id, i.turn_id, i.provider_item_id, i.item_kind, i.payload_json, i.is_final, i.created_at, i.updated_at FROM transcript_items i JOIN session_bindings b ON b.id = i.binding_id WHERE b.session_id = ?1 AND NOT EXISTS (SELECT 1 FROM transcript_items newer JOIN session_bindings nb ON nb.id = newer.binding_id WHERE nb.session_id = b.session_id AND newer.provider_item_id = i.provider_item_id AND nb.incarnation > b.incarnation) ORDER BY i.created_at, i.id LIMIT ?2",
            )
            .map_err(|error| WorkbenchError::storage("Failed to prepare conversation items", error))?;
        let items = items_statement
            .query_map(
                params![session_id, (MAX_HYDRATED_ITEMS + 1) as i64],
                |row| {
                    let payload: String = row.get(4)?;
                    Ok(TranscriptItem {
                        id: row.get(0)?,
                        turn_id: row.get(1)?,
                        provider_item_id: row.get(2)?,
                        item_kind: row.get(3)?,
                        payload: serde_json::from_str(&payload).unwrap_or(Value::Null),
                        is_final: row.get::<_, i64>(5)? != 0,
                        created_at: row.get(6)?,
                        updated_at: row.get(7)?,
                    })
                },
            )
            .map_err(|error| WorkbenchError::storage("Failed to load conversation items", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                WorkbenchError::storage("Failed to decode conversation items", error)
            })?;
        if items.len() > MAX_HYDRATED_ITEMS {
            return Err(WorkbenchError::invalid(
                "Conversation item history exceeds its hydration limit",
            ));
        }
        Ok(ConversationSnapshot {
            workspace,
            session,
            active_binding,
            turns,
            items,
            sequence: current_sequence(&connection)?,
        })
    }

    pub(crate) fn active_binding(
        &self,
        session_id: &str,
    ) -> WorkbenchResult<Option<SessionBinding>> {
        validate_id("session id", session_id)?;
        load_active_binding(&self.connection()?, session_id)
    }

    pub(crate) fn runtime_root(&self, session_id: &str) -> WorkbenchResult<PathBuf> {
        let snapshot = self.session_snapshot(session_id)?;
        if let (Some(workspace), Some(checkpoint)) = (
            snapshot.workspace.as_ref(),
            snapshot
                .session
                .overrides
                .get("projectCheckpointId")
                .and_then(Value::as_str),
        ) {
            return super::project::session_task_root(self, &workspace.id, checkpoint);
        }
        if let Some(root) = snapshot.workspace.and_then(|workspace| workspace.root) {
            let path = PathBuf::from(root);
            if path.is_dir() {
                return Ok(path);
            }
            return Err(WorkbenchError::invalid(
                "The registered Workspace folder is unavailable",
            ));
        }
        let root = self
            .root
            .join("jobs")
            .join("conversations")
            .join(session_id);
        secure_directory(&root)?;
        Ok(root)
    }

    pub fn reconcile_workspace_roots(
        &self,
        operation_id: &str,
    ) -> WorkbenchResult<ReconcileResult> {
        validate_id("operation id", operation_id)?;
        let mut connection = self.connection()?;
        let roots = {
            let mut statement = connection
                .prepare("SELECT id, root FROM workspaces WHERE root IS NOT NULL ORDER BY id")
                .map_err(|error| WorkbenchError::storage("Failed to inspect workspaces", error))?;
            let roots = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|error| WorkbenchError::storage("Failed to inspect workspaces", error))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| WorkbenchError::storage("Failed to decode workspaces", error))?;
            roots
        };
        let mut states = Vec::with_capacity(roots.len());
        for (workspace_id, root) in roots {
            let exists = match std::fs::metadata(&root) {
                Ok(metadata) => metadata.is_dir(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(error) => {
                    return Err(WorkbenchError::storage(
                        "Failed to inspect workspace root",
                        error,
                    ))
                }
            };
            states.push((workspace_id, exists));
        }
        let timestamp = now();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WorkbenchError::storage("Failed to start reconciliation", error))?;
        ensure_operation_unused(&transaction, operation_id)?;
        let mut missing = 0usize;
        let mut restored = 0usize;
        for (workspace_id, exists) in states {
            let prior: Option<String> = transaction
                .query_row(
                    "SELECT missing_root_at FROM workspaces WHERE id = ?1",
                    [&workspace_id],
                    |row| row.get(0),
                )
                .map_err(|error| WorkbenchError::storage("Failed to reconcile workspace", error))?;
            if !exists && prior.is_none() {
                transaction
                    .execute(
                        "UPDATE workspaces SET missing_root_at = ?1, revision = revision + 1, updated_at = ?1 WHERE id = ?2",
                        params![timestamp, workspace_id],
                    )
                    .map_err(|error| WorkbenchError::storage("Failed to mark workspace root missing", error))?;
                missing += 1;
            } else if exists && prior.is_some() {
                transaction
                    .execute(
                        "UPDATE workspaces SET missing_root_at = NULL, revision = revision + 1, updated_at = ?1 WHERE id = ?2",
                        params![timestamp, workspace_id],
                    )
                    .map_err(|error| WorkbenchError::storage("Failed to restore workspace", error))?;
                restored += 1;
            }
        }
        let changed = missing + restored;
        let sequence = insert_change(
            &transaction,
            operation_id,
            "workspaces",
            "all",
            "workspace_roots_reconciled",
            None,
            None,
            &json!({ "changed": changed, "missing": missing, "restored": restored }),
            &timestamp,
        )?;
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit reconciliation", error))?;
        Ok(ReconcileResult {
            changed,
            missing,
            restored,
            sequence,
        })
    }

    pub(crate) fn bind_session(
        &self,
        session_id: &str,
        runtime_namespace: &str,
        provider_thread_id: &str,
    ) -> WorkbenchResult<SessionBinding> {
        validate_id("session id", session_id)?;
        validate_id("runtime namespace", runtime_namespace)?;
        validate_provider_id("provider thread id", provider_thread_id)?;
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WorkbenchError::storage("Failed to start session binding", error))?;
        let session = load_session(&transaction, session_id)?;
        if let Some(existing) = load_active_binding(&transaction, session_id)? {
            if existing.runtime_namespace == runtime_namespace
                && existing.provider_thread_id == provider_thread_id
            {
                return Ok(existing);
            }
            transaction
                .execute(
                    "UPDATE session_bindings SET retired_at = ?1, retirement_reason = 'superseded' WHERE id = ?2",
                    params![timestamp, existing.id],
                )
                .map_err(|error| WorkbenchError::storage("Failed to retire session binding", error))?;
        }
        let incarnation: i64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(incarnation), 0) + 1 FROM session_bindings WHERE session_id = ?1",
                [session_id],
                |row| row.get(0),
            )
            .map_err(|error| WorkbenchError::storage("Failed to allocate binding incarnation", error))?;
        let id = new_id("binding")?;
        transaction
            .execute(
                "INSERT INTO session_bindings (id, session_id, runtime_namespace, provider_thread_id, incarnation, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, session_id, runtime_namespace, provider_thread_id, incarnation, timestamp],
            )
            .map_err(|error| map_constraint("Failed to bind Workbench session", error))?;
        insert_change(
            &transaction,
            &format!("binding-create-{id}"),
            "session_binding",
            &id,
            "session_bound",
            session.workspace_id.as_deref(),
            Some(session_id),
            &json!({
                "runtimeNamespace": runtime_namespace,
                "providerThreadId": provider_thread_id,
                "incarnation": incarnation
            }),
            &timestamp,
        )?;
        let binding = load_binding(&transaction, &id)?;
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit session binding", error))?;
        Ok(binding)
    }

    pub(crate) fn record_turn_submission(
        &self,
        binding_id: &str,
        client_submission_id: &str,
        provider_turn_id: &str,
    ) -> WorkbenchResult<()> {
        validate_id("binding id", binding_id)?;
        validate_id("client submission id", client_submission_id)?;
        validate_provider_id("provider turn id", provider_turn_id)?;
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                WorkbenchError::storage("Failed to start turn reconciliation", error)
            })?;
        let turn_id: Option<String> = transaction
            .query_row(
                "SELECT id FROM turns WHERE binding_id = ?1 AND provider_turn_id = ?2",
                params![binding_id, provider_turn_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| WorkbenchError::storage("Failed to inspect projected turn", error))?;
        if let Some(turn_id) = turn_id {
            transaction
                .execute(
                    "UPDATE turns SET client_submission_id = ?1, updated_at = ?2 WHERE id = ?3",
                    params![client_submission_id, timestamp, turn_id],
                )
                .map_err(|error| map_constraint("Failed to attach turn submission", error))?;
        } else {
            let id = new_id("turn")?;
            transaction
                .execute(
                    "INSERT INTO turns (id, binding_id, client_submission_id, provider_turn_id, state, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'inProgress', ?5, ?5)",
                    params![id, binding_id, client_submission_id, provider_turn_id, timestamp],
                )
                .map_err(|error| map_constraint("Failed to record turn submission", error))?;
        }
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit turn reconciliation", error))
    }

    pub(crate) fn set_binding_harness(
        &self,
        binding_id: &str,
        fingerprint: &str,
        instruction_sources: &[String],
    ) -> WorkbenchResult<()> {
        validate_id("binding id", binding_id)?;
        if fingerprint.len() != 64 || !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(WorkbenchError::invalid("Harness fingerprint is invalid"));
        }
        let instruction_sources = serde_json::to_string(instruction_sources).map_err(|error| {
            WorkbenchError::storage("Failed to encode native instruction sources", error)
        })?;
        let connection = self.connection()?;
        let changed = connection
            .execute(
                "UPDATE session_bindings SET harness_fingerprint = ?2, instruction_sources_json = ?3 WHERE id = ?1",
                params![binding_id, fingerprint, instruction_sources],
            )
            .map_err(|error| WorkbenchError::storage("Failed to bind harness snapshot", error))?;
        if changed != 1 {
            return Err(WorkbenchError::invalid(
                "Workbench session binding does not exist",
            ));
        }
        Ok(())
    }

    pub(crate) fn attach_turn_snapshots(
        &self,
        binding_id: &str,
        client_submission_id: &str,
        config_snapshot_id: &str,
        context_snapshot_id: &str,
    ) -> WorkbenchResult<()> {
        for (label, value) in [
            ("binding id", binding_id),
            ("client submission id", client_submission_id),
            ("configuration snapshot id", config_snapshot_id),
            ("context snapshot id", context_snapshot_id),
        ] {
            validate_id(label, value)?;
        }
        let connection = self.connection()?;
        let changed = connection
            .execute(
                "UPDATE turns SET config_snapshot_id = ?3, context_snapshot_id = ?4, instruction_snapshot_id = ?3 WHERE binding_id = ?1 AND client_submission_id = ?2",
                params![binding_id, client_submission_id, config_snapshot_id, context_snapshot_id],
            )
            .map_err(|error| WorkbenchError::storage("Failed to attach immutable turn snapshots", error))?;
        if changed != 1 {
            return Err(WorkbenchError::invalid(
                "Submitted turn could not be linked to its snapshots",
            ));
        }
        Ok(())
    }

    pub(crate) fn project_codex_event(
        &self,
        binding_id: &str,
        event: &crate::workbench::codex::NormalizedEvent,
    ) -> WorkbenchResult<Option<i64>> {
        use crate::workbench::codex::NormalizedEvent;
        validate_id("binding id", binding_id)?;
        let (thread_id, operation_id) = match event {
            NormalizedEvent::TurnStarted {
                epoch,
                thread_id,
                turn_id,
                ..
            } => (thread_id, format!("codex-{epoch}-turn-{turn_id}-started")),
            NormalizedEvent::TurnCompleted {
                epoch,
                thread_id,
                turn_id,
                ..
            } => (thread_id, format!("codex-{epoch}-turn-{turn_id}-completed")),
            NormalizedEvent::ItemStarted {
                epoch,
                thread_id,
                item_id,
                ..
            } => (thread_id, format!("codex-{epoch}-item-{item_id}-started")),
            NormalizedEvent::ItemCompleted {
                epoch,
                thread_id,
                item_id,
                ..
            } => (thread_id, format!("codex-{epoch}-item-{item_id}-completed")),
            _ => return Ok(None),
        };
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WorkbenchError::storage("Failed to start event projection", error))?;
        let binding = load_binding(&transaction, binding_id)?;
        if binding.provider_thread_id != *thread_id {
            return Err(WorkbenchError::invalid(
                "Codex event thread does not match its Workbench binding",
            ));
        }
        if let Some(sequence) = change_sequence(&transaction, &operation_id)? {
            return Ok(Some(sequence));
        }
        match event {
            NormalizedEvent::TurnStarted { turn_id, turn, .. } => {
                upsert_projected_turn(&transaction, binding_id, turn_id, turn, &timestamp, false)?;
            }
            NormalizedEvent::TurnCompleted { turn_id, turn, .. } => {
                upsert_projected_turn(&transaction, binding_id, turn_id, turn, &timestamp, true)?;
                if let Some(items) = turn.get("items").and_then(Value::as_array) {
                    for item in items {
                        let item_id = required_json_string(item, "id", "completed turn item")?;
                        let item_kind = required_json_string(item, "type", "completed turn item")?;
                        upsert_projected_item(
                            &transaction,
                            binding_id,
                            turn_id,
                            item_id,
                            item_kind,
                            item,
                            true,
                            &timestamp,
                        )?;
                    }
                }
            }
            NormalizedEvent::ItemStarted {
                turn_id,
                item_id,
                item_kind,
                item,
                ..
            } => upsert_projected_item(
                &transaction,
                binding_id,
                turn_id,
                item_id,
                item_kind,
                item,
                false,
                &timestamp,
            )?,
            NormalizedEvent::ItemCompleted {
                turn_id,
                item_id,
                item_kind,
                item,
                ..
            } => upsert_projected_item(
                &transaction,
                binding_id,
                turn_id,
                item_id,
                item_kind,
                item,
                true,
                &timestamp,
            )?,
            _ => unreachable!("non-projectable events returned above"),
        }
        let session = load_session(&transaction, &binding.session_id)?;
        let sequence = insert_change(
            &transaction,
            &operation_id,
            "codex_projection",
            binding_id,
            "provider_event_projected",
            session.workspace_id.as_deref(),
            Some(&binding.session_id),
            &json!({"providerThreadId": thread_id}),
            &timestamp,
        )?;
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit event projection", error))?;
        Ok(Some(sequence))
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
            transaction.execute_batch(include_str!("migrations/011_research_desk.sql"))
                .map_err(|error| WorkbenchError::storage("Failed to apply Workbench migration 11", error))?;
        }
        if from < 12 { transaction.execute_batch(include_str!("migrations/012_task_exchanges.sql")).map_err(|e| WorkbenchError::storage("Failed to migrate task exchanges", e))?; }
        if from < 13 { transaction.execute_batch(include_str!("migrations/013_research_programs.sql")).map_err(|e| WorkbenchError::storage("Failed to migrate research programs", e))?; }
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
