use super::*;

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
/// following page exists. Rendering the requested page, its successor, and one
/// additional probe in a single bounded Poppler call lets the frontend reuse
/// the successor without another Poppler startup while still knowing whether
/// navigation can continue from that prefetched page.
pub struct RenderedPdfPagePreview {
    pub name: String,
    pub has_next: bool,
    pub next_name: Option<String>,
    pub next_has_next: bool,
}

pub(super) fn rendered_page_limit(requested: u32) -> u32 {
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
        .saturating_add(2)
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
    let next_name = page.checked_add(1).and_then(|next_page| {
        names
            .iter()
            .find(|name| page_number(name) == Some(next_page))
            .cloned()
    });
    let next_has_next = page.checked_add(2).is_some_and(|probe_page| {
        names
            .iter()
            .any(|name| page_number(name) == Some(probe_page))
    });
    Ok(RenderedPdfPagePreview {
        name,
        has_next: next_name.is_some(),
        next_name,
        next_has_next,
    })
}

pub(super) fn render_pdf_pages_with_profile(
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
