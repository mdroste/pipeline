//! Research Tauri commands.

use super::*;

#[tauri::command]
pub async fn workbench_harness_catalog(
    workspace_id: Option<String>,
) -> WorkbenchResult<crate::workbench::research::HarnessCatalog> {
    run_store(move |store| {
        crate::workbench::research::harness_catalog(&store, workspace_id.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn workbench_native_prompt_catalog(
) -> WorkbenchResult<crate::workbench::codex::native_prompts::NativePromptCatalog> {
    let home = run_store(|store| Ok(store.codex_home_path())).await?;
    crate::workbench::codex::native_prompts::catalog(home)
        .await
        .map_err(WorkbenchError::invalid)
}

#[tauri::command]
pub async fn workbench_effective_harness(
    session_id: String,
) -> WorkbenchResult<crate::workbench::research::EffectiveHarness> {
    run_store(move |store| crate::workbench::research::resolve_harness(&store, &session_id)).await
}

#[tauri::command]
pub async fn workbench_get_workspace_config(
    workspace_id: Option<String>,
) -> WorkbenchResult<crate::workbench::research::WorkspaceConfig> {
    run_store(move |store| crate::workbench::research::load_config(&store, workspace_id.as_deref()))
        .await
}

#[tauri::command]
pub async fn workbench_save_workspace_config(
    request: crate::workbench::research::SaveWorkspaceConfigRequest,
) -> WorkbenchResult<crate::workbench::research::WorkspaceConfig> {
    run_store(move |store| crate::workbench::research::save_config(&store, request)).await
}

#[tauri::command]
pub async fn workbench_clone_preset(
    request: crate::workbench::research::ClonePresetRequest,
) -> WorkbenchResult<crate::workbench::research::HarnessPreset> {
    run_store(move |store| crate::workbench::research::clone_preset(&store, request)).await
}

#[tauri::command]
pub async fn workbench_update_preset(
    request: crate::workbench::research::UpdatePresetRequest,
) -> WorkbenchResult<crate::workbench::research::HarnessPreset> {
    run_store(move |store| crate::workbench::research::update_preset(&store, request)).await
}

#[tauri::command]
pub async fn workbench_create_note(
    request: crate::workbench::research::CreateNoteRequest,
) -> WorkbenchResult<crate::workbench::research::ResearchNote> {
    run_store(move |store| crate::workbench::research::create_note(&store, request, false)).await
}

#[tauri::command]
pub async fn workbench_update_note(
    request: crate::workbench::research::UpdateNoteRequest,
) -> WorkbenchResult<crate::workbench::research::ResearchNote> {
    run_store(move |store| crate::workbench::research::update_note(&store, request)).await
}

#[tauri::command]
pub async fn workbench_list_notes(
    workspace_id: String,
    include_rejected: bool,
) -> WorkbenchResult<Vec<crate::workbench::research::ResearchNote>> {
    run_store(move |store| {
        crate::workbench::research::list_notes(&store, &workspace_id, include_rejected)
    })
    .await
}

#[tauri::command]
pub async fn workbench_import_paper(
    request: crate::workbench::research::ImportPaperRequest,
) -> WorkbenchResult<crate::workbench::research::PaperWithRevision> {
    run_store(move |store| crate::workbench::research::import_paper(&store, request)).await
}

#[tauri::command]
pub async fn workbench_list_papers(
    workspace_id: String,
) -> WorkbenchResult<Vec<crate::workbench::research::PaperWithRevision>> {
    run_store(move |store| crate::workbench::research::list_papers(&store, &workspace_id)).await
}

#[tauri::command]
pub async fn workbench_paper_read(
    request: crate::workbench::research::PaperReadRequest,
) -> WorkbenchResult<crate::workbench::research::PaperReadResult> {
    run_store(move |store| crate::workbench::research::paper_read(&store, request)).await
}

#[tauri::command]
pub async fn workbench_paper_search(
    workspace_id: String,
    revision_id: String,
    query: String,
    limit: usize,
) -> WorkbenchResult<Vec<crate::workbench::research::PaperSearchHit>> {
    run_store(move |store| {
        crate::workbench::research::paper_search(&store, &workspace_id, &revision_id, &query, limit)
    })
    .await
}

#[tauri::command]
pub async fn workbench_import_source(
    request: crate::workbench::research::ImportSourceRequest,
) -> WorkbenchResult<crate::workbench::research::SourceImportResult> {
    run_store(move |store| crate::workbench::research::import_source(&store, request)).await
}

#[tauri::command]
pub async fn workbench_list_sources(
    workspace_id: String,
) -> WorkbenchResult<Vec<crate::workbench::research::SourceRecord>> {
    run_store(move |store| crate::workbench::research::list_sources(&store, &workspace_id)).await
}

#[tauri::command]
pub async fn workbench_propose_claim(
    request: crate::workbench::research::ProposeClaimRequest,
) -> WorkbenchResult<crate::workbench::research::ClaimRecord> {
    run_store(move |store| crate::workbench::research::propose_claim(&store, request, false)).await
}

#[tauri::command]
pub async fn workbench_set_claim_state(
    request: crate::workbench::research::SetClaimStateRequest,
) -> WorkbenchResult<crate::workbench::research::ClaimRecord> {
    run_store(move |store| crate::workbench::research::set_claim_state(&store, request)).await
}

#[tauri::command]
pub async fn workbench_propose_evidence(
    request: crate::workbench::research::ProposeEvidenceRequest,
) -> WorkbenchResult<crate::workbench::research::EvidenceRecord> {
    run_store(move |store| crate::workbench::research::propose_evidence(&store, request, false))
        .await
}

#[tauri::command]
pub async fn workbench_confirm_evidence(
    request: crate::workbench::research::ConfirmEvidenceRequest,
) -> WorkbenchResult<crate::workbench::research::EvidenceRecord> {
    run_store(move |store| crate::workbench::research::confirm_evidence(&store, request)).await
}

#[tauri::command]
pub async fn workbench_research_ledger(
    workspace_id: String,
) -> WorkbenchResult<crate::workbench::research::ResearchLedger> {
    run_store(move |store| crate::workbench::research::research_ledger(&store, &workspace_id)).await
}

#[tauri::command]
pub async fn workbench_save_execution_profile(
    request: crate::workbench::research::SaveExecutionProfileRequest,
) -> WorkbenchResult<crate::workbench::research::ExecutionProfile> {
    run_store(move |store| crate::workbench::research::save_execution_profile(&store, request))
        .await
}

#[tauri::command]
pub async fn workbench_list_execution_profiles(
    workspace_id: String,
) -> WorkbenchResult<Vec<crate::workbench::research::ExecutionProfile>> {
    run_store(move |store| {
        crate::workbench::research::list_execution_profiles(&store, &workspace_id)
    })
    .await
}

#[tauri::command]
pub async fn workbench_run_execution(
    request: crate::workbench::research::RunExecutionRequest,
) -> WorkbenchResult<crate::workbench::research::ResearchExecution> {
    let _turn_guard = turn_queue().clone().try_acquire_owned().map_err(|_| {
        WorkbenchError::conflict(
            "Wait for the active research turn before starting a detached local job",
        )
    })?;
    let receipt = run_store(move |store| {
        crate::workbench::research::jobs::queue(&store, request, "detached", None)
    })
    .await?;
    crate::workbench::research::jobs::launch_pending();
    Ok(receipt)
}

#[tauri::command]
pub async fn workbench_list_executions(
    workspace_id: String,
) -> WorkbenchResult<Vec<crate::workbench::research::ResearchExecution>> {
    run_store(move |store| crate::workbench::research::list_executions(&store, &workspace_id)).await
}

#[tauri::command]
pub async fn workbench_cancel_execution(
    request: crate::workbench::research::CancelExecutionRequest,
) -> WorkbenchResult<()> {
    run_store(move |store| crate::workbench::research::cancel_execution(&store, request)).await
}

#[tauri::command]
pub async fn workbench_compare_results(
    request: crate::workbench::research::CompareResultsRequest,
) -> WorkbenchResult<crate::workbench::research::ResultComparison> {
    run_store(move |_store| crate::workbench::research::compare_results(request)).await
}

#[tauri::command]
pub async fn workbench_record_structured_result(
    request: crate::workbench::research::RecordStructuredResultRequest,
) -> WorkbenchResult<crate::workbench::research::ResearchResultV1> {
    run_store(move |store| crate::workbench::research::record_structured_result(&store, request))
        .await
}

#[tauri::command]
pub async fn workbench_list_structured_results(
    workspace_id: String,
) -> WorkbenchResult<Vec<crate::workbench::research::ResearchResultV1>> {
    run_store(move |store| {
        crate::workbench::research::list_structured_results(&store, &workspace_id)
    })
    .await
}

#[tauri::command]
pub async fn workbench_verify_evidence_results(
    request: crate::workbench::research::VerifyEvidenceResultsRequest,
) -> WorkbenchResult<crate::workbench::research::VerificationRecord> {
    run_store(move |store| crate::workbench::research::verify_evidence_results(&store, request))
        .await
}

#[tauri::command]
pub async fn workbench_list_recipes(
    workspace_id: Option<String>,
) -> WorkbenchResult<Vec<crate::workbench::release::ResearchRecipe>> {
    run_store(move |store| crate::workbench::release::list_recipes(&store, workspace_id.as_deref()))
        .await
}

#[tauri::command]
pub async fn workbench_clone_recipe(
    request: crate::workbench::release::CloneRecipeRequest,
) -> WorkbenchResult<crate::workbench::release::ResearchRecipe> {
    run_store(move |store| crate::workbench::release::clone_recipe(&store, request)).await
}

#[tauri::command]
pub async fn workbench_update_recipe(
    request: crate::workbench::release::UpdateRecipeRequest,
) -> WorkbenchResult<crate::workbench::release::ResearchRecipe> {
    run_store(move |store| crate::workbench::release::update_recipe(&store, request)).await
}

#[tauri::command]
pub async fn workbench_check_recipe_inputs(
    session_id: String,
    recipe_id: String,
) -> WorkbenchResult<Vec<crate::workbench::release::InputCheck>> {
    run_store(move |store| {
        crate::workbench::release::check_recipe_inputs(&store, &session_id, &recipe_id)
    })
    .await
}

#[tauri::command]
pub async fn workbench_start_recipe(
    request: crate::workbench::release::StartRecipeRequest,
) -> WorkbenchResult<crate::workbench::release::RecipeRun> {
    run_store(move |store| crate::workbench::release::start_recipe(&store, request)).await
}

#[tauri::command]
pub async fn workbench_complete_recipe(
    request: crate::workbench::release::CompleteRecipeRequest,
) -> WorkbenchResult<crate::workbench::release::RecipeRun> {
    run_store(move |store| crate::workbench::release::complete_recipe(&store, request)).await
}

#[tauri::command]
pub async fn workbench_list_recipe_runs(
    session_id: String,
) -> WorkbenchResult<Vec<crate::workbench::release::RecipeRun>> {
    run_store(move |store| crate::workbench::release::list_recipe_runs(&store, &session_id)).await
}

#[tauri::command]
pub async fn workbench_performance_budgets() -> Vec<crate::workbench::release::PerformanceBudget> {
    crate::workbench::release::performance_budgets()
}

#[tauri::command]
pub async fn workbench_record_performance(
    request: crate::workbench::release::RecordPerformanceRequest,
) -> WorkbenchResult<Value> {
    run_store(move |store| crate::workbench::release::record_performance(&store, request)).await
}

#[tauri::command]
pub async fn workbench_record_research_evaluation(
    request: crate::workbench::release::RecordEvaluationRequest,
) -> WorkbenchResult<crate::workbench::release::EvaluationRecord> {
    run_store(move |store| crate::workbench::release::record_evaluation(&store, request)).await
}

#[tauri::command]
pub async fn workbench_list_research_evaluations(
    fixture_id: Option<String>,
) -> WorkbenchResult<Vec<crate::workbench::release::EvaluationRecord>> {
    run_store(move |store| {
        crate::workbench::release::list_evaluations(&store, fixture_id.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn workbench_prepare_review_handoff(
    request: crate::workbench::release::PrepareReviewHandoffRequest,
) -> WorkbenchResult<crate::workbench::release::ReviewHandoff> {
    run_store(move |store| crate::workbench::release::prepare_review_handoff(&store, request)).await
}

#[tauri::command]
pub async fn workbench_link_review_handoff(
    request: crate::workbench::release::LinkReviewHandoffRequest,
) -> WorkbenchResult<crate::workbench::release::ReviewHandoff> {
    run_store(move |store| crate::workbench::release::link_review_handoff(&store, request)).await
}

#[tauri::command]
pub async fn workbench_export_research_archive(
    request: crate::workbench::release::ExportArchiveRequest,
) -> WorkbenchResult<crate::workbench::release::ArchiveReport> {
    run_store_exclusive(move |store| {
        if crate::workbench::research::jobs::has_active() {
            return Err(WorkbenchError::invalid(
                "Stop local jobs before exporting a consistent archive",
            ));
        }
        crate::workbench::release::refresh_retained_blobs(&store)?;
        crate::workbench::release::export_archive(&store, request)
    })
    .await
}

#[tauri::command]
pub async fn workbench_preview_project_exchange(
    workspace_id: String,
    selection: crate::workbench::release::ExchangeSelection,
) -> WorkbenchResult<crate::workbench::release::ExchangePreview> {
    run_store(move |store| {
        crate::workbench::release::preview_exchange(&store, &workspace_id, &selection)
    })
    .await
}
#[tauri::command]
pub async fn workbench_export_project_exchange(
    request: crate::workbench::release::ExportExchangeRequest,
) -> WorkbenchResult<crate::workbench::release::ExchangeReport> {
    run_store(move |store| crate::workbench::release::export_exchange(&store, request)).await
}
#[tauri::command]
pub async fn workbench_inspect_project_exchange(
    path: String,
) -> WorkbenchResult<crate::workbench::release::ExchangeInspection> {
    run_store(move |store| crate::workbench::release::inspect_exchange(&store, &path)).await
}
#[tauri::command]
pub async fn workbench_preview_project_exchange_import(
    path: String,
    target: crate::workbench::release::ExchangeTarget,
) -> WorkbenchResult<crate::workbench::release::ImportPreview> {
    run_store(move |store| crate::workbench::release::preview_import(&store, &path, &target)).await
}
#[tauri::command]
pub async fn workbench_import_project_exchange(
    request: crate::workbench::release::ImportExchangeRequest,
) -> WorkbenchResult<crate::workbench::release::ImportReport> {
    run_store(move |store| crate::workbench::release::import_exchange(&store, request)).await
}
#[tauri::command]
pub async fn workbench_list_exchange_conflicts(
    workspace_id: String,
) -> WorkbenchResult<Vec<crate::workbench::release::ExchangeConflict>> {
    run_store(move |store| crate::workbench::release::list_conflicts(&store, &workspace_id)).await
}
#[tauri::command]
pub async fn workbench_resolve_exchange_conflict(
    workspace_id: String,
    conflict_id: String,
    take_imported: bool,
) -> WorkbenchResult<crate::workbench::release::ExchangeConflict> {
    run_store(move |store| {
        crate::workbench::release::resolve_conflict(
            &store,
            &workspace_id,
            &conflict_id,
            take_imported,
        )
    })
    .await
}
#[tauri::command]
pub async fn workbench_storage_report() -> WorkbenchResult<crate::workbench::release::StorageReport>
{
    run_store_exclusive(move |store| crate::workbench::release::storage_report(&store)).await
}
#[tauri::command]
pub async fn workbench_prune_storage(
    request: crate::workbench::release::PruneRequest,
) -> WorkbenchResult<crate::workbench::release::PrunePlan> {
    if request.apply {
        run_storage_maintenance(move |store| {
            crate::workbench::release::prune_storage(&store, request)
        })
        .await
    } else {
        run_store(move |store| crate::workbench::release::prune_storage(&store, request)).await
    }
}
#[tauri::command]
pub async fn workbench_restore_trash(
    trash_id: String,
) -> WorkbenchResult<crate::workbench::release::TrashEntry> {
    run_storage_maintenance(move |store| {
        crate::workbench::release::restore_trash(&store, &trash_id)
    })
    .await
}
#[tauri::command]
pub async fn workbench_empty_trash() -> WorkbenchResult<usize> {
    run_storage_maintenance(move |store| crate::workbench::release::empty_trash(&store)).await
}
#[tauri::command]
pub async fn workbench_draft_workflow(
    request: crate::workbench::release::DraftWorkflowRequest,
) -> WorkbenchResult<crate::workbench::release::DraftWorkflow> {
    run_store(move |store| crate::workbench::release::draft_workflow(&store, request)).await
}
#[tauri::command]
pub async fn workbench_inspect_research_archive(
    request: crate::workbench::release::ExportArchiveRequest,
) -> WorkbenchResult<crate::workbench::release::ArchiveInspection> {
    run_store(move |_store| crate::workbench::release::inspect_archive(request)).await
}

#[tauri::command]
pub async fn workbench_import_research_archive(
    request: crate::workbench::release::ImportArchiveRequest,
) -> WorkbenchResult<crate::workbench::release::ArchiveReport> {
    run_store_exclusive(move |store| {
        let state = active_turn_state()
            .lock()
            .map_err(|_| WorkbenchError::worker("Workspace turn state is unavailable", true))?;
        if state.setup_in_progress
            || state.permit.is_some()
            || crate::workbench::research::jobs::has_active()
        {
            return Err(WorkbenchError::invalid(
                "Stop the active Workspace turn before restoring a research archive",
            ));
        }
        crate::workbench::release::import_archive(&store, request)
    })
    .await
}

#[tauri::command]
pub async fn workbench_export_conversation(
    session_id: String,
    path: String,
) -> WorkbenchResult<()> {
    run_store(move |store| {
        let target = std::path::PathBuf::from(path);
        if !target.is_absolute()
            || target.extension().and_then(|value| value.to_str()) != Some("md")
        {
            return Err(WorkbenchError::invalid(
                "Conversation exports require an absolute .md path",
            ));
        }
        let snapshot = store.conversation_snapshot(&session_id)?;
        let mut markdown = format!("# {}\n\n", snapshot.session.title);
        for item in snapshot.items {
            if !item.item_kind.to_ascii_lowercase().contains("message") {
                continue;
            }
            let Some(text) = transcript_text(&item.payload) else {
                continue;
            };
            let role = if item.item_kind.to_ascii_lowercase().contains("user") {
                "You"
            } else {
                "ChatGPT"
            };
            markdown.push_str(&format!("## {role}\n\n{text}\n\n"));
        }
        if markdown.len() > 64 * 1024 * 1024 {
            return Err(WorkbenchError::invalid(
                "Conversation export exceeds 64 MiB",
            ));
        }
        std::fs::write(&target, markdown).map_err(|error| {
            WorkbenchError::storage("Failed to write conversation export", error)
        })?;
        std::fs::File::open(&target)
            .and_then(|file| file.sync_all())
            .map_err(|error| {
                WorkbenchError::storage("Failed to sync conversation export", error)
            })?;
        Ok(())
    })
    .await
}
