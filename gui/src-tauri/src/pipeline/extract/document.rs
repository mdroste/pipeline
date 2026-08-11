use super::*;

/// Extract from a PDF file using a supported native extractor.
/// The "llm" setting is handled separately in `extract()` since it's async.
/// `method` should be the resolved effective extractor; callers are expected
/// to translate "auto" / "llm" upstream.
pub(super) fn extract_pdf_native(
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
pub(super) fn scan_math_quality(text: &str) -> Vec<String> {
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
pub(super) fn find_pdf_in_dir(dir: &Path) -> Option<PathBuf> {
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
pub(super) fn find_docx_in_dir(dir: &Path) -> Option<PathBuf> {
    let mut documents: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| ext_eq(path, "docx"))
        .collect();
    documents.sort();
    documents.into_iter().next()
}

pub(super) fn extract_docx(path: &Path) -> Result<ExtractionResult, String> {
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
