//! Managed local-engine provisioning.
//!
//! Python engines are provisioned via a pinned uv binary. Native engines use
//! pinned, SHA-256-verified runtime and model artifacts. Everything remains
//! inside the app-owned toolchain under `~/.pipeline/`:
//!
//! ```text
//! ~/.pipeline/
//! ├── bin/       uv + tool entry-point shims   (UV_TOOL_BIN_DIR)
//! ├── tools/     per-engine venvs              (UV_TOOL_DIR)
//! ├── python/    managed CPython               (UV_PYTHON_INSTALL_DIR)
//! ├── uv-cache/  wheel cache                   (UV_CACHE_DIR)
//! ├── hf/        model weights                 (HF_HOME)
//! └── native/    native runtimes + GGUF models
//! ```
//!
//! Nothing touches system Python, and deleting `~/.pipeline/` removes the
//! entire stack. Installs stream progress to the frontend via
//! `engines:phase` / `engines:log` events.

use serde::Serialize;
use sha2::{Digest, Sha256};
#[cfg(windows)]
use std::io::Read as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

// ── Pinned uv release ───────────────────────────────────────────────
//
// Checksums are Astral's published .sha256 files for this release. Update
// the version and all five hashes together.

const UV_VERSION: &str = "0.11.26";
const MAX_UV_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_UV_BINARY_BYTES: u64 = 128 * 1024 * 1024;

struct UvArtifact {
    target: &'static str,
    sha256: &'static str,
}

const UV_ARTIFACTS: &[UvArtifact] = &[
    UvArtifact {
        target: "aarch64-apple-darwin",
        sha256: "8f7fbf1708399b921857bce71e1d60f0d3ccf52a30caebc1c1a2f175dce13ab6",
    },
    UvArtifact {
        target: "x86_64-apple-darwin",
        sha256: "922b460202707dd5f4ccacbadbe7f6a546cc46e82a99bf50ca99a7977a78eddd",
    },
    UvArtifact {
        target: "x86_64-pc-windows-msvc",
        sha256: "4e1278ede866be6c0bf32d2f466cc6de7a9fb399ecf20c9ce2d186e52424be47",
    },
    UvArtifact {
        target: "x86_64-unknown-linux-gnu",
        sha256: "6426a73c3837e6e2483ee344cbc00f36394d179afcba6183cb77437e67db4af0",
    },
    UvArtifact {
        target: "aarch64-unknown-linux-gnu",
        sha256: "befa1a59c91e96eb601b0fd9a97c03dd666f17baba644b2b4db9c59a767e387e",
    },
];

/// The uv target triple for a given OS/arch pair.
fn uv_target(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("windows", "x86_64") => Some("x86_64-pc-windows-msvc"),
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-gnu"),
        _ => None,
    }
}

fn uv_artifact() -> Result<&'static UvArtifact, String> {
    let target = uv_target(std::env::consts::OS, std::env::consts::ARCH).ok_or_else(|| {
        format!(
            "No managed uv build for {}/{}. Install engines manually (pip install) instead.",
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    })?;
    UV_ARTIFACTS
        .iter()
        .find(|a| a.target == target)
        .ok_or_else(|| format!("No pinned checksum for uv target {target}"))
}

fn uv_artifact_name(target: &str) -> String {
    if target.contains("windows") {
        format!("uv-{target}.zip")
    } else {
        format!("uv-{target}.tar.gz")
    }
}

fn uv_download_url(target: &str) -> String {
    format!(
        "https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/{}",
        uv_artifact_name(target)
    )
}

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

// ── Engine registry ─────────────────────────────────────────────────

pub struct EngineSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub kind: EngineKind,
    /// Rough total download (packages + model weights), for the UI and the
    /// pre-install disk check.
    pub est_download_mb: u64,
    pub est_disk_mb: u64,
}

#[derive(Clone, Copy)]
pub enum EngineKind {
    Python {
        pip_spec: &'static str,
        entry_point: &'static str,
    },
    PaddleOcrVl,
}

pub const ENGINES: &[EngineSpec] = &[
    EngineSpec {
        id: "marker",
        label: "marker-pdf",
        description: "Local PDF-to-Markdown conversion with equations preserved. \
                      Runs on CPU or Apple Silicon; no API calls. GPL-3.0 code; \
                      model weights under Datalab's revenue-capped OpenRAIL-M license.",
        // Pin the last release exercised by Pipeline's extraction integration.
        // A live index spec can replace the engine underneath an unchanged app.
        kind: EngineKind::Python {
            pip_spec: "marker-pdf==1.10.2",
            entry_point: "marker_single",
        },
        est_download_mb: 3500,
        est_disk_mb: 6000,
    },
    EngineSpec {
        id: "paddleocr-vl",
        label: "PaddleOCR-VL 1.6 Q8",
        description: "Compact local vision-language PDF extraction using the official \
                      PaddleOCR-VL 1.6 Q8 GGUF model and a managed llama.cpp runtime. \
                      About 1.9 GB; no Python, containers, API calls, or usage fees. \
                      Apache-2.0 model and MIT runtime.",
        kind: EngineKind::PaddleOcrVl,
        est_download_mb: 1900,
        est_disk_mb: 2300,
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

/// `~/.pipeline/bin` — where uv and tool entry-point shims live.
pub fn managed_bin_dir() -> Option<PathBuf> {
    pipeline_home().ok().map(|h| h.join("bin"))
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn uv_binary_path() -> Result<PathBuf, String> {
    Ok(pipeline_home()?.join("bin").join(exe("uv")))
}

/// Find a managed tool executable (e.g. `marker_single`) if installed.
/// Callers check this before falling back to PATH so a one-click install
/// wins over a stale system copy.
pub fn find_managed(name: &str) -> Option<PathBuf> {
    let candidate = managed_bin_dir()?.join(exe(name));
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub struct PaddleEnginePaths {
    pub server: PathBuf,
    pub model: PathBuf,
    pub mmproj: PathBuf,
}

fn paddle_root() -> Result<PathBuf, String> {
    Ok(pipeline_home()?.join("native").join("paddleocr-vl"))
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

/// Extra environment for *running* a managed tool: keeps model downloads
/// inside `~/.pipeline/` instead of the user's global HuggingFace cache.
/// Only apply when the tool actually resolved from the managed bin dir —
/// redirecting a system install's cache would be a surprising side effect.
pub fn tool_env() -> Vec<(String, String)> {
    match pipeline_home() {
        Ok(home) => vec![(
            "HF_HOME".to_string(),
            home.join("hf").to_string_lossy().to_string(),
        )],
        Err(_) => vec![],
    }
}

/// Environment for uv invocations: the app-owned toolchain layout, plus
/// system trust stores so corporate/university TLS interception (the top
/// real-world failure mode) doesn't break downloads.
fn uv_env() -> Result<Vec<(String, String)>, String> {
    let home = pipeline_home()?;
    let s = |p: PathBuf| p.to_string_lossy().to_string();
    Ok(vec![
        ("UV_TOOL_DIR".to_string(), s(home.join("tools"))),
        ("UV_TOOL_BIN_DIR".to_string(), s(home.join("bin"))),
        ("UV_PYTHON_INSTALL_DIR".to_string(), s(home.join("python"))),
        ("UV_CACHE_DIR".to_string(), s(home.join("uv-cache"))),
        ("UV_SYSTEM_CERTS".to_string(), "1".to_string()),
        ("HF_HOME".to_string(), s(home.join("hf"))),
    ])
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
    /// A non-managed copy found on PATH (pip, anaconda, …), empty if none.
    /// Shown on the install card: it is what extraction falls back to while
    /// no managed install exists.
    pub system_path: String,
    pub est_download_mb: u64,
    pub est_disk_mb: u64,
    /// Size of the whole managed stack (shared across engines), in MB.
    pub managed_stack_mb: u64,
    pub installing: bool,
}

/// Parse `uv tool list` output for a package's version.
/// The format is one `name vX.Y.Z` header line per tool, followed by
/// indented entry-point lines.
fn parse_uv_tool_list(output: &str, pip_spec: &str) -> Option<String> {
    let package = pip_spec
        .split(['=', '<', '>', '!', '~'])
        .next()
        .unwrap_or(pip_spec);
    for line in output.lines() {
        let line = line.trim();
        let mut parts = line.split_whitespace();
        if parts.next() == Some(package) {
            if let Some(ver) = parts.next() {
                return Some(ver.trim_start_matches('v').to_string());
            }
        }
    }
    None
}

/// Status of every registry engine. Spawns `uv tool list` at most once,
/// and only when the managed uv binary exists.
pub fn engine_statuses() -> Vec<EngineStatus> {
    let tool_list = uv_binary_path()
        .ok()
        .filter(|p| p.is_file())
        .and_then(|uv| {
            let env = uv_env().ok()?;
            let mut cmd = std::process::Command::new(uv);
            cmd.args(["tool", "list"]);
            for (k, v) in env {
                cmd.env(k, v);
            }
            let out = crate::process::run_bounded(
                &mut cmd,
                std::time::Duration::from_secs(5),
                1024 * 1024,
            )
            .ok()?;
            if !out.status.success() || out.stdout_truncated || out.stderr_truncated {
                return None;
            }
            Some(String::from_utf8_lossy(&out.stdout).to_string())
        });

    let stack_mb = pipeline_home()
        .map(|home| {
            ["bin", "tools", "python", "uv-cache", "hf", "native"]
                .iter()
                .map(|d| dir_size(&home.join(d)))
                .sum::<u64>()
                / 1_000_000
        })
        .unwrap_or(0);
    let installing = INSTALL_RUNNING.load(Ordering::Acquire);

    let managed_dir = managed_bin_dir();
    ENGINES
        .iter()
        .map(|spec| {
            let (entry, version, system_path) = match spec.kind {
                EngineKind::Python {
                    pip_spec,
                    entry_point,
                } => {
                    let entry = find_managed(entry_point);
                    let version = tool_list
                        .as_deref()
                        .and_then(|out| parse_uv_tool_list(out, pip_spec))
                        .unwrap_or_default();
                    // A copy elsewhere on PATH (pip/anaconda). Exclude the
                    // managed dir in case ~/.pipeline/bin is on PATH.
                    let system_path = crate::deps::find_on_path(entry_point)
                        .filter(|p| managed_dir.as_ref().is_none_or(|d| !p.starts_with(d)))
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    (entry, version, system_path)
                }
                EngineKind::PaddleOcrVl => {
                    let paths = paddle_root().ok().and_then(|root| paddle_paths_at(&root));
                    (
                        paths.as_ref().map(|paths| paths.server.clone()),
                        paths
                            .as_ref()
                            .map(|_| "1.6 Q8".to_string())
                            .unwrap_or_default(),
                        String::new(),
                    )
                }
            };
            EngineStatus {
                id: spec.id.to_string(),
                label: spec.label.to_string(),
                description: spec.description.to_string(),
                installed: entry.is_some(),
                version,
                entry_path: entry
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default(),
                system_path,
                est_download_mb: spec.est_download_mb,
                est_disk_mb: spec.est_disk_mb,
                managed_stack_mb: stack_mb,
                installing,
            }
        })
        .collect()
}

// ── Install lifecycle ───────────────────────────────────────────────

static INSTALL_RUNNING: AtomicBool = AtomicBool::new(false);
static INSTALL_CANCEL: AtomicBool = AtomicBool::new(false);
static INSTALL_CHILD_PID: Mutex<Option<u32>> = Mutex::new(None);

/// Hard ceiling on any single install subprocess. Model downloads on slow
/// connections are the long pole; an hour of no completion means stuck.
const INSTALL_STEP_TIMEOUT_SECS: u64 = 3600;
const INSTALL_LOG_DRAIN_TIMEOUT_SECS: u64 = 5;

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

fn kill_install_child() {
    let pid = INSTALL_CHILD_PID
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    if let Some(pid) = pid {
        crate::commands::kill_process(pid);
    }
}

/// Request cancellation of a running install.
pub fn cancel_install() {
    INSTALL_CANCEL.store(true, Ordering::Release);
    kill_install_child();
}

fn log(app: &AppHandle, line: impl Into<String>) {
    app.emit("engines:log", serde_json::json!({ "line": line.into() }))
        .ok();
}

async fn wait_install_child(
    child: &mut tokio::process::Child,
    timeout: std::time::Duration,
) -> Result<Option<std::process::ExitStatus>, String> {
    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(status) => status
            .map(Some)
            .map_err(|e| format!("Failed waiting for install process: {e}")),
        Err(_) => {
            // Kill and reap before any stdout/stderr reader is joined. Those
            // readers wait for EOF and would otherwise deadlock the timeout
            // path while the child still owns its pipe handles.
            if let Some(pid) = child.id() {
                crate::commands::kill_process(pid);
            }
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(INSTALL_LOG_DRAIN_TIMEOUT_SECS),
                child.kill(),
            )
            .await;
            Ok(None)
        }
    }
}

async fn finish_log_task(task: &mut tokio::task::JoinHandle<()>) {
    if tokio::time::timeout(
        std::time::Duration::from_secs(INSTALL_LOG_DRAIN_TIMEOUT_SECS),
        &mut *task,
    )
    .await
    .is_err()
    {
        task.abort();
        let _ = task.await;
    }
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

/// Download uv (pinned version, checksum-verified) into `~/.pipeline/bin/`
/// unless a matching version is already there.
async fn ensure_uv(app: &AppHandle) -> Result<PathBuf, String> {
    let uv_path = uv_binary_path()?;
    if uv_path.is_file() {
        // Accept only the pinned version so the behavior we tested is the
        // behavior users get; anything else is replaced.
        let mut command = std::process::Command::new(&uv_path);
        command.arg("--version");
        let current =
            crate::process::run_bounded(&mut command, std::time::Duration::from_secs(5), 64 * 1024)
                .ok()
                .filter(|output| {
                    output.status.success() && !output.stdout_truncated && !output.stderr_truncated
                })
                .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
                .unwrap_or_default();
        if current.contains(UV_VERSION) {
            return Ok(uv_path);
        }
        log(app, format!("Replacing managed uv ({} wanted)", UV_VERSION));
    }

    let artifact = uv_artifact()?;
    let url = uv_download_url(artifact.target);
    log(app, format!("Downloading uv {UV_VERSION} ({url})"));

    let bin_dir = uv_path
        .parent()
        .ok_or("uv path has no parent directory")?
        .to_path_buf();
    std::fs::create_dir_all(&bin_dir).map_err(|e| format!("Failed to create bin dir: {e}"))?;

    let client = &*crate::pipeline::api_common::HTTP_CLIENT;
    let mut resp = await_install_operation(
        client
            .get(&url)
            .timeout(std::time::Duration::from_secs(300))
            .send(),
    )
    .await?
    .map_err(|e| format!("Failed to download uv: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("uv download failed: HTTP {}", resp.status()));
    }
    if resp
        .content_length()
        .is_some_and(|length| length > MAX_UV_ARCHIVE_BYTES)
    {
        return Err(format!(
            "uv archive exceeds the {} MB download limit",
            MAX_UV_ARCHIVE_BYTES / 1024 / 1024
        ));
    }

    // Keep the archive on the destination filesystem, but never in memory or
    // at the final binary path. Both temporary files are removed on any error.
    let mut archive = tempfile::Builder::new()
        .prefix(".uv-download-")
        .tempfile_in(&bin_dir)
        .map_err(|e| format!("Failed to create temporary uv archive: {e}"))?;
    let mut hasher = Sha256::new();
    let mut archive_bytes = 0u64;
    while let Some(chunk) = await_install_operation(resp.chunk())
        .await?
        .map_err(|e| format!("Failed to read uv download: {e}"))?
    {
        if INSTALL_CANCEL.load(Ordering::Acquire) {
            return Err("Installation cancelled".to_string());
        }
        archive_bytes = archive_bytes
            .checked_add(chunk.len() as u64)
            .ok_or("uv archive size overflow")?;
        if archive_bytes > MAX_UV_ARCHIVE_BYTES {
            return Err(format!(
                "uv archive exceeds the {} MB download limit",
                MAX_UV_ARCHIVE_BYTES / 1024 / 1024
            ));
        }
        hasher.update(&chunk);
        archive
            .write_all(&chunk)
            .map_err(|e| format!("Failed to store uv download: {e}"))?;
    }
    archive
        .as_file()
        .sync_all()
        .map_err(|e| format!("Failed to sync uv download: {e}"))?;

    let digest = format!("{:x}", hasher.finalize());
    if digest != artifact.sha256 {
        return Err(format!(
            "uv download checksum mismatch (expected {}, got {digest}). \
             Refusing to install — retry, or check for a proxy interfering with downloads.",
            artifact.sha256
        ));
    }
    log(
        app,
        format!("Verified uv archive ({} MB)", archive_bytes / 1_000_000),
    );

    let staged = tempfile::Builder::new()
        .prefix(".uv-executable-")
        .tempfile_in(&bin_dir)
        .map_err(|e| format!("Failed to stage uv executable: {e}"))?;
    let archive_path = archive.path().to_path_buf();
    let staged_path = staged.path().to_path_buf();
    tokio::task::spawn_blocking(move || unpack_uv(&archive_path, &staged_path))
        .await
        .map_err(|e| format!("Unpack task failed: {e}"))??;

    staged
        .as_file()
        .sync_all()
        .map_err(|e| format!("Failed to sync staged uv executable: {e}"))?;
    let mut probe = std::process::Command::new(staged.path());
    probe.arg("--version");
    let output =
        crate::process::run_bounded(&mut probe, std::time::Duration::from_secs(10), 64 * 1024)
            .map_err(|e| format!("Staged uv validation failed: {e}"))?;
    let version = String::from_utf8_lossy(&output.stdout);
    if !output.status.success()
        || output.stdout_truncated
        || output.stderr_truncated
        || !version.contains(UV_VERSION)
    {
        return Err(format!(
            "Staged uv failed its version check (wanted {UV_VERSION})"
        ));
    }
    if INSTALL_CANCEL.load(Ordering::Acquire) {
        return Err("Installation cancelled".to_string());
    }

    // `persist` performs the final same-filesystem replacement. Until this
    // point an existing working uv remains untouched.
    staged
        .into_temp_path()
        .persist(&uv_path)
        .map_err(|e| format!("Failed to install validated uv: {}", e.error))?;

    #[cfg(unix)]
    if let Ok(directory) = std::fs::File::open(&bin_dir) {
        let _ = directory.sync_all();
    }

    log(app, format!("Installed uv to {}", uv_path.display()));
    Ok(uv_path)
}

/// Extract the `uv` binary from the release archive into `dest`.
#[cfg(not(windows))]
fn unpack_uv(archive_path: &Path, dest: &Path) -> Result<(), String> {
    use std::io::Read as _;
    let archive_file = crate::safety::open_regular_file(archive_path)
        .map_err(|e| format!("Failed to open uv archive: {e}"))?;
    let gz = flate2::read::GzDecoder::new(archive_file);
    let mut archive = tar::Archive::new(gz);
    for entry in archive
        .entries()
        .map_err(|e| format!("Invalid uv archive: {e}"))?
    {
        let entry = entry.map_err(|e| format!("Invalid uv archive entry: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("Invalid path in uv archive: {e}"))?
            .into_owned();
        if path.file_name().and_then(|n| n.to_str()) == Some("uv") {
            if entry.size() > MAX_UV_BINARY_BYTES {
                return Err("uv executable exceeds the extraction limit".to_string());
            }
            let mut out = std::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(dest)
                .map_err(|e| format!("Failed to open staged uv: {e}"))?;
            let copied = std::io::copy(&mut entry.take(MAX_UV_BINARY_BYTES + 1), &mut out)
                .map_err(|e| format!("Failed to extract uv: {e}"))?;
            if copied > MAX_UV_BINARY_BYTES {
                return Err("uv executable exceeds the extraction limit".to_string());
            }
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("Failed to set uv permissions: {e}"))?;
            return Ok(());
        }
    }
    Err("uv binary not found in the downloaded archive".to_string())
}

#[cfg(windows)]
fn unpack_uv(archive_path: &Path, dest: &Path) -> Result<(), String> {
    let archive_file = crate::safety::open_regular_file(archive_path)
        .map_err(|e| format!("Failed to open uv archive: {e}"))?;
    let mut archive =
        zip::ZipArchive::new(archive_file).map_err(|e| format!("Invalid uv archive: {e}"))?;
    let names: Vec<String> = archive.file_names().map(|n| n.to_string()).collect();
    let entry_name = names
        .iter()
        .find(|n| n.ends_with("uv.exe"))
        .ok_or("uv.exe not found in the downloaded archive")?
        .clone();
    let entry = archive
        .by_name(&entry_name)
        .map_err(|e| format!("Failed to open uv.exe in archive: {e}"))?;
    if entry.size() > MAX_UV_BINARY_BYTES {
        return Err("uv executable exceeds the extraction limit".to_string());
    }
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(dest)
        .map_err(|e| format!("Failed to open staged uv.exe: {e}"))?;
    let copied = std::io::copy(&mut entry.take(MAX_UV_BINARY_BYTES + 1), &mut out)
        .map_err(|e| format!("Failed to write uv.exe: {e}"))?;
    if copied > MAX_UV_BINARY_BYTES {
        return Err("uv executable exceeds the extraction limit".to_string());
    }
    Ok(())
}

/// Run a subprocess for an install phase, streaming its output to the log
/// panel, with cancellation and a hard timeout.
async fn run_install_step(
    app: &AppHandle,
    program: &Path,
    args: &[&str],
    env: &[(String, String)],
    label: &str,
) -> Result<(), String> {
    use tokio::io::BufReader;

    let mut cmd = crate::pipeline::claude::build_silent_command(
        program.to_str().ok_or("Program path is not valid UTF-8")?,
        None,
    );
    cmd.args(args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start {label}: {e}"))?;
    let pid = child.id().unwrap_or(0);
    if pid > 0 {
        *INSTALL_CHILD_PID.lock().unwrap_or_else(|e| e.into_inner()) = Some(pid);
        crate::commands::register_process_job(pid);
    }

    let app_out = app.clone();
    let stdout = child.stdout.take();
    let mut out_task = tokio::spawn(async move {
        if let Some(stdout) = stdout {
            let mut reader = BufReader::new(stdout);
            while let Ok(Some(record)) = crate::pipeline::logging::next_bounded_line(
                &mut reader,
                crate::pipeline::logging::MAX_CLI_LINE_BYTES,
            )
            .await
            {
                let line = record.text;
                if !line.trim().is_empty() {
                    log(&app_out, line);
                }
            }
        }
    });
    let app_err = app.clone();
    let stderr = child.stderr.take();
    let mut err_task = tokio::spawn(async move {
        if let Some(stderr) = stderr {
            let mut reader = BufReader::new(stderr);
            while let Ok(Some(record)) = crate::pipeline::logging::next_bounded_line(
                &mut reader,
                crate::pipeline::logging::MAX_CLI_LINE_BYTES,
            )
            .await
            {
                let line = record.text;
                if !line.trim().is_empty() {
                    log(&app_err, line);
                }
            }
        }
    });

    let status = wait_install_child(
        &mut child,
        std::time::Duration::from_secs(INSTALL_STEP_TIMEOUT_SECS),
    )
    .await;
    // Keep the PID available to cancel_install until the child has exited or
    // the timeout path has killed and reaped it.
    *INSTALL_CHILD_PID.lock().unwrap_or_else(|e| e.into_inner()) = None;
    crate::commands::unregister_process_job(pid);
    finish_log_task(&mut out_task).await;
    finish_log_task(&mut err_task).await;

    let status = match status {
        Ok(Some(status)) => status,
        Ok(None) => {
            return Err(format!(
                "{label} timed out after {INSTALL_STEP_TIMEOUT_SECS}s"
            ));
        }
        Err(e) => return Err(format!("Failed waiting for {label}: {e}")),
    };

    if INSTALL_CANCEL.load(Ordering::Acquire) {
        return Err("Install cancelled".to_string());
    }
    if !status.success() {
        return Err(format!(
            "{label} failed (exit {}). Check the log for details.",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

/// A minimal one-page PDF, generated with correct xref offsets, used to
/// pre-warm an engine so model weights download under the install progress
/// UI instead of during the user's first real run.
fn minimal_pdf_bytes() -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
         /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>"
            .to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        {
            let stream = "BT /F1 24 Tf 72 720 Td (Pipeline engine check) Tj ET";
            format!(
                "<< /Length {} >>\nstream\n{stream}\nendstream",
                stream.len()
            )
        },
    ];
    let mut body = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(body.len());
        body.push_str(&format!("{} 0 obj\n{obj}\nendobj\n", i + 1));
    }
    let xref_offset = body.len();
    body.push_str(&format!("xref\n0 {}\n", objects.len() + 1));
    body.push_str("0000000000 65535 f \n");
    for off in &offsets {
        body.push_str(&format!("{off:010} 00000 n \n"));
    }
    body.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
        objects.len() + 1
    ));
    body.into_bytes()
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

/// Install an engine: uv runtime → packages → model pre-warm. Streams
/// progress via `engines:phase` and `engines:log` events.
pub async fn install_engine(app: &AppHandle, engine_id: &str) -> Result<(), String> {
    let spec = engine(engine_id)?;
    let _guard = acquire_install_guard()?;
    INSTALL_CANCEL.store(false, Ordering::Release);

    // Disk-space precheck against the estimate, with headroom.
    let home = pipeline_home()?;
    std::fs::create_dir_all(&home).map_err(|e| format!("Failed to create ~/.pipeline: {e}"))?;
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

    if matches!(spec.kind, EngineKind::PaddleOcrVl) {
        return install_paddle_engine(app, spec).await;
    }
    let EngineKind::Python {
        pip_spec,
        entry_point,
    } = spec.kind
    else {
        unreachable!("native engines return above");
    };

    // Phase 1: runtime.
    emit_phase(app, engine_id, "runtime", "running");
    let uv = match ensure_uv(app).await {
        Ok(p) => p,
        Err(e) => {
            emit_phase(app, engine_id, "runtime", "failed");
            return Err(e);
        }
    };
    emit_phase(app, engine_id, "runtime", "done");
    if INSTALL_CANCEL.load(Ordering::Acquire) {
        return Err("Install cancelled".to_string());
    }

    // Phase 2: packages.
    emit_phase(app, engine_id, "packages", "running");
    let env = uv_env()?;
    let mut args = vec![
        "tool", "install", pip_spec, "--python", "3.12",
        // Reinstall cleanly over any prior (possibly broken) install; the
        // wheel cache makes a repeat run cheap.
        "--force",
    ];
    // On Linux the PyPI default torch is the CUDA build (~3-4 GB download,
    // 6-8 GB installed) that most users can't use; Windows benefits from the
    // slimmer +cpu wheel too. Prefer PyTorch's CPU index there. macOS needs
    // no pin — the default wheel is MPS-enabled.
    if cfg!(any(target_os = "linux", target_os = "windows")) {
        args.extend([
            "--index",
            "https://download.pytorch.org/whl/cpu",
            "--index-strategy",
            "unsafe-best-match",
        ]);
    }
    log(app, format!("$ uv {}", args.join(" ")));
    if let Err(e) = run_install_step(app, &uv, &args, &env, "package install").await {
        emit_phase(app, engine_id, "packages", "failed");
        return Err(e);
    }
    emit_phase(app, engine_id, "packages", "done");
    if INSTALL_CANCEL.load(Ordering::Acquire) {
        return Err("Install cancelled".to_string());
    }

    let entry = find_managed(entry_point).ok_or_else(|| {
        format!(
            "Install finished but {} did not appear in the managed bin directory",
            entry_point
        )
    })?;

    // Phase 3: model pre-warm. Failure here is a warning, not an install
    // failure — weights will download on first real use instead.
    emit_phase(app, engine_id, "models", "running");
    log(
        app,
        "Downloading model weights (first-run warm-up; this is the multi-GB part)...",
    );
    let warmup = tempfile::Builder::new()
        .prefix("pipeline_engine_check_")
        .suffix(".pdf")
        .tempfile()
        .and_then(|mut f| {
            use std::io::Write as _;
            f.write_all(&minimal_pdf_bytes())?;
            f.flush()?;
            Ok(f)
        });
    match warmup {
        Ok(f) => {
            let pdf_path = f.path().to_string_lossy().to_string();
            let out_dir = std::env::temp_dir().to_string_lossy().to_string();
            let args = vec![
                pdf_path.as_str(),
                "--output_format",
                "markdown",
                "--output_dir",
                out_dir.as_str(),
            ];
            let mut env = uv_env()?;
            env.extend(tool_env());
            match run_install_step(app, &entry, &args, &env, "model warm-up").await {
                Ok(()) => emit_phase(app, engine_id, "models", "done"),
                Err(e) if e.contains("cancelled") => {
                    emit_phase(app, engine_id, "models", "failed");
                    return Err(e);
                }
                Err(e) => {
                    log(
                        app,
                        format!(
                        "WARNING: model warm-up failed ({e}). Weights will download on first use."
                    ),
                    );
                    emit_phase(app, engine_id, "models", "done");
                }
            }
        }
        Err(e) => {
            log(
                app,
                format!(
                "WARNING: could not create warm-up file ({e}). Weights will download on first use."
            ),
            );
            emit_phase(app, engine_id, "models", "done");
        }
    }

    log(
        app,
        format!("{} installed at {}", spec.label, entry.display()),
    );
    Ok(())
}

/// Uninstall an engine and, when it was the last one, the shared model
/// cache. The uv runtime and wheel cache stay (small, and they make a
/// reinstall fast); deleting `~/.pipeline/` manually removes everything.
pub async fn uninstall_engine(app: &AppHandle, engine_id: &str) -> Result<(), String> {
    let spec = engine(engine_id)?;
    let _guard = acquire_install_guard()?;
    INSTALL_CANCEL.store(false, Ordering::Release);

    if matches!(spec.kind, EngineKind::PaddleOcrVl) {
        let root = paddle_root()?;
        if root.is_dir() {
            tokio::task::spawn_blocking(move || std::fs::remove_dir_all(root))
                .await
                .map_err(|error| format!("PaddleOCR-VL cleanup task failed: {error}"))?
                .map_err(|error| format!("Failed to remove PaddleOCR-VL: {error}"))?;
        }
        log(app, format!("{} uninstalled", spec.label));
        return Ok(());
    }
    let EngineKind::Python {
        pip_spec,
        entry_point: _,
    } = spec.kind
    else {
        unreachable!("native engines return above");
    };

    let uv = uv_binary_path()?;
    if uv.is_file() {
        let env = uv_env()?;
        run_install_step(
            app,
            &uv,
            &[
                "tool",
                "uninstall",
                pip_spec.split('=').next().unwrap_or(pip_spec),
            ],
            &env,
            "uninstall",
        )
        .await?;
    }

    let others_installed = ENGINES
        .iter()
        .filter(|e| e.id != engine_id)
        .any(|e| match e.kind {
            EngineKind::Python { entry_point, .. } => find_managed(entry_point).is_some(),
            EngineKind::PaddleOcrVl => paddle_engine_paths().is_ok(),
        });
    if !others_installed {
        if let Ok(home) = pipeline_home() {
            let hf = home.join("hf");
            if hf.is_dir() {
                let removed = tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&hf))
                    .await
                    .map_err(|e| format!("Cleanup task failed: {e}"))?;
                match removed {
                    Ok(()) => log(app, "Removed model weight cache (~/.pipeline/hf)"),
                    Err(e) => log(app, format!("WARNING: could not remove model cache: {e}")),
                }
            }
        }
    }

    log(app, format!("{} uninstalled", spec.label));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uv_target_covers_release_platforms() {
        for (os, arch) in [
            ("macos", "aarch64"),
            ("macos", "x86_64"),
            ("windows", "x86_64"),
            ("linux", "x86_64"),
            ("linux", "aarch64"),
        ] {
            let target = uv_target(os, arch).expect("supported platform");
            // Every mapped target must have a pinned checksum.
            assert!(
                UV_ARTIFACTS.iter().any(|a| a.target == target),
                "missing checksum for {target}"
            );
        }
        assert!(uv_target("linux", "riscv64").is_none());
    }

    #[test]
    fn artifact_names_match_release_layout() {
        assert_eq!(
            uv_artifact_name("x86_64-pc-windows-msvc"),
            "uv-x86_64-pc-windows-msvc.zip"
        );
        assert_eq!(
            uv_artifact_name("aarch64-apple-darwin"),
            "uv-aarch64-apple-darwin.tar.gz"
        );
        assert!(uv_download_url("aarch64-apple-darwin").contains(UV_VERSION));
    }

    #[test]
    fn checksums_are_well_formed() {
        for a in UV_ARTIFACTS {
            assert_eq!(a.sha256.len(), 64, "{} checksum length", a.target);
            assert!(a.sha256.chars().all(|c| c.is_ascii_hexdigit()));
        }
        for checksum in [PADDLE_MODEL_SHA256, PADDLE_MMPROJ_SHA256] {
            assert_eq!(checksum.len(), 64);
            assert!(checksum.chars().all(|c| c.is_ascii_hexdigit()));
        }
        for artifact in LLAMA_ARTIFACTS {
            assert_eq!(artifact.sha256.len(), 64, "{}", artifact.filename);
            assert!(artifact.sha256.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn parse_tool_list_finds_version() {
        let output = "ruff v0.6.0\n- ruff\nmarker-pdf v1.10.2\n- marker\n- marker_single\n";
        assert_eq!(
            parse_uv_tool_list(output, "marker-pdf"),
            Some("1.10.2".to_string())
        );
        assert_eq!(parse_uv_tool_list(output, "mineru"), None);
        assert_eq!(parse_uv_tool_list("", "marker-pdf"), None);
    }

    #[test]
    fn registry_has_unique_ids_and_entry_points() {
        let mut ids = std::collections::HashSet::new();
        for e in ENGINES {
            assert!(ids.insert(e.id), "duplicate engine id {}", e.id);
            if let EngineKind::Python {
                pip_spec,
                entry_point,
            } = e.kind
            {
                assert!(!pip_spec.is_empty() && !entry_point.is_empty());
            }
        }
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
    fn minimal_pdf_parses_structurally() {
        let bytes = minimal_pdf_bytes();
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.ends_with("%%EOF\n"));
        // The startxref offset must point at the literal "xref" keyword.
        let startxref: usize = text
            .split("startxref\n")
            .nth(1)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(&text[startxref..startxref + 4], "xref");
        // Every object offset in the xref table must point at "N 0 obj".
        for (i, line) in text
            .split("xref\n")
            .nth(1)
            .unwrap()
            .lines()
            .skip(2)
            .take(5)
            .enumerate()
        {
            let off: usize = line.split_whitespace().next().unwrap().parse().unwrap();
            assert!(
                text[off..].starts_with(&format!("{} 0 obj", i + 1)),
                "object {} offset wrong",
                i + 1
            );
        }
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

    #[test]
    #[cfg(unix)]
    fn timed_out_install_child_is_killed_and_reaped_before_return() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let mut child = tokio::process::Command::new("sleep")
                    .arg("5")
                    .spawn()
                    .unwrap();
                let result = wait_install_child(&mut child, std::time::Duration::from_millis(20))
                    .await
                    .unwrap();
                assert!(result.is_none());
                assert!(child.try_wait().unwrap().is_some());
            });
    }
}
