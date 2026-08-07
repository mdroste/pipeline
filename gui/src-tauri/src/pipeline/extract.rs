use crate::env;
use crate::models::ExtractionResult;
#[cfg(test)]
use base64::Engine as _;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

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
const PADDLE_REGION_WARNING: &str = "PaddleOCR-VL warning: this visual region remained degenerate after adaptive subdivision and was not transcribed. Consult the rendered source page.";
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
const PADDLE_STRUCTURE_SCHEMA: u32 = 1;
const PADDLE_STRUCTURE_FILE: &str = "pipeline-paddle-structure.json";

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
fn find_main_tex(dir: &Path) -> Option<PathBuf> {
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
    pub(crate) quality_notes: Vec<String>,
    #[serde(default)]
    pub(crate) pages: Vec<PaddleStructuredPage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PaddleStructuredPage {
    pub(crate) number: u32,
    #[serde(default)]
    pub(crate) blocks: Vec<PaddleStructuredBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PaddleStructuredBlock {
    pub(crate) block_id: String,
    pub(crate) role: String,
    #[serde(default)]
    pub(crate) markdown: String,
    #[serde(default)]
    pub(crate) text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) boundary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) note_marker: Option<String>,
}

#[cfg(test)]
#[derive(Debug)]
struct MarkerExtraction {
    text: String,
    quality_notes: Vec<String>,
}

#[derive(Debug)]
struct PaddleExtraction {
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
    if structure.schema_version != PADDLE_STRUCTURE_SCHEMA {
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

fn executable_identity(path: &Path) -> String {
    let metadata = fs::metadata(path).ok();
    let len = metadata.as_ref().map(|value| value.len()).unwrap_or(0);
    let modified = metadata
        .and_then(|value| value.modified().ok())
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_secs())
        .unwrap_or(0);
    format!("{}:{len}:{modified}", path.display())
}

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
const MAX_PADDLE_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
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

fn read_image_data_url(path: &Path) -> Result<String, String> {
    use base64::Engine as _;
    use std::io::Read as _;
    const MAX_PAGE_IMAGE_BYTES: usize = 25_000_000;
    let mut file = open_regular_file(path)?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_PAGE_IMAGE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Failed to read rendered PDF page: {error}"))?;
    if bytes.len() > MAX_PAGE_IMAGE_BYTES {
        return Err(format!(
            "Rendered PDF page exceeds the {} MB safety limit",
            MAX_PAGE_IMAGE_BYTES / 1_000_000
        ));
    }
    let media_type = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        "image/jpeg"
    } else {
        return Err("Rendered PDF page is not a supported PNG or JPEG image".to_string());
    };
    Ok(format!(
        "data:{media_type};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn paddle_response_text(value: &serde_json::Value) -> Result<String, String> {
    if let Some(message) = value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(|message| message.as_str())
    {
        return Err(format!("PaddleOCR-VL inference failed: {message}"));
    }
    let choice = value
        .get("choices")
        .and_then(|choices| choices.get(0))
        .ok_or("PaddleOCR-VL returned an invalid response")?;
    if let Some(reason) = choice
        .get("finish_reason")
        .and_then(|reason| reason.as_str())
    {
        if reason != "stop" {
            return Err(format!(
                "PaddleOCR-VL stopped before completing the page (finish_reason={reason})"
            ));
        }
    }
    let content = choice
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .ok_or("PaddleOCR-VL returned an invalid response")?;
    let content = strip_markdown_fence(content).trim().to_string();
    if content.is_empty() {
        return Err("PaddleOCR-VL returned empty page output".to_string());
    }
    Ok(content)
}

async fn paddle_extract_page(
    base_url: &str,
    api_key: &str,
    image_path: &Path,
    timeout: std::time::Duration,
    max_output_tokens: u32,
) -> Result<String, String> {
    let image_url = read_image_data_url(image_path)?;
    let body = serde_json::json!({
        "model": PADDLE_MODEL_ALIAS,
        "temperature": 0,
        "max_tokens": max_output_tokens,
        "messages": [{
            "role": "user",
            "content": [
                { "type": "text", "text": "OCR:" },
                { "type": "image_url", "image_url": { "url": image_url } }
            ]
        }]
    });
    let response = crate::pipeline::api_common::LOCAL_HTTP_CLIENT
        .post(format!("{base_url}/v1/chat/completions"))
        .bearer_auth(api_key)
        .timeout(timeout)
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("PaddleOCR-VL request failed: {error}"))?;
    let status = response.status();
    let bytes = crate::pipeline::api_common::response_bytes_limited(
        response,
        MAX_PADDLE_RESPONSE_BYTES,
        None,
    )
    .await
    .map_err(|error| format!("Failed to read PaddleOCR-VL response: {error}"))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Failed to decode PaddleOCR-VL response: {error}"))?;
    if !status.is_success() {
        let message = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(|message| message.as_str())
            .unwrap_or("unknown local inference error");
        return Err(format!(
            "PaddleOCR-VL request failed (HTTP {status}): {message}"
        ));
    }
    paddle_response_text(&value)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PaddleTile {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    depth: u8,
    order: Vec<u8>,
}

fn paddle_png_dimensions(path: &Path) -> Result<(u32, u32), String> {
    use std::io::Read as _;
    let mut file = open_regular_file(path)?;
    let mut header = [0u8; 24];
    file.read_exact(&mut header)
        .map_err(|error| format!("Failed to inspect rendered PDF page: {error}"))?;
    if !header.starts_with(b"\x89PNG\r\n\x1a\n") || &header[12..16] != b"IHDR" {
        return Err("Rendered PaddleOCR-VL page is not a valid PNG image".to_string());
    }
    let width = u32::from_be_bytes(header[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(header[20..24].try_into().unwrap());
    if width == 0 || height == 0 {
        return Err("Rendered PaddleOCR-VL page has invalid dimensions".to_string());
    }
    Ok((width, height))
}

fn split_paddle_tile(tile: &PaddleTile) -> Option<[PaddleTile; 2]> {
    const MIN_TILE_EDGE: u32 = 256;
    let split_vertically = tile.width >= tile.height;
    let dimension = if split_vertically {
        tile.width
    } else {
        tile.height
    };
    if dimension < MIN_TILE_EDGE.saturating_mul(2) {
        return None;
    }
    // A small overlap keeps a text line or chart label that crosses the
    // midpoint intact in at least one child. The page-level verifier remains
    // responsible for rejecting an incomplete aggregate.
    let overlap = (dimension / 20).max(16);
    let first_extent = dimension.saturating_add(overlap) / 2;
    let second_offset = dimension.saturating_sub(first_extent);
    let mut first_order = tile.order.clone();
    first_order.push(0);
    let mut second_order = tile.order.clone();
    second_order.push(1);
    let next_depth = tile.depth.saturating_add(1);
    if split_vertically {
        Some([
            PaddleTile {
                x: tile.x,
                y: tile.y,
                width: first_extent,
                height: tile.height,
                depth: next_depth,
                order: first_order,
            },
            PaddleTile {
                x: tile.x.saturating_add(second_offset),
                y: tile.y,
                width: dimension.saturating_sub(second_offset),
                height: tile.height,
                depth: next_depth,
                order: second_order,
            },
        ])
    } else {
        Some([
            PaddleTile {
                x: tile.x,
                y: tile.y,
                width: tile.width,
                height: first_extent,
                depth: next_depth,
                order: first_order,
            },
            PaddleTile {
                x: tile.x,
                y: tile.y.saturating_add(second_offset),
                width: tile.width,
                height: dimension.saturating_sub(second_offset),
                depth: next_depth,
                order: second_order,
            },
        ])
    }
}

fn render_paddle_tile(
    pdf: &Path,
    output_dir: &Path,
    page: usize,
    tile: &PaddleTile,
    render_dpi: u32,
) -> Result<PathBuf, String> {
    let bin = find_command("pdftoppm").ok_or("pdftoppm not found on PATH")?;
    let pdf_str = pdf
        .to_str()
        .ok_or_else(|| format!("Path contains invalid UTF-8: {}", pdf.display()))?;
    let tile_id = tile
        .order
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join("-");
    let prefix = output_dir.join(format!("recovery-{page:04}-{tile_id}"));
    let prefix_str = prefix
        .to_str()
        .ok_or_else(|| format!("Path contains invalid UTF-8: {}", prefix.display()))?;
    let page = page.to_string();
    let x = tile.x.to_string();
    let y = tile.y.to_string();
    let width = tile.width.to_string();
    let height = tile.height.to_string();
    let render_dpi = render_dpi.to_string();
    let mut command = bin.command([
        "-png",
        "-r",
        &render_dpi,
        "-f",
        &page,
        "-l",
        &page,
        "-singlefile",
        "-x",
        &x,
        "-y",
        &y,
        "-W",
        &width,
        "-H",
        &height,
        pdf_str,
        prefix_str,
    ]);
    command.env("PATH", env::full_path());
    let output = run_bounded_output(
        command,
        "pdftoppm PaddleOCR-VL recovery crop",
        std::time::Duration::from_secs(120),
        1_000_000,
        Some(output_dir),
    )?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Failed to render PaddleOCR-VL recovery region: {}",
            stderr.trim()
        ));
    }
    let image_path = prefix.with_extension("png");
    if !image_path.is_file() {
        return Err("pdftoppm did not produce a PaddleOCR-VL recovery region".to_string());
    }
    Ok(image_path)
}

fn paddle_failure_suggests_decomposition(error: &str) -> bool {
    error.contains("finish_reason=length")
        || error.contains("degenerate output")
        || error.contains("empty page output")
        || error.contains("far shorter than the PDF text-layer baseline")
}

#[allow(clippy::too_many_arguments)]
async fn paddle_extract_page_tiled(
    base_url: &str,
    api_key: &str,
    pdf_path: &Path,
    image_path: &Path,
    output_dir: &Path,
    page: usize,
    timeout: std::time::Duration,
    max_output_tokens: u32,
    max_depth: u8,
    render_dpi: u32,
) -> Result<String, String> {
    use std::collections::VecDeque;

    let (width, height) = paddle_png_dimensions(image_path)?;
    let root = PaddleTile {
        x: 0,
        y: 0,
        width,
        height,
        depth: 0,
        order: Vec::new(),
    };
    let children = split_paddle_tile(&root)
        .ok_or("PaddleOCR-VL page is too small for adaptive layout recovery")?;
    let mut pending = VecDeque::from(children);
    let mut completed: Vec<(Vec<u8>, String)> = Vec::new();
    while let Some(tile) = pending.pop_front() {
        if crate::commands::is_cancelled() {
            return Err("Pipeline cancelled".to_string());
        }
        let pdf = pdf_path.to_path_buf();
        let output_dir = output_dir.to_path_buf();
        let render_tile = tile.clone();
        let tile_path = tokio::task::spawn_blocking(move || {
            render_paddle_tile(&pdf, &output_dir, page, &render_tile, render_dpi)
        })
        .await
        .map_err(|error| format!("PaddleOCR-VL recovery render task failed: {error}"))??;
        match paddle_extract_page(base_url, api_key, &tile_path, timeout, max_output_tokens).await {
            Ok(text) => completed.push((tile.order, text)),
            Err(error)
                if paddle_failure_suggests_decomposition(&error) && tile.depth < max_depth =>
            {
                let children = split_paddle_tile(&tile).ok_or_else(|| {
                    format!("PaddleOCR-VL recovery region could not be divided after: {error}")
                })?;
                pending.push_back(children[0].clone());
                pending.push_back(children[1].clone());
            }
            Err(error) if paddle_failure_suggests_decomposition(&error) => {
                completed.push((
                    tile.order,
                    format!(
                        "> [{PADDLE_REGION_WARNING} Region: x={}, y={}, width={}, height={}.]",
                        tile.x, tile.y, tile.width, tile.height
                    ),
                ));
            }
            Err(error) => {
                return Err(format!(
                    "PaddleOCR-VL adaptive layout recovery failed on a page region: {error}"
                ));
            }
        }
    }
    completed.sort_by(|left, right| left.0.cmp(&right.0));
    let text = completed
        .into_iter()
        .map(|(_, text)| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.is_empty() {
        return Err("PaddleOCR-VL adaptive layout recovery returned empty output".to_string());
    }
    Ok(text)
}

fn paddle_page_is_suspicious(text: &str, baseline_len: Option<usize>) -> bool {
    baseline_len.is_some_and(|baseline_len| {
        let extracted_chars = text
            .lines()
            .filter(|line| !line.contains(PADDLE_REGION_WARNING_PREFIX))
            .flat_map(str::chars)
            .filter(|value| !value.is_whitespace())
            .count();
        baseline_len >= SUSPECT_BASELINE_MIN_CHARS && extracted_chars < baseline_len / 10
    })
}

fn paddle_markdown_blocks(markdown: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = Vec::new();
    let mut fence: Option<char> = None;
    let mut display_math: Option<&str> = None;

    for line in markdown.lines() {
        let trimmed = line.trim();
        let fence_marker = if trimmed.starts_with("```") {
            Some('`')
        } else if trimmed.starts_with("~~~") {
            Some('~')
        } else {
            None
        };
        if let Some(marker) = fence_marker {
            if fence == Some(marker) {
                fence = None;
            } else if fence.is_none() {
                fence = Some(marker);
            }
        }
        if fence.is_none() {
            match trimmed {
                "$$" => {
                    display_math = if display_math == Some("$$") {
                        None
                    } else if display_math.is_none() {
                        Some("$$")
                    } else {
                        display_math
                    };
                }
                r"\[" => {
                    if display_math.is_none() {
                        display_math = Some(r"\[");
                    }
                }
                r"\]" if display_math == Some(r"\[") => display_math = None,
                _ => {}
            }
        }

        if trimmed.is_empty() && fence.is_none() && display_math.is_none() {
            if !current.is_empty() {
                blocks.push(current.join("\n").trim().to_string());
                current.clear();
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        blocks.push(current.join("\n").trim().to_string());
    }
    blocks.retain(|block| !block.is_empty());
    blocks
}

fn paddle_plain_text(markdown: &str) -> String {
    let without_comments = Regex::new(r"(?s)<!--.*?-->")
        .unwrap()
        .replace_all(markdown, " ");
    let without_html = Regex::new(r"(?s)<[^>]+>")
        .unwrap()
        .replace_all(&without_comments, " ");
    let without_links = Regex::new(r"!?\[([^\]]*)\]\([^)]+\)")
        .unwrap()
        .replace_all(&without_html, "$1");
    let without_markers = Regex::new(r"(?m)^\s{0,3}(?:#{1,6}\s+|>\s*|[-+]\s+)")
        .unwrap()
        .replace_all(&without_links, "");
    without_markers
        .replace(['*', '_', '`'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn paddle_boundary_signature(block: &PaddleStructuredBlock) -> Option<String> {
    let text = block.text.trim();
    if text.is_empty()
        || text.len() > 240
        || text.contains(PADDLE_REGION_WARNING_PREFIX)
        || Regex::new(r"^\s*\[\^[^\]\r\n]{1,32}\]:")
            .unwrap()
            .is_match(block.markdown.trim_start())
    {
        return None;
    }
    let normalized = Regex::new(r"\d+")
        .unwrap()
        .replace_all(&text.to_ascii_lowercase(), "#")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

fn paddle_is_standalone_page_number(text: &str) -> bool {
    Regex::new(r"(?i)^\s*(?:page\s+)?(?:\d+|[ivxlcdm]{2,})(?:\s+(?:of|/)\s*\d+)?\s*$")
        .unwrap()
        .is_match(text)
}

fn classify_paddle_margins(pages: &mut [PaddleStructuredPage]) -> usize {
    if pages.is_empty() {
        return 0;
    }
    let mut header_pages = HashMap::<String, HashSet<u32>>::new();
    let mut footer_pages = HashMap::<String, HashSet<u32>>::new();
    for page in pages.iter() {
        let Some(first) = page.blocks.first() else {
            continue;
        };
        if let Some(signature) = paddle_boundary_signature(first) {
            header_pages
                .entry(signature)
                .or_default()
                .insert(page.number);
        }
        if let Some(last) = page.blocks.last() {
            if let Some(signature) = paddle_boundary_signature(last) {
                footer_pages
                    .entry(signature)
                    .or_default()
                    .insert(page.number);
            }
        }
    }
    let page_count = pages.len();
    let repeated = |matches: Option<&HashSet<u32>>| {
        matches.is_some_and(|pages| pages.len() >= 3 && pages.len() * 2 >= page_count)
    };
    let mut classified = 0usize;
    for page in pages {
        if page.blocks.is_empty() {
            continue;
        }
        let last_index = page.blocks.len() - 1;
        let first_is_page_number = paddle_is_standalone_page_number(&page.blocks[0].text);
        let last_is_page_number = paddle_is_standalone_page_number(&page.blocks[last_index].text);

        if last_is_page_number {
            page.blocks[last_index].role = "page_footer".to_string();
            classified += 1;
        }
        if first_is_page_number && last_index != 0 {
            page.blocks[0].role = "page_header".to_string();
            classified += 1;
        }

        let protected_first_page_heading =
            page.number == 1 && page.blocks[0].markdown.trim_start().starts_with('#');
        if !protected_first_page_heading && page.blocks[0].role == "body" {
            if let Some(signature) = paddle_boundary_signature(&page.blocks[0]) {
                if repeated(header_pages.get(&signature)) {
                    page.blocks[0].role = "page_header".to_string();
                    classified += 1;
                }
            }
        }
        if page.blocks[last_index].role == "body" {
            if let Some(signature) = paddle_boundary_signature(&page.blocks[last_index]) {
                if repeated(footer_pages.get(&signature)) {
                    page.blocks[last_index].role = "page_footer".to_string();
                    classified += 1;
                }
            }
        }
    }
    classified
}

#[derive(Debug)]
struct PaddleNoteMarker {
    value: String,
    explicit_definition: bool,
}

fn unicode_superscript_value(value: &str) -> Option<String> {
    let mut output = String::new();
    for character in value.chars() {
        output.push(match character {
            '⁰' => '0',
            '¹' => '1',
            '²' => '2',
            '³' => '3',
            '⁴' => '4',
            '⁵' => '5',
            '⁶' => '6',
            '⁷' => '7',
            '⁸' => '8',
            '⁹' => '9',
            _ => return None,
        });
    }
    (!output.is_empty()).then_some(output)
}

fn unicode_superscript_marker(value: &str) -> String {
    value
        .chars()
        .filter_map(|character| match character {
            '0' => Some('⁰'),
            '1' => Some('¹'),
            '2' => Some('²'),
            '3' => Some('³'),
            '4' => Some('⁴'),
            '5' => Some('⁵'),
            '6' => Some('⁶'),
            '7' => Some('⁷'),
            '8' => Some('⁸'),
            '9' => Some('⁹'),
            _ => None,
        })
        .collect()
}

fn paddle_leading_note_marker(block: &PaddleStructuredBlock) -> Option<PaddleNoteMarker> {
    let markdown = block.markdown.trim_start();
    if let Some(capture) = Regex::new(r"^\[\^([^\]\r\n]{1,32})\]:\s*")
        .unwrap()
        .captures(markdown)
    {
        return Some(PaddleNoteMarker {
            value: capture[1].to_string(),
            explicit_definition: true,
        });
    }
    if let Some(capture) = Regex::new(r"(?is)^<sup\b[^>]*>\s*(\d{1,3}|[*†‡])\s*</sup>\s*")
        .unwrap()
        .captures(markdown)
    {
        return Some(PaddleNoteMarker {
            value: capture[1].to_string(),
            explicit_definition: false,
        });
    }
    if let Some(capture) = Regex::new(r"^([⁰¹²³⁴⁵⁶⁷⁸⁹]{1,3})\s+")
        .unwrap()
        .captures(markdown)
    {
        return unicode_superscript_value(&capture[1]).map(|value| PaddleNoteMarker {
            value,
            explicit_definition: false,
        });
    }
    Regex::new(r"^(?:>\s*)?(\d{1,3}|[*†‡])(?:[.)])?\s+")
        .unwrap()
        .captures(markdown)
        .map(|capture| PaddleNoteMarker {
            value: capture[1].to_string(),
            explicit_definition: false,
        })
}

fn paddle_preceding_note_anchor(blocks: &[PaddleStructuredBlock], marker: &str) -> bool {
    let markdown_reference = format!("[^{marker}]");
    let latex_reference = format!(r"\textsuperscript{{{marker}}}");
    let unicode_reference = unicode_superscript_marker(marker);
    let html_pattern = format!(r"(?is)<sup\b[^>]*>\s*{}\s*</sup>", regex::escape(marker));
    let html_anchor = Regex::new(&html_pattern).ok();
    blocks
        .iter()
        .filter(|block| block.role == "body")
        .any(|block| {
            block.markdown.contains(&markdown_reference)
                || block.markdown.contains(&latex_reference)
                || (!unicode_reference.is_empty() && block.markdown.contains(&unicode_reference))
                || html_anchor
                    .as_ref()
                    .is_some_and(|pattern| pattern.is_match(&block.markdown))
        })
}

fn classify_paddle_footnotes(pages: &mut [PaddleStructuredPage]) -> usize {
    let mut possible = 0usize;
    for page in pages {
        let block_count = page.blocks.len();
        for index in 0..block_count {
            if page.blocks[index].role != "body" {
                continue;
            }
            let Some(marker) = paddle_leading_note_marker(&page.blocks[index]) else {
                continue;
            };
            page.blocks[index].note_marker = Some(marker.value.clone());
            if marker.explicit_definition {
                page.blocks[index].role = "footnote".to_string();
                continue;
            }
            let trailing = index.saturating_mul(4) >= block_count.saturating_mul(3)
                || index.saturating_add(3) >= block_count;
            if trailing
                && page.blocks[index].text.len() <= 2_000
                && paddle_preceding_note_anchor(&page.blocks[..index], &marker.value)
            {
                page.blocks[index].role = "possible_footnote".to_string();
                possible += 1;
            }
        }
    }
    possible
}

fn render_paddle_page(page: &PaddleStructuredPage) -> String {
    let mut output = format!("<!-- PAGE {} -->\n\n", page.number);
    let mut footnotes = Vec::new();
    for block in &page.blocks {
        match block.role.as_str() {
            "page_header" | "page_footer" => {}
            "footnote" => footnotes.push(block),
            "possible_footnote" => {
                output.push_str(&format!(
                    "<!-- BEGIN POSSIBLE_FOOTNOTE page={} block_id={} -->\n\
                     <aside data-document-role=\"possible-footnote\">\n{}\n</aside>\n\
                     <!-- END POSSIBLE_FOOTNOTE page={} -->\n\n",
                    page.number, block.block_id, block.markdown, page.number
                ));
            }
            _ => {
                output.push_str(&block.markdown);
                output.push_str("\n\n");
            }
        }
    }
    if !footnotes.is_empty() {
        output.push_str(&format!("<!-- BEGIN FOOTNOTES page={} -->\n", page.number));
        for block in footnotes {
            output.push_str("<aside data-document-role=\"footnote\">\n");
            output.push_str(&block.markdown);
            output.push_str("\n</aside>\n");
        }
        output.push_str(&format!("<!-- END FOOTNOTES page={} -->\n", page.number));
    }
    output.trim().to_string()
}

fn normalize_paddle_pages(
    raw_pages: &[String],
    structure_path: &Path,
) -> Result<PaddleExtraction, String> {
    let mut pages = Vec::with_capacity(raw_pages.len());
    for (page_index, raw_page) in raw_pages.iter().enumerate() {
        let markdown_blocks = paddle_markdown_blocks(raw_page);
        if markdown_blocks.is_empty() {
            return Err(format!(
                "PaddleOCR-VL normalization found no content blocks on page {}",
                page_index + 1
            ));
        }
        let last_index = markdown_blocks.len() - 1;
        let blocks = markdown_blocks
            .into_iter()
            .enumerate()
            .map(|(block_index, markdown)| {
                let boundary = if block_index == 0 && block_index == last_index {
                    Some("top_and_bottom".to_string())
                } else if block_index == 0 {
                    Some("top".to_string())
                } else if block_index == last_index {
                    Some("bottom".to_string())
                } else {
                    None
                };
                PaddleStructuredBlock {
                    block_id: format!(
                        "paddle-page-{:04}-block-{:04}",
                        page_index + 1,
                        block_index + 1
                    ),
                    role: "body".to_string(),
                    text: paddle_plain_text(&markdown),
                    markdown,
                    boundary,
                    note_marker: None,
                }
            })
            .collect();
        pages.push(PaddleStructuredPage {
            number: page_index as u32 + 1,
            blocks,
        });
    }

    classify_paddle_margins(&mut pages);
    let possible_footnotes = classify_paddle_footnotes(&mut pages);
    let mut quality_notes = Vec::new();
    if possible_footnotes > 0 {
        quality_notes.push(format!(
            "PaddleOCR-VL classified {possible_footnotes} trailing text block(s) as possible footnotes using matching in-page reference markers. Verify the rendered page if a finding depends on whether this text is a footnote."
        ));
    }
    let text = pages
        .iter()
        .map(render_paddle_page)
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.trim().is_empty() {
        return Err("PaddleOCR-VL structured normalization returned empty output".to_string());
    }
    if text.len() > super::claude::MAX_STDOUT_BYTES {
        return Err("Normalized PaddleOCR-VL document exceeded the 50 MB safety limit".to_string());
    }
    let structure = PaddleStructure {
        schema_version: PADDLE_STRUCTURE_SCHEMA,
        quality_notes: quality_notes.clone(),
        pages,
    };
    let structure_json = serde_json::to_string_pretty(&structure)
        .map_err(|error| format!("Failed to serialize PaddleOCR-VL structure: {error}"))?;
    if structure_json.len() > super::claude::MAX_STDOUT_BYTES {
        return Err("PaddleOCR-VL structure exceeded the 50 MB safety limit".to_string());
    }
    atomic_write_cache(structure_path, structure_json.as_bytes())?;
    Ok(PaddleExtraction {
        text: text.trim().to_string(),
        quality_notes,
    })
}

fn paddle_checkpoint_dir(
    hash: &str,
    paths: &crate::engines::PaddleEnginePaths,
    settings: &crate::settings::Settings,
) -> Option<PathBuf> {
    if hash.len() != 16 || !hash.chars().all(|value| value.is_ascii_hexdigit()) {
        return None;
    }
    let identity = serde_json::json!({
        "schema": EXTRACTION_CACHE_SCHEMA,
        "engine": "paddleocr-vl-1.6-q8",
        "server": executable_identity(&paths.server),
        "model": executable_identity(&paths.model),
        "projector": executable_identity(&paths.mmproj),
        "concurrency": crate::settings::resolved_paddle_page_concurrency(settings),
        "vision_batch": crate::settings::resolved_paddle_mtmd_batch_tokens(settings),
        "flash_attention": settings.paddle_flash_attention,
        "max_output_tokens": settings.paddle_max_output_tokens,
        "render_dpi": settings.paddle_render_dpi,
    });
    Some(
        dirs::home_dir()?
            .join(".pipeline")
            .join("cache")
            .join("paddleocr-vl")
            .join(hash)
            .join(cache_fingerprint(&identity)),
    )
}

fn paddle_page_checkpoint_path(root: &Path, index: usize) -> PathBuf {
    root.join(format!("page-{:04}.md", index + 1))
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

/// Extract every rendered PDF page through one managed llama.cpp process.
/// Loading the Q8 model once per document is the important performance
/// property; page boundaries also make completeness checks deterministic.
async fn extract_paddle(
    app: &crate::emit::EventBus,
    path: &Path,
    hash: &str,
    settings: &crate::settings::Settings,
) -> Result<ExtractionResult, String> {
    let paths = crate::engines::paddle_engine_paths()?;
    let render_started = std::time::Instant::now();
    let render_dir = tempfile::Builder::new()
        .prefix("pipeline_paddle_pages_")
        .tempdir()
        .map_err(|error| format!("Failed to create PaddleOCR-VL page directory: {error}"))?;
    let pdf = path.to_path_buf();
    let output_dir = render_dir.path().to_path_buf();
    let render_dpi = settings.paddle_render_dpi;
    let rendered = tokio::task::spawn_blocking(move || {
        render_pdf_pages_for_ocr(&pdf, &output_dir, MAX_RENDERED_PDF_PAGES, render_dpi)
    })
    .await
    .map_err(|error| format!("PDF rendering task failed: {error}"))??;
    if rendered.truncated {
        return Err(format!(
            "PaddleOCR-VL extraction is limited to {MAX_RENDERED_PDF_PAGES} pages per PDF"
        ));
    }
    extraction_log(
        app,
        format!(
            "PaddleOCR-VL: rendered {} OCR page(s) in {:.1}s",
            rendered.names.len(),
            render_started.elapsed().as_secs_f64()
        ),
    );

    let page_concurrency = crate::settings::resolved_paddle_page_concurrency(settings) as usize;
    let mtmd_batch_tokens = crate::settings::resolved_paddle_mtmd_batch_tokens(settings);
    extraction_log(
        app,
        format!(
            "PaddleOCR-VL tuning: {} page slot(s) × 16K context, {} DPI, {} vision batch tokens, Flash Attention {}, {} output tokens/page, {} retr{}",
            page_concurrency,
            settings.paddle_render_dpi,
            mtmd_batch_tokens,
            settings.paddle_flash_attention,
            settings.paddle_max_output_tokens,
            settings.paddle_page_retries,
            if settings.paddle_page_retries == 1 { "y" } else { "ies" }
        ),
    );
    let page_timeout =
        std::time::Duration::from_secs((settings.step_timeout_secs / 2).clamp(120, 900));
    let baseline = {
        let pdf = path.to_path_buf();
        tokio::task::spawn_blocking(move || pdftotext_page_baseline(&pdf))
            .await
            .unwrap_or(None)
    };
    let page_count = rendered.names.len();
    let mut page_texts = vec![None; page_count];
    let checkpoint_root = settings
        .reuse_pdf_extraction_cache
        .then(|| paddle_checkpoint_dir(hash, &paths, settings))
        .flatten();
    let mut cached_pages = 0usize;
    if let Some(root) = &checkpoint_root {
        for (index, slot) in page_texts.iter_mut().enumerate() {
            let content =
                read_utf8_capped(&paddle_page_checkpoint_path(root, index), 2 * 1024 * 1024).ok();
            if let Some(content) = content.filter(|content| {
                !content.trim().is_empty()
                    && !paddle_page_is_suspicious(
                        content,
                        baseline
                            .as_ref()
                            .and_then(|pages| pages.get(index))
                            .copied(),
                    )
            }) {
                *slot = Some(content.trim().to_string());
                cached_pages += 1;
            }
        }
    }
    if cached_pages > 0 {
        extraction_log(
            app,
            format!(
                "PaddleOCR-VL: resumed {cached_pages}/{page_count} page(s) from verified checkpoints"
            ),
        );
    }
    let pending_pages: Vec<usize> = page_texts
        .iter()
        .enumerate()
        .filter_map(|(index, content)| content.is_none().then_some(index))
        .collect();
    let server = if pending_pages.is_empty() {
        None
    } else {
        extraction_log(
            app,
            format!(
                "PaddleOCR-VL: loading the managed Q8 model for {} remaining page(s)",
                pending_pages.len()
            ),
        );
        let load_started = std::time::Instant::now();
        let server = start_paddle_server(&paths, settings).await?;
        extraction_log(
            app,
            format!(
                "PaddleOCR-VL: model server ready in {:.1}s",
                load_started.elapsed().as_secs_f64()
            ),
        );
        Some(server)
    };
    let mut tasks = tokio::task::JoinSet::new();
    let mut next_pending = 0usize;
    while next_pending < pending_pages.len() && tasks.len() < page_concurrency {
        let index = pending_pages[next_pending];
        spawn_paddle_page_task(
            &mut tasks,
            app,
            index,
            page_count,
            server
                .as_ref()
                .expect("pending pages require server")
                .base_url
                .clone(),
            server
                .as_ref()
                .expect("pending pages require server")
                .api_key
                .clone(),
            render_dir.path().join(&rendered.names[index]),
            path.to_path_buf(),
            render_dir.path().to_path_buf(),
            page_timeout,
            settings.paddle_max_output_tokens,
            settings.paddle_page_retries,
            settings.paddle_render_dpi,
            baseline
                .as_ref()
                .and_then(|pages| pages.get(index))
                .copied(),
            checkpoint_root
                .as_ref()
                .map(|root| paddle_page_checkpoint_path(root, index)),
        );
        next_pending += 1;
    }
    while let Some(result) = tasks.join_next().await {
        if crate::commands::is_cancelled() {
            tasks.abort_all();
            return Err("Pipeline cancelled".to_string());
        }
        let (index, page_text) =
            result.map_err(|error| format!("PaddleOCR-VL page task failed: {error}"))??;
        page_texts[index] = Some(page_text);
        if next_pending < pending_pages.len() {
            let index = pending_pages[next_pending];
            spawn_paddle_page_task(
                &mut tasks,
                app,
                index,
                page_count,
                server
                    .as_ref()
                    .expect("pending pages require server")
                    .base_url
                    .clone(),
                server
                    .as_ref()
                    .expect("pending pages require server")
                    .api_key
                    .clone(),
                render_dir.path().join(&rendered.names[index]),
                path.to_path_buf(),
                render_dir.path().to_path_buf(),
                page_timeout,
                settings.paddle_max_output_tokens,
                settings.paddle_page_retries,
                settings.paddle_render_dpi,
                baseline
                    .as_ref()
                    .and_then(|pages| pages.get(index))
                    .copied(),
                checkpoint_root
                    .as_ref()
                    .map(|root| paddle_page_checkpoint_path(root, index)),
            );
            next_pending += 1;
        }
    }

    let mut raw_pages = Vec::with_capacity(page_count);
    for (index, page_text) in page_texts.into_iter().enumerate() {
        let page = index + 1;
        let page_text = page_text
            .ok_or_else(|| format!("PaddleOCR-VL did not return output for page {page}"))?;
        if paddle_page_is_suspicious(
            &page_text,
            baseline
                .as_ref()
                .and_then(|pages| pages.get(index))
                .copied(),
        ) {
            return Err(format!(
                "PaddleOCR-VL returned incomplete output for page {page} after retries"
            ));
        }
        raw_pages.push(page_text.trim().to_string());
    }
    let structure_path = paddle_structure_path(hash)
        .ok_or_else(|| "Could not create PaddleOCR-VL structure path".to_string())?;
    let normalized = normalize_paddle_pages(&raw_pages, &structure_path)?;
    let text = normalized.text;
    let mut quality_notes = normalized.quality_notes;
    quality_notes.extend(scan_math_quality(&text));
    if text.contains(PADDLE_REGION_WARNING_PREFIX) {
        quality_notes.push(
            "PaddleOCR-VL could not reliably transcribe one or more visual regions; consult the rendered source pages at the embedded warnings."
                .to_string(),
        );
    }
    Ok(ExtractionResult {
        text,
        method: "paddleocr-vl".to_string(),
        source_path: path.to_string_lossy().to_string(),
        paper_hash: hash.to_string(),
        quality_notes,
    })
}

#[allow(clippy::too_many_arguments)]
fn spawn_paddle_page_task(
    tasks: &mut tokio::task::JoinSet<Result<(usize, String), String>>,
    app: &crate::emit::EventBus,
    index: usize,
    page_count: usize,
    base_url: String,
    api_key: String,
    image_path: PathBuf,
    pdf_path: PathBuf,
    recovery_dir: PathBuf,
    timeout: std::time::Duration,
    max_output_tokens: u32,
    retries: u32,
    render_dpi: u32,
    baseline_len: Option<usize>,
    checkpoint_path: Option<PathBuf>,
) {
    let app = app.clone();
    tasks.spawn(async move {
        if crate::commands::is_cancelled() {
            return Err("Pipeline cancelled".to_string());
        }
        let page = index + 1;
        let page_started = std::time::Instant::now();
        extraction_log(
            &app,
            format!("PaddleOCR-VL: extracting page {page}/{page_count}"),
        );
        let mut last_error = String::new();
        let mut use_layout_recovery = false;
        for attempt in 0..=retries {
            let result = if use_layout_recovery {
                extraction_log(
                    &app,
                    format!(
                        "PaddleOCR-VL: decomposing page {page}/{page_count} into smaller layout regions"
                    ),
                );
                paddle_extract_page_tiled(
                    &base_url,
                    &api_key,
                    &pdf_path,
                    &image_path,
                    &recovery_dir,
                    page,
                    timeout,
                    max_output_tokens,
                    (attempt as u8).saturating_add(2).min(3),
                    render_dpi,
                )
                .await
            } else {
                paddle_extract_page(
                    &base_url,
                    &api_key,
                    &image_path,
                    timeout,
                    max_output_tokens,
                )
                .await
            };
            match result {
                Ok(text) if !paddle_page_is_suspicious(&text, baseline_len) => {
                    if let Some(path) = &checkpoint_path {
                        if let Err(error) = atomic_write_cache(path, text.as_bytes()) {
                            extraction_log(
                                &app,
                                format!(
                                    "WARNING: could not save PaddleOCR-VL page {page} checkpoint: {error}"
                                ),
                            );
                        }
                    }
                    extraction_log(
                        &app,
                        format!(
                            "PaddleOCR-VL: completed page {page}/{page_count} in {:.1}s (attempt {})",
                            page_started.elapsed().as_secs_f64(),
                            attempt + 1
                        ),
                    );
                    return Ok((index, text));
                }
                Ok(_) => {
                    last_error =
                        "output was far shorter than the PDF text-layer baseline".to_string();
                    use_layout_recovery = true;
                }
                Err(error) => {
                    use_layout_recovery = paddle_failure_suggests_decomposition(&error);
                    last_error = error;
                }
            }
            if attempt < retries {
                extraction_log(
                    &app,
                    format!(
                        "PaddleOCR-VL: retrying page {page}/{page_count} after: {last_error}"
                    ),
                );
            }
        }
        Err(format!(
            "PaddleOCR-VL page {page} failed after {} attempt(s): {last_error}",
            retries + 1
        ))
    });
}

/// Resolve a bundled or PATH command without invoking a shell.
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

#[derive(Clone, Copy)]
enum PageRenderProfile {
    Artifact,
    Ocr(u32),
}

fn rendered_page_limit(requested: u32) -> u32 {
    requested.clamp(1, MAX_RENDERED_PDF_PAGES)
}

pub fn render_pdf_pages(
    pdf: &Path,
    out_dir: &Path,
    max_pages: u32,
) -> Result<RenderedPdfPages, String> {
    render_pdf_pages_with_profile(pdf, out_dir, max_pages, PageRenderProfile::Artifact)
}

fn render_pdf_pages_for_ocr(
    pdf: &Path,
    out_dir: &Path,
    max_pages: u32,
    dpi: u32,
) -> Result<RenderedPdfPages, String> {
    // OCR keeps a lossless profile whose resolution is independently tunable
    // from the compact, durable page artifacts.
    render_pdf_pages_with_profile(pdf, out_dir, max_pages, PageRenderProfile::Ocr(dpi))
}

fn render_pdf_pages_with_profile(
    pdf: &Path,
    out_dir: &Path,
    max_pages: u32,
    profile: PageRenderProfile,
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
    let (extension, args): (&str, Vec<String>) = match profile {
        PageRenderProfile::Artifact => (
            ".jpg",
            vec![
                "-jpeg".into(),
                "-r".into(),
                "120".into(),
                "-jpegopt".into(),
                "quality=85".into(),
                "-l".into(),
                probe_pages.clone(),
                pdf_str.into(),
                prefix_str.into(),
            ],
        ),
        PageRenderProfile::Ocr(dpi) => (
            ".png",
            vec![
                "-png".into(),
                "-r".into(),
                dpi.to_string(),
                "-l".into(),
                probe_pages,
                pdf_str.into(),
                prefix_str.into(),
            ],
        ),
    };
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
    let use_paddle = effective_method == "paddleocr-vl";

    // PaddleOCR-VL owns an async local HTTP server for the duration of one
    // document, so it cannot run inside the blocking native-extractor branch.
    if use_paddle {
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
            return tokio::time::timeout(
                std::time::Duration::from_secs(settings.pdf_extraction_timeout_secs),
                extract_paddle(app, &pdf, &hash, &settings),
            )
            .await
            .map_err(|_| {
                format!(
                    "PaddleOCR-VL extraction exceeded the {} second document budget",
                    settings.pdf_extraction_timeout_secs
                )
            })?;
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
    fn paddle_response_extracts_openai_message_content() {
        let response = serde_json::json!({
            "choices": [{
                "message": {
                    "content": "```markdown\n# Heading\n\n$x=1$\n```"
                }
            }]
        });
        assert_eq!(
            paddle_response_text(&response).unwrap(),
            "# Heading\n\n$x=1$"
        );
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
    fn paddle_response_surfaces_server_errors() {
        let response = serde_json::json!({
            "error": { "message": "failed to load projector" }
        });
        assert!(paddle_response_text(&response)
            .unwrap_err()
            .contains("failed to load projector"));
    }

    #[test]
    fn paddle_response_rejects_truncated_generation() {
        let response = serde_json::json!({
            "choices": [{
                "finish_reason": "length",
                "message": { "content": "partial page" }
            }]
        });
        assert!(paddle_response_text(&response)
            .unwrap_err()
            .contains("finish_reason=length"));
    }

    #[test]
    fn paddle_short_page_check_uses_a_conservative_floor() {
        assert!(paddle_page_is_suspicious("tiny", Some(1_000)));
        assert!(!paddle_page_is_suspicious("tiny", Some(100)));
        assert!(!paddle_page_is_suspicious("complete enough", None));
        let warnings = format!(
            "> [{PADDLE_REGION_WARNING}]\n\n> [{PADDLE_REGION_WARNING}]\n\n\
             > [{PADDLE_REGION_WARNING}]"
        );
        assert!(paddle_page_is_suspicious(&warnings, Some(1_000)));
    }

    #[test]
    fn paddle_normalization_separates_notes_and_repeated_margins() {
        let raw_pages = vec![
            "# Paper Title\n\nThe argument starts here.[^1]\n\n[^1]: An explicit note.\n\n1"
                .to_string(),
            "Journal of Macro 2026\n\nThe argument continues.<sup>2</sup>\n\n2 A possible note.\n\n2"
                .to_string(),
            "Journal of Macro 2026\n\nMore body text.\n\n3".to_string(),
            "Journal of Macro 2026\n\nThe conclusion.\n\n4".to_string(),
        ];
        let output_dir = tempfile::tempdir().unwrap();
        let structure_path = output_dir.path().join(PADDLE_STRUCTURE_FILE);
        let normalized = normalize_paddle_pages(&raw_pages, &structure_path).unwrap();

        assert!(!normalized.text.contains("Journal of Macro 2026"));
        assert!(normalized.text.contains("# Paper Title"));
        assert!(normalized.text.contains("<!-- PAGE 4 -->"));
        assert!(normalized.text.contains("BEGIN FOOTNOTES page=1"));
        assert!(normalized.text.contains("BEGIN POSSIBLE_FOOTNOTE page=2"));
        assert!(!normalized.text.lines().any(|line| line.trim() == "4"));
        assert!(normalized
            .quality_notes
            .iter()
            .any(|note| note.contains("possible footnotes")));

        let structure: PaddleStructure = serde_json::from_str(
            &read_utf8_capped(&structure_path, super::super::claude::MAX_STDOUT_BYTES).unwrap(),
        )
        .unwrap();
        assert_eq!(structure.pages.len(), 4);
        assert_eq!(structure.pages[0].blocks[0].role, "body");
        assert!(structure.pages[1]
            .blocks
            .iter()
            .any(|block| block.role == "page_header"));
        assert!(structure.pages[0]
            .blocks
            .iter()
            .any(|block| block.role == "footnote"));
        assert!(structure.pages[1]
            .blocks
            .iter()
            .any(|block| block.role == "possible_footnote"));
        assert!(structure
            .pages
            .iter()
            .all(|page| page.blocks.last().unwrap().role == "page_footer"));
    }

    #[test]
    fn paddle_numbered_trailing_paragraph_without_anchor_stays_body() {
        let raw_pages = vec![concat!(
            "# Results\n\n",
            "The estimates are stable.\n\n",
            "The robustness checks reach the same conclusion.\n\n",
            "1 A numbered body paragraph near the end of the page."
        )
        .to_string()];
        let output_dir = tempfile::tempdir().unwrap();
        let structure_path = output_dir.path().join(PADDLE_STRUCTURE_FILE);
        let normalized = normalize_paddle_pages(&raw_pages, &structure_path).unwrap();
        let structure: PaddleStructure = serde_json::from_str(
            &read_utf8_capped(&structure_path, super::super::claude::MAX_STDOUT_BYTES).unwrap(),
        )
        .unwrap();
        assert_eq!(structure.pages[0].blocks[3].role, "body");
        assert!(normalized
            .text
            .contains("1 A numbered body paragraph near the end of the page."));
        assert!(!normalized.text.contains("POSSIBLE_FOOTNOTE"));
    }

    #[test]
    fn paddle_block_splitter_preserves_blank_lines_inside_fences_and_math() {
        let markdown = "# Appendix\n\n```text\nline one\n\nline two\n```\n\n$$\nx = 1\n\ny = 2\n$$";
        let blocks = paddle_markdown_blocks(markdown);
        assert_eq!(blocks.len(), 3);
        assert!(blocks[1].contains("line one\n\nline two"));
        assert!(blocks[2].contains("x = 1\n\ny = 2"));
    }

    #[test]
    fn rendered_page_limit_matches_documented_cap() {
        assert_eq!(rendered_page_limit(0), 1);
        assert_eq!(rendered_page_limit(50), 50);
        assert_eq!(rendered_page_limit(500), MAX_RENDERED_PDF_PAGES);
        assert_eq!(MAX_RENDERED_PDF_PAGES, 300);
    }

    #[test]
    fn paddle_page_data_url_uses_the_rendered_image_media_type() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut file, &[0xff, 0xd8, 0xff, 0xd9]).unwrap();
        assert!(read_image_data_url(file.path())
            .unwrap()
            .starts_with("data:image/jpeg;base64,"));
    }

    #[test]
    fn paddle_png_dimensions_reads_ihdr_without_decoding_the_page() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let mut header = Vec::from(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".as_slice());
        header.extend_from_slice(&1275u32.to_be_bytes());
        header.extend_from_slice(&1650u32.to_be_bytes());
        std::io::Write::write_all(&mut file, &header).unwrap();
        assert_eq!(paddle_png_dimensions(file.path()).unwrap(), (1275, 1650));
    }

    #[test]
    fn paddle_tiles_bisect_the_long_axis_with_overlap_and_reading_order() {
        let page = PaddleTile {
            x: 0,
            y: 0,
            width: 1275,
            height: 1650,
            depth: 0,
            order: Vec::new(),
        };
        let [top, bottom] = split_paddle_tile(&page).unwrap();
        assert_eq!(top.order, vec![0]);
        assert_eq!(bottom.order, vec![1]);
        assert_eq!(top.width, page.width);
        assert_eq!(bottom.width, page.width);
        assert!(top.y + top.height > bottom.y);
        assert_eq!(bottom.y + bottom.height, page.height);

        let [left, right] = split_paddle_tile(&top).unwrap();
        assert_eq!(left.order, vec![0, 0]);
        assert_eq!(right.order, vec![0, 1]);
        assert!(left.x + left.width > right.x);
        assert_eq!(right.x + right.width, page.width);
    }

    #[test]
    fn paddle_length_failures_trigger_layout_decomposition() {
        assert!(paddle_failure_suggests_decomposition(
            "PaddleOCR-VL stopped before completing the page (finish_reason=length)"
        ));
        assert!(paddle_failure_suggests_decomposition(
            "PaddleOCR-VL returned empty page output"
        ));
        assert!(!paddle_failure_suggests_decomposition(
            "PaddleOCR-VL request failed: connection reset"
        ));
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
