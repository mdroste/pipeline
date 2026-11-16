//! Document metadata and version history. File formats are independent of types.
use super::*;

pub(crate) fn legacy_document_type(role: &str) -> &'static str {
    match role {
        "manuscript" => "paper",
        "appendix" => "appendix",
        _ => "other",
    }
}
pub(crate) fn document_role(kind: &str) -> &'static str {
    match kind {
        "paper" => "manuscript",
        "appendix" => "appendix",
        _ => "other",
    }
}
pub(crate) fn validate_document_type(kind: &str) -> WorkbenchResult<()> {
    if !matches!(kind, "paper" | "slides" | "appendix" | "response" | "other") {
        return Err(WorkbenchError::invalid("Choose a supported document type"));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateDocumentRequest {
    pub workspace_id: String,
    pub paper_id: String,
    pub expected_updated_at: String,
    pub title: String,
    pub document_type: String,
    pub pinned: bool,
    pub archived: bool,
    pub current_revision_id: Option<String>,
    pub operation_id: String,
}

pub fn update_document(
    store: &Store,
    request: UpdateDocumentRequest,
) -> WorkbenchResult<PaperWithRevision> {
    validate_id("workspace id", &request.workspace_id)?;
    validate_id("document id", &request.paper_id)?;
    validate_document_type(&request.document_type)?;
    let title = validate_text("Document title", &request.title, 500)?;
    let mut connection = open_connection(store)?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| WorkbenchError::storage("Failed to update document", e))?;
    if let Some(revision) = &request.current_revision_id {
        let belongs: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM paper_revisions r JOIN papers p ON p.id=r.paper_id WHERE r.id=?1 AND p.id=?2 AND p.workspace_id=?3)",
            params![revision, request.paper_id, request.workspace_id], |r| r.get(0),
        ).map_err(|e| WorkbenchError::storage("Failed to check document version", e))?;
        if !belongs {
            return Err(WorkbenchError::invalid(
                "This version belongs to another document",
            ));
        }
    }
    // Nanosecond precision makes two metadata edits distinguishable even within a millisecond.
    let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true);
    let changed = tx.execute(
        "UPDATE papers SET title=?4,document_type=?5,role=?6,pinned=?7,archived=?8,current_revision_id=COALESCE(?9,current_revision_id),updated_at=?10 WHERE id=?1 AND workspace_id=?2 AND updated_at=?3",
        params![request.paper_id, request.workspace_id, request.expected_updated_at, title,
            request.document_type, document_role(&request.document_type), request.pinned,
            request.archived, request.current_revision_id, timestamp],
    ).map_err(|e| WorkbenchError::storage("Failed to save document", e))?;
    if changed != 1 {
        return Err(WorkbenchError::conflict(
            "The document changed; refresh and try again",
        ));
    }
    if let Some(revision) = &request.current_revision_id {
        tx.execute("UPDATE evidence_links SET freshness='stale',stale_reason='Another document version was selected',updated_at=?3 WHERE workspace_id=?1 AND target_type='paper_revision' AND target_id IN (SELECT id FROM paper_revisions WHERE paper_id=?2 AND id<>?4) AND freshness='current'",
            params![request.workspace_id, request.paper_id, timestamp, revision])
            .map_err(|e| WorkbenchError::storage("Failed to update document evidence", e))?;
    }
    append_change(
        &tx,
        &request.operation_id,
        "paper",
        &request.paper_id,
        (Some(&request.workspace_id), None),
        "updated",
        &json!({"title":title,"documentType":request.document_type,"pinned":request.pinned,"archived":request.archived,"currentRevisionId":request.current_revision_id}),
    )?;
    tx.commit()
        .map_err(|e| WorkbenchError::storage("Failed to commit document", e))?;
    papers::paper_with_revision(store, &request.workspace_id, &request.paper_id, None)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentVersion {
    pub id: String,
    pub paper_id: String,
    pub input_kind: String,
    pub captured_at: String,
}

pub fn document_versions(
    store: &Store,
    workspace_id: &str,
    paper_id: &str,
) -> WorkbenchResult<Vec<DocumentVersion>> {
    validate_id("workspace id", workspace_id)?;
    validate_id("document id", paper_id)?;
    if papers::paper_by_id(store, workspace_id, paper_id)?.is_none() {
        return Err(WorkbenchError::invalid(
            "Document was not found in this project",
        ));
    }
    let connection = open_connection(store)?;
    let mut statement = connection.prepare(
        "SELECT r.id,r.paper_id,r.input_kind,r.captured_at FROM paper_revisions r JOIN papers p ON p.id=r.paper_id WHERE p.id=?1 AND p.workspace_id=?2 ORDER BY r.captured_at DESC,r.id DESC"
    ).map_err(|e| WorkbenchError::storage("Failed to read document history", e))?;
    let rows = statement
        .query_map(params![paper_id, workspace_id], |row| {
            Ok(DocumentVersion {
                id: row.get(0)?,
                paper_id: row.get(1)?,
                input_kind: row.get(2)?,
                captured_at: row.get(3)?,
            })
        })
        .map_err(|e| WorkbenchError::storage("Failed to list document versions", e))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| WorkbenchError::storage("Failed to read document versions", e))
}
