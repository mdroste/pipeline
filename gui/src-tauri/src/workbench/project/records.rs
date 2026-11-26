//! Scoped project records, revision checks, and exact row decoding.

use super::{err, now, scope, valid_id};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, OptionalExtension as _, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecord {
    pub id: String,
    pub workspace_id: String,
    pub kind: String,
    pub revision: i64,
    pub body: Value,
    pub updated_at: String,
}
pub(super) fn read_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectRecord> {
    let encoded: String = row.get(4)?;
    Ok(ProjectRecord {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        kind: row.get(2)?,
        revision: row.get(3)?,
        body: serde_json::from_str(&encoded).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
        })?,
        updated_at: row.get(5)?,
    })
}
pub fn record(
    store: &Store,
    workspace_id: &str,
    object_id: &str,
    kind: &str,
) -> WorkbenchResult<ProjectRecord> {
    scope(store, workspace_id)?;
    record_on(&store.connection()?, workspace_id, object_id, kind)
}

pub(super) fn record_on(
    connection: &rusqlite::Connection,
    workspace_id: &str,
    object_id: &str,
    kind: &str,
) -> WorkbenchResult<ProjectRecord> {
    valid_id(object_id)?;
    connection.query_row("SELECT id,workspace_id,kind,revision,body_json,updated_at FROM project_records WHERE id=?1 AND workspace_id=?2 AND kind=?3",params![object_id,workspace_id,kind],read_record).optional().map_err(err)?.ok_or_else(||WorkbenchError::invalid("Project object is unavailable in this Workspace"))
}
/// How many records a list shows. Callers that need every record look them up
/// with `records_with_values` or read through `records_up_to`.
const RECORD_LIST_LIMIT: usize = 500;
/// Upper bound for a reader that treats its records as complete.
pub const RECORD_READ_LIMIT: usize = 20_000;

/// The newest records of a kind, for a list that shows a page of them.
pub fn records(
    store: &Store,
    workspace_id: &str,
    kind: &str,
) -> WorkbenchResult<Vec<ProjectRecord>> {
    records_up_to(store, workspace_id, kind, RECORD_LIST_LIMIT)
}
/// The newest `limit` records of a kind.
pub fn records_up_to(
    store: &Store,
    workspace_id: &str,
    kind: &str,
    limit: usize,
) -> WorkbenchResult<Vec<ProjectRecord>> {
    scope(store, workspace_id)?;
    records_on(&store.connection()?, workspace_id, kind, limit)
}

pub(super) fn records_on(
    conn: &rusqlite::Connection,
    workspace_id: &str,
    kind: &str,
    limit: usize,
) -> WorkbenchResult<Vec<ProjectRecord>> {
    let mut statement=conn.prepare("SELECT id,workspace_id,kind,revision,body_json,updated_at FROM project_records WHERE workspace_id=?1 AND kind=?2 ORDER BY updated_at DESC,id LIMIT ?3").map_err(err)?;
    let result = statement
        .query_map(params![workspace_id, kind, limit as i64], read_record)
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    Ok(result)
}
/// Every record of a kind whose JSON value at `path` is one of `values`, however
/// many records the project holds. The answer grows only with what was asked.
pub fn records_with_values(
    store: &Store,
    workspace_id: &str,
    kind: &str,
    path: &str,
    values: &[&str],
) -> WorkbenchResult<Vec<ProjectRecord>> {
    scope(store, workspace_id)?;
    let wanted = serde_json::to_string(values).map_err(err)?;
    let conn = store.connection()?;
    let mut statement=conn.prepare("SELECT id,workspace_id,kind,revision,body_json,updated_at FROM project_records WHERE workspace_id=?1 AND kind=?2 AND json_extract(body_json,?3) IN (SELECT value FROM json_each(?4)) ORDER BY updated_at DESC,id").map_err(err)?;
    let result = statement
        .query_map(params![workspace_id, kind, path, wanted], read_record)
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    Ok(result)
}
pub(super) fn put(
    store: &Store,
    workspace_id: &str,
    object_id: &str,
    kind: &str,
    expected: i64,
    body: &impl Serialize,
) -> WorkbenchResult<ProjectRecord> {
    put_checked(store, workspace_id, object_id, kind, expected, body, |_| {
        Ok(())
    })
}

/// Validate captured database state in the same write transaction as publication.
pub(super) fn put_checked(
    store: &Store,
    workspace_id: &str,
    object_id: &str,
    kind: &str,
    expected: i64,
    body: &impl Serialize,
    check: impl FnOnce(&rusqlite::Connection) -> WorkbenchResult<()>,
) -> WorkbenchResult<ProjectRecord> {
    let encoded = serde_json::to_string(body).map_err(err)?;
    if encoded.len() > 8 * 1024 * 1024 {
        return Err(WorkbenchError::invalid("Project record exceeds 8 MiB"));
    }
    let mut conn = store.connection()?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(err)?;
    check(&tx)?;
    // `id` is the sole primary key. Look it up unscoped so a deterministic id
    // (response_/zotero_ hashes) owned by another project or kind is refused
    // instead of silently overwritten by the upsert below.
    let current: Option<(String, String, i64)> = tx
        .query_row(
            "SELECT workspace_id,kind,revision FROM project_records WHERE id=?1",
            [object_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(err)?;
    if let Some((owner, owner_kind, _)) = &current {
        if owner != workspace_id || owner_kind != kind {
            return Err(WorkbenchError::conflict(
                "This record ID belongs to another project or object kind",
            ));
        }
    }
    if current.as_ref().map_or(0, |c| c.2) != expected {
        return Err(WorkbenchError::conflict(
            "This project record changed; refresh before editing",
        ));
    }
    let timestamp = now();
    let changed = tx.execute("INSERT INTO project_records(id,workspace_id,kind,revision,body_json,updated_at) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,body_json=excluded.body_json,updated_at=excluded.updated_at WHERE project_records.workspace_id=excluded.workspace_id AND project_records.kind=excluded.kind",params![object_id,workspace_id,kind,expected+1,encoded,timestamp]).map_err(err)?;
    if changed != 1 {
        return Err(WorkbenchError::conflict(
            "This record ID belongs to another project or object kind",
        ));
    }
    tx.commit().map_err(err)?;
    record(store, workspace_id, object_id, kind)
}
pub(super) fn decode<T: serde::de::DeserializeOwned>(record: &ProjectRecord) -> WorkbenchResult<T> {
    serde_json::from_value(record.body.clone()).map_err(err)
}

pub fn anchor_read(store: &Store, ws: &str, anchor_id: &str) -> WorkbenchResult<ProjectRecord> {
    record(store, ws, anchor_id, "anchor")
}

/// A feature owns its record kind. Raw records remain the compatibility/export
/// boundary; typed access never rewrites an unknown body merely by reading it.
pub(super) trait RecordBody: Serialize + serde::de::DeserializeOwned {
    const KIND: &'static str;
}
pub(super) fn typed_body<T: RecordBody>(record: &ProjectRecord) -> WorkbenchResult<T> {
    if record.kind != T::KIND {
        return Err(WorkbenchError::invalid(
            "Project record has a different object kind",
        ));
    }
    decode(record)
}
pub(super) fn read_body<T: RecordBody>(
    store: &Store,
    workspace_id: &str,
    object_id: &str,
) -> WorkbenchResult<T> {
    typed_body(&record(store, workspace_id, object_id, T::KIND)?)
}
pub(super) fn put_body<T: RecordBody>(
    store: &Store,
    workspace_id: &str,
    object_id: &str,
    expected: i64,
    body: &T,
) -> WorkbenchResult<ProjectRecord> {
    put(store, workspace_id, object_id, T::KIND, expected, body)
}

#[cfg(test)]
mod typed_tests {
    use super::{typed_body, ProjectRecord};
    use crate::workbench::project::home::ProjectHomeSettings;
    use serde_json::json;
    #[test]
    fn typed_reads_check_kind_without_rewriting_raw_records() {
        let body = serde_json::to_value(ProjectHomeSettings::default()).unwrap();
        let mut record = ProjectRecord {
            id: "home".into(),
            workspace_id: "ws".into(),
            kind: "task".into(),
            revision: 1,
            body,
            updated_at: "now".into(),
        };
        assert!(typed_body::<ProjectHomeSettings>(&record).is_err());
        record.kind = "home".into();
        assert!(typed_body::<ProjectHomeSettings>(&record).is_ok());
        record.body["futureField"] = json!([1, 2]);
        let original = record.clone();
        assert!(typed_body::<ProjectHomeSettings>(&record).is_err());
        assert_eq!(record, original);
    }
}
