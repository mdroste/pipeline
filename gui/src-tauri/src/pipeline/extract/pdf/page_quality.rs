//! Pure page baselines, completeness checks, markers, and bounded ranges.

use super::super::core::PADDLE_REGION_WARNING_PREFIX;
use super::llm::{LLM_CHUNK_MAX_PAGES, LLM_CHUNK_TARGET_BASELINE_CHARS};
use regex::Regex;

pub(in crate::pipeline::extract) fn paddle_substantive_chars(text: &str) -> usize {
    text.lines()
        .filter(|line| !line.contains(PADDLE_REGION_WARNING_PREFIX))
        .flat_map(str::chars)
        .filter(|value| !value.is_whitespace())
        .count()
}

pub(in crate::pipeline::extract) fn paddle_char_count_is_suspicious(
    extracted_chars: usize,
    baseline_len: Option<usize>,
) -> bool {
    baseline_len.is_some_and(|baseline_len| {
        baseline_len >= SUSPECT_BASELINE_MIN_CHARS && extracted_chars < baseline_len / 10
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::pipeline::extract) struct PdfPageTextBaseline {
    pub(in crate::pipeline::extract) total_chars: usize,
    pub(in crate::pipeline::extract) prose_chars: usize,
}

/// A page's extraction is suspect when the pdftotext baseline has at least
/// this many characters but the LLM produced less than a quarter of it.
/// The floor keeps figure-heavy pages (thin text layer) from tripping it.
pub(in crate::pipeline::extract) const SUSPECT_BASELINE_MIN_CHARS: usize = 200;

/// Split pdftotext output on form feeds and count substantive characters.
/// `-layout` can emit thousands of alignment spaces on a sparse figure page;
/// counting those as source text makes complete OCR look implausibly short.
#[cfg(test)]
pub(in crate::pipeline::extract) fn baseline_page_lengths(text: &str) -> Vec<usize> {
    baseline_page_metrics(text)
        .into_iter()
        .map(|page| page.total_chars)
        .collect()
}

/// Measure both the full text layer and ordinary prose-like lines. Plot
/// labels are commonly emitted as thousands of isolated words; they are a
/// useful visual annotation but not a sound completeness target for a
/// layout-aware OCR parser that preserves the rendered page as visual evidence.
pub(in crate::pipeline::extract) fn baseline_page_metrics(text: &str) -> Vec<PdfPageTextBaseline> {
    let mut pages: Vec<PdfPageTextBaseline> = text
        .split('\u{0C}')
        .map(|page| {
            let total_chars = page.chars().filter(|value| !value.is_whitespace()).count();
            let prose_chars = page
                .lines()
                .map(|line| {
                    let chars = line.chars().filter(|value| !value.is_whitespace()).count();
                    let words = line.split_whitespace().count();
                    let alphabetic_words = line
                        .split_whitespace()
                        .filter(|word| {
                            word.chars().filter(|value| value.is_alphabetic()).count() >= 2
                        })
                        .count();
                    if words >= 3 && alphabetic_words >= 2 && chars >= 12 {
                        chars
                    } else {
                        0
                    }
                })
                .sum();
            PdfPageTextBaseline {
                total_chars,
                prose_chars,
            }
        })
        .collect();
    // pdftotext terminates every page with a form feed, leaving a trailing
    // empty segment.
    if pages.last().is_some_and(|page| page.total_chars == 0) {
        pages.pop();
    }
    pages
}

pub(in crate::pipeline::extract) fn page_marker_regex() -> Regex {
    Regex::new(r"(?i)<!--\s*page\s+(\d+)\s*-->").expect("page marker regex is invalid")
}

/// Split extracted text on `<!-- PAGE n -->` markers into (preamble,
/// page → content). Returns None when the text has no markers at all, in
/// which case completeness cannot be verified. Content under a repeated
/// marker is appended, not replaced.
pub(in crate::pipeline::extract) fn parse_page_sections(
    text: &str,
) -> Option<(String, std::collections::BTreeMap<u32, String>)> {
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
pub(in crate::pipeline::extract) fn find_suspect_pages(
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
pub(in crate::pipeline::extract) fn group_into_ranges(pages: &[u32]) -> Vec<(u32, u32)> {
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
pub(in crate::pipeline::extract) fn rebuild_from_sections(
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
#[cfg(test)]
pub(in crate::pipeline::extract) fn strip_markdown_fence(text: &str) -> &str {
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

pub(in crate::pipeline::extract) fn llm_initial_ranges(baseline: &[usize]) -> Vec<(u32, u32)> {
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

pub(in crate::pipeline::extract) fn split_ranges(
    ranges: &[(u32, u32)],
    max_pages: u32,
) -> Vec<(u32, u32)> {
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
