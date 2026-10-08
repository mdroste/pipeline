//! Portable `.pwrx` research-store snapshots and restore validation.

use super::now;
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult, CURRENT_SCHEMA_VERSION};
use rusqlite::{backup::Backup, params, types::Type, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use zip::write::SimpleFileOptions;

mod conversation_files;
const ARCHIVE_FORMAT_VERSION: u32 = 2;
const MAX_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 20_000;
const MAX_ENTRY_BYTES: u64 = 512 * 1024 * 1024;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlobManifest {
    archive_path: String,
    original_reference: String,
    size_bytes: u64,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveManifest {
    format: String,
    format_version: u32,
    store_schema_version: u32,
    created_at: String,
    original_blob_root: String,
    #[serde(default)]
    workspace_roots: Vec<String>,
    blobs: Vec<BlobManifest>,
    #[serde(default)]
    conversation_files: Vec<BlobManifest>,
    portability_note: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportArchiveRequest {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportArchiveRequest {
    pub path: String,
    #[serde(default)]
    pub root_mappings: BTreeMap<String, Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveReport {
    pub path: String,
    pub workspace_count: usize,
    pub session_count: usize,
    pub blob_count: usize,
    pub bytes: u64,
    pub native_bindings_retired: usize,
    pub portability_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInspection {
    pub path: String,
    pub format_version: u32,
    pub store_schema_version: u32,
    pub workspace_roots: Vec<String>,
    pub portability_note: String,
}

fn validate_manifest(manifest: &ArchiveManifest) -> WorkbenchResult<()> {
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
        let name = entry.name().to_string();
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
            if entry.size() > 4 * 1024 * 1024 {
                return Err(WorkbenchError::invalid("Archive manifest exceeds 4 MiB"));
            }
            manifest_index = Some(index);
        }
    }
    let index =
        manifest_index.ok_or_else(|| WorkbenchError::invalid("Archive manifest is missing"))?;
    let manifest: ArchiveManifest =
        serde_json::from_reader(archive.by_index(index).map_err(|error| {
            WorkbenchError::invalid(format!("Invalid archive manifest: {error}"))
        })?)
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

fn transcript_text(payload: &Value) -> Option<String> {
    for key in ["text", "message", "content"] {
        match payload.get(key) {
            Some(Value::String(text)) if !text.is_empty() => return Some(text.clone()),
            Some(Value::Array(parts)) => {
                let text = parts
                    .iter()
                    .filter_map(|part| {
                        part.as_str()
                            .or_else(|| part.get("text").and_then(Value::as_str))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.is_empty() {
                    return Some(text);
                }
            }
            _ => {}
        }
    }
    None
}

fn transcript_items(connection: &Connection, session_id: &str) -> WorkbenchResult<Vec<Value>> {
    let mut statement = connection.prepare(
        "SELECT i.id,i.turn_id,i.provider_item_id,i.item_kind,i.payload_json,i.is_final,i.created_at,i.updated_at \
         FROM transcript_items i JOIN session_bindings b ON b.id=i.binding_id \
         WHERE b.session_id=?1 AND NOT EXISTS (\
           SELECT 1 FROM transcript_items newer JOIN session_bindings nb ON nb.id=newer.binding_id \
           WHERE nb.session_id=b.session_id AND newer.provider_item_id=i.provider_item_id AND nb.incarnation>b.incarnation\
         ) ORDER BY i.created_at,i.id",
    ).map_err(|error| WorkbenchError::storage("Failed to prepare transcript export", error))?;
    let items = statement
        .query_map([session_id], |row| {
            let payload: String = row.get(4)?;
            let payload = serde_json::from_str::<Value>(&payload).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(4, Type::Text, Box::new(error))
            })?;
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "turnId": row.get::<_, Option<String>>(1)?,
                "providerItemId": row.get::<_, String>(2)?,
                "itemKind": row.get::<_, String>(3)?,
                "payload": payload,
                "isFinal": row.get::<_, bool>(5)?,
                "createdAt": row.get::<_, String>(6)?,
                "updatedAt": row.get::<_, String>(7)?,
            }))
        })
        .map_err(|error| WorkbenchError::storage("Failed to read transcript", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkbenchError::storage("Failed to decode transcript", error))?;
    Ok(items)
}

fn transcript_markdown(title: &str, items: &[Value]) -> String {
    let mut body = format!("# {title}\n\n");
    for item in items {
        let kind = item.get("itemKind").and_then(Value::as_str).unwrap_or("");
        if !kind.to_ascii_lowercase().contains("message") {
            continue;
        }
        let text = item.get("payload").and_then(transcript_text);
        if let Some(text) = text {
            let role = if kind.to_ascii_lowercase().contains("user") {
                "You"
            } else {
                "ChatGPT"
            };
            body.push_str(&format!("## {role}\n\n{text}\n\n"));
        }
    }
    body
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
    let note="Readable transcripts and portable research records do not guarantee that native Codex threads can resume on another machine or account. When a binding is unavailable, start a new session from reviewed notes and selected evidence.".to_string();
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

pub(super) fn safe_archive_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && Path::new(name)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn validate_database(connection: &Connection) -> WorkbenchResult<()> {
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
        if name == "manifest.json" && entry.size() > 4 * 1024 * 1024 {
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
        std::io::copy(&mut entry, &mut output).map_err(|error| {
            WorkbenchError::storage("Failed to extract research archive", error)
        })?;
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
                let canonical = super::super::store::canonical_workspace_root(
                    Path::new(new),
                    store.root_path(),
                )?;
                let identity = super::super::store::root_identity(&canonical)?;
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
            if Path::new(&reference).starts_with(&blob_root) {
                continue;
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
