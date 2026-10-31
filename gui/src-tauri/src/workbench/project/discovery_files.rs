//! Host file edits in discovery's isolated writer roots. The project journal
//! records intent before mutation; immutable blobs retain both versions. A
//! replay with an unknown outcome never repeats a write. Main-project editor
//! paths and referee roots never receive discovery writes.
use super::*;

pub(super) fn mutate(
    store: &Store,
    session: &str,
    tool: &str,
    input: &Value,
    operation: &str,
) -> WorkbenchResult<Value> {
    valid_id(operation)?;
    let snapshot = store.session_snapshot(session)?;
    let ws = snapshot
        .session
        .workspace_id
        .as_deref()
        .ok_or_else(|| WorkbenchError::invalid("Select a project"))?;
    let _guard = lock(store, ws)?;
    if !matches!(
        super::super::discovery::role_policy(store, session)?,
        Some((true, _))
    ) {
        return Err(WorkbenchError::invalid(
            "This discovery role cannot edit files",
        ));
    }
    let root = super::super::discovery::runtime_root(store, session)?
        .ok_or_else(|| WorkbenchError::invalid("Discovery folder is unavailable"))?;
    let digest = hash(
        &serde_json::to_vec(&json!({"session":session,"tool":tool,"input":input})).map_err(err)?,
    );
    let previous: Option<(String, String)> = store
        .connection()?
        .query_row(
            "SELECT request_hash,response_json FROM project_operations WHERE operation_id=?1",
            [operation],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(err)?;
    if let Some((old, response)) = previous {
        if old != digest {
            return Err(WorkbenchError::conflict(
                "Operation ID reused with different arguments",
            ));
        }
        return serde_json::from_str(&response).map_err(err);
    }
    let path = input["path"]
        .as_str()
        .ok_or_else(|| WorkbenchError::invalid("Missing path"))?;
    files::relative(path)?;
    if ignored(path, &[]) {
        return Err(WorkbenchError::invalid(
            "Choose a file outside build/cache folders",
        ));
    }
    let bytes = if tool == "project_artifact" {
        let encoded = input["base64"]
            .as_str()
            .ok_or_else(|| WorkbenchError::invalid("Missing file content"))?;
        if encoded.len() > (MAX_FILE as usize).div_ceil(3) * 4 {
            return Err(WorkbenchError::invalid("File exceeds 32 MiB"));
        }
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(err)?
    } else {
        let text = input["content"]
            .as_str()
            .ok_or_else(|| WorkbenchError::invalid("Missing content"))?;
        if text.len() > 2 * 1024 * 1024 {
            return Err(WorkbenchError::invalid("Text exceeds 2 MiB"));
        }
        text.as_bytes().to_vec()
    };
    let safe = files::SafeRoot::open(&root)?;
    let before = safe.optional_read(path)?;
    let expected = if tool == "project_edit" {
        Some(
            input["expectedHash"]
                .as_str()
                .ok_or_else(|| WorkbenchError::invalid("Read the file before editing"))?,
        )
    } else {
        None
    };
    let before_hash = before.as_deref().map(hash);
    if expected != before_hash.as_deref() {
        return Err(WorkbenchError::conflict(
            "File changed or already exists; read it before editing",
        ));
    }
    if let Some(bytes) = &before {
        blob(store, ws, bytes, "discovery_edit_before")?;
    }
    let after_hash = blob(store, ws, &bytes, "discovery_edit_after")?;
    let claim = id("discovery-edit")?;
    let executable = safe.executable(path)?;
    let mut result = json!({"outcome":"unknown","sessionId":session,"path":path,"previousHash":before_hash,"hash":after_hash,"claim":claim,"recovery":"Inspect the isolated paper folder before retrying; previous content is retained in project snapshots."});
    store
        .connection()?
        .execute(
            "INSERT INTO project_operations VALUES(?1,?2,?3,?4)",
            params![operation, digest, result.to_string(), now()],
        )
        .map_err(err)?;
    safe.replace(path, expected, executable, Some(&bytes), executable, &claim)?;
    safe.clear_claim(path, &claim)?;
    result["outcome"] = json!("completed");
    result["saved"] = json!(true);
    store
        .connection()?
        .execute(
            "UPDATE project_operations SET response_json=?2 WHERE operation_id=?1",
            params![operation, result.to_string()],
        )
        .map_err(err)?;
    Ok(result)
}
