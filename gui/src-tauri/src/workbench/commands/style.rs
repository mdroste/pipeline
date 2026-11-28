//! Drafting style notes for About you from the researcher's own papers.
use super::*;
use crate::workbench::runtime::style_draft::StyleDraft;

/// Drafts style notes with the connection and model new conversations use.
/// Returns the draft; the researcher decides whether to save it.
#[tauri::command]
pub async fn workbench_draft_style_notes(
    app: tauri::AppHandle,
) -> Result<StyleDraft, RuntimeCommandError> {
    let overrides = sessions::default_session_overrides(&crate::settings::load())?;
    crate::workbench::runtime::style_draft::draft(app, overrides).await
}
