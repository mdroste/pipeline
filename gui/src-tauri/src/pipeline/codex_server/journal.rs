//! Private per-call receipts under ~/.pipeline/providers/workflows/attempts/.
//!
//! A receipt holds the exact prompt, instructions, roots and runtime version, so
//! it can contain unpublished research; receipts stay local and out of exports.
//! An unfinished receipt blocks new Workflow model calls until the researcher
//! checks the saved result (a `thread/read`, never a new turn) or acknowledges
//! it. Acknowledging records abandonment; it doesn't erase usage or imply the
//! request never ran. Settled receipts need no further attention, so connection
//! startup keeps only the newest of them; an unfinished receipt is never pruned.

use crate::agent_runtime::codex::session::private_write;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;

const MAX_RECORD_BYTES: u64 = 64 * 1024 * 1024;
/// Unfinished receipts the researcher can reasonably work through.
const MAX_UNRESOLVED_RECORDS: usize = 4096;
const MAX_UNRESOLVED_BYTES: u64 = 1024 * 1024 * 1024;
/// Settled receipts kept for inspection, newest first. Each repeats a whole
/// prompt, so the byte limit usually decides.
const MAX_SETTLED_RECORDS: usize = 1024;
const MAX_SETTLED_BYTES: u64 = 512 * 1024 * 1024;

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

fn settled(record: &Value) -> bool {
    matches!(
        record["state"].as_str().unwrap_or("unknown"),
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
}

/// Remove settled receipts beyond the newest kept ones. A file that cannot be
/// removed now is retried at the next startup.
fn prune_settled(mut records: Vec<(std::time::SystemTime, std::path::PathBuf, u64)>) {
    records.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    let mut kept_bytes = 0u64;
    for (index, (_, path, bytes)) in records.iter().enumerate() {
        kept_bytes = kept_bytes.saturating_add(*bytes);
        if index >= MAX_SETTLED_RECORDS || kept_bytes > MAX_SETTLED_BYTES {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Only run at connection startup, under the connection's exclusive lock.
/// Current-process attempts are separately owned by their live invocation
/// guards and must not block sibling calls.
pub(super) fn unresolved(directory: &Path) -> Result<Vec<UnresolvedAttempt>, String> {
    let mut result = Vec::new();
    let mut settled_records = Vec::new();
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
        let metadata = entry.metadata().map_err(|e| e.to_string())?;
        let record = read(&entry.path())?;
        if settled(&record) {
            settled_records.push((
                metadata.modified().map_err(|e| e.to_string())?,
                entry.path(),
                metadata.len(),
            ));
            continue;
        }
        total = total.saturating_add(metadata.len());
        if result.len() >= MAX_UNRESOLVED_RECORDS || total > MAX_UNRESOLVED_BYTES {
            return Err(
                "Too many Workflow calls are waiting for a saved-result check; check or acknowledge them before continuing"
                    .into(),
            );
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
            state: record["state"].as_str().unwrap_or("unknown").into(),
        });
    }
    prune_settled(settled_records);
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

    fn write_record(directory: &Path, id: &str, state: &str) {
        std::fs::write(
            directory.join(format!("{id}.json")),
            json!({"id":id,"state":state,"label":"Review"}).to_string(),
        )
        .unwrap();
    }

    fn set_age(directory: &Path, id: &str, hours: u64) {
        std::fs::File::options()
            .write(true)
            .open(directory.join(format!("{id}.json")))
            .unwrap()
            .set_modified(
                std::time::SystemTime::now() - std::time::Duration::from_secs(hours * 3600),
            )
            .unwrap();
    }

    #[test]
    fn startup_survives_a_full_archive_and_prunes_only_settled_receipts() {
        let directory = tempfile::tempdir().unwrap();
        for index in 0..=MAX_UNRESOLVED_RECORDS {
            write_record(directory.path(), &format!("{index:032x}"), "completed");
        }
        let old = format!("{:032x}", 0);
        let recent = format!("{:032x}", 1);
        set_age(directory.path(), &old, 48);
        set_age(directory.path(), &recent, 0);
        let waiting = "f0000000000000000000000000000000";
        write_record(directory.path(), waiting, "accepted");
        set_age(directory.path(), waiting, 72);

        let pending = unresolved(directory.path()).unwrap();

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, waiting);
        let remaining = std::fs::read_dir(directory.path()).unwrap().count();
        assert_eq!(remaining, MAX_SETTLED_RECORDS + 1);
        assert!(directory.path().join(format!("{waiting}.json")).exists());
        assert!(!directory.path().join(format!("{old}.json")).exists());
        // A second startup finds nothing further to prune.
        unresolved(directory.path()).unwrap();
        assert_eq!(
            std::fs::read_dir(directory.path()).unwrap().count(),
            remaining
        );
    }

    #[test]
    fn startup_stops_only_when_unfinished_receipts_fill_the_limit() {
        let directory = tempfile::tempdir().unwrap();
        for index in 0..=MAX_UNRESOLVED_RECORDS {
            write_record(directory.path(), &format!("{index:032x}"), "accepted");
        }
        let error = unresolved(directory.path()).err().unwrap();
        assert!(error.contains("acknowledge"), "{error}");
        assert_eq!(
            std::fs::read_dir(directory.path()).unwrap().count(),
            MAX_UNRESOLVED_RECORDS + 1,
            "unfinished receipts are never pruned"
        );
    }
}
