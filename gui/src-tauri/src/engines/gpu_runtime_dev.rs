//! Windows debug builds use the same upstream bytes as the release bundler.
//! This module is never compiled into a release build. The complete installed
//! inventory is pinned in the binary, so locally rehashing a changed DLL cannot
//! make it trusted. Published builds require Pipeline's signed manifest instead.
use super::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevelopmentLock {
    version: String,
    archive: Artifact,
    license_text: Artifact,
    development_integrity_sha256: String,
}

#[derive(Deserialize)]
struct Artifact {
    url: String,
    sha256: String,
}

fn lock() -> Result<DevelopmentLock, String> {
    let lock: DevelopmentLock = serde_json::from_str(include_str!(
        "../../../../scripts/release/llama-vulkan-lock.json"
    ))
    .map_err(|error| error.to_string())?;
    if lock.version != LLAMA_CPP_VERSION {
        return Err("Development graphics runtime does not match the parser".into());
    }
    Ok(lock)
}

pub(super) fn is_development_runtime(root: &Path) -> bool {
    // Do not bypass a broken signed installation with development validation.
    !path_present(&root.join("manifest.json")) && !path_present(&root.join("manifest.sig"))
}

pub(super) fn verified_runtime_at(root: &Path) -> Result<GpuServer, String> {
    let identity = lock()?.development_integrity_sha256;
    verify_runtime(root, identity)
}

fn verify_runtime(root: &Path, identity: String) -> Result<GpuServer, String> {
    verify_install_integrity(root, &identity, &[])?;
    Ok(GpuServer {
        path: root.join(SERVER),
        identity,
    })
}

pub(super) async fn stage_runtime(app: &crate::emit::EventBus, stage: &Path) -> Result<(), String> {
    let lock = lock()?;
    let downloads = tempfile::tempdir_in(stage).map_err(|error| error.to_string())?;
    let archive = downloads.path().join("llama.zip");
    let license = downloads.path().join("LICENSE-llama.cpp.txt");
    log(
        app,
        "Downloading graphics card support for the development build",
    );
    download_verified(
        app,
        &[lock.archive.url],
        &archive,
        &lock.archive.sha256,
        MAX_LLAMA_ARCHIVE_BYTES,
        "Graphics card runtime",
    )
    .await?;
    download_verified(
        app,
        &[lock.license_text.url],
        &license,
        &lock.license_text.sha256,
        64 * 1024,
        "Graphics card runtime license",
    )
    .await?;
    let root = stage.to_path_buf();
    tokio::task::spawn_blocking(move || prepare_runtime(&root, &archive, &license))
        .await
        .map_err(|error| error.to_string())??;
    Ok(())
}

fn prepare_runtime(root: &Path, archive: &Path, license: &Path) -> Result<(), String> {
    let lock = lock()?;
    prepare_runtime_with_lock(root, archive, license, &lock)
}

fn prepare_runtime_with_lock(
    root: &Path,
    archive: &Path,
    license: &Path,
    lock: &DevelopmentLock,
) -> Result<(), String> {
    if sha256_regular_file(archive, MAX_LLAMA_ARCHIVE_BYTES)? != lock.archive.sha256
        || sha256_regular_file(license, 64 * 1024)? != lock.license_text.sha256
    {
        return Err("Development graphics runtime download failed verification".into());
    }
    let unpacked = tempfile::tempdir_in(root).map_err(|error| error.to_string())?;
    unpack_llama_archive(archive, unpacked.path(), true)?;
    let runtime = root.join("runtime");
    std::fs::create_dir_all(runtime.join("licenses")).map_err(|error| error.to_string())?;
    for entry in std::fs::read_dir(unpacked.path()).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        let text = name
            .to_str()
            .ok_or("Invalid development runtime filename")?;
        if text == "llama-server.exe" || (text.ends_with(".dll") && text != "ggml-rpc.dll") {
            std::fs::rename(entry.path(), runtime.join(name)).map_err(|error| error.to_string())?;
        }
    }
    std::fs::copy(license, runtime.join("licenses/LICENSE-llama.cpp.txt"))
        .map_err(|error| error.to_string())?;
    write_install_integrity(root, &["runtime"])?;
    verify_runtime(root, lock.development_integrity_sha256.clone())?;
    Ok(())
}

#[cfg(test)]
#[path = "gpu_runtime_dev_tests.rs"]
mod tests;
