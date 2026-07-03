pub mod commands;
pub mod deps;
pub mod env;
pub mod models;
pub mod output;
pub mod pipeline;
pub mod pipeline_config;
pub mod prompts;
pub mod runs;
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
    let bin = if cfg!(windows) { "pdftoppm.exe" } else { "pdftoppm" };
    let candidates = [
        base.join("resources").join("poppler"),
        base.join("poppler"),
        base.clone(),
    ];
    candidates.into_iter().find(|dir| dir.join(bin).is_file())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            env::set_bundled_poppler_dir(locate_bundled_poppler(app));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::run_pipeline,
            commands::save_report_md,
            commands::save_all_artifacts,
            commands::print_report_html,
            commands::list_history,
            commands::cancel_pipeline,
            commands::get_run_manifest,
            commands::read_artifact,
            commands::check_deps,
            commands::get_settings,
            commands::save_settings,
            commands::get_pipeline_config,
            commands::save_pipeline_config,
            commands::get_default_parallel_template,
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
            commands::export_bundle,
            commands::import_bundle,
            // Update check
            commands::check_for_update,
            // Preprocessing artifact inspection
            commands::read_cached_paper_text,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
