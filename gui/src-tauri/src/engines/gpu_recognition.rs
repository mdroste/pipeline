//! Optional graphics-card recognition for the Full Parser on Windows.
//!
//! The Windows installer carries llama.cpp's Vulkan build, signed with the rest
//! of the app, because Smart App Control refuses the unsigned upstream files.
//! This component adds only the GGUF model files that build reads. Once it is
//! installed, recognition runs through the same managed server as macOS and
//! Linux, and page layout stays on the processor. Researchers turn it off with
//! `paddle_use_gpu`. Extraction never moves between devices on its own.

use super::*;

pub(super) const GPU_RECOGNITION_ID: &str = "paddleocr-vl-gpu";
const GPU_BACKEND: &str = "Vulkan";
const GPU_MANIFEST_ENGINE: &str = "paddleocr-vl-gpu";
pub(super) const GPU_STAGING_PREFIX: &str = ".paddleocr-vl-gpu-staging-";
pub(super) const GPU_BACKUP_NAME: &str = ".paddleocr-vl-gpu-backup";
const DEVICE_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const DEVICE_PROBE_OUTPUT_BYTES: usize = 256 * 1024;

pub(super) fn gpu_recognition_root_at(native_root: &Path) -> PathBuf {
    native_root.join("paddleocr-vl-gpu")
}

fn gpu_recognition_root() -> Result<PathBuf, String> {
    Ok(gpu_recognition_root_at(&pipeline_home()?.join("native")))
}

/// Only Windows installers carry the Vulkan build, so other platforms never
/// list this component.
pub(super) fn gpu_recognition_listed_for(os: &str, arch: &str) -> bool {
    os == "windows" && arch == "x86_64"
}

pub(super) fn gpu_recognition_listed() -> bool {
    gpu_recognition_listed_for(std::env::consts::OS, std::env::consts::ARCH)
}

pub(super) fn gpu_recognition_support_for(
    server_bundled: bool,
    parser_installed: bool,
) -> Result<(), String> {
    if !server_bundled {
        return Err("This copy of Pipeline doesn't include graphics card support.".to_string());
    }
    if !parser_installed {
        return Err("Install PaddleOCR-VL 1.6 Full Parser first.".to_string());
    }
    Ok(())
}

pub(super) fn gpu_recognition_support() -> Result<(), String> {
    gpu_recognition_support_for(
        crate::env::bundled_gpu_server().is_some(),
        paddle_full_parser_status().is_ok(),
    )
}

/// The Vulkan devices `llama-server --list-devices` printed. An empty list
/// means llama.cpp would run on the processor, far slower than built-in
/// recognition.
pub(super) fn vulkan_devices(listing: &str) -> Vec<String> {
    listing
        .lines()
        .map(str::trim)
        .filter(|line| {
            line.strip_prefix(GPU_BACKEND).is_some_and(|rest| {
                let digits = rest.chars().take_while(char::is_ascii_digit).count();
                digits > 0 && rest[digits..].starts_with(':')
            })
        })
        .map(str::to_owned)
        .collect()
}

fn probe_devices(server: &Path) -> Result<Vec<String>, String> {
    let mut probe = std::process::Command::new(server);
    probe.arg("--list-devices");
    let output =
        crate::process::run_bounded(&mut probe, DEVICE_PROBE_TIMEOUT, DEVICE_PROBE_OUTPUT_BYTES)
            .map_err(|error| probe_start_error("llama-server", &error))?;
    if !output.status.success() || output.stdout_truncated {
        return Err(probe_exit_error(
            "The bundled llama-server",
            output.status.code(),
        ));
    }
    Ok(vulkan_devices(&String::from_utf8_lossy(&output.stdout)))
}

fn gpu_manifest_matches(manifest: &serde_json::Value) -> bool {
    let text = |key: &str| manifest.get(key).and_then(serde_json::Value::as_str);
    text("engine") == Some(GPU_MANIFEST_ENGINE)
        && text("model_revision") == Some(PADDLE_MODEL_REVISION)
        && text("model_sha256") == Some(PADDLE_MODEL_SHA256)
        && text("mmproj_sha256") == Some(PADDLE_MMPROJ_SHA256)
        && manifest
            .get("integrity_schema")
            .and_then(serde_json::Value::as_u64)
            == Some(u64::from(INSTALL_INTEGRITY_SCHEMA))
}

/// Cheap commit-marker check. Only a directory the installer finished has a
/// readable manifest.
pub(super) fn gpu_install_committed(root: &Path) -> bool {
    let manifest = root.join("install.json");
    bounded_regular_file(&manifest, 2 * 1024 * 1024)
        && bounded_regular_file(
            &root.join("models").join(PADDLE_MODEL_FILE),
            MAX_PADDLE_MODEL_BYTES,
        )
        && bounded_regular_file(
            &root.join("models").join(PADDLE_MMPROJ_FILE),
            MAX_PADDLE_MODEL_BYTES,
        )
        && std::fs::read(&manifest)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .is_some_and(|manifest| gpu_manifest_matches(&manifest))
}

pub(super) fn gpu_paths_at(root: &Path, server: &Path) -> Option<PaddleEnginePaths> {
    gpu_paths_pinned_at(root, server, PADDLE_MODEL_SHA256, PADDLE_MMPROJ_SHA256)
}

fn gpu_paths_pinned_at(
    root: &Path,
    server: &Path,
    model_sha256: &str,
    mmproj_sha256: &str,
) -> Option<PaddleEnginePaths> {
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("install.json")).ok()?).ok()?;
    if !gpu_manifest_matches(&manifest) {
        return None;
    }
    let models_integrity = manifest
        .get("integrity_sha256")
        .and_then(serde_json::Value::as_str)?;
    let model_relative = format!("models/{PADDLE_MODEL_FILE}");
    let mmproj_relative = format!("models/{PADDLE_MMPROJ_FILE}");
    verify_install_integrity(
        root,
        models_integrity,
        &[
            (model_relative.as_str(), model_sha256),
            (mmproj_relative.as_str(), mmproj_sha256),
        ],
    )
    .ok()?;
    // The server ships with the app, so bind its version here. An app update
    // that changes llama.cpp must not reuse earlier extractions.
    let integrity_sha256 = format!(
        "{:x}",
        Sha256::digest(format!(
            "{models_integrity}:{LLAMA_CPP_VERSION}:{GPU_BACKEND}"
        ))
    );
    Some(PaddleEnginePaths {
        server: server.to_path_buf(),
        model: root.join("models").join(PADDLE_MODEL_FILE),
        mmproj: root.join("models").join(PADDLE_MMPROJ_FILE),
        integrity_sha256,
        gpu: true,
    })
}

/// Resolve graphics-card recognition for an extraction. An absent component
/// means built-in recognition. A damaged one stops the run rather than quietly
/// moving a researcher's documents to the processor.
pub(super) fn resolve_gpu_recognition_at(
    root: &Path,
    server: Option<&Path>,
) -> Result<Option<PaddleEnginePaths>, String> {
    if !path_present(&root.join("install.json")) {
        return Ok(None);
    }
    server
        .and_then(|server| gpu_paths_at(root, server))
        .map(Some)
        .ok_or_else(|| {
            "Graphics card recognition failed verification. Repair it from Settings → PDF extraction, or turn off Use graphics card."
                .to_string()
        })
}

pub(super) fn resolve_gpu_recognition() -> Result<Option<PaddleEnginePaths>, String> {
    if !gpu_recognition_listed() {
        return Ok(None);
    }
    let server = crate::env::bundled_gpu_server();
    resolve_gpu_recognition_at(&gpu_recognition_root()?, server.as_deref())
}

pub(super) fn gpu_recognition_status() -> Option<PathBuf> {
    let root = gpu_recognition_root().ok()?;
    gpu_install_committed(&root).then_some(root)
}

pub(super) async fn install_gpu_recognition(
    app: &crate::emit::EventBus,
    spec: &EngineSpec,
) -> Result<(), String> {
    gpu_recognition_support()?;
    let server =
        crate::env::bundled_gpu_server().ok_or("Graphics card support is missing from this app")?;
    emit_phase(app, spec.id, "gpu", "running");
    let result = install_gpu_files(app, spec, &server).await;
    emit_phase(
        app,
        spec.id,
        "gpu",
        if result.is_ok() { "done" } else { "failed" },
    );
    result
}

async fn install_gpu_files(
    app: &crate::emit::EventBus,
    spec: &EngineSpec,
    server: &Path,
) -> Result<(), String> {
    // Check before the download, so a blocked file or a computer with no usable
    // graphics card doesn't cost researchers 1.9 GB.
    let server_for_task = server.to_path_buf();
    let devices = tokio::task::spawn_blocking(move || probe_devices(&server_for_task))
        .await
        .map_err(|error| format!("Graphics card check failed: {error}"))??;
    if devices.is_empty() {
        return Err(
            "Pipeline couldn't find a graphics card it can use. Update your graphics driver and try again."
                .to_string(),
        );
    }
    log(app, format!("Found {}", devices.join("; ")));

    let native_dir = pipeline_home()?.join("native");
    std::fs::create_dir_all(&native_dir)
        .map_err(|error| format!("Failed to create native engine directory: {error}"))?;
    let staging = tempfile::Builder::new()
        .prefix(GPU_STAGING_PREFIX)
        .tempdir_in(&native_dir)
        .map_err(|error| format!("Failed to create graphics card staging directory: {error}"))?;
    let model_dir = staging.path().join("models");
    std::fs::create_dir_all(&model_dir)
        .map_err(|error| format!("Failed to create model directory: {error}"))?;
    for (filename, sha256, label) in [
        (
            PADDLE_MODEL_FILE,
            PADDLE_MODEL_SHA256,
            "PaddleOCR-VL Q8 model",
        ),
        (
            PADDLE_MMPROJ_FILE,
            PADDLE_MMPROJ_SHA256,
            "PaddleOCR-VL vision projector",
        ),
    ] {
        download_verified(
            app,
            &[paddle_model_url(filename)],
            &model_dir.join(filename),
            sha256,
            MAX_PADDLE_MODEL_BYTES,
            label,
        )
        .await?;
    }
    let integrity_sha256 = write_install_integrity(staging.path(), &["models"])?;
    let manifest = serde_json::json!({
        "engine": GPU_MANIFEST_ENGINE,
        "model_version": "1.6",
        "quantization": "Q8",
        "model_revision": PADDLE_MODEL_REVISION,
        "model_sha256": PADDLE_MODEL_SHA256,
        "mmproj_sha256": PADDLE_MMPROJ_SHA256,
        "integrity_schema": INSTALL_INTEGRITY_SCHEMA,
        "integrity_sha256": integrity_sha256,
    });
    std::fs::write(
        staging.path().join("install.json"),
        serde_json::to_vec_pretty(&manifest)
            .map_err(|error| format!("Failed to serialize install manifest: {error}"))?,
    )
    .map_err(|error| format!("Failed to write install manifest: {error}"))?;
    if gpu_paths_at(staging.path(), server).is_none() {
        return Err("The staged graphics card model is incomplete".to_string());
    }
    if INSTALL_CANCEL.load(Ordering::Acquire) {
        return Err("Install cancelled".to_string());
    }

    let target = gpu_recognition_root()?;
    let backup = native_dir.join(GPU_BACKUP_NAME);
    remove_owned_path(&backup)?;
    if target.exists() {
        std::fs::rename(&target, &backup)
            .map_err(|error| format!("Failed to stage the previous model: {error}"))?;
    }
    let staged_path = staging.keep();
    if let Err(error) = std::fs::rename(&staged_path, &target) {
        if backup.exists() {
            let _ = std::fs::rename(&backup, &target);
        }
        return Err(format!(
            "Failed to activate graphics card recognition: {error}"
        ));
    }
    let _ = remove_owned_path(&backup);
    log(app, format!("{} installed", spec.label));
    Ok(())
}

pub(super) async fn uninstall_gpu_recognition(app: &crate::emit::EventBus) -> Result<(), String> {
    let root = gpu_recognition_root()?;
    tokio::task::spawn_blocking(move || remove_owned_path(&root))
        .await
        .map_err(|error| format!("Managed engine cleanup task failed: {error}"))??;
    log(app, "Graphics card recognition uninstalled");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha256(text: &str) -> String {
        format!("{:x}", Sha256::digest(text.as_bytes()))
    }

    fn committed_component(root: &Path) -> String {
        let models = root.join("models");
        std::fs::create_dir_all(&models).unwrap();
        std::fs::write(models.join(PADDLE_MODEL_FILE), "model").unwrap();
        std::fs::write(models.join(PADDLE_MMPROJ_FILE), "projector").unwrap();
        let integrity = write_install_integrity(root, &["models"]).unwrap();
        std::fs::write(
            root.join("install.json"),
            serde_json::to_vec(&serde_json::json!({
                "engine": GPU_MANIFEST_ENGINE,
                "model_revision": PADDLE_MODEL_REVISION,
                "model_sha256": PADDLE_MODEL_SHA256,
                "mmproj_sha256": PADDLE_MMPROJ_SHA256,
                "integrity_schema": INSTALL_INTEGRITY_SCHEMA,
                "integrity_sha256": integrity,
            }))
            .unwrap(),
        )
        .unwrap();
        integrity
    }

    #[test]
    fn only_windows_x64_lists_graphics_card_recognition() {
        assert!(gpu_recognition_listed_for("windows", "x86_64"));
        assert!(!gpu_recognition_listed_for("windows", "aarch64"));
        assert!(!gpu_recognition_listed_for("macos", "aarch64"));
        assert!(!gpu_recognition_listed_for("linux", "x86_64"));
    }

    #[test]
    fn support_names_what_is_missing() {
        assert!(gpu_recognition_support_for(true, true).is_ok());
        assert!(gpu_recognition_support_for(false, true)
            .unwrap_err()
            .contains("doesn't include"));
        assert!(gpu_recognition_support_for(true, false)
            .unwrap_err()
            .contains("Install PaddleOCR-VL 1.6 Full Parser first"));
    }

    #[test]
    fn only_vulkan_devices_count_as_a_usable_graphics_card() {
        let listing = "load_backend: loaded Vulkan backend from ggml-vulkan.dll\n\
            Available devices:\n  \
            Vulkan0: NVIDIA GeForce RTX 4070 (12282 MiB, 11000 MiB free)\n  \
            Vulkan1: AMD Radeon 780M (8192 MiB, 8000 MiB free)\n  \
            RPC0: remote\n";
        assert_eq!(
            vulkan_devices(listing),
            vec![
                "Vulkan0: NVIDIA GeForce RTX 4070 (12282 MiB, 11000 MiB free)",
                "Vulkan1: AMD Radeon 780M (8192 MiB, 8000 MiB free)",
            ]
        );
        assert!(vulkan_devices("Available devices:\n").is_empty());
        assert!(vulkan_devices("Vulkan backend failed to load\n").is_empty());
    }

    #[test]
    fn an_absent_component_keeps_built_in_recognition() {
        let root = tempfile::tempdir().unwrap();
        let resolved = resolve_gpu_recognition_at(root.path(), None).unwrap();
        assert!(resolved.is_none());
    }

    #[test]
    fn a_verified_component_binds_the_bundled_server_version() {
        let root = tempfile::tempdir().unwrap();
        let models_integrity = committed_component(root.path());
        let server = root.path().join("llama-server.exe");
        let resolved =
            gpu_paths_pinned_at(root.path(), &server, &sha256("model"), &sha256("projector"))
                .unwrap();
        assert!(resolved.gpu);
        assert_eq!(resolved.server, server);
        assert_ne!(resolved.integrity_sha256, models_integrity);
        assert!(gpu_install_committed(root.path()));
    }

    #[test]
    fn a_damaged_component_stops_extraction_instead_of_changing_device() {
        let root = tempfile::tempdir().unwrap();
        committed_component(root.path());
        std::fs::write(root.path().join("models").join(PADDLE_MODEL_FILE), "other").unwrap();
        let server = root.path().join("llama-server.exe");
        assert!(
            gpu_paths_pinned_at(root.path(), &server, &sha256("model"), &sha256("projector"))
                .is_none()
        );
        let error = resolve_gpu_recognition_at(root.path(), Some(&server)).unwrap_err();
        assert!(error.contains("turn off Use graphics card"));
    }

    #[test]
    fn a_missing_bundled_server_stops_extraction() {
        let root = tempfile::tempdir().unwrap();
        committed_component(root.path());
        assert!(resolve_gpu_recognition_at(root.path(), None).is_err());
    }
}
