//! Bounded project inventories and explicit task snapshot capture.

use super::home::{home_record, ProjectHomeSettings};
use super::records::{decode, put, records, ProjectRecord};
use super::{err, files, hash, id, now, root, MAX_CAPTURE, MAX_FILE, MAX_FILES};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use serde::{Deserialize, Serialize};
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
    }) || custom
        .iter()
        .any(|p| path == p || path.starts_with(&format!("{p}/")))
        || ["aux", "toc", "out", "synctex.gz", "fls", "fdb_latexmk"]
            .iter()
            .any(|e| path.ends_with(&format!(".{e}")))
}
pub(super) fn inventory_at(root: &Path, custom: &[String]) -> WorkbenchResult<Inventory> {
    let mut result = Inventory {
        root_identity: crate::workbench::store::root_identity(root)?,
        files: Vec::new(),
        warnings: Vec::new(),
        captured_at: now(),
        complete: true,
    };
    let started = Instant::now();
    let mut pending = vec![String::new()];
    let mut total = 0;
    let mut visited = 0;
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
        let mut entries = fs::read_dir(root.join(&relative))
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        entries.sort_by_key(|e| e.file_name());
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
            let meta = fs::symlink_metadata(entry.path()).map_err(err)?;
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
            if !meta.is_file() || meta.file_type().is_symlink() {
                status = "unsupported".into();
                result.complete = false;
            } else if meta.len() > MAX_FILE || total + meta.len() > MAX_CAPTURE {
                status = "unhashed_large_file".into();
                result.complete = false;
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
            result.files.push(FileEntry {
                path,
                hash: digest,
                size: meta.len(),
                status,
                executable,
            });
        }
    }
    result.files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}
pub(super) fn refresh(store: &Store, workspace_id: &str) -> WorkbenchResult<ProjectRecord> {
    let settings: ProjectHomeSettings = decode(&home_record(store, workspace_id)?)?;
    let root = root(store, workspace_id)?;
    let mut inventory = inventory_at(&root, &settings.ignored_paths)?;
    let old = records(store, workspace_id, "inventory")?
        .into_iter()
        .next();
    if let Some(old) = &old {
        let previous: Inventory = decode(old)?;
        for file in &mut inventory.files {
            if file.hash.is_some() {
                file.status = match previous.files.iter().find(|f| f.path == file.path) {
                    None => "added",
                    Some(f) if f.hash != file.hash => "changed",
                    _ => "current",
                }
                .into();
            }
        }
        for file in previous.files {
            if !inventory.files.iter().any(|f| f.path == file.path)
                && !ignored(&file.path, &settings.ignored_paths)
            {
                inventory.files.push(FileEntry {
                    status: "missing".into(),
                    hash: None,
                    ..file
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
