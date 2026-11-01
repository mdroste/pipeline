//! Version control Tauri commands. Git runs on blocking threads outside the
//! database workers: a slow repository or remote must not hold one of them.

use super::*;
use crate::workbench::project::repository;
use crate::workbench::project::version_control::{self, Action, DiffRequest};

async fn folder(workspace_id: String) -> WorkbenchResult<Option<std::path::PathBuf>> {
    run_store(move |store| repository::repository_root(&store, &workspace_id)).await
}

async fn required_folder(workspace_id: String) -> WorkbenchResult<std::path::PathBuf> {
    folder(workspace_id)
        .await?
        .ok_or_else(|| WorkbenchError::invalid("Attach a folder to this project first"))
}

async fn run_git<T, F>(operation: F) -> WorkbenchResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> WorkbenchResult<T> + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| WorkbenchError::worker(format!("Git worker stopped: {error}"), true))?
}

/// The permit every other file change holds: an action that changes files must
/// not switch the branch or commit half a file under a running response.
fn file_turn(
    action: &Action,
    queue: Arc<Semaphore>,
) -> WorkbenchResult<Option<OwnedSemaphorePermit>> {
    if !action.changes_files() {
        return Ok(None);
    }
    queue.try_acquire_owned().map(Some).map_err(|_| {
        WorkbenchError::conflict("Stop the active research turn before changing files")
    })
}

/// These refusals clear on their own, so the record-editing advice a conflict
/// carries would mislead on this page.
fn plain(mut error: WorkbenchError) -> WorkbenchError {
    error.recovery = None;
    error
}

#[tauri::command]
pub async fn workbench_version_control_state(
    workspace_id: String,
) -> WorkbenchResult<version_control::VersionControl> {
    let root = folder(workspace_id).await?;
    run_git(move || version_control::state(root.as_deref())).await
}

#[tauri::command]
pub async fn workbench_version_control_diff(
    workspace_id: String,
    request: DiffRequest,
) -> WorkbenchResult<version_control::FileDiff> {
    let root = required_folder(workspace_id).await?;
    run_git(move || version_control::diff(&root, request)).await
}

#[tauri::command]
pub async fn workbench_version_control_history(
    workspace_id: String,
    skip: usize,
) -> WorkbenchResult<version_control::HistoryPage> {
    let root = required_folder(workspace_id).await?;
    run_git(move || version_control::history(&root, skip)).await
}

#[tauri::command]
pub async fn workbench_version_control_commit(
    workspace_id: String,
    sha: String,
) -> WorkbenchResult<version_control::CommitDetail> {
    let root = required_folder(workspace_id).await?;
    run_git(move || version_control::commit_detail(&root, &sha)).await
}

#[tauri::command]
pub async fn workbench_version_control_action(
    workspace_id: String,
    action: Action,
) -> WorkbenchResult<version_control::ActionResult> {
    let root = required_folder(workspace_id.clone()).await?;
    version_control::check_idle(&root).map_err(plain)?;
    let turn = file_turn(&action, turn_queue().clone()).map_err(plain)?;
    let project = if action.changes_files() {
        let (root, action) = (root.clone(), action.clone());
        let claimed = run_store(move |store| {
            version_control::claim_project(&store, &workspace_id, &root, &action)
        });
        Some(claimed.await.map_err(plain)?)
    } else {
        None
    };
    run_git(move || {
        // Held by the worker: a dropped await must not free them while Git runs.
        let _held = (turn, project);
        version_control::perform(&root, action)
    })
    .await
}

#[tauri::command]
pub async fn workbench_version_control_cancel(workspace_id: String) -> WorkbenchResult<()> {
    let root = required_folder(workspace_id).await?;
    version_control::cancel(&root);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(value: Value) -> Action {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_running_turn_refuses_every_action_that_changes_files() {
        let queue = Arc::new(Semaphore::new(1));
        let _turn = queue.clone().try_acquire_owned().unwrap();
        for changing in [
            json!({ "kind": "initialize" }),
            json!({ "kind": "commit", "paths": ["a"], "summary": "s", "description": null }),
            json!({ "kind": "discard", "paths": ["a"] }),
            json!({ "kind": "ignore", "path": "a" }),
            json!({ "kind": "undoCommit" }),
            json!({ "kind": "switchBranch", "name": "a" }),
            json!({ "kind": "createBranch", "name": "a" }),
            json!({ "kind": "pull" }),
            json!({ "kind": "resolve", "path": "a", "choice": "theirs" }),
            json!({ "kind": "abortOperation" }),
        ] {
            let refused = file_turn(&action(changing.clone()), queue.clone())
                .err()
                .unwrap_or_else(|| panic!("{changing} ran under a turn"));
            assert_eq!(refused.code, "storage_conflict");
            assert!(refused.message.starts_with("Stop the active research turn"));
            assert_eq!(plain(refused).recovery, None);
        }
        for leaving_files_alone in [
            json!({ "kind": "fetch" }),
            json!({ "kind": "push" }),
            json!({ "kind": "setRemote", "url": "x" }),
            json!({ "kind": "setIdentity", "name": "a", "email": "b" }),
        ] {
            assert!(file_turn(&action(leaving_files_alone), queue.clone())
                .unwrap()
                .is_none());
        }
    }

    #[test]
    fn a_change_holds_the_turn_for_as_long_as_it_runs() {
        let queue = Arc::new(Semaphore::new(1));
        let held = file_turn(&action(json!({ "kind": "pull" })), queue.clone()).unwrap();
        assert!(held.is_some());
        assert_eq!(queue.available_permits(), 0);
        drop(held);
        assert_eq!(queue.available_permits(), 1);
    }
}
