use crate::env;
use crate::models::ExtractionResult;
#[cfg(test)]
use base64::Engine as _;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::sync::LazyLock;

/// Maximum total output size for extracted LaTeX (10 MB).
const MAX_LATEX_SIZE: usize = 10_000_000;
const MAX_SCOPED_SOURCE_FILES: usize = 512;
const MAX_SCOPED_SOURCE_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_SCOPED_SOURCE_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
/// Folder inventories are context, not an archival crawler. Bound every
/// dimension independently so a directory-only tree cannot evade the file
/// cap and a small number of huge files cannot monopolize a run indefinitely.
const MAX_INVENTORY_ENTRIES: usize = 100_000;
const MAX_INVENTORY_DIRS: usize = 10_000;
const MAX_INVENTORY_HASH_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Page images are useful artifacts but can consume substantial disk space.
/// Keep this aligned with the documented run-artifact contract.
pub const MAX_RENDERED_PDF_PAGES: u32 = 300;
const PADDLE_REGION_WARNING_PREFIX: &str =
    "PaddleOCR-VL warning: this visual region remained degenerate";
#[cfg(test)]
const MAX_MARKER_IMAGE_BYTES: usize = 25 * 1024 * 1024;
#[cfg(test)]
const MAX_MARKER_IMAGE_TOTAL_BYTES: usize = 150 * 1024 * 1024;
#[cfg(test)]
const MAX_MARKER_IMAGES: usize = 500;
const MARKER_STRUCTURE_SCHEMA: u32 = 1;
const MARKER_STRUCTURE_FILE: &str = "pipeline-marker-structure.json";
#[cfg(test)]
const MARKER_DOCUMENT_FILE: &str = "pipeline-document.md";
const PADDLE_STRUCTURE_SCHEMA: u32 = 2;
const PADDLE_STRUCTURE_FILE: &str = "pipeline-paddle-structure.json";
const PADDLE_FULL_STRUCTURE_FILE: &str = "pipeline-paddle-full-structure.json";
const PADDLE_FULL_ACTIVE_FILE: &str = "active.json";

static ANSI_ESCAPE_SEQUENCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\x1b\[[0-?]*[ -/]*[@-~]").expect("ANSI escape regex must compile")
});
static PADDLE_NUMBERED_CAPTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(fig(?:ure)?\.?|table)\s+([a-z]?(?:[.-]?\d+)+(?:[a-z])?)\b")
        .expect("Paddle caption regex must compile")
});

pub(crate) const MARKER_DISABLED_MESSAGE: &str =
    "Marker PDF extraction is unavailable in Pipeline 1.0.1 because its compatible \
     Python dependency closure contains known security vulnerabilities. Choose \
     PaddleOCR-VL, LLM extraction, or pdftotext in the workflow or Settings.";

fn reject_retired_pdf_extractor(method: &str) -> Result<(), String> {
    if method == "marker" {
        Err(MARKER_DISABLED_MESSAGE.to_string())
    } else {
        Ok(())
    }
}

#[cfg(test)]
fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

fn extraction_log(app: &crate::emit::EventBus, line: impl Into<String>) {
    crate::pipeline::logging::emit(app, line.into());
}

fn clean_paddle_diagnostic(line: &str) -> Option<String> {
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

fn paddle_diagnostic_lines(stderr: &str, limit: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    stderr
        .lines()
        .filter_map(clean_paddle_diagnostic)
        .filter(|line| seen.insert(line.clone()))
        .take(limit)
        .collect()
}

fn open_regular_file(path: &Path) -> Result<fs::File, String> {
    crate::safety::open_regular_file(path)
}

fn read_utf8_capped(path: &Path, limit: usize) -> Result<String, String> {
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
struct BoundedOutput {
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdout_truncated: bool,
    stderr_truncated: bool,
}

fn drain_capped<R: std::io::Read>(mut reader: R, limit: usize) -> (Vec<u8>, bool) {
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
fn run_bounded_output(
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

fn directory_bytes_bounded(
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
fn ext_eq(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

/// Compute SHA-256 hash of file contents, first 16 hex chars.
fn compute_hash(path: &Path) -> Result<String, String> {
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

fn content_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))[..16].to_string()
}

/// Put one selected document in a private, single-purpose directory before a
/// provider CLI receives filesystem access. Granting the original parent would
/// also expose every unrelated sibling in locations such as Downloads.
fn stage_provider_input(source: &Path, root: &Path) -> Result<PathBuf, String> {
    let name = source
        .file_name()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| std::ffi::OsStr::new("document.pdf"));
    let destination = root.join(name);
    let mut total_bytes = 0;
    copy_scoped_source_file(source, &destination, &mut total_bytes)?;
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
            if content.contains("\\documentclass") {
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

#[derive(Debug)]
struct LatexReference {
    target: String,
    default_extensions: &'static [&'static str],
    recursive: bool,
}

fn latex_without_comments(content: &str) -> String {
    let mut visible = String::with_capacity(content.len());
    for line in content.split_inclusive('\n') {
        let mut slash_count = 0usize;
        let mut comment_at = None;
        for (index, character) in line.char_indices() {
            if character == '%' && slash_count & 1 == 0 {
                comment_at = Some(index);
                break;
            }
            if character == '\\' {
                slash_count += 1;
            } else {
                slash_count = 0;
            }
        }
        if let Some(index) = comment_at {
            visible.push_str(&line[..index]);
            if line.ends_with('\n') {
                visible.push('\n');
            }
        } else {
            visible.push_str(line);
        }
    }
    visible
}

fn latex_references(content: &str) -> Vec<LatexReference> {
    fn collect(
        output: &mut Vec<LatexReference>,
        content: &str,
        pattern: &str,
        capture: usize,
        extensions: &'static [&'static str],
        recursive: bool,
        comma_separated: bool,
    ) {
        let regex = Regex::new(pattern).expect("LaTeX dependency regex is invalid");
        for captures in regex.captures_iter(content) {
            let Some(value) = captures.get(capture) else {
                continue;
            };
            let values: Vec<&str> = if comma_separated {
                value.as_str().split(',').collect()
            } else {
                vec![value.as_str()]
            };
            output.extend(values.into_iter().filter_map(|value| {
                let target = value.trim().trim_matches('"');
                if target.is_empty()
                    || target.contains('#')
                    || target.contains('\\')
                    || target.contains("://")
                {
                    return None;
                }
                Some(LatexReference {
                    target: target.to_string(),
                    default_extensions: extensions,
                    recursive,
                })
            }));
        }
    }

    let visible = latex_without_comments(content);
    let mut references = Vec::new();
    collect(
        &mut references,
        &visible,
        r"\\(?:input|include|subfile)\s*\{([^}]+)\}",
        1,
        &["tex"],
        true,
        false,
    );
    collect(
        &mut references,
        &visible,
        r"\\includegraphics\*?(?:\s*\[[^\]]*\])?\s*\{([^}]+)\}",
        1,
        &["pdf", "png", "jpg", "jpeg", "webp", "eps", "svg"],
        false,
        false,
    );
    let graphics_paths = Regex::new(r"\\graphicspath\s*\{((?:\s*\{[^{}]*\})+)\s*\}")
        .expect("LaTeX graphicspath regex is invalid")
        .captures_iter(&visible)
        .flat_map(|capture| {
            Regex::new(r"\{([^{}]+)\}")
                .expect("LaTeX graphicspath entry regex is invalid")
                .captures_iter(capture.get(1).map(|value| value.as_str()).unwrap_or(""))
                .filter_map(|entry| entry.get(1).map(|value| value.as_str().trim().to_string()))
                .collect::<Vec<_>>()
        })
        .filter(|path| !path.is_empty() && !path.contains('#') && !path.contains('\\'))
        .collect::<Vec<_>>();
    let graphics = Regex::new(r"\\includegraphics\*?(?:\s*\[[^\]]*\])?\s*\{([^}]+)\}")
        .expect("LaTeX graphics regex is invalid");
    for capture in graphics.captures_iter(&visible) {
        let Some(target) = capture.get(1).map(|value| value.as_str().trim()) else {
            continue;
        };
        for directory in &graphics_paths {
            references.push(LatexReference {
                target: format!(
                    "{}/{}",
                    directory.trim_end_matches('/'),
                    target.trim_start_matches('/')
                ),
                default_extensions: &["pdf", "png", "jpg", "jpeg", "webp", "eps", "svg"],
                recursive: false,
            });
        }
    }
    let import =
        Regex::new(r"\\(?:import|subimport|inputfrom|includefrom)\s*\{([^}]+)\}\s*\{([^}]+)\}")
            .expect("LaTeX import regex is invalid");
    for capture in import.captures_iter(&visible) {
        let Some(directory) = capture.get(1).map(|value| value.as_str().trim()) else {
            continue;
        };
        let Some(file) = capture.get(2).map(|value| value.as_str().trim()) else {
            continue;
        };
        if directory.contains('#')
            || directory.contains('\\')
            || file.contains('#')
            || file.contains('\\')
        {
            continue;
        }
        references.push(LatexReference {
            target: format!(
                "{}/{}",
                directory.trim_end_matches('/'),
                file.trim_start_matches('/')
            ),
            default_extensions: &["tex"],
            recursive: true,
        });
    }
    collect(
        &mut references,
        &visible,
        r"\\bibliography\s*\{([^}]+)\}",
        1,
        &["bib"],
        false,
        true,
    );
    collect(
        &mut references,
        &visible,
        r"\\addbibresource(?:\s*\[[^\]]*\])?\s*\{([^}]+)\}",
        1,
        &["bib"],
        false,
        false,
    );
    collect(
        &mut references,
        &visible,
        r"\\usepackage(?:\s*\[[^\]]*\])?\s*\{([^}]+)\}",
        1,
        &["sty"],
        true,
        true,
    );
    collect(
        &mut references,
        &visible,
        r"\\documentclass(?:\s*\[[^\]]*\])?\s*\{([^}]+)\}",
        1,
        &["cls"],
        true,
        false,
    );
    collect(
        &mut references,
        &visible,
        r"\\(?:lstinputlisting|verbatiminput)(?:\s*\[[^\]]*\])?\s*\{([^}]+)\}",
        1,
        &[],
        false,
        false,
    );
    collect(
        &mut references,
        &visible,
        r"\\inputminted(?:\s*\[[^\]]*\])?\s*\{[^}]+\}\s*\{([^}]+)\}",
        1,
        &[],
        false,
        false,
    );
    references
}

fn resolve_latex_reference(
    reference: &LatexReference,
    current_dir: &Path,
    project_root: &Path,
) -> Option<PathBuf> {
    let raw = Path::new(&reference.target);
    if raw.is_absolute() {
        return None;
    }
    let mut relative_candidates = vec![raw.to_path_buf()];
    if raw.extension().is_none() {
        relative_candidates.extend(reference.default_extensions.iter().map(|extension| {
            let mut candidate = raw.to_path_buf();
            candidate.set_extension(extension);
            candidate
        }));
    }
    for base in [current_dir, project_root] {
        for relative in &relative_candidates {
            let candidate = base.join(relative);
            let Ok(canonical) = candidate.canonicalize() else {
                continue;
            };
            if canonical.starts_with(project_root) && canonical.is_file() {
                return Some(canonical);
            }
        }
    }
    None
}

fn copy_scoped_source_file(
    source: &Path,
    destination: &Path,
    total_bytes: &mut u64,
) -> Result<(), String> {
    let mut input = open_regular_file(source)?;
    let size = input
        .metadata()
        .map_err(|error| format!("Failed to inspect {}: {error}", source.display()))?
        .len();
    if size > MAX_SCOPED_SOURCE_FILE_BYTES {
        return Err(format!(
            "Referenced source file '{}' exceeds the {} MB per-file limit",
            source.display(),
            MAX_SCOPED_SOURCE_FILE_BYTES / (1024 * 1024)
        ));
    }
    if total_bytes.saturating_add(size) > MAX_SCOPED_SOURCE_TOTAL_BYTES {
        return Err(format!(
            "Referenced source files exceed the {} MB total limit",
            MAX_SCOPED_SOURCE_TOTAL_BYTES / (1024 * 1024)
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create private source directory '{}': {error}",
                parent.display()
            )
        })?;
    }
    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .map_err(|error| {
            format!(
                "Failed to create private source file '{}': {error}",
                destination.display()
            )
        })?;
    let mut limited = std::io::Read::take(&mut input, MAX_SCOPED_SOURCE_FILE_BYTES + 1);
    let copied = std::io::copy(&mut limited, &mut output)
        .map_err(|error| format!("Failed to stage '{}': {error}", source.display()))?;
    if copied > MAX_SCOPED_SOURCE_FILE_BYTES {
        return Err(format!(
            "Referenced source file '{}' exceeds the {} MB per-file limit",
            source.display(),
            MAX_SCOPED_SOURCE_FILE_BYTES / (1024 * 1024)
        ));
    }
    if copied != size {
        return Err(format!(
            "Source file '{}' changed while it was being staged",
            source.display()
        ));
    }
    *total_bytes = total_bytes.saturating_add(copied);
    Ok(())
}

fn stage_latex_project(
    main_file: &Path,
    project_root: &Path,
    destination_root: &Path,
) -> Result<(), String> {
    let main_file = main_file
        .canonicalize()
        .map_err(|error| format!("Failed to resolve {}: {error}", main_file.display()))?;
    let project_root = project_root
        .canonicalize()
        .map_err(|error| format!("Failed to resolve {}: {error}", project_root.display()))?;
    if !main_file.starts_with(&project_root) {
        return Err("The selected LaTeX file is outside its project root".to_string());
    }

    let mut pending = VecDeque::from([(main_file, true)]);
    let mut visited = HashSet::new();
    let mut total_bytes = 0u64;
    while let Some((source, recurse)) = pending.pop_front() {
        if !visited.insert(source.clone()) {
            continue;
        }
        if visited.len() > MAX_SCOPED_SOURCE_FILES {
            return Err(format!(
                "LaTeX source closure exceeds the {MAX_SCOPED_SOURCE_FILES}-file limit"
            ));
        }
        let relative = source
            .strip_prefix(&project_root)
            .map_err(|_| "A referenced LaTeX source escaped the project root".to_string())?;
        copy_scoped_source_file(&source, &destination_root.join(relative), &mut total_bytes)?;
        if !recurse {
            continue;
        }
        let content = match read_utf8_capped(&source, MAX_LATEX_SIZE) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let current_dir = source.parent().unwrap_or(&project_root);
        for reference in latex_references(&content) {
            if let Some(path) = resolve_latex_reference(&reference, current_dir, &project_root) {
                pending.push_back((path, reference.recursive));
            }
        }
    }
    Ok(())
}

pub(crate) fn stage_selected_source(
    selected: &Path,
    input_mode: &str,
    private_root: &Path,
) -> Result<ScopedSourceContext, String> {
    if input_mode == "none" || selected.as_os_str().is_empty() {
        return Ok(ScopedSourceContext::default());
    }
    if input_mode == "folder" {
        let selected = selected
            .canonicalize()
            .map_err(|error| format!("Failed to resolve selected folder: {error}"))?;
        if !selected.is_dir() {
            return Err(format!(
                "Selected folder '{}' is not a directory",
                selected.display()
            ));
        }
        return Ok(ScopedSourceContext {
            source_path: Some(selected.clone()),
            read_root: Some(selected),
        });
    }

    let latex_main = if selected.is_dir() {
        find_main_tex(selected)
    } else if ext_eq(selected, "tex") {
        Some(selected.to_path_buf())
    } else {
        None
    };
    let destination_root = private_root.join("source");
    fs::create_dir_all(&destination_root).map_err(|error| {
        format!(
            "Failed to create private source root '{}': {error}",
            destination_root.display()
        )
    })?;
    if let Some(main_file) = latex_main {
        let project_root = if selected.is_dir() {
            selected
        } else {
            selected.parent().unwrap_or_else(|| Path::new("."))
        };
        stage_latex_project(&main_file, project_root, &destination_root)?;
        return Ok(ScopedSourceContext {
            source_path: Some(destination_root.clone()),
            read_root: Some(destination_root),
        });
    }

    let name = selected
        .file_name()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| std::ffi::OsStr::new("source"));
    let destination = destination_root.join(name);
    let mut total_bytes = 0u64;
    copy_scoped_source_file(selected, &destination, &mut total_bytes)?;
    Ok(ScopedSourceContext {
        source_path: Some(destination),
        read_root: Some(destination_root),
    })
}

/// Extract text from a .tex file, resolving \input{} and \include{} recursively.
fn extract_latex(
    path: &Path,
    depth: usize,
    root_dir: &Path,
    warnings: &mut Vec<String>,
) -> Result<String, String> {
    if depth > 10 {
        warnings.push("LaTeX \\input{} nesting exceeds 10 levels — possible circular includes. Output may be incomplete.".to_string());
        return Ok(String::new());
    }

    let content = read_utf8_capped(path, MAX_LATEX_SIZE)?;

    let parent = path.parent().unwrap_or(Path::new("."));
    let re = Regex::new(r"\\(?:input|include)\{([^}]+)\}").expect("LaTeX include regex is invalid");

    let mut result = String::new();
    let mut last_end = 0;

    for cap in re.captures_iter(&content) {
        let full_match = cap.get(0).unwrap();
        result.push_str(&content[last_end..full_match.start()]);

        let include_name = &cap[1];
        let mut include_path = parent.join(include_name);
        if include_path.extension().is_none() {
            include_path.set_extension("tex");
        }

        // Validate the resolved path stays within the root directory to prevent
        // path traversal via malicious \input{../../../etc/passwd}
        let canonical = include_path.canonicalize().ok();
        let safe = canonical
            .as_ref()
            .is_some_and(|resolved| resolved.starts_with(root_dir));

        if safe && include_path.is_file() {
            match extract_latex(&include_path, depth + 1, root_dir, warnings) {
                Ok(included) => result.push_str(&included),
                Err(_) => result.push_str(&content[full_match.start()..full_match.end()]),
            }
        } else {
            // Diagnose why the include failed
            if !include_path.exists() {
                warnings.push(format!(
                    "\\input{{{}}} — file not found. If this is a multi-file project, select the project folder instead of a single .tex file.",
                    include_name
                ));
            } else if canonical.is_some() && !safe {
                warnings.push(format!(
                    "\\input{{{}}} — resolves outside the project directory (blocked). Select the project folder instead of a single .tex file.",
                    include_name
                ));
            }
            result.push_str(&content[full_match.start()..full_match.end()]);
        }

        if result.len() > MAX_LATEX_SIZE {
            return Err(format!(
                "LaTeX extraction exceeded {} byte limit — paper has too many includes",
                MAX_LATEX_SIZE
            ));
        }

        last_end = full_match.end();
    }

    result.push_str(&content[last_end..]);
    Ok(result)
}

/// Where marker writes its output for a given paper, so the run can pick up
/// extracted figure images afterwards: ~/.pipeline/cache/marker/{hash}/.
pub fn marker_output_dir(paper_hash: &str) -> Option<PathBuf> {
    // Hashes are 16 lowercase hex chars (compute_hash); reject anything else
    // before using it as a path component.
    if paper_hash.len() != 16 || !paper_hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(
        dirs::home_dir()?
            .join(".pipeline")
            .join("cache")
            .join("marker")
            .join(paper_hash),
    )
}

#[cfg(test)]
#[derive(Debug, Clone, Deserialize)]
struct MarkerJsonDocument {
    #[serde(default)]
    block_type: String,
    #[serde(default, deserialize_with = "deserialize_null_default")]
    children: Vec<MarkerJsonBlock>,
}

#[cfg(test)]
#[derive(Debug, Clone, Deserialize)]
struct MarkerJsonBlock {
    #[serde(default)]
    id: String,
    #[serde(default)]
    block_type: String,
    #[serde(default)]
    html: String,
    #[serde(default, deserialize_with = "deserialize_null_default")]
    polygon: Vec<Vec<f64>>,
    #[serde(default, deserialize_with = "deserialize_null_default")]
    bbox: Vec<f64>,
    #[serde(default, deserialize_with = "deserialize_null_default")]
    children: Vec<MarkerJsonBlock>,
    #[serde(default, deserialize_with = "deserialize_null_default")]
    images: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MarkerStructure {
    pub(crate) schema_version: u32,
    #[serde(default)]
    pub(crate) quality_notes: Vec<String>,
    #[serde(default)]
    pub(crate) pages: Vec<MarkerStructuredPage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MarkerStructuredPage {
    pub(crate) number: u32,
    pub(crate) marker_id: String,
    #[serde(default)]
    pub(crate) polygon: Vec<Vec<f64>>,
    #[serde(default)]
    pub(crate) bbox: Vec<f64>,
    #[serde(default)]
    pub(crate) blocks: Vec<MarkerStructuredBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MarkerStructuredBlock {
    pub(crate) marker_id: String,
    pub(crate) marker_type: String,
    pub(crate) role: String,
    #[serde(default)]
    pub(crate) html: String,
    #[serde(default)]
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) polygon: Vec<Vec<f64>>,
    #[serde(default)]
    pub(crate) bbox: Vec<f64>,
    #[serde(default)]
    pub(crate) image_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PaddleStructure {
    pub(crate) schema_version: u32,
    #[serde(default)]
    pub(crate) parser: String,
    #[serde(default)]
    pub(crate) parser_version: String,
    #[serde(default)]
    pub(crate) settings: serde_json::Value,
    #[serde(default)]
    pub(crate) quality_notes: Vec<String>,
    #[serde(default)]
    pub(crate) pages: Vec<PaddleStructuredPage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PaddleStructuredPage {
    pub(crate) number: u32,
    #[serde(default)]
    pub(crate) markdown: String,
    /// Substantive recognition characters measured before Paddle's
    /// cross-page restructuring can move blocks between neighboring pages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) source_text_chars: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) height: Option<u32>,
    #[serde(default)]
    pub(crate) blocks: Vec<PaddleStructuredBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PaddleStructuredBlock {
    pub(crate) block_id: String,
    pub(crate) role: String,
    #[serde(default)]
    pub(crate) block_label: String,
    #[serde(default)]
    pub(crate) markdown: String,
    #[serde(default)]
    pub(crate) text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) boundary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) note_marker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) order: Option<u32>,
    #[serde(default)]
    pub(crate) bbox: Vec<f64>,
    #[serde(default)]
    pub(crate) polygon: Vec<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) confidence: Option<f32>,
    #[serde(default)]
    pub(crate) asset_files: Vec<String>,
    #[serde(default)]
    pub(crate) raw: serde_json::Value,
}

#[cfg(test)]
#[derive(Debug)]
struct MarkerExtraction {
    text: String,
    quality_notes: Vec<String>,
}

#[cfg(test)]
#[derive(Default)]
struct MarkerImageBudget {
    files: usize,
    bytes: usize,
    materialized: HashMap<String, String>,
    warned: HashSet<String>,
}

fn marker_structure_path(paper_hash: &str) -> Option<PathBuf> {
    Some(marker_output_dir(paper_hash)?.join(MARKER_STRUCTURE_FILE))
}

pub(crate) fn read_marker_structure(paper_hash: &str) -> Result<Option<MarkerStructure>, String> {
    let Some(path) = marker_structure_path(paper_hash) else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }
    let text = read_utf8_capped(&path, super::claude::MAX_STDOUT_BYTES)?;
    let structure: MarkerStructure = serde_json::from_str(&text)
        .map_err(|error| format!("Failed to parse {}: {error}", path.display()))?;
    if structure.schema_version != MARKER_STRUCTURE_SCHEMA {
        return Ok(None);
    }
    Ok(Some(structure))
}

pub(crate) fn read_marker_structure_json(paper_hash: &str) -> Result<Option<String>, String> {
    let Some(path) = marker_structure_path(paper_hash) else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }
    read_utf8_capped(&path, super::claude::MAX_STDOUT_BYTES).map(Some)
}

fn paddle_structure_path(paper_hash: &str) -> Option<PathBuf> {
    if paper_hash.len() != 16 || !paper_hash.chars().all(|value| value.is_ascii_hexdigit()) {
        return None;
    }
    Some(
        dirs::home_dir()?
            .join(".pipeline")
            .join("cache")
            .join("paddleocr-vl")
            .join(paper_hash)
            .join(PADDLE_STRUCTURE_FILE),
    )
}

pub(crate) fn read_paddle_structure(paper_hash: &str) -> Result<Option<PaddleStructure>, String> {
    let Some(path) = paddle_structure_path(paper_hash) else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }
    let text = read_utf8_capped(&path, super::claude::MAX_STDOUT_BYTES)?;
    let structure: PaddleStructure = serde_json::from_str(&text)
        .map_err(|error| format!("Failed to parse {}: {error}", path.display()))?;
    if !(1..=PADDLE_STRUCTURE_SCHEMA).contains(&structure.schema_version) {
        return Ok(None);
    }
    Ok(Some(structure))
}

pub(crate) fn read_paddle_structure_json(paper_hash: &str) -> Result<Option<String>, String> {
    let Some(path) = paddle_structure_path(paper_hash) else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }
    read_utf8_capped(&path, super::claude::MAX_STDOUT_BYTES).map(Some)
}

fn paddle_full_cache_root(paper_hash: &str) -> Option<PathBuf> {
    if paper_hash.len() != 16 || !paper_hash.chars().all(|value| value.is_ascii_hexdigit()) {
        return None;
    }
    Some(
        dirs::home_dir()?
            .join(".pipeline")
            .join("cache")
            .join("paddleocr-vl-full")
            .join(paper_hash),
    )
}

fn valid_cache_fingerprint(value: &str) -> bool {
    value.len() == 16 && value.chars().all(|character| character.is_ascii_hexdigit())
}

fn paddle_full_active_dir(paper_hash: &str) -> Result<Option<PathBuf>, String> {
    let Some(root) = paddle_full_cache_root(paper_hash) else {
        return Ok(None);
    };
    let active_path = root.join(PADDLE_FULL_ACTIVE_FILE);
    if !active_path.is_file() {
        return Ok(None);
    }
    let text = read_utf8_capped(&active_path, 64 * 1024)?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| format!("Failed to parse {}: {error}", active_path.display()))?;
    if value.get("schema").and_then(serde_json::Value::as_u64)
        != Some(EXTRACTION_CACHE_SCHEMA as u64)
    {
        return Ok(None);
    }
    let Some(fingerprint) = value.get("fingerprint").and_then(serde_json::Value::as_str) else {
        return Ok(None);
    };
    if !valid_cache_fingerprint(fingerprint) {
        return Err("PaddleOCR-VL full-parser cache has an invalid active fingerprint".to_string());
    }
    let directory = root.join(fingerprint);
    Ok(directory.is_dir().then_some(directory))
}

pub(crate) fn read_paddle_full_structure(
    paper_hash: &str,
) -> Result<Option<PaddleStructure>, String> {
    let Some(directory) = paddle_full_active_dir(paper_hash)? else {
        return Ok(None);
    };
    let path = directory.join(PADDLE_FULL_STRUCTURE_FILE);
    if !path.is_file() {
        return Ok(None);
    }
    let text = read_utf8_capped(&path, super::claude::MAX_STDOUT_BYTES)?;
    let structure: PaddleStructure = serde_json::from_str(&text)
        .map_err(|error| format!("Failed to parse {}: {error}", path.display()))?;
    if structure.schema_version != PADDLE_STRUCTURE_SCHEMA
        || structure.parser != "paddleocr-vl-full"
    {
        return Ok(None);
    }
    Ok(Some(structure))
}

pub(crate) fn read_paddle_structure_for_method(
    paper_hash: &str,
    method: &str,
) -> Result<Option<PaddleStructure>, String> {
    if method == "paddleocr-vl-full" {
        read_paddle_full_structure(paper_hash)
    } else {
        read_paddle_structure(paper_hash)
    }
}

pub(crate) fn read_paddle_structure_json_for_method(
    paper_hash: &str,
    method: &str,
) -> Result<Option<String>, String> {
    if method != "paddleocr-vl-full" {
        return read_paddle_structure_json(paper_hash);
    }
    let Some(directory) = paddle_full_active_dir(paper_hash)? else {
        return Ok(None);
    };
    let path = directory.join(PADDLE_FULL_STRUCTURE_FILE);
    if !path.is_file() {
        return Ok(None);
    }
    let text = read_utf8_capped(&path, super::claude::MAX_STDOUT_BYTES)?;
    let structure: PaddleStructure = serde_json::from_str(&text)
        .map_err(|error| format!("Failed to parse {}: {error}", path.display()))?;
    if structure.schema_version != PADDLE_STRUCTURE_SCHEMA
        || structure.parser != "paddleocr-vl-full"
    {
        return Ok(None);
    }
    Ok(Some(text))
}

fn paddle_full_image_files(paper_hash: &str) -> Result<Vec<PathBuf>, String> {
    let Some(directory) = paddle_full_active_dir(paper_hash)? else {
        return Ok(Vec::new());
    };
    let assets = directory.join("assets");
    if !assets.is_dir() {
        return Ok(Vec::new());
    }
    let mut images = Vec::new();
    let mut walk = crate::safety::WalkBudget::new("PaddleOCR-VL full-parser image discovery");
    for entry in fs::read_dir(&assets)
        .map_err(|error| format!("Failed to read parser image directory: {error}"))?
    {
        walk.entry()?;
        let entry = entry.map_err(|error| format!("Failed to read parser image: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("Failed to inspect parser image: {error}"))?;
        let path = entry.path();
        if file_type.is_file()
            && ["png", "jpg", "jpeg", "gif", "webp"]
                .iter()
                .any(|extension| ext_eq(&path, extension))
        {
            images.push(path);
        }
    }
    images.sort();
    Ok(images)
}

#[derive(Debug, Clone)]
pub(crate) struct PaddleFullImageArtifact {
    pub(crate) source_path: PathBuf,
    pub(crate) display_name: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PaddleFullImageInventory {
    pub(crate) artifacts: Vec<PaddleFullImageArtifact>,
    /// Number of paper-level labels (for example, `figure_1`) that referred
    /// to more than one distinct caption and therefore needed disambiguation.
    pub(crate) disambiguated_labels: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct PaddleFigureIdentity {
    kind: String,
    number: String,
    page: u32,
    caption_id: String,
}

fn normalized_figure_number(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn paddle_figure_identity(
    block: &PaddleStructuredBlock,
    page: u32,
) -> Option<PaddleFigureIdentity> {
    let label = if block.block_label.is_empty() {
        block.role.as_str()
    } else {
        block.block_label.as_str()
    }
    .to_ascii_lowercase();
    if !matches!(
        label.as_str(),
        "caption"
            | "figure_title"
            | "image_caption"
            | "chart_title"
            | "table_title"
            | "table_caption"
            | "paragraph_title"
            | "section_title"
            | "figure"
            | "chart"
            | "table"
    ) {
        return None;
    }
    let caption = if block.text.trim().is_empty() {
        block.markdown.as_str()
    } else {
        block.text.as_str()
    };
    let captures = PADDLE_NUMBERED_CAPTION.captures(caption)?;
    let raw_kind = captures.get(1)?.as_str().to_ascii_lowercase();
    let kind = if raw_kind.starts_with("fig") {
        "figure"
    } else {
        "table"
    };
    let number = normalized_figure_number(captures.get(2)?.as_str());
    if number.is_empty() {
        return None;
    }
    Some(PaddleFigureIdentity {
        kind: kind.to_string(),
        number,
        page,
        caption_id: block.block_id.clone(),
    })
}

fn paddle_asset_page(path: &Path) -> Option<u32> {
    let name = path.file_name()?.to_str()?;
    let digits = name.strip_prefix("page-")?.split('-').next()?;
    digits.parse().ok()
}

fn paddle_identity_base(identity: &PaddleFigureIdentity) -> String {
    if identity.number.is_empty() {
        format!("figure_page_{:04}", identity.page)
    } else {
        format!("{}_{}", identity.kind, identity.number)
    }
}

fn paddle_full_image_inventory_from(
    structure: &PaddleStructure,
    images: Vec<PathBuf>,
) -> PaddleFullImageInventory {
    let mut asset_identities: HashMap<String, PaddleFigureIdentity> = HashMap::new();
    let mut first_page_identity: HashMap<u32, PaddleFigureIdentity> = HashMap::new();
    for page in &structure.pages {
        let mut blocks: Vec<&PaddleStructuredBlock> = page.blocks.iter().collect();
        blocks.sort_by_key(|block| block.order.unwrap_or(u32::MAX));
        let captions: Vec<(u32, PaddleFigureIdentity)> = blocks
            .iter()
            .filter_map(|block| {
                paddle_figure_identity(block, page.number)
                    .map(|identity| (block.order.unwrap_or(u32::MAX), identity))
            })
            .collect();
        if let Some((_, identity)) = captions.first() {
            first_page_identity.insert(page.number, identity.clone());
        }
        for block in blocks {
            let block_order = block.order.unwrap_or(u32::MAX);
            let closest = captions.iter().min_by_key(|(caption_order, _)| {
                (
                    caption_order.abs_diff(block_order),
                    u8::from(*caption_order > block_order),
                )
            });
            if let Some((_, identity)) = closest {
                for asset in &block.asset_files {
                    if let Some(name) = Path::new(asset).file_name().and_then(|name| name.to_str())
                    {
                        asset_identities.insert(name.to_string(), identity.clone());
                    }
                }
            }
        }
    }

    let records: Vec<(PathBuf, PaddleFigureIdentity)> = images
        .into_iter()
        .map(|path| {
            let page = paddle_asset_page(&path).unwrap_or(0);
            let identity = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| asset_identities.get(name))
                .cloned()
                .or_else(|| first_page_identity.get(&page).cloned())
                .unwrap_or_else(|| PaddleFigureIdentity {
                    kind: "figure".to_string(),
                    number: String::new(),
                    page,
                    caption_id: format!("unlabeled-page-{page:04}"),
                });
            (path, identity)
        })
        .collect();

    let mut identities_by_base: HashMap<String, BTreeSet<PaddleFigureIdentity>> = HashMap::new();
    let mut assets_by_identity: HashMap<PaddleFigureIdentity, usize> = HashMap::new();
    for (_, identity) in &records {
        identities_by_base
            .entry(paddle_identity_base(identity))
            .or_default()
            .insert(identity.clone());
        *assets_by_identity.entry(identity.clone()).or_default() += 1;
    }
    let disambiguated_labels = identities_by_base
        .values()
        .filter(|identities| identities.len() > 1)
        .count();
    let identity_ordinals: HashMap<PaddleFigureIdentity, usize> = identities_by_base
        .values()
        .flat_map(|identities| {
            identities
                .iter()
                .cloned()
                .enumerate()
                .map(|(index, identity)| (identity, index + 1))
                .collect::<Vec<_>>()
        })
        .collect();

    let mut seen_within_identity: HashMap<PaddleFigureIdentity, usize> = HashMap::new();
    let mut used_names = HashSet::new();
    let artifacts = records
        .into_iter()
        .map(|(source_path, identity)| {
            let base = paddle_identity_base(&identity);
            let identities = identities_by_base
                .get(&base)
                .map(BTreeSet::len)
                .unwrap_or(1);
            let mut stem = base;
            if identities > 1 {
                stem.push_str(&format!("_page_{:04}", identity.page));
                let same_page = identities_by_base
                    .values()
                    .find(|values| values.contains(&identity))
                    .map(|values| {
                        values
                            .iter()
                            .filter(|candidate| candidate.page == identity.page)
                            .count()
                    })
                    .unwrap_or(1);
                if same_page > 1 {
                    stem.push_str(&format!(
                        "_{}",
                        identity_ordinals.get(&identity).copied().unwrap_or(1)
                    ));
                }
            }
            let occurrence = seen_within_identity.entry(identity.clone()).or_default();
            *occurrence += 1;
            if assets_by_identity.get(&identity).copied().unwrap_or(1) > 1 {
                stem.push_str(&format!("_{occurrence}"));
            }
            let extension = source_path
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or("png")
                .to_ascii_lowercase();
            let mut display_name = format!("{stem}.{extension}");
            let mut collision = 2usize;
            while !used_names.insert(display_name.clone()) {
                display_name = format!("{stem}_{collision}.{extension}");
                collision += 1;
            }
            PaddleFullImageArtifact {
                source_path,
                display_name,
            }
        })
        .collect();
    PaddleFullImageInventory {
        artifacts,
        disambiguated_labels,
    }
}

pub(crate) fn paddle_full_image_inventory(
    paper_hash: &str,
) -> Result<PaddleFullImageInventory, String> {
    let images = paddle_full_image_files(paper_hash)?;
    let Some(structure) = read_paddle_full_structure(paper_hash)? else {
        return Ok(paddle_full_image_inventory_from(
            &PaddleStructure {
                schema_version: PADDLE_STRUCTURE_SCHEMA,
                parser: "paddleocr-vl-full".to_string(),
                parser_version: String::new(),
                settings: serde_json::Value::Null,
                quality_notes: Vec::new(),
                pages: Vec::new(),
            },
            images,
        ));
    };
    Ok(paddle_full_image_inventory_from(&structure, images))
}

#[cfg(test)]
fn marker_type_name(value: &str) -> &str {
    value.rsplit('.').next().unwrap_or(value)
}

#[cfg(test)]
fn marker_block_html(block: &MarkerJsonBlock) -> String {
    if block.children.is_empty() {
        return block.html.clone();
    }
    let mut html = block.html.clone();
    let mut matched_child = false;
    for child in &block.children {
        let child_html = marker_block_html(child);
        let pattern = format!(
            r#"(?is)<content-ref\b[^>]*\bsrc\s*=\s*["']{}["'][^>]*>(?:\s*</content-ref\s*>)?"#,
            regex::escape(&child.id)
        );
        if let Ok(reference) = Regex::new(&pattern) {
            if reference.is_match(&html) {
                matched_child = true;
                html = reference
                    .replace_all(&html, child_html.as_str())
                    .into_owned();
            }
        }
    }
    if html.trim().is_empty() || (!matched_child && html.contains("content-ref")) {
        html = block
            .children
            .iter()
            .map(marker_block_html)
            .collect::<Vec<_>>()
            .join("\n");
    } else if html.contains("content-ref") {
        let unresolved = Regex::new(r"(?is)</?content-ref\b[^>]*>").unwrap();
        html = unresolved.replace_all(&html, "").into_owned();
    }
    html
}

#[cfg(test)]
fn marker_review_html(html: &str) -> String {
    let display_math =
        Regex::new(r#"(?is)<math\b[^>]*display\s*=\s*["']block["'][^>]*>(.*?)</math>"#).unwrap();
    let inline_math = Regex::new(r"(?is)<math\b[^>]*>(.*?)</math>").unwrap();
    let with_display = display_math.replace_all(html, |capture: &regex::Captures<'_>| {
        format!("\n$$\n{}\n$$\n", marker_plain_text(&capture[1]))
    });
    let with_math = inline_math.replace_all(&with_display, |capture: &regex::Captures<'_>| {
        format!("${}$", marker_plain_text(&capture[1]))
    });
    ammonia::clean(&with_math).trim().to_string()
}

#[cfg(test)]
fn marker_plain_text(html: &str) -> String {
    let cleaned = ammonia::clean(html);
    let tags = Regex::new(r"(?is)<[^>]+>").unwrap();
    let without_tags = tags.replace_all(&cleaned, " ");
    let entities = Regex::new(r"&(#x[0-9A-Fa-f]+|#[0-9]+|amp|lt|gt|quot|apos|nbsp);").unwrap();
    let decoded = entities.replace_all(&without_tags, |capture: &regex::Captures<'_>| {
        let entity = &capture[1];
        match entity {
            "amp" => "&".to_string(),
            "lt" => "<".to_string(),
            "gt" => ">".to_string(),
            "quot" => "\"".to_string(),
            "apos" => "'".to_string(),
            "nbsp" => " ".to_string(),
            _ => {
                let value = entity
                    .strip_prefix("#x")
                    .and_then(|value| u32::from_str_radix(value, 16).ok())
                    .or_else(|| {
                        entity
                            .strip_prefix('#')
                            .and_then(|value| value.parse::<u32>().ok())
                    });
                value
                    .and_then(char::from_u32)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| capture[0].to_string())
            }
        }
    });
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
fn collect_marker_images<'a>(block: &'a MarkerJsonBlock, output: &mut Vec<(&'a str, &'a str)>) {
    output.extend(
        block
            .images
            .iter()
            .map(|(id, content)| (id.as_str(), content.as_str())),
    );
    for child in &block.children {
        collect_marker_images(child, output);
    }
}

#[cfg(test)]
fn marker_image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("webp")
    } else {
        None
    }
}

#[cfg(test)]
fn materialize_marker_images(
    block: &MarkerJsonBlock,
    page: u32,
    out_dir: &Path,
    budget: &mut MarkerImageBudget,
    quality_notes: &mut Vec<String>,
) -> Vec<String> {
    let mut encoded = Vec::new();
    collect_marker_images(block, &mut encoded);
    let mut files = Vec::new();
    for (marker_id, content) in encoded {
        if let Some(existing) = budget.materialized.get(marker_id) {
            files.push(existing.clone());
            continue;
        }
        if budget.files >= MAX_MARKER_IMAGES {
            if budget.warned.insert("count".to_string()) {
                quality_notes.push(format!(
                    "Marker emitted more than {MAX_MARKER_IMAGES} extracted images; additional images were omitted. Consult the rendered PDF pages."
                ));
            }
            continue;
        }
        let max_encoded_bytes = MAX_MARKER_IMAGE_BYTES.div_ceil(3).saturating_mul(4);
        if content.len() > max_encoded_bytes {
            if budget.warned.insert("individual".to_string()) {
                quality_notes.push(format!(
                    "A Marker image exceeded the {} MB safety limit and was omitted. Consult the rendered PDF page.",
                    MAX_MARKER_IMAGE_BYTES / 1024 / 1024
                ));
            }
            continue;
        }
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(content.as_bytes()) else {
            if budget.warned.insert("decode".to_string()) {
                quality_notes.push(
                    "Marker emitted an invalid encoded image; it was omitted. Consult the rendered PDF page."
                        .to_string(),
                );
            }
            continue;
        };
        if bytes.len() > MAX_MARKER_IMAGE_BYTES
            || budget.bytes.saturating_add(bytes.len()) > MAX_MARKER_IMAGE_TOTAL_BYTES
        {
            if budget.warned.insert("bytes".to_string()) {
                quality_notes.push(format!(
                    "Marker extracted-image materialization reached its {} MB safety budget; additional images were omitted. Consult the rendered PDF pages.",
                    MAX_MARKER_IMAGE_TOTAL_BYTES / 1024 / 1024
                ));
            }
            continue;
        }
        let Some(extension) = marker_image_extension(&bytes) else {
            if budget.warned.insert("format".to_string()) {
                quality_notes.push(
                    "Marker emitted an extracted image in an unrecognized format; it was omitted. Consult the rendered PDF page."
                        .to_string(),
                );
            }
            continue;
        };
        let suffix = format!("{:x}", Sha256::digest(marker_id.as_bytes()));
        let filename = format!("marker-page-{page:04}-{}.{}", &suffix[..12], extension);
        if atomic_write_cache(&out_dir.join(&filename), &bytes).is_err() {
            if budget.warned.insert("write".to_string()) {
                quality_notes.push(
                    "Pipeline could not retain one or more Marker images. Consult the rendered PDF pages."
                        .to_string(),
                );
            }
            continue;
        }
        budget.files += 1;
        budget.bytes = budget.bytes.saturating_add(bytes.len());
        budget
            .materialized
            .insert(marker_id.to_string(), filename.clone());
        files.push(filename);
    }
    files.sort();
    files.dedup();
    files
}

#[cfg(test)]
fn leading_footnote_marker(text: &str) -> Option<String> {
    let marker = Regex::new(r"^\s*(\d{1,3}|[*†‡])(?:[.)])?\s+").unwrap();
    marker
        .captures(text)
        .and_then(|capture| capture.get(1))
        .map(|value| value.as_str().to_string())
}

#[cfg(test)]
fn preceding_superscript_anchor(blocks: &[MarkerStructuredBlock], marker: &str) -> bool {
    let pattern = format!(r"(?is)<sup\b[^>]*>\s*{}\s*</sup>", regex::escape(marker));
    let Ok(anchor) = Regex::new(&pattern) else {
        return false;
    };
    blocks
        .iter()
        .filter(|block| block.role == "body")
        .any(|block| anchor.is_match(&block.html))
}

#[cfg(test)]
fn bbox_vertical_extent(bbox: &[f64]) -> Option<(f64, f64)> {
    if bbox.len() < 4 || !bbox[1].is_finite() || !bbox[3].is_finite() {
        return None;
    }
    Some((bbox[1].min(bbox[3]), bbox[1].max(bbox[3])))
}

#[cfg(test)]
fn page_vertical_extent(page: &MarkerJsonBlock) -> Option<(f64, f64)> {
    bbox_vertical_extent(&page.bbox).or_else(|| {
        let mut values = page
            .polygon
            .iter()
            .filter_map(|point| point.get(1).copied())
            .filter(|value| value.is_finite());
        let first = values.next()?;
        let (min, max) = values.fold((first, first), |(min, max), value| {
            (min.min(value), max.max(value))
        });
        Some((min, max))
    })
}

#[cfg(test)]
fn possible_footnote(
    blocks: &[MarkerStructuredBlock],
    index: usize,
    page_extent: Option<(f64, f64)>,
) -> bool {
    let block = &blocks[index];
    if block.marker_type != "Text" || block.text.len() > 2_000 {
        return false;
    }
    let Some(marker) = leading_footnote_marker(&block.text) else {
        return false;
    };
    if !preceding_superscript_anchor(&blocks[..index], &marker) {
        return false;
    }
    let Some((page_top, page_bottom)) = page_extent else {
        return false;
    };
    let page_height = page_bottom - page_top;
    let Some((block_top, _)) = bbox_vertical_extent(&block.bbox) else {
        return false;
    };
    if page_height <= 0.0 || (block_top - page_top) / page_height < 0.75 {
        return false;
    }
    let previous_bottom = blocks[..index]
        .iter()
        .rev()
        .filter(|candidate| candidate.role == "body")
        .find_map(|candidate| bbox_vertical_extent(&candidate.bbox).map(|(_, bottom)| bottom));
    previous_bottom.is_some_and(|bottom| block_top - bottom >= page_height * 0.012)
}

#[cfg(test)]
fn marker_role(marker_type: &str) -> &'static str {
    match marker_type {
        "Footnote" => "footnote",
        "PageHeader" => "page_header",
        "PageFooter" => "page_footer",
        _ => "body",
    }
}

#[cfg(test)]
fn marker_block_placeholder(marker_type: &str, page: u32) -> String {
    match marker_type {
        "Figure" | "FigureGroup" | "Picture" | "PictureGroup" => format!(
            "<p><em>[Visual block on page {page}; inspect the corresponding page or extracted figure asset.]</em></p>"
        ),
        _ => String::new(),
    }
}

#[cfg(test)]
fn render_marker_page(page: &MarkerStructuredPage) -> String {
    let mut output = format!("<!-- PAGE {} -->\n\n", page.number);
    let mut footnotes = Vec::new();
    for block in &page.blocks {
        match block.role.as_str() {
            "page_header" | "page_footer" => {}
            "footnote" => footnotes.push(block),
            "possible_footnote" => {
                output.push_str(&format!(
                    "<!-- BEGIN POSSIBLE_FOOTNOTE page={} marker_id={} -->\n\
                     <aside data-document-role=\"possible-footnote\">\n{}\n</aside>\n\
                     <!-- END POSSIBLE_FOOTNOTE page={} -->\n\n",
                    page.number,
                    &format!("{:x}", Sha256::digest(block.marker_id.as_bytes()))[..12],
                    block.html,
                    page.number
                ));
            }
            _ => {
                output.push_str(&block.html);
                output.push_str("\n\n");
            }
        }
    }
    if !footnotes.is_empty() {
        output.push_str(&format!("<!-- BEGIN FOOTNOTES page={} -->\n", page.number));
        for block in footnotes {
            output.push_str("<aside data-document-role=\"footnote\">\n");
            output.push_str(&block.html);
            output.push_str("\n</aside>\n");
        }
        output.push_str(&format!("<!-- END FOOTNOTES page={} -->\n", page.number));
    }
    output.trim().to_string()
}

#[cfg(test)]
fn normalize_marker_json(raw: &str, out_dir: &Path) -> Result<MarkerExtraction, String> {
    let parsed: MarkerJsonDocument = serde_json::from_str(raw)
        .map_err(|error| format!("Failed to parse Marker JSON output: {error}"))?;
    if !parsed.block_type.is_empty() && marker_type_name(&parsed.block_type) != "Document" {
        return Err(format!(
            "Marker JSON returned {} instead of a Document",
            parsed.block_type
        ));
    }
    let mut quality_notes = Vec::new();
    let mut pages = Vec::new();
    let mut image_budget = MarkerImageBudget::default();
    for (page_index, page) in parsed.children.iter().enumerate() {
        if marker_type_name(&page.block_type) != "Page" {
            continue;
        }
        let number = page_index as u32 + 1;
        let mut blocks = Vec::new();
        for source in &page.children {
            let marker_type = marker_type_name(&source.block_type).to_string();
            let resolved_html = marker_block_html(source);
            let mut html = marker_review_html(&resolved_html);
            if html.trim().is_empty() {
                html = marker_block_placeholder(&marker_type, number);
            }
            let text = marker_plain_text(&resolved_html);
            let image_files = materialize_marker_images(
                source,
                number,
                out_dir,
                &mut image_budget,
                &mut quality_notes,
            );
            blocks.push(MarkerStructuredBlock {
                marker_id: source.id.clone(),
                marker_type: marker_type.clone(),
                role: marker_role(&marker_type).to_string(),
                html,
                text,
                polygon: source.polygon.clone(),
                bbox: source.bbox.clone(),
                image_files,
            });
        }
        let page_extent = page_vertical_extent(page);
        let mut candidates = 0usize;
        for index in 0..blocks.len() {
            if possible_footnote(&blocks, index, page_extent) {
                blocks[index].role = "possible_footnote".to_string();
                candidates += 1;
            }
        }
        if candidates > 0 {
            quality_notes.push(format!(
                "Marker classified {candidates} bottom-of-page text block(s) on page {number} as possible footnotes using layout and matching superscript evidence. Verify the page image if a finding depends on whether this text is a footnote."
            ));
        }
        pages.push(MarkerStructuredPage {
            number,
            marker_id: page.id.clone(),
            polygon: page.polygon.clone(),
            bbox: page.bbox.clone(),
            blocks,
        });
    }
    if pages.is_empty() {
        return Err("Marker JSON output contained no page blocks".to_string());
    }
    let text = pages
        .iter()
        .map(render_marker_page)
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.trim().is_empty() {
        return Err("Marker structured normalization returned empty output".to_string());
    }
    if text.len() > super::claude::MAX_STDOUT_BYTES {
        return Err("Normalized Marker document exceeded the 50 MB safety limit".to_string());
    }
    let structure = MarkerStructure {
        schema_version: MARKER_STRUCTURE_SCHEMA,
        quality_notes: quality_notes.clone(),
        pages,
    };
    let structure_json = serde_json::to_string_pretty(&structure)
        .map_err(|error| format!("Failed to serialize normalized Marker structure: {error}"))?;
    if structure_json.len() > super::claude::MAX_STDOUT_BYTES {
        return Err("Normalized Marker structure exceeded the 50 MB safety limit".to_string());
    }
    atomic_write_cache(
        &out_dir.join(MARKER_STRUCTURE_FILE),
        structure_json.as_bytes(),
    )?;
    atomic_write_cache(&out_dir.join(MARKER_DOCUMENT_FILE), text.as_bytes())?;
    Ok(MarkerExtraction {
        text,
        quality_notes,
    })
}

/// Image files marker emitted for a paper (figures/tables extracted from the
/// PDF), retained only for reading historical extraction caches.
pub fn marker_image_files(paper_hash: &str) -> Result<Vec<PathBuf>, String> {
    let Some(root) = marker_output_dir(paper_hash) else {
        return Ok(Vec::new());
    };
    let mut images = Vec::new();
    let mut stack = vec![root];
    let mut walk = crate::safety::WalkBudget::new("Marker image discovery");
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            walk.entry()?;
            let p = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|error| format!("Failed to inspect marker output: {error}"))?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                walk.directory()?;
                stack.push(p);
            } else if file_type.is_file()
                && ["png", "jpg", "jpeg", "gif", "webp"]
                    .iter()
                    .any(|ext| ext_eq(&p, ext))
            {
                images.push(p);
            }
        }
    }
    images.sort();
    Ok(images)
}

const EXTRACTION_CACHE_SCHEMA: u32 = 3;

fn cache_fingerprint(value: &serde_json::Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    format!("{:x}", Sha256::digest(bytes))[..16].to_string()
}

fn atomic_write_cache(path: &Path, content: &[u8]) -> Result<(), String> {
    use std::io::Write as _;
    let parent = path
        .parent()
        .ok_or_else(|| format!("Cache path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create extraction cache: {error}"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("Failed to create extraction cache file: {error}"))?;
    temporary
        .write_all(content)
        .map_err(|error| format!("Failed to write extraction cache: {error}"))?;
    temporary
        .as_file_mut()
        .sync_all()
        .map_err(|error| format!("Failed to flush extraction cache: {error}"))?;
    temporary
        .persist(path)
        .map_err(|error| format!("Failed to publish extraction cache: {}", error.error))?;
    Ok(())
}

/// Extract text from PDF using pdftotext.
fn extract_pdftotext(path: &Path) -> Result<String, String> {
    let path_str = path
        .to_str()
        .ok_or_else(|| format!("Path contains invalid UTF-8: {}", path.display()))?;
    let pdftotext_bin = find_command("pdftotext").ok_or("pdftotext not found on PATH")?;
    let mut command = pdftotext_bin.command(["-layout", path_str, "-"]);
    command.env("PATH", env::full_path());
    let output = run_bounded_output(
        command,
        "pdftotext",
        std::time::Duration::from_secs(crate::settings::load().step_timeout_secs.max(60)),
        super::claude::MAX_STDOUT_BYTES,
        None,
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let suffix = if output.stderr_truncated {
            " (truncated)"
        } else {
            ""
        };
        return Err(format!("pdftotext failed: {}{suffix}", stderr.trim()));
    }
    if output.stdout_truncated {
        return Err("pdftotext output exceeded the 50 MB safety limit".to_string());
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err("pdftotext returned empty output".to_string());
    }
    Ok(text)
}

const PADDLE_MODEL_ALIAS: &str = "paddleocr-vl-1.6";
const MAX_PADDLE_MODEL_LIST_BYTES: usize = 1024 * 1024;
const PADDLE_READINESS_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

fn paddle_server_token() -> Result<String, String> {
    use std::fmt::Write as _;
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes)
        .map_err(|error| format!("Failed to generate managed server credentials: {error}"))?;
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut token, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(token)
}

fn paddle_model_list_has_alias(value: &serde_json::Value) -> bool {
    value
        .get("data")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|models| {
            models.iter().any(|model| {
                model.get("id").and_then(serde_json::Value::as_str) == Some(PADDLE_MODEL_ALIAS)
            })
        })
}

struct PaddleServer {
    child: std::process::Child,
    pid: u32,
    base_url: String,
    api_key: String,
}

impl Drop for PaddleServer {
    fn drop(&mut self) {
        if self.pid > 0 {
            crate::commands::kill_process(self.pid);
            crate::commands::unregister_child_pid(self.pid);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn start_paddle_server(
    paths: &crate::engines::PaddleEnginePaths,
    settings: &crate::settings::Settings,
) -> Result<PaddleServer, String> {
    use std::process::Stdio;

    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("Failed to reserve a PaddleOCR-VL port: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("Failed to inspect the PaddleOCR-VL port: {error}"))?
        .port();
    drop(listener);

    let model = paths
        .model
        .to_str()
        .ok_or("PaddleOCR-VL model path is not valid UTF-8")?;
    let projector = paths
        .mmproj
        .to_str()
        .ok_or("PaddleOCR-VL projector path is not valid UTF-8")?;
    let port = port.to_string();
    let resolved_concurrency = crate::settings::resolved_paddle_page_concurrency(settings);
    let page_concurrency = resolved_concurrency.to_string();
    // llama.cpp's --ctx-size is the total KV budget shared by all parallel
    // slots. Scale it with concurrency so a two-page run still gives every
    // page the model's full 16K context.
    let total_context = (16_384u32 * resolved_concurrency).to_string();
    let mtmd_batch_tokens =
        crate::settings::resolved_paddle_mtmd_batch_tokens(settings).to_string();
    let api_key = paddle_server_token()?;
    let mut command = StdCommand::new(&paths.server);
    command.args([
        "-m",
        model,
        "--mmproj",
        projector,
        "--host",
        "127.0.0.1",
        "--port",
        &port,
        "--temp",
        "0",
        "--ctx-size",
        &total_context,
        "--n-gpu-layers",
        "99",
        "--parallel",
        &page_concurrency,
        "--mtmd-batch-max-tokens",
        &mtmd_batch_tokens,
        "--flash-attn",
        &settings.paddle_flash_attention,
        "--alias",
        PADDLE_MODEL_ALIAS,
        "--api-key",
        &api_key,
        "--no-ui",
    ]);
    crate::pipeline::claude::configure_silent_command(&mut command);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|error| format!("Failed to start managed llama-server: {error}"))?;
    let pid = child.id();
    if pid > 0 {
        crate::commands::register_child_pid(pid);
    }
    let base_url = format!("http://127.0.0.1:{port}");
    let client = &*crate::pipeline::api_common::LOCAL_HTTP_CLIENT;
    let started = std::time::Instant::now();
    loop {
        if crate::commands::is_cancelled() {
            if pid > 0 {
                crate::commands::kill_process(pid);
                crate::commands::unregister_child_pid(pid);
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err("Pipeline cancelled".to_string());
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Failed to inspect llama-server: {error}"))?
        {
            if pid > 0 {
                crate::commands::unregister_child_pid(pid);
            }
            return Err(format!(
                "Managed llama-server exited during model loading (exit {})",
                status.code().unwrap_or(-1)
            ));
        }
        if let Ok(response) = client
            .get(format!("{base_url}/health"))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            if response.status().is_success() {
                let identity = client
                    .get(format!("{base_url}/v1/models"))
                    .bearer_auth(&api_key)
                    .timeout(std::time::Duration::from_secs(2))
                    .send()
                    .await;
                if let Ok(identity) = identity {
                    if identity.status().is_success() {
                        if let Ok(bytes) = crate::pipeline::api_common::response_bytes_limited(
                            identity,
                            MAX_PADDLE_MODEL_LIST_BYTES,
                            None,
                        )
                        .await
                        {
                            if serde_json::from_slice::<serde_json::Value>(&bytes)
                                .ok()
                                .as_ref()
                                .is_some_and(paddle_model_list_has_alias)
                            {
                                return Ok(PaddleServer {
                                    child,
                                    pid,
                                    base_url,
                                    api_key,
                                });
                            }
                        }
                    }
                }
            }
        }
        if started.elapsed() >= std::time::Duration::from_secs(180) {
            if pid > 0 {
                crate::commands::kill_process(pid);
                crate::commands::unregister_child_pid(pid);
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err("PaddleOCR-VL model loading timed out after 180 seconds".to_string());
        }
        let _ = crate::commands::await_or_cancel(
            tokio::time::sleep(PADDLE_READINESS_POLL_INTERVAL),
            None,
        )
        .await;
    }
}

fn paddle_substantive_chars(text: &str) -> usize {
    text.lines()
        .filter(|line| !line.contains(PADDLE_REGION_WARNING_PREFIX))
        .flat_map(str::chars)
        .filter(|value| !value.is_whitespace())
        .count()
}

fn paddle_char_count_is_suspicious(extracted_chars: usize, baseline_len: Option<usize>) -> bool {
    baseline_len.is_some_and(|baseline_len| {
        baseline_len >= SUSPECT_BASELINE_MIN_CHARS && extracted_chars < baseline_len / 10
    })
}

fn llm_extraction_cache_path(hash: &str, settings: &crate::settings::Settings) -> Option<PathBuf> {
    if hash.len() != 16 || !hash.chars().all(|value| value.is_ascii_hexdigit()) {
        return None;
    }
    let identity = serde_json::json!({
        "schema": EXTRACTION_CACHE_SCHEMA,
        "app_version": env!("CARGO_PKG_VERSION"),
        "engine": "llm-pdf-bounded-v2",
        "provider": settings.preferred_provider,
        "transport": if provider_uses_direct_api(settings) { "api" } else { "cli" },
        "claude_model": settings.claude_model,
        "claude_cli_selection": settings.claude_cli_model_selection,
        "claude_api_selection": settings.claude_api_model_selection,
        "claude_effort": settings.claude_effort,
        "codex_model": settings.codex_model,
        "codex_cli_selection": settings.codex_cli_model_selection,
        "codex_api_selection": settings.codex_api_model_selection,
        "codex_effort": settings.codex_effort,
        "gemini_model": settings.gemini_model,
        "gemini_cli_selection": settings.gemini_cli_model_selection,
        "gemini_api_selection": settings.gemini_api_model_selection,
        "chunk_pages": LLM_CHUNK_MAX_PAGES,
        "chunk_chars": LLM_CHUNK_TARGET_BASELINE_CHARS,
        "max_output_tokens": EXTRACTION_MAX_OUTPUT_TOKENS,
    });
    Some(
        dirs::home_dir()?
            .join(".pipeline")
            .join("cache")
            .join("llm-pdf")
            .join(hash)
            .join(format!("{}.md", cache_fingerprint(&identity))),
    )
}

fn full_parser_cache_fingerprint(
    paths: &crate::engines::PaddleFullParserPaths,
    settings: &crate::settings::Settings,
) -> String {
    cache_fingerprint(&serde_json::json!({
        "schema": EXTRACTION_CACHE_SCHEMA,
        "structure_schema": PADDLE_STRUCTURE_SCHEMA,
        "engine": "paddleocr-vl-full",
        "parser_release": paths.release,
        "parser_integrity": paths.integrity_sha256,
        "sidecar_sha256": paths.sidecar_sha256,
        "recognition_integrity": paths.paddle.integrity_sha256,
        "concurrency": crate::settings::resolved_paddle_page_concurrency(settings),
        "vision_batch": crate::settings::resolved_paddle_mtmd_batch_tokens(settings),
        "flash_attention": settings.paddle_flash_attention,
        "max_output_tokens": settings.paddle_max_output_tokens,
        "page_retries": settings.paddle_page_retries,
        "layout_detection": settings.paddle_full_layout_detection,
        "layout_threshold": settings.paddle_full_layout_threshold,
        "layout_nms": settings.paddle_full_layout_nms,
        "layout_merge_bboxes_mode": settings.paddle_full_layout_merge_bboxes_mode,
        "merge_layout_blocks": settings.paddle_full_merge_layout_blocks,
        "ocr_image_blocks": settings.paddle_full_ocr_image_blocks,
        "format_block_content": settings.paddle_full_format_block_content,
        "merge_tables": settings.paddle_full_merge_tables,
        "relevel_titles": settings.paddle_full_relevel_titles,
        "show_formula_numbers": settings.paddle_full_show_formula_numbers,
    }))
}

fn paddle_full_block_markdown(page: &PaddleStructuredPage) -> String {
    page.blocks
        .iter()
        .filter(|block| {
            !matches!(
                block.role.as_str(),
                "number" | "header" | "header_image" | "footer" | "footer_image"
            )
        })
        .map(|block| block.markdown.trim())
        .filter(|block| !block.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn paddle_full_best_markdown(page: &PaddleStructuredPage) -> String {
    let exported = page.markdown.trim();
    let blocks = paddle_full_block_markdown(page);
    if exported.is_empty()
        || (!blocks.is_empty()
            && paddle_substantive_chars(exported).saturating_mul(2)
                < paddle_substantive_chars(&blocks))
    {
        blocks
    } else {
        exported.to_string()
    }
}

fn render_paddle_full_page(page: &PaddleStructuredPage) -> String {
    let markdown = paddle_full_best_markdown(page);
    let markdown = markdown
        .replace("](assets/", "](../artifacts/figures/")
        .replace("src=\"assets/", "src=\"../artifacts/figures/")
        .replace("src='assets/", "src='../artifacts/figures/");
    format!("<!-- PAGE {} -->\n\n{}", page.number, markdown)
        .trim()
        .to_string()
}

fn paddle_full_page_has_content(page: &PaddleStructuredPage) -> bool {
    if !page.markdown.trim().is_empty() {
        return true;
    }
    page.blocks.iter().any(|block| {
        !matches!(
            block.role.as_str(),
            "number" | "header" | "header_image" | "footer" | "footer_image"
        ) && (!block.markdown.trim().is_empty()
            || !block.text.trim().is_empty()
            || !block.asset_files.is_empty())
    })
}

fn full_parser_extraction_from_structure(
    structure: &PaddleStructure,
    path: &Path,
    hash: &str,
    baseline: Option<&[usize]>,
) -> Result<ExtractionResult, String> {
    if baseline.is_some_and(|pages| pages.len() > MAX_RENDERED_PDF_PAGES as usize) {
        return Err(format!(
            "PaddleOCR-VL Full Parser is limited to {MAX_RENDERED_PDF_PAGES} pages per PDF"
        ));
    }
    if structure.schema_version != PADDLE_STRUCTURE_SCHEMA
        || structure.parser != "paddleocr-vl-full"
        || structure.pages.is_empty()
    {
        return Err("PaddleOCR-VL full parser returned an incompatible structure".to_string());
    }
    if let Some(baseline) = baseline {
        if structure.pages.len() != baseline.len() {
            return Err(format!(
                "PaddleOCR-VL full parser returned {} pages for a {}-page PDF",
                structure.pages.len(),
                baseline.len()
            ));
        }
    }
    let mut restructured_empty_pages = Vec::new();
    for (index, page) in structure.pages.iter().enumerate() {
        let expected = index as u32 + 1;
        if page.number != expected {
            return Err(format!(
                "PaddleOCR-VL full parser returned page {} where page {expected} was expected",
                page.number
            ));
        }
        if !paddle_full_page_has_content(page) && page.source_text_chars.unwrap_or(0) == 0 {
            return Err(format!(
                "PaddleOCR-VL full parser returned no content for page {expected}"
            ));
        }
        if !paddle_full_page_has_content(page) {
            restructured_empty_pages.push(expected);
        }
        let rendered = render_paddle_full_page(page);
        let recognized_chars = page
            .source_text_chars
            .unwrap_or_else(|| paddle_substantive_chars(&rendered));
        let baseline_chars = baseline.and_then(|pages| pages.get(index)).copied();
        if paddle_char_count_is_suspicious(recognized_chars, baseline_chars) {
            return Err(format!(
                "PaddleOCR-VL full parser returned incomplete output for page {expected} (recognized {recognized_chars} substantive characters; text layer has {})",
                baseline_chars.unwrap_or_default()
            ));
        }
    }
    let text = structure
        .pages
        .iter()
        .map(render_paddle_full_page)
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.len() > super::claude::MAX_STDOUT_BYTES {
        return Err(
            "PaddleOCR-VL full-parser document exceeded the 50 MB safety limit".to_string(),
        );
    }
    let mut quality_notes = structure.quality_notes.clone();
    if !restructured_empty_pages.is_empty() {
        quality_notes.push(format!(
            "PaddleOCR cross-page restructuring moved all recognized content from page(s) {} into adjacent structured blocks.",
            restructured_empty_pages
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    quality_notes.extend(scan_math_quality(&text));
    Ok(ExtractionResult {
        text,
        method: "paddleocr-vl-full".to_string(),
        source_path: path.to_string_lossy().to_string(),
        paper_hash: hash.to_string(),
        quality_notes,
    })
}

fn activate_paddle_full_cache(root: &Path, fingerprint: &str) -> Result<(), String> {
    let active = serde_json::json!({
        "schema": EXTRACTION_CACHE_SCHEMA,
        "fingerprint": fingerprint,
    });
    atomic_write_cache(
        &root.join(PADDLE_FULL_ACTIVE_FILE),
        &serde_json::to_vec_pretty(&active)
            .map_err(|error| format!("Failed to serialize parser cache activation: {error}"))?,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_paddle_full_sidecar(
    paths: &crate::engines::PaddleFullParserPaths,
    settings: &crate::settings::Settings,
    input: &Path,
    output: &Path,
    assets: &Path,
    base_url: &str,
    api_key: &str,
    baseline_chars: &[usize],
    timeout: std::time::Duration,
) -> Result<BoundedOutput, String> {
    let boolean = |value: bool| if value { "true" } else { "false" };
    let baseline_chars = serde_json::to_string(baseline_chars)
        .map_err(|error| format!("Failed to serialize PDF text-layer baseline: {error}"))?;
    let mut command = StdCommand::new(&paths.python);
    let arguments: Vec<std::ffi::OsString> = vec![
        "-I".into(),
        "-B".into(),
        paths.script.as_os_str().to_owned(),
        "--input".into(),
        input.as_os_str().to_owned(),
        "--output".into(),
        output.as_os_str().to_owned(),
        "--assets-dir".into(),
        assets.as_os_str().to_owned(),
        "--layout-model-dir".into(),
        paths.layout_model.as_os_str().to_owned(),
        "--server-url".into(),
        base_url.into(),
        "--server-model".into(),
        PADDLE_MODEL_ALIAS.into(),
        "--concurrency".into(),
        crate::settings::resolved_paddle_page_concurrency(settings)
            .to_string()
            .into(),
        "--max-new-tokens".into(),
        settings.paddle_max_output_tokens.to_string().into(),
        "--page-retries".into(),
        settings.paddle_page_retries.to_string().into(),
        "--baseline-chars".into(),
        baseline_chars.into(),
        "--layout-detection".into(),
        boolean(settings.paddle_full_layout_detection).into(),
        "--layout-threshold".into(),
        settings.paddle_full_layout_threshold.to_string().into(),
        "--layout-nms".into(),
        boolean(settings.paddle_full_layout_nms).into(),
        "--layout-merge-bboxes-mode".into(),
        settings.paddle_full_layout_merge_bboxes_mode.clone().into(),
        "--merge-layout-blocks".into(),
        boolean(settings.paddle_full_merge_layout_blocks).into(),
        "--ocr-image-blocks".into(),
        boolean(settings.paddle_full_ocr_image_blocks).into(),
        "--format-block-content".into(),
        boolean(settings.paddle_full_format_block_content).into(),
        "--merge-tables".into(),
        boolean(settings.paddle_full_merge_tables).into(),
        "--relevel-titles".into(),
        boolean(settings.paddle_full_relevel_titles).into(),
        "--show-formula-numbers".into(),
        boolean(settings.paddle_full_show_formula_numbers).into(),
    ];
    command.args(arguments);
    let mut environment = crate::engines::paddle_full_parser_env(paths);
    environment.push(("PIPELINE_PADDLE_API_KEY".to_string(), api_key.to_string()));
    crate::engines::apply_managed_environment(&mut command, &environment);
    if let Some(parent) = output.parent() {
        command.current_dir(parent);
    }
    run_bounded_output(
        command,
        "PaddleOCR-VL full parser",
        timeout,
        4 * 1024 * 1024,
        output.parent(),
    )
}

async fn extract_paddle_full(
    app: &crate::emit::EventBus,
    path: &Path,
    hash: &str,
    settings: &crate::settings::Settings,
) -> Result<ExtractionResult, String> {
    let started = std::time::Instant::now();
    let total_timeout = std::time::Duration::from_secs(settings.pdf_extraction_timeout_secs);
    let paths = crate::engines::paddle_full_parser_paths()?;
    let baseline = {
        let pdf = path.to_path_buf();
        tokio::task::spawn_blocking(move || pdftotext_page_baseline(&pdf))
            .await
            .unwrap_or(None)
    };
    if baseline
        .as_ref()
        .is_some_and(|pages| pages.len() > MAX_RENDERED_PDF_PAGES as usize)
    {
        return Err(format!(
            "PaddleOCR-VL Full Parser is limited to {MAX_RENDERED_PDF_PAGES} pages per PDF"
        ));
    }
    let root = paddle_full_cache_root(hash)
        .ok_or_else(|| "Could not create PaddleOCR-VL full-parser cache path".to_string())?;
    fs::create_dir_all(&root)
        .map_err(|error| format!("Failed to create full-parser cache: {error}"))?;
    let fingerprint = full_parser_cache_fingerprint(&paths, settings);
    let target = root.join(&fingerprint);
    let structure_path = target.join(PADDLE_FULL_STRUCTURE_FILE);
    if settings.reuse_pdf_extraction_cache && structure_path.is_file() {
        let text = read_utf8_capped(&structure_path, super::claude::MAX_STDOUT_BYTES)?;
        if let Ok(structure) = serde_json::from_str::<PaddleStructure>(&text) {
            if let Ok(extraction) =
                full_parser_extraction_from_structure(&structure, path, hash, baseline.as_deref())
            {
                activate_paddle_full_cache(&root, &fingerprint)?;
                extraction_log(
                    app,
                    "PaddleOCR-VL Full Parser: reused a schema-validated structured cache",
                );
                return Ok(extraction);
            }
        }
    }

    extraction_log(
        app,
        format!(
            "PaddleOCR-VL Full Parser: layout={}, NMS={}, merge blocks={}, image OCR={}, cross-page tables={}, relevel titles={}",
            settings.paddle_full_layout_detection,
            settings.paddle_full_layout_nms,
            settings.paddle_full_merge_layout_blocks,
            settings.paddle_full_ocr_image_blocks,
            settings.paddle_full_merge_tables,
            settings.paddle_full_relevel_titles,
        ),
    );
    extraction_log(
        app,
        "PaddleOCR-VL Full Parser: loading the managed Q8 recognition server",
    );
    let server = start_paddle_server(&paths.paddle, settings).await?;
    let staging = tempfile::Builder::new()
        .prefix(".full-parser-staging-")
        .tempdir_in(&root)
        .map_err(|error| format!("Failed to stage full-parser output: {error}"))?;
    let staged_structure = staging.path().join(PADDLE_FULL_STRUCTURE_FILE);
    let staged_assets = staging.path().join("assets");
    let parser_paths = paths.clone();
    let parser_settings = settings.clone();
    let input = path.to_path_buf();
    let output = staged_structure.clone();
    let assets = staged_assets.clone();
    let base_url = format!("{}/v1", server.base_url);
    let api_key = server.api_key.clone();
    let baseline_chars = baseline.clone().unwrap_or_default();
    let timeout = total_timeout
        .checked_sub(started.elapsed())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| {
            format!(
                "PaddleOCR-VL Full Parser exceeded the {}s extraction budget while loading",
                settings.pdf_extraction_timeout_secs
            )
        })?;
    let sidecar = tokio::task::spawn_blocking(move || {
        run_paddle_full_sidecar(
            &parser_paths,
            &parser_settings,
            &input,
            &output,
            &assets,
            &base_url,
            &api_key,
            &baseline_chars,
            timeout,
        )
    })
    .await
    .map_err(|error| format!("PaddleOCR-VL full-parser task failed: {error}"))??;
    if !sidecar.status.success() {
        let stderr = String::from_utf8_lossy(&sidecar.stderr);
        let diagnostics = paddle_diagnostic_lines(&stderr, 50).join("\n");
        let diagnostics = if diagnostics.is_empty() {
            ANSI_ESCAPE_SEQUENCE
                .replace_all(stderr.trim(), "")
                .to_string()
        } else {
            diagnostics
        };
        let suffix = if sidecar.stderr_truncated {
            " (truncated)"
        } else {
            ""
        };
        return Err(format!(
            "PaddleOCR-VL full parser failed: {}{suffix}",
            diagnostics.trim()
        ));
    }
    if sidecar.stdout_truncated || sidecar.stderr_truncated {
        extraction_log(
            app,
            "WARNING: PaddleOCR-VL full-parser diagnostic output was truncated",
        );
    }
    let stderr = String::from_utf8_lossy(&sidecar.stderr);
    for line in paddle_diagnostic_lines(&stderr, 50) {
        extraction_log(app, format!("PaddleOCR parser: {line}"));
    }
    let structure_text = read_utf8_capped(&staged_structure, super::claude::MAX_STDOUT_BYTES)?;
    let structure: PaddleStructure = serde_json::from_str(&structure_text)
        .map_err(|error| format!("Failed to parse managed full-parser output: {error}"))?;
    let extraction =
        full_parser_extraction_from_structure(&structure, path, hash, baseline.as_deref())?;

    let backup = root.join(format!(".{fingerprint}-backup"));
    if backup.exists() {
        fs::remove_dir_all(&backup)
            .map_err(|error| format!("Failed to clean old parser cache backup: {error}"))?;
    }
    if target.exists() {
        fs::rename(&target, &backup)
            .map_err(|error| format!("Failed to stage prior parser cache: {error}"))?;
    }
    let staged_path = staging.keep();
    if let Err(error) = fs::rename(&staged_path, &target) {
        if backup.exists() {
            let _ = fs::rename(&backup, &target);
        }
        return Err(format!("Failed to activate full-parser cache: {error}"));
    }
    activate_paddle_full_cache(&root, &fingerprint)?;
    if backup.exists() {
        let _ = fs::remove_dir_all(&backup);
    }
    extraction_log(
        app,
        format!(
            "PaddleOCR-VL Full Parser: completed {} structured page(s) in {:.1}s",
            structure.pages.len(),
            started.elapsed().as_secs_f64()
        ),
    );
    Ok(extraction)
}

#[allow(clippy::too_many_arguments)]
fn find_command(name: &str) -> Option<crate::deps::ResolvedCommand> {
    crate::deps::resolve_command(name)
}

// ── LLM extraction: transport, verification, repair ─────────────────

/// Max output tokens for extraction calls on direct-API paths. Analysis
/// steps keep the smaller default; transcribing a whole paper needs more.
const EXTRACTION_MAX_OUTPUT_TOKENS: u32 = 32_768;

/// Bound individual LLM transcription calls so the response cap cannot
/// silently cut off a long economics paper. Two ranges run concurrently;
/// the document-level timeout remains the hard cost/latency ceiling.
const LLM_CHUNK_MAX_PAGES: usize = 8;
const LLM_CHUNK_TARGET_BASELINE_CHARS: usize = 50_000;
const LLM_EXTRACTION_CONCURRENCY: usize = 2;

/// A page's extraction is suspect when the pdftotext baseline has at least
/// this many characters but the LLM produced less than a quarter of it.
/// The floor keeps figure-heavy pages (thin text layer) from tripping it.
const SUSPECT_BASELINE_MIN_CHARS: usize = 200;

/// True when call_llm() will take a direct-API path for the preferred
/// provider — mirrors the dispatch in pipeline::claude::call_llm. Direct
/// API calls get the PDF attached to the request; CLI calls read it via
/// the Read tool.
fn provider_uses_direct_api(settings: &crate::settings::Settings) -> bool {
    match settings.preferred_provider.as_str() {
        "claude" => !settings.anthropic_api_key.is_empty(),
        "codex" => !settings.openai_api_key.is_empty(),
        "gemini" => !settings.google_api_key.is_empty(),
        // Local OpenAI-compatible servers intentionally reject the file-part
        // attachment used by the verified LLM extraction path.
        "local" => false,
        _ => false,
    }
}

/// Per-page character counts from pdftotext output (pages are separated by
/// form feeds). None when pdftotext is missing or fails — verification is
/// then skipped, not the extraction.
fn pdftotext_page_baseline(path: &Path) -> Option<Vec<usize>> {
    let bin = find_command("pdftotext")?;
    let mut command = bin.command(["-layout", path.to_str()?, "-"]);
    command.env("PATH", env::full_path());
    let output = run_bounded_output(
        command,
        "pdftotext baseline",
        std::time::Duration::from_secs(120),
        super::claude::MAX_STDOUT_BYTES,
        None,
    )
    .ok()?;
    if !output.status.success() || output.stdout_truncated {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let pages = baseline_page_lengths(&text);
    if pages.is_empty() {
        None
    } else {
        Some(pages)
    }
}

/// Split pdftotext output on form feeds and count substantive characters.
/// `-layout` can emit thousands of alignment spaces on a sparse figure page;
/// counting those as source text makes complete OCR look implausibly short.
fn baseline_page_lengths(text: &str) -> Vec<usize> {
    let mut pages: Vec<usize> = text
        .split('\u{0C}')
        .map(|page| page.chars().filter(|value| !value.is_whitespace()).count())
        .collect();
    // pdftotext terminates every page with a form feed, leaving a trailing
    // empty segment.
    if pages.last() == Some(&0) {
        pages.pop();
    }
    pages
}

fn page_marker_regex() -> Regex {
    Regex::new(r"(?i)<!--\s*page\s+(\d+)\s*-->").expect("page marker regex is invalid")
}

/// Split extracted text on `<!-- PAGE n -->` markers into (preamble,
/// page → content). Returns None when the text has no markers at all, in
/// which case completeness cannot be verified. Content under a repeated
/// marker is appended, not replaced.
fn parse_page_sections(text: &str) -> Option<(String, std::collections::BTreeMap<u32, String>)> {
    let re = page_marker_regex();
    let mut sections: std::collections::BTreeMap<u32, String> = std::collections::BTreeMap::new();
    let mut preamble = String::new();
    let mut current: Option<u32> = None;
    let mut seg_start = 0usize;
    let mut found = false;

    let push_segment = |current: Option<u32>,
                        segment: &str,
                        sections: &mut std::collections::BTreeMap<u32, String>,
                        preamble: &mut String| {
        match current {
            Some(page) => sections.entry(page).or_default().push_str(segment),
            None => preamble.push_str(segment),
        }
    };

    for cap in re.captures_iter(text) {
        let m = cap.get(0).expect("regex match has group 0");
        push_segment(
            current,
            &text[seg_start..m.start()],
            &mut sections,
            &mut preamble,
        );
        current = cap[1].parse::<u32>().ok();
        seg_start = m.end();
        found = true;
    }
    push_segment(current, &text[seg_start..], &mut sections, &mut preamble);

    if !found {
        return None;
    }
    Some((preamble.trim().to_string(), sections))
}

/// Pages that are missing entirely or came back far shorter than the
/// pdftotext baseline says they should be.
fn find_suspect_pages(
    sections: &std::collections::BTreeMap<u32, String>,
    baseline: &[usize],
) -> Vec<u32> {
    let mut suspects = Vec::new();
    for (i, &base_len) in baseline.iter().enumerate() {
        let page = (i + 1) as u32;
        match sections.get(&page) {
            None => suspects.push(page),
            Some(content) => {
                if base_len >= SUSPECT_BASELINE_MIN_CHARS && content.trim().len() < base_len / 4 {
                    suspects.push(page);
                }
            }
        }
    }
    suspects
}

/// Group sorted page numbers into contiguous (start, end) ranges.
fn group_into_ranges(pages: &[u32]) -> Vec<(u32, u32)> {
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for &p in pages {
        match ranges.last_mut() {
            Some((_, end)) if *end + 1 == p => *end = p,
            _ => ranges.push((p, p)),
        }
    }
    ranges
}

/// Reassemble the extraction from its page sections, in page order.
fn rebuild_from_sections(
    preamble: &str,
    sections: &std::collections::BTreeMap<u32, String>,
) -> String {
    let mut out = String::new();
    if !preamble.is_empty() {
        out.push_str(preamble);
        out.push_str("\n\n");
    }
    for (page, content) in sections {
        out.push_str(&format!("<!-- PAGE {page} -->\n"));
        out.push_str(content.trim());
        out.push_str("\n\n");
    }
    out.trim().to_string()
}

/// Strip a wrapping code fence (```markdown … ```) if the model emitted one.
fn strip_markdown_fence(text: &str) -> &str {
    let t = text.trim();
    if !t.starts_with("```") {
        return t;
    }
    let Some(first_newline) = t.find('\n') else {
        return t;
    };
    if t[3..first_newline]
        .trim()
        .chars()
        .any(|c| !c.is_ascii_alphanumeric())
    {
        return t;
    }
    let rest = &t[first_newline + 1..];
    match rest.rfind("```") {
        Some(end) if rest[end + 3..].trim().is_empty() => rest[..end].trim(),
        _ => t,
    }
}

/// The shared requirements block for extraction and repair prompts.
fn extraction_requirements() -> &'static str {
    "- Start each page's content with the marker <!-- PAGE n --> (1-based page number)\n\
     - Reproduce ALL text content, including abstract, all sections, footnotes, references, and appendices\n\
     - Preserve spelling, wording, and typographical errors verbatim; do not silently correct the paper\n\
     - Preserve mathematical notation using LaTeX syntax (inline $...$ and display $$...$$)\n\
     - Format tables using markdown table syntax\n\
     - Note figure/table captions and their numbers\n\
     - Preserve section numbering and hierarchy\n\
     - Drop running headers, footers, and page numbers\n\
     - Do not summarize or skip any content — output the complete text\n\
     - Do not add commentary, analysis, or annotations — just the extracted text"
}

/// How the prompt tells the model where the document is, depending on
/// whether it will be attached to the request or read via the Read tool.
fn source_line(attach: bool, prompt_path: &str) -> String {
    if attach {
        "The document is attached to this message as a PDF.".to_string()
    } else {
        format!("Read the PDF file at {prompt_path}.")
    }
}

fn llm_initial_ranges(baseline: &[usize]) -> Vec<(u32, u32)> {
    let mut ranges = Vec::new();
    let mut start = 1u32;
    let mut pages = 0usize;
    let mut chars = 0usize;
    for (index, baseline_chars) in baseline.iter().copied().enumerate() {
        if pages > 0
            && (pages >= LLM_CHUNK_MAX_PAGES
                || chars.saturating_add(baseline_chars) > LLM_CHUNK_TARGET_BASELINE_CHARS)
        {
            ranges.push((start, index as u32));
            start = index as u32 + 1;
            pages = 0;
            chars = 0;
        }
        pages += 1;
        chars = chars.saturating_add(baseline_chars);
    }
    if pages > 0 {
        ranges.push((start, baseline.len() as u32));
    }
    ranges
}

fn split_ranges(ranges: &[(u32, u32)], max_pages: u32) -> Vec<(u32, u32)> {
    let mut split = Vec::new();
    for &(start, end) in ranges {
        let mut cursor = start;
        while cursor <= end {
            let chunk_end = end.min(cursor.saturating_add(max_pages.saturating_sub(1)));
            split.push((cursor, chunk_end));
            cursor = chunk_end.saturating_add(1);
        }
    }
    split
}

/// Request one bounded, explicit page range. Every requested page marker is
/// mandatory, including blank/figure-only pages, because page completeness is
/// a contract rather than a best-effort quality note.
#[allow(clippy::too_many_arguments)]
async fn request_llm_pages(
    app: crate::emit::EventBus,
    path: PathBuf,
    prompt_path: String,
    start: u32,
    end: u32,
    attach: bool,
    timeout_secs: u64,
    read_dirs: Vec<String>,
    settings: std::sync::Arc<crate::settings::Settings>,
) -> Result<std::collections::BTreeMap<u32, String>, String> {
    let span = if start == end {
        format!("page {start}")
    } else {
        format!("pages {start} through {end}")
    };
    let prompt = format!(
        "{} Transcribe ONLY {span} to well-formatted Markdown.\n\nRequirements:\n{}",
        source_line(attach, &prompt_path),
        extraction_requirements()
    );
    let label = format!("LLM PDF extraction (pages {start}-{end})");
    let mut request = super::call::OwnedRequest::new(
        &app,
        format!("extraction-pages-{start}-{end}"),
        label,
        prompt,
        timeout_secs,
    );
    if !attach {
        request.tools = vec!["Read".to_string()];
        request.read_dirs = read_dirs;
    }
    request.pdf_attachment = attach.then_some(path);
    request.max_output_tokens = Some(EXTRACTION_MAX_OUTPUT_TOKENS);
    request.settings = settings;
    let raw = super::call::execute_text(request).await?;
    let text = strip_markdown_fence(&raw).to_string();
    let (_, sections) = parse_page_sections(&text)
        .ok_or_else(|| format!("LLM response for pages {start}-{end} contained no page markers"))?;
    let missing: Vec<u32> = (start..=end)
        .filter(|page| !sections.contains_key(page))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "LLM response for pages {start}-{end} omitted page marker(s): {}",
            missing
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(sections
        .into_iter()
        .filter(|(page, _)| *page >= start && *page <= end)
        .collect())
}

#[allow(clippy::too_many_arguments)]
async fn run_llm_ranges(
    app: &crate::emit::EventBus,
    path: &Path,
    prompt_path: &str,
    ranges: &[(u32, u32)],
    attach: bool,
    timeout_secs: u64,
    read_dirs: &[String],
    settings: std::sync::Arc<crate::settings::Settings>,
) -> Result<std::collections::BTreeMap<u32, String>, String> {
    let mut tasks = tokio::task::JoinSet::new();
    let mut next = 0usize;
    let mut sections = std::collections::BTreeMap::new();
    while next < ranges.len() && tasks.len() < LLM_EXTRACTION_CONCURRENCY {
        let (start, end) = ranges[next];
        tasks.spawn(request_llm_pages(
            app.clone(),
            path.to_path_buf(),
            prompt_path.to_string(),
            start,
            end,
            attach,
            timeout_secs,
            read_dirs.to_vec(),
            settings.clone(),
        ));
        next += 1;
    }
    while let Some(result) = tasks.join_next().await {
        if crate::commands::is_cancelled() {
            tasks.abort_all();
            return Err("Pipeline cancelled".to_string());
        }
        let range_sections =
            result.map_err(|error| format!("LLM extraction task failed: {error}"))??;
        sections.extend(range_sections);
        if next < ranges.len() {
            let (start, end) = ranges[next];
            tasks.spawn(request_llm_pages(
                app.clone(),
                path.to_path_buf(),
                prompt_path.to_string(),
                start,
                end,
                attach,
                timeout_secs,
                read_dirs.to_vec(),
                settings.clone(),
            ));
            next += 1;
        }
    }
    Ok(sections)
}

/// Extract from a PDF using an LLM (Claude, Codex, or Gemini).
///
/// Direct-API providers get the PDF attached to the request; CLI providers
/// read it with their multimodal Read tool. The output is verified against
/// a local per-page text-layer map. Calls are proactively chunked, suspicious
/// pages are retried in smaller ranges, and any remaining gap fails before
/// orientation.
async fn extract_llm(
    app: &crate::emit::EventBus,
    path: &Path,
    hash: &str,
) -> Result<ExtractionResult, String> {
    let path_str = path
        .to_str()
        .ok_or_else(|| format!("Path contains invalid UTF-8: {}", path.display()))?;
    let original_source_path = path_str.to_string();

    let settings = crate::settings::load();
    let attach = provider_uses_direct_api(&settings);
    if settings.preferred_provider == "local" {
        return Err(
            "LLM PDF extraction is not supported by local OpenAI-compatible servers. \
             Use PaddleOCR-VL or pdftotext extraction instead."
                .to_string(),
        );
    }
    let timeout = (settings.step_timeout_secs / 2).clamp(60, 600);
    let settings = std::sync::Arc::new(settings);

    // Completeness baseline (best-effort; poppler is bundled so this is
    // normally available).
    let baseline = {
        let p = path.to_path_buf();
        tokio::task::spawn_blocking(move || pdftotext_page_baseline(&p))
            .await
            .unwrap_or(None)
    }
    .ok_or_else(|| {
        "LLM extraction could not build the local page-completeness map; \
         refusing to run an unverified whole-document transcription"
            .to_string()
    })?;
    if baseline.is_empty() {
        return Err("LLM extraction found no PDF pages".to_string());
    }
    let cache_path = settings
        .reuse_pdf_extraction_cache
        .then(|| llm_extraction_cache_path(hash, &settings))
        .flatten();
    if let Some(path) = &cache_path {
        if let Ok(cached) = read_utf8_capped(path, super::claude::MAX_STDOUT_BYTES) {
            if let Some((_, sections)) = parse_page_sections(&cached) {
                if find_suspect_pages(&sections, &baseline).is_empty() {
                    extraction_log(
                        app,
                        format!(
                            "LLM extraction: reused a verified cached transcription for {hash}"
                        ),
                    );
                    let mut quality_notes = scan_math_quality(&cached);
                    quality_notes.sort();
                    quality_notes.dedup();
                    return Ok(ExtractionResult {
                        text: cached,
                        method: "llm".to_string(),
                        source_path: original_source_path,
                        paper_hash: hash.to_string(),
                        quality_notes,
                    });
                }
            }
        }
    }

    let provider_input = tempfile::Builder::new()
        .prefix("pipeline_pdf_provider_input_")
        .tempdir()
        .map_err(|error| format!("Failed to create private PDF input directory: {error}"))?;
    let source = path.to_path_buf();
    let input_root = provider_input.path().to_path_buf();
    let staged_path =
        tokio::task::spawn_blocking(move || stage_provider_input(&source, &input_root))
            .await
            .map_err(|error| format!("PDF staging task failed: {error}"))??;
    let prompt_path = staged_path.to_string_lossy().replace('\\', "/");
    // Direct APIs receive the selected PDF as an attachment and need no file
    // tool. CLI transports receive exactly one private read root.
    let read_dirs = (!attach)
        .then(|| provider_input.path().to_string_lossy().replace('\\', "/"))
        .into_iter()
        .collect::<Vec<_>>();

    let ranges = llm_initial_ranges(&baseline);
    extraction_log(
        app,
        format!(
            "LLM extraction: {} pages in {} bounded range(s), up to {} concurrent",
            baseline.len(),
            ranges.len(),
            LLM_EXTRACTION_CONCURRENCY
        ),
    );
    let mut sections = run_llm_ranges(
        app,
        &staged_path,
        &prompt_path,
        &ranges,
        attach,
        timeout,
        &read_dirs,
        settings.clone(),
    )
    .await?;

    let suspects = find_suspect_pages(&sections, &baseline);
    if !suspects.is_empty() {
        let retry_ranges = split_ranges(&group_into_ranges(&suspects), 2);
        extraction_log(
            app,
            format!(
                "Extraction verification: retrying {} suspect page(s) in {} smaller range(s)",
                suspects.len(),
                retry_ranges.len()
            ),
        );
        sections.extend(
            run_llm_ranges(
                app,
                &staged_path,
                &prompt_path,
                &retry_ranges,
                attach,
                timeout,
                &read_dirs,
                settings,
            )
            .await?,
        );
    }

    let still_suspect = find_suspect_pages(&sections, &baseline);
    if !still_suspect.is_empty() {
        return Err(format!(
            "LLM extraction remained incomplete after targeted retry on page(s): {}",
            still_suspect
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let final_text = rebuild_from_sections("", &sections);
    let mut quality_notes = Vec::new();
    quality_notes.extend(scan_math_quality(&final_text));
    if let Some(path) = cache_path {
        if let Err(error) = atomic_write_cache(&path, final_text.as_bytes()) {
            extraction_log(
                app,
                format!("WARNING: could not save verified LLM extraction cache: {error}"),
            );
        }
    }

    Ok(ExtractionResult {
        text: final_text,
        method: "llm".to_string(),
        source_path: original_source_path,
        paper_hash: hash.to_string(),
        quality_notes,
    })
}

/// Render each page of a PDF to a compact JPEG in `out_dir` using bundled
/// pdftoppm. Page images are visual references rather than archival copies:
/// 120 DPI preserves readable text and figures while avoiding the much higher
/// CPU and disk cost of lossless 150-DPI PNGs on long papers.
/// Returns the sorted file names. Used to populate the run's page-image
/// artifacts; independent of which extraction method ran.
pub struct RenderedPdfPages {
    pub names: Vec<String>,
    pub truncated: bool,
}

/// One requested PDF page rendered for the artifact explorer, plus whether a
/// following page exists. Rendering the requested page and its successor in a
/// single bounded Poppler call avoids loading the PDF into the webview and
/// gives the frontend enough information for direct page navigation.
pub struct RenderedPdfPagePreview {
    pub name: String,
    pub has_next: bool,
}

fn rendered_page_limit(requested: u32) -> u32 {
    requested.clamp(1, MAX_RENDERED_PDF_PAGES)
}

pub fn render_pdf_pages(
    pdf: &Path,
    out_dir: &Path,
    max_pages: u32,
) -> Result<RenderedPdfPages, String> {
    render_pdf_pages_with_profile(pdf, out_dir, max_pages)
}

pub fn render_pdf_page_preview(
    pdf: &Path,
    out_dir: &Path,
    page: u32,
) -> Result<RenderedPdfPagePreview, String> {
    if page == 0 || page > MAX_RENDERED_PDF_PAGES {
        return Err(format!(
            "PDF preview page must be between 1 and {MAX_RENDERED_PDF_PAGES}"
        ));
    }
    let bin = find_command("pdftoppm").ok_or("pdftoppm not found on PATH")?;
    let pdf_str = pdf
        .to_str()
        .ok_or_else(|| format!("Path contains invalid UTF-8: {}", pdf.display()))?;
    fs::create_dir_all(out_dir)
        .map_err(|e| format!("Failed to create {}: {e}", out_dir.display()))?;
    let prefix = out_dir.join("page");
    let prefix_str = prefix
        .to_str()
        .ok_or_else(|| format!("Path contains invalid UTF-8: {}", prefix.display()))?;
    let first_page = page.to_string();
    let last_page = page
        .saturating_add(1)
        .min(MAX_RENDERED_PDF_PAGES)
        .to_string();
    let args = vec![
        "-jpeg".into(),
        "-scale-to".into(),
        "2400".into(),
        "-jpegopt".into(),
        "quality=90".into(),
        "-f".into(),
        first_page,
        "-l".into(),
        last_page,
        pdf_str.into(),
        prefix_str.into(),
    ];
    let mut command = bin.command(args);
    command.env("PATH", env::full_path());
    let output =
        crate::process::run_bounded(&mut command, std::time::Duration::from_secs(60), 1_000_000)
            .map_err(|error| format!("pdftoppm PDF preview failed: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let suffix = if output.stderr_truncated {
            " (truncated)"
        } else {
            ""
        };
        return Err(format!(
            "pdftoppm failed to render PDF page {page}: {}{suffix}",
            stderr.trim()
        ));
    }

    let names: Vec<String> = fs::read_dir(out_dir)
        .map_err(|e| format!("Failed to list {}: {e}", out_dir.display()))?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| name.starts_with("page-") && name.ends_with(".jpg"))
        .collect();
    let page_number = |name: &str| {
        name.strip_prefix("page-")
            .and_then(|value| value.strip_suffix(".jpg"))
            .and_then(|value| value.parse::<u32>().ok())
    };
    let Some(name) = names
        .iter()
        .find(|name| page_number(name) == Some(page))
        .cloned()
    else {
        return Err(format!("PDF page {page} is not available"));
    };
    Ok(RenderedPdfPagePreview {
        name,
        has_next: names
            .iter()
            .any(|name| page_number(name) == page.checked_add(1)),
    })
}

fn render_pdf_pages_with_profile(
    pdf: &Path,
    out_dir: &Path,
    max_pages: u32,
) -> Result<RenderedPdfPages, String> {
    let bin = find_command("pdftoppm").ok_or("pdftoppm not found on PATH")?;
    let pdf_str = pdf
        .to_str()
        .ok_or_else(|| format!("Path contains invalid UTF-8: {}", pdf.display()))?;
    fs::create_dir_all(out_dir)
        .map_err(|e| format!("Failed to create {}: {e}", out_dir.display()))?;
    let prefix = out_dir.join("page");
    let prefix_str = prefix
        .to_str()
        .ok_or_else(|| format!("Path contains invalid UTF-8: {}", prefix.display()))?;
    let max_pages = rendered_page_limit(max_pages);
    let probe_pages = max_pages.saturating_add(1);
    let probe_pages = probe_pages.to_string();
    let extension = ".jpg";
    let args = vec![
        "-jpeg".into(),
        "-r".into(),
        "120".into(),
        "-jpegopt".into(),
        "quality=85".into(),
        "-l".into(),
        probe_pages,
        pdf_str.into(),
        prefix_str.into(),
    ];
    let mut command = bin.command(args);
    command.env("PATH", env::full_path());
    let output = match run_bounded_output(
        command,
        "pdftoppm",
        std::time::Duration::from_secs(crate::settings::load().step_timeout_secs.max(60)),
        1_000_000,
        Some(out_dir),
    ) {
        Ok(output) => output,
        Err(error) => {
            let _ = fs::remove_dir_all(out_dir);
            return Err(error);
        }
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let suffix = if output.stderr_truncated {
            " (truncated)"
        } else {
            ""
        };
        return Err(format!("pdftoppm failed: {}{suffix}", stderr.trim()));
    }
    let mut names: Vec<String> = fs::read_dir(out_dir)
        .map_err(|e| format!("Failed to list {}: {e}", out_dir.display()))?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
        .filter(|n| n.starts_with("page") && n.ends_with(extension))
        .collect();
    names.sort();
    let truncated = names.len() > max_pages as usize;
    for excess in names.iter().skip(max_pages as usize) {
        let _ = fs::remove_file(out_dir.join(excess));
    }
    names.truncate(max_pages as usize);
    if names.is_empty() {
        return Err("pdftoppm produced no page images".to_string());
    }
    Ok(RenderedPdfPages { names, truncated })
}

/// Extract from a PDF file using a supported native extractor.
/// The "llm" setting is handled separately in `extract()` since it's async.
/// `method` should be the resolved effective extractor; callers are expected
/// to translate "auto" / "llm" upstream.
fn extract_pdf_native(
    _app: &crate::emit::EventBus,
    path: &Path,
    method: &str,
) -> Result<ExtractionResult, String> {
    // This guard intentionally precedes hashing and command resolution. A
    // legacy Marker setting must never inspect or invoke a Marker executable,
    // whether it remains in ~/.pipeline or appears on PATH.
    reject_retired_pdf_extractor(method)?;
    let hash = compute_hash(path)?;

    let try_pdftotext = method == "pdftotext";

    if try_pdftotext {
        if find_command("pdftotext").is_none() {
            return Err("PDF extractor is set to 'pdftotext' but pdftotext is not installed. \
                        Install poppler (brew install poppler on macOS, or poppler-utils on Linux) or change the setting.".to_string());
        }
        let text = extract_pdftotext(path)?;
        let mut quality_notes = vec!["Text extracted via pdftotext. Equations will be garbled. \
             Technical findings should be verified against the original PDF."
            .to_string()];
        quality_notes.extend(scan_math_quality(&text));
        return Ok(ExtractionResult {
            text,
            method: "pdftotext".to_string(),
            source_path: path.to_string_lossy().to_string(),
            paper_hash: hash,
            quality_notes,
        });
    }

    Err("Unknown PDF extraction method. Check your settings.".to_string())
}

/// Heuristic scan for garbled math in extracted text.
/// Returns quality notes describing detected issues.
fn scan_math_quality(text: &str) -> Vec<String> {
    let mut notes = Vec::new();
    let total_chars = text.len() as f64;
    if total_chars < 100.0 {
        return notes;
    }

    // Count isolated single non-ASCII characters (broken symbols from PDF extraction).
    // In clean text, non-ASCII chars appear in words (é, ü) or in math blocks ($\beta$).
    // Garbled extraction produces scattered lone symbols: β  ∈  ≥  ∀
    let isolated_non_ascii: usize = text
        .split_whitespace()
        .filter(|w| {
            w.chars().count() == 1 && w.chars().next().map(|c| !c.is_ascii()).unwrap_or(false)
        })
        .count();
    let isolated_ratio = isolated_non_ascii as f64 / total_chars * 1000.0;

    // Count runs of 3+ consecutive special/math Unicode chars (U+2200..U+27FF, U+0370..U+03FF)
    // which typically indicate garbled equation blocks
    let math_unicode_runs: usize = {
        let mut count = 0;
        let mut run_len = 0;
        for ch in text.chars() {
            let is_math = matches!(ch,
                '\u{0370}'..='\u{03FF}' |  // Greek
                '\u{2100}'..='\u{214F}' |  // Letterlike symbols
                '\u{2190}'..='\u{21FF}' |  // Arrows
                '\u{2200}'..='\u{22FF}' |  // Math operators
                '\u{2300}'..='\u{23FF}' |  // Misc technical
                '\u{27C0}'..='\u{27EF}' |  // Misc math A
                '\u{2980}'..='\u{29FF}'    // Misc math B
            );
            if is_math {
                run_len += 1;
            } else {
                if run_len >= 3 {
                    count += 1;
                }
                run_len = 0;
            }
        }
        if run_len >= 3 {
            count += 1;
        }
        count
    };

    // Detect common PDF extraction artifacts: fi/fl ligatures rendered as single chars
    let ligature_count = text.matches('\u{FB01}').count() + text.matches('\u{FB02}').count();

    if isolated_ratio > 2.0 {
        notes.push(format!(
            "High density of isolated math symbols detected ({isolated_non_ascii} occurrences). \
             Equations are likely garbled from PDF extraction."
        ));
    } else if isolated_ratio > 0.5 {
        notes.push(format!(
            "Some isolated math symbols detected ({isolated_non_ascii} occurrences). \
             Some equations may be partially garbled."
        ));
    }

    if math_unicode_runs > 5 {
        notes.push(format!(
            "Found {math_unicode_runs} clusters of consecutive math Unicode characters. \
             Equation blocks are likely not rendering correctly."
        ));
    }

    if ligature_count > 10 {
        notes.push(format!(
            "Found {ligature_count} PDF ligature artifacts (fi/fl). \
             Text extraction may have minor rendering issues."
        ));
    }

    notes
}

/// Determine which PDF file to extract from a directory, if any.
/// Prefers well-known names (paper.pdf, main.pdf, manuscript.pdf), then
/// falls back to alphabetically first PDF for deterministic selection.
fn find_pdf_in_dir(dir: &Path) -> Option<PathBuf> {
    let mut pdfs: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| ext_eq(p, "pdf"))
        .collect();

    if pdfs.is_empty() {
        return None;
    }

    let preferred = ["paper.pdf", "main.pdf", "manuscript.pdf"];
    for name in &preferred {
        if let Some(p) = pdfs.iter().find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.eq_ignore_ascii_case(name))
                .unwrap_or(false)
        }) {
            return Some(p.clone());
        }
    }

    pdfs.sort();
    pdfs.into_iter().next()
}

/// Find a Word Open XML document in a project directory. Legacy `.doc`
/// binaries are intentionally unsupported because they cannot be inspected
/// safely without an external converter.
fn find_docx_in_dir(dir: &Path) -> Option<PathBuf> {
    let mut documents: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| ext_eq(path, "docx"))
        .collect();
    documents.sort();
    documents.into_iter().next()
}

fn extract_docx(path: &Path) -> Result<ExtractionResult, String> {
    let text = crate::document_bundle::extract_docx_text(path)?;
    let hash = content_hash(text.as_bytes());
    Ok(ExtractionResult {
        text,
        method: "docx".to_string(),
        source_path: path.to_string_lossy().to_string(),
        paper_hash: hash,
        quality_notes: Vec::new(),
    })
}

/// Extract text from a paper file or directory.
/// When `app` is provided and the pdf_extractor setting is "llm", uses the
/// configured LLM provider to read and extract the PDF to Markdown.
pub async fn extract(
    app: &crate::emit::EventBus,
    paper_path: &str,
    extraction_cfg: &crate::pipeline_config::ExtractionConfig,
) -> Result<ExtractionResult, String> {
    let path = PathBuf::from(paper_path);

    if !path.exists() {
        return Err(format!("File not found: {paper_path}"));
    }

    // Resolve the effective extraction method: profile override beats global.
    // An explicit "auto" or empty string falls through to whatever the global
    // setting says.
    let settings = crate::settings::load();
    let cfg_method = extraction_cfg.method.trim();
    let effective_method = if cfg_method.is_empty() || cfg_method == "auto" {
        settings.pdf_extractor.clone()
    } else {
        cfg_method.to_string()
    };
    if ext_eq(&path, "pdf") || (path.is_dir() && find_main_tex(&path).is_none()) {
        let source = if cfg_method.is_empty() || cfg_method == "auto" {
            "global setting"
        } else {
            "profile override"
        };
        extraction_log(
            app,
            format!("PDF extractor resolved to {effective_method} ({source})"),
        );
    }
    // All providers can extract PDFs now: direct APIs get the PDF attached
    // to the request, CLIs read it with their multimodal Read tool.
    let use_llm = effective_method == "llm";
    let use_paddle_full = effective_method == "paddleocr-vl-full";

    if use_paddle_full {
        let pdf_path = if path.is_dir() {
            if find_main_tex(&path).is_some() {
                None
            } else {
                find_pdf_in_dir(&path)
            }
        } else if ext_eq(&path, "pdf") {
            Some(path.clone())
        } else {
            None
        };
        if let Some(pdf) = pdf_path {
            let hash = {
                let pdf_for_hash = pdf.clone();
                tokio::task::spawn_blocking(move || compute_hash(&pdf_for_hash))
                    .await
                    .map_err(|error| format!("Hash computation failed: {error}"))??
            };
            // The sidecar's bounded subprocess runner owns the wall-clock
            // timeout. Avoid dropping its blocking task one instant earlier
            // than its process-tree cleanup.
            return extract_paddle_full(app, &pdf, &hash, &settings).await;
        }
    }

    // For LLM PDF extraction, identify the PDF path and run async
    if use_llm {
        let pdf_path = if path.is_dir() {
            // Check for LaTeX first — always prefer native extraction
            if find_main_tex(&path).is_some() {
                None // will fall through to blocking LaTeX extraction
            } else {
                find_pdf_in_dir(&path)
            }
        } else if ext_eq(&path, "pdf") {
            Some(path.clone())
        } else {
            None // .tex or other — fall through to blocking extraction
        };

        if let Some(pdf) = pdf_path {
            let hash = {
                let p = pdf.clone();
                tokio::task::spawn_blocking(move || compute_hash(&p))
                    .await
                    .map_err(|e| format!("Hash computation failed: {e}"))??
            };
            return tokio::time::timeout(
                std::time::Duration::from_secs(settings.pdf_extraction_timeout_secs),
                extract_llm(app, &pdf, &hash),
            )
            .await
            .map_err(|_| {
                format!(
                    "LLM PDF extraction exceeded the {} second document budget",
                    settings.pdf_extraction_timeout_secs
                )
            })?;
        }
    }

    let blocking_method = effective_method.clone();
    let native_app = app.clone();

    // Non-LLM paths: run blocking I/O on a separate thread
    let native_result = tokio::task::spawn_blocking(move || {
        if path.is_dir() {
            if let Some(tex_path) = find_main_tex(&path) {
                // Must be canonical: extract_latex compares it against canonicalized
                // include paths via starts_with. A non-canonical root silently defeats
                // the path-traversal check.
                let root_dir = path.canonicalize().map_err(|e| {
                    format!("Failed to canonicalize project dir {}: {e}", path.display())
                })?;
                let mut warnings = Vec::new();
                let text = extract_latex(&tex_path, 0, &root_dir, &mut warnings)?;
                if text.trim().is_empty() {
                    return Err("LaTeX extraction produced empty output".to_string());
                }
                let hash = content_hash(text.as_bytes());
                return Ok(ExtractionResult {
                    text,
                    method: "latex".to_string(),
                    source_path: tex_path.to_string_lossy().to_string(),
                    paper_hash: hash,
                    quality_notes: warnings,
                });
            }

            if let Some(pdf) = find_pdf_in_dir(&path) {
                return extract_pdf_native(&native_app, &pdf, &blocking_method);
            }

            if let Some(docx) = find_docx_in_dir(&path) {
                return extract_docx(&docx);
            }

            return Err(format!(
                "No .tex, .pdf, or .docx files found in {}",
                path.display()
            ));
        }

        if ext_eq(&path, "tex") {
            // Must be canonical: extract_latex compares it against canonicalized
            // include paths via starts_with. A non-canonical root silently defeats
            // the path-traversal check.
            let parent = path.parent().unwrap_or(Path::new("."));
            let root_dir = parent.canonicalize().map_err(|e| {
                format!(
                    "Failed to canonicalize parent dir {}: {e}",
                    parent.display()
                )
            })?;
            let mut warnings = Vec::new();
            let text = extract_latex(&path, 0, &root_dir, &mut warnings)?;
            if text.trim().is_empty() {
                return Err("LaTeX extraction produced empty output".to_string());
            }
            let hash = content_hash(text.as_bytes());
            Ok(ExtractionResult {
                text,
                method: "latex".to_string(),
                source_path: path.to_string_lossy().to_string(),
                paper_hash: hash,
                quality_notes: warnings,
            })
        } else if ext_eq(&path, "pdf") {
            extract_pdf_native(&native_app, &path, &blocking_method)
        } else if ext_eq(&path, "docx") {
            extract_docx(&path)
        } else {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("(none)");
            Err(format!("Unsupported file type: .{ext}"))
        }
    })
    .await
    .map_err(|e| format!("Extraction task failed: {e}"))?;

    native_result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_parser_cache_identity_uses_verified_content_digests() {
        let paddle = crate::engines::PaddleEnginePaths {
            server: PathBuf::from("/managed/runtime/llama-server"),
            model: PathBuf::from("/managed/models/model.gguf"),
            mmproj: PathBuf::from("/managed/models/projector.gguf"),
            integrity_sha256: "recognition-integrity-a".to_string(),
        };
        let mut paths = crate::engines::PaddleFullParserPaths {
            python: PathBuf::from("/managed/parser/python"),
            script: PathBuf::from("/managed/parser/sidecar.py"),
            model_cache: PathBuf::from("/managed/parser/cache"),
            layout_model: PathBuf::from("/managed/parser/layout"),
            paddle,
            release: "parser-release".to_string(),
            integrity_sha256: "parser-integrity-a".to_string(),
            sidecar_sha256: "sidecar-a".to_string(),
        };
        let settings = crate::settings::Settings::default();
        let original = full_parser_cache_fingerprint(&paths, &settings);

        // Mutable paths and timestamps are not cache provenance.
        paths.python = PathBuf::from("/different/path/python");
        paths.layout_model = PathBuf::from("/different/path/layout");
        assert_eq!(full_parser_cache_fingerprint(&paths, &settings), original);

        paths.integrity_sha256 = "parser-integrity-b".to_string();
        assert_ne!(full_parser_cache_fingerprint(&paths, &settings), original);
        paths.integrity_sha256 = "parser-integrity-a".to_string();
        paths.paddle.integrity_sha256 = "recognition-integrity-b".to_string();
        assert_ne!(full_parser_cache_fingerprint(&paths, &settings), original);
    }

    #[test]
    fn direct_pdf_attachment_requires_the_matching_cloud_api_key() {
        let mut settings = crate::settings::Settings {
            preferred_provider: "claude".to_string(),
            ..Default::default()
        };
        assert!(!provider_uses_direct_api(&settings));
        settings.anthropic_api_key = "configured".to_string();
        assert!(provider_uses_direct_api(&settings));

        settings.preferred_provider = "codex".to_string();
        assert!(!provider_uses_direct_api(&settings));
        settings.openai_api_key = "configured".to_string();
        assert!(provider_uses_direct_api(&settings));

        settings.preferred_provider = "gemini".to_string();
        assert!(!provider_uses_direct_api(&settings));
        settings.google_api_key = "configured".to_string();
        assert!(provider_uses_direct_api(&settings));

        settings.preferred_provider = "local".to_string();
        settings.local_api_key = "configured".to_string();
        assert!(!provider_uses_direct_api(&settings));
    }

    #[test]
    fn selected_file_is_staged_without_its_siblings() {
        let selected_dir = tempfile::tempdir().unwrap();
        let selected = selected_dir.path().join("paper.pdf");
        fs::write(&selected, b"selected").unwrap();
        fs::write(selected_dir.path().join("private-notes.txt"), b"secret").unwrap();
        let private = tempfile::tempdir().unwrap();

        let scoped = stage_selected_source(&selected, "document", private.path()).unwrap();
        let staged = scoped.source_path.unwrap();
        assert_eq!(fs::read(&staged).unwrap(), b"selected");
        assert_eq!(scoped.read_root.as_deref(), staged.parent());
        assert!(!private.path().join("source/private-notes.txt").exists());
    }

    #[test]
    fn latex_source_staging_copies_only_the_bounded_dependency_closure() {
        let parent = tempfile::tempdir().unwrap();
        let project = parent.path().join("paper");
        fs::create_dir_all(project.join("sections")).unwrap();
        fs::create_dir_all(project.join("figures")).unwrap();
        fs::write(
            project.join("main.tex"),
            "\\documentclass{localclass}\n\\input{sections/model}\n\
             \\graphicspath{{figures/}}\n\\includegraphics{irf}\n\\bibliography{refs}\n\
             % \\input{unused}\n\\input{../secret}\n",
        )
        .unwrap();
        fs::write(
            project.join("sections/model.tex"),
            "\\input{details}\nModel.",
        )
        .unwrap();
        fs::write(project.join("sections/details.tex"), "Details.").unwrap();
        fs::write(project.join("figures/irf.png"), b"image").unwrap();
        fs::write(project.join("refs.bib"), "@article{x}").unwrap();
        fs::write(project.join("localclass.cls"), "\\ProvidesClass{x}").unwrap();
        fs::write(project.join("unused.tex"), "not selected").unwrap();
        fs::write(parent.path().join("secret.tex"), "outside").unwrap();
        let private = tempfile::tempdir().unwrap();

        let scoped = stage_selected_source(&project, "document", private.path()).unwrap();
        let staged_root = scoped.source_path.unwrap();
        assert!(staged_root.is_dir());
        for relative in [
            "main.tex",
            "sections/model.tex",
            "sections/details.tex",
            "figures/irf.png",
            "refs.bib",
            "localclass.cls",
        ] {
            assert!(
                staged_root.join(relative).is_file(),
                "{relative} was omitted"
            );
        }
        assert!(!staged_root.join("unused.tex").exists());
        assert!(!private.path().join("secret.tex").exists());
        assert_eq!(scoped.read_root.as_deref(), Some(staged_root.as_path()));
    }

    #[test]
    fn retired_marker_fails_before_file_or_command_resolution() {
        let app: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
        let missing = tempfile::tempdir()
            .unwrap()
            .path()
            .join("does-not-exist.pdf");
        let error = extract_pdf_native(&app, &missing, "marker").unwrap_err();
        assert_eq!(error, MARKER_DISABLED_MESSAGE);
    }

    fn marker_page(id: &str, children: Vec<serde_json::Value>) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "block_type": "Page",
            "html": children.iter().filter_map(|child| {
                child.get("id").and_then(|value| value.as_str())
            }).map(|child_id| {
                format!("<content-ref src='{child_id}'></content-ref>")
            }).collect::<String>(),
            "polygon": [[0.0, 0.0], [612.0, 0.0], [612.0, 792.0], [0.0, 792.0]],
            "bbox": [0.0, 0.0, 612.0, 792.0],
            "children": children,
            "images": {}
        })
    }

    fn marker_block(id: &str, block_type: &str, html: &str, bbox: [f64; 4]) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "block_type": block_type,
            "html": html,
            "polygon": [
                [bbox[0], bbox[1]],
                [bbox[2], bbox[1]],
                [bbox[2], bbox[3]],
                [bbox[0], bbox[3]]
            ],
            "bbox": bbox,
            "children": null,
            "images": null
        })
    }

    #[test]
    fn marker_normalization_separates_footnotes_pages_and_repeated_margins() {
        let first_page = marker_page(
            "/page/0/Page/0",
            vec![
                marker_block(
                    "/page/0/PageHeader/0",
                    "PageHeader",
                    "<p>Running title</p>",
                    [72.0, 20.0, 540.0, 40.0],
                ),
                marker_block(
                    "/page/0/Text/1",
                    "Text",
                    "<p>The main argument continues.<sup>1</sup></p>",
                    [72.0, 120.0, 540.0, 220.0],
                ),
                marker_block(
                    "/page/0/Text/2",
                    "Text",
                    "<p>1 This qualification belongs in a footnote.</p>",
                    [72.0, 680.0, 540.0, 720.0],
                ),
                marker_block(
                    "/page/0/Footnote/3",
                    "Footnote",
                    "<p><sup>2</sup> Marker recognized this note.</p>",
                    [72.0, 725.0, 540.0, 750.0],
                ),
                marker_block(
                    "/page/0/PageFooter/4",
                    "PageFooter",
                    "<p>7</p>",
                    [300.0, 770.0, 312.0, 785.0],
                ),
            ],
        );
        let second_page = marker_page(
            "/page/1/Page/0",
            vec![marker_block(
                "/page/1/Text/0",
                "Text",
                "<p>The main argument resumes on the next page.</p>",
                [72.0, 80.0, 540.0, 140.0],
            )],
        );
        let raw = serde_json::json!({
            "block_type": "Document",
            "children": [first_page, second_page]
        })
        .to_string();
        let output_dir = tempfile::tempdir().unwrap();
        let normalized = normalize_marker_json(&raw, output_dir.path()).unwrap();

        assert!(normalized.text.contains("<!-- PAGE 1 -->"));
        assert!(normalized.text.contains("<!-- PAGE 2 -->"));
        assert!(!normalized.text.contains("Running title"));
        assert!(!normalized.text.contains("<p>7</p>"));
        assert!(normalized.text.contains("BEGIN POSSIBLE_FOOTNOTE page=1"));
        assert!(normalized.text.contains("BEGIN FOOTNOTES page=1"));
        assert!(
            normalized.text.find("BEGIN FOOTNOTES page=1").unwrap()
                < normalized.text.find("<!-- PAGE 2 -->").unwrap()
        );
        assert!(normalized
            .quality_notes
            .iter()
            .any(|note| note.contains("possible footnotes")));

        let structure = read_utf8_capped(
            &output_dir.path().join(MARKER_STRUCTURE_FILE),
            super::super::claude::MAX_STDOUT_BYTES,
        )
        .unwrap();
        let structure: MarkerStructure = serde_json::from_str(&structure).unwrap();
        assert_eq!(structure.pages.len(), 2);
        assert!(structure.pages[0]
            .blocks
            .iter()
            .any(|block| block.role == "possible_footnote"));
        assert!(structure.pages[0]
            .blocks
            .iter()
            .any(|block| block.role == "footnote"));
        assert!(structure.pages[0]
            .blocks
            .iter()
            .any(|block| block.role == "page_header"));
    }

    #[test]
    fn marker_bottom_text_without_matching_superscript_remains_body_text() {
        let page = marker_page(
            "/page/0/Page/0",
            vec![
                marker_block(
                    "/page/0/Text/0",
                    "Text",
                    "<p>Ordinary body text.</p>",
                    [72.0, 120.0, 540.0, 220.0],
                ),
                marker_block(
                    "/page/0/Text/1",
                    "Text",
                    "<p>1 A numbered body paragraph near the page bottom.</p>",
                    [72.0, 680.0, 540.0, 720.0],
                ),
            ],
        );
        let raw = serde_json::json!({
            "block_type": "Document",
            "children": [page]
        })
        .to_string();
        let output_dir = tempfile::tempdir().unwrap();
        normalize_marker_json(&raw, output_dir.path()).unwrap();
        let structure = serde_json::from_str::<MarkerStructure>(
            &read_utf8_capped(
                &output_dir.path().join(MARKER_STRUCTURE_FILE),
                super::super::claude::MAX_STDOUT_BYTES,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(structure.pages[0].blocks[1].role, "body");
    }

    #[test]
    fn marker_json_images_are_externalized_and_linked() {
        let mut figure = marker_block(
            "/page/0/Figure/0",
            "Figure",
            "<figure></figure>",
            [72.0, 120.0, 540.0, 420.0],
        );
        figure["images"] = serde_json::json!({
            "/page/0/Figure/0": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
        });
        let raw = serde_json::json!({
            "block_type": "Document",
            "children": [marker_page("/page/0/Page/0", vec![figure])]
        })
        .to_string();
        let output_dir = tempfile::tempdir().unwrap();
        normalize_marker_json(&raw, output_dir.path()).unwrap();
        let structure = serde_json::from_str::<MarkerStructure>(
            &read_utf8_capped(
                &output_dir.path().join(MARKER_STRUCTURE_FILE),
                super::super::claude::MAX_STDOUT_BYTES,
            )
            .unwrap(),
        )
        .unwrap();
        let image_files = &structure.pages[0].blocks[0].image_files;
        assert_eq!(image_files.len(), 1);
        assert!(image_files[0].ends_with(".png"));
        assert!(output_dir.path().join(&image_files[0]).is_file());
    }

    #[test]
    fn paddle_readiness_requires_the_managed_model_alias() {
        let expected = serde_json::json!({
            "data": [{"id": PADDLE_MODEL_ALIAS, "object": "model"}]
        });
        let wrong = serde_json::json!({
            "data": [{"id": "unrelated-server", "object": "model"}]
        });
        assert!(paddle_model_list_has_alias(&expected));
        assert!(!paddle_model_list_has_alias(&wrong));
        assert!(!paddle_model_list_has_alias(&serde_json::json!({})));
    }

    #[test]
    fn paddle_readiness_polling_has_a_bounded_backoff() {
        assert!(PADDLE_READINESS_POLL_INTERVAL >= std::time::Duration::from_millis(100));
        assert!(PADDLE_READINESS_POLL_INTERVAL <= std::time::Duration::from_millis(250));
    }

    #[test]
    fn paddle_server_credentials_are_high_entropy_hex() {
        let token = paddle_server_token().unwrap();
        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn full_parser_structure_drives_compatibility_text_and_page_verification() {
        let structure = PaddleStructure {
            schema_version: PADDLE_STRUCTURE_SCHEMA,
            parser: "paddleocr-vl-full".to_string(),
            parser_version: "3.7.0".to_string(),
            settings: serde_json::json!({ "merge_tables": true }),
            quality_notes: vec!["fixture note".to_string()],
            pages: vec![
                PaddleStructuredPage {
                    number: 1,
                    markdown: "# Introduction\n\n![Plot](assets/page-0001-plot.png)".to_string(),
                    source_text_chars: None,
                    width: Some(1200),
                    height: Some(1600),
                    blocks: Vec::new(),
                },
                PaddleStructuredPage {
                    number: 2,
                    markdown: "## Results\n\nThe estimate is positive.".to_string(),
                    source_text_chars: None,
                    width: Some(1200),
                    height: Some(1600),
                    blocks: Vec::new(),
                },
            ],
        };
        let path = Path::new("/tmp/paper.pdf");
        let extraction = full_parser_extraction_from_structure(
            &structure,
            path,
            "0123456789abcdef",
            Some(&[0, 0]),
        )
        .unwrap();
        assert_eq!(extraction.method, "paddleocr-vl-full");
        assert!(extraction.text.contains("<!-- PAGE 2 -->"));
        assert!(extraction
            .text
            .contains("../artifacts/figures/page-0001-plot.png"));
        assert!(extraction
            .quality_notes
            .contains(&"fixture note".to_string()));

        let mut moved_page = structure.clone();
        moved_page.pages[0].markdown.clear();
        moved_page.pages[0].source_text_chars = Some(1_000);
        let moved_extraction = full_parser_extraction_from_structure(
            &moved_page,
            path,
            "0123456789abcdef",
            Some(&[1_000, 0]),
        )
        .unwrap();
        assert!(moved_extraction
            .quality_notes
            .iter()
            .any(|note| note.contains("moved all recognized content from page(s) 1")));

        let mut incomplete_page = structure.clone();
        incomplete_page.pages[0].source_text_chars = Some(10);
        let incomplete_error = full_parser_extraction_from_structure(
            &incomplete_page,
            path,
            "0123456789abcdef",
            Some(&[1_000, 0]),
        )
        .unwrap_err();
        assert!(incomplete_error.contains("recognized 10 substantive characters"));

        let mut missing_page = structure;
        missing_page.pages[1].number = 3;
        assert!(full_parser_extraction_from_structure(
            &missing_page,
            path,
            "0123456789abcdef",
            Some(&[0, 0]),
        )
        .unwrap_err()
        .contains("page 3 where page 2 was expected"));
    }

    #[test]
    fn paddle_diagnostics_remove_terminal_codes_and_expected_dependency_warnings() {
        let stderr = concat!(
            "\u{1b}[32mCreating model: ('PP-DocLayoutV3', None, None)\u{1b}[0m\n",
            "/managed/extension_utils.py:718: UserWarning: No ccache found.\n",
            "warnings.warn(warning_message)\n",
            "/managed/predictor.py:545: UserWarning: 'llama-cpp-server' does not support `min_pixels`.\n",
            "warnings.warn(\n",
            "Page 39: retrying incomplete layout recognition.\n",
            "Page 39: retrying incomplete layout recognition.\n",
        );
        assert_eq!(
            paddle_diagnostic_lines(stderr, 50),
            vec![
                "Creating model: ('PP-DocLayoutV3', None, None)",
                "Page 39: retrying incomplete layout recognition.",
            ]
        );
    }

    #[test]
    fn full_parser_image_labels_use_captions_and_disambiguate_repeated_numbers() {
        let block = |id: &str, text: &str, order: u32, assets: &[&str]| PaddleStructuredBlock {
            block_id: id.to_string(),
            role: if assets.is_empty() {
                "figure_title"
            } else {
                "chart"
            }
            .to_string(),
            block_label: if assets.is_empty() {
                "figure_title"
            } else {
                "chart"
            }
            .to_string(),
            markdown: text.to_string(),
            text: text.to_string(),
            boundary: None,
            note_marker: None,
            order: Some(order),
            bbox: Vec::new(),
            polygon: Vec::new(),
            confidence: None,
            asset_files: assets.iter().map(|asset| asset.to_string()).collect(),
            raw: serde_json::Value::Null,
        };
        let page = |number: u32, blocks: Vec<PaddleStructuredBlock>| PaddleStructuredPage {
            number,
            markdown: String::new(),
            source_text_chars: None,
            width: None,
            height: None,
            blocks,
        };
        let structure = PaddleStructure {
            schema_version: PADDLE_STRUCTURE_SCHEMA,
            parser: "paddleocr-vl-full".to_string(),
            parser_version: "3.7.0".to_string(),
            settings: serde_json::Value::Null,
            quality_notes: Vec::new(),
            pages: vec![
                page(
                    2,
                    vec![
                        block("caption-1a", "Figure 1: Baseline", 1, &[]),
                        block("image-1a", "", 2, &["assets/page-0002-plot.jpg"]),
                    ],
                ),
                page(
                    3,
                    vec![
                        block("caption-1b", "Figure 1: Appendix version", 1, &[]),
                        block("image-1b", "", 2, &["assets/page-0003-plot.jpg"]),
                    ],
                ),
                page(
                    4,
                    vec![
                        block("caption-2", "Figure 2: Responses", 1, &[]),
                        block(
                            "image-2",
                            "",
                            2,
                            &[
                                "assets/page-0004-panel-a.png",
                                "assets/page-0004-panel-b.png",
                            ],
                        ),
                    ],
                ),
                page(
                    5,
                    vec![
                        PaddleStructuredBlock {
                            role: "text".to_string(),
                            block_label: "text".to_string(),
                            ..block(
                                "prose-reference",
                                "Figure 9 discusses an earlier result.",
                                1,
                                &[],
                            )
                        },
                        block(
                            "image-before-caption",
                            "",
                            2,
                            &["assets/page-0005-plot.webp"],
                        ),
                        block("caption-below", "Figure 3: Caption below image", 3, &[]),
                    ],
                ),
            ],
        };
        let images = [
            "/cache/page-0002-plot.jpg",
            "/cache/page-0003-plot.jpg",
            "/cache/page-0004-panel-a.png",
            "/cache/page-0004-panel-b.png",
            "/cache/page-0005-plot.webp",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect();
        let inventory = paddle_full_image_inventory_from(&structure, images);
        assert_eq!(inventory.disambiguated_labels, 1);
        assert_eq!(
            inventory
                .artifacts
                .iter()
                .map(|artifact| artifact.display_name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "figure_1_page_0002.jpg",
                "figure_1_page_0003.jpg",
                "figure_2_1.png",
                "figure_2_2.png",
                "figure_3.webp",
            ]
        );
    }

    #[test]
    fn full_parser_uses_richer_blocks_when_page_markdown_is_partial() {
        let page = PaddleStructuredPage {
            number: 1,
            markdown: "References".to_string(),
            source_text_chars: None,
            width: None,
            height: None,
            blocks: vec![PaddleStructuredBlock {
                block_id: "reference-block".to_string(),
                role: "reference_content".to_string(),
                block_label: "reference_content".to_string(),
                markdown: "A complete bibliography entry with substantially more recognized text."
                    .repeat(8),
                text: String::new(),
                boundary: None,
                note_marker: None,
                order: Some(1),
                bbox: Vec::new(),
                polygon: Vec::new(),
                confidence: Some(0.9),
                asset_files: Vec::new(),
                raw: serde_json::Value::Null,
            }],
        };
        let rendered = render_paddle_full_page(&page);
        assert!(rendered.contains("complete bibliography entry"));
        assert!(rendered.len() > page.markdown.len() * 2);
    }

    #[test]
    fn rendered_page_limit_matches_documented_cap() {
        assert_eq!(rendered_page_limit(0), 1);
        assert_eq!(rendered_page_limit(50), 50);
        assert_eq!(rendered_page_limit(500), MAX_RENDERED_PDF_PAGES);
        assert_eq!(MAX_RENDERED_PDF_PAGES, 300);
    }

    #[test]
    fn pdf_artifact_preview_renders_a_jpeg_when_poppler_is_available() {
        if find_command("pdftoppm").is_none() {
            return;
        }
        let stream = "BT /F1 18 Tf 72 720 Td (Pipeline PDF preview) Tj ET\n";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_string(),
            format!("<< /Length {} >>\nstream\n{stream}endstream", stream.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        let mut pdf = "%PDF-1.4\n".to_string();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
        }
        let xref = pdf.len();
        pdf.push_str(&format!(
            "xref\n0 {}\n0000000000 65535 f \n",
            objects.len() + 1
        ));
        for offset in offsets {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        ));

        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("figure.pdf");
        let output = temp.path().join("rendered");
        fs::write(&input, pdf).unwrap();
        let preview = render_pdf_page_preview(&input, &output, 1).unwrap();
        assert!(!preview.has_next);
        let jpeg = fs::read(output.join(preview.name)).unwrap();
        assert!(jpeg.starts_with(&[0xff, 0xd8, 0xff]));
    }

    #[test]
    #[cfg(unix)]
    fn bounded_subprocess_times_out_promptly() {
        let mut command = StdCommand::new("sleep");
        command.arg("5");
        let started = std::time::Instant::now();
        let error = run_bounded_output(
            command,
            "sleep test",
            std::time::Duration::from_millis(20),
            1024,
            None,
        )
        .unwrap_err();
        assert!(error.contains("timed out"));
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }

    #[test]
    fn file_hash_is_streamed_and_matches_sha256() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut file, b"abc").unwrap();
        assert_eq!(compute_hash(file.path()).unwrap(), "ba7816bf8f01cfea");
    }

    // ── scan_math_quality ──────────────────────────────────────────

    #[test]
    fn scan_clean_text_no_warnings() {
        let text = "This is a normal academic paper with standard English text. \
                    The results are reported in Table 1. We find a significant effect.";
        let notes = scan_math_quality(text);
        assert!(notes.is_empty());
    }

    #[test]
    fn scan_short_text_skipped() {
        let notes = scan_math_quality("short");
        assert!(notes.is_empty());
    }

    #[test]
    fn scan_isolated_math_symbols() {
        // Simulate garbled PDF extraction with many isolated non-ASCII chars
        let mut text = "Normal text here. ".repeat(10);
        for _ in 0..50 {
            text.push_str("β ");
        }
        let notes = scan_math_quality(&text);
        assert!(!notes.is_empty());
        assert!(notes[0].contains("isolated math symbols"));
    }

    #[test]
    fn scan_math_unicode_runs() {
        // 6+ runs of 3+ consecutive math Unicode characters
        let mut text = "Normal text. ".repeat(20);
        for _ in 0..8 {
            text.push_str("∀∈≥ normal text ");
        }
        let notes = scan_math_quality(&text);
        assert!(notes.iter().any(|n| n.contains("clusters")));
    }

    #[test]
    fn scan_ligature_artifacts() {
        let mut text = "Normal text. ".repeat(20);
        // 15 fi ligatures
        for _ in 0..15 {
            text.push_str("the \u{FB01}rst ");
        }
        let notes = scan_math_quality(&text);
        assert!(notes.iter().any(|n| n.contains("ligature")));
    }

    // ── LLM extraction verification helpers ───────────────────────

    #[test]
    fn baseline_splits_on_form_feeds_and_drops_trailing_empty() {
        let text = "page   one text\u{0C}page two\u{0C}";
        let pages = baseline_page_lengths(text);
        assert_eq!(pages, vec!["pageonetext".len(), "pagetwo".len()]);
    }

    #[test]
    fn parse_sections_returns_none_without_markers() {
        assert!(parse_page_sections("just some markdown, no markers").is_none());
    }

    #[test]
    fn parse_sections_splits_preamble_and_pages() {
        let text = "intro\n<!-- PAGE 1 -->\nfirst page\n<!-- page 2 -->\nsecond page";
        let (preamble, sections) = parse_page_sections(text).unwrap();
        assert_eq!(preamble, "intro");
        assert_eq!(sections.len(), 2);
        assert!(sections[&1].contains("first page"));
        // Marker matching is case-insensitive.
        assert!(sections[&2].contains("second page"));
    }

    #[test]
    fn parse_sections_appends_repeated_markers() {
        let text = "<!-- PAGE 1 -->\nstart\n<!-- PAGE 1 -->\ncontinued";
        let (_, sections) = parse_page_sections(text).unwrap();
        assert!(sections[&1].contains("start"));
        assert!(sections[&1].contains("continued"));
    }

    #[test]
    fn suspects_flags_missing_and_short_pages() {
        let mut sections = std::collections::BTreeMap::new();
        sections.insert(1, "x".repeat(500));
        sections.insert(3, "tiny".to_string());
        sections.insert(4, "y".repeat(50));
        // page 2 missing; page 3 short vs a 1000-char baseline;
        // page 4 short but baseline below the 200-char floor → not suspect.
        let baseline = vec![500, 800, 1000, 150];
        assert_eq!(find_suspect_pages(&sections, &baseline), vec![2, 3]);
    }

    #[test]
    fn ranges_group_contiguous_pages() {
        assert_eq!(
            group_into_ranges(&[2, 3, 4, 7, 9, 10]),
            vec![(2, 4), (7, 7), (9, 10)]
        );
        assert!(group_into_ranges(&[]).is_empty());
    }

    #[test]
    fn llm_initial_ranges_bound_pages_and_expected_output() {
        assert_eq!(
            llm_initial_ranges(&[1_000; 20]),
            vec![(1, 8), (9, 16), (17, 20)]
        );
        assert_eq!(
            llm_initial_ranges(&[30_000, 30_000, 1_000]),
            vec![(1, 1), (2, 3)]
        );
    }

    #[test]
    fn retry_ranges_are_split_into_small_requests() {
        assert_eq!(
            split_ranges(&[(2, 6), (10, 10)], 2),
            vec![(2, 3), (4, 5), (6, 6), (10, 10)]
        );
    }

    #[test]
    fn extraction_prompt_preserves_typos_as_evidence() {
        assert!(extraction_requirements().contains("typographical errors verbatim"));
    }

    #[test]
    fn rebuild_orders_pages_and_keeps_preamble() {
        let mut sections = std::collections::BTreeMap::new();
        sections.insert(2, "two".to_string());
        sections.insert(1, "one".to_string());
        let out = rebuild_from_sections("title", &sections);
        assert!(out.starts_with("title"));
        let one_pos = out.find("<!-- PAGE 1 -->").unwrap();
        let two_pos = out.find("<!-- PAGE 2 -->").unwrap();
        assert!(one_pos < two_pos);
        assert!(out.contains("one") && out.contains("two"));
    }

    #[test]
    fn fence_stripping() {
        assert_eq!(strip_markdown_fence("```markdown\n# Title\n```"), "# Title");
        assert_eq!(strip_markdown_fence("```\ntext\n```"), "text");
        // Not a wrapping fence — inner fences stay untouched.
        let mixed = "prose\n```python\ncode\n```\nmore";
        assert_eq!(strip_markdown_fence(mixed), mixed);
        assert_eq!(strip_markdown_fence("plain"), "plain");
    }
}

// ── Generalized ingest modes ────────────────────────────────────────
//
// "folder" and "none" input modes for non-document workflows. Folder mode
// records an inventory of the directory as the context text — file contents
// are never inlined; steps open files on demand with the Read tool. None
// mode runs the pipeline from the prompts alone.

/// Directories that never belong in an inventory.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "__pycache__",
    ".venv",
    "venv",
    "dist",
    "build",
];
/// Inventory size cap — beyond this the listing notes the truncation.
const MAX_INVENTORY_FILES: usize = 2000;

/// Resolve the effective input mode from the profile setting and the path.
/// A directory path implies folder mode even if the profile says document,
/// so picking a folder in the UI "just works" with any profile.
pub fn effective_input_mode(configured: &str, input_path: &str) -> &'static str {
    match configured {
        "none" => "none",
        "folder" => "folder",
        _ => {
            if !input_path.is_empty() && Path::new(input_path).is_dir() {
                "folder"
            } else {
                "document"
            }
        }
    }
}

/// Build a file-inventory context for a folder input.
pub fn ingest_folder(root: &str) -> Result<ExtractionResult, String> {
    let root_path = PathBuf::from(root);
    if !root_path.is_dir() {
        return Err(format!("Not a directory: {root}"));
    }

    let canonical_root = root_path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve input folder {root}: {e}"))?;

    let mut files: Vec<(String, u64, PathBuf)> = Vec::new();
    let mut stack = vec![canonical_root.clone()];
    let mut visited_dirs = std::collections::HashSet::new();
    let mut examined_entries = 0usize;
    let mut examined_dirs = 0usize;
    let mut skipped_special = 0usize;
    let mut truncated = false;
    while let Some(dir) = stack.pop() {
        if crate::commands::is_cancelled() {
            return Err("Pipeline cancelled".to_string());
        }
        let Ok(canonical_dir) = dir.canonicalize() else {
            continue;
        };
        if !canonical_dir.starts_with(&canonical_root)
            || !visited_dirs.insert(canonical_dir.clone())
        {
            continue;
        }
        examined_dirs += 1;
        if examined_dirs > MAX_INVENTORY_DIRS {
            truncated = true;
            break;
        }
        let entries = match fs::read_dir(&canonical_dir) {
            Ok(e) => e,
            Err(_) => continue, // unreadable subdir: skip, don't fail the run
        };
        for entry in entries.flatten() {
            examined_entries += 1;
            if examined_entries > MAX_INVENTORY_ENTRIES {
                truncated = true;
                stack.clear();
                break;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            // Never follow directory/file symlinks from an inventory. Apart
            // from escaping the selected root, a directory symlink can form a
            // cycle and a file symlink can be swapped for a special file.
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push(path);
                }
            } else if file_type.is_file() {
                let Ok(meta) = fs::symlink_metadata(&path) else {
                    continue;
                };
                if !meta.file_type().is_file() {
                    continue;
                }
                if files.len() >= MAX_INVENTORY_FILES {
                    truncated = true;
                    continue;
                }
                let rel = path
                    .strip_prefix(&canonical_root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push((rel, meta.len(), path));
            } else {
                skipped_special += 1;
            }
        }
    }
    files.sort();

    let root_display = root.replace('\\', "/");
    let mut text = format!(
        "# Input folder inventory\n\nRoot: {root_display}\n\n{} files. File contents are NOT included here — use the Read tool with paths under the root to open any file you need.\n\n| File | Bytes |\n|---|---|\n",
        files.len()
    );
    for (rel, size, _) in &files {
        text.push_str(&format!("| {rel} | {size} |\n"));
    }
    if truncated {
        text.push_str(&format!(
            "\n> Inventory truncated by its safety limits (maximum {MAX_INVENTORY_FILES} files, {MAX_INVENTORY_ENTRIES} entries, and {MAX_INVENTORY_DIRS} directories).\n"
        ));
    }
    if skipped_special > 0 {
        text.push_str(&format!(
            "\n> Skipped {skipped_special} non-regular filesystem entr{} (for example, a socket or named pipe).\n",
            if skipped_special == 1 { "y" } else { "ies" }
        ));
    }

    let mut hasher = Sha256::new();
    let mut chunk = [0u8; 64 * 1024];
    let mut hashed_bytes = 0u64;
    for (rel, size, path) in &files {
        if hashed_bytes.saturating_add(*size) > MAX_INVENTORY_HASH_BYTES {
            return Err(format!(
                "Input folder exceeds the {} GB hashing safety limit",
                MAX_INVENTORY_HASH_BYTES / 1024 / 1024 / 1024
            ));
        }
        hasher.update(rel.as_bytes());
        hasher.update([0]);
        hasher.update(size.to_le_bytes());
        let mut file = open_regular_file(path)
            .map_err(|e| format!("Failed to hash {}: {e}", path.display()))?;
        loop {
            if crate::commands::is_cancelled() {
                return Err("Pipeline cancelled".to_string());
            }
            let count = std::io::Read::read(&mut file, &mut chunk)
                .map_err(|e| format!("Failed to hash {}: {e}", path.display()))?;
            if count == 0 {
                break;
            }
            hashed_bytes = hashed_bytes.saturating_add(count as u64);
            if hashed_bytes > MAX_INVENTORY_HASH_BYTES {
                return Err(format!(
                    "Input folder exceeds the {} GB hashing safety limit",
                    MAX_INVENTORY_HASH_BYTES / 1024 / 1024 / 1024
                ));
            }
            hasher.update(&chunk[..count]);
        }
    }
    let hash = format!("{:x}", hasher.finalize())[..16].to_string();
    Ok(ExtractionResult {
        text,
        method: "folder".to_string(),
        source_path: root.to_string(),
        paper_hash: hash,
        quality_notes: vec![],
    })
}

/// Async boundary for folder inventory/hash work. The blocking implementation
/// checks the global cancellation flag between entries and read chunks.
pub async fn ingest_folder_async(root: &str) -> Result<ExtractionResult, String> {
    let root = root.to_string();
    tokio::task::spawn_blocking(move || ingest_folder(&root))
        .await
        .map_err(|e| format!("Folder ingestion task failed: {e}"))?
}

/// Context for a workflow that takes no input at all.
pub fn ingest_none() -> ExtractionResult {
    let stamp = chrono::Local::now().to_rfc3339();
    let hash = format!("{:x}", Sha256::digest(stamp.as_bytes()))[..16].to_string();
    ExtractionResult {
        text: "(This workflow runs from its step prompts alone; there is no input document.)"
            .to_string(),
        method: "none".to_string(),
        source_path: String::new(),
        paper_hash: hash,
        quality_notes: vec![],
    }
}

#[cfg(test)]
mod ingest_tests {
    use super::*;

    #[test]
    fn effective_mode_resolution() {
        assert_eq!(effective_input_mode("none", ""), "none");
        assert_eq!(effective_input_mode("folder", "/x"), "folder");
        assert_eq!(
            effective_input_mode("", "/definitely/not/a/dir.pdf"),
            "document"
        );
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            effective_input_mode("", dir.path().to_str().unwrap()),
            "folder"
        );
    }

    #[test]
    fn folder_inventory_lists_files_and_skips_junk() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("main.py"), "print(1)").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/data.csv"), "a,b\n1,2").unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".git/config"), "x").unwrap();
        fs::create_dir(dir.path().join("node_modules")).unwrap();
        fs::write(dir.path().join("node_modules/junk.js"), "x").unwrap();

        let result = ingest_folder(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(result.method, "folder");
        assert!(result.text.contains("main.py"));
        assert!(result.text.contains("sub/data.csv"));
        assert!(!result.text.contains("junk.js"));
        assert!(!result.text.contains(".git/config"));
        assert_eq!(result.paper_hash.len(), 16);
    }

    #[test]
    fn folder_hash_changes_when_same_size_content_changes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("data.txt");
        fs::write(&file, "alpha").unwrap();
        let first = ingest_folder(dir.path().to_str().unwrap())
            .unwrap()
            .paper_hash;
        fs::write(&file, "bravo").unwrap();
        let second = ingest_folder(dir.path().to_str().unwrap())
            .unwrap()
            .paper_hash;
        assert_ne!(first, second);
    }

    #[test]
    #[cfg(unix)]
    fn folder_inventory_skips_symlink_cycles_and_special_files() {
        use std::os::unix::ffi::OsStrExt as _;

        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("regular.txt"), "safe").unwrap();
        std::os::unix::fs::symlink(dir.path(), dir.path().join("cycle")).unwrap();

        let fifo = dir.path().join("blocked.pipe");
        let fifo_c = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);

        let result = ingest_folder(dir.path().to_str().unwrap()).unwrap();
        assert!(result.text.contains("regular.txt"));
        assert!(!result.text.contains("cycle/"));
        assert!(!result.text.contains("blocked.pipe |"));
        assert!(result
            .text
            .contains("Skipped 1 non-regular filesystem entry"));
    }

    #[test]
    fn none_mode_produces_placeholder_context() {
        let result = ingest_none();
        assert_eq!(result.method, "none");
        assert!(result.source_path.is_empty());
        assert_eq!(result.paper_hash.len(), 16);
    }
}
