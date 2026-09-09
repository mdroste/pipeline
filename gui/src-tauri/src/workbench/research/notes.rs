//! Notes owned by the Workspace research service.

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResearchNote {
    pub id: String,
    pub workspace_id: String,
    pub paper_id: Option<String>,
    pub kind: String,
    pub body: String,
    pub state: String,
    pub origin: String,
    pub pinned: bool,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateNoteRequest {
    pub workspace_id: String,
    pub paper_id: Option<String>,
    pub kind: String,
    pub body: String,
    pub state: Option<String>,
    pub origin: String,
    pub pinned: bool,
    pub operation_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNoteRequest {
    pub note_id: String,
    pub expected_revision: i64,
    pub body: Option<String>,
    pub state: Option<String>,
    pub pinned: Option<bool>,
    pub operation_id: String,
}

fn note_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResearchNote> {
    Ok(ResearchNote {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        paper_id: row.get(2)?,
        kind: row.get(3)?,
        body: row.get(4)?,
        state: row.get(5)?,
        origin: row.get(6)?,
        pinned: row.get(7)?,
        revision: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn validate_note_kind(kind: &str) -> WorkbenchResult<()> {
    if !matches!(
        kind,
        "question" | "assumption" | "decision" | "next_step" | "notation" | "handoff"
    ) {
        return Err(WorkbenchError::invalid("Unknown research note kind"));
    }
    Ok(())
}

fn validate_note_state(state: &str) -> WorkbenchResult<()> {
    if !matches!(state, "proposed" | "accepted" | "rejected" | "retired") {
        return Err(WorkbenchError::invalid("Unknown research note state"));
    }
    Ok(())
}

pub fn create_note(
    store: &Store,
    request: CreateNoteRequest,
    model_origin: bool,
) -> WorkbenchResult<ResearchNote> {
    validate_id("workspace id", &request.workspace_id)?;
    validate_note_kind(&request.kind)?;
    let body = validate_text("Research note", &request.body, 64 * 1024)?;
    let state = request.state.as_deref().unwrap_or("proposed");
    validate_note_state(state)?;
    if model_origin && state != "proposed" {
        return Err(WorkbenchError::invalid(
            "Model-created notes must remain proposals until a user acts",
        ));
    }
    let origin = validate_text("Note origin", &request.origin, 300)?;
    let id = new_id("note")?;
    let timestamp = now();
    let mut connection = open_connection(store)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| WorkbenchError::storage("Failed to start note creation", error))?;
    transaction.execute("INSERT INTO research_notes (id, workspace_id, paper_id, kind, body, state, origin, pinned, revision, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9, ?9)", params![id, request.workspace_id, request.paper_id, request.kind, body, state, origin, request.pinned, timestamp]).map_err(|error| WorkbenchError::storage("Failed to create research note", error))?;
    append_change(
        &transaction,
        &request.operation_id,
        "research_note",
        &id,
        (Some(&request.workspace_id), None),
        "created",
        &json!({"state":state,"modelOrigin":model_origin}),
    )?;
    transaction
        .commit()
        .map_err(|error| WorkbenchError::storage("Failed to commit research note", error))?;
    get_note(store, &id)
}

pub(crate) fn get_note(store: &Store, note_id: &str) -> WorkbenchResult<ResearchNote> {
    open_connection(store)?.query_row("SELECT id, workspace_id, paper_id, kind, body, state, origin, pinned, revision, created_at, updated_at FROM research_notes WHERE id = ?1", [note_id], note_from_row).optional().map_err(|error| WorkbenchError::storage("Failed to read research note", error))?.ok_or_else(|| WorkbenchError::invalid("Research note was not found"))
}

pub fn list_notes(
    store: &Store,
    workspace_id: &str,
    include_rejected: bool,
) -> WorkbenchResult<Vec<ResearchNote>> {
    validate_id("workspace id", workspace_id)?;
    let connection = open_connection(store)?;
    let sql = if include_rejected {
        "SELECT id, workspace_id, paper_id, kind, body, state, origin, pinned, revision, created_at, updated_at FROM research_notes WHERE workspace_id = ?1 ORDER BY pinned DESC, updated_at DESC LIMIT 500"
    } else {
        "SELECT id, workspace_id, paper_id, kind, body, state, origin, pinned, revision, created_at, updated_at FROM research_notes WHERE workspace_id = ?1 AND state NOT IN ('rejected','retired') ORDER BY pinned DESC, updated_at DESC LIMIT 500"
    };
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| WorkbenchError::storage("Failed to prepare research note list", error))?;
    let notes = statement
        .query_map([workspace_id], note_from_row)
        .map_err(|error| WorkbenchError::storage("Failed to list research notes", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkbenchError::storage("Failed to decode research notes", error))?;
    Ok(notes)
}

pub fn update_note(store: &Store, request: UpdateNoteRequest) -> WorkbenchResult<ResearchNote> {
    validate_id("note id", &request.note_id)?;
    if let Some(state) = request.state.as_deref() {
        validate_note_state(state)?;
    }
    let body = request
        .body
        .as_deref()
        .map(|value| validate_text("Research note", value, 64 * 1024))
        .transpose()?;
    let mut connection = open_connection(store)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| WorkbenchError::storage("Failed to start note update", error))?;
    let workspace_id: String = transaction
        .query_row(
            "SELECT workspace_id FROM research_notes WHERE id = ?1 AND revision = ?2",
            params![request.note_id, request.expected_revision],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to inspect research note", error))?
        .ok_or_else(|| WorkbenchError::invalid("Research note changed; refresh before editing"))?;
    let previous = transaction.query_row("SELECT id, workspace_id, paper_id, kind, body, state, origin, pinned, revision, created_at, updated_at FROM research_notes WHERE id=?1", [&request.note_id], note_from_row).map_err(|e| WorkbenchError::storage("Failed to preserve note revision", e))?;
    transaction.execute("UPDATE research_notes SET body = COALESCE(?2, body), state = COALESCE(?3, state), pinned = COALESCE(?4, pinned), revision = revision + 1, updated_at = ?5 WHERE id = ?1", params![request.note_id, body, request.state, request.pinned, now()]).map_err(|error| WorkbenchError::storage("Failed to update research note", error))?;
    append_change(
        &transaction,
        &request.operation_id,
        "research_note",
        &request.note_id,
        (Some(&workspace_id), None),
        "updated",
        &json!({"previous":previous,"body":body,"state":request.state,"pinned":request.pinned,"revision":request.expected_revision+1}),
    )?;
    transaction
        .commit()
        .map_err(|error| WorkbenchError::storage("Failed to commit research note update", error))?;
    get_note(store, &request.note_id)
}
