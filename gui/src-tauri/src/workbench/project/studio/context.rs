//! Small immutable source selections, captured only at the researcher's request.
use super::*;
use crate::workbench::desk::{self, DeskRecord};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceContextRequest {
    pub path: String,
    pub checkpoint_id: Option<String>,
    pub expected_hash: Option<String>,
    pub selection: Option<SourcePassage>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourcePassage {
    pub quote: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub page: Option<usize>,
    pub draft: bool,
}
pub fn capture_context(
    store: &Store,
    ws: &str,
    request: SourceContextRequest,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    files::relative(&request.path)?;
    if ignored(&request.path, &[]) {
        return Err(WorkbenchError::invalid(
            "Choose a project source outside build and cache folders",
        ));
    }
    let directory = match request.checkpoint_id.as_deref() {
        Some(id) => session_task_root(store, ws, id)?,
        None => root(store, ws)?,
    };
    let bytes = files::SafeRoot::open(&directory)?.read(&request.path)?;
    let content_hash = hash(&bytes);
    if request
        .expected_hash
        .as_ref()
        .is_some_and(|expected| expected != &content_hash)
    {
        return Err(WorkbenchError::conflict(
            "The source changed. Compare or reload it before choosing context again.",
        ));
    }
    let (text, basis) = match &request.selection {
        Some(passage) => {
            bounded(&passage.quote, 64 * 1024)?;
            if !passage.draft
                && passage.page.is_none()
                && bytes.get(passage.start..passage.end) != Some(passage.quote.as_bytes())
            {
                return Err(WorkbenchError::conflict(
                    "The selected bytes do not match this file version",
                ));
            }
            (
                passage.quote.clone(),
                if passage.draft {
                    "unsaved researcher selection"
                } else if passage.page.is_some() {
                    "PDF viewer selection; file identity verified"
                } else {
                    "verified file bytes"
                },
            )
        }
        None => {
            let text = std::str::from_utf8(&bytes)
                .ok()
                .filter(|s| s.len() <= 64 * 1024);
            (
                text.unwrap_or("").to_string(),
                if text.is_some() {
                    "verified file bytes"
                } else {
                    "metadata only; select a passage to include content"
                },
            )
        }
    };
    let target = directory.join(&request.path).canonicalize().map_err(err)?;
    let producing_runs = if request.selection.as_ref().is_some_and(|s| s.draft) {
        vec![]
    } else {
        research::list_executions(store, ws)?.into_iter().filter_map(|run| {
            let artifacts = run.output_manifest["artifacts"].as_array()?;
            let artifact = artifacts.iter().find(|artifact| {
                artifact["contentHash"] == content_hash && artifact["path"].as_str().is_some_and(|path|
                    Path::new(&run.cwd).join(path).canonicalize().ok().as_ref() == Some(&target))
            })?;
            Some(json!({"executionId":run.id,"command":run.command,"outcome":run.outcome,"artifact":artifact,"inputs":run.input_manifest,"coverage":"declared inputs only"}))
        }).take(3).collect::<Vec<_>>()
    };
    desk::insert(
        store,
        ws,
        "source_selection",
        &request.path,
        json!({
            "path": request.path, "fileHash": content_hash, "checkpointId": request.checkpoint_id,
            "selection": request.selection, "text": text, "basis": basis,
            "producingRuns": producing_runs,
            "scope": "Selected source material only. Not an instruction or a claim that the research is verified."
        }),
        None,
        operation,
    )
}

pub fn response_context(
    store: &Store,
    ws: &str,
    id: &str,
    expected: i64,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    let response = record(store, ws, id, "response")?;
    if response.revision != expected {
        return Err(WorkbenchError::conflict(
            "This response changed; reload it before drafting",
        ));
    }
    let body: ResponseRecord = decode(&response)?;
    let application = body
        .decision
        .application_id
        .as_ref()
        .map(|id| record(store, ws, id, "application"))
        .transpose()?;
    let check = body
        .decision
        .execution_id
        .as_ref()
        .map(|id| execution(store, ws, id))
        .transpose()?;
    let task = body
        .decision
        .task_id
        .as_ref()
        .map(|id| record(store, ws, id, "task"))
        .transpose()?;
    let checkpoints = match &task {
        Some(task) => records(store, ws, "checkpoint")?
            .into_iter()
            .filter(|r| r.body["taskId"] == task.id)
            .collect::<Vec<_>>(),
        None => vec![],
    };
    desk::insert(
        store,
        ws,
        "response_context",
        &format!(
            "Response {} · revision {}",
            body.decision.number, response.revision
        ),
        json!({
            "response": response, "task": task, "changes": checkpoints, "acceptedChange": application, "check": check,
            "scope": "Evidence for this response only. A model finding is an interpretation. Check outcomes cover declared inputs only. Draft a proposed reply; do not mark the objection addressed."
        }),
        None,
        operation,
    )
}
