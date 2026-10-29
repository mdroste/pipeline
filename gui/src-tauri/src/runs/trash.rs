//! Recoverable run trash and explicit permanent deletion.
use super::{
    load_manifest, read_utf8_at_most, runs_dir, validate_run_id, RunManifest, MAX_MANIFEST_BYTES,
};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct TrashedRun {
    pub run_id: String,
    pub deleted_at: String,
    pub title: String,
    pub input_name: String,
}

fn trash_runs_dir() -> Result<PathBuf, String> {
    let runs = runs_dir()?;
    let pipeline = runs
        .parent()
        .ok_or_else(|| "Cannot resolve Pipeline storage directory".to_string())?;
    let trash = pipeline.join("trash").join("runs");
    fs::create_dir_all(&trash).map_err(|error| format!("Cannot create run Trash: {error}"))?;
    Ok(trash)
}

/// Move a completed run into recoverable Trash. The move is atomic because
/// Trash lives beside the run store on the same filesystem.
pub fn delete_run(run_id: &str) -> Result<(), String> {
    validate_run_id(run_id)?;
    if runs_dir()?.join(run_id).join(".task-pin").exists() {
        return Err("This run is retained by a task that has not yet adopted its result".into());
    }
    let manifest = load_manifest(run_id)?;
    if manifest.status == "running" {
        return Err("A running job cannot be deleted".to_string());
    }
    let base = runs_dir()?
        .canonicalize()
        .map_err(|e| format!("Cannot resolve runs dir: {e}"))?;
    let dir = base.join(run_id);
    let canonical = dir
        .canonicalize()
        .map_err(|_| "Run not found".to_string())?;
    if !canonical.starts_with(&base) || canonical == base {
        return Err("Invalid run id".into());
    }
    let trash = trash_runs_dir()?;
    let destination = trash.join(run_id);
    if destination.exists() {
        return Err(
            "A trashed copy of this run already exists; restore or permanently delete it first"
                .to_string(),
        );
    }
    fs::rename(&canonical, &destination)
        .map_err(|error| format!("Failed to move run to Trash: {error}"))?;
    sync_directory(&base)?;
    sync_directory(&trash)?;
    Ok(())
}

pub fn list_trashed_runs() -> Result<Vec<TrashedRun>, String> {
    let trash = trash_runs_dir()?;
    let mut entries = Vec::new();
    for entry in fs::read_dir(&trash)
        .map_err(|error| format!("Cannot list run Trash: {error}"))?
        .flatten()
    {
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let Some(run_id) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if validate_run_id(&run_id).is_err() {
            continue;
        }
        let deleted_at = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .map(chrono::DateTime::<chrono::Local>::from)
            .map(|time| time.to_rfc3339())
            .unwrap_or_default();
        let manifest = read_utf8_at_most(
            &entry.path().join("manifest.json"),
            MAX_MANIFEST_BYTES,
            "Trashed run manifest",
        )
        .ok()
        .and_then(|json| serde_json::from_str::<RunManifest>(&json).ok());
        let title = manifest
            .as_ref()
            .map(|value| value.title.clone())
            .unwrap_or_default();
        let input_name = manifest
            .as_ref()
            .and_then(|value| Path::new(&value.input_path).file_name())
            .and_then(|value| value.to_str())
            .unwrap_or("Report")
            .to_string();
        entries.push(TrashedRun {
            run_id,
            deleted_at,
            title,
            input_name,
        });
    }
    entries.sort_by(|left, right| right.deleted_at.cmp(&left.deleted_at));
    Ok(entries)
}

pub fn restore_trashed_run(run_id: &str) -> Result<(), String> {
    restore_trashed_run_at(&runs_dir()?, &trash_runs_dir()?, run_id)
}

pub(super) fn restore_trashed_run_at(
    root: &Path,
    trash: &Path,
    run_id: &str,
) -> Result<(), String> {
    validate_run_id(run_id)?;
    // Compare canonical with canonical: the default data folder is never
    // canonicalized, and on Windows (verbatim prefix) or through a symlinked
    // folder the uncanonicalized Trash path never equals the resolved parent.
    let trash = trash
        .canonicalize()
        .map_err(|error| format!("Cannot resolve run Trash: {error}"))?;
    let source = trash.join(run_id);
    let canonical = source
        .canonicalize()
        .map_err(|_| "Trashed run not found".to_string())?;
    if canonical.parent() != Some(trash.as_path()) || !canonical.is_dir() {
        return Err("Invalid trashed run id".to_string());
    }
    let destination = root.join(run_id);
    if destination.exists() {
        return Err("A run with this id already exists in History".to_string());
    }
    crate::safety::open_regular_file(&canonical.join("manifest.json"))
        .map_err(|_| "Trashed run has no valid manifest".to_string())?;
    fs::rename(&canonical, &destination)
        .map_err(|error| format!("Failed to restore run from Trash: {error}"))?;
    sync_directory(root)?;
    sync_directory(&trash)
}

/// Deliberate, irreversible deletion of a run that is already in Trash.
pub fn permanently_delete_trashed_run(run_id: &str) -> Result<(), String> {
    validate_run_id(run_id)?;
    let trash = trash_runs_dir()?
        .canonicalize()
        .map_err(|error| format!("Cannot resolve run Trash: {error}"))?;
    let source = trash.join(run_id);
    let canonical = source
        .canonicalize()
        .map_err(|_| "Trashed run not found".to_string())?;
    if canonical.parent() != Some(trash.as_path()) {
        return Err("Invalid trashed run id".to_string());
    }
    fs::remove_dir_all(&canonical)
        .map_err(|error| format!("Failed to permanently delete trashed run: {error}"))?;
    sync_directory(&trash)
}

fn sync_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("Failed to sync '{}': {error}", path.display()))?;
    Ok(())
}
