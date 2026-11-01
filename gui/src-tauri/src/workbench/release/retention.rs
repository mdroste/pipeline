//! Disk-use reporting and recoverable retention controls for the private
//! Workspace store. Disposable build/cache material is separated from
//! evidence-linked immutable blobs; pruning moves files into a private trash
//! with a journal, and only an explicit second action deletes them.
mod paths;
use super::now;
use crate::workbench::store::{
    is_local_reference, left_out_file, Store, WorkbenchError, WorkbenchResult, REFERENCE_COLUMNS,
};
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

pub(super) fn err(e: impl std::fmt::Display) -> WorkbenchError {
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
/// One walk's entry budget. Running out stops the count, not the report: what
/// was found so far is a lower bound.
struct Walk {
    left: usize,
    partial: bool,
}
impl Walk {
    fn new() -> Self {
        Self {
            left: MAX_WALK_ENTRIES,
            partial: false,
        }
    }
    fn step(&mut self) -> bool {
        if self.left == 0 {
            self.partial = true;
            return false;
        }
        self.left -= 1;
        true
    }
}
/// Size of a file or directory tree, without following symlinks.
fn tree_size(path: &Path, walk: &mut Walk) -> WorkbenchResult<u64> {
    let meta = fs::symlink_metadata(path).map_err(err)?;
    if meta.file_type().is_symlink() {
        return Ok(0);
    }
    if meta.is_file() {
        return Ok(meta.len());
    }
    let mut total = 0;
    for entry in fs::read_dir(path).map_err(err)? {
        if !walk.step() {
            break;
        }
        total += tree_size(&entry.map_err(err)?.path(), walk)?;
    }
    Ok(total)
}
/// Immediate children of a store directory as (path, bytes). `conversations`
/// is measured against its own walk, so a model's working folders can neither
/// use up the budget of what cleanup removes nor be stopped by it.
fn children(
    dir: &Path,
    walk: &mut Walk,
    mut conversations: Option<(&Path, &mut Walk)>,
) -> WorkbenchResult<Vec<(PathBuf, u64)>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).map_err(err)? {
        let path = entry.map_err(err)?.path();
        let own = conversations
            .as_mut()
            .filter(|(owned, _)| *owned == path.as_path());
        let size = match own {
            Some((_, conversation_walk)) => tree_size(&path, conversation_walk)?,
            None => {
                if !walk.step() {
                    break;
                }
                tree_size(&path, walk)?
            }
        };
        out.push((path, size));
    }
    out.sort();
    Ok(out)
}
/// Every stored reference, and how many could not be matched to this folder.
/// A file can only be called unused when that count is zero: a reference to the
/// old location of a moved folder says nothing about the file now sitting here.
struct References {
    paths: HashSet<PathBuf>,
    outside: usize,
}
fn references(store: &Store) -> WorkbenchResult<References> {
    let conn = store.connection()?;
    let mut refs = References {
        paths: HashSet::new(),
        outside: 0,
    };
    for (table, column) in REFERENCE_COLUMNS {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {column} FROM {table} WHERE {column} IS NOT NULL"
            ))
            .map_err(err)?;
        for row in stmt.query_map([], |r| r.get::<_, String>(0)).map_err(err)? {
            let reference = row.map_err(err)?;
            // A placeholder names no location, but the file may yet return.
            if let Some(path) = left_out_file(&reference, store.root_path()) {
                refs.paths.insert(path);
                continue;
            }
            if !is_local_reference(&reference, store.root_path()) {
                refs.outside += 1;
            }
            refs.paths.insert(PathBuf::from(reference));
        }
    }
    Ok(refs)
}
fn referenced(path: &Path, refs: &References) -> bool {
    refs.paths.contains(path) || refs.paths.iter().any(|r| r.starts_with(path))
}
/// Whether a reference could need this category's files. Temporary command
/// files and database backups are never referenced, so they stay cleanable.
fn holds_referenced_files(category: &str) -> bool {
    category.starts_with("blobs_") || category.starts_with("context_")
}
/// Why cleanup of research files is off, in the words Disk use shows and
/// refusals repeat.
fn cleanup_blocked(outside: usize) -> String {
    let locations = if outside == 1 {
        "1 saved file location points".to_string()
    } else {
        format!("{outside} saved file locations point")
    };
    format!(
        "Cleanup of research files is off. {locations} outside this research data folder, so Pipeline can't tell which files are unused. The folder may have moved or been copied without all of its files. If you still have them, restore anything in Trash, copy them here, and restart Pipeline. Temporary command files and database backups can still be removed."
    )
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
    /// The count stopped early, so `bytes` and `entries` are lower bounds.
    pub partial: bool,
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
    outside: usize,
    /// The shared walk stopped early, so cleanup can't trust this listing.
    partial: bool,
    conversations_partial: bool,
}
fn scan(store: &Store) -> WorkbenchResult<Scan> {
    let refs = references(store)?;
    let root = store.root_path();
    let mut budget = Walk::new();
    let mut conversation_walk = Walk::new();
    let mut items: BTreeMap<String, Vec<(PathBuf, u64)>> = BTreeMap::new();
    for (path, size) in children(&root.join("blobs"), &mut budget, None)? {
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
        } else if refs.outside > 0 {
            items
                .entry("blobs_unchecked".into())
                .or_default()
                .push((path, size));
        } else {
            items
                .entry("blobs_unreferenced".into())
                .or_default()
                .push((path, size));
        }
    }
    let jobs = root.join("jobs");
    let conversations = jobs.join("conversations");
    for (path, size) in children(
        &jobs,
        &mut budget,
        Some((&conversations, &mut conversation_walk)),
    )? {
        let category = if path.file_name().is_some_and(|name| name == "conversations") {
            "conversation_files"
        } else {
            "job_scratch"
        };
        items.entry(category.into()).or_default().push((path, size));
    }
    for (path, size) in children(&root.join("context"), &mut budget, None)? {
        if referenced(&path, &refs) {
            items
                .entry("context_referenced".into())
                .or_default()
                .push((path, size));
        } else if refs.outside > 0 {
            items
                .entry("context_unchecked".into())
                .or_default()
                .push((path, size));
        } else {
            items
                .entry("context_unreferenced".into())
                .or_default()
                .push((path, size));
        }
    }
    for (path, size) in children(&root.join("backups"), &mut budget, None)? {
        items
            .entry("migration_backups".into())
            .or_default()
            .push((path, size));
    }
    for (path, size) in children(&root.join("trash"), &mut budget, None)? {
        items.entry("trash".into()).or_default().push((path, size));
    }
    Ok(Scan {
        items,
        outside: refs.outside,
        partial: budget.partial,
        conversations_partial: conversation_walk.partial,
    })
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
    let mut foreign = HashSet::new();
    for (id, original, trash) in pending {
        // A row from another data folder can't be acted on here, and must not
        // take the report and every other cleanup action down with it.
        if !paths::is_local(store, Path::new(&original), Path::new(&trash)) {
            foreign.insert(id);
            continue;
        }
        paths::checked_paths(store, &id, Path::new(&original), Path::new(&trash))?;
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
    Ok(rows
        .into_iter()
        .filter(|entry: &TrashEntry| !foreign.contains(&entry.id))
        .collect())
}
pub fn storage_report(store: &Store) -> WorkbenchResult<StorageReport> {
    let scan = scan(store)?;
    let active = active_jobs(store)?;
    fn describe(key: &str) -> (&'static str, bool, &'static str) {
        match key {
            "blobs_referenced" => (
                "Research files in use",
                false,
                "Used by your papers, sources, evidence, or conversations. Always kept.",
            ),
            "blobs_unreferenced" => ("Unused research files", true, "Nothing uses these files."),
            "blobs_unchecked" => (
                "Research files not checked",
                false,
                "Kept until Pipeline can tell whether these files are in use.",
            ),
            "blobs_staging" => ("Files being saved", false, "Incomplete saves. Always kept."),
            "job_scratch" => (
                "Temporary command files",
                true,
                "Left over from finished commands. Results you kept are stored separately.",
            ),
            "conversation_files" => (
                "Conversation working files",
                false,
                "Files for conversations with no project folder. Kept with the conversation.",
            ),
            "context_referenced" => (
                "Conversation context",
                false,
                "Used by saved conversations.",
            ),
            "context_unreferenced" => (
                "Unused conversation context",
                true,
                "Nothing uses these files.",
            ),
            "context_unchecked" => (
                "Conversation context not checked",
                false,
                "Kept until Pipeline can tell whether these files are in use.",
            ),
            "migration_backups" => (
                "Database backups from updates",
                true,
                "Copies saved before an update changed the database.",
            ),
            "trash" => ("Trash", false, "Items you can restore or delete."),
            _ => ("Other", false, ""),
        }
    }
    let mut categories = Vec::new();
    for key in [
        "blobs_referenced",
        "blobs_unreferenced",
        "blobs_unchecked",
        "blobs_staging",
        "job_scratch",
        "conversation_files",
        "context_referenced",
        "context_unreferenced",
        "context_unchecked",
        "migration_backups",
        "trash",
    ] {
        let items = scan.items.get(key).cloned().unwrap_or_default();
        if key.ends_with("_unchecked") && items.is_empty() {
            continue;
        }
        let (label, disposable, note) = describe(key);
        let disposable = disposable
            && !(scan.outside > 0 && holds_referenced_files(key))
            && !(key == "job_scratch" && active > 0);
        categories.push(StorageCategory {
            key: key.into(),
            label: label.into(),
            bytes: items.iter().map(|(_, b)| b).sum(),
            entries: items.len(),
            disposable,
            note: if key == "job_scratch" && active > 0 {
                format!(
                    "{note} You can't remove these while commands or conversations are running."
                )
            } else {
                note.into()
            },
            partial: if key == "conversation_files" {
                scan.conversations_partial
            } else {
                scan.partial
            },
        });
    }
    let note = if scan.outside > 0 {
        cleanup_blocked(scan.outside)
    } else {
        "Cleanup removes only files nothing is using. Removed files go to Trash, where you can restore them until you empty it.".to_string()
    };
    Ok(StorageReport {
        root: store.root_path().to_string_lossy().into_owned(),
        categories,
        trash: list_trash(store)?,
        active_jobs: active,
        note: if scan.partial || scan.conversations_partial {
            format!("{note} Some folders hold too many items to count, so sizes marked \"at least\" may be larger.")
        } else {
            note
        },
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
    let guarded = request
        .categories
        .iter()
        .any(|category| holds_referenced_files(category));
    let scan = scan(store)?;
    if guarded && scan.outside > 0 {
        return Err(WorkbenchError::invalid(cleanup_blocked(scan.outside)));
    }
    // A preview that stopped early would not show everything cleanup could move.
    if scan.partial {
        return Err(WorkbenchError::invalid(
            "Storage inspection exceeded 200,000 entries",
        ));
    }
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
    if guarded && refs.outside > 0 {
        return Err(WorkbenchError::invalid(cleanup_blocked(refs.outside)));
    }
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
        paths::checked_paths(store, &id, &path, &destination)?;
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
    let (directory, trash_path) =
        paths::checked_paths(store, trash_id, &original, Path::new(&trash_path))?;
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
    let _ = fs::remove_dir_all(directory);
    Ok(TrashEntry {
        state: "restored".into(),
        ..entry
    })
}
pub fn empty_trash(store: &Store) -> WorkbenchResult<usize> {
    let outside = references(store)?.outside;
    let entries = list_trash(store)?;
    // Trash may hold the very files an unresolved reference needs, so those
    // stay put. Nothing can need a command file or a database backup.
    let (held, entries): (Vec<_>, Vec<_>) = entries
        .into_iter()
        .partition(|entry| outside > 0 && holds_referenced_files(&entry.category));
    if entries.is_empty() && !held.is_empty() {
        return Err(WorkbenchError::invalid(cleanup_blocked(outside)));
    }
    let mut removed = 0;
    for entry in entries {
        let recorded: String = store
            .connection()?
            .query_row(
                "SELECT trash_path FROM storage_trash WHERE id=?1",
                [&entry.id],
                |r| r.get(0),
            )
            .map_err(err)?;
        let (dir, _) = paths::checked_paths(
            store,
            &entry.id,
            Path::new(&entry.original_path),
            Path::new(&recorded),
        )?;
        if dir.exists() {
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
    fn deleting_a_conversation_removes_its_working_files() {
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
        let root = store.runtime_root(&session).unwrap();
        fs::write(root.join("work.md"), "conversation work").unwrap();
        store
            .delete_session(crate::workbench::store::DeleteSessionRequest {
                session_id: session,
                operation_id: "delete".into(),
            })
            .unwrap();
        assert!(!root.exists());
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
    fn a_folder_too_large_to_count_is_reported_as_a_lower_bound() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open_at(temp.path()).unwrap();
        let session = store.root_path().join("jobs/conversations/session");
        fs::create_dir_all(&session).unwrap();
        for index in 0..=MAX_WALK_ENTRIES {
            fs::File::create(session.join(index.to_string())).unwrap();
        }
        let scratch = store.root_path().join("jobs/finished-run");
        fs::create_dir_all(&scratch).unwrap();
        fs::write(scratch.join("log.txt"), "scratch").unwrap();
        let category = |report: &StorageReport, key: &str| {
            report
                .categories
                .iter()
                .find(|c| c.key == key)
                .cloned()
                .unwrap()
        };

        // A model's working folder neither hides disk use nor blocks cleanup.
        let report = storage_report(&store).unwrap();
        assert!(category(&report, "conversation_files").partial);
        assert!(!category(&report, "job_scratch").partial);
        assert!(report.note.contains("at least"));
        let plan = prune(&store, &["job_scratch"], true).unwrap();
        assert_eq!(plan.entries.len(), 1);
        assert!(!scratch.exists());

        // Cleanup keeps its limit: it must not act on a listing it couldn't finish.
        fs::rename(&session, store.root_path().join("jobs/huge-run")).unwrap();
        let report = storage_report(&store).unwrap();
        assert!(category(&report, "job_scratch").partial);
        assert!(!category(&report, "conversation_files").partial);
        assert!(prune(&store, &["job_scratch"], false).is_err());
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
