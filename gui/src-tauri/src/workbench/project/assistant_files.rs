//! Claude project file tools reuse the Studio's scoped handles and edit journal.
use super::*;

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
        "project_edit" => {
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
                    action: studio::StudioAction::SaveText {
                        checkpoint_id: checkpoint.map(str::to_owned),
                        path: argument("path")?.into(),
                        expected_hash: argument("expectedHash")?.into(),
                        content: argument("content")?.into(),
                    },
                },
            )
        }
        _ => Err(WorkbenchError::invalid("Unknown project file tool")),
    }
}
