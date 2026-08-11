pub mod auto_review;
pub mod commands;
pub mod deps;
pub mod document_bundle;
pub mod emit;
pub mod engines;
pub mod env;
pub mod model_catalog;
pub mod models;
pub mod output;
pub mod pipeline;
pub mod pipeline_config;
pub mod process;
pub mod projects;
pub mod prompts;
pub mod runs;
pub mod safety;
pub mod settings;
pub mod storage;
pub mod updates;

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
            commands::export::print_report_html,
            commands::config::list_history,
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
            projects::ledger::sync_project_issue_ledger,
            projects::ledger::update_project_issue,
            projects::ledger::merge_project_issues,
            commands::config::check_deps,
            commands::config::get_settings,
            commands::config::save_settings,
            commands::config::get_model_catalog,
            commands::config::get_pipeline_config,
            commands::config::get_auto_review_catalog,
            commands::config::get_auto_review_specialist_step,
            commands::config::get_execution_plan,
            commands::config::save_pipeline_config,
            commands::config::get_default_parallel_template,
            commands::config::get_default_prompt,
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
            // Preprocessing artifact inspection
            commands::config::read_cached_paper_text,
            commands::config::open_pipeline_dir,
            // Managed local engines
            commands::config::list_engines,
            commands::config::install_engine,
            commands::config::uninstall_engine,
            commands::config::retired_marker_status,
            commands::config::remove_retired_marker,
            commands::config::cancel_engine_install,
            mark_smoke_ready,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
