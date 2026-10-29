//! Project home, immutable document selections, and reversible research tasks.
//! No Workflow state or provider DTOs cross this boundary.
use super::research;
use super::store::{Store, WorkbenchError, WorkbenchResult};
use base64::Engine as _;
use fs2::FileExt as _;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

mod blobs;
mod home;
mod inventory;
mod records;

use blobs::{blob, blob_read};
pub(super) use home::home_record;
use home::save_home;
pub use home::{
    excluded_notes, home, project_context, selected_manuscript, ProjectHome, ProjectHomeSettings,
};
use inventory::{ignored, inventory_at, refresh};
pub(crate) use inventory::{materialize_research_files, task_capture};
pub use inventory::{FileEntry, Inventory};
pub use records::{anchor_read, record, records, ProjectRecord};
use records::{decode, put, read_record};

pub(crate) mod assistant_files;
mod documents;
mod file_workspace;
mod files;
mod index;
pub use index::{project_index, ProjectIndexItem};
pub mod relations;
pub mod repository;
pub use file_workspace::{
    read_conversation_file, read_workspace_file, snapshot_conversation_file, FilePreview,
    FileReadRequest,
};
mod studio;
mod task_pages;
mod tasks;
pub use documents::*;
pub use studio::*;
pub use task_pages::{task_page, TaskCursor, TaskPage};
pub use tasks::*;
#[cfg(test)]
mod acceptance_tests;
#[cfg(test)]
mod home_tests;
#[cfg(test)]
mod test_support;
#[cfg(all(test, unix))]
// Qualification helpers are used only by the macOS-gated tests.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod tests;

const MAX_FILE: u64 = 32 * 1024 * 1024;
const MAX_CAPTURE: u64 = 256 * 1024 * 1024;
const MAX_FILES: usize = 5000;
const SCRATCH: &str = ".pipeline-tasks";
fn err(e: impl std::fmt::Display) -> WorkbenchError {
    WorkbenchError::storage("Project operation failed", e)
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn id(prefix: &str) -> WorkbenchResult<String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(err)?;
    Ok(format!(
        "{prefix}_{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}
fn valid_id(value: &str) -> WorkbenchResult<()> {
    if value.is_empty()
        || value.len() > 200
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_- .".contains(&b) && b != b' ')
    {
        return Err(WorkbenchError::invalid("Invalid project object ID"));
    }
    Ok(())
}
fn bounded(value: &str, max: usize) -> WorkbenchResult<()> {
    if value.trim().is_empty() || value.len() > max || value.contains('\0') {
        return Err(WorkbenchError::invalid(
            "Text is empty or exceeds its limit",
        ));
    }
    Ok(())
}
fn scope(store: &Store, workspace_id: &str) -> WorkbenchResult<()> {
    valid_id(workspace_id)?;
    if store.workspace(workspace_id)?.archived_at.is_some() {
        return Err(WorkbenchError::invalid("Restore this Workspace first"));
    }
    Ok(())
}
fn root(store: &Store, workspace_id: &str) -> WorkbenchResult<PathBuf> {
    scope(store, workspace_id)?;
    store.registered_root(&store.workspace(workspace_id)?)
}
fn lock(store: &Store, workspace_id: &str) -> WorkbenchResult<fs::File> {
    valid_id(workspace_id)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(
            store
                .root_path()
                .join(format!(".project-{workspace_id}.lock")),
        )
        .map_err(err)?;
    file.try_lock_exclusive().map_err(|_| {
        WorkbenchError::conflict("Another project operation is in progress; retry when it finishes")
    })?;
    Ok(file)
}
pub fn capabilities() -> Value {
    let tool =
        |name: &str| crate::deps::find_on_path(name).map(|p| p.to_string_lossy().into_owned());
    json!({"platform":std::env::consts::OS,"fileAcceptance":files::REPLACEMENT,"git":tool("git"),"latex":tool("pdflatex").or_else(||tool("latexmk")),"pdfPages":tool("pdftoppm"),"stataPolicy":"Use the Stata adapter with an installed Stata in batch mode, or with a custom launcher for a shell alias. A successful profile test verifies the setup; detection alone does not. Windows and Linux use is not yet release-qualified.","qualification":"Project reading is local and needs no model. Native model turns require ChatGPT sign-in. Research commands require explicit host authorization. Packaged builds and other platforms remain release qualification gates.","record":"docs/workbench/release-qualification.md"})
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ProjectAction {
    SaveHome {
        expected_revision: i64,
        settings: ProjectHomeSettings,
    },
    Refresh,
    Annotate {
        selection: DocumentSelection,
        body: String,
    },
    CreateTask {
        objective: String,
        anchor_id: Option<String>,
        expected_outputs: Vec<String>,
        expected_checks: Vec<String>,
    },
    UpdateTask {
        task_id: String,
        expected_revision: i64,
        status: String,
    },
    Checkpoint {
        task_id: String,
        paths: Vec<String>,
        backend: String,
    },
    CaptureChanges {
        checkpoint_id: String,
    },
    Apply {
        checkpoint_id: String,
        paths: Vec<String>,
        expected_revision: i64,
    },
    Undo {
        application_id: String,
    },
    Recover {
        application_id: String,
    },
    Reject {
        checkpoint_id: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMutation {
    pub workspace_id: String,
    pub operation_id: String,
    #[serde(flatten)]
    pub action: ProjectAction,
}

pub fn mutate(store: &Store, request: ProjectMutation) -> WorkbenchResult<Value> {
    scope(store, &request.workspace_id)?;
    valid_id(&request.operation_id)?;
    let _lock = lock(store, &request.workspace_id)?;
    let fingerprint = hash(&serde_json::to_vec(&request).map_err(err)?);
    let saved: Option<(String, String)> = store
        .connection()?
        .query_row(
            "SELECT request_hash,response_json FROM project_operations WHERE operation_id=?1",
            [&request.operation_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(err)?;
    if let Some((old, response)) = saved {
        if old != fingerprint {
            return Err(WorkbenchError::conflict(
                "Operation ID was reused with different arguments",
            ));
        }
        return serde_json::from_str(&response).map_err(err);
    }
    // Persist unknown before filesystem side effects. A failed/ambiguous mutation is
    // never replayed; callers refresh and use the recovery operation where applicable.
    store.connection()?.execute("INSERT INTO project_operations VALUES(?1,?2,?3,?4)",params![request.operation_id,fingerprint,json!({"outcome":"unknown","recovery":"Refresh the project; inspect interrupted applications before retrying with a new operation ID"}).to_string(),now()]).map_err(err)?;
    let ws = &request.workspace_id;
    let record = match request.action {
        ProjectAction::SaveHome {
            expected_revision,
            settings,
        } => save_home(store, ws, expected_revision, settings),
        ProjectAction::Refresh => refresh(store, ws),
        ProjectAction::Annotate { selection, body } => {
            documents::annotate(store, ws, selection, body)
        }
        ProjectAction::CreateTask {
            objective,
            anchor_id,
            expected_outputs,
            expected_checks,
        } => tasks::create_task(
            store,
            ws,
            objective,
            anchor_id,
            expected_outputs,
            expected_checks,
        ),
        ProjectAction::UpdateTask {
            task_id,
            expected_revision,
            status,
        } => tasks::update_task(store, ws, &task_id, expected_revision, &status),
        ProjectAction::Checkpoint {
            task_id,
            paths,
            backend,
        } => tasks::checkpoint(store, ws, &task_id, paths, &backend),
        ProjectAction::CaptureChanges { checkpoint_id } => {
            tasks::capture_changes(store, ws, &checkpoint_id)
        }
        ProjectAction::Apply {
            checkpoint_id,
            paths,
            expected_revision,
        } => tasks::apply(store, ws, &checkpoint_id, paths, expected_revision),
        ProjectAction::Undo { application_id } => tasks::undo(store, ws, &application_id),
        ProjectAction::Recover { application_id } => tasks::recover(store, ws, &application_id),
        ProjectAction::Reject { checkpoint_id } => tasks::reject(store, ws, &checkpoint_id),
    }?;
    let value = serde_json::to_value(record).map_err(err)?;
    store
        .connection()?
        .execute(
            "UPDATE project_operations SET response_json=?2 WHERE operation_id=?1",
            params![request.operation_id, value.to_string()],
        )
        .map_err(err)?;
    Ok(value)
}

pub(crate) fn execution_lock(store: &Store, workspace_id: &str) -> WorkbenchResult<fs::File> {
    lock(store, workspace_id)
}

/// App-owned generated inputs live under the validated project root. This creates
/// inert files only; profiles and acceptance retain their existing authorization.
pub(crate) fn program_directory(
    store: &Store,
    ws: &str,
    identity: &str,
) -> WorkbenchResult<PathBuf> {
    let _guard = lock(store, ws)?;
    if identity.len() != 64 || !identity.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(WorkbenchError::invalid(
            "Invalid generated directory identity",
        ));
    }
    let mut path = root(store, ws)?;
    for part in [".pipeline-workbench", "generated", identity] {
        path.push(part);
        if path.exists() {
            if fs::symlink_metadata(&path)
                .map_err(err)?
                .file_type()
                .is_symlink()
                || !path.is_dir()
            {
                return Err(WorkbenchError::invalid(
                    "Generated directory is not a private regular directory",
                ));
            }
        } else {
            fs::create_dir(&path).map_err(err)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).map_err(err)?;
            }
        }
    }
    Ok(path)
}

/// Read a confined accepted file for an explicit local change check. No content enters chat.
pub(crate) fn program_file_hash(
    store: &Store,
    ws: &str,
    path: &str,
) -> WorkbenchResult<Option<String>> {
    let _guard = lock(store, ws)?;
    files::relative(path)?;
    let source = files::SafeRoot::open(&root(store, ws)?)?;
    source
        .optional_read(path)
        .map(|bytes| bytes.as_deref().map(hash))
}

/// Install generated inert input without following symlinks or replacing changed material.
pub(crate) fn write_program_input(
    directory: &Path,
    path: &str,
    bytes: &[u8],
) -> WorkbenchResult<()> {
    write_program_input_with_mode(directory, path, bytes, false)
}

pub(crate) fn write_program_input_with_mode(
    directory: &Path,
    path: &str,
    bytes: &[u8],
    executable: bool,
) -> WorkbenchResult<()> {
    let root = files::SafeRoot::open(directory)?;
    if let Some(old) = root.optional_read(path)? {
        if old == bytes {
            return Ok(());
        }
        return Err(WorkbenchError::conflict(
            "Generated input changed; prepare a new operation",
        ));
    }
    let claim = id("input")?;
    root.replace(path, None, false, Some(bytes), executable, &claim)?;
    root.clear_claim(path, &claim)
}
