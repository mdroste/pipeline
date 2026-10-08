//! A failed Review has no result left for its Task to adopt. Successful and
//! uncertain runs retain pins until normal reconciliation durably adopts them.
use super::*;
use std::path::Path;

fn reconcile(
    dir: &Path,
    status: &str,
    active: Option<&str>,
    abandoned: Option<&str>,
) -> Result<(), String> {
    if !matches!(status, "failed" | "cancelled")
        && !(abandoned.is_some()
            && matches!(status, "done" | "degraded" | "partial" | "interrupted"))
    {
        return Ok(());
    }
    let metadata = std::fs::symlink_metadata(dir).map_err(|e| e.to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("Review pin directory must not be a symbolic link".into());
    }
    let path = dir.join(".task-pin");
    if !path.try_exists().map_err(|e| e.to_string())? {
        return Ok(());
    }
    let read = |path: &Path| -> Result<String, String> {
        let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.len() > 4096 {
            return Err("Invalid Task pin metadata".into());
        }
        std::fs::read_to_string(path).map_err(|e| e.to_string())
    };
    let operation = read(&path)?;
    if abandoned.is_some_and(|expected| expected != operation) {
        return Err("Abandoned operation does not own this Review pin".into());
    }
    if active == Some(operation.as_str()) {
        return Ok(());
    }
    let metadata: Value = serde_json::from_str(&read(&dir.join("task-operation.json"))?)
        .map_err(|e| e.to_string())?;
    if metadata["operation"].as_str() != Some(operation.as_str()) {
        return Err("Review pin ownership does not match its journal".into());
    }
    std::fs::remove_file(path).map_err(|e| e.to_string())
}
pub(super) fn release_settled_failure(id: &str) -> Result<(), String> {
    let active = owner().lock().unwrap_or_else(|e| e.into_inner());
    let manifest = crate::runs::load_manifest(id)?;
    reconcile(
        &crate::runs::runs_dir()?.join(id),
        &manifest.status,
        active.as_deref(),
        None,
    )
}
pub fn reconcile_failed_pins() -> Result<(), String> {
    let mut errors = Vec::new();
    for run in crate::runs::list_runs()? {
        if let Err(error) = release_settled_failure(&run.run_id) {
            errors.push(format!("{}: {error}", run.run_id));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}
/// Explicit Task stop/retry may abandon a settled outcome. The global Review
/// lock prevents an old/cross-process writer from racing this decision.
pub fn release_abandoned_pin(id: &str, operation: &str) -> Result<(), String> {
    use fs2::FileExt as _;
    crate::runs::validate_run_id(id)?;
    let root = crate::runs::runs_dir()?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(
            root.parent()
                .ok_or("Missing Review store parent")?
                .join("run.lock"),
        )
        .map_err(|e| e.to_string())?;
    if lock.try_lock_exclusive().is_err() {
        return Ok(());
    }
    let active = owner().lock().unwrap_or_else(|e| e.into_inner());
    let manifest = crate::runs::load_manifest(id)?;
    reconcile(
        &root.join(id),
        &manifest.status,
        active.as_deref(),
        Some(operation),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_settled_matching_failures_release_pins() {
        let dir = tempfile::tempdir().unwrap();
        let pin = dir.path().join(".task-pin");
        std::fs::write(&pin, "operation-a").unwrap();
        std::fs::write(
            dir.path().join("task-operation.json"),
            r#"{"operation":"operation-a"}"#,
        )
        .unwrap();
        for status in ["running", "done", "degraded", "interrupted"] {
            reconcile(dir.path(), status, None, None).unwrap();
            assert!(pin.exists());
        }
        reconcile(dir.path(), "failed", Some("operation-a"), None).unwrap();
        assert!(pin.exists());
        reconcile(dir.path(), "failed", None, None).unwrap();
        assert!(!pin.exists());
        std::fs::write(&pin, "operation-b").unwrap();
        assert!(reconcile(dir.path(), "cancelled", None, None).is_err());
        assert!(pin.exists());
        std::fs::write(&pin, "operation-a").unwrap();
        reconcile(dir.path(), "cancelled", None, None).unwrap();
        assert!(!pin.exists());
    }
    #[test]
    fn explicit_abandonment_releases_only_matching_settled_pins() {
        let dir = tempfile::tempdir().unwrap();
        let pin = dir.path().join(".task-pin");
        std::fs::write(&pin, "a").unwrap();
        std::fs::write(
            dir.path().join("task-operation.json"),
            r#"{"operation":"a"}"#,
        )
        .unwrap();
        reconcile(dir.path(), "running", None, Some("a")).unwrap();
        assert!(pin.exists());
        reconcile(dir.path(), "interrupted", None, None).unwrap();
        assert!(pin.exists());
        assert!(reconcile(dir.path(), "interrupted", None, Some("b")).is_err());
        assert!(pin.exists());
        reconcile(dir.path(), "interrupted", None, Some("a")).unwrap();
        assert!(!pin.exists());
    }
}
