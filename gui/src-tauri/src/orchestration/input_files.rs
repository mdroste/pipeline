//! Read-only selection of an automation input within its conversation folder.
use crate::workbench::{commands::run_store, project, tasks};
use serde::Serialize;
use std::path::{Path, PathBuf};

async fn folder(session_id: String) -> Result<PathBuf, String> {
    run_store(move |s| tasks::binding(&s, &session_id))
        .await
        .map(|binding| PathBuf::from(binding.runtime_root))
        .map_err(|e| e.message)
}

#[tauri::command]
pub async fn task_input_files(
    session_id: String,
    directory: String,
) -> Result<project::FileListing, String> {
    let root = folder(session_id).await?;
    tauri::async_runtime::spawn_blocking(move || {
        project::list_workspace_directory(
            &root,
            project::FileListRequest {
                workspace_id: String::new(),
                directory,
                checkpoint_id: None,
            },
        )
        .map_err(|e| e.message)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputFile {
    root: String,
    path: String,
    name: String,
}

fn validate(root: &Path, path: &str) -> Result<InputFile, String> {
    let root = root
        .canonicalize()
        .map_err(|_| "The conversation folder is unavailable")?;
    let selected = root
        .join(path)
        .canonicalize()
        .map_err(|_| "This file is unavailable. Choose another file.")?;
    let relative = selected
        .strip_prefix(&root)
        .map_err(|_| "Choose a file inside this conversation’s folder.")?
        .to_string_lossy()
        .replace('\\', "/");
    // Reuse the descriptor-relative browser checks; reserved paths and links
    // cannot become readable by selecting them through a native dialog.
    let stamps = project::stat_workspace_files(
        &root,
        project::FileStatRequest {
            workspace_id: String::new(),
            paths: vec![relative.clone()],
            checkpoint_id: None,
        },
    )
    .map_err(|e| e.message)?;
    let stamp = stamps.first().ok_or("This file is unavailable")?;
    if stamp.kind != Some("file") {
        return Err("Choose a readable file inside the conversation folder.".into());
    }
    if stamp.size > 32 * 1024 * 1024 {
        return Err("Choose a file smaller than 32 MB.".into());
    }
    Ok(InputFile {
        root: crate::file_viewer::system_path(&root)
            .to_string_lossy()
            .into_owned(),
        path: relative,
        name: selected
            .file_name()
            .ok_or("This file has no name")?
            .to_string_lossy()
            .into_owned(),
    })
}

#[tauri::command]
pub async fn task_validate_input_file(
    session_id: String,
    path: String,
) -> Result<InputFile, String> {
    let root = folder(session_id).await?;
    tauri::async_runtime::spawn_blocking(move || validate(&root, &path))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn input_selection_checks_membership_availability_and_file_kind() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        std::fs::create_dir_all(root.join("papers")).unwrap();
        std::fs::write(root.join("papers/draft.md"), "Paper").unwrap();
        std::fs::write(temp.path().join("outside.md"), "Outside").unwrap();
        let file = validate(&root, "papers/draft.md").unwrap();
        assert_eq!(file.path, "papers/draft.md");
        assert_eq!(file.name, "draft.md");
        assert!(validate(&root, "../outside.md")
            .unwrap_err()
            .contains("inside"));
        assert!(validate(&root, "missing.md").is_err());
        assert!(validate(&root, "papers").is_err());
        std::fs::create_dir(root.join(".git")).unwrap();
        std::fs::write(root.join(".git/config"), "reserved").unwrap();
        assert!(validate(&root, ".git/config").is_err());
    }
}
