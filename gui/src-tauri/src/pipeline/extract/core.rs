use super::*;

/// Maximum total output size for extracted LaTeX (10 MB).
pub(super) const MAX_LATEX_SIZE: usize = 10_000_000;
pub(super) const MAX_SCOPED_SOURCE_FILES: usize = 512;
pub(super) const MAX_SCOPED_SOURCE_FILE_BYTES: u64 = 32 * 1024 * 1024;
pub(super) const MAX_SCOPED_SOURCE_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
/// Folder inventories are context, not an archival crawler. Bound every
/// dimension independently so a directory-only tree cannot evade the file
/// cap and a small number of huge files cannot monopolize a run indefinitely.
pub(super) const MAX_INVENTORY_ENTRIES: usize = 100_000;
pub(super) const MAX_INVENTORY_DIRS: usize = 10_000;
/// The inventory fingerprint is a run id, not an integrity proof: hash at
/// most this much of each file's content (name, size, and mtime always
/// contribute), so replication folders with multi-gigabyte datasets neither
/// fail the run nor get fully re-read on every launch.
pub(super) const MAX_INVENTORY_HASH_BYTES_PER_FILE: u64 = 1024 * 1024;
/// Page images are useful artifacts but can consume substantial disk space.
/// Keep this aligned with the documented run-artifact contract.
pub const MAX_RENDERED_PDF_PAGES: u32 = 300;
pub(super) const PADDLE_REGION_WARNING_PREFIX: &str =
    "PaddleOCR-VL warning: this visual region remained degenerate";
pub(super) const PADDLE_STRUCTURE_SCHEMA: u32 = 2;
pub(super) const PADDLE_STRUCTURE_FILE: &str = "pipeline-paddle-structure.json";
pub(super) const PADDLE_FULL_STRUCTURE_FILE: &str = "pipeline-paddle-full-structure.json";
pub(super) const PADDLE_FULL_ACTIVE_FILE: &str = "active.json";

pub(super) static ANSI_ESCAPE_SEQUENCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\x1b\[[0-?]*[ -/]*[@-~]").expect("ANSI escape regex must compile")
});
pub(super) static PADDLE_NUMBERED_CAPTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(fig(?:ure)?\.?|table)\s+([a-z]?(?:[.-]?\d+)+(?:[a-z])?)\b")
        .expect("Paddle caption regex must compile")
});

pub(super) fn extraction_log(app: &crate::emit::EventBus, line: impl Into<String>) {
    crate::pipeline::logging::emit(app, line.into());
}

pub(super) fn clean_paddle_diagnostic(line: &str) -> Option<String> {
    let clean = ANSI_ESCAPE_SEQUENCE.replace_all(line, "");
    let clean = clean.trim();
    if clean.is_empty() {
        return None;
    }
    let lower = clean.to_ascii_lowercase();
    // These are expected implementation warnings from the managed dependency
    // stack. They do not describe degraded extraction and their following
    // Python source line otherwise doubles the console noise.
    if lower.contains("userwarning: no ccache found")
        || lower.contains("'llama-cpp-server' does not support `min_pixels`")
        || lower.contains("'llama-cpp-server' does not support `max_pixels`")
        || matches!(clean, "warnings.warn(warning_message)" | "warnings.warn(")
    {
        return None;
    }
    Some(clean.to_string())
}

pub(super) fn paddle_diagnostic_lines(stderr: &str, limit: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    stderr
        .lines()
        .filter_map(clean_paddle_diagnostic)
        .filter(|line| seen.insert(line.clone()))
        .take(limit)
        .collect()
}

pub(super) fn open_regular_file(path: &Path) -> Result<fs::File, String> {
    crate::safety::open_regular_file(path)
}

pub(super) fn read_utf8_capped(path: &Path, limit: usize) -> Result<String, String> {
    use std::io::Read as _;
    let file = open_regular_file(path)?;
    let mut bytes = Vec::with_capacity(64 * 1024);
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    if bytes.len() > limit {
        return Err(format!(
            "{} exceeds the {} MB safety limit",
            path.display(),
            limit / 1_000_000
        ));
    }
    String::from_utf8(bytes).map_err(|e| format!("{} is not valid UTF-8: {e}", path.display()))
}

#[derive(Debug)]
pub(super) struct BoundedOutput {
    pub(super) status: std::process::ExitStatus,
    pub(super) stdout: Vec<u8>,
    pub(super) stderr: Vec<u8>,
    pub(super) stdout_truncated: bool,
    pub(super) stderr_truncated: bool,
}

pub(super) fn drain_capped<R: std::io::Read>(mut reader: R, limit: usize) -> (Vec<u8>, bool) {
    let mut kept = Vec::new();
    let mut truncated = false;
    let mut chunk = [0u8; 16 * 1024];
    loop {
        let count = match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(count) => count,
        };
        let remaining = limit.saturating_sub(kept.len());
        let take = count.min(remaining);
        kept.extend_from_slice(&chunk[..take]);
        truncated |= take < count;
    }
    (kept, truncated)
}

/// Run a blocking extraction subprocess with bounded capture, cancellation,
/// timeout, process-tree termination, and PID cleanup.
pub(super) fn run_bounded_output(
    mut command: StdCommand,
    label: &str,
    timeout: std::time::Duration,
    output_limit: usize,
    watched_output_dir: Option<&Path>,
) -> Result<BoundedOutput, String> {
    use std::process::Stdio;

    crate::pipeline::claude::configure_silent_command(&mut command);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to start {label}: {e}"))?;
    let pid = child.id();
    if pid > 0 {
        crate::commands::register_child_pid(pid);
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (stdout_tx, stdout_rx) = std::sync::mpsc::sync_channel(1);
    let (stderr_tx, stderr_rx) = std::sync::mpsc::sync_channel(1);
    let stdout_thread = std::thread::spawn(move || {
        let result = stdout
            .map(|pipe| drain_capped(pipe, output_limit))
            .unwrap_or_default();
        let _ = stdout_tx.send(result);
    });
    let stderr_thread = std::thread::spawn(move || {
        let result = stderr
            .map(|pipe| drain_capped(pipe, output_limit))
            .unwrap_or_default();
        let _ = stderr_tx.send(result);
    });

    let started = std::time::Instant::now();
    let mut last_output_scan = std::time::Instant::now();
    let result = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if crate::commands::is_cancelled() => {
                if pid > 0 {
                    crate::commands::kill_process(pid);
                }
                let _ = child.kill();
                let _ = child.wait();
                break Err("Pipeline cancelled".to_string());
            }
            Ok(None) if started.elapsed() >= timeout => {
                if pid > 0 {
                    crate::commands::kill_process(pid);
                }
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!("{label} timed out after {}s", timeout.as_secs()));
            }
            Ok(None) => {
                if let Some(dir) = watched_output_dir {
                    if last_output_scan.elapsed() >= std::time::Duration::from_millis(250) {
                        last_output_scan = std::time::Instant::now();
                        let mut walk = crate::safety::WalkBudget::new_cancellable(
                            "Extraction output directory",
                        );
                        match directory_bytes_bounded(dir, &mut walk) {
                            Ok(bytes) if bytes <= 250_000_000 => {}
                            Ok(bytes) => {
                                if pid > 0 {
                                    crate::commands::kill_process(pid);
                                }
                                let _ = child.kill();
                                let _ = child.wait();
                                break Err(format!(
                                    "{label} generated {bytes} bytes; the safety limit is 250000000"
                                ));
                            }
                            Err(error) => {
                                if pid > 0 {
                                    crate::commands::kill_process(pid);
                                }
                                let _ = child.kill();
                                let _ = child.wait();
                                break Err(error);
                            }
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                if pid > 0 {
                    crate::commands::kill_process(pid);
                }
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!("Failed waiting for {label}: {e}"));
            }
        }
    };

    // A successful leader can leave descendants holding its pipes. Bound the
    // drain just like provider CLIs, then terminate the still-owned group/job.
    let mut stdout_result = stdout_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .ok();
    let mut stderr_result = stderr_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .ok();
    if stdout_result.is_none() || stderr_result.is_none() {
        if pid > 0 {
            crate::commands::kill_process(pid);
        }
        let _ = child.kill();
        let _ = child.wait();
        if stdout_result.is_none() {
            stdout_result = stdout_rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .ok();
        }
        if stderr_result.is_none() {
            stderr_result = stderr_rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .ok();
        }
    }
    if pid > 0 {
        crate::commands::unregister_child_pid(pid);
    }
    if stdout_result.is_some() {
        let _ = stdout_thread.join();
    }
    if stderr_result.is_some() {
        let _ = stderr_thread.join();
    }
    let (stdout, stdout_truncated) = stdout_result
        .ok_or_else(|| format!("{label} stdout pipe did not close after termination"))?;
    let (stderr, stderr_truncated) = stderr_result
        .ok_or_else(|| format!("{label} stderr pipe did not close after termination"))?;
    let status = result?;
    Ok(BoundedOutput {
        status,
        stdout,
        stderr,
        stdout_truncated,
        stderr_truncated,
    })
}

pub(super) fn directory_bytes_bounded(
    root: &Path,
    walk: &mut crate::safety::WalkBudget,
) -> Result<u64, String> {
    let mut bytes = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir)
            .map_err(|error| format!("Failed to inspect {}: {error}", dir.display()))?;
        for entry in entries {
            walk.entry()?;
            let entry = entry.map_err(|error| format!("Failed to inspect output: {error}"))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("Failed to inspect output: {error}"))?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                walk.directory()?;
                stack.push(entry.path());
            } else if file_type.is_file() {
                bytes = bytes.saturating_add(
                    entry
                        .metadata()
                        .map_err(|error| format!("Failed to inspect output: {error}"))?
                        .len(),
                );
            }
        }
    }
    Ok(bytes)
}

/// Case-insensitive extension check (handles .PDF, .Tex, etc.).
pub(super) fn ext_eq(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

/// Compute SHA-256 hash of file contents, first 16 hex chars.
pub(super) fn compute_hash(path: &Path) -> Result<String, String> {
    let file = open_regular_file(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let count = std::io::Read::read(&mut reader, &mut chunk)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        if count == 0 {
            break;
        }
        if crate::commands::is_cancelled() {
            return Err("Pipeline cancelled".to_string());
        }
        hasher.update(&chunk[..count]);
    }
    Ok(format!("{:x}", hasher.finalize())[..16].to_string())
}

pub(super) fn content_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))[..16].to_string()
}

/// PDFs get their own staging budget and message — the LaTeX source-closure
/// per-file cap does not apply to them (scanned documents routinely exceed
/// 32 MB), and the error must name the actual constraint.
pub(super) const MAX_STAGED_PDF_BYTES: u64 = 256 * 1024 * 1024;

/// Put the selected PDF in a private, single-purpose directory before any
/// helper child (pdftotext, pdftoppm, the Paddle sidecar, provider CLIs)
/// touches it. Granting the original parent would expose every unrelated
/// sibling in locations such as Downloads, and on macOS the in-process copy
/// is covered by the user's file-picker grant while a child reading the
/// original path needs its own TCC folder permission and can be silently
/// denied.
pub(super) fn stage_pdf_input(source: &Path, root: &Path) -> Result<PathBuf, String> {
    let mut file = open_regular_file(source)?;
    let len = file
        .metadata()
        .map_err(|error| format!("Failed to inspect {}: {error}", source.display()))?
        .len();
    if len > MAX_STAGED_PDF_BYTES {
        // Every extraction method (pdftotext included) stages through this
        // cap, so the message must not recommend one as a way around it.
        return Err(format!(
            "PDF is {} MB; PDFs above {} MB are not supported. Reduce the file size (e.g. split the PDF or downsample scanned pages) and retry",
            len / 1024 / 1024,
            MAX_STAGED_PDF_BYTES / 1024 / 1024
        ));
    }
    let name = source
        .file_name()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| std::ffi::OsStr::new("document.pdf"));
    let destination = root.join(name);
    let mut out = fs::File::create(&destination)
        .map_err(|error| format!("Failed to stage {}: {error}", destination.display()))?;
    let copied = std::io::copy(
        &mut std::io::Read::take(&mut file, MAX_STAGED_PDF_BYTES + 1),
        &mut out,
    )
    .map_err(|error| format!("Failed to stage {}: {error}", source.display()))?;
    if copied > MAX_STAGED_PDF_BYTES {
        return Err(format!(
            "PDF grew past the {} MB staging limit while being copied",
            MAX_STAGED_PDF_BYTES / 1024 / 1024
        ));
    }
    Ok(destination)
}

/// Find the main .tex file in a directory by looking for \documentclass.
pub(crate) fn find_main_tex(dir: &Path) -> Option<PathBuf> {
    let tex_files: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| ext_eq(p, "tex"))
        .collect();

    let mut candidates: Vec<PathBuf> = Vec::new();
    for path in &tex_files {
        if let Ok(content) = read_utf8_capped(path, MAX_LATEX_SIZE) {
            if latex_without_comments(&content).contains("\\documentclass") {
                candidates.push(path.clone());
            }
        }
    }

    if candidates.is_empty() {
        return None;
    }

    let preferred = ["main.tex", "paper.tex", "manuscript.tex"];
    for name in &preferred {
        if let Some(p) = candidates.iter().find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n == *name)
                .unwrap_or(false)
        }) {
            return Some(p.clone());
        }
    }

    candidates.sort_by_key(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.len())
            .unwrap_or(usize::MAX)
    });
    candidates.into_iter().next()
}

/// The only source paths exposed to orientation and review calls. A selected
/// folder is already an explicit user-granted scope. A selected file is copied
/// into a private directory, and LaTeX projects receive a bounded closure of
/// local dependencies rather than the whole containing directory.
#[derive(Debug, Default)]
pub(crate) struct ScopedSourceContext {
    pub(crate) source_path: Option<PathBuf>,
    pub(crate) read_root: Option<PathBuf>,
}
