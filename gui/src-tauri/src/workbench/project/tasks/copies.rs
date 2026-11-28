//! Create, inspect, and retire an isolated project copy.

use super::{application::ensure_no_application, Checkpoint, ResearchTask};
use crate::workbench::project::{
    blob, blob_read, decode, err, files,
    git_process::{capture_git, git_path, GitLimits},
    hash, home_record, id, ignored, inventory_at, lock, now, put, record, root, version_control,
    FileEntry, ProjectHomeSettings, ProjectRecord, MAX_CAPTURE, MAX_FILES, SCRATCH,
};
use crate::workbench::{
    research,
    store::{self, Store, WorkbenchError, WorkbenchResult},
};
use rusqlite::params;
use serde_json::json;
use std::io::Write;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

/// Remove the app-owned copy directory of a settled checkpoint. Only the
/// copy backend is reclaimed (a Git worktree is a separate host operation);
/// the path is re-validated against the recorded identity before deletion.
pub(in crate::workbench::project) fn discard_copy(
    store: &Store,
    ws: &str,
    checkpoint_id: &str,
) -> WorkbenchResult<()> {
    let r = record(store, ws, checkpoint_id, "checkpoint")?;
    let c: Checkpoint = decode(&r)?;
    if c.backend != "copy" {
        return Ok(());
    }
    let task_root = task_root_path(store, ws, checkpoint_id, &c)?;
    fs::remove_dir_all(&task_root).map_err(err)?;
    Ok(())
}
/// Removes the copy of a settled conversation edit that its conversation has
/// replaced. Anything that still needs the folder keeps it: another conversation
/// bound to this edit, a job inside it, an edit left partly accepted, or a file
/// the researcher never saw. Undo works from the retained blobs.
pub(in crate::workbench::project) fn reclaim_replaced_copy(
    store: &Store,
    ws: &str,
    checkpoint_id: &str,
) -> WorkbenchResult<()> {
    let c: Checkpoint = decode(&record(store, ws, checkpoint_id, "checkpoint")?)?;
    if c.backend != "copy" || !["accepted", "rejected"].contains(&c.state.as_str()) {
        return Ok(());
    }
    let task_root = task_root_path(store, ws, checkpoint_id, &c)?;
    research::jobs::ensure_copy_free(ws, &task_root)?;
    let bound: bool = store
        .connection()?
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE workspace_id=?1 AND json_extract(overrides_json,'$.projectCheckpointId')=?2)",
            params![ws, checkpoint_id],
            |r| r.get(0),
        )
        .map_err(err)?;
    let unfinished: bool = store
        .connection()?
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM project_records WHERE workspace_id=?1 AND kind='application' AND json_extract(body_json,'$.checkpointId')=?2 AND json_extract(body_json,'$.state') IN ('applying','recovery_required'))",
            params![ws, checkpoint_id],
            |r| r.get(0),
        )
        .map_err(err)?;
    if bound || unfinished {
        return Ok(());
    }
    if holds_only_reviewed_files(&c, &task_root)? {
        fs::remove_dir_all(&task_root).map_err(err)?;
    }
    Ok(())
}
/// Whether a settled copy holds nothing beyond what its edit showed. Capture
/// skips build and cache folders, so this walks everything: a file in one of
/// those counts only if the edit's base already held the same bytes. Anything
/// it can't vouch for keeps the folder, since only disk space is at stake.
fn holds_only_reviewed_files(c: &Checkpoint, task_root: &Path) -> WorkbenchResult<bool> {
    let reviewed = if c.proposed.is_empty() {
        &c.base
    } else {
        &c.proposed
    };
    let reader = files::SafeRoot::for_records(task_root)?;
    let mut found = BTreeSet::new();
    let mut visited = 0;
    let mut pending = vec![(task_root.to_path_buf(), String::new())];
    while let Some((folder, relative)) = pending.pop() {
        for entry in fs::read_dir(&folder).map_err(err)? {
            let entry = entry.map_err(err)?;
            visited += 1;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                return Ok(false);
            };
            if visited > 20000 {
                return Ok(false);
            }
            let path = if relative.is_empty() {
                name
            } else {
                format!("{relative}/{name}")
            };
            let kind = entry.file_type().map_err(err)?;
            if kind.is_dir() {
                pending.push((entry.path(), path));
                continue;
            }
            let known = reviewed
                .get(&path)
                .or_else(|| c.base.get(&path).filter(|_| ignored(&path, &[])));
            let (true, Some(known)) = (kind.is_file(), known) else {
                return Ok(false);
            };
            match reader.optional_read(&path) {
                Ok(Some(bytes)) if Some(hash(&bytes)) == known.hash => found.insert(path),
                _ => return Ok(false),
            };
        }
    }
    Ok(reviewed
        .keys()
        .all(|path| ignored(path, &[]) || found.contains(path)))
}
fn direct_git(root: &Path) -> WorkbenchResult<version_control::Git> {
    version_control::Git::open(root).ok_or_else(|| WorkbenchError::invalid("Git is not installed"))
}
fn guard(root: &Path) -> WorkbenchResult<std::sync::Arc<version_control::ReadGuard>> {
    direct_git(root)?.guard()
}
/// A read with the guard of `Git::read`: nothing the folder's own Git config names
/// can start. Git runs directly, because a shell's startup output would land in the
/// output these calls parse.
pub(in crate::workbench::project) fn git(root: &Path, args: &[&str]) -> WorkbenchResult<Vec<u8>> {
    direct_git(root)?.read(args)
}
/// A fetch through the login shell, under the same guard. It alone stays there:
/// its output is never parsed, and it may need an SSH agent or credential helper
/// that only the researcher's shell startup sets up. Transports are allowed, and
/// the folder's own config still applies to nothing.
pub(in crate::workbench::project) fn git_fetch(
    root: &Path,
    options: &[&str],
    remote: &str,
) -> WorkbenchResult<Vec<u8>> {
    let guard = guard(root)?;
    use std::process::Command;
    // Paths are argv, never interpolated into shell code.
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = Command::new("/bin/zsh");
        c.args(["-lic", "git \"$@\"", "pipeline-git"]);
        c
    };
    #[cfg(not(target_os = "macos"))]
    let mut command = Command::new("git");
    command.args(["-c", "credential.interactive=false"]);
    guard.fetch(&mut command, options, remote)?;
    capture_git(command, root, GitLimits::default())
}

pub(super) fn task_root_path(
    store: &Store,
    ws: &str,
    checkpoint_id: &str,
    checkpoint: &Checkpoint,
) -> WorkbenchResult<PathBuf> {
    let current = root(store, ws)?;
    if store::root_identity(&current)? != checkpoint.root_identity {
        return Err(WorkbenchError::conflict(
            "The project root changed; restore/remap task files explicitly before continuing",
        ));
    }
    let private = super::super::connections::private_copy_parent(store, ws).join(checkpoint_id);
    // Older copies remain readable at their original location; new copies are private.
    let expected = if Path::new(&checkpoint.task_root) == private {
        private
    } else {
        current.join(SCRATCH).join(checkpoint_id)
    };
    let unavailable = || {
        WorkbenchError::conflict(
            "Task copy is missing, relocated, or replaced; its saved snapshots remain available",
        )
    };
    let metadata = match fs::symlink_metadata(&expected) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(unavailable()),
        other => other.map_err(err)?,
    };
    if expected != Path::new(&checkpoint.task_root)
        || metadata.file_type().is_symlink()
        || store::root_identity(&expected)? != checkpoint.task_root_identity
    {
        return Err(unavailable());
    }
    Ok(expected)
}
/// A task copy being set up. No record refers to it until the checkpoint is saved,
/// so a capture that stops early takes its folder, worktree, and branch with it.
struct UnsavedCopy<'a> {
    source: &'a Path,
    task_root: &'a Path,
    /// The new branch, for a Git worktree.
    branch: Option<String>,
    saved: bool,
}
impl Drop for UnsavedCopy<'_> {
    fn drop(&mut self) {
        if self.saved {
            return;
        }
        if let Some(branch) = &self.branch {
            let path = git_path(self.task_root).to_string_lossy().into_owned();
            let _ = git(self.source, &["worktree", "remove", "--force", &path]);
            let _ = fs::remove_dir_all(self.task_root);
            let _ = git(self.source, &["branch", "-D", branch]);
        } else {
            let _ = fs::remove_dir_all(self.task_root);
        }
    }
}
pub(in crate::workbench::project) fn checkpoint(
    store: &Store,
    ws: &str,
    task_id: &str,
    paths: Vec<String>,
    backend: &str,
) -> WorkbenchResult<ProjectRecord> {
    if !files::REPLACEMENT {
        return Err(WorkbenchError::invalid("Task file isolation is unavailable on this platform; safe replacement has not been qualified."));
    }
    if !["copy", "git"].contains(&backend) || paths.len() > MAX_FILES {
        return Err(WorkbenchError::invalid(
            "Select files and either a local copy or Git worktree",
        ));
    }
    let task: ResearchTask = decode(&record(store, ws, task_id, "task")?)?;
    if ["completed", "rejected"].contains(&task.status.as_str()) {
        return Err(WorkbenchError::invalid(
            "Reopen this task before creating another checkpoint",
        ));
    }
    let source = root(store, ws)?;
    if store.root_path().starts_with(&source) {
        return Err(WorkbenchError::invalid("Move Pipeline's private data folder outside this project before creating isolated task copies"));
    }
    let reader = files::SafeRoot::open(&source)?;
    let mut base = BTreeMap::new();
    let mut total = 0;
    for path in paths {
        files::relative(&path)?;
        if ignored(&path, &[]) {
            return Err(WorkbenchError::invalid(
                "This path is inside a build, cache or dependency folder that task capture excludes; choose another file",
            ));
        }
        if base.contains_key(&path) {
            return Err(WorkbenchError::invalid("Duplicate task path"));
        }
        let bytes = reader.read(&path)?;
        total += bytes.len() as u64;
        if total > MAX_CAPTURE {
            return Err(WorkbenchError::invalid(
                "Task capture exceeds 256 MiB; choose a smaller scope",
            ));
        }
        let meta = fs::symlink_metadata(source.join(&path)).map_err(err)?;
        base.insert(
            path.clone(),
            FileEntry {
                path,
                hash: Some(blob(store, ws, &bytes, task_id)?),
                size: bytes.len() as u64,
                status: "captured".into(),
                executable: files::executable(&meta),
            },
        );
    }
    let checkpoint_id = id("checkpoint")?;
    let parent = super::super::connections::private_copy_parent(store, ws);
    fs::create_dir_all(
        parent
            .parent()
            .ok_or_else(|| WorkbenchError::invalid("Missing private copy parent"))?,
    )
    .map_err(err)?;
    match fs::create_dir(&parent) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(err(e)),
    }
    if !fs::symlink_metadata(&parent).map_err(err)?.is_dir()
        || fs::symlink_metadata(&parent)
            .map_err(err)?
            .file_type()
            .is_symlink()
    {
        return Err(WorkbenchError::invalid(
            "Task directory must be a real directory",
        ));
    }
    #[cfg(unix)]
    {
        crate::safety::restrict_permissions(&parent, 0o700).map_err(err)?;
    }
    // Keep app-owned copies out of the source repository's untracked-file list.
    // This ignore file belongs to the private task directory, not the user's root.
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(parent.join(".gitignore"))
    {
        Ok(mut file) => {
            file.write_all(b"*\n").map_err(err)?;
            crate::safety::sync_file(&file).map_err(err)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(err(error)),
    }
    let task_root = parent.join(&checkpoint_id);
    let mut copy = UnsavedCopy {
        source: &source,
        task_root: &task_root,
        branch: None,
        saved: false,
    };
    let mut git_state = None;
    if backend == "git" {
        let top = git(&source, &["rev-parse", "--show-toplevel"])?;
        if fs::canonicalize(String::from_utf8_lossy(&top).trim()).map_err(err)? != source {
            return Err(WorkbenchError::invalid(
                "Git task creation requires the repository root",
            ));
        }
        let special = git(&source, &["ls-files", "--stage"])?;
        if String::from_utf8_lossy(&special)
            .lines()
            .any(|l| l.starts_with("160000 ") || l.starts_with("120000 "))
        {
            return Err(WorkbenchError::invalid(
                "Git submodules and tracked symlinks need a separately prepared task scope",
            ));
        }
        let head = git(&source, &["rev-parse", "HEAD"])?;
        let branch = git(&source, &["symbolic-ref", "--short", "-q", "HEAD"]).ok();
        let status = git(
            &source,
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        )?;
        let name = format!("codex/{checkpoint_id}");
        let path = git_path(&task_root).to_string_lossy().into_owned();
        copy.branch = Some(name.clone());
        // No checkout: avoids filters/hooks and copies the selected actual working
        // bytes (including staged/unstaged/untracked edits) rather than HEAD bytes.
        git(
            &source,
            &[
                "worktree",
                "add",
                "--no-checkout",
                "-b",
                &name,
                &path,
                "HEAD",
            ],
        )?;
        git_state = Some(
            json!({"head":String::from_utf8_lossy(&head).trim(),"branch":branch.map(|b|String::from_utf8_lossy(&b).trim().to_owned()),"status":String::from_utf8_lossy(&status),"taskBranch":name,"coverage":"selected working files; Git index and source branch remain unchanged"}),
        );
    } else {
        fs::create_dir(&task_root).map_err(err)?;
    }
    let target = files::SafeRoot::open(&task_root)?;
    for (n, (path, file)) in base.iter().enumerate() {
        let bytes = blob_read(store, ws, file.hash.as_deref().unwrap_or_default())?;
        target.replace(
            path,
            None,
            false,
            Some(&bytes),
            file.executable,
            &format!("setup-{checkpoint_id}-{n}"),
        )?;
    }
    // Detect source drift during multi-file capture instead of claiming one instant.
    for (path, file) in &base {
        if reader.read(path).map(|b| hash(&b)).ok().as_deref() != file.hash.as_deref() {
            return Err(WorkbenchError::conflict(
                "Project changed while capturing the task; the original files were not modified",
            ));
        }
    }
    let settings: ProjectHomeSettings = decode(&home_record(store, ws)?)?;
    let selection = task
        .anchor_id
        .as_deref()
        .map(|id| record(store, ws, id, "anchor"))
        .transpose()?;
    let specification = json!({"taskId":task_id,"expectedOutputs":task.expected_outputs,"expectedChecks":task.expected_checks,"selection":selection});
    let instruction=format!("{}\nTask specification and selected source material: {}\nAccepted manuscript: {}\nAccepted baseline: {}\nOnly this task copy is writable. Review/accept its captured changes through Pipeline; do not alter the source checkout or Git metadata.",task.objective,specification,settings.manuscript_revision_id.as_deref().unwrap_or("none"),settings.baseline_execution_id.as_deref().unwrap_or("none"));
    let saved=put(store,ws,&checkpoint_id,"checkpoint",0,&Checkpoint{task_id:task_id.into(),backend:backend.into(),root_identity:store::root_identity(&source)?,task_root:task_root.to_string_lossy().into_owned(),task_root_identity:store::root_identity(&task_root)?,base,proposed:BTreeMap::new(),state:"isolated".into(),instruction,git:git_state,limitations:vec!["Only selected regular files were captured. Excluded or large datasets are not implicitly readable in the task.".into(),"Checks are valid only for their recorded input identities; partial acceptance needs fresh checks.".into()],checked_execution_ids:Vec::new(),captured_at:now()})?;
    copy.saved = true;
    Ok(saved)
}
pub(in crate::workbench::project) fn capture_changes(
    store: &Store,
    ws: &str,
    checkpoint_id: &str,
) -> WorkbenchResult<ProjectRecord> {
    let r = record(store, ws, checkpoint_id, "checkpoint")?;
    let mut c: Checkpoint = decode(&r)?;
    if !["isolated", "review"].contains(&c.state.as_str()) {
        return Err(WorkbenchError::invalid(
            "This checkpoint is already accepted or rejected; create another task copy",
        ));
    }
    let task_root = task_root_path(store, ws, checkpoint_id, &c)?;
    research::jobs::ensure_copy_free(ws, &task_root)?;
    let inventory = inventory_at(&task_root, &[])?;
    if !inventory.complete {
        return Err(WorkbenchError::invalid("Task contains unsupported/large files or exceeds capture limits; remove them or narrow the task before review"));
    }
    let reader = files::SafeRoot::open(&task_root)?;
    let mut proposed = BTreeMap::new();
    for mut f in inventory.files {
        // Base and earlier captures stored their content as blobs already.
        let stored = [&c.proposed, &c.base]
            .iter()
            .any(|known| known.get(&f.path).is_some_and(|k| k.hash == f.hash));
        if !stored {
            let bytes = reader.read(&f.path)?;
            f.hash = Some(blob(store, ws, &bytes, checkpoint_id)?);
        }
        proposed.insert(f.path.clone(), f);
    }
    // A captured base file that inventory skips (an ignored folder) must
    // never read as deleted merely because it was skipped: re-read it from
    // the task copy so acceptance cannot unlink the original.
    for (path, entry) in &c.base {
        if proposed.contains_key(path) || !ignored(path, &[]) {
            continue;
        }
        // A copy from an earlier build can list a file in a folder that is now
        // reserved. Nothing here can read that file, so it stays as it was.
        if path.split('/').any(files::reserved_component) {
            proposed.insert(path.clone(), entry.clone());
            continue;
        }
        if let Ok(bytes) = reader.read(path) {
            let mut f = entry.clone();
            f.size = bytes.len() as u64;
            f.hash = Some(blob(store, ws, &bytes, checkpoint_id)?);
            proposed.insert(path.clone(), f);
        }
    }
    c.proposed = proposed;
    c.state = "review".into();
    c.checked_execution_ids = research::list_executions(store, ws)?
        .into_iter()
        .filter(|e| e.cwd == c.task_root && e.outcome == "completed")
        .map(|e| e.id)
        .collect();
    put(store, ws, checkpoint_id, "checkpoint", r.revision, &c)
}

pub(in crate::workbench::project) fn reject(
    store: &Store,
    ws: &str,
    checkpoint_id: &str,
) -> WorkbenchResult<ProjectRecord> {
    ensure_no_application(store, ws, checkpoint_id)?;
    let r = record(store, ws, checkpoint_id, "checkpoint")?;
    let mut c: Checkpoint = decode(&r)?;
    if !["isolated", "review"].contains(&c.state.as_str()) {
        return Err(WorkbenchError::invalid(
            "Accepted changes must be undone explicitly",
        ));
    }
    research::jobs::ensure_copy_free(ws, Path::new(&c.task_root))?;
    c.state = "rejected".into();
    put(store, ws, checkpoint_id, "checkpoint", r.revision, &c)
}

pub fn task_session(
    store: &Store,
    ws: &str,
    checkpoint_id: &str,
) -> WorkbenchResult<store::WorkbenchSession> {
    let _lock = lock(store, ws)?;
    let r = record(store, ws, checkpoint_id, "checkpoint")?;
    let c: Checkpoint = decode(&r)?;
    task_root_path(store, ws, checkpoint_id, &c)?;
    if !["isolated", "review"].contains(&c.state.as_str()) {
        return Err(WorkbenchError::invalid(
            "This task copy is no longer editable",
        ));
    }
    let session = store
        .create_session(store::CreateSessionRequest {
            workspace_id: Some(ws.into()),
            title: format!(
                "Task: {}",
                c.instruction.chars().take(80).collect::<String>()
            ),
            operation_id: id("session")?,
        })?
        .record;
    Ok(store
        .update_session(store::UpdateSessionRequest {
            session_id: session.id,
            expected_revision: session.revision,
            operation_id: id("bind")?,
            title: None,
            draft: Some(c.instruction),
            overrides: Some(json!({"mode":"edit","projectCheckpointId":checkpoint_id})),
            archived: None,
            preset_id: Some("writing".into()),
            paper_id: None,
            clear_paper: None,
        })?
        .record)
}
pub fn session_task_root(store: &Store, ws: &str, checkpoint_id: &str) -> WorkbenchResult<PathBuf> {
    ensure_no_application(store, ws, checkpoint_id)?;
    let c: Checkpoint = decode(&record(store, ws, checkpoint_id, "checkpoint")?)?;
    if !["isolated", "review"].contains(&c.state.as_str()) {
        return Err(WorkbenchError::invalid(
            "This task has been accepted or rejected. Create a fresh task to continue editing.",
        ));
    }
    task_root_path(store, ws, checkpoint_id, &c)
}
