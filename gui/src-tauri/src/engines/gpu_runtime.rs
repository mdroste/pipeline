//! Signed, optional Windows Vulkan runtime. Model downloads remain separate so
//! upgrades and repairs never redownload sound multi-gigabyte model files.
use super::*;
use base64::Engine as _;

#[cfg(all(windows, debug_assertions))]
#[path = "gpu_runtime_dev.rs"]
mod development;

pub(super) const GPU_RUNTIME_DIR: &str = "llama-vulkan";
pub(super) const GPU_RUNTIME_STAGE: &str = ".llama-vulkan-staging-";
pub(super) const GPU_RUNTIME_BACKUP: &str = ".llama-vulkan-backup";
const METADATA_LIMIT: usize = 64 * 1024;
const SIGNATURE_LIMIT: usize = 16 * 1024;
const SERVER: &str = "runtime/llama-server.exe";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeManifest {
    schema_version: u32,
    engine: String,
    runtime_version: String,
    app_version: String,
    platform: String,
    archive: String,
    archive_bytes: u64,
    archive_sha256: String,
    integrity_sha256: String,
    server_sha256: String,
}

pub(super) struct GpuServer {
    pub path: PathBuf,
    pub identity: String,
}

fn public_key() -> Result<String, String> {
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../../tauri.conf.json")).map_err(|e| e.to_string())?;
    config["plugins"]["updater"]["pubkey"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "The graphics runtime verification key is missing".into())
}

fn decode(text: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text.trim())
        .map_err(|_| "Invalid graphics runtime signature encoding")?;
    String::from_utf8(bytes).map_err(|_| "Invalid graphics runtime signature text".into())
}

fn verify_manifest(
    bytes: &[u8],
    signature: &str,
    key: &str,
    release: Option<&str>,
) -> Result<RuntimeManifest, String> {
    if bytes.len() > METADATA_LIMIT || signature.len() > SIGNATURE_LIMIT {
        return Err("Graphics runtime metadata exceeds its limit".into());
    }
    let key = minisign_verify::PublicKey::decode(&decode(key)?).map_err(|e| e.to_string())?;
    let signature =
        minisign_verify::Signature::decode(&decode(signature)?).map_err(|e| e.to_string())?;
    key.verify(bytes, &signature, true)
        .map_err(|e| format!("Graphics runtime signature failed: {e}"))?;
    let manifest: RuntimeManifest = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let stable_version = manifest.app_version.split('.').collect::<Vec<_>>();
    let hash = |value: &str| value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit());
    if manifest.schema_version != 1
        || manifest.engine != GPU_RUNTIME_DIR
        || manifest.runtime_version != LLAMA_CPP_VERSION
        || manifest.platform != "windows-x86_64"
        || stable_version.len() != 3
        || stable_version
            .iter()
            .any(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()))
        || release.is_some_and(|version| version != manifest.app_version)
        || manifest.archive != format!("Pipeline-{}-GPU-Windows-x64.zip", manifest.app_version)
        || manifest.archive_bytes == 0
        || manifest.archive_bytes > MAX_LLAMA_ARCHIVE_BYTES
        || !hash(&manifest.archive_sha256)
        || !hash(&manifest.integrity_sha256)
        || !hash(&manifest.server_sha256)
        || !signature
            .trusted_comment()
            .split('\t')
            .any(|part| part == format!("version:{}", manifest.app_version))
    {
        return Err("Graphics runtime manifest has the wrong release identity".into());
    }
    Ok(manifest)
}

fn read_manifest(root: &Path, key: &str) -> Result<RuntimeManifest, String> {
    let metadata = root.join("manifest.json");
    let signature = root.join("manifest.sig");
    if !bounded_regular_file(&metadata, METADATA_LIMIT as u64)
        || !bounded_regular_file(&signature, SIGNATURE_LIMIT as u64)
    {
        return Err("Graphics runtime metadata is missing or invalid".into());
    }
    verify_manifest(
        &std::fs::read(metadata).map_err(|e| e.to_string())?,
        &std::fs::read_to_string(signature).map_err(|e| e.to_string())?,
        key,
        None,
    )
}

fn verified_runtime_at(root: &Path) -> Result<GpuServer, String> {
    verified_runtime_with_key(root, &public_key()?)
}

fn installed_runtime_at(root: &Path) -> Result<GpuServer, String> {
    #[cfg(all(windows, debug_assertions))]
    if development::is_development_runtime(root) {
        return development::verified_runtime_at(root);
    }
    verified_runtime_at(root)
}

fn verified_runtime_with_key(root: &Path, key: &str) -> Result<GpuServer, String> {
    let manifest = read_manifest(root, key)?;
    verify_install_integrity(
        root,
        &manifest.integrity_sha256,
        &[(SERVER, &manifest.server_sha256)],
    )?;
    if !bounded_regular_file(&root.join(SERVER), MAX_LLAMA_EXTRACTED_BYTES) {
        return Err("Graphics runtime server is missing".into());
    }
    Ok(GpuServer {
        path: root.join(SERVER),
        identity: manifest.integrity_sha256,
    })
}

fn runtime_root() -> Result<PathBuf, String> {
    Ok(pipeline_home()?.join("native").join(GPU_RUNTIME_DIR))
}

pub(super) fn gpu_runtime_committed() -> bool {
    let Ok(root) = runtime_root() else {
        return false;
    };
    if path_present(&root) {
        #[cfg(all(windows, debug_assertions))]
        if development::is_development_runtime(&root) {
            return development::verified_runtime_at(&root).is_ok();
        }
        public_key().is_ok_and(|key| read_manifest(&root, &key).is_ok())
            && bounded_regular_file(&root.join(SERVER), MAX_LLAMA_EXTRACTED_BYTES)
    } else {
        crate::env::bundled_gpu_server().is_some()
    }
}

/// Old installations may still have the signed runtime in their app bundle.
/// A corrupt managed installation is never silently replaced by that fallback.
pub(super) fn gpu_server() -> Result<Option<GpuServer>, String> {
    let root = runtime_root()?;
    if path_present(&root) {
        return installed_runtime_at(&root).map(Some).map_err(|error|
            format!("Graphics runtime failed verification: {error}. Repair Graphics card recognition in Settings → PDF extraction."));
    }
    Ok(crate::env::bundled_gpu_server().map(|path| GpuServer {
        path,
        identity: format!("bundled:{LLAMA_CPP_VERSION}"),
    }))
}

pub(super) fn bind_gpu_runtime(
    mut paths: PaddleEnginePaths,
    server: &GpuServer,
) -> PaddleEnginePaths {
    paths.integrity_sha256 = format!(
        "{:x}",
        Sha256::digest(format!("{}:{}", paths.integrity_sha256, server.identity))
    );
    paths
}

pub(super) async fn ensure_gpu_runtime(app: &crate::emit::EventBus) -> Result<GpuServer, String> {
    if let Ok(Some(server)) = tokio::task::spawn_blocking(gpu_server)
        .await
        .map_err(|e| e.to_string())?
    {
        return Ok(server);
    }
    let native = pipeline_home()?.join("native");
    std::fs::create_dir_all(&native).map_err(|e| e.to_string())?;
    // Runtime-only upgrades reuse models and need only archive/extraction room.
    if let Ok(free) = fs2::available_space(&native) {
        check_install_space(
            free,
            InstallSpacePlan::GpuRecognition,
            PADDLE_BASE_STAGING_HEADROOM_MB,
        )?;
    }
    let stage = tempfile::Builder::new()
        .prefix(GPU_RUNTIME_STAGE)
        .tempdir_in(&native)
        .map_err(|e| e.to_string())?;
    stage_runtime(app, stage.path()).await?;
    let check_root = stage.path().to_path_buf();
    tokio::task::spawn_blocking(move || installed_runtime_at(&check_root))
        .await
        .map_err(|e| e.to_string())??;
    if INSTALL_CANCEL.load(Ordering::Acquire) {
        return Err("Install cancelled".into());
    }
    let target = runtime_root()?;
    let backup = native.join(GPU_RUNTIME_BACKUP);
    remove_owned_path(&backup)?;
    if path_present(&target) {
        rename_when_free(&target, &backup)
            .await
            .map_err(|e| e.to_string())?;
    }
    if let Err(error) = rename_when_free(stage.path(), &target).await {
        if path_present(&backup) {
            let _ = rename_when_free(&backup, &target).await;
        }
        return Err(error.to_string());
    }
    let _ = remove_owned_path(&backup);
    installed_runtime_at(&target)
}

// Debug builds cannot depend on assets from an as-yet unpublished app release.
// The alternative downloader and its trust pins are absent from release builds.
#[cfg(all(windows, debug_assertions))]
async fn stage_runtime(app: &crate::emit::EventBus, stage: &Path) -> Result<(), String> {
    development::stage_runtime(app, stage).await
}

#[cfg(not(all(windows, debug_assertions)))]
async fn stage_runtime(app: &crate::emit::EventBus, stage: &Path) -> Result<(), String> {
    let version = env!("CARGO_PKG_VERSION");
    let base = format!("https://github.com/mdroste/pipeline/releases/download/v{version}");
    let name = format!("Pipeline-{version}-GPU-Windows-x64.json");
    log(app, "Downloading graphics card support");
    let bytes = fetch_engine_metadata(&format!("{base}/{name}"), METADATA_LIMIT)
        .await
        .map_err(|error| format!("Graphics card support for Pipeline {version} could not be downloaded: {error}. The matching release must include its GPU runtime assets."))?;
    let signature = fetch_engine_metadata(&format!("{base}/{name}.sig"), SIGNATURE_LIMIT).await?;
    let signature = String::from_utf8(signature).map_err(|e| e.to_string())?;
    let manifest = verify_manifest(&bytes, &signature, &public_key()?, Some(version))?;
    let archive = stage.join("download.zip");
    download_verified(
        app,
        &[format!("{base}/{}", manifest.archive)],
        &archive,
        &manifest.archive_sha256,
        manifest.archive_bytes,
        "Graphics card runtime",
    )
    .await?;
    let unpack_root = stage.to_path_buf();
    let archive_copy = archive.clone();
    tokio::task::spawn_blocking(move || unpack_llama_archive(&archive_copy, &unpack_root, true))
        .await
        .map_err(|e| e.to_string())??;
    std::fs::remove_file(archive).map_err(|e| e.to_string())?;
    std::fs::write(stage.join("manifest.json"), bytes).map_err(|e| e.to_string())?;
    std::fs::write(stage.join("manifest.sig"), signature).map_err(|e| e.to_string())?;
    Ok(())
}

pub(super) fn cleanup_gpu_runtime_at(native: &Path) -> Result<usize, String> {
    let target = native.join(GPU_RUNTIME_DIR);
    let backup = native.join(GPU_RUNTIME_BACKUP);
    if path_present(&backup) {
        if path_present(&target) {
            remove_owned_path(&backup)?;
        } else {
            restore_owned_directory(&backup, &target)?;
        }
    }
    remove_children_matching(native, |name| name.starts_with(GPU_RUNTIME_STAGE))
}

#[cfg(test)]
#[path = "gpu_runtime_tests.rs"]
mod tests;
