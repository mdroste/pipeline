//! Bounded manuscript excerpts for orientation and their saved coverage ranges.

const MAX_PAPER_TEXT: usize = 250_000;
pub(crate) const ORIENTATION_EXCERPTS_NOTICE: &str =
    "Orientation used excerpts to plan the review because the full document text exceeded this step’s length limit.";
pub(crate) const INDEXED_ORIENTATION_NOTICE: &str =
    "Orientation indexed the whole document and read selected passages in detail to plan the review.";
pub(crate) const ORIENTATION_SAMPLING_PATH: &str = "context/orientation_sampling.json";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct OrientationSampleRange {
    pub byte_start: usize,
    pub byte_end: usize,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct OrientationSample {
    pub schema_version: u32,
    pub original_bytes: usize,
    pub included_bytes: usize,
    pub omitted_bytes: usize,
    pub truncated: bool,
    pub ranges: Vec<OrientationSampleRange>,
    #[serde(skip)]
    pub text: String,
}

pub(super) fn char_boundary_before(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn normalize_sample_ranges(mut ranges: Vec<OrientationSampleRange>) -> Vec<OrientationSampleRange> {
    ranges.sort_by_key(|range| range.byte_start);
    let mut normalized: Vec<OrientationSampleRange> = Vec::new();
    for range in ranges {
        if let Some(previous) = normalized.last_mut() {
            if range.byte_start <= previous.byte_end {
                previous.byte_end = previous.byte_end.max(range.byte_end);
                for reason in range.reasons {
                    if !previous.reasons.contains(&reason) {
                        previous.reasons.push(reason);
                    }
                }
                continue;
            }
        }
        normalized.push(range);
    }
    normalized
}

/// Deterministically sample a long document across its beginning, section
/// starts, evenly spaced interior positions, and tail. Use spare capacity to
/// join nearby excerpts so that more passages can be read in context. Persist
/// the ranges with the run so orientation coverage is reproducible.
pub fn orientation_sample(text: &str) -> OrientationSample {
    if text.len() <= MAX_PAPER_TEXT {
        return OrientationSample {
            schema_version: 1,
            original_bytes: text.len(),
            included_bytes: text.len(),
            omitted_bytes: 0,
            truncated: false,
            ranges: vec![OrientationSampleRange {
                byte_start: 0,
                byte_end: text.len(),
                reasons: vec!["complete_document".to_string()],
            }],
            text: text.to_string(),
        };
    }

    let range = |start: usize, end: usize, reason: String| OrientationSampleRange {
        byte_start: char_boundary_before(text, start),
        byte_end: char_boundary_before(text, end).max(char_boundary_before(text, start)),
        reasons: vec![reason],
    };
    let mut ranges = vec![
        range(0, 50_000, "document_start".to_string()),
        range(
            text.len().saturating_sub(40_000),
            text.len(),
            "document_tail".to_string(),
        ),
    ];

    let mut major_headings = Vec::new();
    let mut other_headings = Vec::new();
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        let keyword_heading = [
            "abstract",
            "introduction",
            "background",
            "method",
            "methods",
            "data",
            "results",
            "discussion",
            "conclusion",
            "conclusions",
            "appendix",
            "references",
        ]
        .iter()
        .any(|keyword| {
            lower == *keyword
                || lower.ends_with(&format!(" {keyword}"))
                || lower.starts_with(&format!("{keyword} "))
                || lower.starts_with(&format!("{keyword}:"))
        });
        let markdown_heading = trimmed.starts_with('#') && trimmed.len() <= 240;
        if (markdown_heading || keyword_heading) && !trimmed.is_empty() {
            let heading = (offset, trimmed.chars().take(120).collect::<String>());
            if trimmed.starts_with("# ") || trimmed.starts_with("## ") {
                major_headings.push(heading);
            } else {
                other_headings.push(heading);
            }
        }
        offset += line.len();
    }
    let major_limit = major_headings.len().min(24);
    let headings = spaced_headings(major_headings, major_limit)
        .chain(spaced_headings(other_headings, 24 - major_limit));
    for (offset, heading) in headings {
        let candidate = range(
            offset,
            offset.saturating_add(6_000),
            format!("section_start:{heading}"),
        );
        let mut proposed = ranges.clone();
        proposed.push(candidate);
        let proposed = normalize_sample_ranges(proposed);
        let bytes = proposed
            .iter()
            .map(|range| range.byte_end.saturating_sub(range.byte_start))
            .sum::<usize>();
        if bytes <= MAX_PAPER_TEXT {
            ranges = proposed;
        }
    }
    for (index, fraction) in [1usize, 2, 3, 4].into_iter().enumerate() {
        let center = text.len().saturating_mul(fraction) / 5;
        let candidate = range(
            center.saturating_sub(2_500),
            center.saturating_add(2_500),
            format!("stratified_interior_{}", index + 1),
        );
        let mut proposed = ranges.clone();
        proposed.push(candidate);
        let proposed = normalize_sample_ranges(proposed);
        let bytes = proposed
            .iter()
            .map(|range| range.byte_end.saturating_sub(range.byte_start))
            .sum::<usize>();
        if bytes <= MAX_PAPER_TEXT {
            ranges = proposed;
        }
    }
    let mut remaining = MAX_PAPER_TEXT
        - ranges
            .iter()
            .map(|r| r.byte_end - r.byte_start)
            .sum::<usize>();
    let mut gaps: Vec<_> = ranges
        .windows(2)
        .map(|pair| (pair[0].byte_end, pair[1].byte_start))
        .collect();
    gaps.sort_by_key(|&(start, end)| (end - start, start));
    for (start, end) in gaps {
        if remaining == 0 {
            break;
        }
        let end = char_boundary_before(text, end.min(start + remaining));
        if end > start {
            ranges.push(range(start, end, "surrounding_context".to_string()));
            remaining -= end - start;
        }
    }
    let ranges = normalize_sample_ranges(ranges);
    let included_bytes = ranges
        .iter()
        .map(|range| range.byte_end.saturating_sub(range.byte_start))
        .sum();
    let mut sampled = String::with_capacity(included_bytes + ranges.len() * 100);
    for sample in &ranges {
        sampled.push_str(&format!(
            "\n\n[PIPELINE ORIENTATION SAMPLE bytes {}..{}; {}]\n",
            sample.byte_start,
            sample.byte_end,
            sample.reasons.join(" | ")
        ));
        sampled.push_str(&text[sample.byte_start..sample.byte_end]);
    }
    OrientationSample {
        schema_version: 1,
        original_bytes: text.len(),
        included_bytes,
        omitted_bytes: text.len().saturating_sub(included_bytes),
        truncated: true,
        ranges,
        text: sampled,
    }
}

fn spaced_headings<T>(headings: Vec<T>, limit: usize) -> impl Iterator<Item = T> {
    let last = headings.len().saturating_sub(1);
    let count = headings.len().min(limit);
    let mut positions = (0..count)
        .map(move |index| index * last / count.saturating_sub(1).max(1))
        .peekable();
    headings
        .into_iter()
        .enumerate()
        .filter_map(move |(index, heading)| {
            if positions.peek() == Some(&index) {
                positions.next();
                Some(heading)
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_document_sampling_includes_sections_and_tail_with_a_bounded_ledger() {
        let mut text = "# Abstract\nStart evidence.\n".to_string();
        text.push_str(&"middle filler\n".repeat(12_000));
        text.push_str("# Methods\nMETHOD_SENTINEL\n");
        text.push_str(&"more filler\n".repeat(12_000));
        text.push_str("# Appendix A\nAPPENDIX_SENTINEL\n");
        text.push_str(&"tail filler\n".repeat(8_000));
        text.push_str("TAIL_SENTINEL");

        let sample = orientation_sample(&text);
        assert!(sample.truncated);
        assert!(sample.included_bytes <= MAX_PAPER_TEXT);
        assert!(sample.text.contains("METHOD_SENTINEL"));
        assert!(sample.text.contains("APPENDIX_SENTINEL"));
        assert!(sample.text.contains("TAIL_SENTINEL"));
        assert!(sample.omitted_bytes > 0);
    }

    #[test]
    fn long_document_sampling_preserves_major_sections_and_uses_spare_capacity() {
        let mut text = "# Paper\n".to_string();
        text.push_str(&"évidence\n".repeat(6_000));
        for _ in 0..30 {
            text.push_str("### Minor subsection\n");
            text.push_str(&"évidence\n".repeat(330));
        }
        text.push_str("## Figures and Tables\nEXHIBIT_SENTINEL\n");
        text.push_str(&"évidence\n".repeat(12_000));

        let sample = orientation_sample(&text);
        assert!(sample.text.contains("EXHIBIT_SENTINEL"));
        assert!(sample.text.contains("section_start:## Figures and Tables"));
        assert!((MAX_PAPER_TEXT - 3..=MAX_PAPER_TEXT).contains(&sample.included_bytes));
        assert_eq!(sample.included_bytes + sample.omitted_bytes, text.len());
        for range in &sample.ranges {
            assert!(text.is_char_boundary(range.byte_start));
            assert!(text.is_char_boundary(range.byte_end));
        }
    }

    #[test]
    fn short_document_sampling_is_lossless() {
        let sample = orientation_sample("é complete document");
        assert!(!sample.truncated);
        assert_eq!(sample.text, "é complete document");
        assert_eq!(sample.ranges.len(), 1);
    }
}
