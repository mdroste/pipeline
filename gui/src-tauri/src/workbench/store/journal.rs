//! Operation replay checks and ordered durable change records.

use super::{map_constraint, WorkbenchError, WorkbenchResult};
use rusqlite::{params, Connection, OptionalExtension as _};
use serde_json::Value;

pub(super) fn change_sequence(
    connection: &Connection,
    operation_id: &str,
) -> WorkbenchResult<Option<i64>> {
    connection
        .query_row(
            "SELECT sequence FROM change_log WHERE operation_id = ?1",
            [operation_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to inspect projected event", error))
}

pub(super) fn ensure_operation_unused(
    connection: &Connection,
    operation_id: &str,
) -> WorkbenchResult<()> {
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
pub(super) fn insert_change(
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

pub(super) fn current_sequence(connection: &Connection) -> WorkbenchResult<i64> {
    connection
        .query_row(
            "SELECT COALESCE(MAX(sequence), 0) FROM change_log",
            [],
            |row| row.get(0),
        )
        .map_err(|error| WorkbenchError::storage("Failed to read Workbench sequence", error))
}
