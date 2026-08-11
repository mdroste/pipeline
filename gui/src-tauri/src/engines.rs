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

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};
use std::io::Read as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

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
const PYTHON_VERSION: &str = "3.12.13";
const PYTHON_BUILD_RELEASE: &str = "20260623";
const PADDLE_PARSER_VERSION: &str = "3.7.0";
const PADDLE_RUNTIME_VERSION: &str = "3.2.1";
const PADDLE_PARSER_RELEASE: &str = "paddleocr-3.7.0-paddle-3.2.1-python-3.12.13-r2";
const PADDLE_PARSER_SCRIPT: &str = include_str!("paddle_parser_sidecar.py");
const PADDLE_PARSER_RUNTIME_LOCK: &str =
    include_str!("../resources/paddle-parser/runtime-lock.json");
const PADDLE_LAYOUT_MODEL_URL: &str = "https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-DocLayoutV3_infer.tar";
const PADDLE_LAYOUT_MODEL_SHA256: &str =
    "98b9bac88c80f6bc0fda7e0bfc2cae180020371c0b2edbb1eb498a70ace751b1";
const MAX_PYTHON_ARCHIVE_BYTES: u64 = 80_000_000;
const MAX_PYTHON_EXTRACTED_BYTES: u64 = 1_500_000_000;
const MAX_PYTHON_ARCHIVE_ENTRIES: usize = 100_000;
const MAX_LAYOUT_MODEL_ARCHIVE_BYTES: u64 = 200_000_000;
const MAX_LAYOUT_MODEL_EXTRACTED_BYTES: u64 = 200_000_000;
const MAX_UV_ARCHIVE_BYTES: u64 = 80_000_000;
const MAX_UV_EXTRACTED_BYTES: u64 = 150_000_000;

struct PythonArtifact {
    os: &'static str,
    arch: &'static str,
    target: &'static str,
    sha256: &'static str,
    lock: &'static str,
    lock_sha256: &'static str,
}

const PYTHON_ARTIFACTS: &[PythonArtifact] = &[
    PythonArtifact {
        os: "macos",
        arch: "aarch64",
        target: "aarch64-apple-darwin",
        sha256: "41df7d3ae4757e84b97874f76d634268456aaa271740d33f968d826374998fb7",
        lock: include_str!("../resources/paddle-parser/pylock.macos-arm64.toml"),
        lock_sha256: "316f4056bef3db53dcd445ded35f7bc34d0aedcf3a19243e32e7a02deae09260",
    },
    PythonArtifact {
        os: "windows",
        arch: "x86_64",
        target: "x86_64-pc-windows-msvc",
        sha256: "de3e362376859b060fa8b856c434efa81fcf6d4ede3d6e177c7e2169670cac50",
        lock: include_str!("../resources/paddle-parser/pylock.windows-x86_64.toml"),
        lock_sha256: "3b8c4d52cb42020fd88ce84e214c16e7f17e8bb0492ada5b3b292b17391724e8",
    },
    PythonArtifact {
        os: "linux",
        arch: "x86_64",
        target: "x86_64-unknown-linux-gnu",
        sha256: "10a452caac7041357805f0c19a60576df53f1ab06d1abfc9200f1f0157cb3bd1",
        lock: include_str!("../resources/paddle-parser/pylock.linux-x86_64.toml"),
        lock_sha256: "c9e07b1d352347c96403d2162b816b16e43715550db7354f3d61608c501bbcf6",
    },
    PythonArtifact {
        os: "linux",
        arch: "aarch64",
        target: "aarch64-unknown-linux-gnu",
        sha256: "b85154b9c7ca9de3f85f2c9f032d503151db16ef198de86b885fc61890c075ed",
        lock: include_str!("../resources/paddle-parser/pylock.linux-aarch64.toml"),
        lock_sha256: "a17e105f50013713ebacb9a684be5672cf48f8dba625113db74fe429ef3342f3",
    },
];

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

fn python_artifact_for(os: &str, arch: &str) -> Option<&'static PythonArtifact> {
    PYTHON_ARTIFACTS
        .iter()
        .find(|artifact| artifact.os == os && artifact.arch == arch)
}

fn python_artifact() -> Result<&'static PythonArtifact, String> {
    python_artifact_for(std::env::consts::OS, std::env::consts::ARCH).ok_or_else(|| {
        format!(
            "No managed Python parser runtime for {}/{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    })
}

fn python_download_url(artifact: &PythonArtifact) -> String {
    format!(
        "https://releases.astral.sh/github/python-build-standalone/releases/download/{PYTHON_BUILD_RELEASE}/cpython-{PYTHON_VERSION}%2B{PYTHON_BUILD_RELEASE}-{}-install_only_stripped.tar.gz",
        artifact.target
    )
}

fn standalone_python(root: &Path) -> PathBuf {
    if cfg!(windows) {
        root.join("python").join("python.exe")
    } else {
        root.join("python").join("bin").join("python3")
    }
}

fn full_parser_support_for(os: &str, arch: &str) -> Result<(), String> {
    if os == "macos" && arch == "x86_64" {
        return Err(
            "The official PaddlePaddle 3.2.1 runtime has no Intel macOS wheel, so PaddleOCR-VL Full Parser is unavailable on this Mac."
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
    /// Rough total download (packages + model weights), for the UI.
    pub est_download_mb: u64,
    pub est_disk_mb: u64,
}

const PADDLE_RECOGNITION_COMPONENT: EngineSpec = EngineSpec {
    id: "paddleocr-vl",
    label: "PaddleOCR-VL 1.6 Q8 recognition component",
    description: "Internal recognition model and llama.cpp runtime used by the Full Parser.",
    est_download_mb: 1900,
    est_disk_mb: 2300,
};

pub const ENGINES: &[EngineSpec] = &[EngineSpec {
    id: "paddleocr-vl-parser",
    label: "PaddleOCR-VL 1.6 Full Parser",
    description: "Includes the official PaddleOCR layout client, PP-DocLayoutV3, structured \
                      reading order, title hierarchy, formula metadata, and cross-page table \
                      reconstruction, plus the managed Q8 model and llama.cpp server; no \
                      system Python, pip, Conda, or Docker is required.",
    est_download_mb: 2900,
    est_disk_mb: 3800,
}];

const PADDLE_BASE_INSTALLED_MB: u64 = 2300;
const PADDLE_PARSER_ADDON_INSTALLED_MB: u64 = 1500;
const PADDLE_BASE_STAGING_HEADROOM_MB: u64 = 600;
const PADDLE_PARSER_STAGING_HEADROOM_MB: u64 = 700;
const PADDLE_SIDECAR_REFRESH_HEADROOM_MB: u64 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallSpacePlan {
    BaseInstallOrRepair,
    ParserFreshStack,
    ParserAddOn,
    ParserRepairOrUpgrade,
    ParserSidecarRefresh,
}

impl InstallSpacePlan {
    fn label(self) -> &'static str {
        match self {
            Self::BaseInstallOrRepair => "base-engine install or repair",
            Self::ParserFreshStack => "fresh full-parser stack",
            Self::ParserAddOn => "full-parser add-on",
            Self::ParserRepairOrUpgrade => "full-parser repair or upgrade",
            Self::ParserSidecarRefresh => "full-parser sidecar refresh",
        }
    }
}

fn install_space_requirement(
    engine_id: &str,
    base_installed: bool,
    parser_target_present: bool,
    lightweight_parser_refresh: bool,
) -> Result<(InstallSpacePlan, u64), String> {
    match engine_id {
        "paddleocr-vl" => Ok((
            InstallSpacePlan::BaseInstallOrRepair,
            PADDLE_BASE_INSTALLED_MB + PADDLE_BASE_STAGING_HEADROOM_MB,
        )),
        "paddleocr-vl-parser" if lightweight_parser_refresh => Ok((
            InstallSpacePlan::ParserSidecarRefresh,
            PADDLE_SIDECAR_REFRESH_HEADROOM_MB,
        )),
        "paddleocr-vl-parser" if !base_installed => Ok((
            InstallSpacePlan::ParserFreshStack,
            PADDLE_BASE_INSTALLED_MB
                + PADDLE_PARSER_ADDON_INSTALLED_MB
                + PADDLE_PARSER_STAGING_HEADROOM_MB,
        )),
        "paddleocr-vl-parser" if parser_target_present => Ok((
            InstallSpacePlan::ParserRepairOrUpgrade,
            PADDLE_PARSER_ADDON_INSTALLED_MB + PADDLE_PARSER_STAGING_HEADROOM_MB,
        )),
        "paddleocr-vl-parser" => Ok((
            InstallSpacePlan::ParserAddOn,
            PADDLE_PARSER_ADDON_INSTALLED_MB + PADDLE_PARSER_STAGING_HEADROOM_MB,
        )),
        _ => Err(format!("Unknown engine '{engine_id}'")),
    }
}

fn check_install_space(
    free_bytes: u64,
    plan: InstallSpacePlan,
    required_mb: u64,
) -> Result<(), String> {
    let required_bytes = required_mb.saturating_mul(1_000_000);
    if free_bytes >= required_bytes {
        return Ok(());
    }
    Err(format!(
        "Not enough disk space for {}: ~{:.1} GB additional working space is required, {:.1} GB is available.",
        plan.label(),
        required_mb as f64 / 1000.0,
        free_bytes as f64 / 1_000_000_000.0,
    ))
}

fn engine(id: &str) -> Result<&'static EngineSpec, String> {
    if id == PADDLE_RECOGNITION_COMPONENT.id {
        return Ok(&PADDLE_RECOGNITION_COMPONENT);
    }
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
    /// Digest of the verified immutable runtime closure. Cache identities use
    /// this release-bound value rather than mutable path metadata.
    pub integrity_sha256: String,
}

#[derive(Debug, Clone)]
pub struct PaddleFullParserPaths {
    pub python: PathBuf,
    pub script: PathBuf,
    pub model_cache: PathBuf,
    pub layout_model: PathBuf,
    pub paddle: PaddleEnginePaths,
    pub release: String,
    pub integrity_sha256: String,
    pub sidecar_sha256: String,
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

const INSTALL_INTEGRITY_SCHEMA: u32 = 1;
const MAX_INTEGRITY_ENTRIES: usize = 200_000;
const MAX_INTEGRITY_MANIFEST_BYTES: u64 = 32 * 1024 * 1024;
const MAX_INTEGRITY_FILE_BYTES: u64 = 2_000_000_000;
const MAX_INTEGRITY_TOTAL_BYTES: u64 = 8_000_000_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum IntegrityEntryKind {
    File,
    Symlink,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IntegrityEntry {
    path: String,
    kind: IntegrityEntryKind,
    size: u64,
    sha256: String,
    permissions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstallIntegrityManifest {
    schema_version: u32,
    scopes: Vec<String>,
    entries: Vec<IntegrityEntry>,
}

static VERIFIED_INTEGRITY: OnceLock<Mutex<std::collections::HashMap<String, String>>> =
    OnceLock::new();

fn integrity_permissions(metadata: &std::fs::Metadata) -> u32 {
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

fn integrity_relative_path(root: &Path, path: &Path) -> Result<String, String> {
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

fn symlink_target_bytes(path: &Path) -> Result<Vec<u8>, String> {
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

fn ignorable_integrity_metadata(path: &Path, metadata: &std::fs::Metadata) -> bool {
    // Finder may create this inert metadata file after the user opens
    // ~/.pipeline/ from Settings. It is not part of the executable runtime
    // closure, so exclude it while continuing to reject every other unlisted
    // file, symlink, or filesystem object.
    metadata.is_file()
        && !metadata.file_type().is_symlink()
        && path.file_name() == Some(std::ffi::OsStr::new(".DS_Store"))
}

fn collect_integrity_entries(root: &Path, scopes: &[&str]) -> Result<Vec<IntegrityEntry>, String> {
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

fn valid_integrity_path(value: &str) -> bool {
    !value.is_empty()
        && Path::new(value)
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

fn collect_integrity_paths(root: &Path, scopes: &[String]) -> Result<Vec<String>, String> {
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

fn write_install_integrity(root: &Path, scopes: &[&str]) -> Result<String, String> {
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

fn metadata_time_nanos(value: std::io::Result<std::time::SystemTime>) -> u128 {
    value
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

fn integrity_metadata_digest(root: &Path, entries: &[IntegrityEntry]) -> Result<String, String> {
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
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn verify_install_integrity(
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

fn paddle_paths_at(root: &Path) -> Option<PaddleEnginePaths> {
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

fn bounded_regular_file(path: &Path, max_bytes: u64) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() > 0
            && metadata.len() <= max_bytes
    })
}

fn paddle_install_committed(root: &Path) -> bool {
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

fn paddle_full_parser_status_at(native_root: &Path) -> Option<PaddleFullParserStatus> {
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

fn paddle_full_parser_paths_at(root: &Path) -> Option<PaddleFullParserPaths> {
    let artifact = python_artifact().ok()?;
    let version_root = root.join("versions").join(PADDLE_PARSER_RELEASE);
    let python = venv_python(&version_root.join("venv"));
    let script = version_root.join("paddle_parser_sidecar.py");
    let manifest = version_root.join("install.json");
    let packages = version_root.join("packages.txt");
    let lock = version_root.join("pylock.toml");
    let runtime_lock = version_root.join("runtime-lock.json");
    let integrity = version_root.join("integrity.json");
    let layout_model = version_root.join("models").join("PP-DocLayoutV3_infer");
    if !python.is_file()
        || !script.is_file()
        || !packages.is_file()
        || !lock.is_file()
        || !runtime_lock.is_file()
        || !integrity.is_file()
        || !manifest.is_file()
        || !layout_model.join("inference.json").is_file()
        || !layout_model.join("inference.yml").is_file()
        || !layout_model.join("inference.pdiparams").is_file()
    {
        return None;
    }
    let manifest_value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest).ok()?).ok()?;
    if manifest_value
        .get("engine")
        .and_then(serde_json::Value::as_str)
        != Some("paddleocr-vl-parser")
        || manifest_value
            .get("release")
            .and_then(serde_json::Value::as_str)
            != Some(PADDLE_PARSER_RELEASE)
        || manifest_value
            .get("paddleocr")
            .and_then(serde_json::Value::as_str)
            != Some(PADDLE_PARSER_VERSION)
        || manifest_value
            .get("paddlepaddle")
            .and_then(serde_json::Value::as_str)
            != Some(PADDLE_RUNTIME_VERSION)
        || manifest_value
            .get("python")
            .and_then(serde_json::Value::as_str)
            != Some(PYTHON_VERSION)
        || manifest_value
            .get("python_build_release")
            .and_then(serde_json::Value::as_str)
            != Some(PYTHON_BUILD_RELEASE)
        || manifest_value
            .get("python_artifact_sha256")
            .and_then(serde_json::Value::as_str)
            != Some(artifact.sha256)
        || manifest_value.get("uv").and_then(serde_json::Value::as_str) != Some(UV_VERSION)
        || manifest_value
            .get("layout_model_artifact_sha256")
            .and_then(serde_json::Value::as_str)
            != Some(PADDLE_LAYOUT_MODEL_SHA256)
        || manifest_value
            .get("layout_ready")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        || manifest_value
            .get("offline_model_runtime")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        || manifest_value
            .get("recognition_backend")
            .and_then(serde_json::Value::as_str)
            != Some("managed llama.cpp")
        || manifest_value
            .get("sidecar_contract")
            .and_then(serde_json::Value::as_u64)
            != Some(2)
        || manifest_value
            .get("integrity_schema")
            .and_then(serde_json::Value::as_u64)
            != Some(u64::from(INSTALL_INTEGRITY_SCHEMA))
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
    if std::fs::metadata(&lock).ok()?.len() > 4 * 1024 * 1024 {
        return None;
    }
    let lock_digest = sha256_regular_file(&lock, 4 * 1024 * 1024).ok()?;
    if lock_digest != artifact.lock_sha256
        || manifest_value
            .get("lock_sha256")
            .and_then(serde_json::Value::as_str)
            != Some(lock_digest.as_str())
    {
        return None;
    }
    let runtime_lock_digest = sha256_regular_file(&runtime_lock, 1024 * 1024).ok()?;
    let expected_runtime_lock_digest = format!(
        "{:x}",
        Sha256::digest(PADDLE_PARSER_RUNTIME_LOCK.as_bytes())
    );
    if runtime_lock_digest != expected_runtime_lock_digest
        || manifest_value
            .get("runtime_lock_sha256")
            .and_then(serde_json::Value::as_str)
            != Some(runtime_lock_digest.as_str())
    {
        return None;
    }
    let integrity_sha256 = manifest_value
        .get("integrity_sha256")
        .and_then(serde_json::Value::as_str)?
        .to_string();
    verify_install_integrity(&version_root, &integrity_sha256, &[]).ok()?;
    Some(PaddleFullParserPaths {
        python,
        script,
        model_cache: version_root.join("cache"),
        layout_model,
        paddle: paddle_engine_paths().ok()?,
        release: PADDLE_PARSER_RELEASE.to_string(),
        integrity_sha256,
        sidecar_sha256: script_digest,
    })
}

fn parser_runtime_reusable_for_sidecar_refresh(root: &Path) -> bool {
    let Ok(artifact) = python_artifact() else {
        return false;
    };
    let expected_runtime_lock_digest = format!(
        "{:x}",
        Sha256::digest(PADDLE_PARSER_RUNTIME_LOCK.as_bytes())
    );
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
        || manifest.get("python").and_then(serde_json::Value::as_str) != Some(PYTHON_VERSION)
        || manifest
            .get("python_build_release")
            .and_then(serde_json::Value::as_str)
            != Some(PYTHON_BUILD_RELEASE)
        || manifest
            .get("python_artifact_sha256")
            .and_then(serde_json::Value::as_str)
            != Some(artifact.sha256)
        || manifest
            .get("lock_sha256")
            .and_then(serde_json::Value::as_str)
            != Some(artifact.lock_sha256)
        || manifest
            .get("layout_model_artifact_sha256")
            .and_then(serde_json::Value::as_str)
            != Some(PADDLE_LAYOUT_MODEL_SHA256)
        || manifest
            .get("layout_ready")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        || manifest
            .get("runtime_lock_sha256")
            .and_then(serde_json::Value::as_str)
            != Some(expected_runtime_lock_digest.as_str())
        || manifest.get("uv").and_then(serde_json::Value::as_str) != Some(UV_VERSION)
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
        return false;
    }
    let Some(expected_packages_digest) = manifest
        .get("packages_sha256")
        .and_then(serde_json::Value::as_str)
    else {
        return false;
    };
    let Some(expected_integrity_digest) = manifest
        .get("integrity_sha256")
        .and_then(serde_json::Value::as_str)
    else {
        return false;
    };
    std::fs::read(&packages)
        .is_ok_and(|bytes| format!("{:x}", Sha256::digest(&bytes)) == expected_packages_digest)
        && sha256_regular_file(&lock, 4 * 1024 * 1024)
            .is_ok_and(|digest| digest == artifact.lock_sha256)
        && sha256_regular_file(&runtime_lock, 1024 * 1024)
            .is_ok_and(|digest| digest == expected_runtime_lock_digest)
        && verify_install_integrity(&version_root, expected_integrity_digest, &[]).is_ok()
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
    paddle_full_parser_env_at(&paths.model_cache)
}

fn paddle_full_parser_env_at(model_cache: &Path) -> Vec<(String, String)> {
    vec![
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
    remove_owned_path(path).map(|_| ())
}

/// Remove one exact app-owned path without ever following a symlink. Returns
/// false when the path is already absent.
fn remove_owned_path(path: &Path) -> Result<bool, String> {
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

fn path_present(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn restore_owned_directory(backup: &Path, target: &Path) -> Result<bool, String> {
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

fn remove_children_matching(
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

fn collect_owned_removal(path: &Path, removed: &mut usize, errors: &mut Vec<String>) {
    match remove_owned_path(path) {
        Ok(true) => *removed += 1,
        Ok(false) => {}
        Err(error) => errors.push(error),
    }
}

/// Cheap commit-marker check for startup cleanup. The installer writes this
/// manifest only after the complete runtime and integrity inventory exist;
/// normal engine resolution performs the full cryptographic verification.
fn parser_install_committed(parser_root: &Path) -> bool {
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
fn cleanup_managed_engine_storage_at(
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

fn cleanup_managed_engine_storage_locked() -> Result<usize, String> {
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

static ENGINE_OPERATION_RUNNING: AtomicBool = AtomicBool::new(false);
static INSTALL_RUNNING: AtomicBool = AtomicBool::new(false);
static INSTALL_CANCEL: AtomicBool = AtomicBool::new(false);
static INSTALL_CHILD_PID: Mutex<Option<u32>> = Mutex::new(None);
static INSTALL_PROGRESS: OnceLock<Mutex<InstallProgressState>> = OnceLock::new();

const INSTALL_PROGRESS_LOG_LINES: usize = 200;

#[derive(Default)]
struct InstallProgressState {
    engine_id: String,
    phases: BTreeMap<String, String>,
    log_lines: VecDeque<String>,
}

fn install_progress_state() -> &'static Mutex<InstallProgressState> {
    INSTALL_PROGRESS.get_or_init(|| Mutex::new(InstallProgressState::default()))
}

fn clear_install_progress() {
    *install_progress_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = InstallProgressState::default();
}

fn reset_install_progress(engine_id: &str) {
    let mut progress = install_progress_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    *progress = InstallProgressState {
        engine_id: engine_id.to_string(),
        ..InstallProgressState::default()
    };
}

fn current_install_progress() -> Option<EngineInstallProgress> {
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

fn record_install_log(line: &str) {
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

fn record_install_phase(engine_id: &str, phase: &str, status: &str) {
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
const INSTALL_STEP_TIMEOUT_SECS: u64 = 3600;
const INSTALL_LOG_BYTES_PER_STREAM: usize = 4 * 1024 * 1024;
const INSTALL_LOG_LINE_BYTES: usize = 64 * 1024;
const INSTALL_OUTPUT_GRACE_SECS: u64 = 2;
const INSTALL_OUTPUT_POST_KILL_SECS: u64 = 2;

struct InstallGuard {
    lock_file: std::fs::File,
    installing: bool,
}

fn acquire_engine_guard(installing: bool) -> Result<InstallGuard, String> {
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

fn acquire_install_guard() -> Result<InstallGuard, String> {
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

fn log(app: &crate::emit::EventBus, line: impl Into<String>) {
    let line = line.into();
    record_install_log(&line);
    app.emit_event("engines:log", serde_json::json!({ "line": line }))
        .ok();
}

fn emit_phase(app: &crate::emit::EventBus, engine_id: &str, phase: &str, status: &str) {
    record_install_phase(engine_id, phase, status);
    app.emit_event(
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

fn safe_archive_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path.components().all(|component| {
            matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
}

fn archive_link_stays_within_root(entry_path: &Path, link_name: &Path) -> bool {
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

fn path_has_symlink_component(root: &Path, path: &Path) -> bool {
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

fn unpack_python_archive(archive_path: &Path, destination: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(destination)
        .map_err(|error| format!("Failed to create Python staging directory: {error}"))?;
    let file = crate::safety::open_regular_file(archive_path)
        .map_err(|error| format!("Failed to open Python archive: {error}"))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let mut total = 0u64;
    let mut entries_seen = 0usize;
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
            symlinks.push((target, link_name));
            continue;
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
        #[cfg(unix)]
        std::os::unix::fs::symlink(&link_name, &target)
            .map_err(|error| format!("Failed to extract Python symlink: {error}"))?;
        #[cfg(not(unix))]
        return Err("Unexpected symlink in Windows Python archive".to_string());
    }
    let python = standalone_python(destination);
    if !python.is_file() {
        return Err("Python interpreter was not found in its release archive".to_string());
    }
    Ok(python)
}

fn unpack_layout_model_archive(archive_path: &Path, destination: &Path) -> Result<PathBuf, String> {
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

fn managed_path() -> std::ffi::OsString {
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

fn uv_env() -> Result<Vec<(String, String)>, String> {
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

async fn ensure_uv(app: &crate::emit::EventBus) -> Result<PathBuf, String> {
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

#[derive(Debug, Default)]
struct InstallDrainStats {
    bytes_read: u64,
    bytes_emitted: usize,
    truncated: bool,
}

async fn drain_install_output<R, F>(mut reader: R, mut emit: F) -> Result<InstallDrainStats, String>
where
    R: tokio::io::AsyncRead + Unpin,
    F: FnMut(String),
{
    use tokio::io::AsyncReadExt as _;

    let mut stats = InstallDrainStats::default();
    let mut chunk = [0u8; 8192];
    let mut line = Vec::with_capacity(4096);
    let mut line_truncated = false;
    let mut truncation_reported = false;
    let flush = |line: &mut Vec<u8>,
                 line_truncated: &mut bool,
                 stats: &mut InstallDrainStats,
                 truncation_reported: &mut bool,
                 emit: &mut F| {
        while line.last() == Some(&b'\r') {
            line.pop();
        }
        if !line.is_empty() && stats.bytes_emitted < INSTALL_LOG_BYTES_PER_STREAM {
            let remaining = INSTALL_LOG_BYTES_PER_STREAM - stats.bytes_emitted;
            let count = line.len().min(remaining);
            if count > 0 {
                emit(String::from_utf8_lossy(&line[..count]).into_owned());
                stats.bytes_emitted += count;
            }
        }
        if *line_truncated || stats.bytes_emitted >= INSTALL_LOG_BYTES_PER_STREAM {
            stats.truncated = true;
            if !*truncation_reported {
                emit("[installer output truncated; remaining bytes were drained]".to_string());
                *truncation_reported = true;
            }
        }
        line.clear();
        *line_truncated = false;
    };

    loop {
        let count = reader
            .read(&mut chunk)
            .await
            .map_err(|error| format!("Failed to read installer output: {error}"))?;
        if count == 0 {
            break;
        }
        stats.bytes_read = stats.bytes_read.saturating_add(count as u64);
        for byte in &chunk[..count] {
            if *byte == b'\n' {
                flush(
                    &mut line,
                    &mut line_truncated,
                    &mut stats,
                    &mut truncation_reported,
                    &mut emit,
                );
            } else if line.len() < INSTALL_LOG_LINE_BYTES
                && stats.bytes_emitted < INSTALL_LOG_BYTES_PER_STREAM
            {
                line.push(*byte);
            } else {
                line_truncated = true;
            }
        }
    }
    if !line.is_empty() || line_truncated {
        flush(
            &mut line,
            &mut line_truncated,
            &mut stats,
            &mut truncation_reported,
            &mut emit,
        );
    }
    Ok(stats)
}

async fn await_install_drains(
    stdout_task: &mut tokio::task::JoinHandle<Result<InstallDrainStats, String>>,
    stderr_task: &mut tokio::task::JoinHandle<Result<InstallDrainStats, String>>,
) -> Result<(), String> {
    (&mut *stdout_task)
        .await
        .map_err(|error| format!("Installer stdout task failed: {error}"))??;
    (&mut *stderr_task)
        .await
        .map_err(|error| format!("Installer stderr task failed: {error}"))??;
    Ok(())
}

async fn wait_install_drains_finished(
    stdout_task: &tokio::task::JoinHandle<Result<InstallDrainStats, String>>,
    stderr_task: &tokio::task::JoinHandle<Result<InstallDrainStats, String>>,
) {
    while !stdout_task.is_finished() || !stderr_task.is_finished() {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

async fn finish_install_drains(
    pid: u32,
    stdout_task: &mut tokio::task::JoinHandle<Result<InstallDrainStats, String>>,
    stderr_task: &mut tokio::task::JoinHandle<Result<InstallDrainStats, String>>,
    initial_grace: std::time::Duration,
    post_kill_grace: std::time::Duration,
) -> Result<(), String> {
    if tokio::time::timeout(
        initial_grace,
        wait_install_drains_finished(stdout_task, stderr_task),
    )
    .await
    .is_ok()
    {
        return await_install_drains(stdout_task, stderr_task).await;
    }

    if pid > 0 {
        crate::commands::kill_process(pid);
    }
    if tokio::time::timeout(
        post_kill_grace,
        wait_install_drains_finished(stdout_task, stderr_task),
    )
    .await
    .is_err()
    {
        stdout_task.abort();
        stderr_task.abort();
        return Err("Installer output did not close after terminating descendants".to_string());
    }
    await_install_drains(stdout_task, stderr_task).await?;
    Err("Installer descendants kept output pipes open after the command exited".to_string())
}

fn unregister_install_pid(pid: u32) {
    if pid == 0 {
        return;
    }
    crate::commands::unregister_child_pid(pid);
    let mut active = INSTALL_CHILD_PID
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if *active == Some(pid) {
        *active = None;
    }
}

async fn run_install_step(
    app: &crate::emit::EventBus,
    program: &Path,
    args: &[String],
    environment: &[(String, String)],
    label: &str,
) -> Result<(), String> {
    let mut command = crate::pipeline::claude::build_silent_command(
        program
            .to_str()
            .ok_or_else(|| format!("{label} program path is not valid UTF-8"))?,
        None,
    );
    apply_managed_environment(command.as_std_mut(), environment);
    crate::pipeline::claude::configure_silent_command(command.as_std_mut());
    command.args(args);
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

    let stdout = child
        .stdout
        .take()
        .ok_or("Installer stdout pipe was unavailable")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Installer stderr pipe was unavailable")?;
    let stdout_app = app.clone();
    let mut stdout_task = tokio::spawn(async move {
        drain_install_output(stdout, |line| {
            if !line.trim().is_empty() {
                log(&stdout_app, line);
            }
        })
        .await
    });
    let stderr_app = app.clone();
    let mut stderr_task = tokio::spawn(async move {
        drain_install_output(stderr, |line| {
            if !line.trim().is_empty() {
                log(&stderr_app, line);
            }
        })
        .await
    });

    let waited = tokio::time::timeout(
        std::time::Duration::from_secs(INSTALL_STEP_TIMEOUT_SECS),
        child.wait(),
    )
    .await;
    let timed_out = waited.is_err();
    let (status, wait_error) = match waited {
        Ok(Ok(status)) => (Some(status), None),
        Ok(Err(error)) => (None, Some(format!("Failed waiting for {label}: {error}"))),
        Err(_) => {
            if pid > 0 {
                crate::commands::kill_process(pid);
            }
            let _ = child.start_kill();
            (
                tokio::time::timeout(std::time::Duration::from_secs(5), child.wait())
                    .await
                    .ok()
                    .and_then(Result::ok),
                None,
            )
        }
    };
    let drain_result = finish_install_drains(
        pid,
        &mut stdout_task,
        &mut stderr_task,
        std::time::Duration::from_secs(INSTALL_OUTPUT_GRACE_SECS),
        std::time::Duration::from_secs(INSTALL_OUTPUT_POST_KILL_SECS),
    )
    .await;
    unregister_install_pid(pid);
    drain_result.map_err(|error| format!("{label}: {error}"))?;
    if timed_out {
        return Err(format!(
            "{label} timed out after {INSTALL_STEP_TIMEOUT_SECS}s"
        ));
    }
    if let Some(error) = wait_error {
        return Err(error);
    }
    if INSTALL_CANCEL.load(Ordering::Acquire) {
        return Err("Installation cancelled".to_string());
    }
    let status = status.ok_or_else(|| format!("{label} did not report an exit status"))?;
    if !status.success() {
        return Err(format!(
            "{label} failed (exit {}). Check the install log for details.",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

async fn install_paddle_engine(
    app: &crate::emit::EventBus,
    spec: &EngineSpec,
) -> Result<(), String> {
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

    let integrity_sha256 = write_install_integrity(staging.path(), &["runtime", "models"])?;

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
        "integrity_schema": INSTALL_INTEGRITY_SCHEMA,
        "integrity_sha256": integrity_sha256,
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

async fn install_paddle_full_parser(
    app: &crate::emit::EventBus,
    spec: &EngineSpec,
) -> Result<(), String> {
    full_parser_support()?;
    let python_artifact = python_artifact()?;

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
        let mut environment = uv_env()?;
        let model_cache = target.join("cache");
        environment.extend(paddle_full_parser_env_at(&model_cache));

        let lock = target.join("pylock.toml");
        std::fs::write(&lock, python_artifact.lock)
            .map_err(|error| format!("Failed to write parser lock: {error}"))?;
        let lock_sha256 = sha256_regular_file(&lock, 4 * 1024 * 1024)?;
        if lock_sha256 != python_artifact.lock_sha256 {
            return Err("Embedded parser lock failed its integrity check".to_string());
        }
        let runtime_lock = target.join("runtime-lock.json");
        std::fs::write(&runtime_lock, PADDLE_PARSER_RUNTIME_LOCK)
            .map_err(|error| format!("Failed to write parser runtime lock: {error}"))?;
        let runtime_lock_sha256 = sha256_regular_file(&runtime_lock, 1024 * 1024)?;

        let python_archive = target.join("python.tar.gz");
        download_verified(
            app,
            &python_download_url(python_artifact),
            &python_archive,
            python_artifact.sha256,
            MAX_PYTHON_ARCHIVE_BYTES,
            "CPython runtime",
        )
        .await?;
        let cpython = target.join("cpython");
        let archive_for_task = python_archive.clone();
        let cpython_for_task = cpython.clone();
        let base_python = tokio::task::spawn_blocking(move || {
            unpack_python_archive(&archive_for_task, &cpython_for_task)
        })
        .await
        .map_err(|error| format!("Python extraction task failed: {error}"))??;
        std::fs::remove_file(&python_archive)
            .map_err(|error| format!("Failed to remove staged Python archive: {error}"))?;

        let venv = target.join("venv");
        let python = venv_python(&venv);
        let venv_args = vec![
            "venv".to_string(),
            venv.to_string_lossy().to_string(),
            "--python".to_string(),
            base_python.to_string_lossy().to_string(),
            "--no-python-downloads".to_string(),
            "--no-managed-python".to_string(),
            "--no-project".to_string(),
        ];
        log(
            app,
            format!("Creating a private verified Python {PYTHON_VERSION} runtime"),
        );
        run_install_step(app, &uv, &venv_args, &environment, "parser runtime install").await?;

        let sync_args = vec![
            "pip".to_string(),
            "sync".to_string(),
            "--python".to_string(),
            python.to_string_lossy().to_string(),
            "--strict".to_string(),
            "--require-hashes".to_string(),
            "--no-build".to_string(),
            "--no-index".to_string(),
            "--no-sources".to_string(),
            "--no-python-downloads".to_string(),
            "--no-managed-python".to_string(),
            lock.to_string_lossy().to_string(),
        ];
        log(
            app,
            format!(
                "Installing the checksum-locked PaddleOCR {PADDLE_PARSER_VERSION} runtime"
            ),
        );
        run_install_step(app, &uv, &sync_args, &environment, "parser package sync").await?;

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
            "-I".to_string(),
            "-B".to_string(),
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
        let probe_args = vec![
            "-I".to_string(),
            "-B".to_string(),
            script.to_string_lossy().to_string(),
            "--probe".to_string(),
        ];
        run_install_step(app, &python, &probe_args, &environment, "parser import check").await?;
        emit_phase(app, spec.id, "packages", "done");

        emit_phase(app, spec.id, "models", "running");
        std::fs::create_dir_all(&model_cache)
            .map_err(|error| format!("Failed to create parser cache: {error}"))?;
        let model_archive = target.join("PP-DocLayoutV3_infer.tar");
        download_verified(
            app,
            PADDLE_LAYOUT_MODEL_URL,
            &model_archive,
            PADDLE_LAYOUT_MODEL_SHA256,
            MAX_LAYOUT_MODEL_ARCHIVE_BYTES,
            "PP-DocLayoutV3 model",
        )
        .await?;
        let models = target.join("models");
        let archive_for_task = model_archive.clone();
        let models_for_task = models.clone();
        let layout_model = tokio::task::spawn_blocking(move || {
            unpack_layout_model_archive(&archive_for_task, &models_for_task)
        })
        .await
        .map_err(|error| format!("Layout-model extraction task failed: {error}"))??;
        std::fs::remove_file(&model_archive)
            .map_err(|error| format!("Failed to remove staged model archive: {error}"))?;

        // A successful construction proves that the exact local weights can
        // initialize without a first-use download or recognition request.
        let warm_args = vec![
            "-I".to_string(),
            "-B".to_string(),
            script.to_string_lossy().to_string(),
            "--warm-layout".to_string(),
            "--layout-model-dir".to_string(),
            layout_model.to_string_lossy().to_string(),
            "--server-url".to_string(),
            "http://127.0.0.1:9/v1".to_string(),
        ];
        run_install_step(app, &python, &warm_args, &environment, "layout model validation")
            .await?;
        emit_phase(app, spec.id, "models", "done");

        // Bind every immutable interpreter, package, and model file into one
        // deterministic inventory. Mutable download/model caches are excluded.
        let integrity_sha256 = write_install_integrity(
            &target,
            &[
                "cpython",
                "venv",
                "models",
                "packages.txt",
                "pylock.toml",
                "runtime-lock.json",
            ],
        )?;

        let manifest = serde_json::json!({
            "engine": "paddleocr-vl-parser",
            "release": PADDLE_PARSER_RELEASE,
            "paddleocr": PADDLE_PARSER_VERSION,
            "paddlepaddle": PADDLE_RUNTIME_VERSION,
            "python": PYTHON_VERSION,
            "python_build_release": PYTHON_BUILD_RELEASE,
            "python_artifact_sha256": python_artifact.sha256,
            "uv": UV_VERSION,
            "lock_sha256": lock_sha256,
            "runtime_lock_sha256": runtime_lock_sha256,
            "sidecar_sha256": format!("{:x}", Sha256::digest(PADDLE_PARSER_SCRIPT.as_bytes())),
            "sidecar_contract": 2,
            "packages_sha256": inventory_sha256,
            "layout_model": "PP-DocLayoutV3",
            "layout_model_artifact_sha256": PADDLE_LAYOUT_MODEL_SHA256,
            "layout_ready": true,
            "offline_model_runtime": true,
            "recognition_backend": "managed llama.cpp",
            "integrity_schema": INSTALL_INTEGRITY_SCHEMA,
            "integrity_sha256": integrity_sha256,
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
pub async fn install_engine(app: &crate::emit::EventBus, engine_id: &str) -> Result<(), String> {
    if engine_id != "paddleocr-vl-parser" {
        return Err(format!("Unknown engine '{engine_id}'"));
    }
    let spec = engine(engine_id)?;
    let _guard = acquire_install_guard()?;
    reset_install_progress(engine_id);
    INSTALL_CANCEL.store(false, Ordering::Release);
    let removed = cleanup_managed_engine_storage_locked()?;
    if removed > 0 {
        log(
            app,
            format!("Removed {removed} obsolete managed-engine item(s)"),
        );
    }

    // A sidecar-only parser update reuses the already verified private
    // runtime and does not need the full installation's disk headroom.
    let lightweight_parser_refresh = engine_id == "paddleocr-vl-parser"
        && paddle_parser_root()
            .ok()
            .is_some_and(|root| parser_runtime_reusable_for_sidecar_refresh(&root));

    // Check only the components this action must add. Existing targets already
    // consume their disk space; rollback-safe replacement needs one new copy
    // plus bounded archive/cache headroom, not the whole shared stack again.
    let home = pipeline_home()?;
    std::fs::create_dir_all(&home).map_err(|e| format!("Failed to create ~/.pipeline: {e}"))?;
    let base_installed = paddle_root()
        .ok()
        .is_some_and(|root| paddle_paths_at(&root).is_some());
    let parser_target_present = paddle_parser_version_root()
        .ok()
        .is_some_and(|target| target.exists());
    let (space_plan, required_mb) = install_space_requirement(
        engine_id,
        base_installed,
        parser_target_present,
        lightweight_parser_refresh,
    )?;
    if let Ok(free) = fs2::available_space(&home) {
        if let Err(error) = check_install_space(free, space_plan, required_mb) {
            emit_phase(app, engine_id, "runtime", "failed");
            return Err(format!("{} ({})", error, spec.label));
        }
    }

    let result = install_paddle_full_parser(app, spec).await;
    if result.is_ok() {
        match cleanup_managed_engine_storage_locked() {
            Ok(removed) if removed > 0 => log(
                app,
                format!("Removed {removed} temporary managed-engine item(s)"),
            ),
            Ok(_) => {}
            Err(error) => log(
                app,
                format!("Warning: managed-engine cleanup will retry at next startup: {error}"),
            ),
        }
    }
    result
}

/// Uninstall a registered native engine. Retired engine IDs are intentionally
/// rejected; Pipeline never executes their package managers or entry points.
pub async fn uninstall_engine(app: &crate::emit::EventBus, engine_id: &str) -> Result<(), String> {
    if engine_id != "paddleocr-vl-parser" {
        return Err(format!("Unknown engine '{engine_id}'"));
    }
    let spec = engine(engine_id)?;
    let _guard = acquire_install_guard()?;
    INSTALL_CANCEL.store(false, Ordering::Release);
    cleanup_managed_engine_storage_locked()?;

    let roots = [paddle_parser_root()?, paddle_root()?];
    tokio::task::spawn_blocking(move || {
        for root in roots {
            remove_owned_path(&root)?;
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|error| format!("Managed engine cleanup task failed: {error}"))??;
    log(app, format!("{} uninstalled", spec.label));
    Ok(())
}

/// Remove only the retired app-managed Marker venv and its known launch
/// shims. Model/cache directories and every run artifact remain untouched.
pub async fn remove_retired_marker(app: &crate::emit::EventBus) -> Result<(), String> {
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
        for checksum in [
            PADDLE_MODEL_SHA256,
            PADDLE_MMPROJ_SHA256,
            PADDLE_LAYOUT_MODEL_SHA256,
        ] {
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
        for artifact in PYTHON_ARTIFACTS {
            for checksum in [artifact.sha256, artifact.lock_sha256] {
                assert_eq!(checksum.len(), 64, "{}", artifact.target);
                assert!(checksum.chars().all(|c| c.is_ascii_hexdigit()));
            }
            assert_eq!(
                format!("{:x}", Sha256::digest(artifact.lock.as_bytes())),
                artifact.lock_sha256,
                "{}",
                artifact.target
            );
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
            std::collections::HashSet::from(["paddleocr-vl-parser"])
        );
        assert!(engine("paddleocr-vl").is_ok());
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
    fn managed_engine_cleanup_removes_only_installer_owned_leftovers() {
        let native = tempfile::tempdir().unwrap();
        let native_root = native.path();
        let parser_root = native_root.join("paddleocr-parser");
        let versions = parser_root.join("versions");
        let current = versions.join(PADDLE_PARSER_RELEASE);
        let current_backup = versions.join(format!(".{PADDLE_PARSER_RELEASE}-backup"));
        let old_release = versions.join("paddleocr-3.7.0-paddle-3.2.1-r1");

        for path in [
            native_root.join("paddleocr-vl"),
            native_root.join(".paddleocr-vl-backup"),
            native_root.join(".paddleocr-vl-staging-abandoned"),
            current.clone(),
            current_backup.clone(),
            old_release.clone(),
            parser_root.join("uv-cache"),
            parser_root.join("python"),
            parser_root.join("models"),
            parser_root.join("runtime/.uv-staging-abandoned"),
        ] {
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("fixture"), b"fixture").unwrap();
        }
        std::fs::write(parser_root.join("runtime/uv"), b"uv").unwrap();
        std::fs::write(parser_root.join("runtime/.uv-backup"), b"old uv").unwrap();
        std::fs::create_dir_all(versions.join("notes")).unwrap();
        std::fs::create_dir_all(native_root.join("unrelated-engine-data")).unwrap();

        let removed = cleanup_managed_engine_storage_at(native_root, true).unwrap();
        assert!(removed >= 7);
        assert!(native_root.join("paddleocr-vl").is_dir());
        assert!(!native_root.join(".paddleocr-vl-backup").exists());
        assert!(!native_root.join(".paddleocr-vl-staging-abandoned").exists());
        assert!(current.is_dir());
        assert!(!current_backup.exists());
        assert!(!old_release.exists());
        assert!(!parser_root.join("uv-cache").exists());
        assert!(!parser_root.join("python").exists());
        assert!(!parser_root.join("runtime/.uv-staging-abandoned").exists());
        assert!(!parser_root.join("runtime/.uv-backup").exists());

        // Runtime data and names outside the installer's namespaces survive.
        assert!(parser_root.join("models/fixture").is_file());
        assert!(parser_root.join("runtime/uv").is_file());
        assert!(versions.join("notes").is_dir());
        assert!(native_root.join("unrelated-engine-data").is_dir());
    }

    #[test]
    fn managed_engine_cleanup_recovers_interrupted_swaps() {
        let native = tempfile::tempdir().unwrap();
        let native_root = native.path();
        let base_backup = native_root.join(".paddleocr-vl-backup");
        std::fs::create_dir_all(&base_backup).unwrap();
        std::fs::write(base_backup.join("previous"), b"base").unwrap();

        let parser_root = native_root.join("paddleocr-parser");
        let versions = parser_root.join("versions");
        let current = versions.join(PADDLE_PARSER_RELEASE);
        let current_backup = versions.join(format!(".{PADDLE_PARSER_RELEASE}-backup"));
        std::fs::create_dir_all(&current).unwrap();
        std::fs::write(current.join("partial"), b"partial").unwrap();
        std::fs::create_dir_all(&current_backup).unwrap();
        std::fs::write(current_backup.join("previous"), b"parser").unwrap();
        std::fs::create_dir_all(parser_root.join("runtime")).unwrap();
        std::fs::write(parser_root.join("runtime/.uv-backup"), b"uv").unwrap();

        cleanup_managed_engine_storage_at(native_root, false).unwrap();

        assert!(native_root.join("paddleocr-vl/previous").is_file());
        assert!(!base_backup.exists());
        assert!(current.join("previous").is_file());
        assert!(!current.join("partial").exists());
        assert!(!current_backup.exists());
        assert!(parser_root.join("runtime/uv").is_file());
        assert!(!parser_root.join("runtime/.uv-backup").exists());
        assert_eq!(
            cleanup_managed_engine_storage_at(native_root, true).unwrap(),
            0
        );
    }

    #[test]
    fn managed_engine_cleanup_drops_unrecoverable_partial_parser() {
        let native = tempfile::tempdir().unwrap();
        let current = native
            .path()
            .join("paddleocr-parser/versions")
            .join(PADDLE_PARSER_RELEASE);
        std::fs::create_dir_all(&current).unwrap();
        std::fs::write(current.join("partial"), b"partial").unwrap();

        cleanup_managed_engine_storage_at(native.path(), false).unwrap();

        assert!(!current.exists());
    }

    #[test]
    fn parser_cleanup_commit_marker_is_bounded_and_release_specific() {
        let parser = tempfile::tempdir().unwrap();
        let version = parser.path().join("versions").join(PADDLE_PARSER_RELEASE);
        std::fs::create_dir_all(&version).unwrap();
        let manifest = version.join("install.json");
        std::fs::write(&manifest, b"not json").unwrap();
        assert!(!parser_install_committed(parser.path()));

        std::fs::write(
            &manifest,
            serde_json::to_vec(&serde_json::json!({
                "engine": "paddleocr-vl-parser",
                "release": "obsolete",
                "integrity_sha256": "0".repeat(64),
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(!parser_install_committed(parser.path()));

        std::fs::write(
            &manifest,
            serde_json::to_vec(&serde_json::json!({
                "engine": "paddleocr-vl-parser",
                "release": PADDLE_PARSER_RELEASE,
                "integrity_sha256": "0".repeat(64),
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(parser_install_committed(parser.path()));
    }

    #[test]
    fn parser_status_probe_uses_committed_file_shapes() {
        let native = tempfile::tempdir().unwrap();
        let base = native.path().join("paddleocr-vl");
        let base_models = base.join("models");
        let base_runtime = base.join("runtime");
        std::fs::create_dir_all(&base_models).unwrap();
        std::fs::create_dir_all(&base_runtime).unwrap();
        std::fs::write(base_models.join(PADDLE_MODEL_FILE), b"model").unwrap();
        std::fs::write(base_models.join(PADDLE_MMPROJ_FILE), b"projector").unwrap();
        std::fs::write(base_runtime.join(exe("llama-server")), b"server").unwrap();
        let llama = llama_artifact().unwrap();
        std::fs::write(
            base.join("install.json"),
            serde_json::to_vec(&serde_json::json!({
                "engine": "paddleocr-vl",
                "model_version": "1.6",
                "model_revision": PADDLE_MODEL_REVISION,
                "llama_cpp_version": LLAMA_CPP_VERSION,
                "runtime_sha256": llama.sha256,
                "model_sha256": PADDLE_MODEL_SHA256,
                "mmproj_sha256": PADDLE_MMPROJ_SHA256,
            }))
            .unwrap(),
        )
        .unwrap();

        let parser = native.path().join("paddleocr-parser");
        let version = parser.join("versions").join(PADDLE_PARSER_RELEASE);
        let layout = version.join("models/PP-DocLayoutV3_infer");
        let python = venv_python(&version.join("venv"));
        std::fs::create_dir_all(python.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&layout).unwrap();
        std::fs::write(&python, b"python").unwrap();
        for (path, contents) in [
            (
                version.join("paddle_parser_sidecar.py"),
                b"script".as_slice(),
            ),
            (version.join("packages.txt"), b"packages".as_slice()),
            (version.join("pylock.toml"), b"lock".as_slice()),
            (version.join("runtime-lock.json"), b"runtime".as_slice()),
            (version.join("integrity.json"), b"integrity".as_slice()),
            (layout.join("inference.json"), b"json".as_slice()),
            (layout.join("inference.yml"), b"yaml".as_slice()),
            (layout.join("inference.pdiparams"), b"params".as_slice()),
        ] {
            std::fs::write(path, contents).unwrap();
        }
        std::fs::write(
            version.join("install.json"),
            serde_json::to_vec(&serde_json::json!({
                "engine": "paddleocr-vl-parser",
                "release": PADDLE_PARSER_RELEASE,
                "integrity_sha256": "0".repeat(64),
            }))
            .unwrap(),
        )
        .unwrap();

        let status = paddle_full_parser_status_at(native.path()).unwrap();
        assert_eq!(status.release, PADDLE_PARSER_RELEASE);
        assert_eq!(status.script, version.join("paddle_parser_sidecar.py"));

        std::fs::remove_file(base_models.join(PADDLE_MMPROJ_FILE)).unwrap();
        assert!(paddle_full_parser_status_at(native.path()).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn managed_engine_cleanup_unlinks_staging_symlinks_without_following_them() {
        let native = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let protected = outside.path().join("keep");
        std::fs::write(&protected, b"keep").unwrap();
        let staging_link = native.path().join(".paddleocr-vl-staging-symlink");
        std::os::unix::fs::symlink(outside.path(), &staging_link).unwrap();

        cleanup_managed_engine_storage_at(native.path(), false).unwrap();

        assert!(!staging_link.exists());
        assert!(protected.is_file());
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
    fn python_artifacts_and_locks_cover_supported_full_parser_platforms() {
        let runtime_lock: serde_json::Value =
            serde_json::from_str(PADDLE_PARSER_RUNTIME_LOCK).unwrap();
        assert_eq!(
            runtime_lock
                .get("release")
                .and_then(serde_json::Value::as_str),
            Some(PADDLE_PARSER_RELEASE)
        );
        assert_eq!(
            runtime_lock
                .pointer("/python/version")
                .and_then(serde_json::Value::as_str),
            Some(PYTHON_VERSION)
        );
        assert_eq!(
            runtime_lock
                .pointer("/layoutModel/url")
                .and_then(serde_json::Value::as_str),
            Some(PADDLE_LAYOUT_MODEL_URL)
        );
        assert_eq!(
            runtime_lock
                .pointer("/layoutModel/sha256")
                .and_then(serde_json::Value::as_str),
            Some(PADDLE_LAYOUT_MODEL_SHA256)
        );
        for (os, arch) in [
            ("macos", "aarch64"),
            ("windows", "x86_64"),
            ("linux", "x86_64"),
            ("linux", "aarch64"),
        ] {
            let artifact = python_artifact_for(os, arch)
                .unwrap_or_else(|| panic!("missing Python artifact for {os}/{arch}"));
            assert!(artifact.lock.contains("requires-python = \">=3.12.13\""));
            assert!(artifact
                .lock
                .contains("name = \"paddleocr\"\nversion = \"3.7.0\""));
            assert!(artifact
                .lock
                .contains("name = \"paddlepaddle\"\nversion = \"3.2.1\""));
            assert!(artifact.lock.contains("https://files.pythonhosted.org/"));
            let lock_arch = if os == "macos" && arch == "aarch64" {
                "arm64"
            } else {
                arch
            };
            let key = format!("{os}-{lock_arch}");
            let locked = runtime_lock
                .pointer(&format!("/python/artifacts/{key}"))
                .unwrap();
            assert_eq!(
                locked.get("sha256").and_then(serde_json::Value::as_str),
                Some(artifact.sha256)
            );
            assert_eq!(
                locked.get("lockSha256").and_then(serde_json::Value::as_str),
                Some(artifact.lock_sha256)
            );
            let uv = uv_artifact_for(os, arch).unwrap();
            let locked_uv = runtime_lock
                .pointer(&format!("/uv/artifacts/{key}"))
                .unwrap();
            assert_eq!(
                locked_uv.get("sha256").and_then(serde_json::Value::as_str),
                Some(uv.sha256)
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
        let artifact = python_artifact().unwrap();
        std::fs::write(version_root.join("pylock.toml"), artifact.lock).unwrap();
        std::fs::write(
            version_root.join("runtime-lock.json"),
            PADDLE_PARSER_RUNTIME_LOCK,
        )
        .unwrap();
        let layout_model = version_root.join("models").join("PP-DocLayoutV3_infer");
        std::fs::create_dir_all(&layout_model).unwrap();
        for name in ["inference.json", "inference.yml", "inference.pdiparams"] {
            std::fs::write(layout_model.join(name), b"fixture").unwrap();
        }
        let integrity_sha256 = write_install_integrity(
            &version_root,
            &[
                "venv",
                "models",
                "packages.txt",
                "pylock.toml",
                "runtime-lock.json",
            ],
        )
        .unwrap();
        let manifest = serde_json::json!({
            "engine": "paddleocr-vl-parser",
            "release": PADDLE_PARSER_RELEASE,
            "paddleocr": PADDLE_PARSER_VERSION,
            "paddlepaddle": PADDLE_RUNTIME_VERSION,
            "python": PYTHON_VERSION,
            "python_build_release": PYTHON_BUILD_RELEASE,
            "python_artifact_sha256": artifact.sha256,
            "uv": UV_VERSION,
            "lock_sha256": artifact.lock_sha256,
            "runtime_lock_sha256": format!("{:x}", Sha256::digest(PADDLE_PARSER_RUNTIME_LOCK.as_bytes())),
            "layout_model_artifact_sha256": PADDLE_LAYOUT_MODEL_SHA256,
            "layout_ready": true,
            "offline_model_runtime": true,
            "recognition_backend": "managed llama.cpp",
            "packages_sha256": format!("{:x}", Sha256::digest(packages)),
            "integrity_schema": INSTALL_INTEGRITY_SCHEMA,
            "integrity_sha256": integrity_sha256,
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
    fn install_integrity_detects_same_size_file_replacement() {
        let root = tempfile::tempdir().unwrap();
        let runtime = root.path().join("runtime");
        std::fs::create_dir_all(&runtime).unwrap();
        let program = runtime.join("program");
        std::fs::write(&program, b"trusted").unwrap();
        // Opening ~/.pipeline/ in Finder may add this inert metadata file.
        // It must neither enter nor invalidate the executable inventory.
        std::fs::write(runtime.join(".DS_Store"), b"finder metadata").unwrap();
        let expected_file_sha256 = format!("{:x}", Sha256::digest(b"trusted"));
        let manifest_sha256 = write_install_integrity(root.path(), &["runtime"]).unwrap();
        verify_install_integrity(
            root.path(),
            &manifest_sha256,
            &[("runtime/program", expected_file_sha256.as_str())],
        )
        .unwrap();
        // Exercise the metadata-gated verification cache before replacing the
        // file with a different inode and the same byte length.
        verify_install_integrity(root.path(), &manifest_sha256, &[]).unwrap();
        let injected = runtime.join("injected.py");
        std::fs::write(&injected, b"import me").unwrap();
        assert!(verify_install_integrity(root.path(), &manifest_sha256, &[])
            .unwrap_err()
            .contains("file inventory changed"));
        std::fs::remove_file(injected).unwrap();
        verify_install_integrity(root.path(), &manifest_sha256, &[]).unwrap();
        let replacement = runtime.join("replacement");
        std::fs::write(&replacement, b"changed").unwrap();
        std::fs::remove_file(&program).unwrap();
        std::fs::rename(&replacement, &program).unwrap();
        assert!(verify_install_integrity(root.path(), &manifest_sha256, &[])
            .unwrap_err()
            .contains("content changed"));
    }

    #[test]
    fn install_space_is_incremental_and_includes_staging_headroom() {
        assert_eq!(
            install_space_requirement("paddleocr-vl", false, false, false).unwrap(),
            (InstallSpacePlan::BaseInstallOrRepair, 2900)
        );
        assert_eq!(
            install_space_requirement("paddleocr-vl-parser", false, false, false).unwrap(),
            (InstallSpacePlan::ParserFreshStack, 4500)
        );
        let add_on = install_space_requirement("paddleocr-vl-parser", true, false, false).unwrap();
        assert_eq!(add_on, (InstallSpacePlan::ParserAddOn, 2200));
        assert!(add_on.1 < engine("paddleocr-vl-parser").unwrap().est_disk_mb);
        assert_eq!(
            install_space_requirement("paddleocr-vl-parser", true, true, false).unwrap(),
            (InstallSpacePlan::ParserRepairOrUpgrade, 2200)
        );
        assert_eq!(
            install_space_requirement("paddleocr-vl-parser", true, true, true).unwrap(),
            (InstallSpacePlan::ParserSidecarRefresh, 16)
        );
        assert!(check_install_space(2_200_000_000, add_on.0, add_on.1).is_ok());
        assert!(check_install_space(2_199_999_999, add_on.0, add_on.1).is_err());
    }

    #[test]
    fn archive_paths_reject_traversal_and_absolute_paths() {
        assert!(safe_archive_path(Path::new("build/bin/llama-server")));
        assert!(!safe_archive_path(Path::new("../llama-server")));
        assert!(!safe_archive_path(Path::new("/tmp/llama-server")));
        assert!(archive_link_stays_within_root(
            Path::new("python/share/terminfo/x/xterm"),
            Path::new("../78/xterm")
        ));
        assert!(!archive_link_stays_within_root(
            Path::new("python/link"),
            Path::new("../../outside")
        ));
    }

    #[test]
    fn managed_environment_drops_ambient_python_and_package_manager_overrides() {
        let mut command = std::process::Command::new("managed-program");
        command
            .env("PYTHONPATH", "/tmp/injected")
            .env("PYTHONHOME", "/tmp/injected-python")
            .env("PIP_INDEX_URL", "https://attacker.invalid/simple")
            .env("UV_INDEX", "https://attacker.invalid/simple");
        apply_managed_environment(
            &mut command,
            &[("UV_NO_CONFIG".to_string(), "1".to_string())],
        );
        let environment = command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().to_string(),
                    value.map(|item| item.to_string_lossy().to_string()),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        for forbidden in ["PYTHONPATH", "PYTHONHOME", "PIP_INDEX_URL", "UV_INDEX"] {
            assert!(!environment.contains_key(forbidden));
        }
        assert_eq!(
            environment.get("UV_NO_CONFIG").and_then(Option::as_deref),
            Some("1")
        );
        assert!(environment.get("PATH").and_then(Option::as_deref).is_some());
    }

    #[test]
    fn installer_output_drain_bounds_lines_and_total_log_bytes() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            use tokio::io::AsyncWriteExt as _;

            let (mut writer, reader) = tokio::io::duplex(32 * 1024);
            let writer_task = tokio::spawn(async move {
                let chunk = vec![b'x'; 32 * 1024];
                for _ in 0..160 {
                    writer.write_all(&chunk).await.unwrap();
                }
                writer.write_all(b"\n").await.unwrap();
            });
            let emitted = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
            let captured = emitted.clone();
            let stats = drain_install_output(reader, move |line| {
                captured.lock().unwrap().push(line);
            })
            .await
            .unwrap();
            writer_task.await.unwrap();
            assert_eq!(stats.bytes_read, 160 * 32 * 1024 + 1);
            assert!(stats.bytes_emitted <= INSTALL_LOG_BYTES_PER_STREAM);
            assert!(stats.truncated);
            let lines = emitted.lock().unwrap();
            assert!(lines.iter().all(|line| {
                line.len() <= INSTALL_LOG_LINE_BYTES
                    || line == "[installer output truncated; remaining bytes were drained]"
            }));
        });
    }

    #[test]
    fn installer_output_join_has_a_post_exit_deadline() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut stdout = tokio::spawn(async {
                std::future::pending::<Result<InstallDrainStats, String>>().await
            });
            let mut stderr = tokio::spawn(async { Ok(InstallDrainStats::default()) });
            let started = std::time::Instant::now();
            let error = finish_install_drains(
                0,
                &mut stdout,
                &mut stderr,
                std::time::Duration::from_millis(20),
                std::time::Duration::from_millis(20),
            )
            .await
            .unwrap_err();
            assert!(error.contains("did not close"));
            assert!(started.elapsed() < std::time::Duration::from_secs(1));
            assert!(stdout.await.unwrap_err().is_cancelled());
        });
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
