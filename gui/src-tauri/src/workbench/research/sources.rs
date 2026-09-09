//! Sources owned by the Workspace research service.

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceRecord {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    pub citation_key: Option<String>,
    pub identifiers: Value,
    pub version_id: String,
    pub version_label: Option<String>,
    pub locator: Option<String>,
    pub access_state: String,
    pub acquired_via: String,
    pub accessed_at: Option<String>,
    pub content_hash: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSourceRequest {
    pub workspace_id: String,
    pub title: String,
    pub citation_key: Option<String>,
    pub identifiers: Value,
    pub version_label: Option<String>,
    pub path: Option<String>,
    pub locator: Option<String>,
    pub access_state: String,
    pub acquired_via: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceImportResult {
    pub source: SourceRecord,
    pub duplicate_candidates: Vec<SourceRecord>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceReadResult {
    pub source_version_id: String,
    pub content_hash: String,
    pub access_state: String,
    pub acquired_via: String,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

pub fn source_read(
    store: &Store,
    workspace_id: &str,
    source_version_id: &str,
    start: usize,
    length: usize,
) -> WorkbenchResult<SourceReadResult> {
    validate_id("workspace id", workspace_id)?;
    validate_id("source version id", source_version_id)?;
    let connection = open_connection(store)?;
    let (content_hash, access_state, acquired_via, reference): (Option<String>, String, String, Option<String>) = connection.query_row("SELECT v.content_hash, v.access_state, v.acquired_via, v.text_reference FROM source_versions v JOIN sources s ON s.id=v.source_id WHERE v.id=?1 AND s.workspace_id=?2", params![source_version_id, workspace_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional().map_err(|error| WorkbenchError::storage("Failed to resolve source version", error))?.ok_or_else(|| WorkbenchError::invalid("Source version was not found in this workspace"))?;
    let reference = reference.ok_or_else(|| {
        WorkbenchError::invalid("This source version has no captured readable text")
    })?;
    let path = PathBuf::from(reference);
    if !path.starts_with(store.root_path().join("blobs")) {
        return Err(WorkbenchError::invalid(
            "Source text reference escaped Workspace storage",
        ));
    }
    let text = fs::read_to_string(path)
        .map_err(|error| WorkbenchError::storage("Failed to read captured source text", error))?;
    let start = start.min(text.len());
    if !text.is_char_boundary(start) {
        return Err(WorkbenchError::invalid(
            "Source read start must be a UTF-8 boundary",
        ));
    }
    let mut end = start.saturating_add(length.min(64 * 1024)).min(text.len());
    while end > start && !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok(SourceReadResult {
        source_version_id: source_version_id.to_string(),
        content_hash: content_hash.ok_or_else(|| {
            WorkbenchError::invalid("Captured source version has no dependency hash")
        })?,
        access_state,
        acquired_via,
        start,
        end,
        text: text[start..end].to_string(),
    })
}

fn source_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SourceRecord> {
    let identifiers: String = row.get(4)?;
    Ok(SourceRecord {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        title: row.get(2)?,
        citation_key: row.get(3)?,
        identifiers: serde_json::from_str(&identifiers).unwrap_or_else(|_| json!({})),
        version_id: row.get(5)?,
        version_label: row.get(6)?,
        locator: row.get(7)?,
        access_state: row.get(8)?,
        acquired_via: row.get(9)?,
        accessed_at: row.get(10)?,
        content_hash: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

const SOURCE_SELECT: &str = "SELECT s.id, s.workspace_id, s.title, s.citation_key, s.identifiers_json, v.id, v.version_label, v.locator, v.access_state, v.acquired_via, v.accessed_at, v.content_hash, s.created_at, s.updated_at FROM sources s JOIN source_versions v ON v.id = (SELECT id FROM source_versions WHERE source_id=s.id ORDER BY created_at DESC, id DESC LIMIT 1)";

pub fn list_sources(store: &Store, workspace_id: &str) -> WorkbenchResult<Vec<SourceRecord>> {
    validate_id("workspace id", workspace_id)?;
    let connection = open_connection(store)?;
    let mut statement = connection
        .prepare(&format!(
            "{SOURCE_SELECT} WHERE s.workspace_id=?1 ORDER BY s.updated_at DESC LIMIT 500"
        ))
        .map_err(|error| WorkbenchError::storage("Failed to prepare source list", error))?;
    let sources = statement
        .query_map([workspace_id], source_from_row)
        .map_err(|error| WorkbenchError::storage("Failed to list sources", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkbenchError::storage("Failed to decode sources", error))?;
    Ok(sources)
}

pub(crate) fn source_by_id(store: &Store, ws: &str, id: &str) -> WorkbenchResult<SourceRecord> {
    open_connection(store)?
        .query_row(
            &format!("{SOURCE_SELECT} WHERE s.id=?1 AND s.workspace_id=?2"),
            params![id, ws],
            source_from_row,
        )
        .optional()
        .map_err(|e| WorkbenchError::storage("Source identity", e))?
        .ok_or_else(|| WorkbenchError::invalid("Source is unavailable in this project"))
}

pub fn import_source(
    store: &Store,
    request: ImportSourceRequest,
) -> WorkbenchResult<SourceImportResult> {
    validate_id("workspace id", &request.workspace_id)?;
    let title = validate_text("Source title", &request.title, 1_000)?;
    if !matches!(
        request.access_state.as_str(),
        "metadata" | "abstract" | "partial" | "full" | "unavailable"
    ) {
        return Err(WorkbenchError::invalid("Unknown source access state"));
    }
    if !matches!(
        request.acquired_via.as_str(),
        "local_pdf" | "local_bibtex" | "local_file" | "web" | "manual"
    ) {
        return Err(WorkbenchError::invalid(
            "Unknown source acquisition provenance",
        ));
    }
    let (identifiers, identifiers_json) =
        json_object("Source identifiers", request.identifiers, 32 * 1024)?;
    let citation_key = request
        .citation_key
        .as_deref()
        .map(|value| validate_text("Citation key", value, 300))
        .transpose()?;
    let locator = request
        .locator
        .as_deref()
        .map(|value| validate_text("Source locator", value, 4_096))
        .transpose()?;
    let mut access_state = request.access_state.clone();
    if request.path.is_none() && access_state != "unavailable" {
        access_state = "metadata".into();
    }
    let mut content_hash = None;
    let mut text_reference = None;
    let mut effective_locator = locator;
    if let Some(path) = request.path.as_deref() {
        let source = fs::canonicalize(path)
            .map_err(|error| WorkbenchError::storage("Failed to resolve source input", error))?;
        let kind = extension_kind(&source)?;
        let (hash, size) = hash_file(&source)?;
        let suffix = source
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("bin");
        let captured = store
            .root_path()
            .join("blobs")
            .join(format!("{hash}.{suffix}"));
        copy_immutable(&source, &captured, &hash)?;
        open_connection(store)?.execute("INSERT OR IGNORE INTO artifacts(id,workspace_id,content_hash,media_kind,size_bytes,origin,storage_reference,created_at) VALUES(?1,?2,?3,?4,?5,'source_acquisition',?6,?7)",params![new_id("sourceartifact")?,request.workspace_id,hash,suffix,size as i64,captured.to_string_lossy(),now()]).map_err(|e|WorkbenchError::storage("Retain captured source bytes",e))?;
        let (text, _) = extract_import_text(&captured, kind)?;
        if text.is_none() {
            access_state = "unavailable".into();
        }
        if let Some(text) = text {
            let text_hash = hash_bytes(text.as_bytes());
            let path = store
                .root_path()
                .join("blobs")
                .join(format!("{text_hash}.txt"));
            if !path.exists() {
                fs::write(&path, text.as_bytes()).map_err(|error| {
                    WorkbenchError::storage("Failed to store source text", error)
                })?;
            }
            text_reference = Some(path.to_string_lossy().into_owned());
        }
        content_hash = Some(hash);
        effective_locator = Some(source.to_string_lossy().into_owned());
    }
    let existing = list_sources(store, &request.workspace_id)?;
    let normalized_title = title.to_lowercase();
    let duplicate_candidates = existing
        .into_iter()
        .filter(|candidate| {
            candidate.title.to_lowercase() == normalized_title
                || candidate.identifiers.as_object().is_some_and(|old| {
                    identifiers.as_object().is_some_and(|new| {
                        old.iter().any(|(key, value)| new.get(key) == Some(value))
                    })
                })
        })
        .collect::<Vec<_>>();
    let source_id = new_id("source")?;
    let version_id = new_id("sourcever")?;
    let timestamp = now();
    let mut connection = open_connection(store)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| WorkbenchError::storage("Failed to start source import", error))?;
    transaction.execute("INSERT INTO sources (id, workspace_id, title, citation_key, identifiers_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)", params![source_id, request.workspace_id, title, citation_key, identifiers_json, timestamp]).map_err(|error| WorkbenchError::storage("Failed to create source", error))?;
    transaction.execute("INSERT INTO source_versions (id, source_id, version_label, locator, access_state, acquired_via, accessed_at, content_hash, text_reference, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?7)", params![version_id, source_id, request.version_label, effective_locator, access_state, request.acquired_via, timestamp, content_hash, text_reference]).map_err(|error| WorkbenchError::storage("Failed to create source version", error))?;
    append_change(
        &transaction,
        &request.operation_id,
        "source",
        &source_id,
        (Some(&request.workspace_id), None),
        "created",
        &json!({"versionId":version_id,"duplicateCandidates":duplicate_candidates.iter().map(|candidate| candidate.id.as_str()).collect::<Vec<_>>(),"accessState":access_state,"acquiredVia":request.acquired_via}),
    )?;
    transaction
        .commit()
        .map_err(|error| WorkbenchError::storage("Failed to commit source import", error))?;
    let source = open_connection(store)?
        .query_row(
            &format!("{SOURCE_SELECT} WHERE s.id=?1"),
            [&source_id],
            source_from_row,
        )
        .map_err(|error| WorkbenchError::storage("Failed to read imported source", error))?;
    Ok(SourceImportResult {
        source,
        duplicate_candidates,
    })
}
