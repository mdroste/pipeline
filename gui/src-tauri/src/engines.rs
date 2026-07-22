//! Managed local-engine provisioning via uv.
//!
//! Downloads the uv binary (pinned version, SHA-256 verified) into
//! `~/.pipeline/bin/` on demand, then provisions Python-based extraction
//! engines into an app-owned toolchain under `~/.pipeline/`:
//!
//! ```text
//! ~/.pipeline/
//! ├── bin/       uv + tool entry-point shims   (UV_TOOL_BIN_DIR)
//! ├── tools/     per-engine venvs              (UV_TOOL_DIR)
//! ├── python/    managed CPython               (UV_PYTHON_INSTALL_DIR)
//! ├── uv-cache/  wheel cache                   (UV_CACHE_DIR)
//! └── hf/        model weights                 (HF_HOME)
//! ```
//!
//! Nothing touches system Python, and deleting `~/.pipeline/` removes the
//! entire stack. Installs stream progress to the frontend via
//! `engines:phase` / `engines:log` events.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

// ── Pinned uv release ───────────────────────────────────────────────
//
// Checksums are Astral's published .sha256 files for this release. Update
// the version and all five hashes together.

const UV_VERSION: &str = "0.11.26";

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

// ── Engine registry ─────────────────────────────────────────────────

pub struct EngineSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    /// The pip requirement passed to `uv tool install`.
    pub pip_spec: &'static str,
    /// The executable the install creates in the managed bin dir. Also how
    /// the rest of the app detects the engine.
    pub entry_point: &'static str,
    /// Rough total download (packages + model weights), for the UI and the
    /// pre-install disk check.
    pub est_download_mb: u64,
    pub est_disk_mb: u64,
}

pub const ENGINES: &[EngineSpec] = &[EngineSpec {
    id: "marker",
    label: "marker-pdf",
    description: "Local PDF-to-Markdown conversion with equations preserved. \
                  Runs on CPU or Apple Silicon; no API calls. GPL-3.0 code; \
                  model weights under Datalab's revenue-capped OpenRAIL-M license.",
    // Pin the last release exercised by Pipeline's extraction integration.
    // A live index spec can replace the engine underneath an unchanged app.
    pip_spec: "marker-pdf==1.10.2",
    entry_point: "marker_single",
    est_download_mb: 3500,
    est_disk_mb: 6000,
}];

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
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                total += meta.len();
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
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x08000000);
            }
            let out = cmd.output().ok()?;
            Some(String::from_utf8_lossy(&out.stdout).to_string())
        });

    let stack_mb = pipeline_home()
        .map(|home| {
            ["bin", "tools", "python", "uv-cache", "hf"]
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
            let entry = find_managed(spec.entry_point);
            let version = tool_list
                .as_deref()
                .and_then(|out| parse_uv_tool_list(out, spec.pip_spec))
                .unwrap_or_default();
            // A copy elsewhere on PATH (pip/anaconda). Exclude the managed
            // dir in case the user added ~/.pipeline/bin to PATH themselves.
            let system_path = crate::deps::find_on_path(spec.entry_point)
                .filter(|p| managed_dir.as_ref().map_or(true, |d| !p.starts_with(d)))
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
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

/// Download uv (pinned version, checksum-verified) into `~/.pipeline/bin/`
/// unless a matching version is already there.
async fn ensure_uv(app: &AppHandle) -> Result<PathBuf, String> {
    let uv_path = uv_binary_path()?;
    if uv_path.is_file() {
        // Accept only the pinned version so the behavior we tested is the
        // behavior users get; anything else is replaced.
        let current = std::process::Command::new(&uv_path)
            .arg("--version")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        if current.contains(UV_VERSION) {
            return Ok(uv_path);
        }
        log(app, format!("Replacing managed uv ({} wanted)", UV_VERSION));
    }

    let artifact = uv_artifact()?;
    let url = uv_download_url(artifact.target);
    log(app, format!("Downloading uv {UV_VERSION} ({url})"));

    let client = &*crate::pipeline::api_common::HTTP_CLIENT;
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(300))
        .send()
        .await
        .map_err(|e| format!("Failed to download uv: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("uv download failed: HTTP {}", resp.status()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read uv download: {e}"))?;

    let digest = format!("{:x}", Sha256::digest(&bytes));
    if digest != artifact.sha256 {
        return Err(format!(
            "uv download checksum mismatch (expected {}, got {digest}). \
             Refusing to install — retry, or check for a proxy interfering with downloads.",
            artifact.sha256
        ));
    }
    log(
        app,
        format!("Verified uv archive ({} MB)", bytes.len() / 1_000_000),
    );

    let bin_dir = uv_path
        .parent()
        .ok_or("uv path has no parent directory")?
        .to_path_buf();
    std::fs::create_dir_all(&bin_dir).map_err(|e| format!("Failed to create bin dir: {e}"))?;

    let dest = uv_path.clone();
    tokio::task::spawn_blocking(move || unpack_uv(&bytes, &dest))
        .await
        .map_err(|e| format!("Unpack task failed: {e}"))??;

    log(app, format!("Installed uv to {}", uv_path.display()));
    Ok(uv_path)
}

/// Extract the `uv` binary from the release archive into `dest`.
#[cfg(not(windows))]
fn unpack_uv(archive_bytes: &[u8], dest: &Path) -> Result<(), String> {
    use std::io::Read as _;
    let gz = flate2::read::GzDecoder::new(std::io::Cursor::new(archive_bytes));
    let mut archive = tar::Archive::new(gz);
    for entry in archive
        .entries()
        .map_err(|e| format!("Invalid uv archive: {e}"))?
    {
        let mut entry = entry.map_err(|e| format!("Invalid uv archive entry: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("Invalid path in uv archive: {e}"))?
            .into_owned();
        if path.file_name().and_then(|n| n.to_str()) == Some("uv") {
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|e| format!("Failed to read uv from archive: {e}"))?;
            std::fs::write(dest, &buf).map_err(|e| format!("Failed to write uv: {e}"))?;
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("Failed to set uv permissions: {e}"))?;
            return Ok(());
        }
    }
    Err("uv binary not found in the downloaded archive".to_string())
}

#[cfg(windows)]
fn unpack_uv(archive_bytes: &[u8], dest: &Path) -> Result<(), String> {
    let cursor = std::io::Cursor::new(archive_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("Invalid uv archive: {e}"))?;
    let names: Vec<String> = archive.file_names().map(|n| n.to_string()).collect();
    let entry_name = names
        .iter()
        .find(|n| n.ends_with("uv.exe"))
        .ok_or("uv.exe not found in the downloaded archive")?
        .clone();
    let mut entry = archive
        .by_name(&entry_name)
        .map_err(|e| format!("Failed to open uv.exe in archive: {e}"))?;
    let mut out =
        std::fs::File::create(dest).map_err(|e| format!("Failed to create uv.exe: {e}"))?;
    std::io::copy(&mut entry, &mut out).map_err(|e| format!("Failed to write uv.exe: {e}"))?;
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
        "tool",
        "install",
        spec.pip_spec,
        "--python",
        "3.12",
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

    let entry = find_managed(spec.entry_point).ok_or_else(|| {
        format!(
            "Install finished but {} did not appear in the managed bin directory",
            spec.entry_point
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

    let uv = uv_binary_path()?;
    if uv.is_file() {
        let env = uv_env()?;
        run_install_step(
            app,
            &uv,
            &[
                "tool",
                "uninstall",
                spec.pip_spec.split('=').next().unwrap_or(spec.pip_spec),
            ],
            &env,
            "uninstall",
        )
        .await?;
    }

    let others_installed = ENGINES
        .iter()
        .filter(|e| e.id != engine_id)
        .any(|e| find_managed(e.entry_point).is_some());
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
            assert!(!e.pip_spec.is_empty() && !e.entry_point.is_empty());
        }
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
