use crate::env;
use crate::models::ExtractionResult;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use tauri::{AppHandle, Emitter};

/// Case-insensitive extension check (handles .PDF, .Tex, etc.).
fn ext_eq(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

/// Compute SHA-256 hash of file contents, first 16 hex chars.
fn compute_hash(path: &Path) -> Result<String, String> {
    let data = fs::read(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let hash = Sha256::digest(&data);
    Ok(format!("{:x}", hash)[..16].to_string())
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
fn extract_latex(path: &Path, depth: usize, root_dir: &Path, warnings: &mut Vec<String>) -> Result<String, String> {
    if depth > 10 {
        warnings.push("LaTeX \\input{} nesting exceeds 10 levels — possible circular includes. Output may be incomplete.".to_string());
        return Ok(String::new());
    }

    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

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
        let safe = canonical.as_ref().map_or(false, |resolved| {
            resolved.starts_with(root_dir)
        });

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

/// Extract text from PDF using marker_single.
/// Timeout is half the user's step timeout, floored at 120s to accommodate first-run model downloads.
fn extract_marker(path: &Path) -> Result<String, String> {
    use std::io::Read;
    use std::process::Stdio;

    let path_str = path.to_str()
        .ok_or_else(|| format!("Path contains invalid UTF-8: {}", path.display()))?;
    let settings = crate::settings::load();
    let mut marker_args = vec![
        path_str.to_string(),
        "--output_format".to_string(),
        "markdown".to_string(),
    ];
    if settings.marker_disable_images {
        marker_args.push("--disable_image_extraction".to_string());
    }
    if settings.marker_disable_ocr {
        marker_args.push("--disable_ocr".to_string());
    }
    let marker_bin = find_command("marker_single")
        .ok_or("marker_single not found on PATH")?;
    let mut child = StdCommand::new(&marker_bin)
        .env("PATH", env::full_path())
        .args(&marker_args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("marker_single not available: {e}"))?;

    let pid = child.id();
    if pid > 0 { crate::commands::register_child_pid(pid); }

    // Drain stdout and stderr in separate threads to prevent pipe buffer deadlocks.
    // Blocking reads on the same thread can deadlock if the child fills one pipe
    // while we're blocked reading the other.
    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();

    let stdout_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(out) = stdout_handle {
            // Safety cap: marker_single output is bounded by paper size,
            // but guard against pathological inputs.
            let _ = out.take(super::claude::MAX_STDOUT_BYTES as u64).read_to_end(&mut buf);
        }
        buf
    });
    let stderr_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(err) = stderr_handle {
            // Mirror stdout cap so a misbehaving marker_single can't balloon stderr.
            let _ = err.take(super::claude::MAX_STDOUT_BYTES as u64).read_to_end(&mut buf);
        }
        buf
    });

    // Wait with timeout — use half the user's step timeout (same convention as LLM extraction),
    // with a floor of 120s to handle first-run model downloads.
    let timeout_secs = (settings.step_timeout_secs / 2).max(120);
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let start = std::time::Instant::now();
    let exit_status;
    loop {
        match child.try_wait() {
            Ok(Some(s)) => { exit_status = s; break; }
            Ok(None) => {
                if crate::commands::is_cancelled() {
                    let _ = child.kill();
                    let _ = child.wait();
                    if pid > 0 { crate::commands::unregister_child_pid(pid); }
                    return Err("Pipeline cancelled".to_string());
                }
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    if pid > 0 { crate::commands::unregister_child_pid(pid); }
                    return Err(format!("marker_single timed out after {}s", timeout_secs));
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            Err(e) => {
                if pid > 0 { crate::commands::unregister_child_pid(pid); }
                return Err(format!("Failed waiting for marker_single: {e}"));
            }
        }
    }

    if pid > 0 { crate::commands::unregister_child_pid(pid); }
    let stdout_buf = stdout_thread.join().unwrap_or_default();
    let stderr_buf = stderr_thread.join().unwrap_or_default();
    if !exit_status.success() {
        let stderr = String::from_utf8_lossy(&stderr_buf);
        return Err(format!("marker_single failed: {}", stderr.trim()));
    }

    let text = String::from_utf8_lossy(&stdout_buf).trim().to_string();
    if text.is_empty() {
        return Err("marker_single returned empty output".to_string());
    }
    Ok(text)
}

/// Extract text from PDF using pdftotext.
fn extract_pdftotext(path: &Path) -> Result<String, String> {
    let path_str = path.to_str()
        .ok_or_else(|| format!("Path contains invalid UTF-8: {}", path.display()))?;
    let pdftotext_bin = find_command("pdftotext")
        .ok_or("pdftotext not found on PATH")?;
    let output = StdCommand::new(&pdftotext_bin)
        .env("PATH", env::full_path())
        .args(["-layout", path_str, "-"])
        .output()
        .map_err(|e| format!("pdftotext not available: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("pdftotext failed: {}", stderr.trim()));
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err("pdftotext returned empty output".to_string());
    }
    Ok(text)
}

/// Find a command on PATH by scanning directories directly (no subprocess).
/// Returns the full path if found.
fn find_command(name: &str) -> Option<PathBuf> {
    let path_var = env::full_path();
    let sep = if cfg!(windows) { ';' } else { ':' };
    for dir in path_var.split(sep) {
        if dir.is_empty() { continue; }
        let candidate = PathBuf::from(dir).join(name);
        if candidate.is_file() { return Some(candidate); }
        #[cfg(windows)]
        for ext in &[".exe", ".cmd", ".bat"] {
            let with_ext = PathBuf::from(format!("{}{ext}", candidate.display()));
            if with_ext.is_file() { return Some(with_ext); }
        }
    }
    None
}

/// Extract from a PDF using an LLM (Claude, Codex, or Gemini).
/// The LLM reads the PDF directly and outputs well-formatted Markdown.
async fn extract_llm(app: &AppHandle, path: &Path, hash: &str) -> Result<ExtractionResult, String> {
    let path_str = path.to_str()
        .ok_or_else(|| format!("Path contains invalid UTF-8: {}", path.display()))?;

    let prompt = format!(
        "Read the PDF file at {} and convert its entire contents to well-formatted Markdown.\n\n\
         Requirements:\n\
         - Reproduce ALL text content from the paper, including abstract, all sections, footnotes, references, and appendices\n\
         - Preserve mathematical notation using LaTeX syntax (inline $...$ and display $$...$$)\n\
         - Format tables using markdown table syntax\n\
         - Note figure/table captions and their numbers\n\
         - Preserve section numbering and hierarchy\n\
         - Do not summarize or skip any content — output the complete text\n\
         - Do not add commentary, analysis, or annotations — just the extracted text",
        path_str
    );

    let timeout = (crate::settings::load().step_timeout_secs / 2).max(60);
    let text = super::claude::call_llm(
        app,
        &prompt,
        &["Read"],
        None,
        "text",
        timeout,
        "LLM PDF extraction",
        None,
        None,
    )
    .await?;

    if text.is_empty() {
        return Err("LLM returned empty output for PDF extraction".to_string());
    }

    Ok(ExtractionResult {
        text,
        method: "llm".to_string(),
        source_path: path.to_string_lossy().to_string(),
        paper_hash: hash.to_string(),
        quality_notes: vec![],
    })
}

/// Extract from a PDF file using marker or pdftotext.
/// The "llm" setting is handled separately in `extract()` since it's async.
fn extract_pdf_native(path: &Path) -> Result<ExtractionResult, String> {
    let hash = compute_hash(path)?;
    let settings = crate::settings::load();
    let pref = settings.pdf_extractor.as_str();

    let try_marker = pref == "marker";
    let try_pdftotext = pref == "pdftotext";

    if try_marker {
        if find_command("marker_single").is_none() {
            return Err("PDF extractor is set to 'marker' but marker_single is not installed. \
                        Install marker-pdf (pip install marker-pdf) or change the setting.".to_string());
        }
        let text = extract_marker(path)?;
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
        let mut quality_notes = vec![
            "Text extracted via pdftotext. Equations will be garbled. \
             Technical findings should be verified against the original PDF."
                .to_string(),
        ];
        quality_notes.extend(scan_math_quality(&text));
        return Ok(ExtractionResult {
            text,
            method: "pdftotext".to_string(),
            source_path: path.to_string_lossy().to_string(),
            paper_hash: hash,
            quality_notes,
        });
    }

    Err(
        "Unknown PDF extraction method. Check your settings."
            .to_string(),
    )
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
        .filter(|w| w.chars().count() == 1 && w.chars().next().map(|c| !c.is_ascii()).unwrap_or(false))
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
pub async fn extract(app: &AppHandle, paper_path: &str) -> Result<ExtractionResult, String> {
    let path = PathBuf::from(paper_path);

    if !path.exists() {
        return Err(format!("File not found: {paper_path}"));
    }

    // Determine if this is a PDF that should use LLM extraction.
    // LLM extraction relies on the Read tool returning PDF content, which
    // only works for Anthropic (document blocks) and CLI subprocesses
    // (native multimodal Read).  OpenAI and Google direct-API paths cannot
    // return binary content in tool results, so we fall back to native
    // extraction for those providers.
    let settings = crate::settings::load();
    let pdf_read_supported = match settings.preferred_provider.as_str() {
        "codex" if !settings.openai_api_key.is_empty() => false,
        "gemini" if !settings.google_api_key.is_empty() => false,
        _ => true,
    };
    let use_llm = settings.pdf_extractor == "llm" && pdf_read_supported;

    if settings.pdf_extractor == "llm" && !pdf_read_supported {
        let _ = app.emit("pipeline:log", serde_json::json!({
            "line": format!(
                "NOTE: LLM PDF extraction is not available with the {} direct API. Using native extraction. \
                 To use LLM extraction, switch to the CLI provider or use the Anthropic API.",
                settings.preferred_provider
            )
        }));
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
            return extract_llm(app, &pdf, &hash).await;
        }
    }

    // Identify the PDF path (if any) before entering spawn_blocking, so we
    // can fall back to LLM extraction if the native extractor fails.
    // Only set the fallback when the provider actually supports PDF reads.
    let fallback_pdf: Option<PathBuf> = if !use_llm && pdf_read_supported {
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
    let extractor_name = settings.pdf_extractor.clone();

    // Non-LLM paths: run blocking I/O on a separate thread
    let native_result = tokio::task::spawn_blocking(move || {
        if path.is_dir() {
            if let Some(tex_path) = find_main_tex(&path) {
                let hash = compute_hash(&tex_path)?;
                // Must be canonical: extract_latex compares it against canonicalized
                // include paths via starts_with. A non-canonical root silently defeats
                // the path-traversal check.
                let root_dir = path.canonicalize()
                    .map_err(|e| format!("Failed to canonicalize project dir {}: {e}", path.display()))?;
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
                return extract_pdf_native(&pdf);
            }

            return Err(format!(
                "No .tex or .pdf files found in {}",
                path.display()
            ));
        }

        if ext_eq(&path, "tex") {
            let hash = compute_hash(&path)?;
            // Must be canonical: extract_latex compares it against canonicalized
            // include paths via starts_with. A non-canonical root silently defeats
            // the path-traversal check.
            let parent = path.parent().unwrap_or(Path::new("."));
            let root_dir = parent.canonicalize()
                .map_err(|e| format!("Failed to canonicalize parent dir {}: {e}", parent.display()))?;
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
            extract_pdf_native(&path)
        } else {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("(none)");
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
            let _ = app.emit("pipeline:log", serde_json::json!({
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
}
