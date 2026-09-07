//! Disk-use reporting and recoverable retention controls for the private
//! Workspace store. Disposable build/cache material is separated from
//! evidence-linked immutable blobs; pruning moves files into a private trash
//! with a journal, and only an explicit second action deletes them.
use super::now;
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, TransactionBehavior};
use serde::{Deserialize, Serialize};
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
        items
            .entry("job_scratch".into())
            .or_default()
            .push((path, size));
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
            "SELECT COUNT(*) FROM research_executions WHERE outcome IN ('queued','running')",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n.max(0) as usize)
        .map_err(err)
}
pub fn list_trash(store: &Store) -> WorkbenchResult<Vec<TrashEntry>> {
    let conn = store.connection()?;
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
            "job_scratch" => ("Execution scratch", true, "Working directories for conversations without a registered folder and finished job material. Retained receipts and adopted outputs live in blobs, not here."),
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
                    "{note} Pruning is unavailable while {active} job(s) are queued or running."
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
}
pub fn prune_storage(store: &Store, request: PruneRequest) -> WorkbenchResult<PrunePlan> {
    for c in &request.categories {
        if !DISPOSABLE.contains(&c.as_str()) {
            return Err(WorkbenchError::invalid(format!(
                "{c} is not a disposable storage category"
            )));
        }
    }
    if request.categories.contains(&"job_scratch".to_string()) && active_jobs(store)? > 0 {
        return Err(WorkbenchError::invalid(
            "Stop or finish local jobs before pruning execution scratch",
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
    if !request.apply {
        return Ok(PrunePlan {
            entries,
            bytes,
            applied: false,
            trash_ids: Vec::new(),
        });
    }
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
        fs::rename(&path, &destination).map_err(err)?;
        tx.commit().map_err(err)?;
        ids.push(id);
    }
    Ok(PrunePlan {
        entries,
        bytes,
        applied: true,
        trash_ids: ids,
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
        assert!(prune_storage(
            &store,
            PruneRequest {
                categories: vec!["blobs_referenced".into()],
                apply: true
            }
        )
        .is_err());
        let preview = prune_storage(
            &store,
            PruneRequest {
                categories: vec!["blobs_unreferenced".into(), "context_unreferenced".into()],
                apply: false,
            },
        )
        .unwrap();
        assert_eq!(preview.entries.len(), 2);
        assert!(!preview.applied);
        assert!(orphan.exists());
        let applied = prune_storage(
            &store,
            PruneRequest {
                categories: vec!["blobs_unreferenced".into()],
                apply: true,
            },
        )
        .unwrap();
        assert!(applied.applied);
        assert!(!orphan.exists());
        assert!(referenced.exists());
        let trash = list_trash(&store).unwrap();
        assert_eq!(trash.len(), 1);
        restore_trash(&store, &trash[0].id).unwrap();
        assert_eq!(fs::read_to_string(&orphan).unwrap(), "orphan bytes");
        assert!(list_trash(&store).unwrap().is_empty());
        prune_storage(
            &store,
            PruneRequest {
                categories: vec!["blobs_unreferenced".into()],
                apply: true,
            },
        )
        .unwrap();
        assert_eq!(empty_trash(&store).unwrap(), 1);
        assert!(!orphan.exists());
        assert!(list_trash(&store).unwrap().is_empty());
        assert!(referenced.exists());
    }
}
