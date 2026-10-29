//! Both conversation providers use the Studio's scoped handles and edit journal.
use super::*;

pub(crate) fn catalog(edit: bool) -> Vec<Value> {
    let mut tools = vec![
        json!({"name":"project_files","description":"List up to 200 files in this conversation's project or working copy.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"project_read","description":"Read a project text file up to 256 KiB and return its hash for a subsequent edit.","inputSchema":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}}),
    ];
    if edit {
        tools.push(json!({"name":"project_edit","description":"Replace an existing text file using its exact previously read hash. Changes are recoverable and never overwrite a concurrent edit.","inputSchema":{"type":"object","properties":{"path":{"type":"string"},"expectedHash":{"type":"string"},"content":{"type":"string"}},"required":["path","expectedHash","content"],"additionalProperties":false}}));
        tools.push(json!({"name":"project_create","description":"Create a new text file, including missing parent directories. Refuses existing paths. Changes in a working copy await acceptance; other changes use the recoverable project editor.","inputSchema":{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"],"additionalProperties":false}}));
    }
    tools
}

pub(crate) fn call(
    store: &Store,
    session_id: &str,
    tool: &str,
    input: &Value,
    operation_id: &str,
) -> WorkbenchResult<Value> {
    let snapshot = store.session_snapshot(session_id)?;
    let ws = snapshot
        .session
        .workspace_id
        .as_deref()
        .ok_or_else(|| WorkbenchError::invalid("Select a project first"))?;
    let checkpoint = snapshot
        .session
        .overrides
        .get("projectCheckpointId")
        .and_then(Value::as_str);
    let effective = research::resolve_harness(store, session_id)?;
    let root = store.runtime_root(session_id)?;
    let argument = |name: &str| {
        input
            .get(name)
            .and_then(Value::as_str)
            .ok_or_else(|| WorkbenchError::invalid(format!("Missing {name}")))
    };
    match tool {
        "project_files" => {
            let mut inventory = inventory_at(&root, &[])?;
            let truncated = !inventory.complete || inventory.files.len() > 200;
            inventory.files.truncate(200);
            Ok(json!({"files":inventory.files,"truncated":truncated,"warnings":inventory.warnings}))
        }
        "project_read" => {
            let path = argument("path")?;
            files::relative(path)?;
            let bytes = files::SafeRoot::open(&root)?.read(path)?;
            if bytes.len() > 256 * 1024 {
                return Err(WorkbenchError::invalid("Project tool reads are limited to 256 KiB. Attach a selected excerpt for larger files."));
            }
            let digest = hash(&bytes);
            let text = String::from_utf8(bytes).map_err(|_| {
                WorkbenchError::invalid(
                    "Use the paper tools or attach this file for non-text content",
                )
            })?;
            Ok(json!({"path":path,"hash":digest,"text":text}))
        }
        "project_edit" | "project_create" | "project_artifact" => {
            if effective.mode != "edit" {
                return Err(WorkbenchError::invalid(
                    "File editing requires Edit access in Assistant settings",
                ));
            }
            let editor_root = match checkpoint {
                Some(id) => session_task_root(store, ws, id)?,
                None => super::root(store, ws)?,
            };
            if root != editor_root {
                return Err(WorkbenchError::invalid(
                    "This scoped conversation cannot edit the project's main working folder",
                ));
            }
            if research::jobs::workspace_active(ws) {
                return Err(WorkbenchError::conflict(
                    "A local job holds this project's write resources",
                ));
            }
            studio::studio_mutate(
                store,
                studio::StudioMutation {
                    workspace_id: ws.into(),
                    operation_id: operation_id.into(),
                    action: if tool != "project_edit" {
                        use base64::Engine as _;
                        studio::StudioAction::CreateFile {
                            checkpoint_id: checkpoint.map(str::to_owned),
                            path: argument("path")?.into(),
                            base64: if tool == "project_artifact" { argument("base64")?.to_string() } else { base64::engine::general_purpose::STANDARD.encode(argument("content")?) },
                        }
                    } else { studio::StudioAction::SaveText {
                        checkpoint_id: checkpoint.map(str::to_owned),
                        path: argument("path")?.into(),
                        expected_hash: argument("expectedHash")?.into(),
                        content: argument("content")?.into(),
                    } },
                },
            )
        }
        _ => Err(WorkbenchError::invalid("Unknown project file tool")),
    }
}

/// Called only inside studio_mutate's durable operation and project lock.
pub(super) fn create_file(store: &Store, ws: &str, checkpoint: Option<&str>, path: &str, bytes: &[u8]) -> WorkbenchResult<Value> {
    files::relative(path)?;
    if bytes.len() as u64 > MAX_FILE || ignored(path, &[]) {
        return Err(WorkbenchError::invalid("Choose a file outside build/cache folders, no larger than 32 MiB"));
    }
    let root = match checkpoint { Some(cp) => session_task_root(store, ws, cp)?, None => super::root(store, ws)? };
    if files::SafeRoot::open(&root)?.optional_read(path)?.is_some() {
        return Err(WorkbenchError::conflict("This file already exists; read it and use project_edit"));
    }
    let (cp, task) = if let Some(cp) = checkpoint { (cp.to_owned(), None) } else {
        let task = tasks::create_task(store, ws, format!("Create {path}"), None, vec![path.into()], vec![])?;
        let cp = tasks::checkpoint(store, ws, &task.id, vec![], "copy")?.id;
        (cp, Some(task))
    };
    let safe = files::SafeRoot::open(&session_task_root(store, ws, &cp)?)?;
    let claim = id("create")?;
    safe.replace(path, None, false, Some(bytes), false, &claim)?;
    safe.clear_claim(path, &claim)?;
    let captured = tasks::capture_changes(store, ws, &cp)?;
    let application = if let Some(task) = task {
        let applied = tasks::apply(store, ws, &cp, vec![path.into()], captured.revision)?;
        tasks::update_task(store, ws, &task.id, task.revision, "completed")?;
        tasks::discard_copy(store, ws, &cp)?;
        Some(applied)
    } else { None };
    Ok(json!({"created":true,"path":path,"hash":hash(bytes),"bytes":bytes.len(),"checkpointId":cp,"application":application}))
}

pub(crate) fn capture_session(store: &Store, session: &str) -> WorkbenchResult<()> {
    let snapshot = store.session_snapshot(session)?;
    if let (Some(ws), Some(cp)) = (snapshot.session.workspace_id.as_deref(), snapshot.session.overrides["projectCheckpointId"].as_str()) {
        let _guard = lock(store, ws)?;
        tasks::capture_changes(store, ws, cp)?;
    }
    Ok(())
}
