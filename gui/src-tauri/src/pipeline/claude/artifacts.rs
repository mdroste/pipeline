//! Live artifact-write supervision.

use super::*;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct EventTextPreview {
    pub(super) text: String,
    pub(super) truncated: bool,
}

/// Provider requests can contain entire papers and captured user inputs.
/// Events cross into the long-lived WebView, so emit only a small UTF-8-safe
/// diagnostic preview and retain the full character count separately.
pub(super) fn event_text_preview(value: &str) -> EventTextPreview {
    if value.len() <= MAX_EVENT_TEXT_PREVIEW_BYTES {
        return EventTextPreview {
            text: value.to_string(),
            truncated: false,
        };
    }
    let mut boundary = MAX_EVENT_TEXT_PREVIEW_BYTES;
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    EventTextPreview {
        text: value[..boundary].to_string(),
        truncated: true,
    }
}

pub(super) fn check_live_artifact_quota(root: &std::path::Path) -> Result<(), String> {
    let mut stack = vec![root.to_path_buf()];
    let mut files = 0usize;
    let mut bytes = 0u64;
    let mut walk = crate::safety::WalkBudget::new_cancellable("Artifact directory scan");
    while let Some(directory) = stack.pop() {
        // The scan races the provider's own file churn: an entry deleted or
        // renamed between listing and stat is not a quota violation, and
        // treating it as one would permanently cancel the pass.
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("Cannot inspect artifact directory: {e}")),
        };
        for entry in entries.flatten() {
            walk.entry()?;
            let file_type = match entry.file_type() {
                Ok(kind) => kind,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(format!("Cannot inspect artifact entry: {e}")),
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                walk.directory()?;
                stack.push(entry.path());
                continue;
            }
            if !file_type.is_file() {
                return Err("Artifact directory contains a non-regular file".to_string());
            }
            let length = match entry.metadata() {
                Ok(metadata) => metadata.len(),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(format!("Cannot inspect artifact size: {e}")),
            };
            files += 1;
            bytes = bytes.saturating_add(length);
            if files > MAX_LIVE_ARTIFACT_FILES
                || length > MAX_LIVE_ARTIFACT_FILE_BYTES
                || bytes > MAX_LIVE_ARTIFACT_BYTES
            {
                return Err(format!(
                    "Artifact safety quota exceeded ({files} files, {} MB)",
                    bytes / 1024 / 1024
                ));
            }
        }
    }
    Ok(())
}

pub(super) async fn supervise_artifact_writes(
    future: BoxedProviderFuture<'_>,
    write_dir: &str,
    pass_key: Option<String>,
) -> Result<String, String> {
    let mut operation = future;
    let directory = std::path::PathBuf::from(write_dir);
    loop {
        if let Ok(result) =
            tokio::time::timeout(Duration::from_millis(250), operation.as_mut()).await
        {
            // A provider can finish between monitor ticks. Always scan once
            // after completion so a last-millisecond oversized or special file
            // cannot bypass the live quota.
            let path = directory.clone();
            let final_check = tokio::task::spawn_blocking(move || check_live_artifact_quota(&path))
                .await
                .map_err(|e| format!("Artifact monitor failed: {e}"))?;
            final_check?;
            return result;
        }
        let path = directory.clone();
        let check = tokio::task::spawn_blocking(move || check_live_artifact_quota(&path))
            .await
            .map_err(|e| format!("Artifact monitor failed: {e}"))?;
        if let Err(error) = check {
            if let Some(key) = &pass_key {
                let _ = crate::commands::cancel_pass(key.clone()).await;
            } else {
                crate::commands::kill_all_children();
            }
            return Err(error);
        }
    }
}
