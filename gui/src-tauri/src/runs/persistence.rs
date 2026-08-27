use super::*;

/// Serialize a manifest to `{dir}/manifest.json`.
pub(super) fn write_manifest(dir: &Path, manifest: &RunManifest) -> Result<(), String> {
    use std::io::Write as _;
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|e| format!("Failed to serialize manifest: {e}"))?;
    if json.len() > MAX_MANIFEST_BYTES {
        return Err(format!(
            "Run manifest would be {} bytes; the safety limit is {MAX_MANIFEST_BYTES}",
            json.len()
        ));
    }
    let destination = dir.join("manifest.json");
    let mut temp = tempfile::NamedTempFile::new_in(dir)
        .map_err(|e| format!("Failed to create manifest temp file: {e}"))?;
    temp.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write manifest temp file: {e}"))?;
    temp.flush()
        .map_err(|e| format!("Failed to flush manifest temp file: {e}"))?;
    temp.as_file()
        .sync_all()
        .map_err(|e| format!("Failed to sync manifest temp file: {e}"))?;
    temp.persist(&destination)
        .map_err(|e| format!("Failed to publish manifest: {}", e.error))?;
    #[cfg(unix)]
    {
        // Make the rename durable across a sudden power loss.
        fs::File::open(dir)
            .and_then(|directory| directory.sync_all())
            .map_err(|e| format!("Failed to sync run directory: {e}"))?;
    }
    Ok(())
}

pub fn load_manifest(run_id: &str) -> Result<RunManifest, String> {
    validate_run_id(run_id)?;
    let path = runs_dir()?.join(run_id).join("manifest.json");
    let content = read_utf8_at_most(&path, MAX_MANIFEST_BYTES, "Run manifest")?;
    serde_json::from_str(&content).map_err(|e| format!("Invalid manifest: {e}"))
}

pub(super) fn write_text_atomic(dir: &Path, name: &str, content: &[u8]) -> Result<(), String> {
    use std::io::Write as _;
    let destination = dir.join(name);
    let mut temp = tempfile::NamedTempFile::new_in(dir)
        .map_err(|error| format!("Failed to create recovery temp file: {error}"))?;
    temp.write_all(content)
        .map_err(|error| format!("Failed to write recovery temp file: {error}"))?;
    temp.flush()
        .map_err(|error| format!("Failed to flush recovery temp file: {error}"))?;
    temp.as_file()
        .sync_all()
        .map_err(|error| format!("Failed to sync recovery temp file: {error}"))?;
    temp.persist(destination)
        .map_err(|error| format!("Failed to publish recovered file: {}", error.error))?;
    #[cfg(unix)]
    fs::File::open(dir)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("Failed to sync recovery directory: {error}"))?;
    Ok(())
}

pub(super) fn register_recovered_artifact(
    manifest: &mut RunManifest,
    dir: &Path,
    rel_path: &str,
    label: &str,
    group: &str,
) {
    if manifest.artifacts.len() >= MAX_MANIFEST_ARTIFACTS
        || manifest
            .artifacts
            .iter()
            .any(|artifact| artifact.rel_path == rel_path)
    {
        return;
    }
    let path = dir.join(rel_path);
    let Ok((head, bytes, sha256)) = inspect_file(&path) else {
        return;
    };
    manifest.artifacts.push(ArtifactEntry {
        rel_path: rel_path.to_string(),
        label: label.to_string(),
        kind: detect_kind(rel_path, &head).to_string(),
        bytes,
        sha256,
        group: group.to_string(),
    });
}

pub(super) fn register_recovered_directory(
    manifest: &mut RunManifest,
    dir: &Path,
    rel_dir: &str,
    group: &str,
) {
    let Ok(entries) = fs::read_dir(dir.join(rel_dir)) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            (file_type.is_file() && !file_type.is_symlink()).then_some(entry.path())
        })
        .collect();
    paths.sort();
    let limit = if group == "agent_response" {
        2_000
    } else {
        500
    };
    for path in paths.into_iter().take(limit) {
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let relative = format!("{rel_dir}/{name}");
        let label = if group == "agent_response" {
            agent_response_label(&relative)
        } else {
            path.file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or(name)
                .to_string()
        };
        register_recovered_artifact(manifest, dir, &relative, &label, group);
    }
}

pub(super) fn recovery_failure(manifest: &RunManifest) -> Vec<crate::models::StepFailure> {
    if manifest.status == "done" {
        return Vec::new();
    }
    if manifest.status == "partial" && !manifest.failed_steps.is_empty() {
        return manifest
            .failed_steps
            .iter()
            .enumerate()
            .map(|(index, label)| crate::models::StepFailure {
                step_id: format!("__incomplete_{index}__"),
                step_label: label.clone(),
                phase: String::new(),
                error: "Step did not complete before the run stopped".to_string(),
            })
            .collect();
    }
    let (step_id, step_label, error) = match manifest.status.as_str() {
        "cancelled" => (
            "__run_cancelled__",
            "Run cancelled",
            "Pipeline was cancelled before final report finalization",
        ),
        "failed" => (
            "__run_failed__",
            "Run failed",
            "Pipeline failed before final report finalization",
        ),
        "partial" => (
            "__run_partial__",
            "Run incomplete",
            "Pipeline stopped before its structured report was saved",
        ),
        _ => (
            "__run_interrupted__",
            "Run interrupted",
            "Pipeline exited before final report finalization",
        ),
    };
    vec![crate::models::StepFailure {
        step_id: step_id.to_string(),
        step_label: step_label.to_string(),
        phase: String::new(),
        error: error.to_string(),
    }]
}
