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
            let _ = runs::recover_resumable_runs();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::run_pipeline,
            commands::save_report_md,
            commands::save_all_artifacts,
            commands::export_run_artifacts,
            commands::print_report_html,
            commands::list_history,
            commands::cancel_pipeline,
            commands::cancel_pass,
            commands::get_run_manifest,
            commands::read_artifact,
            commands::read_page_artifact,
            commands::list_runs,
            commands::get_run_report,
            commands::reconcile_runs,
            commands::update_run_meta,
            commands::delete_run,
            commands::runs_disk_usage,
            commands::preview_purge_runs,
            commands::purge_runs,
            commands::save_text_file,
            // Batch queue
            commands::start_batch,
            commands::get_batch_status,
            commands::cancel_batch,
            commands::list_input_files,
            commands::start_watch,
            commands::stop_watch,
            commands::get_watch_status,
            // Resume / partial re-run
            commands::rerun_run,
            // Annotations
            commands::get_annotations,
            commands::save_annotations,
            commands::draft_calibration,
            commands::check_deps,
            commands::get_settings,
            commands::save_settings,
            commands::get_model_catalog,
            commands::get_pipeline_config,
            commands::get_execution_plan,
            commands::save_pipeline_config,
            commands::get_default_parallel_template,
            commands::get_default_prompt,
            commands::reset_pipeline_config,
            // Profile management
            commands::list_profiles,
            commands::get_active_profile,
            commands::create_profile,
            commands::duplicate_profile,
            commands::rename_profile,
            commands::delete_profile,
            commands::switch_profile,
            // Export/import
            commands::export_item,
            commands::import_item,
            commands::export_profile,
            commands::import_profile,
            commands::import_profile_from_url,
            commands::export_bundle,
            commands::import_bundle,
            // Update check
            commands::check_for_update,
            // Preprocessing artifact inspection
            commands::read_cached_paper_text,
            commands::open_pipeline_dir,
            // Managed local engines
            commands::list_engines,
            commands::install_engine,
            commands::uninstall_engine,
            commands::retired_marker_status,
            commands::remove_retired_marker,
            commands::cancel_engine_install,
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
