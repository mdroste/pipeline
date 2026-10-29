//! Validated research-store restoration and retirement of imported authority.

use super::format::{
    ArchiveManifest, ArchiveReport, ImportArchiveRequest, MAX_ARCHIVE_BYTES, MAX_ARCHIVE_ENTRIES,
    MAX_ENTRY_BYTES, MAX_MANIFEST_BYTES,
};
use super::inspection::{validate_database, validate_manifest};
use super::{
    conversation_files, copy_entry, hash_reader, now, safe_archive_name, validate_absolute_file,
};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult, CURRENT_SCHEMA_VERSION};
use rusqlite::{backup::Backup, params, Connection};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::path::{Component, Path};
use std::time::Duration;

pub fn import_archive(
    store: &Store,
    request: ImportArchiveRequest,
) -> WorkbenchResult<ArchiveReport> {
    let source_path = validate_absolute_file(&request.path, "pwrx")?;
    let archive_size = fs::metadata(&source_path)
        .map_err(|error| WorkbenchError::storage("Failed to inspect research archive", error))?
        .len();
    if archive_size > MAX_ARCHIVE_BYTES {
        return Err(WorkbenchError::invalid("Research archive exceeds 2 GiB"));
    }
    let current = store.connection()?;
    let occupied: i64 = current
        .query_row(
            "SELECT (SELECT COUNT(*) FROM workspaces)+(SELECT COUNT(*) FROM sessions)+(SELECT COUNT(*) FROM workspace_configs)+(SELECT COUNT(*) FROM presets)+(SELECT COUNT(*) FROM recipe_definitions)+(SELECT COUNT(*) FROM research_evaluations)+(SELECT COUNT(*) FROM performance_samples)",
            [],
            |row| row.get(0),
        )
        .map_err(|error| WorkbenchError::storage("Failed to check import collisions", error))?;
    if occupied > 0 {
        return Err(WorkbenchError::invalid("Import would collide with existing Workspace IDs. Restore into an empty research store."));
    }
    drop(current);
    let temporary = tempfile::tempdir().map_err(|error| {
        WorkbenchError::storage("Failed to create import staging directory", error)
    })?;
    let file = File::open(&source_path)
        .map_err(|error| WorkbenchError::storage("Failed to open research archive", error))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| WorkbenchError::invalid(format!("Invalid research archive: {error}")))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(WorkbenchError::invalid(
            "Research archive contains too many entries",
        ));
    }
    let mut total = 0u64;
    let mut extracted_names = HashSet::with_capacity(archive.len());
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| WorkbenchError::invalid(format!("Invalid archive entry: {error}")))?;
        let name = entry.name().to_string();
        if !safe_archive_name(&name)
            || entry.is_dir()
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(WorkbenchError::invalid(
                "Archive contains a traversal, directory, or symbolic-link entry",
            ));
        }
        if !extracted_names.insert(name.clone()) {
            return Err(WorkbenchError::invalid(
                "Archive contains duplicate entry names",
            ));
        }
        if name == "manifest.json" && entry.size() > MAX_MANIFEST_BYTES {
            return Err(WorkbenchError::invalid("Archive manifest exceeds 4 MiB"));
        }
        if entry.size() > MAX_ENTRY_BYTES {
            return Err(WorkbenchError::invalid("Archive entry exceeds 512 MiB"));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_ARCHIVE_BYTES {
            return Err(WorkbenchError::invalid(
                "Expanded research archive exceeds 2 GiB",
            ));
        }
        let destination = temporary.path().join(&name);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                WorkbenchError::storage("Failed to create import directory", error)
            })?;
        }
        let mut output = File::create(&destination)
            .map_err(|error| WorkbenchError::storage("Failed to create import file", error))?;
        copy_entry(&mut entry, &mut output)?;
    }
    let manifest: ArchiveManifest = serde_json::from_reader(
        File::open(temporary.path().join("manifest.json"))
            .map_err(|error| WorkbenchError::storage("Archive manifest is missing", error))?,
    )
    .map_err(|error| WorkbenchError::invalid(format!("Invalid archive manifest: {error}")))?;
    validate_manifest(&manifest)?;
    let database_path = temporary.path().join("database/research.sqlite3");
    let mut imported = Connection::open(&database_path)
        .map_err(|error| WorkbenchError::invalid(format!("Invalid archive database: {error}")))?;
    let schema: u32 = imported
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| WorkbenchError::storage("Failed to read imported schema", error))?;
    if schema != manifest.store_schema_version || schema == 0 || schema > CURRENT_SCHEMA_VERSION {
        return Err(WorkbenchError::invalid(
            "Archive database schema is unsupported or does not match its manifest",
        ));
    }
    validate_database(&imported)?;
    if schema < CURRENT_SCHEMA_VERSION {
        // Migrate only the extracted copy.
        crate::workbench::store::migrate(&mut imported, schema)?;
        validate_database(&imported)?;
    }
    let session_ids = {
        let mut statement = imported
            .prepare("SELECT id FROM sessions ORDER BY id")
            .map_err(|error| {
                WorkbenchError::storage("Failed to inspect archive transcripts", error)
            })?;
        let session_ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| WorkbenchError::storage("Failed to read archive transcripts", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                WorkbenchError::storage("Failed to decode archive transcripts", error)
            })?;
        session_ids
    };
    let archived_roots = {
        let mut statement = imported
            .prepare("SELECT DISTINCT root FROM workspaces WHERE root IS NOT NULL ORDER BY root")
            .map_err(|error| {
                WorkbenchError::storage("Failed to inspect archived Workspace roots", error)
            })?;
        let roots = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| {
                WorkbenchError::storage("Failed to read archived Workspace roots", error)
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                WorkbenchError::storage("Failed to decode archived Workspace roots", error)
            })?;
        roots
    };
    let mut manifest_roots = manifest.workspace_roots.clone();
    manifest_roots.sort();
    if manifest_roots.windows(2).any(|roots| roots[0] == roots[1])
        || manifest_roots != archived_roots
    {
        return Err(WorkbenchError::invalid(
            "Archive manifest Workspace roots do not match its database",
        ));
    }
    if request.root_mappings.len() != archived_roots.len()
        || archived_roots
            .iter()
            .any(|root| !request.root_mappings.contains_key(root))
    {
        return Err(WorkbenchError::invalid(
            "Choose a new location or null for every archived Workspace root",
        ));
    }
    let mut expected_names = HashSet::from([
        "manifest.json".to_string(),
        "database/research.sqlite3".to_string(),
    ]);
    conversation_files::validate(
        &manifest.conversation_files,
        &session_ids,
        temporary.path(),
        &mut expected_names,
    )?;
    for session_id in session_ids {
        expected_names.insert(format!("transcripts/{session_id}.md"));
        expected_names.insert(format!("transcripts/{session_id}.json"));
    }
    let mut original_blob_references = HashSet::with_capacity(manifest.blobs.len());
    for blob in &manifest.blobs {
        if !expected_names.insert(blob.archive_path.clone())
            || !original_blob_references.insert(blob.original_reference.clone())
        {
            return Err(WorkbenchError::invalid(
                "Archive manifest contains duplicate blob paths or references",
            ));
        }
    }
    if extracted_names != expected_names {
        return Err(WorkbenchError::invalid(
            "Archive entries do not exactly match its database and manifest",
        ));
    }
    let archived_references = manifest
        .blobs
        .iter()
        .map(|blob| blob.original_reference.as_str())
        .collect::<Vec<_>>();
    for (table, column) in [
        ("artifacts", "storage_reference"),
        ("paper_revisions", "text_reference"),
        ("source_versions", "text_reference"),
    ] {
        let mut statement = imported
            .prepare(&format!(
                "SELECT {column} FROM {table} WHERE {column} IS NOT NULL"
            ))
            .map_err(|error| {
                WorkbenchError::storage("Failed to inspect archived blob references", error)
            })?;
        let references = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| {
                WorkbenchError::storage("Failed to read archived blob references", error)
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                WorkbenchError::storage("Failed to decode archived blob references", error)
            })?;
        for reference in references {
            let slash_prefix = format!("{reference}/");
            let backslash_prefix = format!("{reference}\\");
            if !archived_references.iter().any(|candidate| {
                *candidate == reference
                    || candidate.starts_with(&slash_prefix)
                    || candidate.starts_with(&backslash_prefix)
            }) {
                return Err(WorkbenchError::invalid(
                    format!("Archive database {table}.{column} references an immutable blob omitted from the manifest: {reference}"),
                ));
            }
        }
    }
    for blob in &manifest.blobs {
        if !safe_archive_name(&blob.archive_path) || !blob.archive_path.starts_with("blobs/") {
            return Err(WorkbenchError::invalid("Archive blob path is invalid"));
        }
        let path = temporary.path().join(&blob.archive_path);
        let metadata = fs::metadata(&path).map_err(|_| {
            WorkbenchError::invalid("Archive is missing a referenced immutable blob")
        })?;
        if metadata.len() != blob.size_bytes
            || hash_reader(File::open(&path).map_err(|error| {
                WorkbenchError::storage("Failed to validate imported blob", error)
            })?)?
                != blob.sha256
        {
            return Err(WorkbenchError::invalid(
                "Imported immutable blob failed its size or hash check",
            ));
        }
    }
    for (old, new) in &request.root_mappings {
        if old.is_empty() {
            return Err(WorkbenchError::invalid(
                "Root remapping keys cannot be empty",
            ));
        }
        let present: bool = imported
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM workspaces WHERE root=?1)",
                [old],
                |row| row.get(0),
            )
            .map_err(|error| {
                WorkbenchError::storage("Failed to validate imported root mapping", error)
            })?;
        if !present {
            return Err(WorkbenchError::invalid(
                "Root remapping key does not match an archived Workspace root",
            ));
        }
        let (new_root, new_identity) = match new {
            Some(new) => {
                let canonical = crate::workbench::store::canonical_workspace_root(
                    Path::new(new),
                    store.root_path(),
                )?;
                let identity = crate::workbench::store::root_identity(&canonical)?;
                (
                    Some(canonical.to_string_lossy().into_owned()),
                    Some(identity),
                )
            }
            None => (None, None),
        };
        imported
            .execute(
                "UPDATE workspaces SET root=?2,root_identity=?3,missing_root_at=NULL WHERE root=?1",
                params![old, new_root, new_identity],
            )
            .map_err(|error| {
                WorkbenchError::storage("Failed to remap imported Workspace root", error)
            })?;
    }
    let blob_root = store.root_path().join("blobs");
    fs::create_dir_all(&blob_root)
        .map_err(|error| WorkbenchError::storage("Failed to create imported blob store", error))?;
    let mut reference_map = HashMap::new();
    for blob in &manifest.blobs {
        let relative = Path::new(&blob.archive_path)
            .strip_prefix("blobs")
            .map_err(|_| WorkbenchError::invalid("Archive blob path is outside blobs"))?;
        let destination = blob_root.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                WorkbenchError::storage("Failed to create imported blob directory", error)
            })?;
        }
        if destination.exists() {
            let existing = hash_reader(File::open(&destination).map_err(|error| {
                WorkbenchError::storage("Failed to inspect colliding blob", error)
            })?)?;
            if existing != blob.sha256 {
                return Err(WorkbenchError::invalid(
                    "Imported blob collides with different local content",
                ));
            }
        } else {
            fs::copy(temporary.path().join(&blob.archive_path), &destination).map_err(|error| {
                WorkbenchError::storage("Failed to restore immutable blob", error)
            })?;
        }
        reference_map.insert(
            blob.original_reference.clone(),
            destination.to_string_lossy().into_owned(),
        );
    }
    for (old, new) in &reference_map {
        for (table, column) in [
            ("artifacts", "storage_reference"),
            ("paper_revisions", "text_reference"),
            ("source_versions", "text_reference"),
        ] {
            imported
                .execute(
                    &format!("UPDATE {table} SET {column}=?2 WHERE {column}=?1"),
                    params![old, new],
                )
                .map_err(|error| {
                    WorkbenchError::storage("Failed to rebase imported blob reference", error)
                })?;
        }
    }
    for (table, column) in [
        ("artifacts", "storage_reference"),
        ("paper_revisions", "text_reference"),
        ("source_versions", "text_reference"),
    ] {
        let mut statement = imported
            .prepare(&format!(
                "SELECT DISTINCT {column} FROM {table} WHERE {column} IS NOT NULL"
            ))
            .map_err(|error| {
                WorkbenchError::storage("Failed to prepare imported path rebasing", error)
            })?;
        let references = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| WorkbenchError::storage("Failed to read imported paths", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| WorkbenchError::storage("Failed to decode imported paths", error))?;
        drop(statement);
        for reference in references {
            if let Ok(rest) = Path::new(&reference).strip_prefix(&blob_root) {
                // Lexical prefix checks accept `<blobs>/../x`; only plain
                // components may stay untouched.
                if rest
                    .components()
                    .all(|component| matches!(component, Component::Normal(_)))
                {
                    continue;
                }
                return Err(WorkbenchError::invalid(
                    "Imported blob reference contains traversal",
                ));
            }
            let Some(suffix) = reference.strip_prefix(&manifest.original_blob_root) else {
                return Err(WorkbenchError::invalid(
                    "Imported database contains a blob reference outside its declared blob root",
                ));
            };
            let mut rebased = blob_root.clone();
            for component in suffix
                .trim_start_matches(['/', '\\'])
                .split(['/', '\\'])
                .filter(|component| !component.is_empty())
            {
                if component == "." || component == ".." {
                    return Err(WorkbenchError::invalid(
                        "Imported blob reference contains traversal",
                    ));
                }
                rebased.push(component);
            }
            imported
                .execute(
                    &format!("UPDATE {table} SET {column}=?2 WHERE {column}=?1"),
                    params![reference, rebased.to_string_lossy()],
                )
                .map_err(|error| {
                    WorkbenchError::storage("Failed to rebase imported blob directory", error)
                })?;
        }
    }
    imported.execute_batch("UPDATE discovery_roles SET enabled=0; DELETE FROM execution_plan_state; UPDATE experiment_runs SET state='attention',authorized_hash=NULL,reason='Restored material requires execution reconciliation and new authorization'; UPDATE scheduled_checks SET enabled=0; UPDATE research_followups SET state='attention' WHERE state IN ('queued','dispatching','running'); DELETE FROM research_fts; UPDATE research_index_state SET generation=generation+1,cursor='',byte_offset=0,complete=0;").map_err(|e|WorkbenchError::storage("Reset imported desk authority and projection",e))?;
    imported
        .execute("DELETE FROM execution_authorizations", [])
        .map_err(|error| {
            WorkbenchError::storage("Failed to retire imported host authorizations", error)
        })?;
    imported
        .execute("DELETE FROM project_operations", [])
        .map_err(|error| {
            WorkbenchError::storage("Failed to retire imported local operations", error)
        })?;
    imported
        .execute("DELETE FROM storage_trash", [])
        .map_err(|error| {
            WorkbenchError::storage("Failed to discard imported trash authority", error)
        })?;
    imported.execute("UPDATE turns SET state='interrupted',terminal_at=?1,updated_at=?1,error_json=COALESCE(error_json,?2) WHERE terminal_at IS NULL", params![now(), json!({"message":"Imported unfinished turn: outcome is unknown; no work was replayed."}).to_string()])
        .map_err(|error| WorkbenchError::storage("Failed to reconcile imported unfinished turns", error))?;
    let retired=imported.execute("UPDATE session_bindings SET retired_at=COALESCE(retired_at,?1), retirement_reason=COALESCE(retirement_reason,'imported_native_binding_unavailable') WHERE retired_at IS NULL",[now()]).map_err(|error| WorkbenchError::storage("Failed to retire nonportable native bindings", error))?;
    imported
        .execute(
            "UPDATE execution_profiles SET tested_at=NULL,test_status=NULL",
            [],
        )
        .map_err(|error| {
            WorkbenchError::storage("Failed to require imported profile retesting", error)
        })?;
    imported
        .execute("UPDATE context_snapshots SET body_reference=NULL", [])
        .map_err(|error| {
            WorkbenchError::storage("Failed to clear nonportable context references", error)
        })?;
    imported
        .execute("UPDATE review_handoffs SET staged_path=''", [])
        .map_err(|error| {
            WorkbenchError::storage("Failed to clear imported handoff staging paths", error)
        })?;
    drop(imported);
    let imported = Connection::open(&database_path).map_err(|error| {
        WorkbenchError::storage("Failed to reopen validated archive database", error)
    })?;
    let mut installed =
        conversation_files::install(store, &manifest.conversation_files, temporary.path())?;
    let mut destination = store.connection()?;
    let backup = Backup::new(&imported, &mut destination)
        .map_err(|error| WorkbenchError::storage("Failed to start archive restore", error))?;
    backup
        .run_to_completion(100, Duration::from_millis(5), None)
        .map_err(|error| WorkbenchError::storage("Failed to restore archive database", error))?;
    drop(backup);
    installed.commit();
    let workspace_count = destination
        .query_row("SELECT COUNT(*) FROM workspaces", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|error| WorkbenchError::storage("Failed to count restored Workspaces", error))?
        as usize;
    let session_count = destination
        .query_row("SELECT COUNT(*) FROM sessions", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|error| WorkbenchError::storage("Failed to count restored conversations", error))?
        as usize;
    Ok(ArchiveReport {
        path: source_path.to_string_lossy().into_owned(),
        workspace_count,
        session_count,
        blob_count: manifest.blobs.len(),
        bytes: archive_size,
        native_bindings_retired: retired,
        portability_note: manifest.portability_note,
    })
}
