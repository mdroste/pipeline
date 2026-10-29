//! Immutable paper revision preparation and Review handoff links.

use super::{new_id, now, validate_id};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, OptionalExtension as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const HANDOFF_VERSION: u32 = 1;
const MAX_HANDOFF_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_HANDOFF_ENTRIES: usize = 20_000;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareReviewHandoffRequest {
    pub workspace_id: String,
    pub session_id: Option<String>,
    pub paper_id: String,
    pub metadata: Value,
    pub operation_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkReviewHandoffRequest {
    pub handoff_id: String,
    pub external_reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewHandoff {
    pub version: u32,
    pub id: String,
    pub workspace_id: String,
    pub session_id: Option<String>,
    pub paper_id: String,
    pub revision_id: String,
    pub content_hash: String,
    pub media_kind: String,
    pub staged_path: String,
    pub input_interpretation: String,
    pub metadata: Value,
    pub external_reference: Option<String>,
    pub created_at: String,
    pub linked_at: Option<String>,
}

fn handoff_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReviewHandoff> {
    let metadata: String = row.get(9)?;
    Ok(ReviewHandoff {
        version: row.get::<_, i64>(0)? as u32,
        id: row.get(1)?,
        workspace_id: row.get(2)?,
        session_id: row.get(3)?,
        paper_id: row.get(4)?,
        revision_id: row.get(5)?,
        content_hash: row.get(6)?,
        media_kind: row.get(7)?,
        staged_path: row.get(8)?,
        metadata: serde_json::from_str(&metadata).unwrap_or(json!({})),
        input_interpretation: row.get(10)?,
        external_reference: row.get(11)?,
        created_at: row.get(12)?,
        linked_at: row.get(13)?,
    })
}

fn copy_tree(
    source: &Path,
    destination: &Path,
    count: &mut usize,
    bytes: &mut u64,
) -> WorkbenchResult<()> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| WorkbenchError::storage("Failed to inspect handoff input", error))?;
    if metadata.file_type().is_symlink() {
        return Err(WorkbenchError::invalid(
            "Review handoff input contains a symbolic link",
        ));
    }
    if metadata.is_file() {
        *count += 1;
        *bytes = bytes.saturating_add(metadata.len());
        if *count > MAX_HANDOFF_ENTRIES || *bytes > MAX_HANDOFF_BYTES {
            return Err(WorkbenchError::invalid(
                "Review handoff exceeds its file or size limit",
            ));
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                WorkbenchError::storage("Failed to create handoff staging directory", error)
            })?;
        }
        fs::copy(source, destination)
            .map_err(|error| WorkbenchError::storage("Failed to stage Review handoff", error))?;
    } else if metadata.is_dir() {
        fs::create_dir_all(destination).map_err(|error| {
            WorkbenchError::storage("Failed to create handoff directory", error)
        })?;
        for entry in fs::read_dir(source)
            .map_err(|error| WorkbenchError::storage("Failed to read handoff directory", error))?
        {
            let entry = entry
                .map_err(|error| WorkbenchError::storage("Failed to read handoff entry", error))?;
            copy_tree(
                &entry.path(),
                &destination.join(entry.file_name()),
                count,
                bytes,
            )?;
        }
    } else {
        return Err(WorkbenchError::invalid(
            "Review handoff input is not a regular file or directory",
        ));
    }
    Ok(())
}

pub fn prepare_review_handoff(
    store: &Store,
    request: PrepareReviewHandoffRequest,
) -> WorkbenchResult<ReviewHandoff> {
    validate_id("operation id", &request.operation_id)?;
    validate_id("workspace id", &request.workspace_id)?;
    validate_id("paper id", &request.paper_id)?;
    if let Some(session_id) = &request.session_id {
        validate_id("session id", session_id)?;
    }
    if !request.metadata.is_object() || request.metadata.to_string().len() > 64 * 1024 {
        return Err(WorkbenchError::invalid(
            "Handoff metadata must be an object no larger than 64 KiB",
        ));
    }
    let connection = store.connection()?;
    if let Some(existing) = connection.query_row("SELECT version,id,workspace_id,session_id,paper_id,revision_id,content_hash,media_kind,staged_path,metadata_json,input_interpretation,external_reference,created_at,linked_at FROM review_handoffs WHERE operation_id=?1", [&request.operation_id], handoff_from_row).optional().map_err(|error| WorkbenchError::storage("Failed to check handoff idempotency", error))? {
        if existing.workspace_id != request.workspace_id
            || existing.session_id != request.session_id
            || existing.paper_id != request.paper_id
            || existing.metadata != request.metadata
        {
            return Err(WorkbenchError::invalid(
                "Review handoff operation id was already used for different inputs",
            ));
        }
        return Ok(existing);
    }
    if let Some(session_id) = &request.session_id {
        let belongs: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sessions WHERE id=?1 AND workspace_id=?2)",
                params![session_id, request.workspace_id],
                |row| row.get(0),
            )
            .map_err(|error| {
                WorkbenchError::storage("Failed to validate handoff conversation", error)
            })?;
        if !belongs {
            return Err(WorkbenchError::invalid(
                "Review handoff conversation is not in the selected Workspace",
            ));
        }
    }
    let (revision_id, hash, kind, reference): (String,String,String,String) = connection.query_row("SELECT pr.id,pr.content_hash,a.media_kind,a.storage_reference FROM papers p JOIN paper_revisions pr ON pr.id=p.current_revision_id JOIN artifacts a ON a.workspace_id=p.workspace_id AND a.content_hash=pr.content_hash WHERE p.id=?1 AND p.workspace_id=?2 ORDER BY CASE WHEN a.media_kind=pr.input_kind THEN 0 ELSE 1 END LIMIT 1", params![request.paper_id, request.workspace_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional().map_err(|error| WorkbenchError::storage("Failed to resolve handoff revision", error))?.ok_or_else(|| WorkbenchError::invalid("The selected paper has no immutable current revision"))?;
    let source = PathBuf::from(reference);
    if !source.starts_with(store.root_path().join("blobs")) {
        return Err(WorkbenchError::invalid(
            "Paper revision is outside the Workspace immutable blob store",
        ));
    }
    let id = new_id("review_handoff")?;
    let destination = store
        .root_path()
        .join("handoffs")
        .join(&id)
        .join(source.file_name().unwrap_or_default());
    let mut count = 0;
    let mut bytes = 0;
    copy_tree(&source, &destination, &mut count, &mut bytes)?;
    let interpretation = if source.is_dir() {
        "source_tree"
    } else {
        "document"
    };
    let timestamp = now();
    connection.execute("INSERT INTO review_handoffs (id,operation_id,version,workspace_id,session_id,paper_id,revision_id,content_hash,media_kind,staged_path,input_interpretation,metadata_json,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)", params![id,request.operation_id,HANDOFF_VERSION,request.workspace_id,request.session_id,request.paper_id,revision_id,hash,kind,destination.to_string_lossy(),interpretation,request.metadata.to_string(),timestamp]).map_err(|error| WorkbenchError::storage("Failed to record Review handoff", error))?;
    connection.query_row("SELECT version,id,workspace_id,session_id,paper_id,revision_id,content_hash,media_kind,staged_path,metadata_json,input_interpretation,external_reference,created_at,linked_at FROM review_handoffs WHERE id=?1", [id], handoff_from_row).map_err(|error| WorkbenchError::storage("Failed to read Review handoff", error))
}

pub fn link_review_handoff(
    store: &Store,
    request: LinkReviewHandoffRequest,
) -> WorkbenchResult<ReviewHandoff> {
    if request.external_reference.trim().is_empty() || request.external_reference.len() > 2048 {
        return Err(WorkbenchError::invalid(
            "External Review reference must contain 1 to 2048 bytes",
        ));
    }
    let connection = store.connection()?;
    let current: Option<String> = connection
        .query_row(
            "SELECT external_reference FROM review_handoffs WHERE id=?1",
            [&request.handoff_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to inspect handoff link", error))?
        .flatten();
    if let Some(current) = current {
        if current != request.external_reference {
            return Err(WorkbenchError::invalid(
                "Handoff is already linked to a different external reference",
            ));
        }
    }
    connection.execute("UPDATE review_handoffs SET external_reference=COALESCE(external_reference,?2), linked_at=COALESCE(linked_at,?3) WHERE id=?1", params![request.handoff_id,request.external_reference,now()]).map_err(|error| WorkbenchError::storage("Failed to link Review handoff", error))?;
    connection.query_row("SELECT version,id,workspace_id,session_id,paper_id,revision_id,content_hash,media_kind,staged_path,metadata_json,input_interpretation,external_reference,created_at,linked_at FROM review_handoffs WHERE id=?1", [&request.handoff_id], handoff_from_row).optional().map_err(|error| WorkbenchError::storage("Failed to read linked handoff", error))?.ok_or_else(|| WorkbenchError::invalid("Review handoff was not found"))
}
