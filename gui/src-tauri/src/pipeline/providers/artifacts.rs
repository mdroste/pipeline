//! Live artifact-write supervision.

use super::BoxedProviderFuture;
use std::time::Duration;
pub(in crate::pipeline) const MAX_EVENT_TEXT_PREVIEW_BYTES: usize = 16 * 1024;
pub(in crate::pipeline) const MAX_LIVE_ARTIFACT_FILES: usize = 1_000;
pub(in crate::pipeline) const MAX_LIVE_ARTIFACT_BYTES: u64 = 300 * 1024 * 1024;
pub(in crate::pipeline) const MAX_LIVE_ARTIFACT_FILE_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub(in crate::pipeline) struct EventTextPreview {
    pub(in crate::pipeline) text: String,
    pub(in crate::pipeline) truncated: bool,
}

/// Provider requests can contain entire papers and captured user inputs.
/// Events cross into the long-lived WebView, so emit only a small UTF-8-safe
/// diagnostic preview and retain the full character count separately.
pub(in crate::pipeline) fn event_text_preview(value: &str) -> EventTextPreview {
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

pub(in crate::pipeline) fn check_live_artifact_quota(root: &std::path::Path) -> Result<(), String> {
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

pub(in crate::pipeline) async fn supervise_artifact_writes(
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
            let final_check =
                crate::run_scope::spawn_blocking(move || check_live_artifact_quota(&path))
                    .await
                    .map_err(|e| format!("Artifact monitor failed: {e}"))?;
            final_check.map_err(artifact_limit_error)?;
            return result;
        }
        let path = directory.clone();
        let check = crate::run_scope::spawn_blocking(move || check_live_artifact_quota(&path))
            .await
            .map_err(|e| format!("Artifact monitor failed: {e}"))?;
        if let Err(error) = check {
            // The researcher cancelled nothing, so the pass stays uncancelled
            // and this error is the one the step reports.
            match &pass_key {
                Some(key) => crate::commands::kill_pass_children(key),
                None => crate::commands::kill_all_children(),
            }
            return Err(artifact_limit_error(error));
        }
    }
}

/// A stop noticed during the scan stays a cancellation.
fn artifact_limit_error(error: String) -> String {
    if crate::commands::is_pipeline_cancellation_error(&error) {
        error
    } else {
        format!(
            "{} {error}",
            crate::pipeline::provider_error::ARTIFACT_LIMIT_PREFIX
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn over_the_file_limit() -> tempfile::TempDir {
        let folder = tempfile::tempdir().unwrap();
        std::fs::File::create(folder.path().join("huge.bin"))
            .unwrap()
            .set_len(MAX_LIVE_ARTIFACT_FILE_BYTES + 1)
            .unwrap();
        folder
    }

    #[tokio::test]
    async fn a_live_breach_reports_itself_and_leaves_the_pass_uncancelled() {
        let folder = over_the_file_limit();
        let run = crate::run_scope::ActiveScope::begin("artifact-limit-live-test");
        let error = crate::run_scope::scoped(
            run.scope().clone(),
            supervise_artifact_writes(
                Box::pin(std::future::pending()),
                folder.path().to_str().unwrap(),
                Some("audit".to_string()),
            ),
        )
        .await
        .unwrap_err();
        assert!(error.contains("Artifact safety quota exceeded"), "{error}");
        assert!(crate::pipeline::provider_error::is_non_retryable_error(
            &error
        ));
        assert!(!crate::commands::is_pipeline_cancellation_error(&error));
        assert!(!run.scope().is_pass_cancelled("audit"));
    }

    #[tokio::test]
    async fn a_breach_found_after_the_call_is_reported_the_same_way() {
        let folder = over_the_file_limit();
        let error = supervise_artifact_writes(
            Box::pin(std::future::ready(Ok("done".to_string()))),
            folder.path().to_str().unwrap(),
            None,
        )
        .await
        .unwrap_err();
        assert!(crate::pipeline::provider_error::is_non_retryable_error(
            &error
        ));
    }
}
