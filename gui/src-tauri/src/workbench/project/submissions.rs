//! Named immutable milestones and ordinary, human-readable handoff bundles.
use super::*;
use crate::workbench::desk::{self, DeskRecord};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmissionRequest {
    pub title: String,
    pub handoff: String,
    pub files: BTreeMap<String, String>,
    pub revisions: Vec<String>,
    pub records: Vec<SubmissionRecord>,
    pub executions: Vec<String>,
    pub operation_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmissionRecord {
    pub id: String,
    pub kind: String,
    pub revision: i64,
}

fn retain(
    store: &Store,
    ws: &str,
    output: &mut Vec<Value>,
    path: String,
    bytes: &[u8],
    origin: Value,
) -> WorkbenchResult<()> {
    files::relative(&path)?;
    let size: u64 = output.iter().filter_map(|file| file["size"].as_u64()).sum();
    if size + bytes.len() as u64 > 128 * 1024 * 1024 {
        return Err(WorkbenchError::invalid("Submission exceeds 128 MiB"));
    }
    let digest = blob(store, ws, bytes, "submission")?;
    output.push(json!({"path":path,"hash":digest,"size":bytes.len(),"origin":origin}));
    Ok(())
}

pub fn capture(store: &Store, ws: &str, request: SubmissionRequest) -> WorkbenchResult<DeskRecord> {
    scope(store, ws)?;
    let _guard = lock(store, ws)?;
    let request_hash = hash(serde_json::to_string(&request).map_err(err)?.as_bytes());
    if let Some(old) = desk::operation_record(store, ws, &request.operation_id)? {
        if old.kind == "submission" && old.body["requestHash"] == request_hash {
            return Ok(old);
        }
        return Err(WorkbenchError::conflict(
            "Submission operation was reused with different content",
        ));
    }
    bounded(&request.title, 200)?;
    bounded(&request.handoff, 32 * 1024)?;
    if request.title.trim().is_empty()
        || request.files.len() > 200
        || request.revisions.len() > 30
        || request.records.len() > 100
        || request.executions.len() > 30
    {
        return Err(WorkbenchError::invalid("Name the milestone and select at most 200 files, 30 documents, 100 records, and 30 runs"));
    }
    let mut output = Vec::new();
    if !request.files.is_empty() {
        let safe = files::SafeRoot::open(&root(store, ws)?)?;
        for (path, expected) in &request.files {
            let bytes = safe.read(path)?;
            if hash(&bytes) != *expected {
                return Err(WorkbenchError::conflict(format!(
                    "{path} changed since selection. Refresh files and select it again."
                )));
            }
            retain(
                store,
                ws,
                &mut output,
                format!("files/{path}"),
                &bytes,
                json!({"workingFile":path,"hash":expected}),
            )?;
        }
    }
    let mut revisions = Vec::new();
    for (index, id) in request.revisions.iter().enumerate() {
        let revision = documents::revision(store, ws, id)?;
        if revision.input_kind != "source_tree" {
            let reference: Option<String> = store.connection()?.query_row("SELECT storage_reference FROM artifacts WHERE workspace_id=?1 AND content_hash=?2 ORDER BY created_at LIMIT 1", params![ws,revision.content_hash], |row| row.get(0)).optional().map_err(err)?;
            if let Some(reference) = reference {
                let path = Path::new(&reference);
                if path.parent() != Some(store.root_path().join("blobs").as_path()) {
                    return Err(WorkbenchError::invalid(
                        "Document artifact is outside private storage",
                    ));
                }
                let mut bytes = Vec::new();
                crate::safety::open_regular_file(path)
                    .map_err(err)?
                    .take(MAX_FILE + 1)
                    .read_to_end(&mut bytes)
                    .map_err(err)?;
                if hash(&bytes) != revision.content_hash {
                    return Err(WorkbenchError::invalid(
                        "Saved document failed its integrity check",
                    ));
                }
                let extension = match revision.input_kind.as_str() {
                    "pdf" => "pdf",
                    "docx" => "docx",
                    "latex" | "tex" => "tex",
                    "markdown" => "md",
                    _ => "txt",
                };
                retain(
                    store,
                    ws,
                    &mut output,
                    format!("documents/{}.{extension}", index + 1),
                    &bytes,
                    json!({"revisionId":id,"hash":revision.content_hash}),
                )?;
            } else {
                return Err(WorkbenchError::invalid(
                    "Saved document artifact is unavailable; choose another version",
                ));
            }
        }
        revisions.push(revision);
    }
    let mut evidence = Vec::new();
    for selected in &request.records {
        if ![
            "build",
            "response",
            "application",
            "check",
            "theory",
            "literature",
        ]
        .contains(&selected.kind.as_str())
        {
            return Err(WorkbenchError::invalid(
                "Unsupported submission evidence kind",
            ));
        }
        let r = record(store, ws, &selected.id, &selected.kind)?;
        if r.revision != selected.revision {
            return Err(WorkbenchError::conflict(
                "A selected evidence record changed; reload the preview",
            ));
        }
        evidence.push(serde_json::to_value(r).map_err(err)?);
    }
    let mut executions = Vec::new();
    for id in &request.executions {
        let run = research::get_execution(store, id)?;
        if run.workspace_id != ws {
            return Err(WorkbenchError::invalid("Run belongs to another project"));
        }
        if matches!(run.outcome.as_str(), "queued" | "running") {
            return Err(WorkbenchError::invalid("Wait for selected runs to finish"));
        }
        executions.push(json!({"id":run.id,"command":run.command,"outcome":run.outcome,"dependencyHash":run.dependency_hash,"inputs":run.input_manifest,"outputs":run.output_manifest,"validation":run.validation,"createdAt":run.created_at}));
    }
    let mut review_references = Vec::new();
    for revision in &request.revisions {
        let connection = store.connection()?;
        let mut query = connection.prepare("SELECT json_object('handoffId',id,'revisionId',revision_id,'contentHash',content_hash,'reviewRunId',external_reference,'createdAt',created_at,'linkedAt',linked_at) FROM review_handoffs WHERE workspace_id=?1 AND revision_id=?2 ORDER BY created_at DESC LIMIT 100").map_err(err)?;
        for value in query
            .query_map(params![ws, revision], |row| row.get::<_, String>(0))
            .map_err(err)?
        {
            review_references
                .push(serde_json::from_str::<Value>(&value.map_err(err)?).map_err(err)?);
        }
    }
    let evidence = json!({"documentRevisions":revisions,"reviewReferences":review_references,"records":evidence,"executions":executions});
    retain(
        store,
        ws,
        &mut output,
        "evidence.json".into(),
        &serde_json::to_vec_pretty(&evidence).map_err(err)?,
        json!({"type":"selected exact records"}),
    )?;
    let readme = format!("# {}\n\nCaptured {}. This milestone is a researcher-created record, not proof of submission to a service.\n\n{}\n\n## Package\n\nOrdinary files are in files/ and saved documents in documents/. evidence.json contains selected exact records; manifest.json lists SHA-256 hashes. Source-tree revisions are references: select required source files separately. Working files and saved revisions can differ; their hashes and origins are separate. Unsaved buffers are excluded. Checks cover declared inputs, not scientific correctness.\n\n## Included files\n\n{}\n", request.title, now(), request.handoff, output.iter().map(|file| format!("- {} ({} bytes)", file["path"].as_str().unwrap_or(""), file["size"])).collect::<Vec<_>>().join("\n"));
    retain(
        store,
        ws,
        &mut output,
        "README.md".into(),
        readme.as_bytes(),
        json!({"type":"researcher handoff"}),
    )?;
    desk::insert(
        store,
        ws,
        "submission",
        &request.title,
        json!({"format":"pipeline-submission","version":1,"requestHash":request_hash,"handoff":request.handoff,"files":output,"revisions":request.revisions,"capturedAt":now()}),
        None,
        &request.operation_id,
    )
}

pub fn export(
    store: &Store,
    ws: &str,
    id: &str,
    expected_hash: &str,
    destination: &str,
) -> WorkbenchResult<Value> {
    let snapshot = desk::record(store, ws, id)?;
    if snapshot.kind != "submission" || snapshot.content_hash != expected_hash {
        return Err(WorkbenchError::conflict(
            "Choose the exact submission snapshot shown in the preview",
        ));
    }
    let target = Path::new(destination);
    let parent = target
        .parent()
        .ok_or_else(|| WorkbenchError::invalid("Choose a destination folder"))?
        .canonicalize()
        .map_err(err)?;
    if parent.starts_with(store.root_path()) {
        return Err(WorkbenchError::invalid(
            "Export outside Pipeline's private data folder",
        ));
    }
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(err)?;
    {
        let mut archive = zip::ZipWriter::new(temp.as_file_mut());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for file in snapshot.body["files"]
            .as_array()
            .ok_or_else(|| WorkbenchError::invalid("Submission file manifest is missing"))?
        {
            let path = file["path"]
                .as_str()
                .ok_or_else(|| WorkbenchError::invalid("Submission path is missing"))?;
            files::relative(path)?;
            let bytes = blob_read(store, ws, file["hash"].as_str().unwrap_or(""))?;
            archive.start_file(path, options).map_err(err)?;
            archive.write_all(&bytes).map_err(err)?;
        }
        archive.start_file("manifest.json", options).map_err(err)?;
        archive
            .write_all(&serde_json::to_vec_pretty(&snapshot).map_err(err)?)
            .map_err(err)?;
        archive.finish().map_err(err)?;
    }
    crate::safety::sync_file(temp.as_file()).map_err(err)?;
    // An export has a user-selected identity, unlike a content-addressed blob.
    // Atomic no-replace publication must preserve an existing destination.
    temp.persist_noclobber(target).map_err(|error| {
        WorkbenchError::invalid(format!("Choose a new export filename: {error}"))
    })?;
    Ok(json!({"path":destination,"snapshotId":id,"snapshotHash":expected_hash}))
}
