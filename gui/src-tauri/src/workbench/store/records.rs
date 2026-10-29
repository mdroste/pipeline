//! Workspace record DTOs and exact SQLite row decoding.

use super::history::TranscriptCursor;
use super::{WorkbenchError, WorkbenchResult};
use rusqlite::{Connection, OptionalExtension as _, Row};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    #[serde(default)]
    pub older_cursor: Option<TranscriptCursor>,
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

fn parse_json(value: String, column: usize) -> rusqlite::Result<Value> {
    serde_json::from_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

pub(super) fn read_workspace_row(row: &Row<'_>) -> rusqlite::Result<Workspace> {
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

pub(super) fn load_workspace(
    connection: &Connection,
    workspace_id: &str,
) -> WorkbenchResult<Workspace> {
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

pub(super) fn read_session_row(row: &Row<'_>) -> rusqlite::Result<WorkbenchSession> {
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

pub(super) fn load_binding(
    connection: &Connection,
    binding_id: &str,
) -> WorkbenchResult<SessionBinding> {
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

pub(super) fn load_active_binding(
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

pub(super) fn load_session(
    connection: &Connection,
    session_id: &str,
) -> WorkbenchResult<WorkbenchSession> {
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
