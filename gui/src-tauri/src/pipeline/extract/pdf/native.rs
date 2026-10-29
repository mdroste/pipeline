//! Bounded native PDF extraction and text-layer metrics.

use super::super::core::{run_bounded_output, stage_pdf_input};
use super::page_quality::{baseline_page_metrics, PdfPageTextBaseline};
use crate::env;
use std::path::Path;

/// Extract text from PDF using pdftotext. The child reads a staged private
/// copy, never the original path (macOS TCC: helper children reading the
/// user's Documents/Desktop need their own folder permission; the in-process
/// staging copy is covered by the file-picker grant).
pub(crate) fn extract_pdftotext(path: &Path) -> Result<String, String> {
    let staging = tempfile::Builder::new()
        .prefix("pipeline_pdf_text_input_")
        .tempdir()
        .map_err(|error| format!("Failed to create private PDF input directory: {error}"))?;
    let staged = stage_pdf_input(path, staging.path())?;
    let path_str = staged
        .to_str()
        .ok_or_else(|| format!("Path contains invalid UTF-8: {}", staged.display()))?;
    let pdftotext_bin = find_command("pdftotext").ok_or("pdftotext not found on PATH")?;
    let mut command = pdftotext_bin.command(["-layout", path_str, "-"]);
    command.env("PATH", env::full_path());
    let output = run_bounded_output(
        command,
        "pdftotext",
        std::time::Duration::from_secs(crate::settings::load().step_timeout_secs.max(60)),
        crate::pipeline::claude::MAX_STDOUT_BYTES,
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

pub(in crate::pipeline::extract) fn find_command(
    name: &str,
) -> Option<crate::deps::ResolvedCommand> {
    crate::deps::resolve_command(name)
}

/// Per-page character counts from pdftotext output (pages are separated by
/// form feeds). An error names the actual cause — missing binary, non-zero
/// exit with its stderr (an encrypted PDF says "Incorrect password" here),
/// or truncated output — so callers can surface it instead of a generic
/// "could not build the page map".
pub(in crate::pipeline::extract) fn pdftotext_page_metrics(
    path: &Path,
) -> Result<Vec<PdfPageTextBaseline>, String> {
    let bin = find_command("pdftotext")
        .ok_or_else(|| "pdftotext is not available on PATH".to_string())?;
    let path_str = path
        .to_str()
        .ok_or_else(|| format!("Path contains invalid UTF-8: {}", path.display()))?;
    let mut command = bin.command(["-layout", path_str, "-"]);
    command.env("PATH", env::full_path());
    let output = run_bounded_output(
        command,
        "pdftotext baseline",
        std::time::Duration::from_secs(120),
        crate::pipeline::claude::MAX_STDOUT_BYTES,
        None,
    )?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "pdftotext failed ({}): {}",
            output.status,
            stderr.trim()
        ));
    }
    if output.stdout_truncated {
        return Err("pdftotext baseline output exceeded the 50 MB safety limit".to_string());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let pages = baseline_page_metrics(&text);
    if pages.is_empty() {
        Err("pdftotext reported no pages".to_string())
    } else {
        Ok(pages)
    }
}

pub(in crate::pipeline::extract) fn pdftotext_page_baseline(
    path: &Path,
) -> Result<Vec<usize>, String> {
    pdftotext_page_metrics(path)
        .map(|pages| pages.into_iter().map(|page| page.total_chars).collect())
}
