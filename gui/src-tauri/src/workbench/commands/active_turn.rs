// Tauri wrappers; runtime services own mutable turn state.
use super::*;
#[tauri::command]
pub async fn workbench_active_turn() -> Option<ActiveTurn> {
    crate::workbench::runtime::admission::active_turn().await
}
