//! Portable `.pwrx` research-store snapshots and restore validation.

use super::now;
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use sha2::{Digest as _, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

mod conversation_files;
mod export;
mod format;
mod inspection;
mod omissions;
mod restore;
mod transcripts;

pub use export::{export_archive, export_archive_without};
pub use format::{ArchiveInspection, ArchiveReport, ExportArchiveRequest, ImportArchiveRequest};
pub use inspection::inspect_archive;
pub use omissions::{missing_backup_files, MissingBackupFile};
pub use restore::import_archive;
pub(crate) use transcripts::transcript_role;

fn validate_absolute_file(path: &str, extension: &str) -> WorkbenchResult<PathBuf> {
    let path = PathBuf::from(path);
    if !path.is_absolute()
        || path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| !value.eq_ignore_ascii_case(extension))
            .unwrap_or(true)
    {
        return Err(WorkbenchError::invalid(format!(
            "Choose an absolute .{extension} path"
        )));
    }
    Ok(path)
}

fn hash_reader(mut reader: impl Read) -> WorkbenchResult<String> {
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| WorkbenchError::storage("Failed to hash archive content", error))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(super) fn safe_archive_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && Path::new(name)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

/// Extract one entry. Deflate streams are bounded only by their compressed
/// size, so cap the copy at the declared size (+1 so an honest entry still
/// reaches EOF and its CRC check) and refuse entries whose header lies.
/// Shared by .pwrx restore and .pwex import as archive mechanics only.
pub(super) fn copy_entry(
    entry: &mut zip::read::ZipFile<'_, File>,
    output: &mut File,
) -> WorkbenchResult<()> {
    let declared = entry.size();
    let copied = std::io::copy(&mut entry.take(declared + 1), output)
        .map_err(|error| WorkbenchError::storage("Failed to extract archive entry", error))?;
    if copied != declared {
        return Err(WorkbenchError::invalid(
            "Archive entry size does not match its header",
        ));
    }
    Ok(())
}

pub fn refresh_retained_blobs(store: &Store) -> WorkbenchResult<usize> {
    let connection = store.connection()?;
    connection
        .execute("DELETE FROM retained_blobs", [])
        .map_err(|error| {
            WorkbenchError::storage("Failed to refresh retained blob references", error)
        })?;
    let timestamp = now();
    connection.execute("INSERT OR IGNORE INTO retained_blobs (storage_reference,reference_type,reference_id,reason,recorded_at) SELECT a.storage_reference,'artifact',a.id,'immutable Workspace artifact',?1 FROM artifacts a",[&timestamp]).map_err(|error| WorkbenchError::storage("Failed to retain artifact blobs", error))?;
    connection.execute("INSERT OR IGNORE INTO retained_blobs (storage_reference,reference_type,reference_id,reason,recorded_at) SELECT a.storage_reference,'evidence',e.id,'referenced by claim evidence',?1 FROM evidence_links e JOIN artifacts a ON e.target_type='artifact' AND a.id=e.target_id",[&timestamp]).map_err(|error| WorkbenchError::storage("Failed to retain evidence blobs", error))?;
    connection
        .query_row("SELECT COUNT(*) FROM retained_blobs", [], |row| {
            row.get::<_, i64>(0)
        })
        .map(|value| value as usize)
        .map_err(|error| WorkbenchError::storage("Failed to count retained blobs", error))
}

#[cfg(test)]
mod migration_tests;

#[cfg(test)]
mod regression_tests;

#[cfg(test)]
mod relocation_tests;
