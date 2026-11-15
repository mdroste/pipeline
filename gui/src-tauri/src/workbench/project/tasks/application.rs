//! Accept, recover, and undo project changes through the application journal.

use super::{copies::task_root_path, Application, AppliedFile, Checkpoint};
use crate::workbench::project::{
    blob, blob_read, decode, err, files, hash, id, put, record, root, FileEntry, ProjectRecord,
};
use crate::workbench::{
    research,
    store::{self, Store, WorkbenchError, WorkbenchResult},
};
use rusqlite::params;
use std::collections::BTreeSet;

/// `Application::direction` of changes accepted from within one file. Unlike a
/// file-level acceptance, a finished one leaves the rest of its edit open.
const ACCEPT_CHANGES: &str = "accept_changes";

pub(super) fn ensure_no_application(
    store: &Store,
    ws: &str,
    checkpoint_id: &str,
) -> WorkbenchResult<()> {
    let exists: bool=store.connection()?.query_row("SELECT EXISTS(SELECT 1 FROM project_records WHERE workspace_id=?1 AND kind='application' AND json_extract(body_json,'$.checkpointId')=?2 AND json_extract(body_json,'$.state') <> 'rolled_back' AND NOT (json_extract(body_json,'$.direction')=?3 AND json_extract(body_json,'$.state')='applied'))",params![ws,checkpoint_id,ACCEPT_CHANGES],|r|r.get(0)).map_err(err)?;
    if exists {
        return Err(WorkbenchError::conflict("This task has an acceptance journal. Inspect its recovery/undo history before continuing."));
    }
    Ok(())
}

/// The project root for paths from saved edits and their journals, which earlier
/// builds could write in the generated folder. Those paths stay reachable so that
/// old edits can still be accepted, undone, and recovered. Capture gives a new
/// edit no path there, so this reaches nothing a model has written since.
fn original(store: &Store, ws: &str, identity: &str) -> WorkbenchResult<files::SafeRoot> {
    let p = root(store, ws)?;
    if store::root_identity(&p)? != identity {
        return Err(WorkbenchError::conflict(
            "Project root identity changed; application cannot be replayed",
        ));
    }
    files::SafeRoot::for_records(&p)
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
/// Why a project file no longer matches its edit. An acceptance stopped between
/// writing the file and updating the edit leaves that file looking changed by
/// someone else, though the change is its own and its Undo restores both.
fn external_change(
    store: &Store,
    ws: &str,
    checkpoint_id: &str,
    path: &str,
    before: Option<&str>,
    current: Option<&str>,
) -> WorkbenchResult<WorkbenchError> {
    let interrupted: bool = store
        .connection()?
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM project_records a, json_each(a.body_json,'$.files') f WHERE a.workspace_id=?1 AND a.kind='application' AND json_extract(a.body_json,'$.checkpointId')=?2 AND json_extract(a.body_json,'$.direction')=?3 AND json_extract(a.body_json,'$.state')='applied' AND json_extract(f.value,'$.path')=?4 AND json_extract(f.value,'$.before') IS ?5 AND json_extract(f.value,'$.after') IS ?6)",
            params![ws, checkpoint_id, ACCEPT_CHANGES, path, before, current],
            |r| r.get(0),
        )
        .map_err(err)?;
    Ok(WorkbenchError::conflict(if interrupted {
        format!("An earlier acceptance in {path} was interrupted. Its change is in the project; choose Undo under Accepted edits, then accept again.")
    } else {
        format!("External change detected in {path}; no changes applied")
    }))
}
pub(in crate::workbench::project) fn apply(
    store: &Store,
    ws: &str,
    checkpoint_id: &str,
    paths: Vec<String>,
    expected: i64,
) -> WorkbenchResult<ProjectRecord> {
    if !files::REPLACEMENT {
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
        let current = source.optional_read(path)?.as_deref().map(hash);
        if current != before_hash
            || source.executable(path)? != before.is_some_and(|f| f.executable)
        {
            return Err(external_change(
                store,
                ws,
                checkpoint_id,
                path,
                before_hash.as_deref(),
                current.as_deref(),
            )?);
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
/// Accept part of one file's changes. `content` is the file with the selected
/// hunks applied. It reaches the project through the same journal as `apply`,
/// so Undo and recovery work, and it becomes the edit's base for that file: the
/// remaining diff shrinks, and later acceptances compare against what is now in
/// the project, which is how an external change is still refused.
pub(in crate::workbench::project) fn accept_changes(
    store: &Store,
    ws: &str,
    checkpoint_id: &str,
    path: &str,
    content: &str,
    expected: i64,
) -> WorkbenchResult<ProjectRecord> {
    if !files::REPLACEMENT {
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
    let before = match c.base.get(path) {
        Some(before) if c.proposed.contains_key(path) && changed(&c).contains(path) => {
            before.clone()
        }
        _ => {
            return Err(WorkbenchError::invalid(
                "Select a changed file from this review",
            ))
        }
    };
    let source = original(store, ws, &c.root_identity)?;
    let current = source.optional_read(path)?.as_deref().map(hash);
    if current != before.hash || source.executable(path)? != before.executable {
        return Err(external_change(
            store,
            ws,
            checkpoint_id,
            path,
            before.hash.as_deref(),
            current.as_deref(),
        )?);
    }
    let bytes = content.as_bytes();
    if before.hash.as_deref() == Some(hash(bytes).as_str()) {
        return Err(WorkbenchError::invalid("Select a change to accept"));
    }
    let after = blob(store, ws, bytes, checkpoint_id)?;
    let app_id = id("apply")?;
    let application = Application {
        checkpoint_id: checkpoint_id.into(),
        root_identity: c.root_identity.clone(),
        state: "applying".into(),
        files: vec![AppliedFile {
            path: path.into(),
            before: before.hash.clone(),
            after: Some(after.clone()),
            before_executable: before.executable,
            after_executable: before.executable,
            claim: format!("{app_id}-0"),
            state: "pending".into(),
        }],
        direction: ACCEPT_CHANGES.into(),
        error: None,
        checks_valid: false,
    };
    let result = run_application(store, ws, &app_id, application, 0)?;
    c.base.insert(
        path.into(),
        FileEntry {
            hash: Some(after),
            size: bytes.len() as u64,
            ..before
        },
    );
    const LIMITATION: &str =
        "Only selected changes were accepted. Prior task checks do not certify the assembled project.";
    if changed(&c).is_empty() {
        c.state = "accepted".into();
    } else if !c.limitations.iter().any(|known| known == LIMITATION) {
        c.limitations.push(LIMITATION.into());
    }
    put(store, ws, checkpoint_id, "checkpoint", r.revision, &c)?;
    Ok(result)
}
/// Project files reach or leave the project only here, so this is where an
/// edit gives way to a job that declared one of them.
fn ensure_job_free(store: &Store, ws: &str, application: &Application) -> WorkbenchResult<()> {
    research::jobs::ensure_paths_free(
        ws,
        &root(store, ws)?,
        application.files.iter().map(|f| f.path.as_str()),
    )
}
fn run_application(
    store: &Store,
    ws: &str,
    app_id: &str,
    mut application: Application,
    revision: i64,
) -> WorkbenchResult<ProjectRecord> {
    let source = original(store, ws, &application.root_identity)?;
    ensure_job_free(store, ws, &application)?;
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
/// Undo reopens an edit only while its copy exists to reopen. Once the copy is
/// gone the edit is over: its project files are back, and nothing can continue.
fn state_after_undo(store: &Store, ws: &str, checkpoint_id: &str, c: &Checkpoint) -> &'static str {
    if task_root_path(store, ws, checkpoint_id, c).is_ok() {
        "review"
    } else {
        "undone"
    }
}
pub(in crate::workbench::project) fn recover(
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
    ensure_job_free(store, ws, &a)?;
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
    if a.direction == ACCEPT_CHANGES {
        // Their project files are back, so the edit's base must follow, or the
        // next acceptance would read them as an external change.
        for f in &a.files {
            if let (Some(entry), Some(before)) = (c.base.get_mut(&f.path), &f.before) {
                entry.size = blob_read(store, ws, before)?.len() as u64;
                entry.hash = Some(before.clone());
            }
        }
        if c.state == "accepted" && ensure_no_application(store, ws, &a.checkpoint_id).is_ok() {
            c.state = state_after_undo(store, ws, &a.checkpoint_id, &c).into();
        }
    } else {
        c.state = state_after_undo(store, ws, &a.checkpoint_id, &c).into();
    }
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
pub(in crate::workbench::project) fn undo(
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
    ensure_job_free(store, ws, &a)?;
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
