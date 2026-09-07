use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchTask {
    pub objective: String,
    pub anchor_id: Option<String>,
    pub expected_outputs: Vec<String>,
    pub expected_checks: Vec<String>,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    pub task_id: String,
    pub backend: String,
    pub root_identity: String,
    pub task_root: String,
    pub task_root_identity: String,
    pub base: BTreeMap<String, FileEntry>,
    pub proposed: BTreeMap<String, FileEntry>,
    pub state: String,
    pub instruction: String,
    pub git: Option<Value>,
    pub limitations: Vec<String>,
    pub checked_execution_ids: Vec<String>,
    pub captured_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedFile {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub before_executable: bool,
    pub after_executable: bool,
    pub claim: String,
    pub state: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Application {
    pub checkpoint_id: String,
    pub root_identity: String,
    pub state: String,
    pub files: Vec<AppliedFile>,
    pub direction: String,
    pub error: Option<String>,
    pub checks_valid: bool,
}

pub(super) fn create_task(
    store: &Store,
    ws: &str,
    objective: String,
    anchor_id: Option<String>,
    expected_outputs: Vec<String>,
    expected_checks: Vec<String>,
) -> WorkbenchResult<ProjectRecord> {
    bounded(&objective, 16000)?;
    if expected_outputs.len() > 100 || expected_checks.len() > 100 {
        return Err(WorkbenchError::invalid(
            "At most 100 outputs/checks per task",
        ));
    }
    for value in expected_outputs.iter().chain(expected_checks.iter()) {
        bounded(value, 1000)?;
    }
    if let Some(anchor) = &anchor_id {
        record(store, ws, anchor, "anchor")?;
    }
    put(
        store,
        ws,
        &id("task")?,
        "task",
        0,
        &ResearchTask {
            objective,
            anchor_id,
            expected_outputs,
            expected_checks,
            status: "open".into(),
        },
    )
}
pub(super) fn update_task(
    store: &Store,
    ws: &str,
    task_id: &str,
    expected: i64,
    status: &str,
) -> WorkbenchResult<ProjectRecord> {
    if !["open", "investigating", "deferred", "completed", "rejected"].contains(&status) {
        return Err(WorkbenchError::invalid("Invalid task disposition"));
    }
    let r = record(store, ws, task_id, "task")?;
    let mut task: ResearchTask = decode(&r)?;
    task.status = status.into();
    put(store, ws, task_id, "task", expected, &task)
}
pub(super) fn git(root: &Path, args: &[&str]) -> WorkbenchResult<Vec<u8>> {
    use std::process::{Command, Stdio};
    // This is a host operation explicitly selected by the user. Paths are argv,
    // never interpolated into shell code. On this Mac every Git call uses zsh.
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = Command::new("/bin/zsh");
        c.args(["-lic", "git \"$@\"", "pipeline-git"]);
        c
    };
    #[cfg(not(target_os = "macos"))]
    let mut command = Command::new("git");
    command
        .args([
            "--no-optional-locks",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
        ])
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null());
    // File-backed capture cannot block behind inherited pipes. Independent owner:
    // Workflow cancellation never receives this PID.
    let out = tempfile::tempfile().map_err(err)?;
    let error = tempfile::tempfile().map_err(err)?;
    command
        .stdout(out.try_clone().map_err(err)?)
        .stderr(error.try_clone().map_err(err)?);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(err)?;
    let pid = child.id();
    crate::commands::lifecycle::register_independent_child_pid(pid);
    let started = Instant::now();
    let result = loop {
        if out.metadata().map_err(err)?.len() > 2 * 1024 * 1024
            || error.metadata().map_err(err)?.len() > 2 * 1024 * 1024
        {
            break Err(WorkbenchError::invalid(
                "Git output exceeded its bounded capture limit",
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if started.elapsed() < Duration::from_secs(30) => {
                std::thread::sleep(Duration::from_millis(25))
            }
            Ok(None) => {
                break Err(WorkbenchError::invalid(
                    "Git operation timed out; inspect task state before retrying",
                ))
            }
            Err(e) => break Err(err(e)),
        }
    };
    // Reap descendants even after leader exit.
    crate::commands::lifecycle::kill_independent_process(pid);
    let _ = child.wait();
    crate::commands::lifecycle::unregister_independent_child_pid(pid);
    use std::io::{Seek, SeekFrom};
    let mut out = out;
    let mut error = error;
    out.seek(SeekFrom::Start(0)).map_err(err)?;
    error.seek(SeekFrom::Start(0)).map_err(err)?;
    let mut bytes = Vec::new();
    out.take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if !result?.success() {
        let mut message = String::new();
        error.take(8192).read_to_string(&mut message).map_err(err)?;
        return Err(WorkbenchError::invalid(format!(
            "Git task setup failed: {message}"
        )));
    }
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(WorkbenchError::invalid("Git metadata exceeds its limit"));
    }
    Ok(bytes)
}
fn ensure_no_application(store: &Store, ws: &str, checkpoint_id: &str) -> WorkbenchResult<()> {
    let exists: bool=store.connection()?.query_row("SELECT EXISTS(SELECT 1 FROM project_records WHERE workspace_id=?1 AND kind='application' AND json_extract(body_json,'$.checkpointId')=?2 AND json_extract(body_json,'$.state') <> 'rolled_back')",params![ws,checkpoint_id],|r|r.get(0)).map_err(err)?;
    if exists {
        return Err(WorkbenchError::conflict("This task has an acceptance journal. Inspect its recovery/undo history before continuing."));
    }
    Ok(())
}
fn task_root_path(
    store: &Store,
    ws: &str,
    checkpoint_id: &str,
    checkpoint: &Checkpoint,
) -> WorkbenchResult<PathBuf> {
    let current = root(store, ws)?;
    if super::super::store::root_identity(&current)? != checkpoint.root_identity {
        return Err(WorkbenchError::conflict(
            "The project root changed; restore/remap task files explicitly before continuing",
        ));
    }
    let expected = current.join(SCRATCH).join(checkpoint_id);
    if expected != Path::new(&checkpoint.task_root)
        || fs::symlink_metadata(&expected)
            .map_err(err)?
            .file_type()
            .is_symlink()
        || super::super::store::root_identity(&expected)? != checkpoint.task_root_identity
    {
        return Err(WorkbenchError::conflict(
            "Task copy is missing, relocated, or replaced; its saved snapshots remain available",
        ));
    }
    Ok(expected)
}
pub(super) fn checkpoint(
    store: &Store,
    ws: &str,
    task_id: &str,
    paths: Vec<String>,
    backend: &str,
) -> WorkbenchResult<ProjectRecord> {
    if !cfg!(unix) {
        return Err(WorkbenchError::invalid("Task file isolation is unavailable on this platform; safe replacement has not been qualified."));
    }
    if !["copy", "git"].contains(&backend) || paths.is_empty() || paths.len() > MAX_FILES {
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
    let reader = files::SafeRoot::open(&source)?;
    let mut base = BTreeMap::new();
    let mut total = 0;
    for path in paths {
        files::relative(&path)?;
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
    let parent = source.join(SCRATCH);
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
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).map_err(err)?;
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
            file.sync_all().map_err(err)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(err(error)),
    }
    let task_root = parent.join(&checkpoint_id);
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
        let path = task_root.to_string_lossy().into_owned();
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
    put(store,ws,&checkpoint_id,"checkpoint",0,&Checkpoint{task_id:task_id.into(),backend:backend.into(),root_identity:super::super::store::root_identity(&source)?,task_root:task_root.to_string_lossy().into_owned(),task_root_identity:super::super::store::root_identity(&task_root)?,base,proposed:BTreeMap::new(),state:"isolated".into(),instruction,git:git_state,limitations:vec!["Only selected regular files were captured. Excluded or large datasets are not implicitly readable in the task.".into(),"Checks are valid only for their recorded input identities; partial acceptance needs fresh checks.".into()],checked_execution_ids:Vec::new(),captured_at:now()})
}
pub(super) fn capture_changes(
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
    let inventory = inventory_at(&task_root, &[])?;
    if !inventory.complete {
        return Err(WorkbenchError::invalid("Task contains unsupported/large files or exceeds capture limits; remove them or narrow the task before review"));
    }
    let reader = files::SafeRoot::open(&task_root)?;
    let mut proposed = BTreeMap::new();
    for mut f in inventory.files {
        let bytes = reader.read(&f.path)?;
        f.hash = Some(blob(store, ws, &bytes, checkpoint_id)?);
        proposed.insert(f.path.clone(), f);
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
fn original(store: &Store, ws: &str, identity: &str) -> WorkbenchResult<files::SafeRoot> {
    let p = root(store, ws)?;
    if super::super::store::root_identity(&p)? != identity {
        return Err(WorkbenchError::conflict(
            "Project root identity changed; application cannot be replayed",
        ));
    }
    files::SafeRoot::open(&p)
}
fn changed(c: &Checkpoint) -> BTreeSet<String> {
    c.base
        .keys()
        .chain(c.proposed.keys())
        .filter(|p| {
            c.base.get(*p).map(|f| (&f.hash, f.executable))
                != c.proposed.get(*p).map(|f| (&f.hash, f.executable))
        })
        .cloned()
        .collect()
}
pub(super) fn apply(
    store: &Store,
    ws: &str,
    checkpoint_id: &str,
    paths: Vec<String>,
    expected: i64,
) -> WorkbenchResult<ProjectRecord> {
    if !cfg!(unix) {
        return Err(WorkbenchError::invalid("File acceptance is unavailable on this platform; safe replacement has not been qualified."));
    }
    ensure_no_application(store, ws, checkpoint_id)?;
    let r = record(store, ws, checkpoint_id, "checkpoint")?;
    if r.revision != expected {
        return Err(WorkbenchError::conflict(
            "Change set changed; refresh before accepting",
        ));
    }
    let mut c: Checkpoint = decode(&r)?;
    if c.state != "review" {
        return Err(WorkbenchError::invalid(
            "Capture task changes before accepting them",
        ));
    }
    let source = original(store, ws, &c.root_identity)?;
    let changes = changed(&c);
    let selection = paths.into_iter().collect::<BTreeSet<_>>();
    if selection.is_empty() || !selection.is_subset(&changes) {
        return Err(WorkbenchError::invalid(
            "Select changed files from this review",
        ));
    }
    let app_id = id("apply")?;
    let mut files = Vec::new();
    for (n, path) in selection.iter().enumerate() {
        let before = c.base.get(path);
        let after = c.proposed.get(path);
        let before_hash = before.and_then(|f| f.hash.clone());
        if source.optional_read(path)?.as_deref().map(hash) != before_hash
            || source.executable(path)? != before.is_some_and(|f| f.executable)
        {
            return Err(WorkbenchError::conflict(format!(
                "External change detected in {path}; no changes applied"
            )));
        }
        files.push(AppliedFile {
            path: path.clone(),
            before: before_hash,
            after: after.and_then(|f| f.hash.clone()),
            before_executable: before.is_some_and(|f| f.executable),
            after_executable: after.is_some_and(|f| f.executable),
            claim: format!("{app_id}-{n}"),
            state: "pending".into(),
        });
    }
    let application = Application {
        checkpoint_id: checkpoint_id.into(),
        root_identity: c.root_identity.clone(),
        state: "applying".into(),
        files,
        direction: "accept".into(),
        error: None,
        checks_valid: false,
    };
    let result = run_application(store, ws, &app_id, application, 0)?;
    c.state = if selection == changes {
        "accepted"
    } else {
        "partially_accepted"
    }
    .into();
    if selection != changes {
        c.limitations.push("Only selected files were accepted. Prior task checks do not certify the assembled project.".into());
    }
    put(store, ws, checkpoint_id, "checkpoint", r.revision, &c)?;
    Ok(result)
}
fn run_application(
    store: &Store,
    ws: &str,
    app_id: &str,
    mut application: Application,
    revision: i64,
) -> WorkbenchResult<ProjectRecord> {
    let source = original(store, ws, &application.root_identity)?;
    let mut saved = put(store, ws, app_id, "application", revision, &application)?;
    for n in 0..application.files.len() {
        let f = &application.files[n];
        let bytes = f
            .after
            .as_ref()
            .map(|h| blob_read(store, ws, h))
            .transpose()?;
        if let Err(error) = source.replace(
            &f.path,
            f.before.as_deref(),
            f.before_executable,
            bytes.as_deref(),
            f.after_executable,
            &f.claim,
        ) {
            application.state = "recovery_required".into();
            application.error = Some(error.message);
            put(
                store,
                ws,
                app_id,
                "application",
                saved.revision,
                &application,
            )?;
            return Err(WorkbenchError::conflict(format!("Application {app_id} needs recovery. Existing edits and original snapshots are retained.")));
        }
        application.files[n].state = "applied".into();
        saved = put(
            store,
            ws,
            app_id,
            "application",
            saved.revision,
            &application,
        )?;
    }
    application.state = "applied".into();
    saved = put(
        store,
        ws,
        app_id,
        "application",
        saved.revision,
        &application,
    )?;
    for f in &application.files {
        source.clear_claim(&f.path, &f.claim)?;
    }
    Ok(saved)
}
pub(super) fn recover(
    store: &Store,
    ws: &str,
    application_id: &str,
) -> WorkbenchResult<ProjectRecord> {
    let r = record(store, ws, application_id, "application")?;
    let mut a: Application = decode(&r)?;
    if a.state == "rolled_back" {
        return Ok(r);
    }
    if !["applying", "recovery_required"].contains(&a.state.as_str()) {
        return Err(WorkbenchError::invalid(
            "Use Undo for an already accepted application",
        ));
    }
    let source = original(store, ws, &a.root_identity)?;
    for (n, f) in a.files.iter_mut().enumerate().rev() {
        if source.recover_claim(&f.path, &f.claim)? {
            f.state = "restored".into();
            continue;
        }
        let recovery = format!("recover-{application_id}-{n}");
        // A crash during rollback may have claimed the accepted file but not yet
        // installed the original. Restore that claim before deciding what to do.
        source.recover_claim(&f.path, &recovery)?;
        let current = source.optional_read(&f.path)?.as_deref().map(hash);
        if current == f.before && source.executable(&f.path)? == f.before_executable {
            source.clear_claim(&f.path, &f.claim)?;
            source.clear_claim(&f.path, &recovery)?;
            f.state = "restored".into();
            continue;
        }
        if current != f.after || source.executable(&f.path)? != f.after_executable {
            return Err(WorkbenchError::conflict(format!("External edit in {} prevents rollback. Keep it and resolve manually; original snapshots remain available.",f.path)));
        }
        let bytes = f
            .before
            .as_ref()
            .map(|h| blob_read(store, ws, h))
            .transpose()?;
        source.replace(
            &f.path,
            f.after.as_deref(),
            f.after_executable,
            bytes.as_deref(),
            f.before_executable,
            &recovery,
        )?;
        source.clear_claim(&f.path, &recovery)?;
        source.clear_claim(&f.path, &f.claim)?;
        f.state = "restored".into();
    }
    a.state = "rolled_back".into();
    a.error = None;
    let result = put(store, ws, application_id, "application", r.revision, &a)?;
    let checkpoint = record(store, ws, &a.checkpoint_id, "checkpoint")?;
    let mut c: Checkpoint = decode(&checkpoint)?;
    c.state = "review".into();
    put(
        store,
        ws,
        &checkpoint.id,
        "checkpoint",
        checkpoint.revision,
        &c,
    )?;
    Ok(result)
}
pub(super) fn undo(
    store: &Store,
    ws: &str,
    application_id: &str,
) -> WorkbenchResult<ProjectRecord> {
    let r = record(store, ws, application_id, "application")?;
    let mut a: Application = decode(&r)?;
    if a.state != "applied" {
        return Err(WorkbenchError::invalid(
            "Only an applied change set can be undone",
        ));
    }
    let source = original(store, ws, &a.root_identity)?;
    for f in &a.files {
        if source.optional_read(&f.path)?.as_deref().map(hash) != f.after
            || source.executable(&f.path)? != f.after_executable
        {
            return Err(WorkbenchError::conflict(format!(
                "{} has subsequent edits; undo would overwrite them",
                f.path
            )));
        }
    }
    a.state = "recovery_required".into();
    put(store, ws, application_id, "application", r.revision, &a)?;
    recover(store, ws, application_id)
}
pub(super) fn reject(
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
    c.state = "rejected".into();
    put(store, ws, checkpoint_id, "checkpoint", r.revision, &c)
}

pub fn task_session(
    store: &Store,
    ws: &str,
    checkpoint_id: &str,
) -> WorkbenchResult<super::super::store::WorkbenchSession> {
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
        .create_session(super::super::store::CreateSessionRequest {
            workspace_id: Some(ws.into()),
            title: format!(
                "Task: {}",
                c.instruction.chars().take(80).collect::<String>()
            ),
            operation_id: id("session")?,
        })?
        .record;
    Ok(store
        .update_session(super::super::store::UpdateSessionRequest {
            session_id: session.id,
            expected_revision: session.revision,
            operation_id: id("bind")?,
            title: None,
            draft: Some(c.instruction),
            overrides: Some(json!({"mode":"edit","projectCheckpointId":checkpoint_id})),
            archived: None,
            preset_id: Some("paper_revision".into()),
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
