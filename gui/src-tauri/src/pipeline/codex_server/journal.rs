use crate::agent_runtime::codex::session::private_write;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;

const MAX_RECORD_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedAttempt {
    pub id: String,
    pub label: String,
    pub state: String,
}

pub(super) fn read(path: &Path) -> Result<Value, String> {
    use std::io::Read;
    let mut file = crate::safety::open_regular_file(path)?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err("Workflow attempt record exceeds its size limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid Workflow attempt record: {e}"))
}

/// Only run at connection startup. Current-process attempts are separately
/// owned by their live invocation guards and must not block sibling calls.
pub(super) fn unresolved(directory: &Path) -> Result<Vec<UnresolvedAttempt>, String> {
    let mut result = Vec::new();
    let mut count = 0;
    let mut total = 0u64;
    for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry
            .path()
            .extension()
            .is_none_or(|extension| extension != "json")
        {
            continue;
        }
        count += 1;
        total = total.saturating_add(entry.metadata().map_err(|e| e.to_string())?.len());
        if count > 4096 || total > 1024 * 1024 * 1024 {
            return Err("Workflow attempt archive reached its 4,096-record / 1 GiB limit; archive completed records before continuing".into());
        }
        let record = read(&entry.path())?;
        let state = record["state"].as_str().unwrap_or("unknown");
        if matches!(
            state,
            "completed"
                | "completed_reconciled"
                | "failed"
                | "interrupted"
                | "rejected"
                | "abandoned"
                | "prepared"
                | "not_submitted"
        ) || record["cleanup"] == "interrupted_terminal_observed"
            || record["cleanup"] == "terminal_reconciled"
        {
            continue;
        }
        let id = record["id"]
            .as_str()
            .ok_or("Attempt record has no identity")?;
        validate_id(id)?;
        if entry.file_name().to_str() != Some(&format!("{id}.json")) {
            return Err("Attempt record identity does not match its filename".into());
        }
        result.push(UnresolvedAttempt {
            id: id.into(),
            label: record["label"].as_str().unwrap_or("Workflow call").into(),
            state: state.into(),
        });
    }
    Ok(result)
}

pub(super) fn validate_id(id: &str) -> Result<(), String> {
    if id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("Invalid Workflow attempt identity".into())
    }
}

pub(super) fn acknowledge(directory: &Path, id: &str) -> Result<(), String> {
    validate_id(id)?;
    let path = directory.join(format!("{id}.json"));
    let mut record = read(&path)?;
    record["previous_state"] = record["state"].clone();
    record["state"] = json!("abandoned");
    record["acknowledged_at"] = json!(chrono::Utc::now().to_rfc3339());
    private_write(
        &path,
        &serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_requires_acknowledgement_only_for_uncertain_submissions() {
        let directory = tempfile::tempdir().unwrap();
        let id = "00000000000000000000000000000001";
        let path = directory.path().join(format!("{id}.json"));
        std::fs::write(
            &path,
            json!({"id":id,"state":"accepted","label":"Review"}).to_string(),
        )
        .unwrap();
        assert_eq!(unresolved(directory.path()).unwrap().len(), 1);
        acknowledge(directory.path(), id).unwrap();
        assert!(unresolved(directory.path()).unwrap().is_empty());
        assert!(acknowledge(directory.path(), "../secret").is_err());
    }
}
