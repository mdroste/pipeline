use super::*;

#[derive(Debug, Clone, Serialize)]
pub struct EngineInstallProgress {
    pub engine_id: String,
    pub phases: BTreeMap<String, String>,
    pub log_lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EngineStatus {
    pub id: String,
    pub label: String,
    pub description: String,
    pub installed: bool,
    pub version: String,
    pub entry_path: String,
    /// Reserved for status-compatible system engine reporting. Native managed
    /// engines leave this empty.
    pub system_path: String,
    pub est_download_mb: u64,
    pub est_disk_mb: u64,
    /// Size of the whole managed stack (shared across engines), in MB.
    pub managed_stack_mb: u64,
    pub installing: bool,
    pub install_progress: Option<EngineInstallProgress>,
    pub available: bool,
    pub unavailable_reason: String,
}

/// Remove one exact app-owned path without ever following a symlink. Returns
/// false when the path is already absent.
pub(super) fn remove_owned_path(path: &Path) -> Result<bool, String> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("Failed to inspect {}: {error}", path.display())),
    };
    if metadata.file_type().is_symlink() || metadata.is_file() {
        std::fs::remove_file(path)
            .map_err(|error| format!("Failed to remove {}: {error}", path.display()))?;
    } else if metadata.is_dir() {
        std::fs::remove_dir_all(path)
            .map_err(|error| format!("Failed to remove {}: {error}", path.display()))?;
    } else {
        return Err(format!(
            "Refusing to remove unexpected filesystem object {}",
            path.display()
        ));
    }
    Ok(true)
}

pub(super) fn path_present(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

pub(super) fn restore_owned_directory(backup: &Path, target: &Path) -> Result<bool, String> {
    let metadata = match std::fs::symlink_metadata(backup) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!(
                "Failed to inspect recovery backup {}: {error}",
                backup.display()
            ));
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        remove_owned_path(backup)?;
        return Ok(false);
    }
    std::fs::rename(backup, target).map_err(|error| {
        format!(
            "Failed to restore {} from {}: {error}",
            target.display(),
            backup.display()
        )
    })?;
    Ok(true)
}

pub(super) fn remove_children_matching(
    parent: &Path,
    mut matches: impl FnMut(&str) -> bool,
) -> Result<usize, String> {
    let entries = match std::fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(format!(
                "Failed to inspect managed engine directory {}: {error}",
                parent.display()
            ));
        }
    };
    let mut removed = 0usize;
    let mut errors = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                errors.push(format!(
                    "Failed to inspect an entry under {}: {error}",
                    parent.display()
                ));
                continue;
            }
        };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if matches(&name) {
            match remove_owned_path(&entry.path()) {
                Ok(true) => removed += 1,
                Ok(false) => {}
                Err(error) => errors.push(error),
            }
        }
    }
    if errors.is_empty() {
        Ok(removed)
    } else {
        Err(errors.join("; "))
    }
}

pub(super) fn collect_owned_removal(path: &Path, removed: &mut usize, errors: &mut Vec<String>) {
    match remove_owned_path(path) {
        Ok(true) => *removed += 1,
        Ok(false) => {}
        Err(error) => errors.push(error),
    }
}

/// Cheap commit-marker check for startup cleanup. The installer writes this
/// manifest only after the complete runtime and integrity inventory exist;
/// normal engine resolution performs the full cryptographic verification.
pub(super) fn parser_install_committed(parser_root: &Path) -> bool {
    let manifest = parser_root
        .join("versions")
        .join(PADDLE_PARSER_RELEASE)
        .join("install.json");
    match std::fs::symlink_metadata(&manifest) {
        Ok(metadata)
            if metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() <= 2 * 1024 * 1024 => {}
        _ => return false,
    }
    std::fs::read(&manifest)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some_and(|value| {
            value.get("engine").and_then(serde_json::Value::as_str) == Some("paddleocr-vl-parser")
                && value.get("release").and_then(serde_json::Value::as_str)
                    == Some(PADDLE_PARSER_RELEASE)
                && value
                    .get("integrity_sha256")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|digest| {
                        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
                    })
        })
}

/// Clean only installer-owned disposable paths. The caller must hold the
/// engine lock, which makes every staging and backup name here stale.
pub(super) fn cleanup_managed_engine_storage_at(
    native_root: &Path,
    current_parser_safe_to_keep: bool,
) -> Result<usize, String> {
    let mut removed = 0usize;
    let mut errors = Vec::new();

    // The native recognition stack swaps atomically. If activation was
    // interrupted before the new target appeared, put the old target back.
    let paddle_target = native_root.join("paddleocr-vl");
    let paddle_backup = native_root.join(".paddleocr-vl-backup");
    if path_present(&paddle_backup) {
        if path_present(&paddle_target) {
            collect_owned_removal(&paddle_backup, &mut removed, &mut errors);
        } else {
            match restore_owned_directory(&paddle_backup, &paddle_target) {
                Ok(true) => {}
                Ok(false) => removed += 1,
                Err(error) => errors.push(error),
            }
        }
    }
    match remove_children_matching(native_root, |name| {
        name.starts_with(".paddleocr-vl-staging-")
    }) {
        Ok(count) => removed += count,
        Err(error) => errors.push(error),
    }

    let parser_root = native_root.join("paddleocr-parser");
    let versions = parser_root.join("versions");
    let parser_target = versions.join(PADDLE_PARSER_RELEASE);
    let parser_backup = versions.join(format!(".{PADDLE_PARSER_RELEASE}-backup"));
    if path_present(&parser_backup) {
        if current_parser_safe_to_keep {
            collect_owned_removal(&parser_backup, &mut removed, &mut errors);
        } else {
            collect_owned_removal(&parser_target, &mut removed, &mut errors);
            if !path_present(&parser_target) {
                match restore_owned_directory(&parser_backup, &parser_target) {
                    Ok(_) => {}
                    Err(error) => errors.push(error),
                }
            }
        }
    } else if !current_parser_safe_to_keep {
        // A current-release directory with neither a valid runtime nor a
        // rollback copy can only be an interrupted fresh installation.
        collect_owned_removal(&parser_target, &mut removed, &mut errors);
    }

    // Older releases are never executable by this build. Preserve arbitrary
    // user files in `versions`; remove only the installer's release namespace.
    let current_backup_name = format!(".{PADDLE_PARSER_RELEASE}-backup");
    match remove_children_matching(&versions, |name| {
        name != PADDLE_PARSER_RELEASE
            && name != current_backup_name
            && (name.starts_with("paddleocr-")
                || (name.starts_with(".paddleocr-") && name.ends_with("-backup")))
    }) {
        Ok(count) => removed += count,
        Err(error) => errors.push(error),
    }

    // uv's wheel/archive cache and the pre-versioned Python tree are install
    // inputs, not runtime dependencies. They are recreated on demand.
    collect_owned_removal(&parser_root.join("uv-cache"), &mut removed, &mut errors);
    collect_owned_removal(&parser_root.join("python"), &mut removed, &mut errors);

    let uv_runtime = parser_root.join("runtime");
    let uv_target = uv_runtime.join(exe("uv"));
    let uv_backup = uv_runtime.join(format!(".{}-backup", exe("uv")));
    if path_present(&uv_backup) {
        if path_present(&uv_target) {
            collect_owned_removal(&uv_backup, &mut removed, &mut errors);
        } else {
            let backup_is_file = std::fs::symlink_metadata(&uv_backup)
                .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink());
            if backup_is_file {
                if let Err(error) = std::fs::rename(&uv_backup, &uv_target) {
                    errors.push(format!(
                        "Failed to restore {} from {}: {error}",
                        uv_target.display(),
                        uv_backup.display()
                    ));
                }
            } else {
                collect_owned_removal(&uv_backup, &mut removed, &mut errors);
            }
        }
    }
    match remove_children_matching(&uv_runtime, |name| name.starts_with(".uv-staging-")) {
        Ok(count) => removed += count,
        Err(error) => errors.push(error),
    }

    if errors.is_empty() {
        Ok(removed)
    } else {
        Err(errors.join("; "))
    }
}

pub(super) fn cleanup_managed_engine_storage_locked() -> Result<usize, String> {
    let home = pipeline_home()?;
    let native_root = home.join("native");
    let parser_root = native_root.join("paddleocr-parser");
    let parser_backup = parser_root
        .join("versions")
        .join(format!(".{PADDLE_PARSER_RELEASE}-backup"));
    // Only pay for full verification in the rare recovery case where cleanup
    // must choose between a newly activated target and its rollback copy.
    let parser_safe_to_keep = if path_present(&parser_backup) {
        parser_runtime_reusable_for_sidecar_refresh(&parser_root)
    } else {
        parser_install_committed(&parser_root)
    };
    cleanup_managed_engine_storage_at(&native_root, parser_safe_to_keep)
}

/// Startup maintenance for crashes and older releases. Another Pipeline
/// process that is actively installing owns the lock, in which case cleanup is
/// safely deferred to the next startup or install attempt.
pub fn cleanup_stale_managed_engine_storage() -> Result<usize, String> {
    let _guard = acquire_engine_guard(false)?;
    cleanup_managed_engine_storage_locked()
}

/// Start best-effort maintenance without extending the application's startup
/// critical path. The engine lock keeps this disjoint from install/uninstall.
pub fn schedule_stale_managed_engine_cleanup() {
    let _ = std::thread::Builder::new()
        .name("managed-engine-cleanup".to_string())
        .spawn(|| {
            if let Err(error) = cleanup_stale_managed_engine_storage() {
                eprintln!("Managed engine cleanup deferred: {error}");
            }
        });
}

/// Status of every registry engine.
pub fn engine_statuses() -> Vec<EngineStatus> {
    let stack_mb = pipeline_home()
        .map(|home| dir_size(&home.join("native")) / 1_000_000)
        .unwrap_or(0);
    let installing = INSTALL_RUNNING.load(Ordering::Acquire);
    let install_progress = if installing {
        current_install_progress()
    } else {
        None
    };

    ENGINES
        .iter()
        .map(|spec| {
            let (entry, version, support) = match spec.id {
                "paddleocr-vl-parser" => {
                    let parser_status = paddle_full_parser_status().ok();
                    (
                        parser_status.as_ref().map(|status| status.script.clone()),
                        parser_status
                            .as_ref()
                            .map(|status| status.release.clone())
                            .unwrap_or_default(),
                        full_parser_support(),
                    )
                }
                _ => (None, String::new(), Err("Unknown engine".to_string())),
            };
            let unavailable_reason = support.as_ref().err().cloned().unwrap_or_default();
            EngineStatus {
                id: spec.id.to_string(),
                label: spec.label.to_string(),
                description: spec.description.to_string(),
                installed: entry.is_some(),
                version,
                entry_path: entry
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default(),
                system_path: String::new(),
                est_download_mb: spec.est_download_mb,
                est_disk_mb: spec.est_disk_mb,
                managed_stack_mb: stack_mb,
                installing,
                install_progress: install_progress
                    .clone()
                    .filter(|progress| progress.engine_id == spec.id),
                available: support.is_ok(),
                unavailable_reason,
            }
        })
        .collect()
}

// ── Install lifecycle ───────────────────────────────────────────────
