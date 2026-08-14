use super::*;

pub(super) const INSTALL_INTEGRITY_SCHEMA: u32 = 1;
pub(super) const MAX_INTEGRITY_ENTRIES: usize = 200_000;
pub(super) const MAX_INTEGRITY_MANIFEST_BYTES: u64 = 32 * 1024 * 1024;
pub(super) const MAX_INTEGRITY_FILE_BYTES: u64 = 2_000_000_000;
pub(super) const MAX_INTEGRITY_TOTAL_BYTES: u64 = 8_000_000_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum IntegrityEntryKind {
    File,
    Symlink,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct IntegrityEntry {
    path: String,
    kind: IntegrityEntryKind,
    size: u64,
    sha256: String,
    permissions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct InstallIntegrityManifest {
    schema_version: u32,
    scopes: Vec<String>,
    entries: Vec<IntegrityEntry>,
}

pub(super) static VERIFIED_INTEGRITY: OnceLock<Mutex<std::collections::HashMap<String, String>>> =
    OnceLock::new();

pub(super) fn integrity_permissions(metadata: &std::fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        metadata.mode() & 0o7777
    }
    #[cfg(not(unix))]
    {
        u32::from(metadata.permissions().readonly())
    }
}

pub(super) fn integrity_relative_path(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "Integrity path escaped its installation root".to_string())?;
    if relative.as_os_str().is_empty()
        || relative.components().any(|component| {
            !matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
    {
        return Err("Integrity manifest contains an unsafe path".to_string());
    }
    let parts = relative
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return Err("Integrity manifest contains an empty path".to_string());
    }
    Ok(parts.join("/"))
}

pub(super) fn symlink_target_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let target = std::fs::read_link(path)
        .map_err(|error| format!("Failed to read managed-runtime symlink: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;
        Ok(target.as_os_str().as_bytes().to_vec())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt as _;
        Ok(target
            .as_os_str()
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect())
    }
}

pub(super) fn ignorable_integrity_metadata(path: &Path, metadata: &std::fs::Metadata) -> bool {
    // Finder may create this inert metadata file after the user opens
    // ~/.pipeline/ from Settings. It is not part of the executable runtime
    // closure, so exclude it while continuing to reject every other unlisted
    // file, symlink, or filesystem object.
    metadata.is_file()
        && !metadata.file_type().is_symlink()
        && path.file_name() == Some(std::ffi::OsStr::new(".DS_Store"))
}

pub(super) fn collect_integrity_entries(
    root: &Path,
    scopes: &[&str],
) -> Result<Vec<IntegrityEntry>, String> {
    let mut stack = scopes
        .iter()
        .map(|scope| root.join(scope))
        .collect::<Vec<_>>();
    let mut entries = Vec::new();
    let mut total_bytes = 0u64;
    while let Some(path) = stack.pop() {
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| {
            format!(
                "Failed to inspect managed-runtime integrity path {}: {error}",
                path.display()
            )
        })?;
        if ignorable_integrity_metadata(&path, &metadata) {
            continue;
        }
        if metadata.is_dir() {
            let mut children = std::fs::read_dir(&path)
                .map_err(|error| format!("Failed to inspect managed runtime: {error}"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("Failed to inspect managed runtime: {error}"))?;
            children.sort_by_key(std::fs::DirEntry::file_name);
            stack.extend(children.into_iter().rev().map(|entry| entry.path()));
            continue;
        }
        if entries.len() >= MAX_INTEGRITY_ENTRIES {
            return Err("Managed runtime exceeds its integrity entry limit".to_string());
        }
        let relative = integrity_relative_path(root, &path)?;
        let permissions = integrity_permissions(&metadata);
        if metadata.file_type().is_symlink() {
            let bytes = symlink_target_bytes(&path)?;
            entries.push(IntegrityEntry {
                path: relative,
                kind: IntegrityEntryKind::Symlink,
                size: bytes.len() as u64,
                sha256: format!("{:x}", Sha256::digest(bytes)),
                permissions,
            });
        } else if metadata.is_file() {
            if metadata.len() > MAX_INTEGRITY_FILE_BYTES {
                return Err(format!(
                    "Managed runtime file exceeds its integrity size limit: {}",
                    path.display()
                ));
            }
            total_bytes = total_bytes
                .checked_add(metadata.len())
                .ok_or("Managed runtime integrity size overflow")?;
            if total_bytes > MAX_INTEGRITY_TOTAL_BYTES {
                return Err("Managed runtime exceeds its integrity size limit".to_string());
            }
            entries.push(IntegrityEntry {
                path: relative,
                kind: IntegrityEntryKind::File,
                size: metadata.len(),
                sha256: sha256_regular_file(&path, MAX_INTEGRITY_FILE_BYTES)?,
                permissions,
            });
        } else {
            return Err(format!(
                "Managed runtime contains an unsupported filesystem object: {}",
                path.display()
            ));
        }
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    if entries.is_empty() || entries.windows(2).any(|pair| pair[0].path == pair[1].path) {
        return Err("Managed runtime integrity inventory is empty or duplicated".to_string());
    }
    Ok(entries)
}

pub(super) fn valid_integrity_path(value: &str) -> bool {
    !value.is_empty()
        && Path::new(value)
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

pub(super) fn collect_integrity_paths(
    root: &Path,
    scopes: &[String],
) -> Result<Vec<String>, String> {
    if scopes.is_empty() || scopes.iter().any(|scope| !valid_integrity_path(scope)) {
        return Err("Runtime integrity manifest has invalid scopes".to_string());
    }
    let mut stack = scopes
        .iter()
        .map(|scope| root.join(scope))
        .collect::<Vec<_>>();
    let mut paths = Vec::new();
    while let Some(path) = stack.pop() {
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|error| format!("Managed runtime integrity inventory changed: {error}"))?;
        if ignorable_integrity_metadata(&path, &metadata) {
            continue;
        }
        if metadata.is_dir() {
            let mut children = std::fs::read_dir(&path)
                .map_err(|error| format!("Failed to inspect managed runtime: {error}"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("Failed to inspect managed runtime: {error}"))?;
            children.sort_by_key(std::fs::DirEntry::file_name);
            stack.extend(children.into_iter().rev().map(|entry| entry.path()));
            continue;
        }
        if paths.len() >= MAX_INTEGRITY_ENTRIES
            || (!metadata.is_file() && !metadata.file_type().is_symlink())
        {
            return Err("Managed runtime integrity inventory is invalid".to_string());
        }
        paths.push(integrity_relative_path(root, &path)?);
    }
    paths.sort();
    if paths.is_empty() || paths.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("Managed runtime integrity inventory is empty or duplicated".to_string());
    }
    Ok(paths)
}

pub(super) fn write_install_integrity(root: &Path, scopes: &[&str]) -> Result<String, String> {
    let manifest = InstallIntegrityManifest {
        schema_version: INSTALL_INTEGRITY_SCHEMA,
        scopes: scopes.iter().map(|scope| (*scope).to_string()).collect(),
        entries: collect_integrity_entries(root, scopes)?,
    };
    let bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("Failed to serialize runtime integrity manifest: {error}"))?;
    if bytes.len() as u64 > MAX_INTEGRITY_MANIFEST_BYTES {
        return Err("Runtime integrity manifest exceeds its size limit".to_string());
    }
    std::fs::write(root.join("integrity.json"), &bytes)
        .map_err(|error| format!("Failed to write runtime integrity manifest: {error}"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub(super) fn metadata_time_nanos(value: std::io::Result<std::time::SystemTime>) -> u128 {
    value
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

pub(super) fn integrity_metadata_digest(
    root: &Path,
    entries: &[IntegrityEntry],
) -> Result<String, String> {
    let mut digest = Sha256::new();
    for entry in entries {
        let path = root.join(&entry.path);
        if path_has_symlink_component(root, path.parent().unwrap_or(root)) {
            return Err("Managed runtime integrity path traverses a symlink".to_string());
        }
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|error| format!("Managed runtime integrity check failed: {error}"))?;
        digest.update(entry.path.as_bytes());
        digest.update(metadata.len().to_le_bytes());
        digest.update(metadata_time_nanos(metadata.modified()).to_le_bytes());
        digest.update(metadata_time_nanos(metadata.created()).to_le_bytes());
        digest.update(integrity_permissions(&metadata).to_le_bytes());
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            digest.update(metadata.dev().to_le_bytes());
            digest.update(metadata.ino().to_le_bytes());
            digest.update(metadata.ctime().to_le_bytes());
            digest.update(metadata.ctime_nsec().to_le_bytes());
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle as _;
            use windows_sys::Win32::Storage::FileSystem::{
                GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
            };

            let file = std::fs::File::open(&path).map_err(|error| {
                format!("Failed to open managed runtime file for identity check: {error}")
            })?;
            let mut info = BY_HANDLE_FILE_INFORMATION::default();
            // SAFETY: `file` owns a valid handle for the duration of the call,
            // and `info` is a writable structure of the required type.
            if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
                return Err(format!(
                    "Failed to identify managed runtime file: {}",
                    std::io::Error::last_os_error()
                ));
            }
            digest.update(info.dwVolumeSerialNumber.to_le_bytes());
            digest.update(info.nFileIndexHigh.to_le_bytes());
            digest.update(info.nFileIndexLow.to_le_bytes());
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(super) fn verify_install_integrity(
    root: &Path,
    expected_manifest_sha256: &str,
    expected_files: &[(&str, &str)],
) -> Result<(), String> {
    let manifest_path = root.join("integrity.json");
    let metadata = std::fs::symlink_metadata(&manifest_path)
        .map_err(|error| format!("Failed to inspect runtime integrity manifest: {error}"))?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_INTEGRITY_MANIFEST_BYTES
    {
        return Err("Runtime integrity manifest is not a bounded regular file".to_string());
    }
    let bytes = std::fs::read(&manifest_path)
        .map_err(|error| format!("Failed to read runtime integrity manifest: {error}"))?;
    if bytes.len() as u64 > MAX_INTEGRITY_MANIFEST_BYTES
        || format!("{:x}", Sha256::digest(&bytes)) != expected_manifest_sha256
    {
        return Err("Runtime integrity manifest does not match install.json".to_string());
    }
    let manifest: InstallIntegrityManifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid runtime integrity manifest: {error}"))?;
    if manifest.schema_version != INSTALL_INTEGRITY_SCHEMA
        || manifest.entries.is_empty()
        || manifest.entries.len() > MAX_INTEGRITY_ENTRIES
        || manifest
            .scopes
            .iter()
            .any(|scope| !valid_integrity_path(scope))
        || manifest.entries.iter().any(|entry| {
            !valid_integrity_path(&entry.path)
                || entry.sha256.len() != 64
                || !entry.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        || manifest
            .entries
            .windows(2)
            .any(|pair| pair[0].path >= pair[1].path)
    {
        return Err("Runtime integrity manifest has an invalid inventory".to_string());
    }
    for (path, sha256) in expected_files {
        if !manifest
            .entries
            .iter()
            .any(|entry| entry.path == *path && entry.sha256 == *sha256)
        {
            return Err(format!("Runtime integrity manifest does not bind {path}"));
        }
    }

    let root = root
        .canonicalize()
        .map_err(|error| format!("Failed to resolve managed runtime: {error}"))?;
    let listed_paths = manifest
        .entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();
    if collect_integrity_paths(&root, &manifest.scopes)? != listed_paths {
        return Err("Managed runtime file inventory changed".to_string());
    }
    let before = integrity_metadata_digest(&root, &manifest.entries)?;
    let cache_key = format!("{}:{expected_manifest_sha256}", root.display());
    if VERIFIED_INTEGRITY
        .get_or_init(|| Mutex::new(std::collections::HashMap::new()))
        .lock()
        .ok()
        .and_then(|cache| cache.get(&cache_key).cloned())
        .as_deref()
        == Some(before.as_str())
    {
        return Ok(());
    }

    let mut total_bytes = 0u64;
    for entry in &manifest.entries {
        let path = root.join(&entry.path);
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|error| format!("Managed runtime integrity check failed: {error}"))?;
        if metadata.len() != entry.size || integrity_permissions(&metadata) != entry.permissions {
            return Err(format!("Managed runtime metadata changed: {}", entry.path));
        }
        let actual = match entry.kind {
            IntegrityEntryKind::File if metadata.is_file() => {
                total_bytes = total_bytes
                    .checked_add(metadata.len())
                    .ok_or("Managed runtime integrity size overflow")?;
                if total_bytes > MAX_INTEGRITY_TOTAL_BYTES {
                    return Err("Managed runtime exceeds its integrity size limit".to_string());
                }
                sha256_regular_file(&path, MAX_INTEGRITY_FILE_BYTES)?
            }
            IntegrityEntryKind::Symlink if metadata.file_type().is_symlink() => {
                format!("{:x}", Sha256::digest(symlink_target_bytes(&path)?))
            }
            _ => {
                return Err(format!(
                    "Managed runtime object type changed: {}",
                    entry.path
                ))
            }
        };
        if actual != entry.sha256 {
            return Err(format!("Managed runtime content changed: {}", entry.path));
        }
    }
    if collect_integrity_paths(&root, &manifest.scopes)? != listed_paths {
        return Err(
            "Managed runtime file inventory changed during its integrity check".to_string(),
        );
    }
    let after = integrity_metadata_digest(&root, &manifest.entries)?;
    if before != after {
        return Err("Managed runtime changed during its integrity check".to_string());
    }
    if let Ok(mut cache) = VERIFIED_INTEGRITY
        .get_or_init(|| Mutex::new(std::collections::HashMap::new()))
        .lock()
    {
        cache.insert(cache_key, after);
    }
    Ok(())
}
