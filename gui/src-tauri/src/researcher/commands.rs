//! Settings commands for About you. Every change returns the whole profile,
//! so Settings shows what is on disk rather than what it asked for.

use super::{Note, Profile, SampleUpdate};
use std::path::PathBuf;

async fn in_folder<T: Send + 'static>(
    work: impl FnOnce(PathBuf) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(move || work(super::folder()?))
        .await
        .map_err(|error| format!("About you could not finish: {error}"))?
}

#[tauri::command]
pub async fn researcher_profile() -> Result<Profile, String> {
    in_folder(|folder| super::load(&folder)).await
}

/// `note` is `about` or `style`.
#[tauri::command]
pub async fn researcher_save_note(note: String, text: String) -> Result<Profile, String> {
    let note = match note.as_str() {
        "about" => Note::About,
        "style" => Note::Style,
        _ => return Err("Unknown About you note.".into()),
    };
    in_folder(move |folder| {
        super::save_note(&folder, note, &text)?;
        super::load(&folder)
    })
    .await
}

#[tauri::command]
pub async fn researcher_add_sample(path: String) -> Result<Profile, String> {
    in_folder(move |folder| {
        super::add_sample(&folder, &PathBuf::from(path))?;
        super::load(&folder)
    })
    .await
}

#[tauri::command]
pub async fn researcher_update_sample(update: SampleUpdate) -> Result<Profile, String> {
    in_folder(move |folder| {
        super::update_sample(&folder, update)?;
        super::load(&folder)
    })
    .await
}

#[tauri::command]
pub async fn researcher_remove_sample(id: String) -> Result<Profile, String> {
    in_folder(move |folder| {
        super::remove_sample(&folder, &id)?;
        super::load(&folder)
    })
    .await
}

/// Opens the folder, creating it first so there is something to show. The
/// path is resolved here and never taken from the window.
#[tauri::command]
pub async fn researcher_open_folder() -> Result<(), String> {
    in_folder(|folder| {
        std::fs::create_dir_all(&folder)
            .map_err(|error| format!("Could not create the About you folder: {error}"))?;
        crate::file_viewer::open_trusted_path(&folder)
            .map_err(|error| format!("Failed to open folder: {error}"))
    })
    .await
}
