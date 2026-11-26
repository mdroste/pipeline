use crate::workbench::titles::TitlePreferences;
// Tauri wrappers; runtime services own mutable turn state.
use super::*;
#[tauri::command]
pub async fn workbench_get_title_preferences() -> WorkbenchResult<TitlePreferences> {
    crate::workbench::runtime::titles::get_title_preferences().await
}

#[tauri::command]
pub async fn workbench_save_title_preferences(
    preferences: TitlePreferences,
) -> WorkbenchResult<TitlePreferences> {
    crate::workbench::runtime::titles::save_title_preferences(preferences).await
}

#[tauri::command]
pub async fn workbench_generate_session_title(
    app: tauri::AppHandle,
    session_id: String,
) -> Result<String, RuntimeCommandError> {
    crate::workbench::runtime::titles::request_session_title(app, session_id).await
}
