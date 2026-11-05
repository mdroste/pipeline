//! Files a research backup can't contain.
//!
//! A referenced file can be gone for good: a sync client dropped it, or it was
//! deleted by hand. Restore refuses a reference with no file in the archive, so
//! export stops and names what's affected. The researcher can then choose to
//! back up without those files, and the manifest lists their references.
//!
//! Restore turns each listed reference into an honest gap. Text references are
//! nullable, so they are cleared and the record says its text is unavailable.
//! `artifacts.storage_reference` can't be empty, so that row stays with a
//! placeholder and readers report the file as unavailable. Retention rows are
//! dropped by the usual adoption step, because nothing restored backs them.
//!
//! The list comes from an archive. It is validated like everything else, and it
//! is only ever compared with stored text in the extracted database, so it can
//! clear a reference and do nothing more.

use super::export::collect_files;
use super::format::{ArchiveManifest, MAX_ARCHIVE_ENTRIES};
use super::safe_archive_name;
use crate::workbench::store::{
    is_left_out_reference, left_out_reference, portable_reference, Store, WorkbenchError,
    WorkbenchResult,
};
use rusqlite::{params, Connection, OptionalExtension as _};
use serde::Serialize;
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

const PORTABLE_ROOT: &str = "workspace://blobs";
/// The columns whose file a restored store would need.
const FILE_COLUMNS: [(&str, &str); 3] = [
    ("artifacts", "storage_reference"),
    ("paper_revisions", "text_reference"),
    ("source_versions", "text_reference"),
];
const MAX_NAMED_IN_REFUSAL: usize = 8;
const MAX_REFERENCE_BYTES: usize = 1024;
const MAX_TITLE_CHARS: usize = 200;

/// One record affected by a missing file. Sorted by what the researcher reads.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct MissingBackupFile {
    pub description: String,
    pub reference: String,
}

/// Every reference the archive could satisfy: each file, and each folder above
/// one, because a source-tree reference names the folder.
fn with_folders(references: impl Iterator<Item = String>) -> HashSet<String> {
    let mut known = HashSet::new();
    for reference in references {
        let mut current = reference.as_str();
        while let Some((parent, _)) = current
            .rsplit_once('/')
            .filter(|(parent, _)| *parent != PORTABLE_ROOT)
        {
            known.insert(parent.to_string());
            current = parent;
        }
        known.insert(reference);
    }
    known
}

pub(super) fn archived_references(
    blob_root: &Path,
    files: &[PathBuf],
) -> WorkbenchResult<HashSet<String>> {
    let names = files
        .iter()
        .map(|path| {
            let relative = path
                .strip_prefix(blob_root)
                .map_err(|_| WorkbenchError::invalid("Blob escaped private storage"))?;
            Ok(format!(
                "{PORTABLE_ROOT}/{}",
                relative.to_string_lossy().replace('\\', "/")
            ))
        })
        .collect::<WorkbenchResult<Vec<_>>>()?;
    Ok(with_folders(names.into_iter()))
}

fn stored_references(
    connection: &Connection,
    table: &str,
    column: &str,
) -> WorkbenchResult<Vec<(i64, String)>> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT rowid,{column} FROM {table} WHERE {column} IS NOT NULL"
        ))
        .map_err(|error| WorkbenchError::storage("Failed to read stored files", error))?;
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|error| WorkbenchError::storage("Failed to read stored files", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkbenchError::storage("Failed to decode stored files", error))?;
    Ok(rows)
}

fn quoted(kind: &str, title: &str) -> String {
    let title = title.chars().take(MAX_TITLE_CHARS).collect::<String>();
    format!("{kind} “{title}”")
}

/// What a researcher would call this record. Titles come first, because a hash
/// or a path into private storage means nothing to them.
fn describe(connection: &Connection, table: &str, rowid: i64) -> WorkbenchResult<String> {
    let text = |sql: &str| -> WorkbenchResult<Option<String>> {
        connection
            .query_row(sql, [rowid], |row| row.get::<_, Option<String>>(0))
            .optional()
            .map(Option::flatten)
            .map_err(|error| WorkbenchError::storage("Failed to describe a stored file", error))
    };
    match table {
        "paper_revisions" => Ok(quoted(
            "Readable text of paper",
            &text("SELECT p.title FROM paper_revisions pr JOIN papers p ON p.id=pr.paper_id WHERE pr.rowid=?1")?.unwrap_or_default(),
        )),
        "source_versions" => Ok(quoted(
            "Readable text of source",
            &text("SELECT s.title FROM source_versions v JOIN sources s ON s.id=v.source_id WHERE v.rowid=?1")?.unwrap_or_default(),
        )),
        _ => {
            for (kind, sql) in [
                ("Paper", "SELECT p.title FROM artifacts a JOIN paper_revisions pr ON pr.content_hash=a.content_hash JOIN papers p ON p.id=pr.paper_id AND p.workspace_id=a.workspace_id WHERE a.rowid=?1"),
                ("Source", "SELECT s.title FROM artifacts a JOIN source_versions v ON v.content_hash=a.content_hash JOIN sources s ON s.id=v.source_id AND s.workspace_id=a.workspace_id WHERE a.rowid=?1"),
                ("Dataset", "SELECT d.title FROM artifacts a JOIN desk_records d ON d.workspace_id=a.workspace_id AND d.kind='dataset' AND CASE WHEN json_valid(d.body_json) THEN json_extract(d.body_json,'$.artifactId') END=a.id WHERE a.rowid=?1"),
            ] {
                if let Some(title) = text(sql)? {
                    return Ok(quoted(kind, &title));
                }
            }
            if let Some(name) = text("SELECT original_path FROM artifacts WHERE rowid=?1")?
                .as_deref()
                .and_then(|path| path.rsplit(['/', '\\']).find(|part| !part.is_empty()))
            {
                return Ok(quoted("File", name));
            }
            let kind = text("SELECT media_kind FROM artifacts WHERE rowid=?1")?;
            Ok(match kind.as_deref() {
                Some("project_snapshot") => "A saved version of a project file",
                Some("page_image") => "A rendered PDF page",
                Some("capture") => "A saved analysis input",
                _ => "A saved file",
            }
            .to_string())
        }
    }
}

pub(super) fn missing_files(
    connection: &Connection,
    archived: &HashSet<String>,
) -> WorkbenchResult<Vec<MissingBackupFile>> {
    let mut missing = BTreeSet::new();
    for (table, column) in FILE_COLUMNS {
        for (rowid, reference) in stored_references(connection, table, column)? {
            if is_left_out_reference(&reference) {
                continue;
            }
            let portable = portable_reference(&reference);
            if !archived.contains(&portable) {
                missing.insert(MissingBackupFile {
                    description: describe(connection, table, rowid)?,
                    reference: portable,
                });
            }
        }
    }
    Ok(missing.into_iter().collect())
}

/// The records a backup can't cover right now, for the researcher to review.
pub fn missing_backup_files(store: &Store) -> WorkbenchResult<Vec<MissingBackupFile>> {
    let blob_root = store.root_path().join("blobs");
    let archived = archived_references(&blob_root, &collect_files(&blob_root)?)?;
    missing_files(&store.connection()?, &archived)
}

pub(super) fn missing_files_error(missing: &[MissingBackupFile]) -> WorkbenchError {
    let count = missing.len();
    let mut names = missing
        .iter()
        .take(MAX_NAMED_IN_REFUSAL)
        .map(|file| file.description.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    if count > MAX_NAMED_IN_REFUSAL {
        names.push_str(&format!("; and {} more", count - MAX_NAMED_IN_REFUSAL));
    }
    let (items, verb) = if count == 1 {
        ("1 saved item".to_string(), "has")
    } else {
        (format!("{count} saved items"), "have")
    };
    WorkbenchError::invalid(format!(
        "No backup was saved, because {items} {verb} no file in this research data folder, so the backup could not be restored: {names}. Choose Export research backup again to review them and back up everything else."
    ))
}

/// Gives every reference its portable name, so no path from this computer or an
/// earlier location of this folder travels in the archive.
pub(super) fn make_references_portable(snapshot: &Connection) -> WorkbenchResult<()> {
    for (table, column) in FILE_COLUMNS {
        for (rowid, reference) in stored_references(snapshot, table, column)? {
            let portable = portable_reference(&reference);
            if portable != reference && !is_left_out_reference(&reference) {
                snapshot
                    .execute(
                        &format!("UPDATE {table} SET {column}=?2 WHERE rowid=?1"),
                        params![rowid, portable],
                    )
                    .map_err(|error| {
                        WorkbenchError::storage(
                            "Failed to make archive blob references portable",
                            error,
                        )
                    })?;
            }
        }
    }
    Ok(())
}

/// Checks the list against the manifest alone, so `inspect` refuses it too.
pub(super) fn validate_omissions(manifest: &ArchiveManifest) -> WorkbenchResult<()> {
    let omitted = &manifest.omitted_references;
    if omitted.is_empty() {
        return Ok(());
    }
    let invalid = || WorkbenchError::invalid("Archive lists left-out files it can't account for");
    if manifest.format_version < 2
        || manifest.original_blob_root != PORTABLE_ROOT
        || omitted.len() > MAX_ARCHIVE_ENTRIES
    {
        return Err(invalid());
    }
    let archived = with_folders(
        manifest
            .blobs
            .iter()
            .map(|blob| blob.original_reference.clone()),
    );
    let mut seen = HashSet::with_capacity(omitted.len());
    for reference in omitted {
        let name = reference
            .strip_prefix(PORTABLE_ROOT)
            .and_then(|rest| rest.strip_prefix('/'))
            .ok_or_else(invalid)?;
        // A file can't be both in the archive and left out of it.
        let inside_archived_folder = with_folders(std::iter::once(reference.clone()))
            .iter()
            .any(|known| archived.contains(known));
        if reference.len() > MAX_REFERENCE_BYTES
            || !safe_archive_name(name)
            || !seen.insert(reference.as_str())
            || inside_archived_folder
        {
            return Err(invalid());
        }
    }
    Ok(())
}

/// Clears each listed reference in the extracted database and returns what was
/// affected. A listed reference no record uses is a list that doesn't describe
/// this database, so the archive is refused.
pub(super) fn clear_omitted_references(
    imported: &Connection,
    omitted: &[String],
) -> WorkbenchResult<Vec<String>> {
    if omitted.is_empty() {
        return Ok(Vec::new());
    }
    let listed = omitted.iter().map(String::as_str).collect::<HashSet<_>>();
    let mut used = HashSet::new();
    let mut affected = BTreeSet::new();
    let failed = |error| WorkbenchError::storage("Failed to clear a left-out file", error);
    for (table, column) in FILE_COLUMNS {
        for (rowid, reference) in stored_references(imported, table, column)? {
            if !listed.contains(reference.as_str()) {
                continue;
            }
            affected.insert(describe(imported, table, rowid)?);
            match table {
                "artifacts" => imported.execute(
                    "UPDATE artifacts SET storage_reference=?2 WHERE rowid=?1",
                    params![rowid, left_out_reference(&reference)],
                ),
                "paper_revisions" => imported.execute(
                    "UPDATE paper_revisions SET text_reference=NULL,extraction_json=json_set(CASE WHEN json_valid(extraction_json) THEN extraction_json ELSE '{}' END,'$.status','failed','$.error',?2) WHERE rowid=?1",
                    params![rowid, "This text's file was already missing when the research backup was made."],
                ),
                _ => imported.execute(
                    "UPDATE source_versions SET text_reference=NULL,access_state='unavailable' WHERE rowid=?1",
                    [rowid],
                ),
            }
            .map_err(failed)?;
            used.insert(reference);
        }
    }
    if used.len() != listed.len() {
        return Err(WorkbenchError::invalid(
            "Archive lists left-out files it can't account for",
        ));
    }
    Ok(affected.into_iter().collect())
}
