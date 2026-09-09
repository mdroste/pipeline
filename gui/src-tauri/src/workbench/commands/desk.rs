//! Desk Tauri commands.

use super::*;

// Research desk commands keep all app-owned storage work on the bounded gate.
#[tauri::command]
pub async fn workbench_desk_records(
    workspace_id: String,
    kind: String,
) -> WorkbenchResult<Vec<crate::workbench::desk::DeskRecord>> {
    run_store(move |s| crate::workbench::desk::records(&s, &workspace_id, &kind)).await
}
#[tauri::command]
pub async fn workbench_research_object(
    workspace_id: String,
    object: crate::workbench::desk::ResearchObjectRef,
) -> WorkbenchResult<crate::workbench::search::ResearchObject> {
    run_store(move |s| crate::workbench::search::read_object(&s, &workspace_id, &object, 64 * 1024))
        .await
}
#[tauri::command]
pub async fn workbench_context_selection(
    session_id: String,
) -> WorkbenchResult<crate::workbench::desk::ContextSelection> {
    run_store(move |s| crate::workbench::desk::context(&s, &session_id)).await
}
#[tauri::command]
pub async fn workbench_save_context_selection(
    session_id: String,
    expected_revision: i64,
    items: Vec<crate::workbench::desk::ContextItem>,
) -> WorkbenchResult<crate::workbench::desk::ContextSelection> {
    ensure_session_idle(session_id.clone()).await?;
    run_store(move |s| {
        crate::workbench::desk::save_context(&s, &session_id, expected_revision, items)
    })
    .await
}
#[tauri::command]
pub async fn workbench_reading_collection(
    request: crate::workbench::desk::SaveCollection,
) -> WorkbenchResult<crate::workbench::desk::DeskRecord> {
    run_store(move |s| crate::workbench::desk::save_collection(&s, request)).await
}
#[tauri::command]
pub async fn workbench_research_search(
    request: crate::workbench::search::SearchRequest,
) -> WorkbenchResult<crate::workbench::search::SearchPage> {
    run_store(move |s| crate::workbench::search::search(&s, request)).await
}
#[tauri::command]
pub async fn workbench_research_index(
    workspace_id: String,
    rebuild: bool,
) -> WorkbenchResult<crate::workbench::search::IndexStatus> {
    run_store(move |s| {
        if rebuild {
            crate::workbench::search::rebuild(&s, &workspace_id)?;
        }
        crate::workbench::search::advance(&s, &workspace_id)
    })
    .await
}
#[tauri::command]
pub async fn workbench_save_relation(
    workspace_id: String,
    relation: crate::workbench::project::relations::Relation,
    operation_id: String,
) -> WorkbenchResult<crate::workbench::desk::DeskRecord> {
    run_store(move |s| {
        crate::workbench::project::relations::save_relation(
            &s,
            &workspace_id,
            relation,
            &operation_id,
        )
    })
    .await
}
#[tauri::command]
pub async fn workbench_save_decision(
    workspace_id: String,
    title: String,
    decision: crate::workbench::project::relations::Decision,
    supersedes: Option<String>,
    operation_id: String,
) -> WorkbenchResult<crate::workbench::desk::DeskRecord> {
    run_store(move |s| {
        crate::workbench::project::relations::save_decision(
            &s,
            &workspace_id,
            &title,
            decision,
            supersedes.as_deref(),
            &operation_id,
        )
    })
    .await
}
#[tauri::command]
pub async fn workbench_change_impact(
    workspace_id: String,
) -> WorkbenchResult<crate::workbench::project::relations::ImpactReport> {
    run_store(move |s| crate::workbench::project::relations::impact(&s, &workspace_id)).await
}
#[tauri::command]
pub async fn workbench_session_handoff(
    session_id: String,
    operation_id: String,
) -> WorkbenchResult<crate::workbench::desk::DeskRecord> {
    run_store(move |s| {
        crate::workbench::project::relations::handoff(&s, &session_id, &operation_id)
    })
    .await
}
#[tauri::command]
pub async fn workbench_data_policy(
    workspace_id: String,
    policy: Option<crate::workbench::data::DataPolicy>,
) -> WorkbenchResult<crate::workbench::data::DataPolicy> {
    run_store(move |s| match policy {
        Some(p) => crate::workbench::data::save_policy(&s, &workspace_id, p),
        None => crate::workbench::data::policy(&s, &workspace_id),
    })
    .await
}
#[tauri::command]
pub async fn workbench_import_dataset(
    request: crate::workbench::data::ImportDataset,
) -> WorkbenchResult<crate::workbench::desk::DeskRecord> {
    run_store(move |s| crate::workbench::data::import(&s, request)).await
}
#[tauri::command]
pub async fn workbench_save_sample(
    workspace_id: String,
    title: String,
    sample: crate::workbench::data::SampleDefinition,
    supersedes: Option<String>,
    operation_id: String,
) -> WorkbenchResult<crate::workbench::desk::DeskRecord> {
    run_store(move |s| {
        crate::workbench::data::save_sample(
            &s,
            &workspace_id,
            &title,
            sample,
            supersedes.as_deref(),
            &operation_id,
        )
    })
    .await
}
#[tauri::command]
pub async fn workbench_acquisition_network(
    workspace_id: String,
    enabled: Option<bool>,
) -> WorkbenchResult<bool> {
    run_store(move |s| match enabled {
        Some(e) => crate::workbench::acquisition::set_network(&s, &workspace_id, e),
        None => crate::workbench::acquisition::network_enabled(&s, &workspace_id),
    })
    .await
}
async fn acquisition_allowed(ws: String) -> WorkbenchResult<()> {
    if !run_store(move |s| crate::workbench::acquisition::network_enabled(&s, &ws)).await? {
        return Err(WorkbenchError::invalid(
            "Enable literature and data acquisition for this Workspace first",
        ));
    }
    Ok(())
}
#[tauri::command]
pub async fn workbench_crossref_lookup(
    workspace_id: String,
    query: String,
    operation_id: String,
) -> WorkbenchResult<crate::workbench::desk::DeskRecord> {
    acquisition_allowed(workspace_id.clone()).await?;
    let result = crate::workbench::acquisition::crossref(&query).await;
    run_store(move |s| {
        crate::workbench::acquisition::record_lookup(
            &s,
            &workspace_id,
            &query,
            result,
            &operation_id,
        )
    })
    .await
}
#[tauri::command]
pub async fn workbench_acquire_candidate(
    workspace_id: String,
    receipt_id: String,
    index: usize,
    citation_key: Option<String>,
    operation_id: String,
) -> WorkbenchResult<crate::workbench::desk::DeskRecord> {
    run_store(move |s| {
        crate::workbench::acquisition::import_candidate(
            &s,
            &workspace_id,
            &receipt_id,
            index,
            citation_key,
            &operation_id,
        )
    })
    .await
}
#[tauri::command]
pub async fn workbench_acquire_pdf(
    workspace_id: String,
    title: String,
    url: String,
    operation_id: String,
) -> WorkbenchResult<crate::workbench::desk::DeskRecord> {
    acquisition_allowed(workspace_id.clone()).await?;
    let result = crate::workbench::acquisition::download_pdf(&url).await;
    run_store(move |s| {
        crate::workbench::acquisition::record_pdf(
            &s,
            &workspace_id,
            &title,
            &url,
            result,
            &operation_id,
        )
    })
    .await
}
#[tauri::command]
pub async fn workbench_acquire_fred(
    request: crate::workbench::acquisition::FredRequest,
) -> WorkbenchResult<crate::workbench::desk::DeskRecord> {
    acquisition_allowed(request.workspace_id.clone()).await?;
    let result = crate::workbench::acquisition::fred(&request).await;
    let ws = request.workspace_id;
    let series = request.series_id;
    let vintage = request.vintage;
    let operation = request.operation_id;
    run_store(move|s|match result{Ok((bytes,provenance))=>crate::workbench::data::import_bytes(&s,&ws,&format!("{series} · {vintage}"),&bytes,"csv",provenance,None,&operation),Err(e)=>crate::workbench::desk::insert(&s,&ws,"acquisition",&series,json!({"provider":"FRED/ALFRED","seriesId":series,"requestedVintage":vintage,"retrievedAt":crate::workbench::desk::now(),"state":"failed","error":e.message}),None,&operation)}).await
}

pub(crate) fn task_pending_request(thread: &str, turn: &str) -> Option<String> {
    pending_server_requests()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .find_map(|event| {
            if let crate::workbench::codex::NormalizedEvent::ServerRequest {
                method, params, ..
            } = event
            {
                if params.get("threadId").and_then(Value::as_str) == Some(thread)
                    && params.get("turnId").and_then(Value::as_str) == Some(turn)
                    && method != "item/tool/call"
                {
                    return Some(
                        params
                            .get("reason")
                            .or_else(|| params.get("questions"))
                            .map(Value::to_string)
                            .unwrap_or_else(|| method.clone()),
                    );
                }
            }
            None
        })
}
#[tauri::command]
pub async fn workbench_capture_execution_plan(
    request: crate::workbench::research::execution_plan::CapturePlanRequest,
) -> WorkbenchResult<crate::workbench::desk::DeskRecord> {
    run_store(move |s| crate::workbench::research::execution_plan::capture(&s, request)).await
}
#[tauri::command]
pub async fn workbench_execution_plan_status(
    workspace_id: String,
    plan_id: String,
) -> WorkbenchResult<crate::workbench::research::execution_plan::PlanStatus> {
    run_store(move |s| {
        crate::workbench::research::execution_plan::status(&s, &workspace_id, &plan_id)
    })
    .await
}
#[tauri::command]
pub async fn workbench_authorize_execution_plan(
    workspace_id: String,
    plan_id: String,
    fingerprint: String,
) -> WorkbenchResult<crate::workbench::research::execution_plan::PlanStatus> {
    run_store(move |s| {
        crate::workbench::research::execution_plan::authorize(
            &s,
            &workspace_id,
            &plan_id,
            &fingerprint,
        )
    })
    .await
}
#[tauri::command]
pub async fn workbench_import_dataset_metadata(
    request: crate::workbench::data::MetadataDataset,
) -> WorkbenchResult<crate::workbench::desk::DeskRecord> {
    run_store(move |s| crate::workbench::data::import_metadata(&s, request)).await
}
#[tauri::command]
pub async fn workbench_dataset_rows(
    workspace_id: String,
    dataset_id: String,
    start: usize,
) -> WorkbenchResult<crate::workbench::data::RowPreview> {
    run_store(move |s| {
        crate::workbench::data::preview_rows(&s, &workspace_id, &dataset_id, start, false)
    })
    .await
}
#[tauri::command]
pub async fn workbench_reading_inbox_state(
    workspace_id: String,
    receipt_id: String,
    state: String,
    operation_id: String,
) -> WorkbenchResult<crate::workbench::desk::DeskRecord> {
    run_store(move |s| {
        crate::workbench::acquisition::inbox_state(
            &s,
            &workspace_id,
            &receipt_id,
            &state,
            &operation_id,
        )
    })
    .await
}

pub(crate) async fn deliver_task(
    binding: crate::workbench::tasks::TaskBinding,
    operation: String,
    value: Value,
) -> Result<Value, String> {
    let _permit = turn_queue()
        .clone()
        .try_acquire_owned()
        .map_err(|_| "task_busy: Workspace is running another turn".to_string())?;
    run_store(move |store| {
        crate::workbench::tasks::validate_binding(&store, &binding)?;
        let mut result =
            crate::workbench::tasks::deliver(&store, &binding.session_id, &operation, value)?;
        let next = crate::workbench::tasks::binding(&store, &binding.session_id)?;
        result["harnessFingerprint"] = serde_json::json!(next.harness_fingerprint);
        result["cursor"] = serde_json::json!(next.cursor);
        Ok(result)
    })
    .await
    .map_err(|e| e.message)
}

#[tauri::command]
pub async fn workbench_file_reveal(
    request: crate::workbench::project::FileReadRequest,
) -> WorkbenchResult<()> {
    run_store(move |store| {
        let file = crate::workbench::project::read_workspace_file(&store, request)?;
        let path = file.external_path.ok_or_else(|| {
            WorkbenchError::invalid("Captured documents have no writable external file")
        })?;
        crate::file_viewer::reveal(std::path::Path::new(&path)).map_err(WorkbenchError::invalid)
    })
    .await
}
