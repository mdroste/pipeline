use crate::env;
use crate::models::ExtractionResult;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

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
    let stdout_thread = std::thread::spawn(move || {
        stdout
            .map(|pipe| drain_capped(pipe, output_limit))
            .unwrap_or_default()
    });
    let stderr_thread = std::thread::spawn(move || {
        stderr
            .map(|pipe| drain_capped(pipe, output_limit))
            .unwrap_or_default()
    });

    let started = std::time::Instant::now();
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
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
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

    if pid > 0 {
        crate::commands::unregister_child_pid(pid);
    }
    let (stdout, stdout_truncated) = stdout_thread.join().unwrap_or_default();
    let (stderr, stderr_truncated) = stderr_thread.join().unwrap_or_default();
    let status = result?;
    Ok(BoundedOutput {
        status,
        stdout,
        stderr,
        stdout_truncated,
        stderr_truncated,
    })
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
    let file =
        fs::File::open(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let count = std::io::Read::read(&mut reader, &mut chunk)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&chunk[..count]);
    }
    Ok(format!("{:x}", hasher.finalize())[..16].to_string())
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
        if let Ok(content) = fs::read_to_string(path) {
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

/// Maximum total output size for extracted LaTeX (10 MB).
const MAX_LATEX_SIZE: usize = 10_000_000;

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

    let content =
        fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

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

/// Image files marker emitted for a paper (figures/tables extracted from the
/// PDF), for registration as run artifacts.
pub fn marker_image_files(paper_hash: &str) -> Vec<PathBuf> {
    let Some(root) = marker_output_dir(paper_hash) else {
        return Vec::new();
    };
    let mut images = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if ["png", "jpg", "jpeg", "gif", "webp"]
                .iter()
                .any(|ext| ext_eq(&p, ext))
            {
                images.push(p);
            }
        }
    }
    images.sort();
    images
}

/// Find the markdown file marker wrote under its output dir (layout is
/// {output_dir}/{pdf_stem}/{pdf_stem}.md, but search defensively).
fn find_marker_markdown(root: &Path) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if ext_eq(&p, "md") {
                return Some(p);
            }
        }
    }
    None
}

/// Extract text from PDF using marker_single.
/// Timeout is half the user's step timeout, floored at 120s to accommodate first-run model downloads.
fn extract_marker(
    path: &Path,
    paper_hash: &str,
    marker_disable_ocr: bool,
    marker_disable_images: bool,
) -> Result<String, String> {
    let path_str = path
        .to_str()
        .ok_or_else(|| format!("Path contains invalid UTF-8: {}", path.display()))?;
    let settings = crate::settings::load();
    let mut marker_args = vec![
        path_str.to_string(),
        "--output_format".to_string(),
        "markdown".to_string(),
    ];
    // Write into the per-paper cache dir so the emitted markdown and figure
    // images land somewhere the run can collect them. Cleared first so a
    // prior run's files can't leak into this one.
    let out_dir = marker_output_dir(paper_hash);
    if let Some(dir) = &out_dir {
        let _ = fs::remove_dir_all(dir);
        if fs::create_dir_all(dir).is_ok() {
            marker_args.push("--output_dir".to_string());
            marker_args.push(dir.to_string_lossy().to_string());
        }
    }
    if marker_disable_images {
        marker_args.push("--disable_image_extraction".to_string());
    }
    if marker_disable_ocr {
        marker_args.push("--disable_ocr".to_string());
    }
    let marker_bin = find_command("marker_single").ok_or("marker_single not found on PATH")?;
    let mut cmd = StdCommand::new(&marker_bin);
    cmd.env("PATH", env::full_path()).args(&marker_args);
    // Managed installs keep their model weights under ~/.pipeline/hf.
    // System installs keep their own cache — don't redirect it.
    let is_managed = crate::engines::managed_bin_dir()
        .map(|d| marker_bin.starts_with(&d))
        .unwrap_or(false);
    if is_managed {
        for (k, v) in crate::engines::tool_env() {
            cmd.env(k, v);
        }
    }
    // Wait with timeout — use half the user's step timeout (same convention as LLM extraction),
    // with a floor of 120s to handle first-run model downloads.
    let timeout_secs = (settings.step_timeout_secs / 2).max(120);
    let output = run_bounded_output(
        cmd,
        "marker_single",
        std::time::Duration::from_secs(timeout_secs),
        super::claude::MAX_STDOUT_BYTES,
    )?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("marker_single failed: {}", stderr.trim()));
    }

    // Prefer the markdown file marker wrote to the output dir — stdout mixes
    // in log lines. Fall back to stdout for marker versions that don't write
    // the file where expected.
    if let Some(dir) = &out_dir {
        if let Some(md) = find_marker_markdown(dir) {
            if let Ok(content) = fs::read_to_string(&md) {
                let content = content.trim().to_string();
                if !content.is_empty() {
                    return Ok(content);
                }
            }
        }
    }
    if output.stdout_truncated {
        return Err("marker_single output exceeded the 50 MB safety limit".to_string());
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err("marker_single returned empty output".to_string());
    }
    Ok(text)
}

/// Extract text from PDF using pdftotext.
fn extract_pdftotext(path: &Path) -> Result<String, String> {
    let path_str = path
        .to_str()
        .ok_or_else(|| format!("Path contains invalid UTF-8: {}", path.display()))?;
    let pdftotext_bin = find_command("pdftotext").ok_or("pdftotext not found on PATH")?;
    let mut command = StdCommand::new(&pdftotext_bin);
    command
        .env("PATH", env::full_path())
        .args(["-layout", path_str, "-"]);
    let output = run_bounded_output(
        command,
        "pdftotext",
        std::time::Duration::from_secs(crate::settings::load().step_timeout_secs.max(60)),
        super::claude::MAX_STDOUT_BYTES,
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

/// Find a command by scanning the managed tool directory (~/.pipeline/bin),
/// then PATH, directly (no subprocess). Managed installs win over PATH so
/// the one-click install is the copy that actually runs.
fn find_command(name: &str) -> Option<PathBuf> {
    resolve_command(crate::engines::find_managed(name), &env::full_path(), name)
}

/// Pure resolution order: a managed install always beats anything on PATH.
/// Split from find_command so the precedence is unit-testable.
fn resolve_command(managed: Option<PathBuf>, path_var: &str, name: &str) -> Option<PathBuf> {
    if let Some(p) = managed {
        return Some(p);
    }
    let sep = if cfg!(windows) { ';' } else { ':' };
    for dir in path_var.split(sep) {
        if dir.is_empty() {
            continue;
        }
        let candidate = PathBuf::from(dir).join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        for ext in &[".exe", ".cmd", ".bat"] {
            let with_ext = PathBuf::from(format!("{}{ext}", candidate.display()));
            if with_ext.is_file() {
                return Some(with_ext);
            }
        }
    }
    None
}

// ── LLM extraction: transport, verification, repair ─────────────────

/// Max output tokens for extraction calls on direct-API paths. Analysis
/// steps keep the smaller default; transcribing a whole paper needs more.
const EXTRACTION_MAX_OUTPUT_TOKENS: u32 = 32_768;

/// Give up on targeted repair after this many page ranges per run; anything
/// left becomes a quality note instead of more LLM calls.
const MAX_REPAIR_RANGES: usize = 3;

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
        "codex" => !settings.openai_api_key.is_empty(),
        "gemini" => !settings.google_api_key.is_empty(),
        _ => !settings.anthropic_api_key.is_empty(),
    }
}

/// Per-page character counts from pdftotext output (pages are separated by
/// form feeds). None when pdftotext is missing or fails — verification is
/// then skipped, not the extraction.
fn pdftotext_page_baseline(path: &Path) -> Option<Vec<usize>> {
    let bin = find_command("pdftotext")?;
    let mut command = StdCommand::new(&bin);
    command
        .env("PATH", env::full_path())
        .args(["-layout", path.to_str()?, "-"]);
    let output = run_bounded_output(
        command,
        "pdftotext baseline",
        std::time::Duration::from_secs(120),
        super::claude::MAX_STDOUT_BYTES,
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

/// Split pdftotext output on form feeds and return trimmed char counts.
fn baseline_page_lengths(text: &str) -> Vec<usize> {
    let mut pages: Vec<usize> = text.split('\u{0C}').map(|p| p.trim().len()).collect();
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

/// Re-request a specific page range that verification flagged.
#[allow(clippy::too_many_arguments)]
async fn repair_pages(
    app: &crate::emit::EventBus,
    path: &Path,
    prompt_path: &str,
    start: u32,
    end: u32,
    attach: bool,
    timeout_secs: u64,
    extra_dirs: &[&str],
) -> Result<Vec<(u32, String)>, String> {
    let span = if start == end {
        format!("page {start}")
    } else {
        format!("pages {start} through {end}")
    };
    let prompt = format!(
        "{} Transcribe ONLY {span} to well-formatted Markdown.\n\nRequirements:\n{}",
        source_line(attach, prompt_path),
        extraction_requirements()
    );
    let overrides = super::claude::LlmOverrides {
        pdf_attachment: attach.then_some(path),
        max_output_tokens: Some(EXTRACTION_MAX_OUTPUT_TOKENS),
        ..Default::default()
    };
    let label = format!("LLM extraction repair (pages {start}-{end})");
    let raw = super::claude::call_llm(
        app,
        &prompt,
        &["Read"],
        None,
        "text",
        timeout_secs,
        &label,
        None,
        None,
        extra_dirs,
        &overrides,
    )
    .await?;
    let text = strip_markdown_fence(&raw).to_string();
    match parse_page_sections(&text) {
        Some((_, sections)) if !sections.is_empty() => Ok(sections
            .into_iter()
            .filter(|(p, _)| *p >= start && *p <= end)
            .collect()),
        // A single-page repair without markers is still usable as-is.
        _ if start == end && !text.is_empty() => Ok(vec![(start, text)]),
        _ => Err("repair response contained no page markers".to_string()),
    }
}

/// Extract from a PDF using an LLM (Claude, Codex, or Gemini).
///
/// Direct-API providers get the PDF attached to the request; CLI providers
/// read it with their multimodal Read tool. The output is verified against
/// a pdftotext per-page baseline: pages that are missing or far too short
/// are re-requested once, and anything still failing becomes an
/// extraction-quality note instead of a silent gap.
async fn extract_llm(
    app: &crate::emit::EventBus,
    path: &Path,
    hash: &str,
) -> Result<ExtractionResult, String> {
    let path_str = path
        .to_str()
        .ok_or_else(|| format!("Path contains invalid UTF-8: {}", path.display()))?;
    // Normalize backslashes so the path Claude sees in the prompt matches
    // its internal POSIX-form normalization on Windows.
    let prompt_path = path_str.replace('\\', "/");
    // Grant Read access to the PDF's parent directory.  The cwd defaults
    // to the system temp dir, so without this Claude can't reach files
    // sitting under the user's Documents/Downloads/etc.
    let parent_dir = path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let extra_dirs: Vec<&str> = if parent_dir.is_empty() {
        vec![]
    } else {
        vec![&parent_dir]
    };

    let settings = crate::settings::load();
    let attach = provider_uses_direct_api(&settings);
    let timeout = (settings.step_timeout_secs / 2).max(60);

    // Completeness baseline (best-effort; poppler is bundled so this is
    // normally available).
    let baseline = {
        let p = path.to_path_buf();
        tokio::task::spawn_blocking(move || pdftotext_page_baseline(&p))
            .await
            .unwrap_or(None)
    };

    let pages_line = baseline
        .as_ref()
        .map(|b| format!("The document has {} pages.\n\n", b.len()))
        .unwrap_or_default();
    let prompt = format!(
        "{} Convert its entire contents to well-formatted Markdown.\n\n{pages_line}Requirements:\n{}\n\
         - If the full text cannot fit in your output, stop cleanly at a page boundary; the remaining pages will be requested separately",
        source_line(attach, &prompt_path),
        extraction_requirements()
    );

    let overrides = super::claude::LlmOverrides {
        pdf_attachment: attach.then_some(path),
        max_output_tokens: Some(EXTRACTION_MAX_OUTPUT_TOKENS),
        ..Default::default()
    };
    let raw = super::claude::call_llm(
        app,
        &prompt,
        &["Read"],
        None,
        "text",
        timeout,
        "LLM PDF extraction",
        None,
        None,
        &extra_dirs,
        &overrides,
    )
    .await?;
    let text = strip_markdown_fence(&raw).to_string();
    if text.is_empty() {
        return Err("LLM returned empty output for PDF extraction".to_string());
    }

    let mut quality_notes = Vec::new();
    let final_text = match (&baseline, parse_page_sections(&text)) {
        (Some(bl), Some((preamble, mut sections))) => {
            let suspects = find_suspect_pages(&sections, bl);
            if !suspects.is_empty() {
                let ranges = group_into_ranges(&suspects);
                if ranges.len() > MAX_REPAIR_RANGES {
                    let _ = app.emit_event("pipeline:log", serde_json::json!({
                        "line": format!(
                            "Extraction verification: {} suspect ranges, repairing the first {MAX_REPAIR_RANGES}",
                            ranges.len()
                        )
                    }));
                }
                for &(start, end) in ranges.iter().take(MAX_REPAIR_RANGES) {
                    if crate::commands::is_cancelled() {
                        return Err("Pipeline cancelled".into());
                    }
                    let _ = app.emit_event("pipeline:log", serde_json::json!({
                        "line": format!("Extraction verification: pages {start}-{end} missing or short, re-requesting")
                    }));
                    match repair_pages(
                        app,
                        path,
                        &prompt_path,
                        start,
                        end,
                        attach,
                        timeout,
                        &extra_dirs,
                    )
                    .await
                    {
                        Ok(repaired) => {
                            for (page, content) in repaired {
                                sections.insert(page, content);
                            }
                        }
                        Err(e) => {
                            let _ = app.emit_event("pipeline:log", serde_json::json!({
                                "line": format!("WARNING: repair of pages {start}-{end} failed: {e}")
                            }));
                        }
                    }
                }
                let still = find_suspect_pages(&sections, bl);
                if !still.is_empty() {
                    let list = still
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    quality_notes.push(format!(
                        "Extraction may be incomplete on page(s) {list}. \
                         Findings that depend on those pages should be verified against the original PDF."
                    ));
                }
            }
            rebuild_from_sections(&preamble, &sections)
        }
        (Some(_), None) => {
            quality_notes.push(
                "The extraction has no page markers, so completeness could not be verified against the PDF."
                    .to_string(),
            );
            text
        }
        (None, _) => {
            quality_notes.push(
                "pdftotext is unavailable, so extraction completeness was not verified."
                    .to_string(),
            );
            text
        }
    };
    quality_notes.extend(scan_math_quality(&final_text));

    Ok(ExtractionResult {
        text: final_text,
        method: "llm".to_string(),
        source_path: path.to_string_lossy().to_string(),
        paper_hash: hash.to_string(),
        quality_notes,
    })
}

/// Render each page of a PDF to a PNG in `out_dir` using bundled pdftoppm.
/// Returns the sorted file names. Used to populate the run's page-image
/// artifacts; independent of which extraction method ran.
pub fn render_pdf_pages(pdf: &Path, out_dir: &Path, max_pages: u32) -> Result<Vec<String>, String> {
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
    let mut command = StdCommand::new(&bin);
    command.env("PATH", env::full_path()).args([
        "-png",
        "-r",
        "150",
        "-l",
        &max_pages.to_string(),
        pdf_str,
        prefix_str,
    ]);
    let output = run_bounded_output(
        command,
        "pdftoppm",
        std::time::Duration::from_secs(crate::settings::load().step_timeout_secs.max(60)),
        1_000_000,
    )?;
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
        .filter(|n| n.starts_with("page") && n.ends_with(".png"))
        .collect();
    names.sort();
    if names.is_empty() {
        return Err("pdftoppm produced no page images".to_string());
    }
    Ok(names)
}

/// Extract from a PDF file using marker or pdftotext.
/// The "llm" setting is handled separately in `extract()` since it's async.
/// `method` should be the resolved effective extractor ("marker" or "pdftotext");
/// callers are expected to translate "auto" / "llm" upstream.
fn extract_pdf_native(
    path: &Path,
    method: &str,
    marker_disable_ocr: bool,
    marker_disable_images: bool,
) -> Result<ExtractionResult, String> {
    let hash = compute_hash(path)?;

    let try_marker = method == "marker";
    let try_pdftotext = method == "pdftotext";

    if try_marker {
        if find_command("marker_single").is_none() {
            return Err(
                "PDF extractor is set to 'marker' but marker_single is not installed. \
                        Install it from Settings → Text Extraction, or change the setting."
                    .to_string(),
            );
        }
        let text = extract_marker(path, &hash, marker_disable_ocr, marker_disable_images)?;
        let quality_notes = scan_math_quality(&text);
        return Ok(ExtractionResult {
            text,
            method: "marker".to_string(),
            source_path: path.to_string_lossy().to_string(),
            paper_hash: hash,
            quality_notes,
        });
    }

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
    let effective_marker_disable_ocr = extraction_cfg
        .marker_disable_ocr
        .unwrap_or(settings.marker_disable_ocr);
    let effective_marker_disable_images = extraction_cfg
        .marker_disable_images
        .unwrap_or(settings.marker_disable_images);

    // All providers can extract PDFs now: direct APIs get the PDF attached
    // to the request, CLIs read it with their multimodal Read tool.
    let use_llm = effective_method == "llm";

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
            match extract_llm(app, &pdf, &hash).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    // Fall back to native extraction unless the run was
                    // cancelled or there is nothing to fall back to.
                    if crate::commands::is_cancelled() || find_command("pdftotext").is_none() {
                        return Err(e);
                    }
                    let _ = app.emit_event("pipeline:log", serde_json::json!({
                        "line": format!("WARNING: LLM extraction failed: {e}. Falling back to pdftotext.")
                    }));
                    let p = pdf.clone();
                    return tokio::task::spawn_blocking(move || {
                        extract_pdf_native(&p, "pdftotext", false, false)
                    })
                    .await
                    .map_err(|e| format!("Extraction task failed: {e}"))?;
                }
            }
        }
    }

    // Identify the PDF path (if any) before entering spawn_blocking, so we
    // can fall back to LLM extraction if the native extractor fails.
    let fallback_pdf: Option<PathBuf> = if !use_llm {
        if path.is_dir() {
            if find_main_tex(&path).is_some() {
                None // LaTeX takes priority
            } else {
                find_pdf_in_dir(&path)
            }
        } else if ext_eq(&path, "pdf") {
            Some(path.clone())
        } else {
            None
        }
    } else {
        None
    };
    let extractor_name = effective_method.clone();
    let blocking_method = effective_method.clone();
    let blocking_disable_ocr = effective_marker_disable_ocr;
    let blocking_disable_images = effective_marker_disable_images;

    // Non-LLM paths: run blocking I/O on a separate thread
    let native_result = tokio::task::spawn_blocking(move || {
        if path.is_dir() {
            if let Some(tex_path) = find_main_tex(&path) {
                let hash = compute_hash(&tex_path)?;
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
                return Ok(ExtractionResult {
                    text,
                    method: "latex".to_string(),
                    source_path: tex_path.to_string_lossy().to_string(),
                    paper_hash: hash,
                    quality_notes: warnings,
                });
            }

            if let Some(pdf) = find_pdf_in_dir(&path) {
                return extract_pdf_native(
                    &pdf,
                    &blocking_method,
                    blocking_disable_ocr,
                    blocking_disable_images,
                );
            }

            return Err(format!("No .tex or .pdf files found in {}", path.display()));
        }

        if ext_eq(&path, "tex") {
            let hash = compute_hash(&path)?;
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
            Ok(ExtractionResult {
                text,
                method: "latex".to_string(),
                source_path: path.to_string_lossy().to_string(),
                paper_hash: hash,
                quality_notes: warnings,
            })
        } else if ext_eq(&path, "pdf") {
            extract_pdf_native(
                &path,
                &blocking_method,
                blocking_disable_ocr,
                blocking_disable_images,
            )
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

    // If native PDF extraction failed, fall back to LLM extraction
    match native_result {
        Ok(result) => Ok(result),
        Err(e) if fallback_pdf.is_some() => {
            let pdf = fallback_pdf.unwrap();
            let _ = app.emit_event("pipeline:log", serde_json::json!({
                "line": format!("WARNING: {extractor_name} failed: {e}. Falling back to LLM extraction.")
            }));
            let hash = {
                let p = pdf.clone();
                tokio::task::spawn_blocking(move || compute_hash(&p))
                    .await
                    .map_err(|e| format!("Hash computation failed: {e}"))??
            };
            extract_llm(app, &pdf, &hash).await
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let text = "page one text\u{0C}page two\u{0C}";
        let pages = baseline_page_lengths(text);
        assert_eq!(pages, vec!["page one text".len(), "page two".len()]);
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
    fn managed_install_beats_system_path() {
        // Regression guard for the engine-resolution contract: a marker
        // installed from Settings (~/.pipeline/bin) must be the copy that
        // runs, even when a system marker_single is on PATH.
        let dir = tempfile::tempdir().unwrap();
        let system = dir.path().join("marker_single");
        fs::write(&system, "#!/bin/sh\n").unwrap();
        let path_var = dir.path().to_string_lossy().to_string();

        // No managed install: PATH resolution finds the system copy.
        assert_eq!(
            resolve_command(None, &path_var, "marker_single"),
            Some(system.clone())
        );

        // Managed install present: it wins despite the system copy on PATH.
        let managed = PathBuf::from("/managed/bin/marker_single");
        assert_eq!(
            resolve_command(Some(managed.clone()), &path_var, "marker_single"),
            Some(managed)
        );
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

    let mut files: Vec<(String, u64)> = Vec::new();
    let mut stack = vec![root_path.clone()];
    let mut truncated = false;
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue, // unreadable subdir: skip, don't fail the run
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push(path);
                }
            } else if let Ok(meta) = entry.metadata() {
                if files.len() >= MAX_INVENTORY_FILES {
                    truncated = true;
                    continue;
                }
                let rel = path
                    .strip_prefix(&root_path)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push((rel, meta.len()));
            }
        }
    }
    files.sort();

    let root_display = root.replace('\\', "/");
    let mut text = format!(
        "# Input folder inventory\n\nRoot: {root_display}\n\n{} files. File contents are NOT included here — use the Read tool with paths under the root to open any file you need.\n\n| File | Bytes |\n|---|---|\n",
        files.len()
    );
    for (rel, size) in &files {
        text.push_str(&format!("| {rel} | {size} |\n"));
    }
    if truncated {
        text.push_str(&format!(
            "\n> Inventory truncated at {MAX_INVENTORY_FILES} files.\n"
        ));
    }

    let hash = format!("{:x}", Sha256::digest(text.as_bytes()))[..16].to_string();
    Ok(ExtractionResult {
        text,
        method: "folder".to_string(),
        source_path: root.to_string(),
        paper_hash: hash,
        quality_notes: vec![],
    })
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
    fn none_mode_produces_placeholder_context() {
        let result = ingest_none();
        assert_eq!(result.method, "none");
        assert!(result.source_path.is_empty());
        assert_eq!(result.paper_hash.len(), 16);
    }
}
