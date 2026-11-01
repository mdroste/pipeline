//! Conflict inspection and revision-preserving human decisions.
use super::authority::note_status;
use super::types::{ExchangeConflict, RECORD_KINDS};
use super::{err, now, text};
use crate::workbench::research::validate_text;
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, OptionalExtension as _, TransactionBehavior};
use serde_json::Value;

const CONFLICT_COLUMNS: &str =
    "id,import_id,object_kind,object_id,local_json,imported_json,state,recorded_at,base_json";
fn conflict_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ExchangeConflict> {
    Ok(ExchangeConflict {
        id: r.get(0)?,
        import_id: r.get(1)?,
        object_kind: r.get(2)?,
        object_id: r.get(3)?,
        local: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or(Value::Null),
        imported: serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or(Value::Null),
        state: r.get(6)?,
        recorded_at: r.get(7)?,
        base: r
            .get::<_, Option<String>>(8)?
            .and_then(|s| serde_json::from_str(&s).ok()),
    })
}
/// Open conflicts come first, so settled ones never push one off the list.
pub fn list_conflicts(store: &Store, ws: &str) -> WorkbenchResult<Vec<ExchangeConflict>> {
    store.workspace(ws)?;
    let conn = store.connection()?;
    let mut stmt = conn.prepare(&format!("SELECT {CONFLICT_COLUMNS} FROM exchange_conflicts WHERE workspace_id=?1 ORDER BY state='open' DESC,recorded_at DESC LIMIT 500")).map_err(err)?;
    let rows = stmt
        .query_map([ws], conflict_from_row)
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    Ok(rows)
}
fn load_conflict(store: &Store, ws: &str, conflict_id: &str) -> WorkbenchResult<ExchangeConflict> {
    store
        .connection()?
        .query_row(
            &format!(
                "SELECT {CONFLICT_COLUMNS} FROM exchange_conflicts WHERE id=?1 AND workspace_id=?2"
            ),
            params![conflict_id, ws],
            conflict_from_row,
        )
        .optional()
        .map_err(err)?
        .ok_or_else(|| WorkbenchError::invalid("Conflict was not found"))
}
/// Resolve a conflict. `keep_local` records the decision; `take_imported`
/// replaces the local project record or note body with the imported one under
/// a fresh revision (the previous body stays in history). Papers, executions,
/// sources and ledger entries are immutable and can only be kept.
pub fn resolve_conflict(
    store: &Store,
    ws: &str,
    conflict_id: &str,
    take_imported: bool,
) -> WorkbenchResult<ExchangeConflict> {
    store.workspace(ws)?;
    let conflict = load_conflict(store, ws, conflict_id)?;
    if conflict.state != "open" {
        return Err(WorkbenchError::invalid("This conflict is already resolved"));
    }
    let mut conn = store.connection()?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(err)?;
    if take_imported {
        match conflict.object_kind.as_str() {
            "note" => {
                let body = validate_text(
                    "Imported note",
                    &text(&conflict.imported, "body"),
                    64 * 1024,
                )?;
                let (state, origin) = note_status(&conflict.imported);
                tx.execute("UPDATE research_notes SET body=?2, state=?3, origin=?4, pinned=?5, revision=revision+1, updated_at=?6 WHERE id=?1 AND workspace_id=?7", params![conflict.object_id, body, state, origin, conflict.imported["pinned"].as_bool().unwrap_or(false), now(), ws]).map_err(err)?;
            }
            kind if RECORD_KINDS.contains(&kind) => {
                let body = conflict.imported["body"].clone();
                if !body.is_object() {
                    return Err(WorkbenchError::invalid(
                        "The imported copy of this record has no content; keep the local version",
                    ));
                }
                tx.execute("UPDATE project_records SET revision=revision+1, body_json=?2, updated_at=?3 WHERE id=?1 AND workspace_id=?4", params![conflict.object_id, body.to_string(), now(), ws]).map_err(err)?;
            }
            _ => return Err(WorkbenchError::invalid(
                "Immutable objects keep the local version; the imported copy stays reviewable here",
            )),
        }
    }
    if take_imported {
        tx.execute("INSERT INTO exchange_bases(workspace_id,source_namespace,kind,source_id,local_id,value_json) SELECT ?1,i.source_namespace,?2,json_extract(d.value,'$.id'),?3,?4 FROM exchange_imports i,json_each(i.decisions_json) d WHERE i.id=?5 AND json_extract(d.value,'$.kind')=?2 AND COALESCE(json_extract(d.value,'$.newId'),json_extract(d.value,'$.id'))=?3 ON CONFLICT(workspace_id,source_namespace,kind,source_id) DO UPDATE SET value_json=excluded.value_json,local_id=excluded.local_id",params![ws,conflict.object_kind,conflict.object_id,conflict.imported.to_string(),conflict.import_id]).map_err(err)?;
    }
    tx.execute(
        "UPDATE exchange_conflicts SET state=?2, resolved_at=?3 WHERE id=?1",
        params![
            conflict_id,
            if take_imported {
                "took_imported"
            } else {
                "kept_local"
            },
            now()
        ],
    )
    .map_err(err)?;
    tx.commit().map_err(err)?;
    load_conflict(store, ws, conflict_id)
}
