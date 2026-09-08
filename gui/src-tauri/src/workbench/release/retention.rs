//! Disk-use reporting and recoverable retention controls for the private
//! Workspace store. Disposable build/cache material is separated from
//! evidence-linked immutable blobs; pruning moves files into a private trash
//! with a journal, and only an explicit second action deletes them.
use super::now;
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_WALK_ENTRIES: usize = 200_000;
pub const DISPOSABLE: [&str; 4] = [
    "blobs_unreferenced",
    "job_scratch",
    "context_unreferenced",
    "migration_backups",
];

fn err(e: impl std::fmt::Display) -> WorkbenchError {
    WorkbenchError::storage("Storage operation failed", e)
}
fn random_id(prefix: &str) -> WorkbenchResult<String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(err)?;
    Ok(format!(
        "{prefix}_{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}
/// Size of a file or directory tree, without following symlinks.
fn tree_size(path: &Path, budget: &mut usize) -> WorkbenchResult<u64> {
    let meta = fs::symlink_metadata(path).map_err(err)?;
    if meta.file_type().is_symlink() {
        return Ok(0);
    }
    if meta.is_file() {
        return Ok(meta.len());
    }
    let mut total = 0;
    for entry in fs::read_dir(path).map_err(err)? {
        *budget = budget.saturating_sub(1);
        if *budget == 0 {
            return Err(WorkbenchError::invalid(
                "Storage inspection exceeded 200,000 entries",
            ));
        }
        total += tree_size(&entry.map_err(err)?.path(), budget)?;
    }
    Ok(total)
}
/// Immediate children of a store directory as (path, bytes).
fn children(dir: &Path, budget: &mut usize) -> WorkbenchResult<Vec<(PathBuf, u64)>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).map_err(err)? {
        let path = entry.map_err(err)?.path();
        *budget = budget.saturating_sub(1);
        if *budget == 0 {
            return Err(WorkbenchError::invalid(
                "Storage inspection exceeded 200,000 entries",
            ));
        }
        let size = tree_size(&path, budget)?;
        out.push((path, size));
    }
    out.sort();
    Ok(out)
}
fn references(store: &Store) -> WorkbenchResult<HashSet<String>> {
    let conn = store.connection()?;
    let mut refs = HashSet::new();
    for sql in [
        "SELECT storage_reference FROM artifacts",
        "SELECT text_reference FROM paper_revisions WHERE text_reference IS NOT NULL",
        "SELECT text_reference FROM source_versions WHERE text_reference IS NOT NULL",
        "SELECT storage_reference FROM retained_blobs",
        "SELECT body_reference FROM context_snapshots WHERE body_reference IS NOT NULL",
    ] {
        let mut stmt = conn.prepare(sql).map_err(err)?;
        for row in stmt.query_map([], |r| r.get::<_, String>(0)).map_err(err)? {
            refs.insert(row.map_err(err)?);
        }
    }
    Ok(refs)
}
fn referenced(path: &Path, refs: &HashSet<String>) -> bool {
    let text = path.to_string_lossy();
    refs.contains(text.as_ref()) || refs.iter().any(|r| r.starts_with(&format!("{text}/")))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageCategory {
    pub key: String,
    pub label: String,
    pub bytes: u64,
    pub entries: usize,
    pub disposable: bool,
    pub note: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashEntry {
    pub id: String,
    pub category: String,
    pub original_path: String,
    pub size_bytes: u64,
    pub reason: String,
    pub moved_at: String,
    pub state: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageReport {
    pub root: String,
    pub categories: Vec<StorageCategory>,
    pub trash: Vec<TrashEntry>,
    pub active_jobs: usize,
    pub note: String,
}
struct Scan {
    items: BTreeMap<String, Vec<(PathBuf, u64)>>,
}
fn scan(store: &Store) -> WorkbenchResult<Scan> {
    let refs = references(store)?;
    let root = store.root_path();
    let mut budget = MAX_WALK_ENTRIES;
    let mut items: BTreeMap<String, Vec<(PathBuf, u64)>> = BTreeMap::new();
    for (path, size) in children(&root.join("blobs"), &mut budget)? {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        if name.as_deref().is_some_and(|n| n.ends_with(".pending")) {
            items
                .entry("blobs_staging".into())
                .or_default()
                .push((path, size));
        } else if referenced(&path, &refs) {
            items
                .entry("blobs_referenced".into())
                .or_default()
                .push((path, size));
        } else {
            items
                .entry("blobs_unreferenced".into())
                .or_default()
                .push((path, size));
        }
    }
    for (path, size) in children(&root.join("jobs"), &mut budget)? {
        let category = if path.file_name().is_some_and(|name| name == "conversations") {
            "conversation_files"
        } else {
            "job_scratch"
        };
        items.entry(category.into()).or_default().push((path, size));
    }
    for (path, size) in children(&root.join("context"), &mut budget)? {
        if referenced(&path, &refs) {
            items
                .entry("context_referenced".into())
                .or_default()
                .push((path, size));
        } else {
            items
                .entry("context_unreferenced".into())
                .or_default()
                .push((path, size));
        }
    }
    for (path, size) in children(&root.join("backups"), &mut budget)? {
        items
            .entry("migration_backups".into())
            .or_default()
            .push((path, size));
    }
    for (path, size) in children(&root.join("trash"), &mut budget)? {
        items.entry("trash".into()).or_default().push((path, size));
    }
    Ok(Scan { items })
}
fn active_jobs(store: &Store) -> WorkbenchResult<usize> {
    store
        .connection()?
        .query_row(
            "SELECT (SELECT COUNT(*) FROM research_executions WHERE outcome IN ('queued','running')) + (SELECT COUNT(*) FROM turns WHERE state='running')",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n.max(0) as usize)
        .map_err(err)
}
pub fn list_trash(store: &Store) -> WorkbenchResult<Vec<TrashEntry>> {
    let conn = store.connection()?;
    // A durable row precedes the move. An interrupted move or restore can leave
    // the file at its original path; reconcile before offering recovery controls.
    // Production callers hold the exclusive store gate across this inspection.
    let pending = {
        let mut q = conn
            .prepare("SELECT id,original_path,trash_path FROM storage_trash WHERE state='trashed'")
            .map_err(err)?;
        let rows = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        rows
    };
    for (id, original, trash) in pending {
        if !Path::new(&trash).try_exists().map_err(err)?
            && Path::new(&original).try_exists().map_err(err)?
        {
            conn.execute("UPDATE storage_trash SET state='restored',finished_at=?2 WHERE id=?1 AND state='trashed'", params![id, now()]).map_err(err)?;
        }
    }
    let mut stmt = conn.prepare("SELECT id,category,original_path,size_bytes,reason,moved_at,state FROM storage_trash WHERE state='trashed' ORDER BY moved_at DESC LIMIT 1000").map_err(err)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(TrashEntry {
                id: r.get(0)?,
                category: r.get(1)?,
                original_path: r.get(2)?,
                size_bytes: r.get::<_, i64>(3)?.max(0) as u64,
                reason: r.get(4)?,
                moved_at: r.get(5)?,
                state: r.get(6)?,
            })
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    Ok(rows)
}
pub fn storage_report(store: &Store) -> WorkbenchResult<StorageReport> {
    let scan = scan(store)?;
    let active = active_jobs(store)?;
    fn describe(key: &str) -> (&'static str, bool, &'static str) {
        match key {
            "blobs_referenced" => ("Immutable evidence blobs (referenced)", false, "Referenced by artifacts, paper or source text, evidence, exchange imports or context snapshots; never pruned."),
            "blobs_unreferenced" => ("Immutable blobs without a current reference", true, "No record references these files; they can be moved to trash and restored until trash is emptied."),
            "blobs_staging" => ("Blob staging in progress", false, "Partial captures; left alone."),
            "job_scratch" => ("Execution scratch", true, "Finished job material. Retained receipts and adopted outputs live in blobs, not here."),
            "conversation_files" => ("Conversation working files", false, "Working files for conversations without a registered folder; retained with the conversation."),
            "context_referenced" => ("Turn context snapshots (referenced)", false, "Referenced by recorded turns."),
            "context_unreferenced" => ("Turn context files without a recorded turn", true, "Not referenced by any context snapshot."),
            "migration_backups" => ("Pre-migration database backups", true, "Safety copies made before schema migrations; restorable from trash until emptied."),
            "trash" => ("Trash", false, "Pruned material awaiting restore or explicit deletion."),
            _ => ("Other", false, ""),
        }
    }
    let mut categories = Vec::new();
    for key in [
        "blobs_referenced",
        "blobs_unreferenced",
        "blobs_staging",
        "job_scratch",
        "conversation_files",
        "context_referenced",
        "context_unreferenced",
        "migration_backups",
        "trash",
    ] {
        let items = scan.items.get(key).cloned().unwrap_or_default();
        let (label, disposable, note) = describe(key);
        let disposable = disposable && !(key == "job_scratch" && active > 0);
        categories.push(StorageCategory {
            key: key.into(),
            label: label.into(),
            bytes: items.iter().map(|(_, b)| b).sum(),
            entries: items.len(),
            disposable,
            note: if key == "job_scratch" && active > 0 {
                format!(
                    "{note} Pruning is unavailable while {active} job(s) or conversation turn(s) are active."
                )
            } else {
                note.into()
            },
        });
    }
    Ok(StorageReport {
        root: store.root_path().to_string_lossy().into_owned(),
        categories,
        trash: list_trash(store)?,
        active_jobs: active,
        note: "Pruning never touches referenced evidence, credentials, the database or Workflow runs. Pruned files go to a private trash journal and can be restored until trash is emptied.".into(),
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PruneRequest {
    pub categories: Vec<String>,
    pub apply: bool,
    pub expected_preview_token: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PruneEntry {
    pub category: String,
    pub path: String,
    pub bytes: u64,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrunePlan {
    pub entries: Vec<PruneEntry>,
    pub bytes: u64,
    pub applied: bool,
    pub trash_ids: Vec<String>,
    pub preview_token: String,
}

fn fingerprint_tree(path: &Path, digest: &mut Sha256, budget: &mut usize) -> WorkbenchResult<()> {
    *budget = budget
        .checked_sub(1)
        .ok_or_else(|| WorkbenchError::invalid("Storage preview exceeded 200,000 entries"))?;
    let metadata = fs::symlink_metadata(path).map_err(err)?;
    let identity = serde_json::to_vec(&(
        path.to_string_lossy(),
        metadata.len(),
        metadata
            .modified()
            .map_err(err)?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|t| t.as_nanos()),
        metadata.is_dir(),
        metadata.file_type().is_symlink(),
    ))
    .map_err(err)?;
    digest.update((identity.len() as u64).to_le_bytes());
    digest.update(identity);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        for value in [
            metadata.dev(),
            metadata.ino(),
            metadata.ctime() as u64,
            metadata.ctime_nsec() as u64,
        ] {
            digest.update(value.to_le_bytes());
        }
    }
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        let mut children = fs::read_dir(path)
            .map_err(err)?
            .map(|e| e.map(|e| e.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        children.sort();
        for child in children {
            fingerprint_tree(&child, digest, budget)?;
        }
    }
    Ok(())
}

pub fn prune_storage(store: &Store, mut request: PruneRequest) -> WorkbenchResult<PrunePlan> {
    request.categories.sort();
    request.categories.dedup();
    for c in &request.categories {
        if !DISPOSABLE.contains(&c.as_str()) {
            return Err(WorkbenchError::invalid(format!(
                "{c} is not a disposable storage category"
            )));
        }
    }
    if request.categories.contains(&"job_scratch".to_string()) && active_jobs(store)? > 0 {
        return Err(WorkbenchError::invalid(
            "Stop or finish local jobs and Workspace turns before pruning execution scratch",
        ));
    }
    let scan = scan(store)?;
    let mut entries = Vec::new();
    for category in &request.categories {
        for (path, bytes) in scan.items.get(category).cloned().unwrap_or_default() {
            entries.push(PruneEntry {
                category: category.clone(),
                path: path.to_string_lossy().into_owned(),
                bytes,
            });
        }
    }
    let bytes = entries.iter().map(|e| e.bytes).sum();
    let mut digest = Sha256::new();
    digest.update(b"workspace-prune-v1\0");
    digest.update(
        serde_json::to_vec(&(store.root_path(), &request.categories, &entries)).map_err(err)?,
    );
    let mut budget = MAX_WALK_ENTRIES;
    for entry in &entries {
        fingerprint_tree(Path::new(&entry.path), &mut digest, &mut budget)?;
    }
    let preview_token = format!("{:x}", digest.finalize());
    if !request.apply {
        return Ok(PrunePlan {
            entries,
            bytes,
            applied: false,
            trash_ids: Vec::new(),
            preview_token,
        });
    }
    if request.expected_preview_token.as_deref() != Some(preview_token.as_str()) {
        return Err(WorkbenchError::conflict("Storage changed or no matching preview was provided. Preview deletion again; nothing was moved."));
    }
    // Resolve an earlier interrupted intent before retrying a move of that path.
    list_trash(store)?;
    let refs = references(store)?;
    let trash_root = store.root_path().join("trash");
    fs::create_dir_all(&trash_root).map_err(err)?;
    let mut ids = Vec::new();
    let mut conn = store.connection()?;
    for entry in &entries {
        let path = PathBuf::from(&entry.path);
        if !path.starts_with(store.root_path()) || referenced(&path, &refs) {
            return Err(WorkbenchError::invalid(
                "A referenced or foreign path was scheduled for pruning; nothing was moved",
            ));
        }
    }
    for entry in &entries {
        let path = PathBuf::from(&entry.path);
        if !path.exists() {
            continue;
        }
        let id = random_id("trash")?;
        let relative = path
            .strip_prefix(store.root_path())
            .map_err(|_| WorkbenchError::invalid("Path escaped private storage"))?;
        let destination = trash_root.join(&id).join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(err)?;
        }
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        tx.execute("INSERT INTO storage_trash(id,category,original_path,trash_path,size_bytes,reason,moved_at,state) VALUES(?1,?2,?3,?4,?5,?6,?7,'trashed')", params![id, entry.category, entry.path, destination.to_string_lossy(), entry.bytes as i64, format!("pruned {}", entry.category), now()]).map_err(err)?;
        // Never move bytes until their recovery path is durably recorded.
        tx.commit().map_err(err)?;
        fs::rename(&path, &destination).map_err(err)?;
        ids.push(id);
    }
    Ok(PrunePlan {
        entries,
        bytes,
        applied: true,
        trash_ids: ids,
        preview_token,
    })
}
pub fn restore_trash(store: &Store, trash_id: &str) -> WorkbenchResult<TrashEntry> {
    let entry = list_trash(store)?
        .into_iter()
        .find(|t| t.id == trash_id)
        .ok_or_else(|| WorkbenchError::invalid("Trash entry was not found"))?;
    let trash_path: String = store
        .connection()?
        .query_row(
            "SELECT trash_path FROM storage_trash WHERE id=?1",
            [trash_id],
            |r| r.get(0),
        )
        .map_err(err)?;
    let original = PathBuf::from(&entry.original_path);
    if original.exists() {
        return Err(WorkbenchError::conflict(
            "The original path exists again; resolve it before restoring",
        ));
    }
    if let Some(parent) = original.parent() {
        fs::create_dir_all(parent).map_err(err)?;
    }
    fs::rename(&trash_path, &original).map_err(err)?;
    store
        .connection()?
        .execute(
            "UPDATE storage_trash SET state='restored', finished_at=?2 WHERE id=?1",
            params![trash_id, now()],
        )
        .map_err(err)?;
    let _ = fs::remove_dir_all(store.root_path().join("trash").join(trash_id));
    Ok(TrashEntry {
        state: "restored".into(),
        ..entry
    })
}
pub fn empty_trash(store: &Store) -> WorkbenchResult<usize> {
    let entries = list_trash(store)?;
    let trash_root = store.root_path().join("trash");
    let mut removed = 0;
    for entry in entries {
        let dir = trash_root.join(&entry.id);
        if dir.starts_with(&trash_root) && dir.exists() {
            fs::remove_dir_all(&dir).map_err(err)?;
        }
        store
            .connection()?
            .execute(
                "UPDATE storage_trash SET state='deleted', finished_at=?2 WHERE id=?1",
                params![entry.id, now()],
            )
            .map_err(err)?;
        removed += 1;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::store::CreateWorkspaceRequest;

    fn prune(store: &Store, categories: &[&str], apply: bool) -> WorkbenchResult<PrunePlan> {
        let categories: Vec<String> = categories.iter().map(|c| (*c).into()).collect();
        let preview = prune_storage(
            store,
            PruneRequest {
                categories: categories.clone(),
                apply: false,
                expected_preview_token: None,
            },
        )?;
        if !apply {
            return Ok(preview);
        }
        prune_storage(
            store,
            PruneRequest {
                categories,
                apply: true,
                expected_preview_token: Some(preview.preview_token),
            },
        )
    }

    #[test]
    fn prune_requires_a_current_preview_of_categories_and_file_identities() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open_at(temp.path()).unwrap();
        let dir = store.root_path().join("jobs/finished");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("result.txt");
        fs::write(&file, "first").unwrap();
        let apply = |token, categories| {
            prune_storage(
                &store,
                PruneRequest {
                    categories,
                    apply: true,
                    expected_preview_token: token,
                },
            )
        };
        assert!(apply(None, vec!["job_scratch".into()]).is_err());
        let preview = prune(&store, &["job_scratch"], false).unwrap();
        fs::write(dir.join("new.txt"), "new").unwrap();
        assert!(apply(Some(preview.preview_token), vec!["job_scratch".into()]).is_err());
        let preview = prune(&store, &["job_scratch"], false).unwrap();
        // Same name and length, different file identity.
        let replacement = store.root_path().join("replacement");
        fs::write(&replacement, "other").unwrap();
        fs::rename(replacement, &file).unwrap();
        assert!(apply(Some(preview.preview_token), vec!["job_scratch".into()]).is_err());
        let preview = prune(&store, &["job_scratch"], false).unwrap();
        assert!(apply(
            Some(preview.preview_token.clone()),
            vec!["job_scratch".into(), "blobs_unreferenced".into()]
        )
        .is_err());
        assert!(file.exists());
        assert!(
            apply(Some(preview.preview_token), vec!["job_scratch".into()])
                .unwrap()
                .applied
        );
        assert!(!dir.exists());
    }

    #[test]
    fn failed_trash_commit_leaves_the_original_file_in_place() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open_at(temp.path()).unwrap();
        let file = store.root_path().join("blobs/orphan.txt");
        fs::write(&file, "recoverable work").unwrap();
        store.connection().unwrap().execute_batch(
            "CREATE TABLE commit_failure(value TEXT REFERENCES workspaces(id) DEFERRABLE INITIALLY DEFERRED);
             CREATE TRIGGER defer_failure AFTER INSERT ON storage_trash BEGIN INSERT INTO commit_failure VALUES('missing'); END;"
        ).unwrap();
        assert!(prune(&store, &["blobs_unreferenced"], true).is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "recoverable work");
        assert!(list_trash(&store).unwrap().is_empty());
    }

    #[test]
    fn trash_reconciles_interruptions_before_move_and_after_restore() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open_at(temp.path()).unwrap();
        let file = store.root_path().join("blobs/orphan.txt");
        let trash = store.root_path().join("trash/interrupted/blobs/orphan.txt");
        fs::write(&file, "recoverable work").unwrap();
        store.connection().unwrap().execute("INSERT INTO storage_trash(id,category,original_path,trash_path,size_bytes,reason,moved_at,state) VALUES('interrupted','blobs_unreferenced',?1,?2,16,'test','t','trashed')", params![file.to_string_lossy(), trash.to_string_lossy()]).unwrap();
        // Crash after committing the intent, before moving the file.
        let store = Store::open_at(temp.path()).unwrap();
        assert!(list_trash(&store).unwrap().is_empty());
        assert!(file.exists());
        // Crash after moving into trash: the same durable intent exposes it.
        fs::create_dir_all(trash.parent().unwrap()).unwrap();
        store
            .connection()
            .unwrap()
            .execute(
                "UPDATE storage_trash SET state='trashed' WHERE id='interrupted'",
                [],
            )
            .unwrap();
        fs::rename(&file, &trash).unwrap();
        let store = Store::open_at(temp.path()).unwrap();
        assert_eq!(list_trash(&store).unwrap().len(), 1);
        // Simulate an interrupted restore after its rename but before its UPDATE.
        fs::rename(&trash, &file).unwrap();
        let store = Store::open_at(temp.path()).unwrap();
        assert!(list_trash(&store).unwrap().is_empty());
        assert_eq!(fs::read_to_string(&file).unwrap(), "recoverable work");
    }

    #[test]
    fn scratch_pruning_preserves_conversation_files_and_rejects_active_turns() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open_at(temp.path()).unwrap();
        let session = store
            .create_session(crate::workbench::store::CreateSessionRequest {
                workspace_id: None,
                title: "Unfiled".into(),
                operation_id: "session".into(),
            })
            .unwrap()
            .record
            .id;
        let file = store.runtime_root(&session).unwrap().join("work.md");
        fs::write(&file, "conversation work").unwrap();
        let scratch = store.root_path().join("jobs/finished");
        fs::create_dir_all(&scratch).unwrap();
        fs::write(scratch.join("out.txt"), "scratch").unwrap();
        let c = store.connection().unwrap();
        c.execute("INSERT INTO session_bindings(id,session_id,runtime_namespace,provider_thread_id,incarnation,created_at) VALUES('b',?1,'test','thread',1,'t')", [&session]).unwrap();
        c.execute("INSERT INTO turns(id,binding_id,client_submission_id,state,created_at,updated_at) VALUES('t','b','submit','running','t','t')", []).unwrap();
        assert!(prune(&store, &["job_scratch"], true).is_err());
        assert!(file.exists() && scratch.exists());
        c.execute("UPDATE turns SET state='completed' WHERE id='t'", [])
            .unwrap();
        let plan = prune(&store, &["job_scratch"], true).unwrap();
        assert_eq!(plan.entries.len(), 1);
        assert!(file.exists());
        assert!(!scratch.exists());
        assert!(
            !storage_report(&store)
                .unwrap()
                .categories
                .iter()
                .find(|c| c.key == "conversation_files")
                .unwrap()
                .disposable
        );
    }

    #[test]
    fn prune_preserves_referenced_evidence_and_is_recoverable() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open_at(temp.path()).unwrap();
        let ws = store
            .create_workspace(CreateWorkspaceRequest {
                name: "Retention".into(),
                root: None,
                operation_id: "create".into(),
            })
            .unwrap()
            .record
            .id;
        let blobs = store.root_path().join("blobs");
        let referenced = blobs.join("aaaa.txt");
        let orphan = blobs.join("bbbb.txt");
        fs::write(&referenced, "evidence").unwrap();
        fs::write(&orphan, "orphan bytes").unwrap();
        store.connection().unwrap().execute("INSERT INTO artifacts(id,workspace_id,content_hash,media_kind,size_bytes,origin,storage_reference,created_at) VALUES('a1',?1,'h','txt',8,'test',?2,'t')", params![ws, referenced.to_string_lossy()]).unwrap();
        let context = store.root_path().join("context");
        fs::create_dir_all(&context).unwrap();
        fs::write(context.join("stale.txt"), "stale").unwrap();
        let report = storage_report(&store).unwrap();
        let cat = |k: &str| {
            report
                .categories
                .iter()
                .find(|c| c.key == k)
                .unwrap()
                .clone()
        };
        assert_eq!(cat("blobs_referenced").entries, 1);
        assert_eq!(cat("blobs_unreferenced").entries, 1);
        assert_eq!(cat("context_unreferenced").entries, 1);
        assert!(!cat("blobs_referenced").disposable);
        assert!(prune(&store, &["blobs_referenced"], true).is_err());
        let preview = prune(
            &store,
            &["blobs_unreferenced", "context_unreferenced"],
            false,
        )
        .unwrap();
        assert_eq!(preview.entries.len(), 2);
        assert!(!preview.applied);
        assert!(orphan.exists());
        let applied = prune(&store, &["blobs_unreferenced"], true).unwrap();
        assert!(applied.applied);
        assert!(!orphan.exists());
        assert!(referenced.exists());
        let trash = list_trash(&store).unwrap();
        assert_eq!(trash.len(), 1);
        restore_trash(&store, &trash[0].id).unwrap();
        assert_eq!(fs::read_to_string(&orphan).unwrap(), "orphan bytes");
        assert!(list_trash(&store).unwrap().is_empty());
        prune(&store, &["blobs_unreferenced"], true).unwrap();
        assert_eq!(empty_trash(&store).unwrap(), 1);
        assert!(!orphan.exists());
        assert!(list_trash(&store).unwrap().is_empty());
        assert!(referenced.exists());
    }
}
