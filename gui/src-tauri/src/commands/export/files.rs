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
    source_root: &crate::safety::ReadRoot,
    destination_root: &std::path::Path,
    relative: &str,
    expected: Option<&crate::runs::ArtifactEntry>,
    remaining: &mut u64,
) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    let relative = safe_export_relative_path(relative)?;
    let (mut input, size): (Box<dyn Read>, u64) = match source_root.open_regular(&relative) {
        Ok(file) => {
            let size = file.metadata().map_err(|error| error.to_string())?.len();
            (Box::new(file), size)
        }
        // A finished run may keep a listed file compressed; its manifest
        // entry gives the original size, which the copy is checked against.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && expected.is_some() => {
            let mut compressed = relative.clone().into_os_string();
            compressed.push(crate::runs::GZIP_SUFFIX);
            let file = source_root
                .open_regular(std::path::Path::new(&compressed))
                .map_err(|_| format!("Could not export '{}': {error}", relative.display()))?;
            let size = expected.map_or(0, |entry| entry.bytes);
            (Box::new(flate2::read::GzDecoder::new(file)), size)
        }
        Err(error) => {
            return Err(format!(
                "Could not export '{}': {error}",
                relative.display()
            ))
        }
    };
    if size > *remaining || expected.is_some_and(|entry| entry.bytes != size) {
        return Err("Artifact size changed or exceeds the export limit".into());
    }
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
    let mut digest = Sha256::new();
    let mut bytes = 0;
    let mut buffer = [0u8; 65536];
    loop {
        let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        if bytes > size {
            return Err("Artifact grew during export".into());
        }
        digest.update(&buffer[..n]);
        output.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
    }
    let hash = format!("{:x}", digest.finalize());
    if bytes != size
        || expected.is_some_and(|entry| entry.sha256 != hash[..16] && entry.sha256 != hash)
    {
        return Err(format!(
            "Artifact changed since capture: {}",
            relative.display()
        ));
    }
    *remaining -= bytes;
    Ok(())
}

/// Preserve accepted report sizes, but never follow links, block on a special
/// file, or keep reading bytes appended beyond the opened file's initial size.
pub(super) fn read_export_text(
    root: &crate::safety::ReadRoot,
    relative: &str,
    limit: usize,
) -> Result<String, String> {
    use std::io::Read as _;
    let mut file = root
        .open_regular(&safe_export_relative_path(relative)?)
        .map_err(|error| format!("Could not read '{relative}': {error}"))?;
    let size = file.metadata().map_err(|error| error.to_string())?.len();
    if size > limit as u64 {
        return Err("Run artifact exceeds the read limit".into());
    }
    let capacity =
        usize::try_from(size).map_err(|_| format!("'{relative}' is too large to read"))?;
    let mut text = String::new();
    text.try_reserve_exact(capacity)
        .map_err(|_| format!("Not enough memory to read '{relative}'"))?;
    (&mut file)
        .take(size.saturating_add(1))
        .read_to_string(&mut text)
        .map_err(|error| format!("Could not read '{relative}': {error}"))?;
    if text.len() as u64 != size
        || file.metadata().map_err(|error| error.to_string())?.len() != size
    {
        return Err(format!("Run artifact '{relative}' changed during export"));
    }
    Ok(text)
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
    let root = crate::safety::ReadRoot::open(source).map_err(|error| error.to_string())?;
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
            let mut input = root
                .open_regular(
                    entry
                        .path()
                        .strip_prefix(source)
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| format!("Could not open run artifact: {error}"))?;
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
    // The copy shows a finished run as it was saved: compressed files and
    // removed page images come back, and compaction's own files stay behind.
    crate::runs::expand_compressed_copy(source, destination, "")?;
    crate::runs::restore_pages_in_copy(source, destination)?;
    let _ = std::fs::remove_file(destination.join(crate::runs::COMPACTION_RECORD_PATH));
    let _ = std::fs::remove_dir_all(destination.join(".compaction"));
    Ok((file_count, total_bytes))
}

#[cfg(all(test, unix))]
mod source_tests {
    use super::*;
    #[test]
    fn custom_copy_rejects_redirected_parents_and_modified_content() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("x"), "outside").unwrap();
        std::os::unix::fs::symlink(outside.path(), source.path().join("context")).unwrap();
        let mut remaining = 1024;
        assert!(copy_export_file(
            &crate::safety::ReadRoot::open(source.path()).unwrap(),
            target.path(),
            "context/x",
            None,
            &mut remaining
        )
        .is_err());
        assert!(!target.path().join("context/x").exists());
        let entry = crate::runs::ArtifactEntry {
            rel_path: "x".into(),
            bytes: 3,
            sha256: "ba7816bf8f01cfea".into(),
            kind: String::new(),
            group: String::new(),
            label: String::new(),
        };
        std::fs::write(source.path().join("x"), "xyz").unwrap();
        assert!(copy_export_file(
            &crate::safety::ReadRoot::open(source.path()).unwrap(),
            target.path(),
            "x",
            Some(&entry),
            &mut remaining
        )
        .is_err());
    }
}
