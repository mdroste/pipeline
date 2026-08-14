use super::*;

/// Count of runs on disk and total bytes they occupy (best-effort walk).
pub fn disk_usage() -> Result<RunsDiskUsage, String> {
    let dir = runs_dir()?;
    let mut count = 0u32;
    let mut bytes = 0u64;
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(RunsDiskUsage { count: 0, bytes: 0 });
    };
    let mut walk = crate::safety::WalkBudget::new("Run storage scan");
    for entry in entries.flatten() {
        walk.entry()?;
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        count += 1;
        walk.directory()?;
        bytes = bytes.saturating_add(dir_size(&entry.path(), &mut walk)?);
    }
    Ok(RunsDiskUsage { count, bytes })
}

fn dir_size(path: &Path, walk: &mut crate::safety::WalkBudget) -> Result<u64, String> {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            walk.entry()?;
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                walk.directory()?;
                stack.push(entry.path());
            } else if let Ok(meta) = entry.metadata() {
                total = total.saturating_add(meta.len());
            }
        }
    }
    Ok(total)
}

fn sized_runs_for_retention() -> Result<Vec<(RunSummary, u64)>, String> {
    let summaries = list_runs()?; // already newest-first
    let root = runs_dir()?;
    let mut sized = Vec::with_capacity(summaries.len());
    for summary in summaries {
        // Each run directory gets its own walk budget. The scan covers only
        // app-owned run data, and one budget across the whole store trips at a
        // few hundred runs — which would silently disable retention forever.
        let mut walk = crate::safety::WalkBudget::new("Run retention scan");
        let bytes = dir_size(&root.join(&summary.run_id), &mut walk)?;
        sized.push((summary, bytes));
    }
    Ok(sized)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RetentionPlan {
    pub(super) preview: RunPurgePreview,
    pub(super) candidates: Vec<String>,
}

fn retention_preview_token(
    keep: usize,
    max_bytes: u64,
    candidates: &[(String, u64)],
    remaining_count: u32,
    remaining_bytes: u64,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"pipeline-run-purge-preview-v1\0");
    digest.update(u64::try_from(keep).unwrap_or(u64::MAX).to_le_bytes());
    digest.update(max_bytes.to_le_bytes());
    digest.update(
        u64::try_from(candidates.len())
            .unwrap_or(u64::MAX)
            .to_le_bytes(),
    );
    for (run_id, bytes) in candidates {
        digest.update(
            u64::try_from(run_id.len())
                .unwrap_or(u64::MAX)
                .to_le_bytes(),
        );
        digest.update(run_id.as_bytes());
        digest.update(bytes.to_le_bytes());
    }
    digest.update(remaining_count.to_le_bytes());
    digest.update(remaining_bytes.to_le_bytes());
    format!("{:x}", digest.finalize())
}

pub(super) fn retention_plan_from_state(
    sized: &[(String, bool, u64)],
    keep: usize,
    max_bytes: u64,
) -> RetentionPlan {
    let mut remaining = sized.len();
    let mut remaining_bytes = sized
        .iter()
        .fold(0u64, |total, (_, _, bytes)| total.saturating_add(*bytes));
    let mut delete_bytes = 0u64;
    let mut candidates = Vec::new();

    for (index, (run_id, protected, bytes)) in sized.iter().enumerate().rev() {
        let count_exceeded = keep > 0 && remaining > keep;
        let bytes_exceeded = max_bytes > 0 && remaining_bytes > max_bytes;
        if !count_exceeded && !bytes_exceeded {
            break;
        }
        // The newest run is never a candidate: a byte limit smaller than a
        // single run must not delete the run the moment it completes.
        if index == 0 || *protected {
            continue;
        }
        candidates.push((run_id.clone(), *bytes));
        delete_bytes = delete_bytes.saturating_add(*bytes);
        remaining = remaining.saturating_sub(1);
        remaining_bytes = remaining_bytes.saturating_sub(*bytes);
    }

    let remaining_count = remaining.min(u32::MAX as usize) as u32;
    let preview_token = retention_preview_token(
        keep,
        max_bytes,
        &candidates,
        remaining_count,
        remaining_bytes,
    );
    RetentionPlan {
        preview: RunPurgePreview {
            delete_count: candidates.len().min(u32::MAX as usize) as u32,
            delete_bytes,
            remaining_count,
            remaining_bytes,
            preview_token,
        },
        candidates: candidates.into_iter().map(|(run_id, _)| run_id).collect(),
    }
}

fn retention_plan_from_sized_runs(
    sized: &[(RunSummary, u64)],
    keep: usize,
    max_bytes: u64,
) -> RetentionPlan {
    let state = sized
        .iter()
        .map(|(summary, bytes)| {
            (
                summary.run_id.clone(),
                summary.status.is_empty() || summary.status == "running",
                *bytes,
            )
        })
        .collect::<Vec<_>>();
    retention_plan_from_state(&state, keep, max_bytes)
}

/// Calculate the exact completed-run set selected by the current retention
/// limits without deleting anything. Pending/running runs are protected.
pub fn preview_purge_runs_with_limits(
    keep: usize,
    max_bytes: u64,
) -> Result<RunPurgePreview, String> {
    let sized = sized_runs_for_retention()?;
    Ok(retention_plan_from_sized_runs(&sized, keep, max_bytes).preview)
}

/// Read a run's annotations (per-issue accept/reject/note), or "{}" if none.
/// Annotations live beside the run in `annotations.json` and never touch the
/// report artifact.
pub fn read_annotations(run_id: &str) -> Result<String, String> {
    validate_run_id(run_id)?;
    load_manifest(run_id)?;
    let path = runs_dir()?.join(run_id).join("annotations.json");
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok("{}".to_string()),
        Err(error) => Err(format!("Cannot inspect annotations: {error}")),
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err("Annotations file cannot be a symlink".to_string())
        }
        Ok(_) => read_utf8_at_most(&path, MAX_ANNOTATION_BYTES, "Annotations file"),
    }
}

/// Write a run's annotations. `content` must be valid JSON and under 1 MB.
pub fn write_annotations(run_id: &str, content: &str) -> Result<(), String> {
    validate_run_id(run_id)?;
    validate_annotation_content(content)?;
    let dir = runs_dir()?.join(run_id);
    // A delayed frontend save must not recreate a run that was just deleted.
    load_manifest(run_id)?;
    let destination = dir.join("annotations.json");
    if fs::symlink_metadata(&destination)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("Annotations file cannot be a symlink".to_string());
    }
    let mut temp = tempfile::NamedTempFile::new_in(&dir)
        .map_err(|e| format!("Cannot create annotation temp file: {e}"))?;
    use std::io::Write as _;
    temp.write_all(content.as_bytes())
        .map_err(|e| format!("Cannot write annotations: {e}"))?;
    temp.flush()
        .map_err(|e| format!("Cannot flush annotations: {e}"))?;
    temp.as_file()
        .sync_all()
        .map_err(|e| format!("Cannot sync annotations: {e}"))?;
    temp.persist(destination)
        .map_err(|e| format!("Cannot save annotations: {}", e.error))?;
    #[cfg(unix)]
    fs::File::open(&dir)
        .and_then(|directory| directory.sync_all())
        .map_err(|e| format!("Cannot sync annotations directory: {e}"))?;
    Ok(())
}

pub(super) fn validate_annotation_content(content: &str) -> Result<(), String> {
    if content.len() > MAX_ANNOTATION_BYTES {
        return Err("Annotations are too large".into());
    }
    let value = serde_json::from_str::<serde_json::Value>(content)
        .map_err(|e| format!("Annotations are not valid JSON: {e}"))?;
    if !value.is_object() {
        return Err("Annotations must be a JSON object".to_string());
    }
    Ok(())
}

/// Delete oldest completed runs until both count and byte limits hold. A zero
/// limit disables that dimension. Pending/running runs are never candidates.
pub fn purge_runs_with_limits(keep: usize, max_bytes: u64) -> Result<usize, String> {
    if keep == 0 && max_bytes == 0 {
        return Ok(0);
    }
    let sized = sized_runs_for_retention()?;
    let plan = retention_plan_from_sized_runs(&sized, keep, max_bytes);
    let mut removed = 0usize;
    for run_id in plan.candidates {
        if delete_run(&run_id).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// Apply a manually confirmed purge only when the current deletion plan still
/// matches the preview the user saw. The caller must hold the pipeline guard
/// across this rescan and deletion.
pub fn purge_runs_with_expected_preview(
    keep: usize,
    max_bytes: u64,
    expected_preview_token: &str,
) -> Result<usize, String> {
    if expected_preview_token.is_empty() {
        return Err("A purge preview is required before deleting run history".to_string());
    }
    let sized = sized_runs_for_retention()?;
    let plan = retention_plan_from_sized_runs(&sized, keep, max_bytes);
    if plan.preview.preview_token != expected_preview_token {
        return Err(
            "Run history changed after the preview. Review the updated deletion summary before purging."
                .to_string(),
        );
    }

    let mut removed = 0usize;
    for run_id in plan.candidates {
        if delete_run(&run_id).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}
