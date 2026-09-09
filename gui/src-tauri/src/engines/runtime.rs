use super::*;

pub(super) fn paddle_paths_at(root: &Path) -> Option<PaddleEnginePaths> {
    let artifact = llama_artifact().ok()?;
    let server = find_file_named(root, &exe("llama-server"))?;
    let model = root.join("models").join(PADDLE_MODEL_FILE);
    let mmproj = root.join("models").join(PADDLE_MMPROJ_FILE);
    let manifest_path = root.join("install.json");
    if !model.is_file() || !mmproj.is_file() || !manifest_path.is_file() {
        return None;
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).ok()?).ok()?;
    if manifest.get("engine").and_then(serde_json::Value::as_str) != Some("paddleocr-vl")
        || manifest
            .get("model_version")
            .and_then(serde_json::Value::as_str)
            != Some("1.6")
        || manifest
            .get("quantization")
            .and_then(serde_json::Value::as_str)
            != Some("Q8")
        || manifest
            .get("model_revision")
            .and_then(serde_json::Value::as_str)
            != Some(PADDLE_MODEL_REVISION)
        || manifest
            .get("llama_cpp_version")
            .and_then(serde_json::Value::as_str)
            != Some(LLAMA_CPP_VERSION)
        || manifest
            .get("runtime_archive")
            .and_then(serde_json::Value::as_str)
            != Some(artifact.filename)
        || manifest
            .get("runtime_sha256")
            .and_then(serde_json::Value::as_str)
            != Some(artifact.sha256)
        || manifest
            .get("model_sha256")
            .and_then(serde_json::Value::as_str)
            != Some(PADDLE_MODEL_SHA256)
        || manifest
            .get("mmproj_sha256")
            .and_then(serde_json::Value::as_str)
            != Some(PADDLE_MMPROJ_SHA256)
        || manifest
            .get("integrity_schema")
            .and_then(serde_json::Value::as_u64)
            != Some(u64::from(INSTALL_INTEGRITY_SCHEMA))
    {
        return None;
    }
    let integrity_sha256 = manifest
        .get("integrity_sha256")
        .and_then(serde_json::Value::as_str)?
        .to_string();
    let model_relative = format!("models/{PADDLE_MODEL_FILE}");
    let mmproj_relative = format!("models/{PADDLE_MMPROJ_FILE}");
    verify_install_integrity(
        root,
        &integrity_sha256,
        &[
            (model_relative.as_str(), PADDLE_MODEL_SHA256),
            (mmproj_relative.as_str(), PADDLE_MMPROJ_SHA256),
        ],
    )
    .ok()?;
    Some(PaddleEnginePaths {
        server,
        model,
        mmproj,
        integrity_sha256,
    })
}

/// Resolve the complete managed Paddle stack. Partial installs are rejected
/// so extraction never starts with a missing projector or runtime library.
pub fn paddle_engine_paths() -> Result<PaddleEnginePaths, String> {
    let root = paddle_root()?;
    paddle_paths_at(&root).ok_or_else(|| {
        "The PaddleOCR-VL Full Parser recognition component is missing. Install or repair the Full Parser from Settings → PDF Extraction."
            .to_string()
    })
}

#[derive(Debug, Clone)]
pub struct PaddleFullParserStatus {
    pub script: PathBuf,
    pub release: String,
}

pub(super) fn bounded_regular_file(path: &Path, max_bytes: u64) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() > 0
            && metadata.len() <= max_bytes
    })
}

pub(super) fn paddle_install_committed(root: &Path) -> bool {
    let Ok(artifact) = llama_artifact() else {
        return false;
    };
    let manifest_path = root.join("install.json");
    if !bounded_regular_file(&manifest_path, 2 * 1024 * 1024)
        || !bounded_regular_file(
            &root.join("models").join(PADDLE_MODEL_FILE),
            MAX_PADDLE_MODEL_BYTES,
        )
        || !bounded_regular_file(
            &root.join("models").join(PADDLE_MMPROJ_FILE),
            MAX_PADDLE_MODEL_BYTES,
        )
        || find_file_named(root, &exe("llama-server")).is_none()
    {
        return false;
    }
    std::fs::read(&manifest_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some_and(|manifest| {
            manifest.get("engine").and_then(serde_json::Value::as_str) == Some("paddleocr-vl")
                && manifest
                    .get("model_version")
                    .and_then(serde_json::Value::as_str)
                    == Some("1.6")
                && manifest
                    .get("model_revision")
                    .and_then(serde_json::Value::as_str)
                    == Some(PADDLE_MODEL_REVISION)
                && manifest
                    .get("llama_cpp_version")
                    .and_then(serde_json::Value::as_str)
                    == Some(LLAMA_CPP_VERSION)
                && manifest
                    .get("runtime_sha256")
                    .and_then(serde_json::Value::as_str)
                    == Some(artifact.sha256)
                && manifest
                    .get("model_sha256")
                    .and_then(serde_json::Value::as_str)
                    == Some(PADDLE_MODEL_SHA256)
                && manifest
                    .get("mmproj_sha256")
                    .and_then(serde_json::Value::as_str)
                    == Some(PADDLE_MMPROJ_SHA256)
        })
}

pub(super) fn paddle_full_parser_status_at(native_root: &Path) -> Option<PaddleFullParserStatus> {
    let parser_root = native_root.join("paddleocr-parser");
    let version_root = parser_root.join("versions").join(PADDLE_PARSER_RELEASE);
    let layout_model = version_root.join("models").join("PP-DocLayoutV3_infer");
    let script = version_root.join("paddle_parser_sidecar.py");
    if !paddle_install_committed(&native_root.join("paddleocr-vl"))
        || !parser_install_committed(&parser_root)
        || !venv_python(&version_root.join("venv")).is_file()
        || !bounded_regular_file(&script, 2 * 1024 * 1024)
        || !bounded_regular_file(&version_root.join("packages.txt"), 2 * 1024 * 1024)
        || !bounded_regular_file(&version_root.join("pylock.toml"), 4 * 1024 * 1024)
        || !bounded_regular_file(&version_root.join("runtime-lock.json"), 1024 * 1024)
        || !bounded_regular_file(
            &version_root.join("integrity.json"),
            MAX_INTEGRITY_MANIFEST_BYTES,
        )
        || !bounded_regular_file(&layout_model.join("inference.json"), 16 * 1024 * 1024)
        || !bounded_regular_file(&layout_model.join("inference.yml"), 16 * 1024 * 1024)
        || !bounded_regular_file(
            &layout_model.join("inference.pdiparams"),
            MAX_LAYOUT_MODEL_EXTRACTED_BYTES,
        )
    {
        return None;
    }
    Some(PaddleFullParserStatus {
        script,
        release: PADDLE_PARSER_RELEASE.to_string(),
    })
}

/// Fast startup/status probe. This checks the installer's commit markers and
/// required file shapes only; extraction still uses `paddle_full_parser_paths`
/// for complete cryptographic verification before executing the parser.
pub fn paddle_full_parser_status() -> Result<PaddleFullParserStatus, String> {
    full_parser_support()?;
    let native_root = pipeline_home()?.join("native");
    paddle_full_parser_status_at(&native_root).ok_or_else(|| {
        "The managed PaddleOCR-VL Full Parser is incomplete. Install or repair it from Settings → PDF Extraction."
            .to_string()
    })
}

pub(super) struct VerifiedParserRuntime {
    pub(super) version_root: PathBuf,
    pub(super) manifest: serde_json::Value,
    pub(super) integrity_sha256: String,
}

/// Verify the installed parser runtime against its own install-time record.
///
/// The release identity — engine, `PADDLE_PARSER_RELEASE`, and the
/// PaddleOCR/PaddlePaddle/Python versions it encodes — must match this
/// binary: the release name is the compatibility contract, and any material
/// runtime change must bump it. Provisioning provenance (`pylock.toml`,
/// `runtime-lock.json`, the package inventory, Python/uv build metadata) is
/// instead verified for self-consistency against the manifest digests and the
/// install integrity inventory recorded when the runtime was provisioned, so
/// a runtime installed by an older app build keeps verifying after the app
/// updates. The sidecar script is deliberately not checked here; callers pin
/// or refresh it against the binary's bundled copy.
pub(super) fn verified_parser_runtime_at(root: &Path) -> Option<VerifiedParserRuntime> {
    let version_root = root.join("versions").join(PADDLE_PARSER_RELEASE);
    let python = venv_python(&version_root.join("venv"));
    let manifest_path = version_root.join("install.json");
    let packages = version_root.join("packages.txt");
    let lock = version_root.join("pylock.toml");
    let runtime_lock = version_root.join("runtime-lock.json");
    let integrity = version_root.join("integrity.json");
    let layout_model = version_root.join("models").join("PP-DocLayoutV3_infer");
    if !python.is_file()
        || !manifest_path.is_file()
        || !packages.is_file()
        || !lock.is_file()
        || !runtime_lock.is_file()
        || !integrity.is_file()
        || !layout_model.join("inference.json").is_file()
        || !layout_model.join("inference.yml").is_file()
        || !layout_model.join("inference.pdiparams").is_file()
    {
        return None;
    }
    if std::fs::metadata(&manifest_path).ok()?.len() > 2 * 1024 * 1024
        || std::fs::metadata(&packages).ok()?.len() > 2 * 1024 * 1024
    {
        return None;
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).ok()?).ok()?;
    if manifest.get("engine").and_then(serde_json::Value::as_str) != Some("paddleocr-vl-parser")
        || manifest.get("release").and_then(serde_json::Value::as_str)
            != Some(PADDLE_PARSER_RELEASE)
        || manifest
            .get("paddleocr")
            .and_then(serde_json::Value::as_str)
            != Some(PADDLE_PARSER_VERSION)
        || manifest
            .get("paddlepaddle")
            .and_then(serde_json::Value::as_str)
            != Some(PADDLE_RUNTIME_VERSION)
        || manifest.get("python").and_then(serde_json::Value::as_str) != Some(PYTHON_VERSION)
        || manifest
            .get("layout_ready")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        || manifest
            .get("offline_model_runtime")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        || manifest
            .get("recognition_backend")
            .and_then(serde_json::Value::as_str)
            != Some("managed llama.cpp")
        || manifest
            .get("integrity_schema")
            .and_then(serde_json::Value::as_u64)
            != Some(u64::from(INSTALL_INTEGRITY_SCHEMA))
    {
        return None;
    }
    let package_bytes = std::fs::read(&packages).ok()?;
    let package_digest = format!("{:x}", Sha256::digest(&package_bytes));
    if manifest
        .get("packages_sha256")
        .and_then(serde_json::Value::as_str)
        != Some(package_digest.as_str())
    {
        return None;
    }
    let lock_digest = sha256_regular_file(&lock, 4 * 1024 * 1024).ok()?;
    if manifest
        .get("lock_sha256")
        .and_then(serde_json::Value::as_str)
        != Some(lock_digest.as_str())
    {
        return None;
    }
    let runtime_lock_digest = sha256_regular_file(&runtime_lock, 1024 * 1024).ok()?;
    if manifest
        .get("runtime_lock_sha256")
        .and_then(serde_json::Value::as_str)
        != Some(runtime_lock_digest.as_str())
    {
        return None;
    }
    let integrity_sha256 = manifest
        .get("integrity_sha256")
        .and_then(serde_json::Value::as_str)?
        .to_string();
    verify_install_integrity(&version_root, &integrity_sha256, &[]).ok()?;
    Some(VerifiedParserRuntime {
        version_root,
        manifest,
        integrity_sha256,
    })
}

pub(super) fn paddle_full_parser_paths_at(root: &Path) -> Option<PaddleFullParserPaths> {
    let verified = verified_parser_runtime_at(root)?;
    let version_root = verified.version_root;
    let script = version_root.join("paddle_parser_sidecar.py");
    if !script.is_file() || std::fs::metadata(&script).ok()?.len() > 2 * 1024 * 1024 {
        return None;
    }
    // The sidecar is code this app executes, so unlike the provisioned
    // runtime it must match this binary's bundled copy exactly.
    let script_bytes = std::fs::read(&script).ok()?;
    let script_digest = format!("{:x}", Sha256::digest(&script_bytes));
    let expected_script_digest = format!("{:x}", Sha256::digest(PADDLE_PARSER_SCRIPT.as_bytes()));
    if script_digest != expected_script_digest {
        return None;
    }
    if verified
        .manifest
        .get("sidecar_sha256")
        .and_then(serde_json::Value::as_str)
        != Some(script_digest.as_str())
        || verified
            .manifest
            .get("sidecar_contract")
            .and_then(serde_json::Value::as_u64)
            != Some(2)
    {
        return None;
    }
    Some(PaddleFullParserPaths {
        python: venv_python(&version_root.join("venv")),
        script,
        model_cache: version_root.join("cache"),
        layout_model: version_root.join("models").join("PP-DocLayoutV3_infer"),
        paddle: paddle_engine_paths().ok()?,
        release: PADDLE_PARSER_RELEASE.to_string(),
        integrity_sha256: verified.integrity_sha256,
        sidecar_sha256: script_digest,
    })
}

pub(super) fn parser_runtime_reusable_for_sidecar_refresh(root: &Path) -> bool {
    verified_parser_runtime_at(root).is_some()
}

fn atomic_replace_runtime_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Managed runtime path has no parent: {}", path.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("Failed to create parser refresh file: {error}"))?;
    temporary
        .write_all(bytes)
        .map_err(|error| format!("Failed to write parser refresh file: {error}"))?;
    temporary
        .flush()
        .map_err(|error| format!("Failed to flush parser refresh file: {error}"))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("Failed to sync parser refresh file: {error}"))?;
    temporary
        .persist(path)
        .map_err(|error| format!("Failed to publish parser refresh file: {}", error.error))?;
    #[cfg(unix)]
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("Failed to sync parser runtime directory: {error}"))?;
    Ok(())
}

/// Caller must hold the managed-engine guard. Each file replacement is atomic;
/// if the process stops between the sidecar and manifest swaps, the next
/// guarded resolution observes the digest mismatch and completes the refresh.
pub(super) fn refresh_paddle_parser_sidecar_locked(root: &Path) -> Result<bool, String> {
    if !parser_runtime_reusable_for_sidecar_refresh(root) {
        return Ok(false);
    }
    let version_root = root.join("versions").join(PADDLE_PARSER_RELEASE);
    let script = version_root.join("paddle_parser_sidecar.py");
    let manifest_path = version_root.join("install.json");
    let expected_digest = format!("{:x}", Sha256::digest(PADDLE_PARSER_SCRIPT.as_bytes()));
    let installed_digest = std::fs::read(&script)
        .ok()
        .map(|bytes| format!("{:x}", Sha256::digest(&bytes)));
    let mut manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&manifest_path)
            .map_err(|error| format!("Failed to read parser manifest: {error}"))?,
    )
    .map_err(|error| format!("Failed to parse parser manifest: {error}"))?;
    if installed_digest.as_deref() == Some(expected_digest.as_str())
        && manifest
            .get("sidecar_sha256")
            .and_then(serde_json::Value::as_str)
            == Some(expected_digest.as_str())
    {
        return Ok(false);
    }

    atomic_replace_runtime_file(&script, PADDLE_PARSER_SCRIPT.as_bytes())?;
    manifest["sidecar_sha256"] = serde_json::Value::String(expected_digest);
    manifest["sidecar_contract"] = serde_json::Value::from(2);
    atomic_replace_runtime_file(
        &manifest_path,
        &serde_json::to_vec_pretty(&manifest)
            .map_err(|error| format!("Failed to serialize parser manifest: {error}"))?,
    )?;
    if paddle_full_parser_paths_at(root).is_none() {
        return Err("The refreshed parser sidecar failed its integrity check".to_string());
    }
    Ok(true)
}

/// Resolve the managed full parser and the native VLM stack it depends on.
pub fn paddle_full_parser_paths() -> Result<PaddleFullParserPaths, String> {
    let (paths, _lease) = paddle_full_parser_lease()?;
    Ok(paths)
}

/// Keep the managed installation fixed through validation and native extraction.
/// The same cross-process gate protects install, repair, cleanup and uninstall.
pub(crate) fn paddle_full_parser_lease() -> Result<
    (
        PaddleFullParserPaths,
        std::sync::Arc<super::installer_io::InstallGuard>,
    ),
    String,
> {
    let lease = std::sync::Arc::new(acquire_engine_guard(false)?);
    full_parser_support()?;
    let root = paddle_parser_root()?;
    // An app update may bundle a newer sidecar than the one on disk. When the
    // pinned runtime still verifies, refresh the sidecar in place so installs
    // provisioned by older app builds keep working without a reinstall; on
    // any failure is surfaced instead of being collapsed into a generic
    // "not installed" message.
    if parser_runtime_reusable_for_sidecar_refresh(&root)
        && paddle_full_parser_paths_at(&root).is_none()
    {
        refresh_paddle_parser_sidecar_locked(&root)?;
    }
    paddle_full_parser_paths_at(&root)
        .map(|paths| (paths, lease))
        .ok_or_else(|| {
            "PaddleOCR-VL Full Parser is not installed or failed verification. \
         Install or repair it from Settings → PDF Extraction."
                .to_string()
        })
}

/// Runtime environment for the private parser process. Model weights and
/// caches remain inside Pipeline's managed root and user-site imports are
/// disabled so a global Python installation cannot alter extraction.
pub fn paddle_full_parser_env(paths: &PaddleFullParserPaths) -> Vec<(String, String)> {
    paddle_full_parser_env_at(&paths.model_cache)
}

pub(super) fn paddle_full_parser_env_at(model_cache: &Path) -> Vec<(String, String)> {
    let managed_home = model_cache.to_string_lossy().to_string();
    vec![
        // pathlib.Path.home() uses USERPROFILE on Windows and HOME on Unix.
        // Point both at the app-owned cache because the managed environment
        // deliberately clears the user's ambient Python configuration.
        ("HOME".to_string(), managed_home.clone()),
        ("USERPROFILE".to_string(), managed_home),
        (
            "PADDLE_PDX_CACHE_HOME".to_string(),
            model_cache.to_string_lossy().to_string(),
        ),
        (
            "HF_HOME".to_string(),
            model_cache
                .join("huggingface")
                .to_string_lossy()
                .to_string(),
        ),
        ("HF_HUB_OFFLINE".to_string(), "1".to_string()),
        (
            "PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK".to_string(),
            "True".to_string(),
        ),
        ("PYTHONNOUSERSITE".to_string(), "1".to_string()),
        ("PYTHONUTF8".to_string(), "1".to_string()),
        ("PIP_DISABLE_PIP_VERSION_CHECK".to_string(), "1".to_string()),
    ]
}

/// Total size in bytes of a directory tree. Best-effort; unreadable entries
/// are skipped.
pub(super) fn dir_size(root: &Path) -> u64 {
    const MAX_ENTRIES: usize = 500_000;
    const MAX_ELAPSED: std::time::Duration = std::time::Duration::from_secs(2);
    let Ok(canonical_root) = root.canonicalize() else {
        return 0;
    };
    let mut total = 0u64;
    let mut stack = vec![canonical_root.clone()];
    let mut visited_dirs = std::collections::HashSet::new();
    let mut entries_seen = 0usize;
    let started = std::time::Instant::now();
    while let Some(dir) = stack.pop() {
        if entries_seen >= MAX_ENTRIES || started.elapsed() >= MAX_ELAPSED {
            break;
        }
        let Ok(canonical_dir) = dir.canonicalize() else {
            continue;
        };
        if !canonical_dir.starts_with(&canonical_root)
            || !visited_dirs.insert(canonical_dir.clone())
        {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&canonical_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            entries_seen += 1;
            if entries_seen > MAX_ENTRIES || started.elapsed() >= MAX_ELAPSED {
                break;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if file_type.is_file() {
                if let Ok(meta) = entry.metadata() {
                    total = total.saturating_add(meta.len());
                }
            }
        }
    }
    total
}

// ── Status ──────────────────────────────────────────────────────────
