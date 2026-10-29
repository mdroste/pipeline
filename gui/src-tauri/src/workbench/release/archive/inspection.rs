//! Archive manifest inspection and extracted database validation.

use super::format::{
    ArchiveInspection, ArchiveManifest, ExportArchiveRequest, ARCHIVE_FORMAT_VERSION,
    MAX_ARCHIVE_BYTES, MAX_ARCHIVE_ENTRIES, MAX_MANIFEST_BYTES,
};
use super::{safe_archive_name, validate_absolute_file};
use crate::workbench::store::{WorkbenchError, WorkbenchResult, CURRENT_SCHEMA_VERSION};
use rusqlite::Connection;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Read;

pub(super) fn validate_manifest(manifest: &ArchiveManifest) -> WorkbenchResult<()> {
    if manifest.format != "pipeline-workspace-research"
        || !(1..=ARCHIVE_FORMAT_VERSION).contains(&manifest.format_version)
        || (manifest.format_version == 1 && !manifest.conversation_files.is_empty())
        || manifest.store_schema_version == 0
        || manifest.store_schema_version > CURRENT_SCHEMA_VERSION
    {
        return Err(WorkbenchError::invalid(
            "Archive format or store schema is unsupported by this build",
        ));
    }
    Ok(())
}

pub fn inspect_archive(request: ExportArchiveRequest) -> WorkbenchResult<ArchiveInspection> {
    let source_path = validate_absolute_file(&request.path, "pwrx")?;
    let archive_size = fs::metadata(&source_path)
        .map_err(|error| WorkbenchError::storage("Failed to inspect research archive", error))?
        .len();
    if archive_size > MAX_ARCHIVE_BYTES {
        return Err(WorkbenchError::invalid("Research archive exceeds 2 GiB"));
    }
    let file = File::open(&source_path)
        .map_err(|error| WorkbenchError::storage("Failed to open research archive", error))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| WorkbenchError::invalid(format!("Invalid research archive: {error}")))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(WorkbenchError::invalid(
            "Research archive contains too many entries",
        ));
    }
    let mut names = HashSet::with_capacity(archive.len());
    let mut manifest_index = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| WorkbenchError::invalid(format!("Invalid archive entry: {error}")))?;
        let name = entry
            .name()
            .map_err(|error| {
                WorkbenchError::invalid(format!("Invalid archive entry name: {error}"))
            })?
            .into_owned();
        if !safe_archive_name(&name)
            || entry.is_dir()
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            || !names.insert(name.clone())
        {
            return Err(WorkbenchError::invalid(
                "Archive contains an unsafe or duplicate entry",
            ));
        }
        if name == "manifest.json" {
            if entry.size() > MAX_MANIFEST_BYTES {
                return Err(WorkbenchError::invalid("Archive manifest exceeds 4 MiB"));
            }
            manifest_index = Some(index);
        }
    }
    let index =
        manifest_index.ok_or_else(|| WorkbenchError::invalid("Archive manifest is missing"))?;
    // The declared size is untrusted: bound the decompressed stream itself.
    let mut raw = Vec::new();
    archive
        .by_index(index)
        .map_err(|error| WorkbenchError::invalid(format!("Invalid archive manifest: {error}")))?
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut raw)
        .map_err(|error| WorkbenchError::storage("Failed to read archive manifest", error))?;
    if raw.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(WorkbenchError::invalid("Archive manifest exceeds 4 MiB"));
    }
    let manifest: ArchiveManifest = serde_json::from_slice(&raw)
        .map_err(|error| WorkbenchError::invalid(format!("Invalid archive manifest: {error}")))?;
    validate_manifest(&manifest)?;
    Ok(ArchiveInspection {
        path: source_path.to_string_lossy().into_owned(),
        format_version: manifest.format_version,
        store_schema_version: manifest.store_schema_version,
        workspace_roots: manifest.workspace_roots,
        portability_note: manifest.portability_note,
    })
}

pub(super) fn validate_database(connection: &Connection) -> WorkbenchResult<()> {
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|error| WorkbenchError::storage("Failed to validate archive database", error))?;
    if integrity != "ok" {
        return Err(WorkbenchError::invalid(
            "Archive database failed integrity validation",
        ));
    }
    let violations: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .map_err(|error| {
            WorkbenchError::storage("Failed to check imported database references", error)
        })?;
    if violations != 0 {
        return Err(WorkbenchError::invalid(
            "Archive database contains broken references",
        ));
    }
    Ok(())
}
