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
    crate::workbench::store::require_available("This paper's saved file", &reference)?;
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
    let interpretation = folder_interpretation(&destination);
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

fn folder_interpretation(path: &std::path::Path) -> &'static str {
    if !path.is_dir() {
        "document"
    } else if crate::pipeline::extract::find_main_tex(path).is_some() {
        "latex_project"
    } else {
        "source_tree"
    }
}

#[cfg(test)]
mod folder_tests {
    use super::*;
    #[test]
    fn staged_folder_uses_its_actual_paper_type() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(folder_interpretation(root.path()), "source_tree");
        std::fs::write(root.path().join("main.tex"), r"\documentclass{article}").unwrap();
        assert_eq!(folder_interpretation(root.path()), "latex_project");
    }
}

/// A Review accepts this project reference only with the input actually staged
/// by the Workspace. Batch selections can select documents below a staged tree.
pub(crate) fn validate_handoff_inputs(
    store: &Store,
    id: &str,
    paths: &[String],
) -> WorkbenchResult<()> {
    validate_id("handoff id", id)?;
    let path: String = store
        .connection()?
        .query_row(
            "SELECT staged_path FROM review_handoffs WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| WorkbenchError::storage("Failed to read project Review handoff", e))?
        .ok_or_else(|| {
            WorkbenchError::invalid("The selected project Review handoff no longer exists")
        })?;
    let staged = Path::new(&path);
    let folder = store.root_path().join("handoffs").join(id);
    if staged.parent() != Some(folder.as_path()) || paths.is_empty() {
        return Err(WorkbenchError::invalid(
            "The Review handoff has no staged input here",
        ));
    }
    let staged = fs::canonicalize(staged)
        .map_err(|e| WorkbenchError::storage("Cannot resolve staged handoff input", e))?;
    for selected in paths {
        let source = fs::canonicalize(selected)
            .map_err(|e| WorkbenchError::storage("Cannot resolve selected Review input", e))?;
        if !source.starts_with(&staged) {
            return Err(WorkbenchError::invalid(
                "The selected file does not belong to this project Review handoff",
            ));
        }
    }
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectReviewLink {
    id: String,
    staged_path: String,
    external_reference: Option<String>,
    content_hash: String,
}

pub fn project_review_links(
    store: &Store,
    workspace_id: &str,
    after: Option<String>,
) -> WorkbenchResult<Vec<ProjectReviewLink>> {
    validate_id("workspace id", workspace_id)?;
    let connection = store.connection()?;
    let mut stmt=connection.prepare("SELECT id,staged_path,external_reference,content_hash FROM review_handoffs WHERE workspace_id=?1 AND id>?2 ORDER BY id LIMIT 200")
        .map_err(|e|WorkbenchError::storage("Failed to load project Review links",e))?;
    let rows = stmt
        .query_map(params![workspace_id, after.as_deref().unwrap_or("")], |r| {
            Ok(ProjectReviewLink {
                id: r.get(0)?,
                staged_path: r.get(1)?,
                external_reference: r.get(2)?,
                content_hash: r.get(3)?,
            })
        })
        .map_err(|e| WorkbenchError::storage("Failed to read project Review links", e))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| WorkbenchError::storage("Invalid Review link", e))
}
