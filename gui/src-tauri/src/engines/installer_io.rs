use super::*;

pub(super) static ENGINE_OPERATION_RUNNING: AtomicBool = AtomicBool::new(false);
pub(super) static INSTALL_RUNNING: AtomicBool = AtomicBool::new(false);
pub(super) static INSTALL_CANCEL: AtomicBool = AtomicBool::new(false);
pub(super) static INSTALL_CHILD_PID: Mutex<Option<u32>> = Mutex::new(None);
pub(super) static INSTALL_PROGRESS: OnceLock<Mutex<InstallProgressState>> = OnceLock::new();

pub(super) const INSTALL_PROGRESS_LOG_LINES: usize = 200;

#[derive(Default)]
pub(super) struct InstallProgressState {
    engine_id: String,
    phases: BTreeMap<String, String>,
    log_lines: VecDeque<String>,
}

pub(super) fn install_progress_state() -> &'static Mutex<InstallProgressState> {
    INSTALL_PROGRESS.get_or_init(|| Mutex::new(InstallProgressState::default()))
}

pub(super) fn clear_install_progress() {
    *install_progress_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = InstallProgressState::default();
}

pub(super) fn reset_install_progress(engine_id: &str) {
    let mut progress = install_progress_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    *progress = InstallProgressState {
        engine_id: engine_id.to_string(),
        ..InstallProgressState::default()
    };
}

pub(super) fn current_install_progress() -> Option<EngineInstallProgress> {
    let progress = install_progress_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if progress.engine_id.is_empty() {
        return None;
    }
    Some(EngineInstallProgress {
        engine_id: progress.engine_id.clone(),
        phases: progress.phases.clone(),
        log_lines: progress.log_lines.iter().cloned().collect(),
    })
}

pub(super) fn record_install_log(line: &str) {
    let mut progress = install_progress_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if progress.engine_id.is_empty() {
        return;
    }
    if progress.log_lines.len() == INSTALL_PROGRESS_LOG_LINES {
        progress.log_lines.pop_front();
    }
    progress.log_lines.push_back(line.to_string());
}

pub(super) fn record_install_phase(engine_id: &str, phase: &str, status: &str) {
    let mut progress = install_progress_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if progress.engine_id != engine_id {
        return;
    }
    progress
        .phases
        .insert(phase.to_string(), status.to_string());
}

/// Hard ceiling on a managed-engine download.
pub(super) const INSTALL_STEP_TIMEOUT_SECS: u64 = 3600;
pub(super) const INSTALL_LOG_BYTES_PER_STREAM: usize = 4 * 1024 * 1024;
pub(super) const INSTALL_LOG_LINE_BYTES: usize = 64 * 1024;
pub(super) const INSTALL_OUTPUT_GRACE_SECS: u64 = 2;
pub(super) const INSTALL_OUTPUT_POST_KILL_SECS: u64 = 2;

pub(super) struct InstallGuard {
    lock_file: std::fs::File,
    installing: bool,
}

pub(super) fn acquire_engine_guard(installing: bool) -> Result<InstallGuard, String> {
    use fs2::FileExt as _;
    if ENGINE_OPERATION_RUNNING.swap(true, Ordering::SeqCst) {
        return Err("A managed-engine operation is already running".to_string());
    }
    if installing {
        INSTALL_RUNNING.store(true, Ordering::SeqCst);
    }
    let result = (|| {
        let home = pipeline_home()?;
        std::fs::create_dir_all(&home).map_err(|e| format!("Failed to create ~/.pipeline: {e}"))?;
        let lock_file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(home.join("engine.lock"))
            .map_err(|e| format!("Failed to open engine lock: {e}"))?;
        lock_file
            .try_lock_exclusive()
            .map_err(|e| format!("Another Pipeline process is using managed engines ({e})"))?;
        Ok(InstallGuard {
            lock_file,
            installing,
        })
    })();
    if result.is_err() {
        ENGINE_OPERATION_RUNNING.store(false, Ordering::SeqCst);
        if installing {
            INSTALL_RUNNING.store(false, Ordering::SeqCst);
        }
    } else if installing {
        clear_install_progress();
    }
    result
}

pub(super) fn acquire_install_guard() -> Result<InstallGuard, String> {
    acquire_engine_guard(true)
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        if self.installing {
            kill_install_child();
            INSTALL_RUNNING.store(false, Ordering::SeqCst);
        }
        ENGINE_OPERATION_RUNNING.store(false, Ordering::SeqCst);
        let _ = fs2::FileExt::unlock(&self.lock_file);
    }
}

/// Request cancellation of a running install.
pub fn cancel_install() {
    INSTALL_CANCEL.store(true, Ordering::Release);
    kill_install_child();
}

pub(super) fn kill_install_child() {
    let pid = INSTALL_CHILD_PID
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take();
    if let Some(pid) = pid {
        crate::commands::kill_process(pid);
        crate::commands::unregister_engine_child_pid(pid);
    }
}

pub(super) fn log(app: &crate::emit::EventBus, line: impl Into<String>) {
    let line = line.into();
    record_install_log(&line);
    app.emit_event("engines:log", serde_json::json!({ "line": line }))
        .ok();
}

pub(super) fn emit_phase(app: &crate::emit::EventBus, engine_id: &str, phase: &str, status: &str) {
    record_install_phase(engine_id, phase, status);
    app.emit_event(
        "engines:phase",
        serde_json::json!({ "engine": engine_id, "phase": phase, "status": status }),
    )
    .ok();
}

pub(super) async fn await_install_operation<F, T>(future: F) -> Result<T, String>
where
    F: std::future::Future<Output = T>,
{
    let mut operation = std::pin::pin!(future);
    loop {
        if INSTALL_CANCEL.load(Ordering::Acquire) {
            return Err("Installation cancelled".to_string());
        }
        if let Ok(value) =
            tokio::time::timeout(std::time::Duration::from_millis(200), operation.as_mut()).await
        {
            return Ok(value);
        }
    }
}

pub(super) async fn download_verified(
    app: &crate::emit::EventBus,
    url: &str,
    destination: &Path,
    expected_sha256: &str,
    max_bytes: u64,
    label: &str,
) -> Result<u64, String> {
    log(app, format!("Downloading {label} ({url})"));
    let client = &*crate::pipeline::api_common::HTTP_CLIENT;
    let mut response = await_install_operation(
        client
            .get(url)
            .timeout(std::time::Duration::from_secs(INSTALL_STEP_TIMEOUT_SECS))
            .send(),
    )
    .await?
    .map_err(|error| format!("Failed to download {label}: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "{label} download failed: HTTP {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes)
    {
        return Err(format!("{label} exceeds its download safety limit"));
    }

    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .map_err(|error| format!("Failed to stage {label}: {error}"))?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0u64;
    let mut next_progress = 100_000_000u64;
    while let Some(chunk) = await_install_operation(response.chunk())
        .await?
        .map_err(|error| format!("Failed while downloading {label}: {error}"))?
    {
        downloaded = downloaded
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| format!("{label} size overflow"))?;
        if downloaded > max_bytes {
            return Err(format!("{label} exceeds its download safety limit"));
        }
        hasher.update(&chunk);
        output
            .write_all(&chunk)
            .map_err(|error| format!("Failed to store {label}: {error}"))?;
        if downloaded >= next_progress {
            log(app, format!("{label}: {} MB", downloaded / 1_000_000));
            next_progress = next_progress.saturating_add(100_000_000);
        }
    }
    output
        .sync_all()
        .map_err(|error| format!("Failed to sync {label}: {error}"))?;
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected_sha256 {
        return Err(format!(
            "{label} checksum mismatch (expected {expected_sha256}, got {actual}). \
             Refusing to install."
        ));
    }
    log(
        app,
        format!("Verified {label} ({} MB)", downloaded / 1_000_000),
    );
    Ok(downloaded)
}

pub(super) fn safe_archive_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path.components().all(|component| {
            matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
}

pub(super) fn archive_link_stays_within_root(entry_path: &Path, link_name: &Path) -> bool {
    if link_name.is_absolute() {
        return false;
    }
    let mut depth = entry_path
        .parent()
        .into_iter()
        .flat_map(Path::components)
        .filter(|component| matches!(component, std::path::Component::Normal(_)))
        .count();
    for component in link_name.components() {
        match component {
            std::path::Component::Normal(_) => depth += 1,
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir if depth > 0 => depth -= 1,
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => return false,
        }
    }
    true
}

pub(super) fn path_has_symlink_component(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return true;
    };
    let mut current = root.to_path_buf();
    for component in relative.components() {
        if let std::path::Component::Normal(part) = component {
            current.push(part);
            if std::fs::symlink_metadata(&current)
                .is_ok_and(|metadata| metadata.file_type().is_symlink())
            {
                return true;
            }
        }
    }
    false
}

pub(super) fn unpack_python_archive(
    archive_path: &Path,
    destination: &Path,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(destination)
        .map_err(|error| format!("Failed to create Python staging directory: {error}"))?;
    let file = crate::safety::open_regular_file(archive_path)
        .map_err(|error| format!("Failed to open Python archive: {error}"))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let mut total = 0u64;
    let mut entries_seen = 0usize;
    #[cfg(unix)]
    let mut symlinks = Vec::new();
    for entry in archive
        .entries()
        .map_err(|error| format!("Invalid Python archive: {error}"))?
    {
        entries_seen += 1;
        if entries_seen > MAX_PYTHON_ARCHIVE_ENTRIES {
            return Err("Python archive exceeds its entry limit".to_string());
        }
        let mut entry = entry.map_err(|error| format!("Invalid Python archive entry: {error}"))?;
        let kind = entry.header().entry_type();
        let relative = entry
            .path()
            .map_err(|error| format!("Invalid Python archive path: {error}"))?
            .into_owned();
        if !safe_archive_path(&relative) {
            return Err("Unsafe path in Python archive".to_string());
        }
        let target = destination.join(&relative);
        if kind.is_symlink() {
            let link_name = entry
                .link_name()
                .map_err(|error| format!("Invalid Python symlink: {error}"))?
                .ok_or("Python symlink has no target")?
                .into_owned();
            if !archive_link_stays_within_root(&relative, &link_name) {
                return Err("Unsafe symlink target in Python archive".to_string());
            }
            #[cfg(unix)]
            {
                symlinks.push((target, link_name));
                continue;
            }
            #[cfg(not(unix))]
            return Err("Unexpected symlink in Windows Python archive".to_string());
        }
        if kind.is_dir() {
            std::fs::create_dir_all(&target)
                .map_err(|error| format!("Failed to extract Python: {error}"))?;
            continue;
        }
        if !kind.is_file() {
            return Err("Unsupported entry type in Python archive".to_string());
        }
        total = total
            .checked_add(entry.size())
            .ok_or("Python extracted size overflow")?;
        if total > MAX_PYTHON_EXTRACTED_BYTES {
            return Err("Python archive exceeds its extraction limit".to_string());
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("Failed to extract Python: {error}"))?;
        }
        let mut output = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&target)
            .map_err(|error| format!("Failed to extract Python: {error}"))?;
        std::io::copy(&mut entry, &mut output)
            .map_err(|error| format!("Failed to extract Python: {error}"))?;
        #[cfg(unix)]
        if let Ok(mode) = entry.header().mode() {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode))
                .map_err(|error| format!("Failed to set Python permissions: {error}"))?;
        }
    }
    #[cfg(unix)]
    for (target, link_name) in symlinks {
        if let Some(parent) = target.parent() {
            if path_has_symlink_component(destination, parent) {
                return Err("Python archive nests content beneath a symlink".to_string());
            }
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("Failed to extract Python symlink: {error}"))?;
        }
        if std::fs::symlink_metadata(&target).is_ok() {
            return Err("Python archive contains a duplicate symlink path".to_string());
        }
        std::os::unix::fs::symlink(&link_name, &target)
            .map_err(|error| format!("Failed to extract Python symlink: {error}"))?;
    }
    let python = standalone_python(destination);
    if !python.is_file() {
        return Err("Python interpreter was not found in its release archive".to_string());
    }
    Ok(python)
}

pub(super) fn unpack_layout_model_archive(
    archive_path: &Path,
    destination: &Path,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(destination)
        .map_err(|error| format!("Failed to create layout-model directory: {error}"))?;
    let file = crate::safety::open_regular_file(archive_path)
        .map_err(|error| format!("Failed to open layout-model archive: {error}"))?;
    let mut archive = tar::Archive::new(file);
    let mut total = 0u64;
    let mut entries_seen = 0usize;
    for entry in archive
        .entries()
        .map_err(|error| format!("Invalid layout-model archive: {error}"))?
    {
        entries_seen += 1;
        if entries_seen > 32 {
            return Err("Layout-model archive exceeds its entry limit".to_string());
        }
        let mut entry =
            entry.map_err(|error| format!("Invalid layout-model archive entry: {error}"))?;
        let kind = entry.header().entry_type();
        let relative = entry
            .path()
            .map_err(|error| format!("Invalid layout-model archive path: {error}"))?
            .into_owned();
        if !safe_archive_path(&relative) {
            return Err("Unsafe path in layout-model archive".to_string());
        }
        let target = destination.join(relative);
        if kind.is_dir() {
            std::fs::create_dir_all(&target)
                .map_err(|error| format!("Failed to extract layout model: {error}"))?;
            continue;
        }
        if !kind.is_file() {
            return Err("Unsupported entry type in layout-model archive".to_string());
        }
        total = total
            .checked_add(entry.size())
            .ok_or("Layout-model extracted size overflow")?;
        if total > MAX_LAYOUT_MODEL_EXTRACTED_BYTES {
            return Err("Layout-model archive exceeds its extraction limit".to_string());
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("Failed to extract layout model: {error}"))?;
        }
        let mut output = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&target)
            .map_err(|error| format!("Failed to extract layout model: {error}"))?;
        std::io::copy(&mut entry, &mut output)
            .map_err(|error| format!("Failed to extract layout model: {error}"))?;
    }
    let model = destination.join("PP-DocLayoutV3_infer");
    for required in ["inference.json", "inference.yml", "inference.pdiparams"] {
        if !model.join(required).is_file() {
            return Err(format!("Layout-model archive is missing {required}"));
        }
    }
    Ok(model)
}

pub(super) fn unpack_llama_archive(
    archive_path: &Path,
    destination: &Path,
    zip_archive: bool,
) -> Result<(), String> {
    std::fs::create_dir_all(destination)
        .map_err(|error| format!("Failed to create runtime staging directory: {error}"))?;
    if zip_archive {
        let file = crate::safety::open_regular_file(archive_path)
            .map_err(|error| format!("Failed to open llama.cpp archive: {error}"))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|error| format!("Invalid llama.cpp zip: {error}"))?;
        let mut total = 0u64;
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|error| format!("Invalid llama.cpp zip entry: {error}"))?;
            if entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            {
                return Err("Symlinks are not allowed in a llama.cpp zip archive".to_string());
            }
            let relative = entry
                .enclosed_name()
                .ok_or("Unsafe path in llama.cpp archive")?
                .to_path_buf();
            if !safe_archive_path(&relative) {
                return Err("Unsafe path in llama.cpp archive".to_string());
            }
            let target = destination.join(relative);
            if entry.is_dir() {
                std::fs::create_dir_all(&target)
                    .map_err(|error| format!("Failed to extract llama.cpp: {error}"))?;
                continue;
            }
            total = total
                .checked_add(entry.size())
                .ok_or("llama.cpp extracted size overflow")?;
            if total > MAX_LLAMA_EXTRACTED_BYTES {
                return Err("llama.cpp archive exceeds its extraction limit".to_string());
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("Failed to extract llama.cpp: {error}"))?;
            }
            let mut output = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&target)
                .map_err(|error| format!("Failed to extract llama.cpp: {error}"))?;
            std::io::copy(&mut entry, &mut output)
                .map_err(|error| format!("Failed to extract llama.cpp: {error}"))?;
        }
    } else {
        let file = crate::safety::open_regular_file(archive_path)
            .map_err(|error| format!("Failed to open llama.cpp archive: {error}"))?;
        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        let mut total = 0u64;
        for entry in archive
            .entries()
            .map_err(|error| format!("Invalid llama.cpp archive: {error}"))?
        {
            let mut entry =
                entry.map_err(|error| format!("Invalid llama.cpp archive entry: {error}"))?;
            let kind = entry.header().entry_type();
            let relative = entry
                .path()
                .map_err(|error| format!("Invalid llama.cpp archive path: {error}"))?
                .into_owned();
            if !safe_archive_path(&relative) {
                return Err("Unsafe path in llama.cpp archive".to_string());
            }
            let target = destination.join(relative);
            if kind.is_symlink() {
                let link_name = entry
                    .link_name()
                    .map_err(|error| format!("Invalid llama.cpp symlink: {error}"))?
                    .ok_or("llama.cpp symlink has no target")?
                    .into_owned();
                // Release-library links are simple relative filenames. Reject
                // absolute and parent-traversing targets rather than relying
                // on platform-specific symlink normalization.
                if !safe_archive_path(&link_name) {
                    return Err("Unsafe symlink target in llama.cpp archive".to_string());
                }
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|error| format!("Failed to extract llama.cpp: {error}"))?;
                }
                #[cfg(unix)]
                {
                    std::os::unix::fs::symlink(&link_name, &target)
                        .map_err(|error| format!("Failed to extract llama.cpp symlink: {error}"))?;
                    continue;
                }
                #[cfg(not(unix))]
                return Err("Unexpected symlink in llama.cpp archive".to_string());
            }
            if !kind.is_file() && !kind.is_dir() {
                continue;
            }
            if kind.is_dir() {
                std::fs::create_dir_all(&target)
                    .map_err(|error| format!("Failed to extract llama.cpp: {error}"))?;
                continue;
            }
            total = total
                .checked_add(entry.size())
                .ok_or("llama.cpp extracted size overflow")?;
            if total > MAX_LLAMA_EXTRACTED_BYTES {
                return Err("llama.cpp archive exceeds its extraction limit".to_string());
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("Failed to extract llama.cpp: {error}"))?;
            }
            let mut output = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&target)
                .map_err(|error| format!("Failed to extract llama.cpp: {error}"))?;
            std::io::copy(&mut entry, &mut output)
                .map_err(|error| format!("Failed to extract llama.cpp: {error}"))?;
            #[cfg(unix)]
            if let Ok(mode) = entry.header().mode() {
                use std::os::unix::fs::PermissionsExt as _;
                std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode))
                    .map_err(|error| format!("Failed to set runtime permissions: {error}"))?;
            }
        }
    }

    if find_file_named(destination, &exe("llama-server")).is_none() {
        return Err("llama-server was not found in the downloaded runtime".to_string());
    }
    Ok(())
}

pub(super) fn unpack_uv_archive(
    archive_path: &Path,
    destination: &Path,
    zip_archive: bool,
) -> Result<(), String> {
    if zip_archive {
        let file = crate::safety::open_regular_file(archive_path)
            .map_err(|error| format!("Failed to open uv archive: {error}"))?;
        let mut archive =
            zip::ZipArchive::new(file).map_err(|error| format!("Invalid uv zip: {error}"))?;
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|error| format!("Invalid uv zip entry: {error}"))?;
            if entry.name().replace('\\', "/").rsplit('/').next() != Some("uv.exe") {
                continue;
            }
            if entry.size() > MAX_UV_EXTRACTED_BYTES {
                return Err("uv binary exceeds its extraction limit".to_string());
            }
            let mut output = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(destination)
                .map_err(|error| format!("Failed to stage uv.exe: {error}"))?;
            std::io::copy(&mut entry, &mut output)
                .map_err(|error| format!("Failed to extract uv.exe: {error}"))?;
            output
                .sync_all()
                .map_err(|error| format!("Failed to sync uv.exe: {error}"))?;
            return Ok(());
        }
    } else {
        use std::io::Read as _;
        let file = crate::safety::open_regular_file(archive_path)
            .map_err(|error| format!("Failed to open uv archive: {error}"))?;
        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        for entry in archive
            .entries()
            .map_err(|error| format!("Invalid uv archive: {error}"))?
        {
            let entry = entry.map_err(|error| format!("Invalid uv entry: {error}"))?;
            if !entry.header().entry_type().is_file() {
                continue;
            }
            let path = entry
                .path()
                .map_err(|error| format!("Invalid uv archive path: {error}"))?;
            if path.file_name().and_then(|name| name.to_str()) != Some("uv") {
                continue;
            }
            if entry.size() > MAX_UV_EXTRACTED_BYTES {
                return Err("uv binary exceeds its extraction limit".to_string());
            }
            let mut bytes = Vec::new();
            entry
                .take(MAX_UV_EXTRACTED_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| format!("Failed to extract uv: {error}"))?;
            if bytes.len() as u64 > MAX_UV_EXTRACTED_BYTES {
                return Err("uv binary exceeds its extraction limit".to_string());
            }
            let mut output = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(destination)
                .map_err(|error| format!("Failed to stage uv: {error}"))?;
            output
                .write_all(&bytes)
                .map_err(|error| format!("Failed to write uv: {error}"))?;
            output
                .sync_all()
                .map_err(|error| format!("Failed to sync uv: {error}"))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                std::fs::set_permissions(destination, std::fs::Permissions::from_mode(0o755))
                    .map_err(|error| format!("Failed to set uv permissions: {error}"))?;
            }
            return Ok(());
        }
    }
    Err("uv binary was not found in its release archive".to_string())
}

pub(super) fn uv_binary_path() -> Result<PathBuf, String> {
    Ok(paddle_parser_root()?.join("runtime").join(exe("uv")))
}

pub(super) fn sha256_regular_file(path: &Path, max_bytes: u64) -> Result<String, String> {
    let mut input = crate::safety::open_regular_file(path)
        .map_err(|error| format!("Failed to open {}: {error}", path.display()))?;
    let length = input
        .metadata()
        .map_err(|error| format!("Failed to inspect {}: {error}", path.display()))?
        .len();
    if length > max_bytes {
        return Err(format!("{} exceeds its verification limit", path.display()));
    }
    let mut hasher = Sha256::new();
    let mut bytes = 0u64;
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let count = input
            .read(&mut chunk)
            .map_err(|error| format!("Failed to verify {}: {error}", path.display()))?;
        if count == 0 {
            break;
        }
        bytes = bytes
            .checked_add(count as u64)
            .ok_or_else(|| format!("{} size overflow", path.display()))?;
        if bytes > max_bytes {
            return Err(format!("{} exceeds its verification limit", path.display()));
        }
        hasher.update(&chunk[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub(super) fn managed_path() -> std::ffi::OsString {
    let mut paths = Vec::new();
    if let Some(poppler) = crate::env::bundled_poppler_dir() {
        paths.push(poppler.to_path_buf());
    }
    #[cfg(windows)]
    if let Some(system_root) = std::env::var_os("SystemRoot").map(PathBuf::from) {
        paths.push(system_root.join("System32"));
        paths.push(system_root);
    }
    #[cfg(not(windows))]
    paths.extend(
        ["/usr/bin", "/bin", "/usr/sbin", "/sbin"]
            .into_iter()
            .map(PathBuf::from),
    );
    std::env::join_paths(paths).unwrap_or_default()
}

/// Clear ambient package-manager and Python configuration before starting an
/// app-owned runtime. Only OS identity, locale, temporary-directory, proxy,
/// and certificate variables are deliberately carried across.
pub(crate) fn apply_managed_environment(
    command: &mut std::process::Command,
    environment: &[(String, String)],
) {
    const PASSTHROUGH: &[&str] = &[
        "HOME",
        "USER",
        "LOGNAME",
        "TMPDIR",
        "TMP",
        "TEMP",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "REQUESTS_CA_BUNDLE",
        "CURL_CA_BUNDLE",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
        "SystemRoot",
        "WINDIR",
        "COMSPEC",
        "PATHEXT",
        "NUMBER_OF_PROCESSORS",
    ];
    command.env_clear();
    for key in PASSTHROUGH {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command.env("PATH", managed_path());
    for (key, value) in environment {
        command.env(key, value);
    }
}

pub(super) fn uv_env() -> Result<Vec<(String, String)>, String> {
    let root = paddle_parser_root()?;
    let value = |path: PathBuf| path.to_string_lossy().to_string();
    Ok(vec![
        ("UV_CACHE_DIR".to_string(), value(root.join("uv-cache"))),
        ("UV_SYSTEM_CERTS".to_string(), "1".to_string()),
        ("UV_PYTHON_DOWNLOADS".to_string(), "never".to_string()),
        ("UV_NO_CONFIG".to_string(), "1".to_string()),
        ("UV_NO_PROJECT".to_string(), "1".to_string()),
        ("UV_NO_SOURCES".to_string(), "1".to_string()),
        ("UV_NO_PROGRESS".to_string(), "1".to_string()),
        (
            "UV_DEFAULT_INDEX".to_string(),
            "https://pypi.org/simple".to_string(),
        ),
        ("UV_INDEX_STRATEGY".to_string(), "first-index".to_string()),
        ("UV_KEYRING_PROVIDER".to_string(), "disabled".to_string()),
        (
            "PADDLE_PDX_CACHE_HOME".to_string(),
            value(root.join("models")),
        ),
        ("PYTHONNOUSERSITE".to_string(), "1".to_string()),
    ])
}

pub(super) async fn ensure_uv(app: &crate::emit::EventBus) -> Result<PathBuf, String> {
    let uv_path = uv_binary_path()?;
    let artifact = uv_artifact()?;
    if uv_path.is_file() {
        let verified = sha256_regular_file(&uv_path, MAX_UV_EXTRACTED_BYTES)
            .is_ok_and(|digest| digest == artifact.sha256);
        if verified {
            let mut probe = std::process::Command::new(&uv_path);
            probe.arg("--version");
            apply_managed_environment(&mut probe, &[]);
            if let Ok(output) = crate::process::run_bounded(
                &mut probe,
                std::time::Duration::from_secs(15),
                256 * 1024,
            ) {
                if output.status.success()
                    && String::from_utf8_lossy(&output.stdout).contains(UV_VERSION)
                {
                    return Ok(uv_path);
                }
            }
        } else {
            log(app, "Replacing managed uv after a checksum mismatch");
        }
        if verified {
            log(
                app,
                format!("Replacing managed uv (version {UV_VERSION} required)"),
            );
        }
    }

    let runtime_dir = uv_path.parent().ok_or("Managed uv path has no parent")?;
    std::fs::create_dir_all(runtime_dir)
        .map_err(|error| format!("Failed to create parser runtime directory: {error}"))?;
    let staging = tempfile::Builder::new()
        .prefix(".uv-staging-")
        .tempdir_in(runtime_dir)
        .map_err(|error| format!("Failed to create uv staging directory: {error}"))?;
    let zip_archive = artifact.os == "windows";
    let archive = staging
        .path()
        .join(if zip_archive { "uv.zip" } else { "uv.tar.gz" });
    download_verified(
        app,
        &uv_download_url(artifact),
        &archive,
        artifact.sha256,
        MAX_UV_ARCHIVE_BYTES,
        "managed Python bootstrap",
    )
    .await?;
    let staged_uv = staging.path().join(exe("uv"));
    let archive_for_task = archive.clone();
    let staged_for_task = staged_uv.clone();
    tokio::task::spawn_blocking(move || {
        unpack_uv_archive(&archive_for_task, &staged_for_task, zip_archive)
    })
    .await
    .map_err(|error| format!("uv extraction task failed: {error}"))??;

    let mut probe = std::process::Command::new(&staged_uv);
    probe.arg("--version");
    apply_managed_environment(&mut probe, &[]);
    let output =
        crate::process::run_bounded(&mut probe, std::time::Duration::from_secs(15), 256 * 1024)
            .map_err(|error| format!("Managed uv validation failed: {error}"))?;
    if !output.status.success() || !String::from_utf8_lossy(&output.stdout).contains(UV_VERSION) {
        return Err("The downloaded uv binary failed its version check".to_string());
    }
    let backup = runtime_dir.join(format!(".{}-backup", exe("uv")));
    if backup.exists() {
        std::fs::remove_file(&backup)
            .map_err(|error| format!("Failed to remove old uv backup: {error}"))?;
    }
    if uv_path.exists() {
        std::fs::rename(&uv_path, &backup)
            .map_err(|error| format!("Failed to stage the previous uv runtime: {error}"))?;
    }
    if let Err(error) = std::fs::rename(&staged_uv, &uv_path) {
        if backup.exists() {
            let _ = std::fs::rename(&backup, &uv_path);
        }
        return Err(format!("Failed to activate managed uv: {error}"));
    }
    if backup.exists() {
        let _ = std::fs::remove_file(backup);
    }
    log(app, format!("Installed managed uv {UV_VERSION}"));
    Ok(uv_path)
}
