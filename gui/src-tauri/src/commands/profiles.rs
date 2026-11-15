//! Workflow profile editing, activation, and local import and export.

use super::{profile_import::read_import_file, save_via_dialog};
use crate::pipeline_config::{self, PipelineConfig, ProfileSummary};
use tauri::AppHandle;

#[tauri::command]
pub async fn get_pipeline_config() -> Result<PipelineConfig, String> {
    Ok(pipeline_config::load())
}

#[tauri::command]
pub async fn save_pipeline_config(
    config: PipelineConfig,
    profile_id: Option<String>,
) -> Result<(), String> {
    let target = profile_id.unwrap_or_else(pipeline_config::get_active_profile_id);
    pipeline_config::save_for(&target, &config)
}

#[tauri::command]
pub async fn reset_pipeline_config() -> Result<PipelineConfig, String> {
    pipeline_config::reset_defaults()
}

#[tauri::command]
pub async fn list_profiles() -> Result<Vec<ProfileSummary>, String> {
    pipeline_config::list_profiles()
}

#[tauri::command]
pub async fn get_active_profile() -> Result<String, String> {
    Ok(pipeline_config::get_active_profile_id())
}

#[tauri::command]
pub async fn create_profile(name: String) -> Result<ProfileSummary, String> {
    pipeline_config::create_profile(&name)
}

#[tauri::command]
pub async fn duplicate_profile(
    source_id: String,
    new_name: String,
) -> Result<ProfileSummary, String> {
    pipeline_config::duplicate_profile(&source_id, &new_name)
}

#[tauri::command]
pub async fn rename_profile(id: String, new_name: String) -> Result<ProfileSummary, String> {
    pipeline_config::rename_profile(&id, &new_name)
}

#[tauri::command]
pub async fn delete_profile(id: String) -> Result<(), String> {
    pipeline_config::delete_profile(&id)
}

#[tauri::command]
pub async fn switch_profile(id: String) -> Result<PipelineConfig, String> {
    pipeline_config::switch_profile(&id)
}

#[tauri::command]
pub async fn export_item(
    app: AppHandle,
    json: String,
    suggested_name: Option<String>,
) -> Result<Option<String>, String> {
    let name = suggested_name.as_deref().unwrap_or("pipeline-export.json");
    save_via_dialog(&app, name, "JSON", "json", json.into_bytes()).await
}

#[tauri::command]
pub async fn import_item(path: String) -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(move || {
        let content = read_import_file(&path)?;
        let envelope = pipeline_config::import_envelope(&content)?;
        serde_json::to_value(&envelope).map_err(|e| format!("Serialize error: {e}"))
    })
    .await
    .map_err(|e| format!("Import failed: {e}"))?
}

#[tauri::command]
pub async fn export_profile(
    app: AppHandle,
    id: String,
    suggested_name: Option<String>,
) -> Result<Option<String>, String> {
    let json = pipeline_config::export_profile_data(&id)?;
    let name = suggested_name.as_deref().unwrap_or("pipeline-profile.json");
    save_via_dialog(&app, name, "JSON", "json", json.into_bytes()).await
}

/// Import a profile from a parsed envelope, checking the schema version.
fn import_profile_envelope(
    envelope: pipeline_config::ExportEnvelope,
) -> Result<ProfileSummary, String> {
    match envelope {
        pipeline_config::ExportEnvelope::Profile { schema_version, name, steps, merge, outputs, context_cache, use_orientation, orientation_prompt, orientation_schema, extraction, parallel_context_template, variables } => {
            if schema_version > pipeline_config::CURRENT_SCHEMA_VERSION {
                return Err(format!(
                    "This profile was made with a newer version of Pipeline (schema v{schema_version}). Update the app to import it."
                ));
            }
            pipeline_config::import_profile_data(&name, steps, merge, outputs, context_cache, use_orientation, orientation_prompt, orientation_schema, extraction, parallel_context_template, variables)
        }
        pipeline_config::ExportEnvelope::Step { .. } => {
            Err("This file contains a single step, not a profile. Use Import on the pipeline page to add it to the current profile.".into())
        }
        pipeline_config::ExportEnvelope::Bundle { .. } => {
            Err("This file is a full bundle. Use Import All to restore it.".into())
        }
    }
}

#[tauri::command]
pub async fn import_profile(path: String) -> Result<ProfileSummary, String> {
    tokio::task::spawn_blocking(move || {
        let content = read_import_file(&path)?;
        let envelope = pipeline_config::import_envelope(&content)?;
        import_profile_envelope(envelope)
    })
    .await
    .map_err(|e| format!("Import failed: {e}"))?
}

#[tauri::command]
pub async fn export_bundle(
    app: AppHandle,
    suggested_name: Option<String>,
) -> Result<Option<String>, String> {
    let json = pipeline_config::export_bundle()?;
    let name = suggested_name
        .as_deref()
        .unwrap_or("pipeline-settings-backup.json");
    save_via_dialog(&app, name, "JSON", "json", json.into_bytes()).await
}

#[tauri::command]
pub async fn import_bundle(path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let content = read_import_file(&path)?;
        pipeline_config::import_bundle(&content)
    })
    .await
    .map_err(|e| format!("Import failed: {e}"))?
}
