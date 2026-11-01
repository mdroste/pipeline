//! Archive manifest inspection and extracted database validation.

use super::format::{
    ArchiveInspection, ArchiveManifest, ExportArchiveRequest, ARCHIVE_FORMAT_VERSION,
    MAX_ARCHIVE_BYTES, MAX_ARCHIVE_ENTRIES, MAX_MANIFEST_BYTES,
};
use super::omissions::validate_omissions;
use super::{safe_archive_name, validate_absolute_file};
use crate::workbench::store::{
    reference_schema, WorkbenchError, WorkbenchResult, CURRENT_SCHEMA_VERSION,
};
use rusqlite::Connection;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

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
    validate_omissions(manifest)
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
        omitted_file_count: manifest.omitted_references.len(),
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

/// Opens an extracted database. An archive is untrusted until its schema has
/// been compared, so nothing in it may run a function with side effects.
pub(super) fn open_archived_database(path: &Path) -> WorkbenchResult<Connection> {
    let connection = Connection::open(path)
        .map_err(|error| WorkbenchError::invalid(format!("Invalid archive database: {error}")))?;
    connection
        .pragma_update(None, "trusted_schema", false)
        .map_err(|error| WorkbenchError::storage("Failed to secure archive database", error))?;
    Ok(connection)
}

type SchemaObject = (String, String, String, Option<String>);

/// Every schema row, sorted. `rootpage` is left out because it is the one column
/// SQLite assigns by where it happened to allocate the object.
fn schema_objects(connection: &Connection) -> WorkbenchResult<Vec<SchemaObject>> {
    let mut statement = connection
        .prepare("SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name,tbl_name,sql")
        .map_err(|error| {
            WorkbenchError::invalid(format!(
                "Archive database has an unreadable schema: {error}"
            ))
        })?;
    let objects = statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(|error| WorkbenchError::storage("Failed to read archive schema", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkbenchError::storage("Failed to decode archive schema", error))?;
    Ok(objects)
}

/// Refuses a database unless every schema object is exactly what version
/// `version` of Pipeline creates. Triggers, views, extra tables, and changed
/// definitions would otherwise run during retirement and in the live store.
pub(super) fn verify_schema(connection: &Connection, version: u32) -> WorkbenchResult<()> {
    let expected = schema_objects(&reference_schema(version)?)?;
    let actual = schema_objects(connection)?;
    if actual == expected {
        return Ok(());
    }
    let difference = actual
        .iter()
        .find(|object| !expected.contains(object))
        .or_else(|| expected.iter().find(|object| !actual.contains(object)))
        .map(|(kind, name, _, _)| format!("{kind} {}", name.chars().take(80).collect::<String>()))
        .unwrap_or_else(|| "duplicate schema entry".to_string());
    Err(WorkbenchError::invalid(format!(
        "This backup wasn't restored because its database doesn't match what Pipeline creates ({difference}). Use a backup Pipeline made."
    )))
}

/// Confirms retirement took effect. The statements ran against a verified schema,
/// so this should never fire, and restore must not continue if it does.
pub(super) fn verify_retired(connection: &Connection) -> WorkbenchResult<()> {
    for sql in [
        "SELECT COUNT(*) FROM execution_plan_state",
        "SELECT COUNT(*) FROM execution_authorizations",
        "SELECT COUNT(*) FROM project_operations",
        "SELECT COUNT(*) FROM storage_trash",
        "SELECT COUNT(*) FROM discovery_roles WHERE enabled<>0",
        "SELECT COUNT(*) FROM scheduled_checks WHERE enabled<>0",
        "SELECT COUNT(*) FROM experiment_runs WHERE state<>'attention' OR authorized_hash IS NOT NULL",
        "SELECT COUNT(*) FROM execution_profiles WHERE tested_at IS NOT NULL OR test_status IS NOT NULL",
        "SELECT COUNT(*) FROM research_followups WHERE state IN ('queued','dispatching','running')",
        "SELECT COUNT(*) FROM session_bindings WHERE retired_at IS NULL",
        "SELECT COUNT(*) FROM turns WHERE terminal_at IS NULL",
        "SELECT COUNT(*) FROM conversation_tool_receipts WHERE state='running'",
    ] {
        let remaining: i64 = connection
            .query_row(sql, [], |row| row.get(0))
            .map_err(|error| WorkbenchError::storage("Failed to confirm retired authority", error))?;
        if remaining != 0 {
            return Err(WorkbenchError::invalid(
                "This backup wasn't restored because Pipeline couldn't remove its saved approvals. Use a backup Pipeline made.",
            ));
        }
    }
    Ok(())
}
