//! Recoverable run trash, its expiry, and explicit permanent deletion.
use super::{
    load_manifest, read_utf8_at_most, runs_dir, validate_run_id, write_text_atomic, RunManifest,
    MAX_MANIFEST_BYTES,
};
use chrono::{DateTime, Utc};
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

/// How long a run stays recoverable in Trash. After that, the next app start
/// or finished review deletes it permanently.
pub const TRASH_RETENTION_DAYS: i64 = 30;

/// Suffix of the file beside a trashed run's folder that records when the run
/// entered Trash. The folder's own modification time cannot say: a rename
/// leaves it unchanged on Windows, so it dates the run's last write instead.
const TRASHED_AT_SUFFIX: &str = ".trashed-at";

fn trashed_at_name(run_id: &str) -> String {
    format!("{run_id}{TRASHED_AT_SUFFIX}")
}

pub(super) fn record_trashed_at(
    trash: &Path,
    run_id: &str,
    at: DateTime<Utc>,
) -> Result<(), String> {
    write_text_atomic(trash, &trashed_at_name(run_id), at.to_rfc3339().as_bytes())
}

/// When `run_id` entered Trash. A run trashed before times were recorded is
/// dated `now`, so its time in Trash starts when it is first seen and an
/// upgrade never deletes it early.
fn trashed_at(trash: &Path, run_id: &str, now: DateTime<Utc>) -> DateTime<Utc> {
    let recorded = read_utf8_at_most(&trash.join(trashed_at_name(run_id)), 64, "Trash time")
        .ok()
        .and_then(|text| DateTime::parse_from_rfc3339(text.trim()).ok());
    match recorded {
        Some(at) => at.with_timezone(&Utc),
        None => {
            let _ = record_trashed_at(trash, run_id, now);
            now
        }
    }
}

fn forget_trashed_at(trash: &Path, run_id: &str) {
    let _ = fs::remove_file(trash.join(trashed_at_name(run_id)));
}

pub(super) fn trash_runs_dir() -> Result<PathBuf, String> {
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
    super::retention::with_unheld_run(run_id, || move_to_trash(run_id))
}

pub(super) fn move_to_trash(run_id: &str) -> Result<(), String> {
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
    // If this fails, the run is dated when Trash is next read, which only
    // lengthens its stay.
    let _ = record_trashed_at(&trash, run_id, Utc::now());
    super::history_reads::changed();
    sync_directory(&base)?;
    sync_directory(&trash)?;
    Ok(())
}

pub fn list_trashed_runs() -> Result<Vec<TrashedRun>, String> {
    list_trashed_runs_in(&trash_runs_dir()?, Utc::now())
}

pub(super) fn list_trashed_runs_in(
    trash: &Path,
    now: DateTime<Utc>,
) -> Result<Vec<TrashedRun>, String> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(trash)
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
        let deleted_at = trashed_at(trash, &run_id, now)
            .with_timezone(&chrono::Local)
            .to_rfc3339();
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
    forget_trashed_at(&trash, run_id);
    super::history_reads::changed();
    sync_directory(root)?;
    sync_directory(&trash)
}

/// Deliberate, irreversible deletion of a run that is already in Trash.
pub fn permanently_delete_trashed_run(run_id: &str) -> Result<(), String> {
    permanently_delete_trashed_run_at(&trash_runs_dir()?, run_id)?;
    super::history_reads::changed();
    Ok(())
}

fn permanently_delete_trashed_run_at(trash: &Path, run_id: &str) -> Result<(), String> {
    validate_run_id(run_id)?;
    let trash = trash
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
    forget_trashed_at(&trash, run_id);
    sync_directory(&trash)
}

/// Permanently delete runs that have been in Trash for
/// [`TRASH_RETENTION_DAYS`]. Returns how many were deleted.
pub fn empty_expired_trash() -> Result<usize, String> {
    let (removed, failures) = empty_expired_trash_at(
        &trash_runs_dir()?,
        Utc::now(),
        chrono::Duration::days(TRASH_RETENTION_DAYS),
    )?;
    if removed > 0 {
        super::history_reads::changed();
    }
    match failures.first() {
        None => Ok(removed),
        Some(first) => Err(format!(
            "Could not permanently delete {} expired report(s) from Trash: {first}",
            failures.len()
        )),
    }
}

/// Delete each run in `trash` held for at least `keep_for`, and the time
/// records of runs that are no longer there. A run that cannot be deleted is
/// reported and tried again next time. Returns the number deleted and the
/// failures.
pub(super) fn empty_expired_trash_at(
    trash: &Path,
    now: DateTime<Utc>,
    keep_for: chrono::Duration,
) -> Result<(usize, Vec<String>), String> {
    let trash = trash
        .canonicalize()
        .map_err(|error| format!("Cannot resolve run Trash: {error}"))?;
    let mut removed = 0;
    let mut failures = Vec::new();
    for entry in fs::read_dir(&trash)
        .map_err(|error| format!("Cannot list run Trash: {error}"))?
        .flatten()
    {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if kind.is_dir() {
            if validate_run_id(&name).is_err() || now - trashed_at(&trash, &name, now) < keep_for {
                continue;
            }
            match permanently_delete_trashed_run_at(&trash, &name) {
                Ok(()) => removed += 1,
                Err(error) => failures.push(format!("{name}: {error}")),
            }
        } else if let Some(run_id) = name.strip_suffix(TRASHED_AT_SUFFIX) {
            // Left behind when a restore or deletion could not remove it.
            if validate_run_id(run_id).is_ok() && !trash.join(run_id).exists() {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    Ok((removed, failures))
}

fn sync_directory(_path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    fs::File::open(_path)
        .and_then(|directory| crate::safety::sync_file(&directory))
        .map_err(|error| format!("Failed to sync '{}': {error}", _path.display()))?;
    Ok(())
}
