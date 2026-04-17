pub mod commands;
pub mod deps;
pub mod env;
pub mod models;
pub mod output;
pub mod pipeline;
pub mod pipeline_config;
pub mod prompts;
pub mod settings;
pub mod storage;
pub mod updates;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::run_pipeline,
            commands::save_report_md,
            commands::save_all_artifacts,
            commands::print_report_html,
            commands::list_history,
            commands::cancel_pipeline,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
