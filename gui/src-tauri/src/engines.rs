//! Managed local-engine provisioning.
//!
//! Native engines use pinned, SHA-256-verified runtime and model artifacts.
//! Everything remains inside the app-owned toolchain under `~/.pipeline/`:
//!
//! ```text
//! ~/.pipeline/
//! └── native/    native runtimes + GGUF models
//! ```
//!
//! Installs stream progress to the frontend via `engines:phase` /
//! `engines:log` events.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Read as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

// ── Pinned PaddleOCR-VL native stack ───────────────────────────────

const LLAMA_CPP_VERSION: &str = "b9637";
const PADDLE_MODEL_REVISION: &str = "511b09642bb324401f15f97cc23bc67e8f0a291d";
const PADDLE_MODEL_FILE: &str = "PaddleOCR-VL-1.6-GGUF.gguf";
const PADDLE_MMPROJ_FILE: &str = "PaddleOCR-VL-1.6-GGUF-mmproj.gguf";
const PADDLE_MODEL_SHA256: &str =
    "f3ae46ec885050acf4b3d31944431e1fd90d50664fb09126af4a3c050ba14ee8";
const PADDLE_MMPROJ_SHA256: &str =
    "204d757d7610d9b3faab10d506d69e5b244e32bf765e2bab2d0167e65e0a058a";
const MAX_PADDLE_MODEL_BYTES: u64 = 1_100_000_000;
const MAX_LLAMA_ARCHIVE_BYTES: u64 = 500_000_000;
const MAX_LLAMA_EXTRACTED_BYTES: u64 = 1_000_000_000;

// The full parser is an optional, app-owned Python sidecar. uv is used only
// while provisioning that private runtime; extraction never consults a
// system Python or package manager.
const UV_VERSION: &str = "0.11.26";
const PADDLE_PARSER_VERSION: &str = "3.7.0";
const PADDLE_RUNTIME_VERSION: &str = "3.2.1";
const PADDLE_PARSER_RELEASE: &str = "paddleocr-3.7.0-paddle-3.2.1-r1";
const PADDLE_PARSER_SCRIPT: &str = include_str!("paddle_parser_sidecar.py");
const MAX_UV_ARCHIVE_BYTES: u64 = 80_000_000;
const MAX_UV_EXTRACTED_BYTES: u64 = 150_000_000;

struct UvArtifact {
    os: &'static str,
    arch: &'static str,
    target: &'static str,
    sha256: &'static str,
}

// SHA-256 digests are Astral's published uv 0.11.26 release checksums.
const UV_ARTIFACTS: &[UvArtifact] = &[
    UvArtifact {
        os: "macos",
        arch: "aarch64",
        target: "aarch64-apple-darwin",
        sha256: "8f7fbf1708399b921857bce71e1d60f0d3ccf52a30caebc1c1a2f175dce13ab6",
    },
    UvArtifact {
        os: "macos",
        arch: "x86_64",
        target: "x86_64-apple-darwin",
        sha256: "922b460202707dd5f4ccacbadbe7f6a546cc46e82a99bf50ca99a7977a78eddd",
    },
    UvArtifact {
        os: "windows",
        arch: "x86_64",
        target: "x86_64-pc-windows-msvc",
        sha256: "4e1278ede866be6c0bf32d2f466cc6de7a9fb399ecf20c9ce2d186e52424be47",
    },
    UvArtifact {
        os: "linux",
        arch: "x86_64",
        target: "x86_64-unknown-linux-gnu",
        sha256: "6426a73c3837e6e2483ee344cbc00f36394d179afcba6183cb77437e67db4af0",
    },
    UvArtifact {
        os: "linux",
        arch: "aarch64",
        target: "aarch64-unknown-linux-gnu",
        sha256: "befa1a59c91e96eb601b0fd9a97c03dd666f17baba644b2b4db9c59a767e387e",
    },
];

struct LlamaArtifact {
    os: &'static str,
    arch: &'static str,
    filename: &'static str,
    sha256: &'static str,
}

// SHA-256 digests are published on the official llama.cpp b9637 release.
const LLAMA_ARTIFACTS: &[LlamaArtifact] = &[
    LlamaArtifact {
        os: "macos",
        arch: "aarch64",
        filename: "llama-b9637-bin-macos-arm64.tar.gz",
        sha256: "72a93f3e68c31de3e438d462669aad1fcdb423b995e9c41033cc7d27a9a3ac69",
    },
    LlamaArtifact {
        os: "macos",
        arch: "x86_64",
        filename: "llama-b9637-bin-macos-x64.tar.gz",
        sha256: "71743f8db0958e7c266cceb7add7b16aa418a964667e471094aa6ae65b9c8298",
    },
    LlamaArtifact {
        os: "linux",
        arch: "x86_64",
        filename: "llama-b9637-bin-ubuntu-x64.tar.gz",
        sha256: "a50ee14f021a9d8e92e30f622f7e3be1318ee1125bb9a9ba8d2025388df48743",
    },
    LlamaArtifact {
        os: "linux",
        arch: "aarch64",
        filename: "llama-b9637-bin-ubuntu-arm64.tar.gz",
        sha256: "211d9e9ee738698beb7ca271be82661ae2b5da3fbb489cf7d9e4e6ed601be106",
    },
    LlamaArtifact {
        os: "windows",
        arch: "x86_64",
        filename: "llama-b9637-bin-win-cpu-x64.zip",
        sha256: "f7783c2b8c007f95e710ac40f26a24861a80b603b0b739fc54d7c926a4716c1e",
    },
    LlamaArtifact {
        os: "windows",
        arch: "aarch64",
        filename: "llama-b9637-bin-win-cpu-arm64.zip",
        sha256: "db1d3f4c13c08b693f539e100bf6d3a435148b0ffc186b044fdd65d490cc6df7",
    },
];

fn llama_artifact_for(os: &str, arch: &str) -> Option<&'static LlamaArtifact> {
    LLAMA_ARTIFACTS
        .iter()
        .find(|artifact| artifact.os == os && artifact.arch == arch)
}

fn llama_artifact() -> Result<&'static LlamaArtifact, String> {
    llama_artifact_for(std::env::consts::OS, std::env::consts::ARCH).ok_or_else(|| {
        format!(
            "No managed PaddleOCR-VL runtime for {}/{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    })
}

fn llama_download_url(artifact: &LlamaArtifact) -> String {
    format!(
        "https://github.com/ggml-org/llama.cpp/releases/download/{LLAMA_CPP_VERSION}/{}",
        artifact.filename
    )
}

fn paddle_model_url(filename: &str) -> String {
    format!(
        "https://huggingface.co/PaddlePaddle/PaddleOCR-VL-1.6-GGUF/resolve/{PADDLE_MODEL_REVISION}/{filename}?download=true"
    )
}

fn uv_artifact_for(os: &str, arch: &str) -> Option<&'static UvArtifact> {
    UV_ARTIFACTS
        .iter()
        .find(|artifact| artifact.os == os && artifact.arch == arch)
}

fn uv_artifact() -> Result<&'static UvArtifact, String> {
    uv_artifact_for(std::env::consts::OS, std::env::consts::ARCH).ok_or_else(|| {
        format!(
            "No managed Python bootstrap for {}/{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    })
}

fn uv_artifact_name(artifact: &UvArtifact) -> String {
    if artifact.os == "windows" {
        format!("uv-{}.zip", artifact.target)
    } else {
        format!("uv-{}.tar.gz", artifact.target)
    }
}

fn uv_download_url(artifact: &UvArtifact) -> String {
    format!(
        "https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/{}",
        uv_artifact_name(artifact)
    )
}

fn full_parser_support_for(os: &str, arch: &str) -> Result<(), String> {
    if os == "macos" && arch == "x86_64" {
        return Err(
            "The official PaddlePaddle 3.2.1 runtime has no Intel macOS wheel. Use the fast PaddleOCR-VL engine on this Mac."
                .to_string(),
        );
    }
    match (os, arch) {
        ("macos", "aarch64")
        | ("windows", "x86_64")
        | ("linux", "x86_64")
        | ("linux", "aarch64") => Ok(()),
        _ => Err(format!(
            "The full PaddleOCR-VL parser is not packaged for {os}/{arch}"
        )),
    }
}

fn full_parser_support() -> Result<(), String> {
    full_parser_support_for(std::env::consts::OS, std::env::consts::ARCH)
}

// ── Engine registry ─────────────────────────────────────────────────

pub struct EngineSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    /// Rough total download (packages + model weights), for the UI and the
    /// pre-install disk check.
    pub est_download_mb: u64,
    pub est_disk_mb: u64,
}

pub const ENGINES: &[EngineSpec] = &[
    EngineSpec {
        id: "paddleocr-vl",
        label: "PaddleOCR-VL 1.6 Q8 (fast)",
        description: "Compact local page transcription using the official PaddleOCR-VL 1.6 Q8 \
                      GGUF model and a managed llama.cpp runtime. About 1.9 GB; no Python, \
                      containers, API calls, or usage fees. Apache-2.0 model and MIT runtime.",
        est_download_mb: 1900,
        est_disk_mb: 2300,
    },
    EngineSpec {
        id: "paddleocr-vl-parser",
        label: "PaddleOCR-VL 1.6 Full Parser",
        description: "Adds the official PaddleOCR layout client, PP-DocLayoutV3, structured \
                      reading order, title hierarchy, formula metadata, and cross-page table \
                      reconstruction. Reuses the managed Q8 model and llama.cpp server; no \
                      system Python, pip, Conda, or Docker is required.",
        // Includes the base Q8 engine when it is not already present.
        est_download_mb: 2900,
        est_disk_mb: 3800,
    },
];

fn engine(id: &str) -> Result<&'static EngineSpec, String> {
    ENGINES
        .iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("Unknown engine '{id}'"))
}

// ── Paths and environment ───────────────────────────────────────────

fn pipeline_home() -> Result<PathBuf, String> {
    Ok(dirs::home_dir()
        .ok_or("Cannot determine home directory")?
        .join(".pipeline"))
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

#[derive(Debug, Clone)]
pub struct PaddleEnginePaths {
    pub server: PathBuf,
    pub model: PathBuf,
    pub mmproj: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PaddleFullParserPaths {
    pub python: PathBuf,
    pub script: PathBuf,
    pub model_cache: PathBuf,
    pub paddle: PaddleEnginePaths,
    pub release: String,
}

fn paddle_root() -> Result<PathBuf, String> {
    Ok(pipeline_home()?.join("native").join("paddleocr-vl"))
}

fn paddle_parser_root() -> Result<PathBuf, String> {
    Ok(pipeline_home()?.join("native").join("paddleocr-parser"))
}

fn paddle_parser_version_root() -> Result<PathBuf, String> {
    Ok(paddle_parser_root()?
        .join("versions")
        .join(PADDLE_PARSER_RELEASE))
}

fn venv_python(venv: &Path) -> PathBuf {
    if cfg!(windows) {
        venv.join("Scripts").join("python.exe")
    } else {
        venv.join("bin").join("python")
    }
}

fn find_file_named(root: &Path, name: &str) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    let mut visited = 0usize;
    while let Some(dir) = stack.pop() {
        if visited >= 10_000 {
            return None;
        }
        let entries = std::fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            visited += 1;
            let file_type = entry.file_type().ok()?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if file_type.is_file()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|value| value == name)
            {
                return Some(entry.path());
            }
        }
    }
    None
}

fn paddle_paths_at(root: &Path) -> Option<PaddleEnginePaths> {
    let server = find_file_named(root, &exe("llama-server"))?;
    let model = root.join("models").join(PADDLE_MODEL_FILE);
    let mmproj = root.join("models").join(PADDLE_MMPROJ_FILE);
    if !model.is_file() || !mmproj.is_file() {
        return None;
    }
    Some(PaddleEnginePaths {
        server,
        model,
        mmproj,
    })
}

/// Resolve the complete managed Paddle stack. Partial installs are rejected
/// so extraction never starts with a missing projector or runtime library.
pub fn paddle_engine_paths() -> Result<PaddleEnginePaths, String> {
    let root = paddle_root()?;
    paddle_paths_at(&root).ok_or_else(|| {
        "PaddleOCR-VL 1.6 Q8 is not installed. Install it from Settings → PDF Extraction."
            .to_string()
    })
}

fn paddle_full_parser_paths_at(root: &Path) -> Option<PaddleFullParserPaths> {
    let version_root = root.join("versions").join(PADDLE_PARSER_RELEASE);
    let python = venv_python(&version_root.join("venv"));
    let script = version_root.join("paddle_parser_sidecar.py");
    let manifest = version_root.join("install.json");
    let packages = version_root.join("packages.txt");
    if !python.is_file() || !script.is_file() || !packages.is_file() || !manifest.is_file() {
        return None;
    }
    let manifest_value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest).ok()?).ok()?;
    if manifest_value
        .get("release")
        .and_then(serde_json::Value::as_str)
        != Some(PADDLE_PARSER_RELEASE)
    {
        return None;
    }
    if std::fs::metadata(&script).ok()?.len() > 2 * 1024 * 1024 {
        return None;
    }
    let script_bytes = std::fs::read(&script).ok()?;
    let script_digest = format!("{:x}", Sha256::digest(&script_bytes));
    let expected_script_digest = format!("{:x}", Sha256::digest(PADDLE_PARSER_SCRIPT.as_bytes()));
    if script_digest != expected_script_digest {
        return None;
    }
    if manifest_value
        .get("sidecar_sha256")
        .and_then(serde_json::Value::as_str)
        != Some(script_digest.as_str())
    {
        return None;
    }
    if std::fs::metadata(&packages).ok()?.len() > 2 * 1024 * 1024 {
        return None;
    }
    let package_bytes = std::fs::read(&packages).ok()?;
    let package_digest = format!("{:x}", Sha256::digest(&package_bytes));
    if manifest_value
        .get("packages_sha256")
        .and_then(serde_json::Value::as_str)
        != Some(package_digest.as_str())
    {
        return None;
    }
    Some(PaddleFullParserPaths {
        python,
        script,
        model_cache: root.join("models"),
        paddle: paddle_engine_paths().ok()?,
        release: PADDLE_PARSER_RELEASE.to_string(),
    })
}

fn parser_runtime_reusable_for_sidecar_refresh(root: &Path) -> bool {
    let version_root = root.join("versions").join(PADDLE_PARSER_RELEASE);
    let python = venv_python(&version_root.join("venv"));
    let manifest_path = version_root.join("install.json");
    let packages = version_root.join("packages.txt");
    if !python.is_file() || !manifest_path.is_file() || !packages.is_file() {
        return false;
    }
    if std::fs::metadata(&manifest_path)
        .ok()
        .is_none_or(|metadata| metadata.len() > 2 * 1024 * 1024)
        || std::fs::metadata(&packages)
            .ok()
            .is_none_or(|metadata| metadata.len() > 2 * 1024 * 1024)
    {
        return false;
    }
    let Some(manifest) = std::fs::read(&manifest_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
    else {
        return false;
    };
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
        || manifest.get("python").and_then(serde_json::Value::as_str) != Some("3.12")
    {
        return false;
    }
    let Some(expected_packages_digest) = manifest
        .get("packages_sha256")
        .and_then(serde_json::Value::as_str)
    else {
        return false;
    };
    std::fs::read(&packages)
        .is_ok_and(|bytes| format!("{:x}", Sha256::digest(&bytes)) == expected_packages_digest)
}

fn refresh_paddle_parser_sidecar(root: &Path) -> Result<bool, String> {
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

    std::fs::write(&script, PADDLE_PARSER_SCRIPT)
        .map_err(|error| format!("Failed to refresh parser sidecar: {error}"))?;
    manifest["sidecar_sha256"] = serde_json::Value::String(expected_digest);
    manifest["sidecar_contract"] = serde_json::Value::from(2);
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest)
            .map_err(|error| format!("Failed to serialize parser manifest: {error}"))?,
    )
    .map_err(|error| format!("Failed to refresh parser manifest: {error}"))?;
    if paddle_full_parser_paths_at(root).is_none() {
        return Err("The refreshed parser sidecar failed its integrity check".to_string());
    }
    Ok(true)
}

/// Resolve the managed full parser and the native VLM stack it depends on.
pub fn paddle_full_parser_paths() -> Result<PaddleFullParserPaths, String> {
    full_parser_support()?;
    let root = paddle_parser_root()?;
    paddle_full_parser_paths_at(&root).ok_or_else(|| {
        "PaddleOCR-VL Full Parser is not installed. Install it from Settings → PDF Extraction."
            .to_string()
    })
}

/// Runtime environment for the private parser process. Model weights and
/// caches remain inside Pipeline's managed root and user-site imports are
/// disabled so a global Python installation cannot alter extraction.
pub fn paddle_full_parser_env(paths: &PaddleFullParserPaths) -> Vec<(String, String)> {
    vec![
        (
            "PADDLE_PDX_CACHE_HOME".to_string(),
            paths.model_cache.to_string_lossy().to_string(),
        ),
        ("PYTHONNOUSERSITE".to_string(), "1".to_string()),
        ("PYTHONUTF8".to_string(), "1".to_string()),
        ("PIP_DISABLE_PIP_VERSION_CHECK".to_string(), "1".to_string()),
    ]
}

/// Total size in bytes of a directory tree. Best-effort; unreadable entries
/// are skipped.
fn dir_size(root: &Path) -> u64 {
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
    pub available: bool,
    pub unavailable_reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetiredMarkerStatus {
    pub present: bool,
    pub bytes: u64,
}

const RETIRED_MARKER_SHIMS: &[&str] = &[
    "marker",
    "marker_chunk_convert",
    "marker_extract",
    "marker_gui",
    "marker_server",
    "marker_single",
];

fn retired_marker_paths() -> Result<Vec<PathBuf>, String> {
    let home = pipeline_home()?;
    let mut paths = vec![home.join("tools").join("marker-pdf")];
    let bin = home.join("bin");
    for shim in RETIRED_MARKER_SHIMS {
        paths.push(bin.join(exe(shim)));
        if cfg!(windows) {
            paths.push(bin.join(format!("{shim}.cmd")));
            paths.push(bin.join(format!("{shim}.bat")));
        }
    }
    Ok(paths)
}

/// Detect only the app-managed Marker environment from older Pipeline builds.
/// System installations elsewhere on PATH are deliberately ignored.
pub fn retired_marker_status() -> RetiredMarkerStatus {
    let paths = retired_marker_paths().unwrap_or_default();
    let present = paths
        .iter()
        .any(|path| std::fs::symlink_metadata(path).is_ok());
    let bytes = paths
        .first()
        .map(|tool_root| dir_size(tool_root))
        .unwrap_or(0);
    RetiredMarkerStatus { present, bytes }
}

fn remove_retired_marker_path(path: &Path) -> Result<(), String> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("Failed to inspect {}: {error}", path.display())),
    };
    if metadata.file_type().is_symlink() || metadata.is_file() {
        std::fs::remove_file(path)
            .map_err(|error| format!("Failed to remove {}: {error}", path.display()))
    } else if metadata.is_dir() {
        std::fs::remove_dir_all(path)
            .map_err(|error| format!("Failed to remove {}: {error}", path.display()))
    } else {
        Err(format!(
            "Refusing to remove unexpected filesystem object {}",
            path.display()
        ))
    }
}

/// Status of every registry engine.
pub fn engine_statuses() -> Vec<EngineStatus> {
    let stack_mb = pipeline_home()
        .map(|home| dir_size(&home.join("native")) / 1_000_000)
        .unwrap_or(0);
    let installing = INSTALL_RUNNING.load(Ordering::Acquire);

    ENGINES
        .iter()
        .map(|spec| {
            let (entry, version, support) = match spec.id {
                "paddleocr-vl" => {
                    let paths = paddle_root().ok().and_then(|root| paddle_paths_at(&root));
                    (
                        paths.as_ref().map(|paths| paths.server.clone()),
                        paths
                            .as_ref()
                            .map(|_| "1.6 Q8".to_string())
                            .unwrap_or_default(),
                        llama_artifact().map(|_| ()),
                    )
                }
                "paddleocr-vl-parser" => {
                    let paths = paddle_parser_root()
                        .ok()
                        .and_then(|root| paddle_full_parser_paths_at(&root));
                    (
                        paths.as_ref().map(|paths| paths.script.clone()),
                        paths
                            .as_ref()
                            .map(|_| PADDLE_PARSER_VERSION.to_string())
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
                available: support.is_ok(),
                unavailable_reason,
            }
        })
        .collect()
}

// ── Install lifecycle ───────────────────────────────────────────────

static INSTALL_RUNNING: AtomicBool = AtomicBool::new(false);
static INSTALL_CANCEL: AtomicBool = AtomicBool::new(false);
static INSTALL_CHILD_PID: Mutex<Option<u32>> = Mutex::new(None);

/// Hard ceiling on a managed-engine download.
const INSTALL_STEP_TIMEOUT_SECS: u64 = 3600;

struct InstallGuard {
    lock_file: std::fs::File,
}

fn acquire_install_guard() -> Result<InstallGuard, String> {
    use fs2::FileExt as _;
    if INSTALL_RUNNING.swap(true, Ordering::SeqCst) {
        return Err("An engine install is already running".to_string());
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
            .map_err(|e| format!("Another Pipeline process is installing an engine ({e})"))?;
        Ok(InstallGuard { lock_file })
    })();
    if result.is_err() {
        INSTALL_RUNNING.store(false, Ordering::SeqCst);
    }
    result
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        kill_install_child();
        INSTALL_RUNNING.store(false, Ordering::SeqCst);
        let _ = fs2::FileExt::unlock(&self.lock_file);
    }
}

/// Request cancellation of a running install.
pub fn cancel_install() {
    INSTALL_CANCEL.store(true, Ordering::Release);
    kill_install_child();
}

fn kill_install_child() {
    let pid = INSTALL_CHILD_PID
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take();
    if let Some(pid) = pid {
        crate::commands::kill_process(pid);
        crate::commands::unregister_child_pid(pid);
    }
}

fn log(app: &AppHandle, line: impl Into<String>) {
    app.emit("engines:log", serde_json::json!({ "line": line.into() }))
        .ok();
}

fn emit_phase(app: &AppHandle, engine_id: &str, phase: &str, status: &str) {
    app.emit(
        "engines:phase",
        serde_json::json!({ "engine": engine_id, "phase": phase, "status": status }),
    )
    .ok();
}

async fn await_install_operation<F, T>(future: F) -> Result<T, String>
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

async fn download_verified(
    app: &AppHandle,
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

fn safe_archive_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path.components().all(|component| {
            matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
}

fn unpack_llama_archive(
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
                std::os::unix::fs::symlink(&link_name, &target)
                    .map_err(|error| format!("Failed to extract llama.cpp symlink: {error}"))?;
                #[cfg(not(unix))]
                return Err("Unexpected symlink in llama.cpp archive".to_string());
                continue;
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

fn unpack_uv_archive(
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

fn uv_binary_path() -> Result<PathBuf, String> {
    Ok(paddle_parser_root()?.join("runtime").join(exe("uv")))
}

fn sha256_regular_file(path: &Path, max_bytes: u64) -> Result<String, String> {
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

fn uv_env() -> Result<Vec<(String, String)>, String> {
    let root = paddle_parser_root()?;
    let value = |path: PathBuf| path.to_string_lossy().to_string();
    Ok(vec![
        (
            "UV_PYTHON_INSTALL_DIR".to_string(),
            value(root.join("python")),
        ),
        ("UV_CACHE_DIR".to_string(), value(root.join("uv-cache"))),
        ("UV_SYSTEM_CERTS".to_string(), "1".to_string()),
        ("UV_MANAGED_PYTHON".to_string(), "1".to_string()),
        ("UV_NO_CONFIG".to_string(), "1".to_string()),
        ("UV_KEYRING_PROVIDER".to_string(), "disabled".to_string()),
        (
            "PADDLE_PDX_CACHE_HOME".to_string(),
            value(root.join("models")),
        ),
        ("PYTHONNOUSERSITE".to_string(), "1".to_string()),
    ])
}

async fn ensure_uv(app: &AppHandle) -> Result<PathBuf, String> {
    let uv_path = uv_binary_path()?;
    let artifact = uv_artifact()?;
    if uv_path.is_file() {
        let verified = sha256_regular_file(&uv_path, MAX_UV_EXTRACTED_BYTES)
            .is_ok_and(|digest| digest == artifact.sha256);
        if verified {
            let mut probe = std::process::Command::new(&uv_path);
            probe.arg("--version");
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

async fn run_install_step(
    app: &AppHandle,
    program: &Path,
    args: &[String],
    environment: &[(String, String)],
    label: &str,
) -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt as _, BufReader};

    let mut command = crate::pipeline::claude::build_silent_command(
        program
            .to_str()
            .ok_or_else(|| format!("{label} program path is not valid UTF-8"))?,
        None,
    );
    command.args(args);
    for (key, value) in environment {
        command.env(key, value);
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("Failed to start {label}: {error}"))?;
    let pid = child.id().unwrap_or(0);
    if pid > 0 {
        crate::commands::register_child_pid(pid);
        *INSTALL_CHILD_PID
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(pid);
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_app = app.clone();
    let stdout_task = tokio::spawn(async move {
        if let Some(stdout) = stdout {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    log(&stdout_app, line);
                }
            }
        }
    });
    let stderr_app = app.clone();
    let stderr_task = tokio::spawn(async move {
        if let Some(stderr) = stderr {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    log(&stderr_app, line);
                }
            }
        }
    });

    let status = tokio::time::timeout(
        std::time::Duration::from_secs(INSTALL_STEP_TIMEOUT_SECS),
        child.wait(),
    )
    .await;
    let status = match status {
        Ok(status) => status.map_err(|error| format!("Failed waiting for {label}: {error}"))?,
        Err(_) => {
            crate::commands::kill_process(pid);
            let _ = child.kill().await;
            let _ = child.wait().await;
            if pid > 0 {
                crate::commands::unregister_child_pid(pid);
                let mut active = INSTALL_CHILD_PID
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                if *active == Some(pid) {
                    *active = None;
                }
            }
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            return Err(format!(
                "{label} timed out after {INSTALL_STEP_TIMEOUT_SECS}s"
            ));
        }
    };
    if pid > 0 {
        crate::commands::unregister_child_pid(pid);
        let mut active = INSTALL_CHILD_PID
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if *active == Some(pid) {
            *active = None;
        }
    }
    let _ = stdout_task.await;
    let _ = stderr_task.await;
    if INSTALL_CANCEL.load(Ordering::Acquire) {
        return Err("Installation cancelled".to_string());
    }
    if !status.success() {
        return Err(format!(
            "{label} failed (exit {}). Check the install log for details.",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

async fn install_paddle_engine(app: &AppHandle, spec: &EngineSpec) -> Result<(), String> {
    let artifact = llama_artifact()?;
    let native_dir = pipeline_home()?.join("native");
    std::fs::create_dir_all(&native_dir)
        .map_err(|error| format!("Failed to create native engine directory: {error}"))?;
    let staging = tempfile::Builder::new()
        .prefix(".paddleocr-vl-staging-")
        .tempdir_in(&native_dir)
        .map_err(|error| format!("Failed to create PaddleOCR-VL staging directory: {error}"))?;

    emit_phase(app, spec.id, "runtime", "running");
    let runtime_archive = staging.path().join(if artifact.filename.ends_with(".zip") {
        "llama-runtime.zip"
    } else {
        "llama-runtime.tar.gz"
    });
    if let Err(error) = download_verified(
        app,
        &llama_download_url(artifact),
        &runtime_archive,
        artifact.sha256,
        MAX_LLAMA_ARCHIVE_BYTES,
        "llama.cpp runtime",
    )
    .await
    {
        emit_phase(app, spec.id, "runtime", "failed");
        return Err(error);
    }
    let runtime_dir = staging.path().join("runtime");
    let archive_for_task = runtime_archive.clone();
    let runtime_for_task = runtime_dir.clone();
    let is_zip = artifact.filename.ends_with(".zip");
    let unpacked = tokio::task::spawn_blocking(move || {
        unpack_llama_archive(&archive_for_task, &runtime_for_task, is_zip)
    })
    .await
    .map_err(|error| format!("Runtime extraction task failed: {error}"))?;
    if let Err(error) = unpacked {
        emit_phase(app, spec.id, "runtime", "failed");
        return Err(error);
    }
    let _ = std::fs::remove_file(&runtime_archive);
    let staged_server = find_file_named(&runtime_dir, &exe("llama-server"))
        .ok_or("llama-server disappeared after extraction")?;
    let mut probe = std::process::Command::new(&staged_server);
    probe.arg("--version");
    let output =
        crate::process::run_bounded(&mut probe, std::time::Duration::from_secs(15), 256 * 1024)
            .map_err(|error| format!("llama-server validation failed: {error}"))?;
    if !output.status.success() || output.stdout_truncated || output.stderr_truncated {
        emit_phase(app, spec.id, "runtime", "failed");
        return Err("The downloaded llama-server failed its validation check".to_string());
    }
    emit_phase(app, spec.id, "runtime", "done");

    // This engine deliberately has no Python/package-manager layer.
    emit_phase(app, spec.id, "packages", "running");
    log(app, "Native engine: no Python packages required");
    emit_phase(app, spec.id, "packages", "done");

    emit_phase(app, spec.id, "models", "running");
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
        if let Err(error) = download_verified(
            app,
            &paddle_model_url(filename),
            &model_dir.join(filename),
            sha256,
            MAX_PADDLE_MODEL_BYTES,
            label,
        )
        .await
        {
            emit_phase(app, spec.id, "models", "failed");
            return Err(error);
        }
    }
    emit_phase(app, spec.id, "models", "done");

    let manifest = serde_json::json!({
        "engine": "paddleocr-vl",
        "model_version": "1.6",
        "quantization": "Q8",
        "model_revision": PADDLE_MODEL_REVISION,
        "llama_cpp_version": LLAMA_CPP_VERSION,
        "runtime_archive": artifact.filename,
        "runtime_sha256": artifact.sha256,
        "model_sha256": PADDLE_MODEL_SHA256,
        "mmproj_sha256": PADDLE_MMPROJ_SHA256,
    });
    std::fs::write(
        staging.path().join("install.json"),
        serde_json::to_vec_pretty(&manifest)
            .map_err(|error| format!("Failed to serialize install manifest: {error}"))?,
    )
    .map_err(|error| format!("Failed to write install manifest: {error}"))?;
    if paddle_paths_at(staging.path()).is_none() {
        return Err("The staged PaddleOCR-VL installation is incomplete".to_string());
    }
    if INSTALL_CANCEL.load(Ordering::Acquire) {
        return Err("Install cancelled".to_string());
    }

    // Swap only after every checksum and executable check succeeds. If a prior
    // install exists, retain it as a same-filesystem backup until the rename.
    let target = paddle_root()?;
    let backup = native_dir.join(".paddleocr-vl-backup");
    if backup.exists() {
        std::fs::remove_dir_all(&backup)
            .map_err(|error| format!("Failed to clean an old engine backup: {error}"))?;
    }
    if target.exists() {
        std::fs::rename(&target, &backup)
            .map_err(|error| format!("Failed to stage the previous engine version: {error}"))?;
    }
    let staged_path = staging.keep();
    if let Err(error) = std::fs::rename(&staged_path, &target) {
        if backup.exists() {
            let _ = std::fs::rename(&backup, &target);
        }
        return Err(format!("Failed to activate PaddleOCR-VL: {error}"));
    }
    if backup.exists() {
        let _ = std::fs::remove_dir_all(&backup);
    }
    let installed = paddle_engine_paths()?;
    log(
        app,
        format!("{} installed at {}", spec.label, installed.server.display()),
    );
    Ok(())
}

async fn install_paddle_full_parser(app: &AppHandle, spec: &EngineSpec) -> Result<(), String> {
    full_parser_support()?;

    // The parser is an add-on to the native Q8 stack. Installing it is one
    // user action even on a fresh machine.
    if paddle_engine_paths().is_err() {
        let base = engine("paddleocr-vl")?;
        log(
            app,
            "Installing the managed PaddleOCR-VL model and llama.cpp dependency first",
        );
        install_paddle_engine(app, base).await?;
    }

    let parser_root = paddle_parser_root()?;
    match refresh_paddle_parser_sidecar(&parser_root) {
        Ok(true) => {
            for phase in ["runtime", "packages", "models"] {
                emit_phase(app, spec.id, phase, "done");
            }
            let installed = paddle_full_parser_paths()?;
            log(
                app,
                format!(
                    "{} sidecar updated without reinstalling its managed runtime ({})",
                    spec.label,
                    installed.script.display()
                ),
            );
            return Ok(());
        }
        Ok(false) => {}
        Err(error) => log(
            app,
            format!("Parser sidecar refresh was unavailable; rebuilding the runtime ({error})"),
        ),
    }

    emit_phase(app, spec.id, "runtime", "running");
    let uv = match ensure_uv(app).await {
        Ok(uv) => uv,
        Err(error) => {
            emit_phase(app, spec.id, "runtime", "failed");
            return Err(error);
        }
    };
    emit_phase(app, spec.id, "runtime", "done");

    let versions = parser_root.join("versions");
    let target = paddle_parser_version_root()?;
    let backup = versions.join(format!(".{PADDLE_PARSER_RELEASE}-backup"));
    std::fs::create_dir_all(&versions)
        .map_err(|error| format!("Failed to create parser versions directory: {error}"))?;
    if backup.exists() {
        std::fs::remove_dir_all(&backup)
            .map_err(|error| format!("Failed to remove an old parser backup: {error}"))?;
    }
    if target.exists() {
        std::fs::rename(&target, &backup)
            .map_err(|error| format!("Failed to stage the previous parser version: {error}"))?;
    }
    std::fs::create_dir_all(&target)
        .map_err(|error| format!("Failed to create parser version directory: {error}"))?;

    let provision_result: Result<(), String> = async {
        emit_phase(app, spec.id, "packages", "running");
        let environment = uv_env()?;
        let venv = target.join("venv");
        let python = venv_python(&venv);
        let venv_args = vec![
            "venv".to_string(),
            venv.to_string_lossy().to_string(),
            "--python".to_string(),
            "3.12".to_string(),
            "--managed-python".to_string(),
        ];
        log(app, "Creating a private managed Python 3.12 runtime");
        run_install_step(app, &uv, &venv_args, &environment, "parser runtime install").await?;

        let install_args = vec![
            "pip".to_string(),
            "install".to_string(),
            "--python".to_string(),
            python.to_string_lossy().to_string(),
            format!("paddlepaddle=={PADDLE_RUNTIME_VERSION}"),
            format!("paddleocr[doc-parser]=={PADDLE_PARSER_VERSION}"),
        ];
        log(
            app,
            format!(
                "Installing pinned PaddleOCR {PADDLE_PARSER_VERSION} and PaddlePaddle {PADDLE_RUNTIME_VERSION}"
            ),
        );
        run_install_step(app, &uv, &install_args, &environment, "parser package install").await?;

        let check_args = vec![
            "pip".to_string(),
            "check".to_string(),
            "--python".to_string(),
            python.to_string_lossy().to_string(),
        ];
        run_install_step(app, &uv, &check_args, &environment, "parser dependency check").await?;

        let package_inventory = target.join("packages.txt");
        let inventory_code = r#"from importlib.metadata import distributions
from pathlib import Path
import sys
items = sorted(f"{name}=={dist.version}" for dist in distributions() if (name := dist.metadata.get("Name")))
Path(sys.argv[1]).write_text("\n".join(items) + "\n", encoding="utf-8")"#;
        let inventory_args = vec![
            "-c".to_string(),
            inventory_code.to_string(),
            package_inventory.to_string_lossy().to_string(),
        ];
        run_install_step(
            app,
            &python,
            &inventory_args,
            &environment,
            "parser package inventory",
        )
        .await?;
        let inventory_metadata = std::fs::metadata(&package_inventory)
            .map_err(|error| format!("Failed to inspect parser package inventory: {error}"))?;
        if inventory_metadata.len() > 2 * 1024 * 1024 {
            return Err("Parser package inventory exceeds its safety limit".to_string());
        }
        let inventory_bytes = std::fs::read(&package_inventory)
            .map_err(|error| format!("Failed to read parser package inventory: {error}"))?;
        let inventory_sha256 = format!("{:x}", Sha256::digest(&inventory_bytes));

        let script = target.join("paddle_parser_sidecar.py");
        std::fs::write(&script, PADDLE_PARSER_SCRIPT)
            .map_err(|error| format!("Failed to write parser sidecar: {error}"))?;
        let probe_args = vec![script.to_string_lossy().to_string(), "--probe".to_string()];
        run_install_step(app, &python, &probe_args, &environment, "parser import check").await?;
        emit_phase(app, spec.id, "packages", "done");

        emit_phase(app, spec.id, "models", "running");
        std::fs::create_dir_all(parser_root.join("models"))
            .map_err(|error| format!("Failed to create parser model cache: {error}"))?;
        // Construction downloads and validates PP-DocLayoutV3 without sending
        // a document to the deliberately unreachable recognition endpoint.
        let warm_args = vec![
            script.to_string_lossy().to_string(),
            "--warm-layout".to_string(),
            "--server-url".to_string(),
            "http://127.0.0.1:9/v1".to_string(),
        ];
        match run_install_step(app, &python, &warm_args, &environment, "layout model warm-up").await {
            Ok(()) => {}
            Err(error) if error.contains("cancelled") => return Err(error),
            Err(error) => log(
                app,
                format!(
                    "WARNING: PP-DocLayoutV3 warm-up did not complete ({error}). The parser will retry the model download on first use."
                ),
            ),
        }
        emit_phase(app, spec.id, "models", "done");

        let manifest = serde_json::json!({
            "engine": "paddleocr-vl-parser",
            "release": PADDLE_PARSER_RELEASE,
            "paddleocr": PADDLE_PARSER_VERSION,
            "paddlepaddle": PADDLE_RUNTIME_VERSION,
            "python": "3.12",
            "uv": UV_VERSION,
            "sidecar_sha256": format!("{:x}", Sha256::digest(PADDLE_PARSER_SCRIPT.as_bytes())),
            "sidecar_contract": 2,
            "packages_sha256": inventory_sha256,
            "recognition_backend": "managed llama.cpp",
        });
        std::fs::write(
            target.join("install.json"),
            serde_json::to_vec_pretty(&manifest)
                .map_err(|error| format!("Failed to serialize parser manifest: {error}"))?,
        )
        .map_err(|error| format!("Failed to write parser manifest: {error}"))?;
        paddle_full_parser_paths_at(&parser_root)
            .ok_or("The managed full-parser installation is incomplete")?;
        Ok(())
    }
    .await;

    if let Err(error) = provision_result {
        let _ = std::fs::remove_dir_all(&target);
        if backup.exists() {
            let _ = std::fs::rename(&backup, &target);
        }
        emit_phase(app, spec.id, "packages", "failed");
        return Err(error);
    }
    if backup.exists() {
        let _ = std::fs::remove_dir_all(&backup);
    }
    let installed = paddle_full_parser_paths()?;
    log(
        app,
        format!("{} installed at {}", spec.label, installed.script.display()),
    );
    Ok(())
}

/// Install a registered native engine. Unknown and retired engine IDs are
/// rejected before any download or subprocess can begin.
pub async fn install_engine(app: &AppHandle, engine_id: &str) -> Result<(), String> {
    let spec = engine(engine_id)?;
    let _guard = acquire_install_guard()?;
    INSTALL_CANCEL.store(false, Ordering::Release);

    // A sidecar-only parser update reuses the already verified private
    // runtime and does not need the full installation's disk headroom.
    let lightweight_parser_refresh = engine_id == "paddleocr-vl-parser"
        && paddle_parser_root()
            .ok()
            .is_some_and(|root| parser_runtime_reusable_for_sidecar_refresh(&root));

    // Disk-space precheck against the estimate, with headroom.
    let home = pipeline_home()?;
    std::fs::create_dir_all(&home).map_err(|e| format!("Failed to create ~/.pipeline: {e}"))?;
    if !lightweight_parser_refresh {
        if let Ok(free) = fs2::available_space(&home) {
            let needed = spec.est_disk_mb * 1_000_000;
            if free < needed {
                emit_phase(app, engine_id, "runtime", "failed");
                return Err(format!(
                    "Not enough disk space: {} needs ~{} GB free, {} GB available.",
                    spec.label,
                    spec.est_disk_mb / 1000,
                    free / 1_000_000_000
                ));
            }
        }
    }

    match engine_id {
        "paddleocr-vl" => install_paddle_engine(app, spec).await,
        "paddleocr-vl-parser" => install_paddle_full_parser(app, spec).await,
        _ => Err(format!("Unknown engine '{engine_id}'")),
    }
}

/// Uninstall a registered native engine. Retired engine IDs are intentionally
/// rejected; Pipeline never executes their package managers or entry points.
pub async fn uninstall_engine(app: &AppHandle, engine_id: &str) -> Result<(), String> {
    let spec = engine(engine_id)?;
    let _guard = acquire_install_guard()?;
    INSTALL_CANCEL.store(false, Ordering::Release);

    let full_parser_installed = paddle_parser_root()
        .ok()
        .and_then(|root| paddle_full_parser_paths_at(&root))
        .is_some();
    if engine_id == "paddleocr-vl" && full_parser_installed {
        return Err(
            "Uninstall PaddleOCR-VL Full Parser before removing its managed Q8 dependency."
                .to_string(),
        );
    }
    let root = match engine_id {
        "paddleocr-vl" => paddle_root()?,
        "paddleocr-vl-parser" => paddle_parser_root()?,
        _ => return Err(format!("Unknown engine '{engine_id}'")),
    };
    if root.is_dir() {
        tokio::task::spawn_blocking(move || std::fs::remove_dir_all(root))
            .await
            .map_err(|error| format!("Managed engine cleanup task failed: {error}"))?
            .map_err(|error| format!("Failed to remove {}: {error}", spec.label))?;
    }
    log(app, format!("{} uninstalled", spec.label));
    Ok(())
}

/// Remove only the retired app-managed Marker venv and its known launch
/// shims. Model/cache directories and every run artifact remain untouched.
pub async fn remove_retired_marker(app: &AppHandle) -> Result<(), String> {
    let _guard = acquire_install_guard()?;
    INSTALL_CANCEL.store(false, Ordering::Release);
    let paths = retired_marker_paths()?;
    tokio::task::spawn_blocking(move || {
        let mut errors = Vec::new();
        for path in paths {
            if let Err(error) = remove_retired_marker_path(&path) {
                errors.push(error);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    })
    .await
    .map_err(|error| format!("Retired Marker cleanup task failed: {error}"))??;
    log(app, "Removed the retired managed Marker environment");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_are_well_formed() {
        for checksum in [PADDLE_MODEL_SHA256, PADDLE_MMPROJ_SHA256] {
            assert_eq!(checksum.len(), 64);
            assert!(checksum.chars().all(|c| c.is_ascii_hexdigit()));
        }
        for artifact in LLAMA_ARTIFACTS {
            assert_eq!(artifact.sha256.len(), 64, "{}", artifact.filename);
            assert!(artifact.sha256.chars().all(|c| c.is_ascii_hexdigit()));
        }
        for artifact in UV_ARTIFACTS {
            assert_eq!(artifact.sha256.len(), 64, "{}", artifact.target);
            assert!(artifact.sha256.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn managed_bootstrap_digest_is_bounded() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(b"abc").unwrap();
        assert_eq!(
            sha256_regular_file(file.path(), 3).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(sha256_regular_file(file.path(), 2).is_err());
    }

    #[test]
    fn registry_has_unique_ids_and_excludes_retired_marker() {
        let mut ids = std::collections::HashSet::new();
        for e in ENGINES {
            assert!(ids.insert(e.id), "duplicate engine id {}", e.id);
        }
        assert_eq!(
            ids,
            std::collections::HashSet::from(["paddleocr-vl", "paddleocr-vl-parser"])
        );
        assert!(engine("marker").is_err());
    }

    #[test]
    fn retired_marker_cleanup_targets_only_known_app_owned_paths() {
        let home = pipeline_home().unwrap();
        let paths = retired_marker_paths().unwrap();
        assert!(paths
            .iter()
            .all(|path| path.starts_with(&home) && path != &home));
        assert!(paths.contains(&home.join("tools").join("marker-pdf")));
        assert!(!paths.iter().any(|path| path.starts_with(home.join("runs"))));
        assert!(!paths
            .iter()
            .any(|path| path.starts_with(home.join("cache"))));
    }

    #[test]
    fn llama_artifacts_cover_desktop_release_platforms() {
        for (os, arch) in [
            ("macos", "aarch64"),
            ("macos", "x86_64"),
            ("linux", "aarch64"),
            ("linux", "x86_64"),
            ("windows", "aarch64"),
            ("windows", "x86_64"),
        ] {
            assert!(
                llama_artifact_for(os, arch).is_some(),
                "missing llama.cpp artifact for {os}/{arch}"
            );
        }
        assert!(llama_artifact_for("linux", "riscv64").is_none());
    }

    #[test]
    fn full_parser_support_is_explicit_for_release_platforms() {
        assert!(full_parser_support_for("macos", "aarch64").is_ok());
        assert!(full_parser_support_for("windows", "x86_64").is_ok());
        assert!(full_parser_support_for("linux", "x86_64").is_ok());
        assert!(full_parser_support_for("macos", "x86_64")
            .unwrap_err()
            .contains("Intel macOS"));
        assert!(full_parser_support_for("windows", "aarch64").is_err());
    }

    #[test]
    fn uv_artifacts_cover_supported_full_parser_platforms() {
        for (os, arch) in [
            ("macos", "aarch64"),
            ("windows", "x86_64"),
            ("linux", "x86_64"),
            ("linux", "aarch64"),
        ] {
            assert!(
                uv_artifact_for(os, arch).is_some(),
                "missing uv for {os}/{arch}"
            );
        }
    }

    #[test]
    fn parser_sidecar_refresh_reuses_only_a_verified_pinned_runtime() {
        let root = tempfile::tempdir().unwrap();
        let version_root = root.path().join("versions").join(PADDLE_PARSER_RELEASE);
        let python = venv_python(&version_root.join("venv"));
        std::fs::create_dir_all(python.parent().unwrap()).unwrap();
        std::fs::write(&python, b"managed python fixture").unwrap();
        let packages = b"paddleocr==3.7.0\npaddlepaddle==3.2.1\n";
        std::fs::write(version_root.join("packages.txt"), packages).unwrap();
        let manifest = serde_json::json!({
            "engine": "paddleocr-vl-parser",
            "release": PADDLE_PARSER_RELEASE,
            "paddleocr": PADDLE_PARSER_VERSION,
            "paddlepaddle": PADDLE_RUNTIME_VERSION,
            "python": "3.12",
            "packages_sha256": format!("{:x}", Sha256::digest(packages)),
        });
        std::fs::write(
            version_root.join("install.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();

        assert!(parser_runtime_reusable_for_sidecar_refresh(root.path()));
        std::fs::write(version_root.join("packages.txt"), b"changed").unwrap();
        assert!(!parser_runtime_reusable_for_sidecar_refresh(root.path()));
    }

    #[test]
    fn archive_paths_reject_traversal_and_absolute_paths() {
        assert!(safe_archive_path(Path::new("build/bin/llama-server")));
        assert!(!safe_archive_path(Path::new("../llama-server")));
        assert!(!safe_archive_path(Path::new("/tmp/llama-server")));
    }

    #[cfg(unix)]
    #[test]
    fn llama_unpack_preserves_safe_runtime_symlinks() {
        use std::io::Cursor;

        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("runtime.tar.gz");
        let archive_file = std::fs::File::create(&archive_path).unwrap();
        let encoder = flate2::write::GzEncoder::new(archive_file, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        for (name, bytes, mode) in [
            ("bundle/llama-server", b"server".as_slice(), 0o755),
            ("bundle/libfoo.1.dylib", b"library".as_slice(), 0o644),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(mode);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();
            builder
                .append_data(&mut header, name, Cursor::new(bytes))
                .unwrap();
        }
        let mut link = tar::Header::new_gnu();
        link.set_size(0);
        link.set_mode(0o777);
        link.set_entry_type(tar::EntryType::Symlink);
        link.set_link_name("libfoo.1.dylib").unwrap();
        link.set_cksum();
        builder
            .append_data(
                &mut link,
                "bundle/libfoo.dylib",
                Cursor::new(Vec::<u8>::new()),
            )
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap();

        let destination = temp.path().join("unpacked");
        unpack_llama_archive(&archive_path, &destination, false).unwrap();
        assert_eq!(
            std::fs::read(destination.join("bundle/libfoo.dylib")).unwrap(),
            b"library"
        );
        assert!(
            std::fs::symlink_metadata(destination.join("bundle/libfoo.dylib"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn dir_size_sums_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.bin"), vec![0u8; 1000]).unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/b.bin"), vec![0u8; 500]).unwrap();
        assert_eq!(dir_size(dir.path()), 1500);
        assert_eq!(dir_size(&dir.path().join("missing")), 0);
    }

    #[cfg(unix)]
    #[test]
    fn dir_size_does_not_follow_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("inside"), vec![0u8; 10]).unwrap();
        std::fs::write(outside.path().join("outside"), vec![0u8; 1_000]).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
        std::os::unix::fs::symlink(root.path(), root.path().join("cycle")).unwrap();
        assert_eq!(dir_size(root.path()), 10);
    }
}
