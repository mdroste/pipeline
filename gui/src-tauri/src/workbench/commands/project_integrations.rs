//! Network adapters run outside the bounded database workers.
use super::*;
use crate::workbench::project;
use crate::workbench::{
    desk,
    project::{github, repository},
};

#[tauri::command]
pub async fn workbench_check_policy(
    workspace_id: String,
    expected_revision: i64,
    policy: project::checks::CheckPolicy,
) -> WorkbenchResult<project::ProjectRecord> {
    run_store(move |store| {
        project::checks::save_policy(&store, &workspace_id, expected_revision, policy)
    })
    .await
}
#[tauri::command]
pub async fn workbench_check_run(
    workspace_id: String,
) -> WorkbenchResult<Option<project::ProjectRecord>> {
    run_store(move |store| project::checks::run(&store, &workspace_id, None)).await
}

pub(super) fn schedule_project_check(app: tauri::AppHandle, workspace_id: String, path: String) {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock};
    use tauri::Emitter;
    type Pending = BTreeMap<String, (u64, BTreeSet<String>)>;
    static PENDING: OnceLock<Mutex<Pending>> = OnceLock::new();
    static GENERATION: AtomicU64 = AtomicU64::new(1);
    let pending = PENDING.get_or_init(|| Mutex::new(BTreeMap::new()));
    let generation = {
        let Ok(mut map) = pending.lock() else {
            return;
        };
        let entry = map.entry(workspace_id.clone()).or_default();
        entry.0 = GENERATION.fetch_add(1, Ordering::Relaxed);
        entry.1.insert(path);
        entry.0
    };
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let paths = {
            let Ok(mut map) = pending.lock() else {
                return;
            };
            if map.get(&workspace_id).map(|entry| entry.0) != Some(generation) {
                return;
            }
            map.remove(&workspace_id)
                .unwrap()
                .1
                .into_iter()
                .collect::<Vec<_>>()
        };
        let ws = workspace_id.clone();
        match run_store(move |store| project::checks::run(&store, &ws, Some(&paths))).await {
            Ok(Some(_)) => {
                let _ = app.emit(
                    "project-check-completed",
                    serde_json::json!({"workspaceId":workspace_id}),
                );
            }
            Ok(None) => (),
            Err(error) => {
                let _ = app.emit(
                    "project-check-completed",
                    serde_json::json!({"workspaceId":workspace_id,"error":error.message}),
                );
            }
        }
    });
}

#[tauri::command]
pub async fn workbench_submission_capture(
    workspace_id: String,
    request: project::submissions::SubmissionRequest,
) -> WorkbenchResult<desk::DeskRecord> {
    run_store(move |store| project::submissions::capture(&store, &workspace_id, request)).await
}
#[tauri::command]
pub async fn workbench_submission_export(
    workspace_id: String,
    id: String,
    expected_hash: String,
    path: String,
) -> WorkbenchResult<serde_json::Value> {
    run_store(move |store| {
        project::submissions::export(&store, &workspace_id, &id, &expected_hash, &path)
    })
    .await
}

#[tauri::command]
pub async fn workbench_citations(
    workspace_id: String,
    request: project::citations::CitationRequest,
) -> WorkbenchResult<serde_json::Value> {
    run_store(move |store| project::citations::inspect(&store, &workspace_id, request)).await
}
#[tauri::command]
pub async fn workbench_zotero_preview(
    collection: Option<String>,
    start: usize,
    expected_server: Option<String>,
) -> WorkbenchResult<project::ZoteroPreview> {
    project::zotero_preview(collection, start, expected_server).await
}
#[tauri::command]
pub async fn workbench_zotero_import(
    workspace_id: String,
    preview: project::ZoteroPreview,
    selected: Vec<String>,
) -> WorkbenchResult<serde_json::Value> {
    run_store(move |store| project::import_zotero(&store, &workspace_id, preview, &selected)).await
}

#[tauri::command]
pub async fn workbench_github_inspect(workspace_id: String) -> WorkbenchResult<desk::DeskRecord> {
    let ws = workspace_id.clone();
    let root = run_store(move |store| repository::repository_root(&store, &ws))
        .await?
        .ok_or_else(|| WorkbenchError::invalid("Attach a GitHub project folder"))?;
    let body = tokio::task::spawn_blocking(move || github::inspect(&root))
        .await
        .map_err(|error| {
            WorkbenchError::worker(format!("GitHub worker stopped: {error}"), true)
        })??;
    run_store(move |store| {
        let operation = format!("github-{}", desk::hash(body.to_string().as_bytes()));
        desk::insert(
            &store,
            &workspace_id,
            "github_collaboration",
            "GitHub collaboration snapshot",
            body,
            None,
            &operation,
        )
    })
    .await
}

#[tauri::command]
pub async fn workbench_github_draft(
    workspace_id: String,
    request: github::GitHubRequest,
) -> WorkbenchResult<String> {
    let root = run_store(move |store| repository::repository_root(&store, &workspace_id))
        .await?
        .ok_or_else(|| WorkbenchError::invalid("Attach a GitHub project folder"))?;
    tokio::task::spawn_blocking(move || github::draft_pull_request(&root, request))
        .await
        .map_err(|error| WorkbenchError::worker(format!("GitHub worker stopped: {error}"), true))?
}

#[tauri::command]
pub async fn workbench_github_comment(
    workspace_id: String,
    snapshot_id: String,
    comment_id: String,
) -> WorkbenchResult<crate::workbench::project::ProjectRecord> {
    run_store(move |store| github::import_comment(&store, &workspace_id, &snapshot_id, &comment_id))
        .await
}
