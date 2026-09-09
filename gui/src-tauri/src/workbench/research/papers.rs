//! Papers owned by the Workspace research service.

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Paper {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    pub role: String,
    pub current_revision_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PaperRevision {
    pub id: String,
    pub paper_id: String,
    pub input_kind: String,
    pub entrypoint: String,
    pub dependency_manifest: Value,
    pub content_hash: String,
    pub text_reference: Option<String>,
    pub compiled_artifact_id: Option<String>,
    pub extraction: Value,
    pub capture_complete: bool,
    pub captured_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperWithRevision {
    pub paper: Paper,
    pub revision: Option<PaperRevision>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPaperRequest {
    pub workspace_id: String,
    pub paper_id: Option<String>,
    pub title: String,
    pub role: String,
    pub path: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperReadRequest {
    pub workspace_id: String,
    pub revision_id: String,
    pub start: Option<usize>,
    pub length: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperReadResult {
    pub revision_id: String,
    pub content_hash: String,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperSearchHit {
    pub revision_id: String,
    pub content_hash: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub page: Option<usize>,
    pub excerpt: String,
}

fn ensure_safe_regular_file(path: &Path) -> WorkbenchResult<u64> {
    if !path.is_absolute() {
        return Err(WorkbenchError::invalid("Paper path must be absolute"));
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| WorkbenchError::storage("Failed to inspect paper input", error))?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_IMPORT_BYTES {
        return Err(WorkbenchError::invalid(
            "Paper input must be a regular file no larger than 200 MiB",
        ));
    }
    Ok(metadata.len())
}

pub(super) fn hash_file(path: &Path) -> WorkbenchResult<(String, u64)> {
    let size = ensure_safe_regular_file(path)?;
    let mut file = fs::File::open(path)
        .map_err(|error| WorkbenchError::storage("Failed to open paper input", error))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut read = 0u64;
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| WorkbenchError::storage("Failed to hash paper input", error))?;
        if count == 0 {
            break;
        }
        read += count as u64;
        if read > MAX_IMPORT_BYTES {
            return Err(WorkbenchError::invalid(
                "Paper input changed beyond the 200 MiB limit while reading",
            ));
        }
        hasher.update(&buffer[..count]);
    }
    Ok((
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        size,
    ))
}

pub(super) fn copy_immutable(
    source: &Path,
    destination: &Path,
    expected_hash: &str,
) -> WorkbenchResult<()> {
    if destination.exists() {
        return Ok(());
    }
    let pending = destination.with_extension(format!("{}.pending", new_id("capture")?));
    let mut input = fs::File::open(source)
        .map_err(|error| WorkbenchError::storage("Failed to open paper snapshot source", error))?;
    let mut output = fs::File::create(&pending)
        .map_err(|error| WorkbenchError::storage("Failed to create paper snapshot", error))?;
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|error| WorkbenchError::storage("Failed to read paper snapshot", error))?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > MAX_IMPORT_BYTES {
            let _ = fs::remove_file(&pending);
            return Err(WorkbenchError::invalid(
                "Paper input changed beyond the 200 MiB limit while copying",
            ));
        }
        hasher.update(&buffer[..count]);
        output
            .write_all(&buffer[..count])
            .map_err(|error| WorkbenchError::storage("Failed to write paper snapshot", error))?;
    }
    output
        .sync_all()
        .map_err(|error| WorkbenchError::storage("Failed to sync paper snapshot", error))?;
    let actual: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if actual != expected_hash {
        let _ = fs::remove_file(&pending);
        return Err(WorkbenchError::invalid(
            "Paper changed while it was being captured; retry the import",
        ));
    }
    fs::rename(&pending, destination)
        .map_err(|error| WorkbenchError::storage("Failed to publish paper snapshot", error))
}

pub(super) fn extension_kind(path: &Path) -> WorkbenchResult<&'static str> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "pdf" => Ok("pdf"),
        "tex" => Ok("tex"),
        "docx" => Ok("docx"),
        "txt" | "md" | "bib" | "py" | "r" | "jl" | "do" | "json" | "csv" | "tsv" => Ok("text"),
        _ => Err(WorkbenchError::invalid(
            "Supported documents are PDF, TeX, DOCX, Markdown, text, BibTeX, research code, JSON, CSV, and TSV",
        )),
    }
}

pub(super) fn extract_import_text(
    path: &Path,
    kind: &str,
) -> WorkbenchResult<(Option<String>, Value)> {
    let result = match kind {
        "pdf" => crate::pipeline::extract::extract_pdftotext(path),
        "docx" => crate::document_bundle::extract_docx_text(path),
        _ => fs::read_to_string(path).map_err(|error| error.to_string()),
    };
    match result {
        Ok(text) if text.len() <= MAX_TEXT_BYTES => Ok((
            Some(text),
            json!({"status":"complete","method":if kind == "pdf" {"pdftotext"} else if kind == "docx" {"docx-local"} else {"utf8"},"qualityNotes":if kind == "pdf" {json!(["Text extracted deterministically with pdftotext; equation layout may be lossy."])} else {json!([])}}),
        )),
        Ok(_) => Ok((
            None,
            json!({"status":"failed","error":"Extracted text exceeded the 16 MiB Workspace limit","fallbackUsed":false}),
        )),
        Err(error) => Ok((
            None,
            json!({"status":"failed","error":error,"fallbackUsed":false}),
        )),
    }
}

struct CapturedPaperInput {
    kind: String,
    content_hash: String,
    size: u64,
    storage_reference: PathBuf,
    entrypoint: String,
    text: Option<String>,
    extraction: Value,
    manifest: Value,
}

fn capture_paper_input(store: &Store, requested: &str) -> WorkbenchResult<CapturedPaperInput> {
    let source = fs::canonicalize(requested)
        .map_err(|error| WorkbenchError::storage("Failed to resolve paper input", error))?;
    if source.is_file() {
        let kind = extension_kind(&source)?;
        let (content_hash, size) = hash_file(&source)?;
        let suffix = source
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("bin");
        let storage_reference = store
            .root_path()
            .join("blobs")
            .join(format!("{content_hash}.{suffix}"));
        copy_immutable(&source, &storage_reference, &content_hash)?;
        let (text, extraction) = extract_import_text(&storage_reference, kind)?;
        let manifest = json!({"complete":true,"files":[{"path":source,"contentHash":content_hash,"sizeBytes":size}],"unresolved":[]});
        return Ok(CapturedPaperInput {
            kind: kind.to_string(),
            content_hash,
            size,
            storage_reference,
            entrypoint: source.to_string_lossy().into_owned(),
            text,
            extraction,
            manifest,
        });
    }
    if !source.is_dir() {
        return Err(WorkbenchError::invalid(
            "Paper input must be a regular file or explicit source-tree directory",
        ));
    }
    let mut pending = vec![source.clone()];
    let mut files = Vec::<(PathBuf, PathBuf, String, u64)>::new();
    let mut unresolved = Vec::new();
    let mut total = 0u64;
    while let Some(directory) = pending.pop() {
        for item in fs::read_dir(&directory)
            .map_err(|error| WorkbenchError::storage("Failed to read source tree", error))?
        {
            let item = item
                .map_err(|error| WorkbenchError::storage("Failed to inspect source tree", error))?;
            let path = item.path();
            let name = item.file_name();
            let name = name.to_string_lossy();
            if matches!(name.as_ref(), ".git" | ".pipeline-tasks")
                || name.starts_with(".pipeline-apply-")
            {
                continue;
            }
            let metadata = fs::symlink_metadata(&path).map_err(|error| {
                WorkbenchError::storage("Failed to inspect source-tree entry", error)
            })?;
            if metadata.file_type().is_symlink() {
                unresolved.push(json!({"path":path,"reason":"symlink not followed"}));
                continue;
            }
            if metadata.is_dir() {
                pending.push(path);
                continue;
            }
            if !metadata.is_file() {
                unresolved.push(json!({"path":path,"reason":"non-regular entry"}));
                continue;
            }
            let relative = path
                .strip_prefix(&source)
                .map_err(|_| WorkbenchError::invalid("Source-tree path escaped its root"))?
                .to_path_buf();
            let (hash, size) = hash_file(&path)?;
            total = total.saturating_add(size);
            if total > MAX_IMPORT_BYTES || files.len() >= 1_000 {
                return Err(WorkbenchError::invalid(
                    "Source tree exceeds 1,000 files or 200 MiB",
                ));
            }
            files.push((relative, path, hash, size));
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    if files.is_empty() {
        return Err(WorkbenchError::invalid(
            "Source tree contains no regular files",
        ));
    }
    let mut identity = Sha256::new();
    for (relative, _, hash, size) in &files {
        identity.update(relative.to_string_lossy().as_bytes());
        identity.update([0]);
        identity.update(hash.as_bytes());
        identity.update(size.to_le_bytes());
    }
    let content_hash = identity
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let storage_reference = store
        .root_path()
        .join("blobs")
        .join(format!("source-tree-{content_hash}"));
    if !storage_reference.exists() {
        let staged = store
            .root_path()
            .join("blobs")
            .join(format!("source-tree-{}.pending", new_id("capture")?));
        fs::create_dir(&staged)
            .map_err(|error| WorkbenchError::storage("Failed to stage source tree", error))?;
        for (relative, path, hash, _) in &files {
            let target = staged.join(relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    WorkbenchError::storage("Failed to stage source-tree folder", error)
                })?;
            }
            copy_immutable(path, &target, hash)?;
        }
        fs::rename(&staged, &storage_reference).map_err(|error| {
            WorkbenchError::storage("Failed to publish source-tree snapshot", error)
        })?;
    }
    let mut text = String::new();
    for (relative, _, _, _) in &files {
        let extension = relative
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "tex" | "bib" | "md" | "txt") {
            continue;
        }
        if let Ok(body) = fs::read_to_string(storage_reference.join(relative)) {
            let addition = format!("\n\n===== {} =====\n{}", relative.to_string_lossy(), body);
            if text.len() + addition.len() > MAX_TEXT_BYTES {
                break;
            }
            text.push_str(&addition);
        }
    }
    let extraction = if text.is_empty() {
        json!({"status":"failed","error":"No supported UTF-8 text files were found in the captured source tree","fallbackUsed":false})
    } else {
        json!({"status":"complete","method":"source-tree-utf8","qualityNotes":["TeX, BibTeX, Markdown, and text files were concatenated in stable path order; generated and binary files remain available only through the immutable manifest."]})
    };
    let manifest_files = files.iter().map(|(relative, _, hash, size)| json!({"path":relative,"contentHash":hash,"sizeBytes":size})).collect::<Vec<_>>();
    Ok(CapturedPaperInput {
        kind: "source_tree".into(),
        content_hash,
        size: total,
        storage_reference,
        entrypoint: source.to_string_lossy().into_owned(),
        text: (!text.is_empty()).then_some(text),
        extraction,
        manifest: json!({"complete":unresolved.is_empty(),"files":manifest_files,"unresolved":unresolved}),
    })
}

fn paper_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Paper> {
    Ok(Paper {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        title: row.get(2)?,
        role: row.get(3)?,
        current_revision_id: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

pub(crate) fn revision_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PaperRevision> {
    let manifest: String = row.get(4)?;
    let extraction: String = row.get(8)?;
    Ok(PaperRevision {
        id: row.get(0)?,
        paper_id: row.get(1)?,
        input_kind: row.get(2)?,
        entrypoint: row.get(3)?,
        dependency_manifest: serde_json::from_str(&manifest).unwrap_or(Value::Null),
        content_hash: row.get(5)?,
        text_reference: row.get(6)?,
        compiled_artifact_id: row.get(7)?,
        extraction: serde_json::from_str(&extraction).unwrap_or(Value::Null),
        capture_complete: row.get(9)?,
        captured_at: row.get(10)?,
    })
}

pub fn import_paper(
    store: &Store,
    request: ImportPaperRequest,
) -> WorkbenchResult<PaperWithRevision> {
    validate_id("workspace id", &request.workspace_id)?;
    if !matches!(request.role.as_str(), "manuscript" | "appendix" | "other") {
        return Err(WorkbenchError::invalid(
            "Paper role must be manuscript, appendix, or other",
        ));
    }
    let title = validate_text("Paper title", &request.title, 500)?;
    let captured = capture_paper_input(store, &request.path)?;
    let text_reference = if let Some(text) = captured.text.as_ref() {
        let text_hash = hash_bytes(text.as_bytes());
        let path = store
            .root_path()
            .join("blobs")
            .join(format!("{text_hash}.txt"));
        if !path.exists() {
            fs::write(&path, text.as_bytes()).map_err(|error| {
                WorkbenchError::storage("Failed to store extracted paper text", error)
            })?;
        }
        Some(path.to_string_lossy().into_owned())
    } else {
        None
    };
    let paper_id = match request.paper_id {
        Some(id) => id,
        None => new_id("paper")?,
    };
    validate_id("paper id", &paper_id)?;
    let revision_id = new_id("paperrev")?;
    let artifact_id = new_id("artifact")?;
    let timestamp = now();
    let mut connection = open_connection(store)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| WorkbenchError::storage("Failed to start paper import", error))?;
    let existing_workspace: Option<String> = transaction
        .query_row(
            "SELECT workspace_id FROM papers WHERE id=?1",
            [&paper_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to scope paper revision", error))?;
    if existing_workspace
        .as_deref()
        .is_some_and(|workspace| workspace != request.workspace_id)
    {
        return Err(WorkbenchError::invalid(
            "Paper belongs to another workspace",
        ));
    }
    let existing: Option<String> = transaction.query_row("SELECT pr.id FROM paper_revisions pr JOIN papers p ON p.id = pr.paper_id WHERE p.workspace_id = ?1 AND pr.paper_id = ?2 AND pr.content_hash = ?3", params![request.workspace_id, paper_id, captured.content_hash], |row| row.get(0)).optional().map_err(|error| WorkbenchError::storage("Failed to inspect paper revisions", error))?;
    if let Some(existing) = existing {
        drop(transaction);
        return paper_with_revision(store, &request.workspace_id, &paper_id, Some(&existing));
    }
    transaction.execute("INSERT INTO papers (id, workspace_id, title, role, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5) ON CONFLICT(id) DO UPDATE SET title = excluded.title, role = excluded.role, updated_at = excluded.updated_at WHERE workspace_id = excluded.workspace_id", params![paper_id, request.workspace_id, title, request.role, timestamp]).map_err(|error| WorkbenchError::storage("Failed to create or update paper", error))?;
    transaction.execute("INSERT OR IGNORE INTO artifacts (id, workspace_id, content_hash, media_kind, size_bytes, origin, storage_reference, original_path, created_at) VALUES (?1, ?2, ?3, ?4, ?5, 'paper_import', ?6, ?7, ?8)", params![artifact_id, request.workspace_id, captured.content_hash, captured.kind, captured.size as i64, captured.storage_reference.to_string_lossy(), captured.entrypoint, timestamp]).map_err(|error| WorkbenchError::storage("Failed to register paper artifact", error))?;
    transaction.execute("INSERT INTO paper_revisions (id, paper_id, input_kind, entrypoint, dependency_manifest_json, content_hash, text_reference, extraction_json, capture_complete, captured_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)", params![revision_id, paper_id, captured.kind, captured.entrypoint, serde_json::to_string(&captured.manifest).unwrap_or_default(), captured.content_hash, text_reference, serde_json::to_string(&captured.extraction).unwrap_or_default(), captured.manifest.get("complete").and_then(Value::as_bool).unwrap_or(false), timestamp]).map_err(|error| WorkbenchError::storage("Failed to create paper revision", error))?;
    transaction
        .execute(
            "UPDATE papers SET current_revision_id = ?2, updated_at = ?3 WHERE id = ?1",
            params![paper_id, revision_id, timestamp],
        )
        .map_err(|error| WorkbenchError::storage("Failed to select paper revision", error))?;
    transaction.execute("UPDATE evidence_links SET freshness = 'stale', stale_reason = 'A newer paper revision was captured', updated_at = ?2 WHERE workspace_id = ?1 AND target_type = 'paper_revision' AND target_id IN (SELECT id FROM paper_revisions WHERE paper_id = ?3 AND id <> ?4) AND freshness = 'current'", params![request.workspace_id, timestamp, paper_id, revision_id]).map_err(|error| WorkbenchError::storage("Failed to propagate paper evidence freshness", error))?;
    append_change(
        &transaction,
        &request.operation_id,
        "paper_revision",
        &revision_id,
        (Some(&request.workspace_id), None),
        "created",
        &json!({"paperId":paper_id,"contentHash":captured.content_hash,"extraction":captured.extraction}),
    )?;
    transaction
        .commit()
        .map_err(|error| WorkbenchError::storage("Failed to commit paper import", error))?;
    paper_with_revision(store, &request.workspace_id, &paper_id, Some(&revision_id))
}

fn paper_with_revision(
    store: &Store,
    workspace_id: &str,
    paper_id: &str,
    revision_id: Option<&str>,
) -> WorkbenchResult<PaperWithRevision> {
    let connection = open_connection(store)?;
    let paper = connection.query_row("SELECT id, workspace_id, title, role, current_revision_id, created_at, updated_at FROM papers WHERE id = ?1 AND workspace_id = ?2", params![paper_id, workspace_id], paper_from_row).optional().map_err(|error| WorkbenchError::storage("Failed to read paper", error))?.ok_or_else(|| WorkbenchError::invalid("Paper was not found in this workspace"))?;
    let selected = revision_id.or(paper.current_revision_id.as_deref());
    let revision = selected.map(|id| connection.query_row("SELECT id, paper_id, input_kind, entrypoint, dependency_manifest_json, content_hash, text_reference, compiled_artifact_id, extraction_json, capture_complete, captured_at FROM paper_revisions WHERE id = ?1 AND paper_id = ?2", params![id, paper_id], revision_from_row).optional()).transpose().map_err(|error| WorkbenchError::storage("Failed to read paper revision", error))?.flatten();
    Ok(PaperWithRevision { paper, revision })
}

pub fn list_papers(store: &Store, workspace_id: &str) -> WorkbenchResult<Vec<PaperWithRevision>> {
    validate_id("workspace id", workspace_id)?;
    let connection = open_connection(store)?;
    let mut statement = connection.prepare("SELECT id, workspace_id, title, role, current_revision_id, created_at, updated_at FROM papers WHERE workspace_id = ?1 ORDER BY updated_at DESC LIMIT 500").map_err(|error| WorkbenchError::storage("Failed to prepare paper list", error))?;
    let papers = statement
        .query_map([workspace_id], paper_from_row)
        .map_err(|error| WorkbenchError::storage("Failed to list papers", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkbenchError::storage("Failed to decode papers", error))?;
    papers
        .into_iter()
        .map(|paper| paper_with_revision(store, workspace_id, &paper.id, None))
        .collect()
}

fn revision_text(
    store: &Store,
    workspace_id: &str,
    revision_id: &str,
) -> WorkbenchResult<(PaperRevision, String)> {
    validate_id("revision id", revision_id)?;
    let connection = open_connection(store)?;
    let revision = connection.query_row("SELECT pr.id, pr.paper_id, pr.input_kind, pr.entrypoint, pr.dependency_manifest_json, pr.content_hash, pr.text_reference, pr.compiled_artifact_id, pr.extraction_json, pr.capture_complete, pr.captured_at FROM paper_revisions pr JOIN papers p ON p.id = pr.paper_id WHERE pr.id = ?1 AND p.workspace_id = ?2", params![revision_id, workspace_id], revision_from_row).optional().map_err(|error| WorkbenchError::storage("Failed to read scoped paper revision", error))?.ok_or_else(|| WorkbenchError::invalid("Paper revision was not found in this workspace"))?;
    let reference = revision.text_reference.as_ref().ok_or_else(|| {
        WorkbenchError::invalid(
            "This paper revision has no extracted text; inspect its extraction failure",
        )
    })?;
    let path = PathBuf::from(reference);
    if !path.starts_with(store.root_path().join("blobs")) {
        return Err(WorkbenchError::invalid(
            "Paper text reference escaped Workspace storage",
        ));
    }
    let bytes = fs::read(&path)
        .map_err(|error| WorkbenchError::storage("Failed to read immutable paper text", error))?;
    if bytes.len() > MAX_TEXT_BYTES {
        return Err(WorkbenchError::invalid(
            "Stored paper text exceeds the read limit",
        ));
    }
    let text = String::from_utf8(bytes)
        .map_err(|error| WorkbenchError::storage("Stored paper text is not UTF-8", error))?;
    Ok((revision, text))
}

pub fn paper_read(store: &Store, request: PaperReadRequest) -> WorkbenchResult<PaperReadResult> {
    validate_id("workspace id", &request.workspace_id)?;
    let (revision, text) = revision_text(store, &request.workspace_id, &request.revision_id)?;
    let start = request.start.unwrap_or(0).min(text.len());
    if !text.is_char_boundary(start) {
        return Err(WorkbenchError::invalid(
            "Paper read start must be a UTF-8 boundary",
        ));
    }
    let requested = request.length.unwrap_or(32 * 1024).min(64 * 1024);
    let mut end = start.saturating_add(requested).min(text.len());
    while end > start && !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok(PaperReadResult {
        revision_id: revision.id,
        content_hash: revision.content_hash,
        start,
        end,
        text: text[start..end].to_string(),
    })
}

pub fn paper_search(
    store: &Store,
    workspace_id: &str,
    revision_id: &str,
    query: &str,
    limit: usize,
) -> WorkbenchResult<Vec<PaperSearchHit>> {
    let query = validate_text("Paper search query", query, 1_000)?;
    let (revision, text) = revision_text(store, workspace_id, revision_id)?;
    let haystack = text.to_ascii_lowercase();
    let needle = query.to_ascii_lowercase();
    let mut hits = Vec::new();
    let mut cursor = 0;
    let limit = limit.clamp(1, MAX_SEARCH_RESULTS);
    while hits.len() < limit {
        let Some(relative) = haystack[cursor..].find(&needle) else {
            break;
        };
        let start = cursor + relative;
        let end = start + needle.len();
        let mut excerpt_start = text[..start]
            .rfind('\n')
            .map_or(start.saturating_sub(160), |value| value + 1);
        let mut excerpt_end = text[end..]
            .find('\n')
            .map_or((end + 160).min(text.len()), |value| end + value);
        while !text.is_char_boundary(excerpt_start) {
            excerpt_start += 1;
        }
        while !text.is_char_boundary(excerpt_end) {
            excerpt_end -= 1;
        }
        let line = text[..start].bytes().filter(|byte| *byte == b'\n').count() + 1;
        let page_count = text[..start].bytes().filter(|byte| *byte == 0x0c).count();
        hits.push(PaperSearchHit {
            revision_id: revision.id.clone(),
            content_hash: revision.content_hash.clone(),
            start,
            end,
            line,
            page: (revision.input_kind == "pdf").then_some(page_count + 1),
            excerpt: text[excerpt_start..excerpt_end].trim().to_string(),
        });
        cursor = end;
    }
    Ok(hits)
}
