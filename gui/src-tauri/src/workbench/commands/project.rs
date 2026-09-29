//! Project Tauri commands.

use super::*;

#[tauri::command]
pub async fn workbench_project_index(
) -> WorkbenchResult<Vec<crate::workbench::project::ProjectIndexItem>> {
    run_store(move |store| crate::workbench::project::project_index(&store)).await
}

#[tauri::command]
pub async fn workbench_project_home(
    workspace_id: String,
) -> WorkbenchResult<crate::workbench::project::ProjectHome> {
    run_store(move |store| crate::workbench::project::home(&store, &workspace_id)).await
}
#[tauri::command]
pub async fn workbench_project_tasks(
    workspace_id: String,
    before: Option<crate::workbench::project::TaskCursor>,
) -> WorkbenchResult<crate::workbench::project::TaskPage> {
    run_store(move |store| {
        crate::workbench::project::task_page(&store, &workspace_id, before.as_ref())
    })
    .await
}
#[tauri::command]
pub async fn workbench_project_mutate(
    request: crate::workbench::project::ProjectMutation,
) -> WorkbenchResult<Value> {
    // Hold the same permit as native turns across checkpoint and acceptance I/O.
    // Checking active state alone would race a turn starting immediately afterward.
    let _turn_guard = if matches!(
        request.action,
        crate::workbench::project::ProjectAction::Checkpoint { .. }
            | crate::workbench::project::ProjectAction::CaptureChanges { .. }
            | crate::workbench::project::ProjectAction::Apply { .. }
            | crate::workbench::project::ProjectAction::Undo { .. }
            | crate::workbench::project::ProjectAction::Recover { .. }
            | crate::workbench::project::ProjectAction::Reject { .. }
    ) {
        Some(turn_queue().clone().try_acquire_owned().map_err(|_| WorkbenchError::conflict("Wait for the active Workspace turn to finish or stop it before changing a task's files"))?)
    } else {
        None
    };
    run_store(move |store| crate::workbench::project::mutate(&store, request)).await
}
#[tauri::command]
pub async fn workbench_document_read(
    workspace_id: String,
    revision_id: String,
    start: usize,
    page: Option<u32>,
) -> WorkbenchResult<crate::workbench::project::DocumentView> {
    run_store(move |store| {
        crate::workbench::project::read_document(&store, &workspace_id, &revision_id, start, page)
    })
    .await
}
#[tauri::command]
pub async fn workbench_anchor_mapping(
    workspace_id: String,
    anchor_id: String,
    revision_id: String,
) -> WorkbenchResult<crate::workbench::project::AnchorMapping> {
    run_store(move |store| {
        crate::workbench::project::map_anchor(&store, &workspace_id, &anchor_id, &revision_id)
    })
    .await
}
#[tauri::command]
pub async fn workbench_file_read(
    request: crate::workbench::project::FileReadRequest,
) -> WorkbenchResult<crate::workbench::project::FilePreview> {
    run_store(move |store| crate::workbench::project::read_workspace_file(&store, request)).await
}

#[tauri::command]
pub async fn workbench_conversation_file_open(
    app: tauri::AppHandle,
    session_id: String,
    path: String,
) -> WorkbenchResult<()> {
    let path = run_store(move |store| {
        crate::workbench::project::snapshot_conversation_file(&store, &session_id, &path)
    })
    .await?;
    #[allow(deprecated)]
    app.shell()
        .open(path.to_string_lossy().into_owned(), None)
        .map_err(|error| WorkbenchError::storage("Failed to open conversation file", error))
}

#[tauri::command]
pub async fn workbench_conversation_file_read(
    session_id: String,
    path: String,
) -> WorkbenchResult<crate::workbench::project::FilePreview> {
    run_store(move |store| {
        crate::workbench::project::read_conversation_file(&store, &session_id, &path)
    })
    .await
}
#[tauri::command]
pub async fn workbench_snapshot_preview(
    workspace_id: String,
    hash: String,
) -> WorkbenchResult<crate::workbench::project::SnapshotPreview> {
    run_store(move |store| {
        crate::workbench::project::preview_snapshot(&store, &workspace_id, &hash)
    })
    .await
}
#[tauri::command]
pub async fn workbench_task_session(
    workspace_id: String,
    checkpoint_id: String,
) -> WorkbenchResult<WorkbenchSession> {
    run_store(move |store| {
        crate::workbench::project::task_session(&store, &workspace_id, &checkpoint_id)
    })
    .await
}

#[tauri::command]
pub async fn workbench_preview_host_execution(
    profile_id: String,
) -> WorkbenchResult<crate::workbench::research::HostExecutionPreview> {
    run_store(move |store| crate::workbench::research::preview_host_execution(&store, &profile_id))
        .await
}
#[tauri::command]
pub async fn workbench_authorize_host_execution(
    profile_id: String,
    fingerprint: String,
) -> WorkbenchResult<crate::workbench::research::HostExecutionPreview> {
    run_store(move |store| {
        crate::workbench::research::authorize_host_execution(&store, &profile_id, &fingerprint)
    })
    .await
}

#[tauri::command]
pub fn workbench_project_capabilities() -> Value {
    crate::workbench::project::capabilities()
}

#[tauri::command]
pub async fn workbench_working_file_preview(
    workspace_id: String,
    path: String,
) -> WorkbenchResult<crate::workbench::project::SnapshotPreview> {
    run_store(move |store| {
        crate::workbench::project::preview_working_file(&store, &workspace_id, &path)
    })
    .await
}

#[tauri::command]
pub async fn workbench_list_jobs(
    workspace_id: String,
) -> WorkbenchResult<Vec<crate::workbench::research::jobs::JobStatus>> {
    run_store(move |store| crate::workbench::research::jobs::list_jobs(&store, &workspace_id)).await
}
#[tauri::command]
pub async fn workbench_job_log(
    workspace_id: String,
    execution_id: String,
    stream: String,
    offset: usize,
) -> WorkbenchResult<crate::workbench::research::jobs::JobLogPage> {
    run_store(move |store| {
        crate::workbench::research::jobs::log_page(
            &store,
            &workspace_id,
            &execution_id,
            &stream,
            offset,
        )
    })
    .await
}
#[tauri::command]
pub async fn workbench_reconcile_job(
    workspace_id: String,
    execution_id: String,
) -> WorkbenchResult<crate::workbench::research::ResearchExecution> {
    run_store(move |store| {
        crate::workbench::research::jobs::reconcile_job(&store, &workspace_id, &execution_id)
    })
    .await
}

#[tauri::command]
pub async fn workbench_studio_mutate(
    request: crate::workbench::project::StudioMutation,
) -> WorkbenchResult<Value> {
    let _turn_guard = if matches!(
        request.action,
        crate::workbench::project::StudioAction::SaveText { .. }
            | crate::workbench::project::StudioAction::GenerateValues { .. }
    ) {
        Some(turn_queue().clone().try_acquire_owned().map_err(|_| {
            WorkbenchError::conflict("Stop the active research turn before saving files")
        })?)
    } else {
        None
    };
    run_store(move |store| crate::workbench::project::studio_mutate(&store, request)).await
}
#[tauri::command]
pub async fn workbench_studio_records(
    workspace_id: String,
    kind: String,
) -> WorkbenchResult<Vec<crate::workbench::project::ProjectRecord>> {
    run_store(move |store| crate::workbench::project::studio_records(&store, &workspace_id, &kind))
        .await
}
#[tauri::command]
pub async fn workbench_theory_overview(
    workspace_id: String,
) -> WorkbenchResult<crate::workbench::project::TheoryOverview> {
    run_store(move |store| crate::workbench::project::theory_overview(&store, &workspace_id)).await
}
#[tauri::command]
pub async fn workbench_studio_history(
    workspace_id: String,
    object_id: String,
) -> WorkbenchResult<Vec<Value>> {
    run_store(move |store| {
        crate::workbench::project::studio_history(&store, &workspace_id, &object_id)
    })
    .await
}
#[tauri::command]
pub async fn workbench_editor_file(
    workspace_id: String,
    checkpoint_id: Option<String>,
    path: String,
) -> WorkbenchResult<crate::workbench::project::EditorFile> {
    run_store(move |store| {
        crate::workbench::project::editor_file(
            &store,
            &workspace_id,
            checkpoint_id.as_deref(),
            &path,
        )
    })
    .await
}
#[tauri::command]
pub async fn workbench_editor_external(
    app: tauri::AppHandle,
    workspace_id: String,
    checkpoint_id: Option<String>,
    path: String,
) -> WorkbenchResult<()> {
    let path = run_store(move |store| {
        crate::workbench::project::editor_external_path(
            &store,
            &workspace_id,
            checkpoint_id.as_deref(),
            &path,
        )
    })
    .await?;
    #[allow(deprecated)]
    app.shell()
        .open(path, None)
        .map_err(|e| WorkbenchError::storage("Failed to open external editor", e))
}
#[tauri::command]
pub async fn workbench_build_sync(
    request: crate::workbench::project::SyncRequest,
) -> WorkbenchResult<Value> {
    run_store(move |store| crate::workbench::project::synchronize(&store, request)).await
}
#[tauri::command]
pub async fn workbench_workflow_finding_preview(run_id: String) -> WorkbenchResult<Value> {
    let report = crate::commands::get_run_report(run_id.clone())
        .await
        .map_err(WorkbenchError::invalid)?;
    let findings = crate::findings::canonical_findings(&report).ok_or_else(|| {
        WorkbenchError::invalid("The selected Workflow run has no canonical findings")
    })?;
    crate::workbench::project::preview_findings(&crate::workbench::project::FindingPackage {
        version: 1,
        run_id,
        source_step_id: findings.source_step_id,
        findings: findings.findings,
    })
}
#[tauri::command]
pub async fn workbench_finding_package_preview(
    package: crate::workbench::project::FindingPackage,
) -> WorkbenchResult<Value> {
    crate::workbench::project::preview_findings(&package)
}
#[tauri::command]
pub async fn workbench_report_preview(
    workspace_id: String,
    revision_id: String,
) -> WorkbenchResult<Vec<crate::workbench::project::ReportComment>> {
    run_store(move |store| {
        crate::workbench::project::preview_report(&store, &workspace_id, &revision_id)
    })
    .await
}
#[tauri::command]
pub async fn workbench_response_export(
    workspace_id: String,
    selected: Vec<String>,
    format: String,
) -> WorkbenchResult<Value> {
    run_store(move |store| {
        crate::workbench::project::export_responses(&store, &workspace_id, &selected, &format)
    })
    .await
}
#[tauri::command]
pub async fn workbench_focus_review(
    request: crate::workbench::project::FocusReviewRequest,
) -> WorkbenchResult<crate::workbench::release::ReviewHandoff> {
    run_store(move |store| crate::workbench::project::focus_review(&store, request)).await
}
#[tauri::command]
pub async fn workbench_compare_experiment(
    request: crate::workbench::project::CompareExperimentRequest,
) -> WorkbenchResult<Value> {
    run_store(move |store| crate::workbench::project::compare_experiment(&store, request)).await
}
#[tauri::command]
pub async fn workbench_compare_series(
    workspace_id: String,
    left_id: String,
    right_id: String,
    rationale: String,
) -> WorkbenchResult<Value> {
    run_store(move |store| {
        crate::workbench::project::compare_series(
            &store,
            &workspace_id,
            &left_id,
            &right_id,
            &rationale,
        )
    })
    .await
}
#[tauri::command]
pub async fn workbench_binding_coverage(workspace_id: String) -> WorkbenchResult<Vec<Value>> {
    run_store(move |store| crate::workbench::project::binding_coverage(&store, &workspace_id)).await
}
#[tauri::command]
pub async fn workbench_bibliography_preview(
    workspace_id: String,
    revision_id: String,
) -> WorkbenchResult<Value> {
    run_store(move |store| {
        crate::workbench::project::preview_bibliography(&store, &workspace_id, &revision_id)
    })
    .await
}
#[tauri::command]
pub async fn workbench_citation_navigation(
    workspace_id: String,
    revision_id: String,
) -> WorkbenchResult<Value> {
    run_store(move |store| {
        crate::workbench::project::citation_navigation(&store, &workspace_id, &revision_id)
    })
    .await
}
#[tauri::command]
pub async fn workbench_source_passage(
    workspace_id: String,
    version_id: String,
    start: usize,
    length: usize,
) -> WorkbenchResult<crate::workbench::research::SourceReadResult> {
    run_store(move |store| {
        crate::workbench::research::source_read(
            &store,
            &workspace_id,
            &version_id,
            start,
            length.min(64 * 1024),
        )
    })
    .await
}
#[tauri::command]
pub async fn workbench_zotero_preview(
    collection: Option<String>,
    start: usize,
    expected_server: Option<String>,
) -> WorkbenchResult<crate::workbench::project::ZoteroPreview> {
    crate::workbench::project::zotero_preview(collection, start, expected_server).await
}
#[tauri::command]
pub async fn workbench_zotero_import(
    workspace_id: String,
    preview: crate::workbench::project::ZoteroPreview,
    selected: Vec<String>,
) -> WorkbenchResult<Value> {
    run_store(move |store| {
        crate::workbench::project::import_zotero(&store, &workspace_id, preview, &selected)
    })
    .await
}

#[tauri::command]
pub async fn workbench_studio_export_file(path: String, content: String) -> WorkbenchResult<()> {
    run_store(move |_store| {
        let path = std::path::PathBuf::from(path);
        if !path.is_absolute()
            || !matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("md" | "tex" | "json")
            )
            || content.len() > 8 * 1024 * 1024
        {
            return Err(WorkbenchError::invalid(
                "Choose an absolute Markdown, TeX or JSON export path (up to 8 MiB)",
            ));
        }
        std::fs::write(&path, content)
            .map_err(|e| WorkbenchError::storage("Failed to export research document", e))?;
        std::fs::File::open(path)
            .and_then(|f| f.sync_all())
            .map_err(|e| WorkbenchError::storage("Failed to sync research export", e))
    })
    .await
}
