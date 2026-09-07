#![recursion_limit = "256"]

pub mod agent_runtime;
pub mod auto_review;
pub mod commands;
pub mod deps;
pub mod document_bundle;
pub mod emit;
pub mod engines;
pub mod env;
pub mod findings;
pub mod model_catalog;
pub mod models;
pub mod orientation_contract;
pub mod output;
pub mod pipeline;
pub mod pipeline_config;
pub mod process;
pub mod projects;
pub mod prompts;
pub mod runs;
pub mod safety;
pub mod settings;
pub mod updates;
pub mod workbench;

/// Probe a few candidate paths under the resource dir for the bundled
/// `pdftoppm` binary.  Returns the directory that holds it.
///
/// Tauri places resources slightly differently per bundle target; the most
/// common layouts are checked here so the runtime works on .app/.dmg, .msi,
/// .deb/.rpm, and .AppImage without per-platform code.
fn locate_bundled_poppler(app: &tauri::App) -> Option<std::path::PathBuf> {
    use tauri::Manager;
    let base = app.path().resource_dir().ok()?;
    let bin = if cfg!(windows) {
        "pdftoppm.exe"
    } else {
        "pdftoppm"
    };
    let candidates = [
        base.join("resources").join("poppler"),
        base.join("poppler"),
        base.clone(),
    ];
    candidates.into_iter().find(|dir| dir.join(bin).is_file())
}

/// Signal to a packaging smoke test that the native window loaded the
/// frontend and completed a Tauri IPC round trip. In normal application runs
/// the environment variable is absent and this command is a no-op.
#[tauri::command]
fn mark_smoke_ready() -> Result<bool, String> {
    write_smoke_ready(std::env::var_os("PIPELINE_SMOKE_READY_FILE"))
}

fn write_smoke_ready(path: Option<std::ffi::OsString>) -> Result<bool, String> {
    let Some(path) = path else {
        return Ok(false);
    };
    std::fs::write(std::path::PathBuf::from(path), b"ready\n")
        .map_err(|error| format!("failed to write smoke-test readiness marker: {error}"))?;
    Ok(true)
}

pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(feature = "e2e")]
    let builder = builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            env::set_bundled_poppler_dir(locate_bundled_poppler(app));
            commands::cleanup_stale_print_exports();
            engines::schedule_stale_managed_engine_cleanup();
            let _ = runs::recover_resumable_runs();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::run_entry::run_pipeline,
            commands::export::save_report_md,
            commands::export::save_all_artifacts,
            commands::export::export_run_artifacts,
            commands::export::export_run_package,
            commands::export::reveal_export_in_folder,
            commands::export::print_report_html,
            commands::artifacts::cancel_pipeline,
            commands::lifecycle::cancel_pass,
            commands::artifacts::get_run_manifest,
            commands::artifacts::read_artifact,
            commands::artifacts::read_pdf_artifact_page,
            commands::artifacts::read_page_artifact,
            commands::artifacts::list_runs,
            commands::artifacts::get_run_report,
            commands::artifacts::reconcile_runs,
            commands::artifacts::update_run_meta,
            commands::artifacts::delete_run,
            commands::artifacts::list_trashed_runs,
            commands::artifacts::restore_trashed_run,
            commands::artifacts::permanently_delete_trashed_run,
            commands::artifacts::runs_disk_usage,
            commands::artifacts::preview_purge_runs,
            commands::artifacts::purge_runs,
            commands::artifacts::save_text_file,
            // Batch queue
            commands::batch::start_batch,
            commands::batch::get_batch_status,
            commands::batch::cancel_batch,
            commands::batch::list_input_files,
            // Resume / partial re-run
            commands::rerun::rerun_run,
            // Annotations
            commands::artifacts::get_annotations,
            commands::artifacts::save_annotations,
            commands::artifacts::draft_calibration,
            // Local projects group immutable runs without changing them.
            projects::list_projects,
            projects::create_project,
            projects::update_project,
            projects::set_project_run,
            projects::delete_project,
            projects::list_trashed_projects,
            projects::restore_trashed_project,
            projects::permanently_delete_trashed_project,
            projects::ledger::sync_project_issue_ledger,
            projects::ledger::update_project_issue,
            projects::ledger::merge_project_issues,
            // Workbench research-store boundary. All database calls run on a
            // dedicated bounded blocking pool rather than Tauri's async runtime.
            workbench::commands::workbench_create_workspace,
            workbench::commands::workbench_register_workspace_root,
            workbench::commands::workbench_clear_workspace_root,
            workbench::commands::workbench_get_workspace,
            workbench::commands::workbench_list_workspaces,
            workbench::commands::workbench_update_workspace,
            workbench::commands::workbench_create_session,
            workbench::commands::workbench_update_session,
            workbench::commands::workbench_move_session,
            workbench::commands::workbench_delete_session,
            workbench::commands::workbench_get_title_preferences,
            workbench::commands::workbench_save_title_preferences,
            workbench::commands::workbench_generate_session_title,
            workbench::commands::workbench_list_sessions,
            workbench::commands::workbench_session_snapshot,
            workbench::commands::workbench_conversation_snapshot,
            workbench::commands::workbench_harness_catalog,
            workbench::commands::workbench_effective_harness,
            workbench::commands::workbench_get_workspace_config,
            workbench::commands::workbench_save_workspace_config,
            workbench::commands::workbench_clone_preset,
            workbench::commands::workbench_update_preset,
            workbench::commands::workbench_create_note,
            workbench::commands::workbench_update_note,
            workbench::commands::workbench_list_notes,
            workbench::commands::workbench_import_paper,
            workbench::commands::workbench_list_papers,
            workbench::commands::workbench_project_home,
            workbench::commands::workbench_studio_mutate,
            workbench::commands::workbench_studio_export_file,
            workbench::commands::workbench_studio_records,
            workbench::commands::workbench_studio_history,
            workbench::commands::workbench_theory_overview,
            workbench::commands::workbench_editor_file,
            workbench::commands::workbench_editor_external,
            workbench::commands::workbench_build_sync,
            workbench::commands::workbench_workflow_finding_preview,
            workbench::commands::workbench_finding_package_preview,
            workbench::commands::workbench_report_preview,
            workbench::commands::workbench_response_export,
            workbench::commands::workbench_focus_review,
            workbench::commands::workbench_compare_experiment,
            workbench::commands::workbench_compare_series,
            workbench::commands::workbench_binding_coverage,
            workbench::commands::workbench_bibliography_preview,
            workbench::commands::workbench_citation_navigation,
            workbench::commands::workbench_source_passage,
            workbench::commands::workbench_zotero_preview,
            workbench::commands::workbench_zotero_import,
            workbench::commands::workbench_working_file_preview,
            workbench::commands::workbench_project_capabilities,
            workbench::commands::workbench_preview_host_execution,
            workbench::commands::workbench_authorize_host_execution,
            workbench::commands::workbench_project_mutate,
            workbench::commands::workbench_document_read,
            workbench::commands::workbench_anchor_mapping,
            workbench::commands::workbench_snapshot_preview,
            workbench::commands::workbench_task_session,
            workbench::commands::workbench_paper_read,
            workbench::commands::workbench_paper_search,
            workbench::commands::workbench_import_source,
            workbench::commands::workbench_list_sources,
            workbench::commands::workbench_propose_claim,
            workbench::commands::workbench_set_claim_state,
            workbench::commands::workbench_propose_evidence,
            workbench::commands::workbench_confirm_evidence,
            workbench::commands::workbench_research_ledger,
            workbench::commands::workbench_save_execution_profile,
            workbench::commands::workbench_list_execution_profiles,
            workbench::commands::workbench_run_execution,
            workbench::commands::workbench_list_jobs,
            workbench::commands::workbench_job_log,
            workbench::commands::workbench_reconcile_job,
            workbench::commands::workbench_list_executions,
            workbench::commands::workbench_cancel_execution,
            workbench::commands::workbench_compare_results,
            workbench::commands::workbench_record_structured_result,
            workbench::commands::workbench_list_structured_results,
            workbench::commands::workbench_verify_evidence_results,
            workbench::commands::workbench_list_recipes,
            workbench::commands::workbench_clone_recipe,
            workbench::commands::workbench_update_recipe,
            workbench::commands::workbench_check_recipe_inputs,
            workbench::commands::workbench_start_recipe,
            workbench::commands::workbench_complete_recipe,
            workbench::commands::workbench_list_recipe_runs,
            workbench::commands::workbench_performance_budgets,
            workbench::commands::workbench_record_performance,
            workbench::commands::workbench_record_research_evaluation,
            workbench::commands::workbench_list_research_evaluations,
            workbench::commands::workbench_prepare_review_handoff,
            workbench::commands::workbench_link_review_handoff,
            workbench::commands::workbench_export_research_archive,
            workbench::commands::workbench_inspect_research_archive,
            workbench::commands::workbench_preview_project_exchange,
            workbench::commands::workbench_export_project_exchange,
            workbench::commands::workbench_inspect_project_exchange,
            workbench::commands::workbench_preview_project_exchange_import,
            workbench::commands::workbench_import_project_exchange,
            workbench::commands::workbench_list_exchange_conflicts,
            workbench::commands::workbench_resolve_exchange_conflict,
            workbench::commands::workbench_storage_report,
            workbench::commands::workbench_prune_storage,
            workbench::commands::workbench_restore_trash,
            workbench::commands::workbench_empty_trash,
            workbench::commands::workbench_draft_workflow,
            workbench::commands::workbench_import_research_archive,
            workbench::commands::workbench_export_conversation,
            workbench::commands::workbench_reconcile_workspace_roots,
            workbench::commands::workbench_codex_connect,
            workbench::commands::workbench_codex_account_state,
            workbench::commands::workbench_codex_login_start,
            workbench::commands::workbench_codex_login_cancel,
            workbench::commands::workbench_codex_logout,
            workbench::commands::workbench_codex_model_catalog,
            workbench::commands::workbench_codex_validate_model_selection,
            workbench::commands::workbench_codex_rate_limits,
            workbench::commands::workbench_codex_send_turn,
            workbench::commands::workbench_codex_interrupt_turn,
            workbench::commands::workbench_codex_reconcile_session,
            workbench::commands::workbench_codex_resolve_server_request,
            workbench::commands::workbench_codex_pending_requests,
            commands::config::get_settings,
            commands::config::save_settings,
            commands::config::get_model_catalog,
            commands::config::get_pipeline_config,
            commands::config::get_run_setup,
            commands::config::get_auto_review_catalog,
            commands::config::resolve_orientation_schema_catalogs,
            commands::config::get_auto_review_specialist_step,
            commands::config::get_execution_plan,
            commands::config::save_pipeline_config,
            commands::config::get_default_parallel_template,
            commands::config::get_default_prompt,
            commands::config::get_orientation_defaults,
            commands::config::get_auto_review_orientation_prompt,
            commands::config::get_findings_output_schema,
            commands::config::preview_provider_schema,
            commands::config::get_auto_review_orientation_defaults,
            commands::config::reset_pipeline_config,
            // Profile management
            commands::config::list_profiles,
            commands::config::get_active_profile,
            commands::config::create_profile,
            commands::config::duplicate_profile,
            commands::config::rename_profile,
            commands::config::delete_profile,
            commands::config::switch_profile,
            // Export/import
            commands::config::export_item,
            commands::config::import_item,
            commands::config::export_profile,
            commands::config::import_profile,
            commands::config::import_profile_from_url,
            commands::config::export_bundle,
            commands::config::import_bundle,
            // Update check
            commands::config::check_for_update,
            commands::config::open_pipeline_dir,
            // Managed local engines
            commands::config::list_engines,
            commands::config::install_engine,
            commands::config::uninstall_engine,
            commands::config::cancel_engine_install,
            pipeline::codex_server::connection::workflow_codex_status,
            pipeline::codex_server::connection::workflow_codex_acknowledge_attempt,
            pipeline::codex_server::connection::workflow_codex_reconcile_attempt,
            pipeline::codex_server::connection::workflow_codex_login_start,
            pipeline::codex_server::connection::workflow_codex_login_cancel,
            pipeline::codex_server::connection::workflow_codex_logout,
            pipeline::codex_server::connection::workflow_codex_rate_limits,
            mark_smoke_ready,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                tauri::async_runtime::block_on(workbench::research::jobs::shutdown());
                tauri::async_runtime::block_on(pipeline::codex_server::shutdown());
            }
        });
    commands::cleanup_print_export();
}

#[cfg(test)]
mod smoke_tests {
    use super::write_smoke_ready;

    #[test]
    fn readiness_marker_is_a_noop_without_ci_environment() {
        assert!(!write_smoke_ready(None).unwrap());
    }
}
