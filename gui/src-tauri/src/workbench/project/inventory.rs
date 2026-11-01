//! Bounded project inventories and explicit task snapshot capture.

use super::home::{home_record, ProjectHomeSettings};
use super::records::{decode, put, records, ProjectRecord};
use super::{err, files, hash, id, now, root, MAX_CAPTURE, MAX_FILE, MAX_FILES};
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
pub(super) fn ignored(path: &str, custom: &[String]) -> bool {
    path.split('/').any(|p| {
        files::reserved_component(p)
            || ["node_modules", "target", ".venv", "__pycache__", ".cache"]
                .iter()
                .any(|name| p.trim_end_matches(['.', ' ']).eq_ignore_ascii_case(name))
    }) || custom.iter().any(|p| {
        path.strip_prefix(p.as_str())
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    }) || [
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
    scan(root, custom, false, None)
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
/// `record_times` keeps each file's modified time. A file in `known` with the
/// same size and time keeps its hash without being read.
fn scan(
    root: &Path,
    custom: &[String],
    record_times: bool,
    known: Option<&Inventory>,
) -> WorkbenchResult<Inventory> {
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
    let mut total = 0;
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
    while let Some(relative) = pending.pop() {
        if started.elapsed() > Duration::from_secs(5)
            || visited > 20000
            || result.files.len() >= MAX_FILES
        {
            result.complete = false;
            result.warnings.push("Inventory limit reached; select a smaller folder or exclude directories and refresh".into());
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
            visited += 1;
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
                if pending.len() < MAX_FILES {
                    pending.push(path);
                } else {
                    result.complete = false;
                }
                continue;
            }
            let executable = files::executable(&meta);
            let mut status = "current".to_string();
            let mut digest = None;
            let modified = modified_ms(&meta).filter(|_| record_times);
            let unchanged = known_files.get(path.as_str()).filter(|file| {
                let recorded = known.and_then(|inventory| inventory.modified_ms.get(&file.path));
                file.hash.is_some() && file.size == meta.len() && recorded == modified.as_ref()
            });
            if !meta.is_file() || meta.file_type().is_symlink() {
                status = "unsupported".into();
                result.complete = false;
            } else if meta.len() > MAX_FILE || total + meta.len() > MAX_CAPTURE {
                status = "unhashed_large_file".into();
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
    if unreadable > 0 {
        result.warnings.push(format!(
            "{unreadable} item(s) could not be read; refresh to try again"
        ));
    }
    result.files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
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
fn update_inventory(
    store: &Store,
    workspace_id: &str,
    trust_times: bool,
) -> WorkbenchResult<ProjectRecord> {
    let settings: ProjectHomeSettings = decode(&home_record(store, workspace_id)?)?;
    let root = root(store, workspace_id)?;
    let old = records(store, workspace_id, "inventory")?
        .into_iter()
        .next();
    let previous: Option<Inventory> = old.as_ref().map(decode).transpose()?;
    let known = previous.as_ref().filter(|_| trust_times);
    let mut inventory = scan(&root, &settings.ignored_paths, true, known)?;
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
            if file.status == "missing"
                || present.contains(&file.path)
                || ignored(&file.path, &settings.ignored_paths)
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
    put(
        store,
        workspace_id,
        &format!("inventory_{workspace_id}"),
        "inventory",
        old.map_or(0, |r| r.revision),
        &inventory,
    )
}
