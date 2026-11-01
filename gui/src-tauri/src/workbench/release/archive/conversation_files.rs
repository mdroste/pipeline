//! Version-two archive payload for app-owned conversation working files.
//! Models write these folders, so what cannot travel is left out and counted.
//! It never fails the backup, and links are never followed.
use super::format::{BlobManifest, MAX_ARCHIVE_ENTRIES, MAX_ENTRY_BYTES};
use super::{hash_reader, safe_archive_name};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::Connection;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

fn io(error: std::io::Error) -> WorkbenchError {
    WorkbenchError::storage("Conversation file archive failed", error)
}
fn directory(path: &Path) -> WorkbenchResult<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => Ok(true),
        Ok(_) => Err(WorkbenchError::invalid(
            "Conversation archive folders must be real directories",
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(io(e)),
    }
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
/// What one export copied, and how many items it left out.
pub(super) struct Staged {
    pub(super) files: Vec<BlobManifest>,
    pub(super) omitted: usize,
}
/// Space left for these files after the rest of the archive.
pub(super) struct Budget {
    pub(super) entries: usize,
    pub(super) bytes: u64,
    pub(super) manifest_bytes: u64,
}
/// A folder that can't be listed is left out and counted, like anything else
/// that cannot travel.
fn walk(
    root: &Path,
    files: &mut Vec<PathBuf>,
    entries: &mut usize,
    omitted: &mut usize,
    depth: usize,
) {
    if depth > 32 {
        *omitted += 1;
        return;
    }
    let Ok(listing) = fs::read_dir(root) else {
        *omitted += 1;
        return;
    };
    for entry in listing {
        *entries += 1;
        if *entries > MAX_ARCHIVE_ENTRIES {
            *omitted += 1;
            return;
        }
        let Ok((path, kind)) = entry.and_then(|entry| Ok((entry.path(), entry.file_type()?)))
        else {
            *omitted += 1;
            continue;
        };
        if kind.is_dir() {
            walk(&path, files, entries, omitted, depth + 1);
        } else if kind.is_file() {
            files.push(path);
        } else {
            *omitted += 1;
        }
    }
}
/// Copies one file into staging, or removes the partial copy and returns None
/// when it can't be read or has grown past `limit` since the walk measured it.
fn copy_within(source: &Path, target: &Path, limit: u64) -> Option<u64> {
    let copied = (|| -> WorkbenchResult<u64> {
        fs::create_dir_all(target.parent().unwrap()).map_err(io)?;
        std::io::copy(
            &mut crate::safety::open_regular_file(source)
                .map_err(WorkbenchError::invalid)?
                .take(limit.saturating_add(1)),
            &mut File::create(target).map_err(io)?,
        )
        .map_err(io)
    })();
    match copied {
        Ok(count) if count <= limit => Some(count),
        _ => {
            let _ = fs::remove_file(target);
            None
        }
    }
}
/// Bytes one entry adds to the pretty-printed manifest, with its indentation.
fn manifest_cost(entry: &BlobManifest) -> u64 {
    serde_json::to_string_pretty(entry).map_or(u64::MAX, |text| text.len() as u64 + 32)
}
pub(super) fn stage(
    store: &Store,
    db: &Connection,
    staging: &Path,
    budget: Budget,
) -> WorkbenchResult<Staged> {
    let jobs = store.root_path().join("jobs");
    directory(&jobs)?;
    let root = jobs.join("conversations");
    if !directory(&root)? {
        return Ok(Staged {
            files: Vec::new(),
            omitted: 0,
        });
    }
    let mut statement = db
        .prepare("SELECT id FROM sessions ORDER BY id")
        .map_err(|e| WorkbenchError::storage("List archived conversations", e))?;
    let ids = statement
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| WorkbenchError::storage("Read archived conversations", e))?;
    let mut output = Vec::new();
    let mut entries = 0;
    let mut omitted = 0;
    let mut total = 0u64;
    let mut manifest_bytes = 0u64;
    for id in ids {
        let id = id.map_err(|e| WorkbenchError::storage("Decode archived conversation", e))?;
        if !valid_id(&id) {
            return Err(WorkbenchError::invalid(
                "Invalid archived conversation identity",
            ));
        }
        let session = root.join(&id);
        if !directory(&session)? {
            continue;
        }
        let mut files = Vec::new();
        walk(&session, &mut files, &mut entries, &mut omitted, 0);
        files.sort();
        for source in files {
            if source.strip_prefix(&root).unwrap().components().any(|c| {
                c.as_os_str()
                    .to_str()
                    .is_none_or(|name| name.contains('\\'))
            }) {
                omitted += 1;
                continue;
            }
            let relative = source
                .strip_prefix(&root)
                .unwrap()
                .to_str()
                .ok_or_else(|| WorkbenchError::invalid("Conversation filename is not UTF-8"))?
                .replace('\\', "/");
            let archive_path = format!("conversation-files/{relative}");
            let Ok(metadata) = fs::symlink_metadata(&source) else {
                omitted += 1;
                continue;
            };
            let size = metadata.len();
            if !safe_archive_name(&archive_path)
                || output.len() >= budget.entries
                || size > MAX_ENTRY_BYTES
                || total.saturating_add(size) > budget.bytes
            {
                omitted += 1;
                continue;
            }
            let target = staging.join(&archive_path);
            let limit = MAX_ENTRY_BYTES.min(budget.bytes - total);
            let Some(count) = copy_within(&source, &target, limit) else {
                omitted += 1;
                continue;
            };
            let entry = BlobManifest {
                archive_path,
                original_reference: relative,
                size_bytes: count,
                sha256: hash_reader(File::open(&target).map_err(io)?)?,
            };
            let cost = manifest_cost(&entry);
            if manifest_bytes.saturating_add(cost) > budget.manifest_bytes {
                let _ = fs::remove_file(&target);
                omitted += 1;
                continue;
            }
            manifest_bytes += cost;
            total = total.saturating_add(count);
            output.push(entry);
        }
    }
    Ok(Staged {
        files: output,
        omitted,
    })
}
pub(super) fn validate(
    files: &[BlobManifest],
    sessions: &[String],
    staging: &Path,
    expected: &mut HashSet<String>,
) -> WorkbenchResult<()> {
    for file in files {
        let Some((session, name)) = file.original_reference.split_once('/') else {
            return Err(WorkbenchError::invalid("Missing conversation file owner"));
        };
        if !valid_id(session)
            || !sessions.iter().any(|id| id == session)
            || name.is_empty()
            || !safe_archive_name(&file.original_reference)
            || file.archive_path != format!("conversation-files/{}", file.original_reference)
            || !expected.insert(file.archive_path.clone())
        {
            return Err(WorkbenchError::invalid(
                "Invalid or duplicate conversation file identity",
            ));
        }
        let path = staging.join(&file.archive_path);
        if fs::metadata(&path).map_err(io)?.len() != file.size_bytes
            || hash_reader(File::open(path).map_err(io)?)? != file.sha256
        {
            return Err(WorkbenchError::invalid(
                "Conversation file failed its size or hash check",
            ));
        }
    }
    Ok(())
}
// Imported payload is installed only in a fresh conversation store. A failed DB
// restore removes this new tree; existing local files are never overwritten.
pub(super) struct Installed {
    path: Option<PathBuf>,
}
impl Installed {
    pub(super) fn commit(&mut self) {
        self.path = None;
    }
}
impl Drop for Installed {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            let _ = fs::remove_dir_all(path);
        }
    }
}
pub(super) fn install(
    store: &Store,
    files: &[BlobManifest],
    staging: &Path,
) -> WorkbenchResult<Installed> {
    if files.is_empty() {
        return Ok(Installed { path: None });
    }
    let jobs = store.root_path().join("jobs");
    directory(&jobs)?;
    let destination = jobs.join("conversations");
    if directory(&destination)? {
        if fs::read_dir(&destination).map_err(io)?.next().is_some() {
            return Err(WorkbenchError::invalid(
                "Restore requires an empty conversation file folder",
            ));
        }
        fs::remove_dir(&destination).map_err(io)?;
    }
    let temp = tempfile::tempdir_in(&jobs).map_err(io)?;
    for file in files {
        let target = temp.path().join(&file.original_reference);
        fs::create_dir_all(target.parent().unwrap()).map_err(io)?;
        fs::copy(staging.join(&file.archive_path), target).map_err(io)?;
    }
    fs::rename(temp.path(), &destination).map_err(io)?;
    Ok(Installed {
        path: Some(destination),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_that_grew_after_the_walk_is_left_out_without_a_partial_copy() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("growing.log");
        fs::write(&source, "twelve bytes").unwrap();
        let target = temp.path().join("staged/growing.log");
        assert_eq!(copy_within(&source, &target, 4), None);
        assert!(!target.exists());
        assert_eq!(copy_within(&source, &target, 12), Some(12));
        assert_eq!(fs::read(&target).unwrap(), b"twelve bytes");
    }
}
