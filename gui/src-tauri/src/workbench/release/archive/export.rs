//! Consistent research snapshots and portable archive publication.

use super::format::{
    ArchiveManifest, ArchiveReport, BlobManifest, ExportArchiveRequest, ARCHIVE_FORMAT_VERSION,
    MAX_ARCHIVE_BYTES, MAX_ARCHIVE_ENTRIES, MAX_ENTRY_BYTES,
};
use super::transcripts::{transcript_items, transcript_markdown};
use super::{conversation_files, hash_reader, now, validate_absolute_file};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult, CURRENT_SCHEMA_VERSION};
use rusqlite::{backup::Backup, params, Connection};
use serde_json::json;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use zip::write::SimpleFileOptions;

fn collect_files(root: &Path) -> WorkbenchResult<Vec<PathBuf>> {
    fn visit(
        root: &Path,
        at: &Path,
        files: &mut Vec<PathBuf>,
        bytes: &mut u64,
    ) -> WorkbenchResult<()> {
        for entry in fs::read_dir(at).map_err(|error| {
            WorkbenchError::storage("Failed to enumerate immutable blobs", error)
        })? {
            let entry = entry.map_err(|error| {
                WorkbenchError::storage("Failed to read immutable blob entry", error)
            })?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(|error| {
                WorkbenchError::storage("Failed to inspect immutable blob", error)
            })?;
            if metadata.file_type().is_symlink() {
                return Err(WorkbenchError::invalid(
                    "Immutable blob store contains a symbolic link",
                ));
            }
            if metadata.is_dir() {
                visit(root, &entry.path(), files, bytes)?;
            } else if metadata.is_file() {
                *bytes = bytes.saturating_add(metadata.len());
                if metadata.len() > MAX_ENTRY_BYTES
                    || *bytes > MAX_ARCHIVE_BYTES
                    || files.len() >= MAX_ARCHIVE_ENTRIES
                {
                    return Err(WorkbenchError::invalid(
                        "Research archive exceeds its entry or size limits",
                    ));
                }
                files.push(entry.path());
            }
        }
        let _ = root;
        Ok(())
    }
    let mut files = Vec::new();
    let mut bytes = 0;
    if root.exists() {
        visit(root, root, &mut files, &mut bytes)?;
    }
    Ok(files)
}

fn snapshot_database(source: &Connection, target: &Path) -> WorkbenchResult<()> {
    let mut destination = Connection::open(target).map_err(|error| {
        WorkbenchError::storage("Failed to create consistent archive snapshot", error)
    })?;
    let backup = Backup::new(source, &mut destination).map_err(|error| {
        WorkbenchError::storage("Failed to start consistent archive snapshot", error)
    })?;
    backup
        .run_to_completion(100, Duration::from_millis(5), None)
        .map_err(|error| {
            WorkbenchError::storage("Failed to finish consistent archive snapshot", error)
        })?;
    Ok(())
}

pub fn export_archive(
    store: &Store,
    request: ExportArchiveRequest,
) -> WorkbenchResult<ArchiveReport> {
    let target = validate_absolute_file(&request.path, "pwrx")?;
    if target.starts_with(store.root_path()) {
        return Err(WorkbenchError::invalid(
            "Choose an archive destination outside Workspace private storage",
        ));
    }
    let temporary = tempfile::tempdir().map_err(|error| {
        WorkbenchError::storage("Failed to create archive staging directory", error)
    })?;
    let snapshot_path = temporary.path().join("research.sqlite3");
    let source = store.connection()?;
    snapshot_database(&source, &snapshot_path)?;
    let snapshot = Connection::open(&snapshot_path)
        .map_err(|error| WorkbenchError::storage("Failed to open archive snapshot", error))?;
    let workspace_count = snapshot
        .query_row("SELECT COUNT(*) FROM workspaces", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|error| WorkbenchError::storage("Failed to count archived Workspaces", error))?
        as usize;
    let session_count = snapshot
        .query_row("SELECT COUNT(*) FROM sessions", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|error| WorkbenchError::storage("Failed to count archived conversations", error))?
        as usize;
    let workspace_roots = {
        let mut statement = snapshot
            .prepare("SELECT DISTINCT root FROM workspaces WHERE root IS NOT NULL ORDER BY root")
            .map_err(|error| {
                WorkbenchError::storage("Failed to prepare archive Workspace roots", error)
            })?;
        let roots = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| {
                WorkbenchError::storage("Failed to read archive Workspace roots", error)
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                WorkbenchError::storage("Failed to decode archive Workspace roots", error)
            })?;
        roots
    };
    let blob_root = store.root_path().join("blobs");
    let blob_files = collect_files(&blob_root)?;
    let logical_blob_root = "workspace://blobs";
    let mut blobs = Vec::new();
    for path in &blob_files {
        let relative = path
            .strip_prefix(&blob_root)
            .map_err(|_| WorkbenchError::invalid("Blob escaped private storage"))?;
        let archive_path = format!("blobs/{}", relative.to_string_lossy().replace('\\', "/"));
        let metadata = fs::metadata(path)
            .map_err(|error| WorkbenchError::storage("Failed to inspect archive blob", error))?;
        blobs.push(BlobManifest {
            archive_path,
            original_reference: format!(
                "{logical_blob_root}/{}",
                relative.to_string_lossy().replace('\\', "/")
            ),
            size_bytes: metadata.len(),
            sha256: hash_reader(
                File::open(path).map_err(|error| {
                    WorkbenchError::storage("Failed to open archive blob", error)
                })?,
            )?,
        });
    }
    let old_blob_root = blob_root.to_string_lossy().into_owned();
    for (table, column) in [
        ("artifacts", "storage_reference"),
        ("paper_revisions", "text_reference"),
        ("source_versions", "text_reference"),
    ] {
        snapshot
            .execute(
                &format!("UPDATE {table} SET {column}=?2 || substr({column},length(?1)+1) WHERE {column} IS NOT NULL AND substr({column},1,length(?1))=?1"),
                params![old_blob_root, logical_blob_root],
            )
            .map_err(|error| {
                WorkbenchError::storage("Failed to make archive blob references portable", error)
            })?;
        // Windows stores native separators; the manifest and importers use '/'.
        snapshot
            .execute(
                &format!("UPDATE {table} SET {column}=replace({column},'\\','/') WHERE {column} IS NOT NULL AND substr({column},1,length(?1))=?1"),
                params![logical_blob_root],
            )
            .map_err(|error| {
                WorkbenchError::storage("Failed to make archive blob references portable", error)
            })?;
    }
    snapshot
        .execute("UPDATE context_snapshots SET body_reference=NULL", [])
        .map_err(|error| {
            WorkbenchError::storage("Failed to remove private context paths from archive", error)
        })?;
    snapshot
        .execute("UPDATE review_handoffs SET staged_path=''", [])
        .map_err(|error| {
            WorkbenchError::storage("Failed to remove private handoff paths from archive", error)
        })?;
    snapshot
        .execute("DELETE FROM storage_trash", [])
        .map_err(|error| {
            WorkbenchError::storage("Failed to discard local trash authority", error)
        })?;
    let portable_snapshot_path = temporary.path().join("portable-research.sqlite3");
    snapshot_database(&snapshot, &portable_snapshot_path)?;
    let note="Conversations from this backup may not continue on another computer or account. If one can't, start a new conversation from your notes and evidence.".to_string();
    let conversation_files = conversation_files::stage(store, &snapshot, temporary.path())?;
    if 2 + session_count * 2 + blobs.len() + conversation_files.len() > MAX_ARCHIVE_ENTRIES {
        return Err(WorkbenchError::invalid(
            "Research archive contains too many entries",
        ));
    }
    let manifest = ArchiveManifest {
        format: "pipeline-workspace-research".into(),
        format_version: ARCHIVE_FORMAT_VERSION,
        store_schema_version: CURRENT_SCHEMA_VERSION,
        created_at: now(),
        original_blob_root: logical_blob_root.into(),
        workspace_roots,
        blobs,
        conversation_files,
        portability_note: note.clone(),
    };
    let parent = target
        .parent()
        .ok_or_else(|| WorkbenchError::invalid("Archive destination has no parent directory"))?;
    fs::create_dir_all(parent)
        .map_err(|error| WorkbenchError::storage("Failed to create archive destination", error))?;
    let temporary_archive = tempfile::Builder::new()
        .prefix(".workspace-research-")
        .suffix(".pwrx.partial")
        .tempfile_in(parent)
        .map_err(|error| WorkbenchError::storage("Failed to create research archive", error))?;
    let file = temporary_archive
        .reopen()
        .map_err(|error| WorkbenchError::storage("Failed to open research archive", error))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o600);
    zip.start_file("manifest.json", options).map_err(|error| {
        WorkbenchError::storage("Failed to write archive manifest entry", error)
    })?;
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|error| {
        WorkbenchError::invalid(format!("Failed to encode archive manifest: {error}"))
    })?;
    if manifest_json.len() > 4 * 1024 * 1024 {
        return Err(WorkbenchError::invalid("Archive manifest exceeds 4 MiB"));
    }
    zip.write_all(manifest_json.as_bytes())
        .map_err(|error| WorkbenchError::storage("Failed to write archive manifest", error))?;
    zip.start_file("database/research.sqlite3", options)
        .map_err(|error| {
            WorkbenchError::storage("Failed to write archive database entry", error)
        })?;
    std::io::copy(
        &mut File::open(&portable_snapshot_path)
            .map_err(|error| WorkbenchError::storage("Failed to read archive database", error))?,
        &mut zip,
    )
    .map_err(|error| WorkbenchError::storage("Failed to write archive database", error))?;
    let database_bytes = fs::metadata(&portable_snapshot_path)
        .map_err(|error| WorkbenchError::storage("Failed to inspect archive database", error))?
        .len();
    if database_bytes > MAX_ENTRY_BYTES {
        return Err(WorkbenchError::invalid(
            "Research database exceeds the 512 MiB archive-entry limit",
        ));
    }
    let mut expanded_bytes = database_bytes
        .saturating_add(manifest_json.len() as u64)
        .saturating_add(
            manifest
                .blobs
                .iter()
                .map(|blob| blob.size_bytes)
                .sum::<u64>(),
        );
    expanded_bytes = expanded_bytes.saturating_add(
        manifest
            .conversation_files
            .iter()
            .map(|f| f.size_bytes)
            .sum::<u64>(),
    );
    if expanded_bytes > MAX_ARCHIVE_BYTES {
        return Err(WorkbenchError::invalid("Expanded archive exceeds 2 GiB"));
    }
    {
        let mut statement = snapshot
            .prepare("SELECT id,title FROM sessions ORDER BY id")
            .map_err(|error| {
                WorkbenchError::storage("Failed to prepare transcript archive", error)
            })?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| WorkbenchError::storage("Failed to list transcripts", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| WorkbenchError::storage("Failed to decode transcripts", error))?;
        for (id, title) in rows {
            let items = transcript_items(&snapshot, &id)?;
            let md = transcript_markdown(&title, &items);
            let payload = json!({
                "schemaVersion": 1,
                "session": {"id": id, "title": title},
                "items": items,
            });
            let json = serde_json::to_string_pretty(&payload).map_err(|error| {
                WorkbenchError::invalid(format!("Failed to encode transcript: {error}"))
            })?;
            if md.len() as u64 > MAX_ENTRY_BYTES || json.len() as u64 > MAX_ENTRY_BYTES {
                return Err(WorkbenchError::invalid(
                    "A readable transcript exceeds the 512 MiB archive-entry limit",
                ));
            }
            expanded_bytes = expanded_bytes
                .saturating_add(md.len() as u64)
                .saturating_add(json.len() as u64);
            if expanded_bytes > MAX_ARCHIVE_BYTES {
                return Err(WorkbenchError::invalid(
                    "Expanded research archive exceeds 2 GiB",
                ));
            }
            zip.start_file(format!("transcripts/{id}.md"), options)
                .map_err(|error| {
                    WorkbenchError::storage("Failed to add Markdown transcript", error)
                })?;
            zip.write_all(md.as_bytes()).map_err(|error| {
                WorkbenchError::storage("Failed to write Markdown transcript", error)
            })?;
            zip.start_file(format!("transcripts/{id}.json"), options)
                .map_err(|error| WorkbenchError::storage("Failed to add JSON transcript", error))?;
            zip.write_all(json.as_bytes()).map_err(|error| {
                WorkbenchError::storage("Failed to write JSON transcript", error)
            })?;
        }
    }
    for (blob, source_path) in manifest.blobs.iter().zip(&blob_files) {
        zip.start_file(&blob.archive_path, options)
            .map_err(|error| WorkbenchError::storage("Failed to add immutable blob", error))?;
        std::io::copy(
            &mut File::open(source_path).map_err(|error| {
                WorkbenchError::storage("Failed to reopen immutable blob", error)
            })?,
            &mut zip,
        )
        .map_err(|error| WorkbenchError::storage("Failed to copy immutable blob", error))?;
    }
    for file in &manifest.conversation_files {
        zip.start_file(&file.archive_path, options)
            .map_err(|e| WorkbenchError::storage("Add conversation file", e))?;
        std::io::copy(
            &mut File::open(temporary.path().join(&file.archive_path))
                .map_err(|e| WorkbenchError::storage("Open staged conversation file", e))?,
            &mut zip,
        )
        .map_err(|e| WorkbenchError::storage("Write conversation file", e))?;
    }
    let file = zip
        .finish()
        .map_err(|error| WorkbenchError::storage("Failed to finalize research archive", error))?;
    if file
        .metadata()
        .map_err(|e| WorkbenchError::storage("Inspect staged archive", e))?
        .len()
        > MAX_ARCHIVE_BYTES
    {
        return Err(WorkbenchError::invalid("Research archive exceeds 2 GiB"));
    }
    file.sync_all()
        .map_err(|error| WorkbenchError::storage("Failed to sync research archive", error))?;
    drop(file);
    temporary_archive.persist(&target).map_err(|error| {
        WorkbenchError::storage("Failed to publish research archive", error.error)
    })?;
    let bytes = fs::metadata(&target)
        .map_err(|error| WorkbenchError::storage("Failed to inspect research archive", error))?
        .len();
    Ok(ArchiveReport {
        path: target.to_string_lossy().into_owned(),
        workspace_count,
        session_count,
        blob_count: manifest.blobs.len(),
        bytes,
        native_bindings_retired: 0,
        portability_note: note,
    })
}
