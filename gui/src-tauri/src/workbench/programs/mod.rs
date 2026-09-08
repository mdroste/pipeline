//! Typed research plans and deliverables layered on existing Workspace services.
//! Immutable records describe research; mutable run/check state never changes their bodies.
use super::{
    desk::{self, DeskRecord, ResearchObjectRef},
    store::{Store, WorkbenchError, WorkbenchResult},
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, io::Read, path::Path};
pub mod assets;
pub mod campaigns;
pub mod capsule;
pub mod commands;
pub mod delivery;
pub mod experiments;
pub mod followups;
pub mod monitors;
#[cfg(test)]
mod tests;
pub mod theory;
fn err(e: impl std::fmt::Display) -> WorkbenchError {
    desk::err(e)
}
fn bounded(s: &str, n: usize) -> WorkbenchResult<()> {
    desk::check_text(s, n)
}
fn load<T: for<'de> Deserialize<'de>>(
    store: &Store,
    ws: &str,
    id: &str,
    kind: &str,
) -> WorkbenchResult<(DeskRecord, T)> {
    let record = desk::record(store, ws, id)?;
    if record.kind != kind {
        return Err(WorkbenchError::invalid(format!("Select a {kind} record")));
    }
    let body = serde_json::from_value(record.body.clone()).map_err(err)?;
    Ok((record, body))
}
fn exact(store: &Store, ws: &str, object: &ResearchObjectRef) -> WorkbenchResult<()> {
    super::search::read_object(store, ws, object, 1).map(|_| ())
}
fn refs(
    store: &Store,
    ws: &str,
    objects: &[ResearchObjectRef],
    limit: usize,
) -> WorkbenchResult<()> {
    if objects.len() > limit {
        return Err(WorkbenchError::invalid("Too many research references"));
    }
    for object in objects {
        exact(store, ws, object)?;
    }
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub id: String,
    pub hash: String,
    pub extension: String,
    pub size_bytes: usize,
}
fn blob(store: &Store, ws: &str, bytes: &[u8], extension: &str) -> WorkbenchResult<Artifact> {
    store.workspace(ws)?;
    if bytes.len() > 8 * 1024 * 1024
        || !["md", "tex", "csv", "json", "py", "svg", "pdf", "png", "bin"].contains(&extension)
    {
        return Err(WorkbenchError::invalid(
            "Unsupported or oversized research artifact",
        ));
    }
    let hash = desk::hash(bytes);
    let id = format!(
        "asset_{}",
        desk::hash(format!("{ws}:{extension}:{hash}").as_bytes())
    );
    let path = store
        .root_path()
        .join("blobs")
        .join(format!("{hash}.{extension}"));
    if path.exists() {
        if desk::hash(&fs::read(&path).map_err(err)?) != hash {
            return Err(WorkbenchError::invalid(
                "Retained artifact integrity failed",
            ));
        }
    } else {
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap()).map_err(err)?;
        tmp.write_all(bytes).map_err(err)?;
        tmp.as_file().sync_all().map_err(err)?;
        tmp.persist_noclobber(&path).map_err(|e| err(e.error))?;
    }
    store.connection()?.execute("INSERT OR IGNORE INTO artifacts(id,workspace_id,content_hash,media_kind,size_bytes,origin,storage_reference,created_at) VALUES(?1,?2,?3,?4,?5,'research_program',?6,?7)",params![id,ws,hash,extension,bytes.len() as i64,path.to_string_lossy(),desk::now()]).map_err(err)?;
    let id: String = store
        .connection()?
        .query_row(
            "SELECT id FROM artifacts WHERE workspace_id=?1 AND content_hash=?2 AND media_kind=?3",
            params![ws, hash, extension],
            |r| r.get(0),
        )
        .map_err(err)?;
    Ok(Artifact {
        id,
        hash,
        extension: extension.into(),
        size_bytes: bytes.len(),
    })
}
fn artifact_bytes(store: &Store, ws: &str, id: &str, limit: usize) -> WorkbenchResult<Vec<u8>> {
    let (path, hash): (String, String) = store
        .connection()?
        .query_row(
            "SELECT storage_reference,content_hash FROM artifacts WHERE workspace_id=?1 AND id=?2",
            params![ws, id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(err)?;
    let path = fs::canonicalize(path).map_err(err)?;
    if !path.starts_with(
        store
            .root_path()
            .join("blobs")
            .canonicalize()
            .map_err(err)?,
    ) {
        return Err(WorkbenchError::invalid("Artifact escaped private storage"));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(err)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() > limit || desk::hash(&bytes) != hash {
        return Err(WorkbenchError::invalid(
            "Artifact is oversized or no longer matches its capture",
        ));
    }
    Ok(bytes)
}
pub fn read_artifact(store: &Store, ws: &str, id: &str) -> WorkbenchResult<Value> {
    let bytes = artifact_bytes(store, ws, id, 8 * 1024 * 1024)?;
    use base64::Engine;
    Ok(json!({"base64":base64::engine::general_purpose::STANDARD.encode(bytes)}))
}
pub fn export_artifact(store: &Store, ws: &str, id: &str, path: &str) -> WorkbenchResult<()> {
    let bytes = artifact_bytes(store, ws, id, 8 * 1024 * 1024)?;
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err(WorkbenchError::invalid("Choose an absolute export path"));
    }
    fs::write(path, bytes).map_err(err)
}

// Recover a profile committed before its enclosing immutable record was saved.
// Changed arguments never reuse an operation or clear an existing authorization.
fn save_profile_once(
    store: &Store,
    r: super::research::SaveExecutionProfileRequest,
) -> WorkbenchResult<super::research::ExecutionProfile> {
    let prior: Option<(String, String)> = store
        .connection()?
        .query_row(
            "SELECT entity_type,entity_id FROM change_log WHERE operation_id=?1",
            [&r.operation_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(err)?;
    if let Some((kind, id)) = prior {
        if kind != "execution_profile" {
            return Err(WorkbenchError::conflict(
                "Profile operation belongs to another mutation",
            ));
        }
        let p = super::research::get_execution_profile(store, &id)?;
        if p.workspace_id != r.workspace_id
            || p.name != r.name.trim()
            || p.adapter != r.adapter
            || p.argv != r.argv
            || Path::new(&p.cwd) != Path::new(&r.cwd)
            || p.environment != r.environment
            || p.inputs != r.inputs
            || p.outputs != r.outputs
            || p.timeout_seconds != r.timeout_seconds
        {
            return Err(WorkbenchError::conflict("A partially prepared profile differs from this request; use a new operation after review"));
        }
        return Ok(p);
    }
    super::research::save_execution_profile(store, r)
}
