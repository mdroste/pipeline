//! Complete snapshots of private edit copies. An unreadable or changing copy
//! stops export; none of its uncaptured work is silently left out.
use super::format::{MAX_ARCHIVE_BYTES, MAX_ARCHIVE_ENTRIES, MAX_ENTRY_BYTES};
use super::{hash_reader, PortablePaths};
use crate::workbench::store::{root_identity, Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CopyFile {
    path: String,
    pub(super) size_bytes: u64,
    sha256: String,
    executable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CopyManifest {
    workspace_id: String,
    checkpoint_id: String,
    directories: Vec<String>,
    pub(super) files: Vec<CopyFile>,
}

impl CopyManifest {
    fn relative_root(&self) -> PathBuf {
        Path::new("project-copies")
            .join(&self.workspace_id)
            .join(&self.checkpoint_id)
    }
    pub(super) fn archive_path(&self, file: &CopyFile) -> String {
        format!(
            "project-copies/{}/{}/{}",
            self.workspace_id, self.checkpoint_id, file.path
        )
    }
}

fn err(error: impl std::fmt::Display) -> WorkbenchError {
    WorkbenchError::storage("Could not back up or restore a private project copy", error)
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
fn directory(path: &Path) -> WorkbenchResult<()> {
    let meta = fs::symlink_metadata(path).map_err(err)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(WorkbenchError::invalid(
            "Private project copy folders must be real directories",
        ));
    }
    Ok(())
}
fn executable(meta: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        false
    }
}

#[cfg(test)]
thread_local! {
    static AFTER_STAGE: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}

/// A complete bounded walk, including empty directories and generated files.
fn listing(root: &Path, git_worktree: bool) -> WorkbenchResult<(Vec<String>, Vec<String>)> {
    let mut directories = Vec::new();
    let mut files = Vec::new();
    let mut pending = vec![String::new()];
    let mut names = PortablePaths::default();
    while let Some(relative) = pending.pop() {
        let folder = root.join(&relative);
        directory(&folder)?;
        for entry in fs::read_dir(folder).map_err(err)? {
            let entry = entry.map_err(err)?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| err("Non-UTF-8 copy filename"))?;
            let path = if relative.is_empty() {
                name
            } else {
                format!("{relative}/{name}")
            };
            // Git's host-specific link is not working content. Restored worktrees
            // become ordinary edit copies, without authority over the old repo.
            if git_worktree && path == ".git" {
                continue;
            }
            if files.len() + directories.len() >= MAX_ARCHIVE_ENTRIES
                || path.split('/').count() > 32
            {
                return Err(err(
                    "Private project copy exceeds the archive file/depth limit",
                ));
            }
            let meta = fs::symlink_metadata(entry.path()).map_err(err)?;
            if meta.file_type().is_symlink() || (!meta.is_file() && !meta.is_dir()) {
                return Err(err(format!("Copy contains a link or unsupported file: {path}. Remove it before backing up.")));
            }
            if meta.is_dir() {
                names.directory(&path)?;
                directories.push(path.clone());
                pending.push(path);
            } else {
                names.insert(&path)?;
                files.push(path);
            }
        }
    }
    directories.sort();
    files.sort();
    Ok((directories, files))
}

pub(super) fn stage(
    store: &Store,
    db: &Connection,
    staging: &Path,
) -> WorkbenchResult<Vec<CopyManifest>> {
    let mut query = db.prepare("SELECT workspace_id,id,body_json FROM project_records WHERE kind='checkpoint' ORDER BY workspace_id,id").map_err(err)?;
    let rows = query
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(err)?;
    let mut copies = Vec::new();
    let mut total = 0u64;
    let mut entries = 0usize;
    for row in rows {
        let (workspace_id, checkpoint_id, body) = row.map_err(err)?;
        let body: Value = serde_json::from_str(&body).map_err(err)?;
        if !matches!(body["backend"].as_str(), Some("copy" | "git")) {
            continue;
        }
        let git_worktree = body["backend"] == "git";
        if !valid_id(&workspace_id) || !valid_id(&checkpoint_id) {
            return Err(err("Invalid copy identity"));
        }
        let mut copy = CopyManifest {
            workspace_id,
            checkpoint_id,
            directories: vec![],
            files: vec![],
        };
        let relative = copy.relative_root();
        let root = store.root_path().join(&relative);
        // A settled edit no longer needs its copy, which may have been
        // reclaimed: every plain editor save applies and removes its own, and
        // Undo then records `undone`. Its record keeps the path it had before
        // a restore or a data-folder move, so decide this before comparing.
        if matches!(
            body["state"].as_str(),
            Some("accepted" | "rejected" | "undone")
        ) && !root.try_exists().map_err(err)?
        {
            continue;
        }
        let recorded = Path::new(body["taskRoot"].as_str().unwrap_or(""));
        if recorded != root {
            if recorded.ends_with(&relative) || root.try_exists().map_err(err)? {
                return Err(err("Private project copy has moved. Reopen the research data folder before backing up."));
            }
            continue; // Legacy external project copies are not private-store files.
        }
        directory(&store.root_path().join("project-copies"))?;
        directory(root.parent().unwrap())?;
        directory(&root)?;
        let identity = root_identity(&root)?;
        if body["taskRootIdentity"].as_str() != Some(identity.as_str()) {
            return Err(err("Private project copy was replaced"));
        }
        let (directories, files) = listing(&root, git_worktree)?;
        entries += directories.len() + files.len() + 1;
        if entries > MAX_ARCHIVE_ENTRIES {
            return Err(err("Private copies exceed the archive entry limit"));
        }
        for path in &files {
            let source = crate::safety::open_regular_file(&root.join(path)).map_err(err)?;
            let meta = source.metadata().map_err(err)?;
            let name = format!(
                "project-copies/{}/{}/{path}",
                copy.workspace_id, copy.checkpoint_id
            );
            let target = staging.join(name);
            fs::create_dir_all(target.parent().unwrap()).map_err(err)?;
            let size_bytes = std::io::copy(
                &mut source.take(MAX_ENTRY_BYTES + 1),
                &mut File::create(&target).map_err(err)?,
            )
            .map_err(err)?;
            total = total.saturating_add(size_bytes);
            if size_bytes > MAX_ENTRY_BYTES || total > MAX_ARCHIVE_BYTES {
                return Err(err("Private copies exceed the archive size limit"));
            }
            copy.files.push(CopyFile {
                path: path.clone(),
                size_bytes,
                sha256: hash_reader(File::open(target).map_err(err)?)?,
                executable: executable(&meta),
            });
        }
        // The UI holds the storage-maintenance gate. Also reject external edits
        // during capture, rather than publish a mixed working-copy snapshot.
        #[cfg(test)]
        AFTER_STAGE.with(|hook| {
            if let Some(hook) = hook.borrow_mut().take() {
                hook();
            }
        });
        if listing(&root, git_worktree)? != (directories.clone(), files)
            || root_identity(&root)? != identity
        {
            return Err(err(
                "Private project copy changed during backup. Retry after edits finish.",
            ));
        }
        for file in &copy.files {
            let source = crate::safety::open_regular_file(&root.join(&file.path)).map_err(err)?;
            let meta = source.metadata().map_err(err)?;
            if meta.len() != file.size_bytes
                || executable(&meta) != file.executable
                || hash_reader(source.take(MAX_ENTRY_BYTES + 1))? != file.sha256
            {
                return Err(err(
                    "Private project copy changed during backup. Retry after edits finish.",
                ));
            }
        }
        copy.directories = directories;
        copies.push(copy);
    }
    Ok(copies)
}

pub(super) fn validate(
    copies: &[CopyManifest],
    db: &Connection,
    staging: &Path,
    expected: &mut HashSet<String>,
) -> WorkbenchResult<()> {
    let mut owners = HashSet::new();
    let mut names = PortablePaths::default();
    let mut entries = 0usize;
    for copy in copies {
        entries += 1 + copy.directories.len() + copy.files.len();
        if entries > MAX_ARCHIVE_ENTRIES
            || !valid_id(&copy.workspace_id)
            || !valid_id(&copy.checkpoint_id)
            || !owners.insert((&copy.workspace_id, &copy.checkpoint_id))
        {
            return Err(err("Invalid or duplicate private copy identity"));
        }
        let exists: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM project_records WHERE workspace_id=?1 AND id=?2 AND kind='checkpoint' AND json_extract(body_json,'$.backend') IN ('copy','git'))", params![copy.workspace_id, copy.checkpoint_id], |r| r.get(0)).map_err(err)?;
        if !exists {
            return Err(err("Private copy has no matching checkpoint"));
        }
        names.directory(&format!(
            "project-copies/{}/{}",
            copy.workspace_id, copy.checkpoint_id
        ))?;
        for directory in &copy.directories {
            if !super::safe_archive_name(directory) {
                return Err(err("Invalid copy directory"));
            }
            names.directory(&format!(
                "project-copies/{}/{}/{directory}",
                copy.workspace_id, copy.checkpoint_id
            ))?;
        }
        for file in &copy.files {
            if !super::safe_archive_name(&file.path) {
                return Err(err("Invalid copy file path"));
            }
            let name = copy.archive_path(file);
            names.insert(&name)?;
            if !expected.insert(name.clone()) {
                return Err(err("Duplicate private copy file"));
            }
            let path = staging.join(name);
            if fs::metadata(&path).map_err(err)?.len() != file.size_bytes
                || hash_reader(File::open(path).map_err(err)?)? != file.sha256
            {
                return Err(err("Private copy file failed its size or hash check"));
            }
        }
    }
    Ok(())
}

pub(super) struct Installed(Option<PathBuf>);
impl Installed {
    pub(super) fn commit(&mut self) {
        self.0 = None;
    }
}
impl Drop for Installed {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = fs::remove_dir_all(path);
        }
    }
}

pub(super) fn install(
    store: &Store,
    copies: &[CopyManifest],
    staging: &Path,
    db: &Connection,
) -> WorkbenchResult<Installed> {
    if copies.is_empty() {
        return Ok(Installed(None));
    }
    let destination = store.root_path().join("project-copies");
    if destination.try_exists().map_err(err)? {
        directory(&destination)?;
        if fs::read_dir(&destination).map_err(err)?.next().is_some() {
            return Err(err("Restore requires an empty private project copy folder"));
        }
        fs::remove_dir(&destination).map_err(err)?;
    }
    let temp = tempfile::tempdir_in(store.root_path()).map_err(err)?;
    for copy in copies {
        let root = temp
            .path()
            .join(&copy.workspace_id)
            .join(&copy.checkpoint_id);
        fs::create_dir_all(&root).map_err(err)?;
        for directory in &copy.directories {
            fs::create_dir_all(root.join(directory)).map_err(err)?;
        }
        for file in &copy.files {
            let target = root.join(&file.path);
            fs::create_dir_all(target.parent().unwrap()).map_err(err)?;
            fs::copy(staging.join(copy.archive_path(file)), &target).map_err(err)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(
                    &target,
                    fs::Permissions::from_mode(if file.executable { 0o700 } else { 0o600 }),
                )
                .map_err(err)?;
            }
        }
    }
    fs::rename(temp.path(), &destination).map_err(err)?;
    let installed = Installed(Some(destination));
    for copy in copies {
        let root = store.root_path().join(copy.relative_root());
        let identity = root_identity(&root)?;
        // The user explicitly mapped the external project root during restore.
        // Keep checkpoint authority aligned with that choice, not the old host.
        // A location change is a new record revision, too. Reusing the old
        // revision would collide with the history trigger on the next edit.
        db.execute("UPDATE project_records SET revision=revision+1,body_json=json_set(body_json,'$.backend','copy','$.git',NULL,'$.taskRoot',?3,'$.taskRootIdentity',?4,'$.rootIdentity',COALESCE((SELECT root_identity FROM workspaces WHERE id=?1),json_extract(body_json,'$.rootIdentity'))) WHERE workspace_id=?1 AND id=?2 AND kind='checkpoint'", params![copy.workspace_id, copy.checkpoint_id, root.to_string_lossy(), identity]).map_err(err)?;
    }
    Ok(installed)
}

#[cfg(test)]
mod tests;
