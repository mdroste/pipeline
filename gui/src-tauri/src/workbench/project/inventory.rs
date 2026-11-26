//! Bounded project inventories and explicit task snapshot capture.

use super::home::{home_record, ProjectHomeSettings};
use super::records::{decode, put_checked, records, ProjectRecord};
use super::{err, files, hash, id, now, MAX_CAPTURE, MAX_FILE, MAX_FILES};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub hash: Option<String>,
    pub size: u64,
    pub status: String,
    pub executable: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    pub root_identity: String,
    pub files: Vec<FileEntry>,
    pub warnings: Vec<String>,
    pub captured_at: String,
    pub complete: bool,
    /// Modified times of hashed files, which let a background check keep their
    /// hashes. Only project inventories record these.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub modified_ms: HashMap<String, u64>,
}

const LIMIT_WARNING: &str =
    "Inventory limit reached. Select a smaller folder or exclude directories and refresh.";

/// Bounded, descriptor-relative reads for the explicit task snapshot bridge.
/// The caller binds the root; this returns bytes, never a writable project handle.
pub(crate) fn materialize_research_files(
    root: &Path,
    entries: Vec<(String, Vec<u8>)>,
) -> WorkbenchResult<()> {
    let safe = files::SafeRoot::open(root)?;
    for (name, bytes) in entries {
        files::relative(&name)?;
        if let Some(old) = safe.optional_read(&name)? {
            if old != bytes {
                return Err(WorkbenchError::conflict(
                    "Research input changed during preparation",
                ));
            }
        } else {
            safe.replace(
                &name,
                None,
                false,
                Some(&bytes),
                false,
                &id("discovery-input")?,
            )?;
        }
    }
    Ok(())
}

pub(crate) fn task_capture(root: &Path, path: &Path) -> WorkbenchResult<Vec<(String, Vec<u8>)>> {
    let root = root.canonicalize().map_err(err)?;
    let selected = path.canonicalize().map_err(err)?;
    if !selected.starts_with(&root) {
        return Err(WorkbenchError::invalid(
            "Snapshot is outside the task folder",
        ));
    }
    let identity = crate::workbench::store::root_identity(&root)?;
    let safe = files::SafeRoot::open(&root)?;
    let mut paths = Vec::new();
    if selected.is_dir() {
        let inventory = inventory_at(&selected, &[])?;
        if !inventory.complete || inventory.files.iter().any(|f| f.status != "current") {
            return Err(WorkbenchError::invalid("Folder cannot be captured completely; remove symlinks and unsupported files or choose a smaller folder"));
        }
        for entry in inventory.files {
            paths.push((entry.path.clone(), selected.join(&entry.path)));
        }
    } else {
        paths.push((
            selected
                .file_name()
                .ok_or_else(|| WorkbenchError::invalid("Missing snapshot filename"))?
                .to_string_lossy()
                .into_owned(),
            selected.clone(),
        ));
    }
    let mut total = 0;
    let mut result = Vec::new();
    for (name, path) in paths {
        let relative = path
            .strip_prefix(&root)
            .map_err(err)?
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = safe
            .optional_read(&relative)?
            .ok_or_else(|| WorkbenchError::invalid("Snapshot file disappeared"))?;
        total += bytes.len();
        if total > 64 * 1024 * 1024 {
            return Err(WorkbenchError::invalid("Snapshot exceeds 64 MiB"));
        }
        // Re-read through the same root descriptor to reject edits during capture.
        if safe.optional_read(&relative)?.as_ref() != Some(&bytes) {
            return Err(WorkbenchError::conflict(
                "File changed during snapshot; capture again",
            ));
        }
        result.push((name, bytes));
    }
    if crate::workbench::store::root_identity(&root)? != identity {
        return Err(WorkbenchError::conflict(
            "Task folder changed during capture",
        ));
    }
    result.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(result)
}
/// Folders where coding agents keep their Git worktrees. Each worktree is a
/// full checkout of the project, so listing it would count every file again.
const AGENT_WORKTREES: &[(&str, &str)] = &[(".claude", "worktrees")];

pub(super) fn ignored(path: &str, custom: &[String]) -> bool {
    let is = |part: &str, name: &str| part.trim_end_matches(['.', ' ']).eq_ignore_ascii_case(name);
    path.split('/').any(|p| {
        files::reserved_component(p)
            || ["node_modules", "target", ".venv", "__pycache__", ".cache"]
                .iter()
                .any(|name| is(p, name))
    }) || path
        .split('/')
        .zip(path.split('/').skip(1))
        .any(|(parent, name)| {
            AGENT_WORKTREES
                .iter()
                .any(|(tool, folder)| is(parent, tool) && is(name, folder))
        })
        || custom.iter().any(|p| {
            path.strip_prefix(p.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        })
        || [
            ".aux",
            ".toc",
            ".out",
            ".synctex.gz",
            ".fls",
            ".fdb_latexmk",
        ]
        .iter()
        .any(|suffix| path.ends_with(suffix))
}
pub(super) fn inventory_at(root: &Path, custom: &[String]) -> WorkbenchResult<Inventory> {
    scan(root, custom, false, None).map(|(inventory, _)| inventory)
}
/// Whether this folder is a linked Git worktree: its `.git` is a file that
/// names a folder in a repository's `worktrees` directory. A submodule's `.git`
/// file names one under `modules` instead, and the submodule stays listed.
/// Only the file's text is read; the folder it names is never opened.
fn linked_worktree(folder: &Path) -> bool {
    let marker = folder.join(".git");
    if !fs::symlink_metadata(&marker).is_ok_and(|meta| meta.is_file() && meta.len() <= 4096) {
        return false;
    }
    let Ok(text) = fs::read_to_string(&marker) else {
        return false;
    };
    let Some(target) = text.trim().strip_prefix("gitdir:") else {
        return false;
    };
    let parts: Vec<&str> = target
        .trim()
        .split(['/', '\\'])
        .filter(|part| !part.is_empty())
        .collect();
    match parts.as_slice() {
        [rest @ .., folder, _] => {
            folder.eq_ignore_ascii_case("worktrees")
                && !rest
                    .last()
                    .is_some_and(|part| part.eq_ignore_ascii_case("modules"))
        }
        _ => false,
    }
}
/// Reads past this many bytes in one scan are left unhashed. Tests lower it.
fn capture_budget() -> u64 {
    #[cfg(test)]
    if let Some(budget) = CAPTURE_BUDGET.get() {
        return budget;
    }
    MAX_CAPTURE
}
#[cfg(test)]
thread_local! {
    pub(super) static CAPTURE_BUDGET: std::cell::Cell<Option<u64>> =
        const { std::cell::Cell::new(None) };
}
fn modified_ms(meta: &fs::Metadata) -> Option<u64> {
    let since = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH);
    u64::try_from(since.ok()?.as_millis()).ok()
}
#[cfg(test)]
thread_local! {
    /// Runs between listing a folder and reading its entries, where a file can
    /// vanish.
    pub(super) static AFTER_LISTING: std::cell::RefCell<Option<Box<dyn Fn()>>> =
        const { std::cell::RefCell::new(None) };
}
/// `project` marks a scan of the project folder itself. It keeps each file's
/// modified time and leaves out linked Git worktrees nested in the folder,
/// which it returns. Other scans list every folder: in a task copy, a folder
/// left out would read as its files being deleted. A file in `known` with the
/// same size and time keeps its hash without being read.
fn scan(
    root: &Path,
    custom: &[String],
    project: bool,
    known: Option<&Inventory>,
) -> WorkbenchResult<(Inventory, HashSet<String>)> {
    let known_files: HashMap<&str, &FileEntry> = known
        .into_iter()
        .flat_map(|inventory| &inventory.files)
        .map(|file| (file.path.as_str(), file))
        .collect();
    let mut result = Inventory {
        root_identity: crate::workbench::store::root_identity(root)?,
        files: Vec::new(),
        warnings: Vec::new(),
        captured_at: now(),
        complete: true,
        modified_ms: HashMap::new(),
    };
    let started = Instant::now();
    let mut pending = vec![String::new()];
    let mut worktrees = HashSet::new();
    let budget = capture_budget();
    let mut total = 0;
    let mut over_budget = 0;
    let mut visited = 0;
    let mut unreadable = 0;
    // One entry that can't be read leaves the rest of the scan good: files come
    // and go during a build, and a project can hold a folder nobody may open.
    let mut skip = |result: &mut Inventory, path: String| {
        unreadable += 1;
        result.complete = false;
        result.files.push(FileEntry {
            path,
            hash: None,
            size: 0,
            status: "unavailable".into(),
            executable: false,
        });
    };
    let safe = files::SafeRoot::open(root)?;
    let mut limited = false;
    'walk: while let Some(relative) = pending.pop() {
        if started.elapsed() > Duration::from_secs(5)
            || visited >= 20000
            || result.files.len() >= MAX_FILES
        {
            limited = true;
            break;
        }
        let listing = match fs::read_dir(root.join(&relative)) {
            Ok(listing) => listing,
            Err(error) if relative.is_empty() => return Err(err(error)),
            Err(_) => {
                skip(&mut result, relative);
                continue;
            }
        };
        let mut entries = Vec::new();
        for entry in listing {
            if visited >= 20000
                || result.files.len() >= MAX_FILES
                || started.elapsed() > Duration::from_secs(5)
            {
                limited = true;
                break;
            }
            visited += 1;
            match entry {
                Ok(entry) => entries.push(entry),
                Err(_) => skip(&mut result, relative.clone()),
            }
        }
        entries.sort_by_key(|e| e.file_name());
        #[cfg(test)]
        AFTER_LISTING.with(|hook| {
            if let Some(hook) = hook.borrow().as_ref() {
                hook();
            }
        });
        for entry in entries {
            if result.files.len() >= MAX_FILES || started.elapsed() > Duration::from_secs(5) {
                limited = true;
                break 'walk;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = if relative.is_empty() {
                name
            } else {
                format!("{relative}/{name}")
            };
            if ignored(&path, custom) {
                continue;
            }
            let Ok(meta) = fs::symlink_metadata(entry.path()) else {
                skip(&mut result, path);
                continue;
            };
            if meta.is_dir() {
                if project && linked_worktree(&entry.path()) {
                    worktrees.insert(path);
                } else if pending.len() < MAX_FILES {
                    pending.push(path);
                } else {
                    limited = true;
                }
                continue;
            }
            let executable = files::executable(&meta);
            let mut status = "current".to_string();
            let mut digest = None;
            let modified = modified_ms(&meta).filter(|_| project);
            let unchanged = known_files.get(path.as_str()).filter(|file| {
                let recorded = known.and_then(|inventory| inventory.modified_ms.get(&file.path));
                file.hash.is_some() && file.size == meta.len() && recorded == modified.as_ref()
            });
            if !meta.is_file() || meta.file_type().is_symlink() {
                status = "unsupported".into();
                result.complete = false;
            } else if meta.len() > MAX_FILE {
                status = "unhashed_large_file".into();
                result.complete = false;
            } else if total + meta.len() > budget {
                // The file may be small: earlier files used up what one scan reads.
                status = "unhashed_capture_limit".into();
                over_budget += 1;
                result.complete = false;
            } else if let Some(file) = unchanged.filter(|_| modified.is_some()) {
                total += meta.len();
                digest = file.hash.clone();
            } else {
                match safe.read(&path) {
                    Ok(bytes) => {
                        total += bytes.len() as u64;
                        digest = Some(hash(&bytes));
                    }
                    Err(_) => {
                        status = "unavailable".into();
                        result.complete = false;
                    }
                }
            }
            if let Some(modified) = modified.filter(|_| digest.is_some()) {
                result.modified_ms.insert(path.clone(), modified);
            }
            result.files.push(FileEntry {
                path,
                hash: digest,
                size: meta.len(),
                status,
                executable,
            });
        }
    }
    if limited {
        result.complete = false;
        result.warnings.push(LIMIT_WARNING.into());
    }
    if over_budget > 0 {
        result.warnings.push(format!(
            "Pipeline reads at most {} MB of files in one pass, so {over_budget} file(s) were listed but not read. Leave out large data folders in Project settings and refresh.",
            MAX_CAPTURE / (1024 * 1024)
        ));
    }
    if unreadable > 0 {
        result.warnings.push(format!(
            "{unreadable} item(s) could not be read; refresh to try again"
        ));
    }
    result.files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((result, worktrees))
}
/// Reads every file, as an explicit Refresh should.
pub(super) fn refresh(store: &Store, workspace_id: &str) -> WorkbenchResult<ProjectRecord> {
    update_inventory(store, workspace_id, false)
}
/// The check on window focus and on the timer. It trusts a file's size and
/// modified time, so returning to the app does not read the whole project.
pub(super) fn check_for_changes(
    store: &Store,
    workspace_id: &str,
) -> WorkbenchResult<ProjectRecord> {
    update_inventory(store, workspace_id, true)
}
/// Whether a path sits under one of these folders.
fn inside_any(path: &str, folders: &HashSet<String>) -> bool {
    path.rmatch_indices('/')
        .any(|(end, _)| folders.contains(&path[..end]))
}
pub(crate) struct InventoryScan {
    // Keep the project lock through scan and commit, including cancelled waits.
    _lock: Option<fs::File>,
    workspace_id: String,
    workspace_revision: i64,
    workspace_root: Option<String>,
    settings_revision: i64,
    root: std::path::PathBuf,
    root_identity: String,
    settings: ProjectHomeSettings,
    old: Option<ProjectRecord>,
    previous: Option<Inventory>,
    trust_times: bool,
}

pub(crate) struct ScannedInventory {
    scan: InventoryScan,
    inventory: Inventory,
}

#[cfg(test)]
#[path = "inventory_phase_tests.rs"]
mod phase_tests;

impl InventoryScan {
    pub(crate) fn for_request(
        store: &Store,
        request: super::ProjectMutation,
    ) -> WorkbenchResult<Self> {
        super::scope(store, &request.workspace_id)?;
        super::valid_id(&request.operation_id)?;
        let trust_times = match request.action {
            super::ProjectAction::Refresh => false,
            super::ProjectAction::CheckForChanges => true,
            _ => {
                return Err(WorkbenchError::invalid(
                    "This operation is not a folder scan",
                ))
            }
        };
        let guard = super::lock(store, &request.workspace_id)?;
        Self::prepare(store, &request.workspace_id, trust_times, Some(guard))
    }

    fn prepare(
        store: &Store,
        workspace_id: &str,
        trust_times: bool,
        guard: Option<fs::File>,
    ) -> WorkbenchResult<Self> {
        let home = home_record(store, workspace_id)?;
        let settings = decode(&home)?;
        let workspace = store.workspace(workspace_id)?;
        let root = store.registered_root(&workspace)?;
        let root_identity = crate::workbench::store::root_identity(&root)?;
        let old = records(store, workspace_id, "inventory")?
            .into_iter()
            .next();
        let previous = old.as_ref().map(decode).transpose()?;
        Ok(Self {
            _lock: guard,
            workspace_id: workspace_id.into(),
            workspace_revision: workspace.revision,
            workspace_root: workspace.root,
            settings_revision: home.revision,
            root,
            root_identity,
            settings,
            old,
            previous,
            trust_times,
        })
    }

    pub(crate) fn scan(self) -> WorkbenchResult<ScannedInventory> {
        let Self {
            root,
            settings,
            previous,
            trust_times,
            ..
        } = &self;
        let trust_times = *trust_times;
        if crate::workbench::store::root_identity(root)? != self.root_identity {
            return Err(WorkbenchError::conflict(
                "Project folder changed before the scan; refresh again",
            ));
        }
        let known = previous.as_ref().filter(|_| trust_times);
        let (mut inventory, worktrees) = scan(root, &settings.ignored_paths, true, known)?;
        // An earlier build listed nested worktrees; their files are not missing.
        let left_out =
            |path: &str| ignored(path, &settings.ignored_paths) || inside_any(path, &worktrees);
        if let Some(previous) = &previous {
            let previous_hashes: HashMap<&str, &Option<String>> = previous
                .files
                .iter()
                .map(|f| (f.path.as_str(), &f.hash))
                .collect();
            for file in &mut inventory.files {
                if file.hash.is_some() {
                    file.status = match previous_hashes.get(file.path.as_str()) {
                        None => "added",
                        Some(hash) if **hash != file.hash => "changed",
                        _ => "current",
                    }
                    .into();
                }
            }
            if inventory.warnings.iter().any(|w| w == LIMIT_WARNING) {
                let mut scanned: std::collections::BTreeMap<_, _> = inventory
                    .files
                    .drain(..)
                    .map(|f| (f.path.clone(), f))
                    .collect();
                for old in previous
                    .files
                    .iter()
                    .filter(|f| f.status != "missing" && !left_out(&f.path))
                    .take(MAX_FILES)
                {
                    if let Some(current) = scanned.remove(&old.path) {
                        inventory.files.push(current);
                    } else {
                        inventory.files.push(old.clone());
                        if let Some(time) = previous.modified_ms.get(&old.path) {
                            inventory.modified_ms.insert(old.path.clone(), *time);
                        }
                    }
                }
                inventory.files.extend(
                    scanned
                        .into_values()
                        .take(MAX_FILES - inventory.files.len()),
                );
                inventory.files.sort_by(|a, b| a.path.cmp(&b.path));
                let kept: HashSet<_> = inventory.files.iter().map(|f| f.path.as_str()).collect();
                inventory
                    .modified_ms
                    .retain(|path, _| kept.contains(path.as_str()));
                return Ok(ScannedInventory {
                    scan: self,
                    inventory,
                });
            }
            let present: HashSet<String> = inventory.files.iter().map(|f| f.path.clone()).collect();
            let unread: HashSet<String> = inventory
                .files
                .iter()
                .filter(|f| f.status == "unavailable")
                .map(|f| f.path.clone())
                .collect();
            for file in previous.files.iter() {
                // A file that was already missing was reported once; keeping it for
                // good would grow this record to its size limit.
                if file.status == "missing" || present.contains(&file.path) || left_out(&file.path)
                {
                    continue;
                }
                // Nothing is known about a file in a folder that couldn't be read,
                // so it keeps what it had until the folder can be read again.
                if inside_any(&file.path, &unread) {
                    inventory.files.push(file.clone());
                    if let Some(modified) = previous.modified_ms.get(&file.path) {
                        inventory.modified_ms.insert(file.path.clone(), *modified);
                    }
                    continue;
                }
                // A folder listed as unread is not a file that went away.
                let folder = file.status == "unavailable"
                    && fs::symlink_metadata(root.join(&file.path)).is_ok_and(|meta| meta.is_dir());
                if !folder {
                    inventory.files.push(FileEntry {
                        status: "missing".into(),
                        hash: None,
                        ..file.clone()
                    });
                }
            }
        }
        Ok(ScannedInventory {
            scan: self,
            inventory,
        })
    }
}

impl ScannedInventory {
    pub(crate) fn commit(self, store: &Store) -> WorkbenchResult<ProjectRecord> {
        let scan = &self.scan;
        // A different command/process may move the workspace root without this
        // feature's project lock. Revalidate after I/O, before publishing hashes.
        if crate::workbench::store::root_identity(&scan.root)? != self.inventory.root_identity
            || self.inventory.root_identity != scan.root_identity
        {
            return Err(WorkbenchError::conflict(
                "Project folder or preferences changed during the scan; refresh again",
            ));
        }
        put_checked(
            store,
            &scan.workspace_id,
            &format!("inventory_{}", scan.workspace_id),
            "inventory",
            scan.old.as_ref().map_or(0, |record| record.revision),
            &self.inventory,
            |connection| {
                let workspace = Store::workspace_on(connection, &scan.workspace_id)?;
                if workspace.archived_at.is_some()
                    || workspace.revision != scan.workspace_revision
                    || workspace.root != scan.workspace_root
                    || workspace.root_identity.as_deref() != Some(scan.root_identity.as_str())
                    || super::home::home_record_on(connection, &scan.workspace_id)?.revision
                        != scan.settings_revision
                {
                    return Err(WorkbenchError::conflict(
                        "Project folder or preferences changed during the scan; refresh again",
                    ));
                }
                Ok(())
            },
        )
    }
}

fn update_inventory(
    store: &Store,
    workspace_id: &str,
    trust_times: bool,
) -> WorkbenchResult<ProjectRecord> {
    InventoryScan::prepare(store, workspace_id, trust_times, None)?
        .scan()?
        .commit(store)
}

#[cfg(test)]
#[test]
fn inventory_limits_a_single_wide_directory() {
    use crate::workbench::store::CreateWorkspaceRequest;
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("project");
    fs::create_dir(&dir).unwrap();
    let store = Store::open_at(&temp.path().join("store")).unwrap();
    let ws = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Scan".into(),
            root: Some(dir.to_string_lossy().into()),
            operation_id: "scan".into(),
        })
        .unwrap()
        .record
        .id;
    fs::write(dir.join("zz_original.txt"), b"keep").unwrap();
    refresh(&store, &ws).unwrap();
    for i in 0..MAX_FILES {
        fs::write(dir.join(i.to_string()), []).unwrap();
    }
    let scan: Inventory = decode(&refresh(&store, &ws).unwrap()).unwrap();
    assert!(scan.files.len() <= MAX_FILES);
    assert!(!scan.complete);
    assert!(scan.warnings.iter().any(|w| w == LIMIT_WARNING));
    assert!(scan
        .files
        .iter()
        .any(|f| f.path == "zz_original.txt" && f.hash.is_some() && f.status != "missing"));
}
