#![recursion_limit = "256"]

pub mod agent_runtime;
mod app_menu;
pub mod auto_review;
pub mod commands;
pub mod connections;
pub mod deps;
pub mod document_bundle;
pub mod emit;
pub mod engines;
pub mod env;
pub mod file_stream;
pub mod file_viewer;
pub mod findings;
pub mod model_catalog;
pub mod models;
pub mod orchestration;
pub mod orientation_contract;
pub mod output;
pub mod pipeline;
pub mod pipeline_config;
pub mod process;
pub mod projects;
pub mod prompts;
mod public_web;
pub mod run_scope;
pub mod runs;
pub mod safety;
pub mod settings;
pub mod storage;
pub mod updates;
pub mod workbench;
use tauri::Manager;

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
    process::suppress_windows_error_dialogs();
    let builder = tauri::Builder::default();
    #[cfg(feature = "e2e")]
    let builder = builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());
    #[cfg(target_os = "macos")]
    let builder = builder
        .menu(app_menu::build)
        .on_menu_event(app_menu::handle_event);

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .register_asynchronous_uri_scheme_protocol(
            file_stream::SCHEME,
            |context, request, responder| {
                static READERS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(8);
                let is_main = context.webview_label() == "main";
                tauri::async_runtime::spawn(async move {
                    let permit = READERS.acquire().await;
                    let response =
                        tokio::task::spawn_blocking(move || file_stream::handle(&request, is_main))
                            .await
                            .unwrap_or_else(|_| {
                                let mut response = tauri::http::Response::new(Vec::new());
                                *response.status_mut() =
                                    tauri::http::StatusCode::INTERNAL_SERVER_ERROR;
                                response
                            });
                    drop(permit);
                    responder.respond(response);
                });
            },
        )
        .setup(|app| {
            // Pin storage before any recovery or background work opens a store.
            // Settings remains accessible if a configured drive is unavailable.
            let _ = storage::data_root();
            env::initialize_resources(app.path().resource_dir().ok());
            commands::cleanup_stale_print_exports();
            engines::schedule_stale_managed_engine_cleanup();
            // Recovery scales with run history, so it must not hold the window
            // blank. The coordinator still starts after it.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let _ = runs::recover_resumable_runs();
                orchestration::start_if_present(handle);
            });
            // The login shell can take seconds, so its PATH is ready before the
            // first provider check has to wait for it.
            std::thread::spawn(|| {
                let _ = env::full_path();
            });
            workbench::programs::commands::start(app.handle().clone());
            updates::start(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main"
                    && orchestration::background_enabled()
                    && window.hide().is_ok()
                {
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            workbench::programs::commands::workbench_program,
            workbench::programs::commands::workbench_research_activity,
            workbench::programs::commands::workbench_followups,
            orchestration::commands::task_prepare,
            orchestration::missions::commands::mission_prepare,
            orchestration::missions::commands::mission_choices,
            orchestration::missions::commands::mission_list,
            orchestration::missions::commands::mission_get,
            orchestration::missions::commands::mission_events,
            orchestration::missions::commands::mission_methods,
            orchestration::missions::commands::mission_control,
            orchestration::missions::commands::mission_answer,
            orchestration::missions::commands::mission_retain_method,
            orchestration::missions::commands::mission_export,
            orchestration::missions::commands::mission_evidence,
            orchestration::discovery::commands::discovery_start,
            orchestration::discovery::commands::discovery_list,
            orchestration::discovery::commands::discovery_get,
            orchestration::discovery::commands::discovery_candidates,
            orchestration::discovery::commands::discovery_paper,
            orchestration::discovery::commands::discovery_select,
            orchestration::discovery::commands::discovery_control,
            orchestration::discovery::commands::discovery_export,
            orchestration::discovery::commands::discovery_events,
            orchestration::commands::task_list,
            orchestration::commands::task_get,
            orchestration::commands::task_events,
            orchestration::commands::task_control,
            orchestration::commands::task_input,
            orchestration::commands::task_schedules,
            orchestration::commands::task_update_schedule,
            orchestration::commands::task_delete_schedule,
            orchestration::commands::task_mark_seen,
            orchestration::commands::task_preview_times,
            orchestration::commands::task_saved_chains,
            orchestration::commands::task_save_chain,
            orchestration::commands::task_validate_chain,
            orchestration::commands::task_background,
            orchestration::commands::task_sessions,
            orchestration::commands::task_proposals,
            orchestration::commands::task_prepare_proposal,
            orchestration::commands::task_export_artifact,
            orchestration::commands::task_export_chain,
            orchestration::commands::task_step_output,
            commands::export::save_report_md,
            commands::export::save_report_html,
            commands::export::save_report_docx,
            commands::export::save_report_docx_comments,
            commands::export::save_report_annotated_pdf,
            commands::export::save_all_artifacts,
            commands::export::export_run_package,
            commands::export::reveal_export_in_folder,
            commands::export::print_report_html,
            commands::artifacts::cancel_pipeline,
            commands::lifecycle::cancel_pass,
            commands::artifacts::get_run_manifest,
            commands::artifacts::read_artifact,
            commands::artifacts::read_pdf_artifact_page,
            commands::artifacts::read_pdf_artifact_bytes,
            commands::artifacts::reveal_run_artifact,
            commands::artifacts::open_run_artifact,
            commands::artifacts::open_run_artifact_stream,
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
            commands::rerun::rerun_requirements,
            // Review run queue (several reviews at once)
            commands::run_queue::enqueue_run,
            commands::run_queue::enqueue_batch,
            commands::run_queue::enqueue_rerun,
            commands::run_queue::list_run_queue,
            commands::run_queue::get_queued_run_result,
            commands::run_queue::dismiss_queued_runs,
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
            workbench::commands::workbench_transcript_page,
            workbench::commands::workbench_harness_catalog,
            workbench::commands::workbench_native_prompt_catalog,
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
            workbench::commands::workbench_project_tasks,
            workbench::commands::workbench_project_index,
            workbench::commands::workbench_repository_status,
            workbench::commands::workbench_repository_fetch,
            workbench::commands::workbench_version_control_state,
            workbench::commands::workbench_version_control_diff,
            workbench::commands::workbench_version_control_history,
            workbench::commands::workbench_version_control_commit,
            workbench::commands::workbench_version_control_action,
            workbench::commands::workbench_version_control_cancel,
            workbench::commands::workbench_capture_execution_plan,
            workbench::commands::workbench_execution_plan_status,
            workbench::commands::workbench_authorize_execution_plan,
            workbench::commands::workbench_import_dataset_metadata,
            workbench::commands::workbench_dataset_rows,
            workbench::commands::workbench_reading_inbox_state,
            workbench::commands::workbench_desk_records,
            workbench::commands::workbench_research_object,
            workbench::commands::workbench_context_selection,
            workbench::commands::workbench_save_context_selection,
            workbench::commands::workbench_reading_collection,
            workbench::commands::workbench_research_search,
            workbench::commands::workbench_research_index,
            workbench::commands::workbench_save_relation,
            workbench::commands::workbench_save_decision,
            workbench::commands::workbench_change_impact,
            workbench::commands::workbench_session_handoff,
            workbench::commands::workbench_data_policy,
            workbench::commands::workbench_import_dataset,
            workbench::commands::workbench_save_sample,
            workbench::commands::workbench_acquisition_network,
            workbench::commands::workbench_crossref_lookup,
            workbench::commands::workbench_acquire_candidate,
            workbench::commands::workbench_acquire_pdf,
            workbench::commands::workbench_acquire_fred,
            workbench::commands::workbench_acquire_series,
            workbench::commands::workbench_crossref_contact,
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
            workbench::commands::workbench_file_read,
            workbench::commands::workbench_file_external,
            workbench::commands::workbench_file_open,
            workbench::commands::workbench_file_release,
            workbench::commands::workbench_file_table,
            workbench::commands::workbench_file_image_preview,
            workbench::commands::workbench_file_document_text,
            workbench::commands::workbench_file_list,
            workbench::commands::workbench_file_search,
            workbench::commands::workbench_file_stat,
            workbench::commands::workbench_conversation_file_open,
            workbench::commands::workbench_conversation_folder_reveal,
            workbench::commands::workbench_conversation_file_read,
            workbench::commands::workbench_file_reveal,
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
            workbench::commands::workbench_detect_toolchains,
            workbench::commands::workbench_run_execution,
            workbench::commands::workbench_list_jobs,
            workbench::commands::workbench_execution_output,
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
            workbench::commands::workbench_missing_backup_files,
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
            workbench::claude::transport::workbench_claude_model_catalog,
            workbench::api::config::workbench_api_model_catalog,
            workbench::commands::workbench_codex_validate_model_selection,
            workbench::commands::workbench_codex_rate_limits,
            workbench::claude::usage::workbench_claude_rate_limits,
            workbench::browser::workbench_browser_status,
            workbench::tool_setup::workbench_tool_setup_cancel,
            workbench::shell::workbench_shell_status,
            workbench::shell::workbench_shell_install,
            workbench::browser::workbench_browser_install,
            workbench::commands::workbench_codex_send_turn,
            workbench::commands::workbench_codex_interrupt_turn,
            workbench::commands::workbench_codex_reconcile_session,
            workbench::commands::workbench_codex_resolve_server_request,
            workbench::commands::workbench_codex_pending_requests,
            workbench::commands::workbench_active_turn,
            commands::config::get_settings,
            commands::config::save_settings,
            commands::config::discard_undecryptable_secret,
            commands::config::get_model_catalog,
            commands::config::get_pipeline_config,
            commands::config::get_run_setup,
            commands::config::get_auto_review_catalog,
            commands::config::resolve_orientation_schema_catalogs,
            commands::config::get_auto_review_specialist_step,
            commands::config::get_execution_plan,
            pipeline::analyzers::registry::list_analyzers,
            commands::config::save_pipeline_config,
            commands::config::get_default_parallel_template,
            commands::config::get_default_prompt,
            commands::config::get_orientation_defaults,
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
            commands::config::export_bundle,
            commands::config::import_bundle,
            // Update check
            commands::config::check_for_update,
            commands::config::update_status,
            commands::config::update_set_automatic,
            commands::config::update_install_state,
            commands::config::update_install_start,
            commands::config::update_install_cancel,
            commands::config::update_install_restart,
            commands::config::update_running_automations,
            commands::config::open_pipeline_dir,
            commands::config::get_storage_settings,
            commands::config::set_storage_directory,
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
            agent_runtime::codex::chatgpt::chatgpt_account_status,
            agent_runtime::codex::chatgpt::chatgpt_login_start,
            agent_runtime::codex::chatgpt::chatgpt_login_cancel,
            agent_runtime::codex::chatgpt::chatgpt_logout,
            agent_runtime::codex::chatgpt::chatgpt_select_existing_account,
            agent_runtime::codex::chatgpt::chatgpt_model_catalog,
            agent_runtime::codex::chatgpt::chatgpt_rate_limits,
            app_menu::set_sidebar_menu_collapsed,
            mark_smoke_ready,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                stop_runtimes();
            }
        });
    commands::cleanup_print_export();
}

/// Stop every runtime that owns child processes. Runs on normal exit and
/// before the Windows updater ends the process, which skips the exit event.
fn stop_runtimes() {
    workbench::store::Store::close_idle_connection();
    tauri::async_runtime::block_on(orchestration::shutdown());
    tauri::async_runtime::block_on(workbench::claude::shutdown());
    tauri::async_runtime::block_on(workbench::api::shutdown());
    tauri::async_runtime::block_on(workbench::research::jobs::shutdown());
    tauri::async_runtime::block_on(pipeline::codex_server::shutdown());
    tauri::async_runtime::block_on(workbench::commands::stop_codex_runtime());
    tauri::async_runtime::block_on(agent_runtime::codex::chatgpt::shutdown());
}

#[cfg(test)]
mod smoke_tests {
    use super::write_smoke_ready;

    #[test]
    fn readiness_marker_is_a_noop_without_ci_environment() {
        assert!(!write_smoke_ready(None).unwrap());
    }

    #[test]
    fn exit_stops_conversation_app_server_before_the_shared_account() {
        let source = include_str!("lib.rs");
        let stops = &source[source.find("fn stop_runtimes").unwrap()..];
        let codex = stops.find("stop_codex_runtime()").unwrap();
        assert!(codex < stops.find("chatgpt::shutdown()").unwrap());
    }
}
