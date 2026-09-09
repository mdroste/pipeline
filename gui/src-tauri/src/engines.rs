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
const MAX_LLAMA_ARCHIVE_ENTRIES: usize = 100_000;

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

mod installer_io;
mod integrity;
mod operations;
mod paddle_install;
mod process;
mod runtime;
mod storage;

pub use installer_io::*;
pub use operations::*;
pub use runtime::*;
pub use storage::*;

use integrity::*;
use paddle_install::*;
use process::*;

#[cfg(test)]
mod tests;

pub(crate) use runtime::paddle_full_parser_lease;
