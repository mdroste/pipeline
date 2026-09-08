//! Atomic run checkpoints and the bounded output budget.
use super::paths::step_slug;
use crate::models::{StepFailure, StepOutput};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
pub(super) struct OutputBudget(AtomicUsize);

impl OutputBudget {
    pub(super) fn reserve(&self, output: &StepOutput) -> Result<(), String> {
        let bytes = output.raw_text.len();
        let result = self
            .0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current
                    .checked_add(bytes)
                    .filter(|total| *total <= crate::safety::MAX_RUN_OUTPUT_BYTES)
            });
        result.map(|_| ()).map_err(|_| {
            format!(
                "Run outputs exceed the {} MB safety limit",
                crate::safety::MAX_RUN_OUTPUT_BYTES / 1024 / 1024
            )
        })
    }
}

/// Persist each completed output immediately. Finalization still writes the
/// human-facing numbered markdown artifacts, but these structured checkpoints
/// make an interrupted run recoverable before the final report exists.
pub(super) async fn checkpoint_output(
    write_dir: Option<&str>,
    ordinal: usize,
    output: &StepOutput,
) -> Result<(), String> {
    let Some(write_dir) = write_dir else {
        return Ok(());
    };
    let directory = std::path::PathBuf::from(write_dir).join("checkpoints");
    let destination = directory.join(format!("{ordinal:04}_{}.json", step_slug(&output.step_id)));
    let output = output.clone();
    tokio::task::spawn_blocking(move || {
        use std::io::Write as _;
        std::fs::create_dir_all(&directory)
            .map_err(|error| format!("Failed to create checkpoint directory: {error}"))?;
        let json = serde_json::to_vec_pretty(&output)
            .map_err(|error| format!("Failed to serialize step checkpoint: {error}"))?;
        let mut temp = tempfile::NamedTempFile::new_in(&directory)
            .map_err(|error| format!("Failed to create checkpoint temp file: {error}"))?;
        temp.write_all(&json)
            .map_err(|error| format!("Failed to write step checkpoint: {error}"))?;
        temp.flush()
            .map_err(|error| format!("Failed to flush step checkpoint: {error}"))?;
        temp.as_file()
            .sync_all()
            .map_err(|error| format!("Failed to sync step checkpoint: {error}"))?;
        temp.persist(&destination)
            .map_err(|error| format!("Failed to publish step checkpoint: {}", error.error))?;
        #[cfg(unix)]
        std::fs::File::open(&directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("Failed to sync checkpoint directory: {error}"))?;
        Ok(())
    })
    .await
    .map_err(|error| format!("Step checkpoint task failed: {error}"))?
}

pub(super) async fn checkpoint_outputs(
    _app: &crate::emit::EventBus,
    write_dir: Option<&str>,
    start: usize,
    outputs: &[StepOutput],
) -> Result<(), String> {
    for (offset, output) in outputs.iter().enumerate() {
        checkpoint_output(write_dir, start + offset, output)
            .await
            .map_err(|error| {
                format!(
                    "Could not durably checkpoint completed step '{}': {error}",
                    output.step_label
                )
            })?;
    }
    Ok(())
}

/// Remove checkpoint files at or above the given ordinal. Runs before a
/// wave's final rewrite so its provisional completion-order checkpoints
/// cannot linger next to the final ordering and double-load in recovery.
/// Failure checkpoints (`failure_*.json`) have no numeric prefix and are
/// never touched.
pub(super) async fn remove_checkpoints_from(
    write_dir: Option<&str>,
    start: usize,
) -> Result<(), String> {
    let Some(write_dir) = write_dir else {
        return Ok(());
    };
    let directory = std::path::PathBuf::from(write_dir).join("checkpoints");
    tokio::task::spawn_blocking(move || {
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("Failed to read checkpoint directory: {error}")),
        };
        for entry in entries {
            let entry = entry.map_err(|error| format!("Failed to inspect checkpoint: {error}"))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(ordinal) = name
                .split('_')
                .next()
                .and_then(|prefix| prefix.parse::<usize>().ok())
            else {
                continue;
            };
            if ordinal >= start {
                std::fs::remove_file(entry.path()).map_err(|error| {
                    format!("Failed to replace provisional checkpoint: {error}")
                })?;
            }
        }
        Ok(())
    })
    .await
    .map_err(|error| format!("Checkpoint cleanup task failed: {error}"))?
}

pub(super) async fn checkpoint_failure(
    write_dir: Option<&str>,
    failure: &StepFailure,
) -> Result<(), String> {
    let Some(write_dir) = write_dir else {
        return Ok(());
    };
    let directory = std::path::PathBuf::from(write_dir).join("checkpoints");
    let destination = directory.join(format!("failure_{}.json", step_slug(&failure.step_id)));
    let failure = failure.clone();
    tokio::task::spawn_blocking(move || {
        use std::io::Write as _;
        std::fs::create_dir_all(&directory)
            .map_err(|error| format!("Failed to create checkpoint directory: {error}"))?;
        let json = serde_json::to_vec_pretty(&serde_json::json!({ "failure": failure }))
            .map_err(|error| format!("Failed to serialize failure checkpoint: {error}"))?;
        let mut temp = tempfile::NamedTempFile::new_in(&directory)
            .map_err(|error| format!("Failed to create checkpoint temp file: {error}"))?;
        temp.write_all(&json)
            .map_err(|error| format!("Failed to write failure checkpoint: {error}"))?;
        temp.flush()
            .map_err(|error| format!("Failed to flush failure checkpoint: {error}"))?;
        temp.as_file()
            .sync_all()
            .map_err(|error| format!("Failed to sync failure checkpoint: {error}"))?;
        temp.persist(&destination)
            .map_err(|error| format!("Failed to publish failure checkpoint: {}", error.error))?;
        #[cfg(unix)]
        std::fs::File::open(&directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("Failed to sync checkpoint directory: {error}"))?;
        Ok(())
    })
    .await
    .map_err(|error| format!("Failure checkpoint task failed: {error}"))?
}

pub(super) async fn checkpoint_failures(
    _app: &crate::emit::EventBus,
    write_dir: Option<&str>,
    failures: &[StepFailure],
) -> Result<(), String> {
    for failure in failures {
        checkpoint_failure(write_dir, failure)
            .await
            .map_err(|error| {
                format!(
                    "Could not durably checkpoint failed step '{}': {error}",
                    failure.step_label
                )
            })?;
    }
    Ok(())
}
