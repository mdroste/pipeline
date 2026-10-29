//! Confined copies and non-overwriting export publication.
pub(in crate::commands) const MAX_RUN_EXPORT_FILES: usize = 20_000;
pub(in crate::commands) const MAX_RUN_EXPORT_DIRS: usize = 10_000;
pub(in crate::commands) const MAX_RUN_EXPORT_BYTES: u64 = 4 * 1024 * 1024 * 1024;

fn safe_export_relative_path(relative: &str) -> Result<std::path::PathBuf, String> {
    let path = std::path::Path::new(relative);
    if relative.is_empty()
        || relative.contains('\\')
        || !path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        return Err(format!("Invalid run artifact path '{relative}'"));
    }
    Ok(path.to_path_buf())
}

pub(in crate::commands) fn copy_export_file(
    source_root: &std::path::Path,
    destination_root: &std::path::Path,
    relative: &str,
) -> Result<(), String> {
    let relative = safe_export_relative_path(relative)?;
    let source = source_root.join(&relative);
    let mut input = crate::safety::open_regular_file(&source)
        .map_err(|error| format!("Could not export '{}': {error}", relative.display()))?;
    let target = destination_root.join(&relative);
    let parent = target
        .parent()
        .ok_or_else(|| format!("Export path has no parent: {}", target.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create '{}': {error}", parent.display()))?;
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&target)
        .map_err(|error| format!("Could not create '{}': {error}", target.display()))?;
    std::io::copy(&mut input, &mut output)
        .map_err(|error| format!("Could not copy '{}': {error}", relative.display()))?;
    Ok(())
}

pub(in crate::commands) fn write_export_json(
    destination_root: &std::path::Path,
    relative: &str,
    value: &impl serde::Serialize,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("Could not serialize {relative}: {error}"))?;
    let target = destination_root.join(safe_export_relative_path(relative)?);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create export directory: {error}"))?;
    }
    std::fs::write(&target, bytes)
        .map_err(|error| format!("Could not write '{}': {error}", target.display()))
}

pub(in crate::commands) fn create_export_stage(
    destination: &std::path::Path,
) -> Result<tempfile::TempDir, String> {
    std::fs::create_dir_all(destination).map_err(|error| {
        format!(
            "Failed to create export destination '{}': {error}",
            destination.display()
        )
    })?;
    if !destination.is_dir() {
        return Err(format!(
            "Export destination '{}' is not a directory",
            destination.display()
        ));
    }
    tempfile::Builder::new()
        .prefix(".pipeline-export-staging-")
        .tempdir_in(destination)
        .map_err(|error| format!("Failed to create export staging directory: {error}"))
}

pub(in crate::commands) fn finish_export_stage(
    stage: &tempfile::TempDir,
    destination: &std::path::Path,
    name: &str,
) -> Result<std::path::PathBuf, String> {
    for suffix in 0..1_000usize {
        let candidate_name = if suffix == 0 {
            name.to_string()
        } else {
            format!("{name}-{}", suffix + 1)
        };
        let candidate = destination.join(candidate_name);
        if candidate.exists() {
            continue;
        }
        match std::fs::rename(stage.path(), &candidate) {
            Ok(()) => return Ok(candidate),
            Err(_) if candidate.exists() => continue,
            Err(error) => {
                return Err(format!(
                    "Failed to finalize export '{}': {error}",
                    candidate.display()
                ));
            }
        }
    }
    Err("Could not choose a unique export directory name".to_string())
}

pub(in crate::commands) fn copy_export_tree(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(u64, u64), String> {
    let mut pending = vec![(source.to_path_buf(), destination.to_path_buf())];
    let mut file_count = 0u64;
    let mut directory_count = 1usize;
    let mut total_bytes = 0u64;
    while let Some((source_dir, destination_dir)) = pending.pop() {
        std::fs::create_dir_all(&destination_dir).map_err(|error| {
            format!(
                "Failed to create export directory '{}': {error}",
                destination_dir.display()
            )
        })?;
        let entries = std::fs::read_dir(&source_dir).map_err(|error| {
            format!(
                "Failed to read run directory '{}': {error}",
                source_dir.display()
            )
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("Failed to read run entry: {error}"))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("Failed to inspect run entry: {error}"))?;
            if file_type.is_symlink() {
                return Err(format!(
                    "Run export refused symbolic link '{}'",
                    entry.path().display()
                ));
            }
            let target = destination_dir.join(entry.file_name());
            if file_type.is_dir() {
                directory_count = directory_count.saturating_add(1);
                if directory_count > MAX_RUN_EXPORT_DIRS {
                    return Err(format!(
                        "Run export exceeds the {MAX_RUN_EXPORT_DIRS}-directory safety limit"
                    ));
                }
                pending.push((entry.path(), target));
                continue;
            }
            if !file_type.is_file() {
                return Err(format!(
                    "Run export refused non-regular file '{}'",
                    entry.path().display()
                ));
            }
            file_count = file_count.saturating_add(1);
            if file_count as usize > MAX_RUN_EXPORT_FILES {
                return Err(format!(
                    "Run export exceeds the {MAX_RUN_EXPORT_FILES}-file safety limit"
                ));
            }
            let mut input = crate::safety::open_regular_file(&entry.path())?;
            let size = input
                .metadata()
                .map_err(|error| format!("Failed to inspect run artifact: {error}"))?
                .len();
            if total_bytes.saturating_add(size) > MAX_RUN_EXPORT_BYTES {
                return Err(format!(
                    "Run export exceeds the {} GB safety limit",
                    MAX_RUN_EXPORT_BYTES / 1024 / 1024 / 1024
                ));
            }
            let mut output = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&target)
                .map_err(|error| {
                    format!(
                        "Failed to create exported artifact '{}': {error}",
                        target.display()
                    )
                })?;
            let remaining = MAX_RUN_EXPORT_BYTES.saturating_sub(total_bytes);
            let mut limited = std::io::Read::take(&mut input, remaining.saturating_add(1));
            let copied = std::io::copy(&mut limited, &mut output)
                .map_err(|error| format!("Failed to copy run artifact: {error}"))?;
            if copied != size || copied > remaining {
                return Err(format!(
                    "Run artifact '{}' changed or exceeded limits during export",
                    entry.path().display()
                ));
            }
            total_bytes = total_bytes.saturating_add(copied);
        }
    }
    Ok((file_count, total_bytes))
}
