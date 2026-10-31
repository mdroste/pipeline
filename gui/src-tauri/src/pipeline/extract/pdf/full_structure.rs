//! Full-parser structure rendering and page-completeness validation.

use super::super::core::{MAX_RENDERED_PDF_PAGES, PADDLE_STRUCTURE_SCHEMA};
use super::super::document::scan_math_quality;
use super::super::structured::{PaddleStructure, PaddleStructuredPage};
use super::page_quality::{
    paddle_char_count_is_suspicious, paddle_substantive_chars, PdfPageTextBaseline,
    SUSPECT_BASELINE_MIN_CHARS,
};
use crate::models::ExtractionResult;
use std::path::Path;

/// Mirrors `FIGURE_ONLY_MIN_PAGE_SHARE` in the parser sidecar, which applies
/// the same rule before deciding whether to replace a page's layout result.
const FIGURE_ONLY_MIN_PAGE_SHARE: f64 = 0.25;

/// A page without figures below this share of its text layer gets a quality
/// note. Plot labels make the comparison meaningless on figure pages, and
/// recognized math markup only raises the count, so ordinary pages sit well
/// above it unless text was really lost.
const PARTIAL_PAGE_MIN_COVERAGE_PERCENT: usize = 80;

/// Mirrors `UNVERIFIED_IMAGE_TEXT_KEY` in the parser sidecar, which moves image
/// recognition that looks made up out of its block's text and into this key of
/// the block's raw record.
const UNVERIFIED_IMAGE_TEXT_KEY: &str = "unverified_image_text";
const WITHHELD_IMAGE_TEXT_NOTE: &str = "Image text withheld as unverified";

/// Withheld image text leaves a figure as complete as any `chart` region, which
/// is never recognized, so this note is reported without degrading the run.
pub(crate) fn is_withheld_image_text_note(note: &str) -> bool {
    note.starts_with(WITHHELD_IMAGE_TEXT_NOTE)
}

fn withheld_image_text_note(structure: &PaddleStructure) -> Option<String> {
    let pages = structure
        .pages
        .iter()
        .filter_map(|page| {
            let regions = page
                .blocks
                .iter()
                .filter(|block| block.raw.get(UNVERIFIED_IMAGE_TEXT_KEY).is_some())
                .count();
            (regions > 0).then_some((page.number, regions))
        })
        .collect::<Vec<_>>();
    if pages.is_empty() {
        return None;
    }
    let regions = pages.iter().map(|(_, regions)| regions).sum::<usize>();
    let pages = pages
        .iter()
        .map(|(page, _)| page.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "{WITHHELD_IMAGE_TEXT_NOTE}: {regions} image region(s) on page(s) {pages} returned the kind of data the recognizer makes up for plots. Read those figures from their images."
    ))
}

fn is_figure_region(role: &str) -> bool {
    matches!(
        role,
        "chart" | "image" | "figure" | "chart_body" | "image_body" | "picture"
    )
}

/// Figures are the only content the layout model found on the page, so its
/// text layer holds plot labels that remain visible in the page image.
fn paddle_page_is_figure_only(page: &PaddleStructuredPage) -> bool {
    let (Some(width), Some(height)) = (page.width, page.height) else {
        return false;
    };
    let mut figure_area = 0.0;
    for block in &page.blocks {
        let role = block.role.to_ascii_lowercase();
        if is_figure_region(&role) {
            if let [left, top, right, bottom] = block.bbox[..] {
                figure_area += (right - left).max(0.0) * (bottom - top).max(0.0);
            }
        } else if !matches!(
            role.as_str(),
            "figure_title"
                | "number"
                | "header"
                | "header_image"
                | "footer"
                | "footer_image"
                | "paragraph_title"
                | "vision_footnote"
        ) {
            return false;
        }
    }
    figure_area >= FIGURE_ONLY_MIN_PAGE_SHARE * f64::from(width) * f64::from(height)
}

fn paddle_page_is_figure_related(page: &PaddleStructuredPage) -> bool {
    let mut has_figure_region = false;
    for block in &page.blocks {
        match block.role.to_ascii_lowercase().as_str() {
            "table" | "table_title" => return false,
            "chart" | "image" | "figure" | "figure_title" | "chart_body" | "image_body"
            | "picture" => has_figure_region = true,
            _ => {}
        }
    }
    has_figure_region
}

fn paddle_effective_baseline(
    page: &PaddleStructuredPage,
    baseline: PdfPageTextBaseline,
) -> (usize, bool) {
    if paddle_page_is_figure_only(page) {
        return (0, false);
    }
    let fragmented_visual_page = baseline.total_chars >= SUSPECT_BASELINE_MIN_CHARS
        && baseline.prose_chars < baseline.total_chars / 2
        && paddle_page_is_figure_related(page);
    if fragmented_visual_page {
        (baseline.prose_chars, true)
    } else {
        (baseline.total_chars, false)
    }
}

fn fragmented_visual_baseline_note(page: u32, baseline: PdfPageTextBaseline) -> String {
    format!(
        "Page {page}: the PDF text layer is dominated by isolated figure labels; completeness was checked against {} prose-like characters rather than {} total characters.",
        baseline.prose_chars, baseline.total_chars
    )
}

fn page_lost_text(
    page: &PaddleStructuredPage,
    recognized_chars: usize,
    total_chars: usize,
) -> bool {
    total_chars >= SUSPECT_BASELINE_MIN_CHARS
        && recognized_chars.saturating_mul(100)
            < total_chars.saturating_mul(PARTIAL_PAGE_MIN_COVERAGE_PERCENT)
        && !page
            .blocks
            .iter()
            .any(|block| is_figure_region(&block.role.to_ascii_lowercase()))
}

pub(in crate::pipeline::extract) fn paddle_full_block_markdown(
    page: &PaddleStructuredPage,
) -> String {
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

pub(in crate::pipeline::extract) fn paddle_full_best_markdown(
    page: &PaddleStructuredPage,
) -> String {
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

pub(in crate::pipeline::extract) fn render_paddle_full_page(page: &PaddleStructuredPage) -> String {
    let markdown = paddle_full_best_markdown(page);
    let markdown = markdown
        .replace("](assets/", "](../artifacts/figures/")
        .replace("src=\"assets/", "src=\"../artifacts/figures/")
        .replace("src='assets/", "src='../artifacts/figures/");
    format!("<!-- PAGE {} -->\n\n{}", page.number, markdown)
        .trim()
        .to_string()
}

pub(in crate::pipeline::extract) fn paddle_full_page_has_content(
    page: &PaddleStructuredPage,
) -> bool {
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

pub(in crate::pipeline::extract) fn full_parser_extraction_from_structure(
    structure: &PaddleStructure,
    path: &Path,
    hash: &str,
    baseline: Option<&[PdfPageTextBaseline]>,
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
    let mut coverage_notes = Vec::new();
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
        let page_baseline = baseline.and_then(|pages| pages.get(index)).copied();
        let (baseline_chars, used_fragmented_visual_baseline) = page_baseline
            .map(|value| paddle_effective_baseline(page, value))
            .unwrap_or((0, false));
        if paddle_char_count_is_suspicious(recognized_chars, Some(baseline_chars)) {
            if used_fragmented_visual_baseline {
                return Err(format!(
                    "PaddleOCR-VL full parser returned incomplete output for page {expected} (recognized {recognized_chars} substantive characters; prose-like text baseline has {baseline_chars}; full text layer has {})",
                    page_baseline.map_or(0, |value| value.total_chars)
                ));
            }
            return Err(format!(
                "PaddleOCR-VL full parser returned incomplete output for page {expected} (recognized {recognized_chars} substantive characters; text layer has {baseline_chars})"
            ));
        }
        if used_fragmented_visual_baseline
            && paddle_char_count_is_suspicious(
                recognized_chars,
                page_baseline.map(|value| value.total_chars),
            )
        {
            coverage_notes.push(fragmented_visual_baseline_note(
                expected,
                page_baseline.unwrap_or_default(),
            ));
        }
        if let Some(total_chars) = page_baseline
            .map(|value| value.total_chars)
            .filter(|total_chars| page_lost_text(page, recognized_chars, *total_chars))
        {
            coverage_notes.push(format!(
                "Page {expected}: recognized {recognized_chars} of {total_chars} characters in the PDF text layer; some text may be missing, so consult the rendered page."
            ));
        }
    }
    let text = structure
        .pages
        .iter()
        .map(render_paddle_full_page)
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.len() > crate::pipeline::claude::MAX_STDOUT_BYTES {
        return Err(
            "PaddleOCR-VL full-parser document exceeded the 50 MB safety limit".to_string(),
        );
    }
    let mut quality_notes = structure.quality_notes.clone();
    for note in coverage_notes {
        if !quality_notes.contains(&note) {
            quality_notes.push(note);
        }
    }
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
    quality_notes.extend(withheld_image_text_note(structure));
    quality_notes.extend(scan_math_quality(&text));
    Ok(ExtractionResult {
        text,
        method: "paddleocr-vl-full".to_string(),
        source_path: path.to_string_lossy().to_string(),
        paper_hash: hash.to_string(),
        quality_notes,
    })
}
