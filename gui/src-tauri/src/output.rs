//! Report rendering.

use crate::models::PipelineReport;
use crate::settings::Settings;
use regex::Regex;
use std::sync::LazyLock;
use std::time::Duration;

static PREAMBLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?mi)^(I(?:'|’)(?:ve|ll|m)|I (?:have|need|can|should|will)|Let me|Now (?:I|let)|OK[,.]|Alright[,.]|Here (?:is|are)|Looking at|After reading|Having read)[^\n]*\n*"
    ).expect("preamble regex is invalid")
});

// Keep this deliberately narrow. Pipeline cannot judge whether a report is
// insightful, but it can distinguish a terminal refusal from report content.
// Limiting the scan to short responses and explicit request/report language
// avoids rejecting a substantive review that happens to say, for example,
// that one empirical result cannot be verified.
static APPARENT_REFUSAL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?is)\bi(?:\s+(?:cannot|can't|am\s+unable\s+to|must\s+refuse\s+to|will\s+not|won't)|'m\s+unable\s+to)\s+(?:comply(?:\s+with\s+(?:this|the)\s+request)?|(?:provide|produce|write|generate|complete)\s+(?:(?:this|the|a)\s+)?(?:requested\s+)?(?:report|review|task|request)|(?:help|assist)\s+with\s+(?:this|that|the\s+request)|follow\s+(?:these|the)\s+instructions)\b",
    )
    .expect("refusal regex is invalid")
});

const RUN_DETAILS_START: &str = "<!-- PIPELINE RUN DETAILS START -->";
const RUN_DETAILS_END: &str = "<!-- PIPELINE RUN DETAILS END -->";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportRejectionKind {
    Envelope,
    Content,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportValidationError {
    kind: ReportRejectionKind,
    reason: String,
}

impl ReportValidationError {
    fn envelope(reason: impl Into<String>) -> Self {
        Self {
            kind: ReportRejectionKind::Envelope,
            reason: reason.into(),
        }
    }

    fn content(reason: impl Into<String>) -> Self {
        Self {
            kind: ReportRejectionKind::Content,
            reason: reason.into(),
        }
    }

    pub fn kind(&self) -> ReportRejectionKind {
        self.kind
    }
}

impl std::fmt::Display for ReportValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for ReportValidationError {}

/// Generate a fresh identifier for one logical report-producing call. Static
/// markers are easy for quoted source material or stale artifacts to collide
/// with; a random call nonce makes the report boundary unambiguous.
pub fn new_report_nonce() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| format!("RNG failed: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub fn report_markers(nonce: &str) -> (String, String) {
    (
        format!("<!-- PIPELINE REPORT {nonce} START -->"),
        format!("<!-- PIPELINE REPORT {nonce} END -->"),
    )
}

/// Produce the provider-neutral terminal response contract. Models may write
/// supporting artifacts, but the report itself always travels through the
/// provider's terminal assistant-response channel.
pub fn report_output_format(write_dir: Option<&str>, nonce: &str) -> String {
    let (start, end) = report_markers(nonce);
    let artifact_note = write_dir
        .map(|dir| {
            format!(
                "\nSupporting files (data tables, extracted figures) may be saved under \
                 {dir}/files/ and referenced from the report by relative path. Do not write \
                 the report itself to a file."
            )
        })
        .unwrap_or_default();
    format!(
        "OUTPUT FORMAT:\n\
         These instructions supersede any earlier output-format or report-file instructions.\n\
         Return the complete markdown report in your final response between these exact markers:\n\
         {start}\n\
         [complete report]\n\
         {end}\n\
         The first marker ends in START and the closing marker ends in END; do not repeat the \
         opening marker as the closing marker.\n\
         Put nothing before the start marker or after the end marker. Do not include progress \
         narration, acknowledgments, or a description of your process.{artifact_note}\n\
         Use `$...$` for inline math and `$$...$$` for display math. Do not use `\\(...\\)` or \
         `\\[...\\]` delimiters."
    )
}

/// Extract a complete, nonce-delimited report.
///
/// The random nonce, rather than the human-readable START/END label, identifies
/// the boundary. Exactly two nonce-bearing markers therefore delimit an
/// unambiguous report even when a model repeats START as the closing label,
/// swaps the labels, or adds narration outside them. Fewer markers may indicate
/// truncated output and more markers are ambiguous, so both still fail closed.
pub fn extract_report_envelope(text: &str, nonce: &str) -> Result<String, ReportValidationError> {
    let (start, end) = report_markers(nonce);
    let start_count = text.matches(&start).count();
    let end_count = text.matches(&end).count();
    let mut boundaries = text
        .match_indices(&start)
        .map(|(position, marker)| (position, marker.len()))
        .chain(
            text.match_indices(&end)
                .map(|(position, marker)| (position, marker.len())),
        )
        .collect::<Vec<_>>();
    boundaries.sort_unstable_by_key(|(position, _)| *position);

    if boundaries.len() != 2 {
        return Err(ReportValidationError::envelope(format!(
            "expected exactly two report boundary markers; found {} ({start_count} START, {end_count} END)",
            boundaries.len()
        )));
    }

    let content_start = boundaries[0].0 + boundaries[0].1;
    let content_end = boundaries[1].0;
    let report = text[content_start..content_end].trim();
    if report.is_empty() {
        return Err(ReportValidationError::content(
            "report between boundary markers was empty",
        ));
    }
    if apparent_refusal(report) {
        return Err(ReportValidationError::content(
            "response contained an apparent refusal instead of the requested report",
        ));
    }
    Ok(report.to_string())
}

fn apparent_refusal(report: &str) -> bool {
    if report.chars().count() > 4_000 {
        return false;
    }
    let prefix = report
        .chars()
        .take(1_200)
        .collect::<String>()
        .replace(['’', '‘'], "'");
    APPARENT_REFUSAL_RE.is_match(&prefix)
}

/// Extract content between `<!-- REPORT START -->` and `<!-- REPORT END -->` markers.
/// Falls back to the full text (with preamble stripping) if markers aren't present.
pub fn strip_to_report(text: &str) -> String {
    let trimmed = text.trim();

    const START: &str = "<!-- REPORT START -->";
    const END: &str = "<!-- REPORT END -->";

    if let Some(start_pos) = trimmed.find(START) {
        let content_start = start_pos + START.len();
        if let Some(end_pos) = trimmed[content_start..].find(END) {
            return trimmed[content_start..content_start + end_pos]
                .trim()
                .to_string();
        }
        // START found but no END — take everything after START
        return trimmed[content_start..].trim().to_string();
    }

    // No markers — fall back to preamble stripping
    strip_preamble(trimmed)
}

pub(crate) fn fence_marker(line: &str) -> Option<(u8, usize)> {
    let bytes = line.trim_start().as_bytes();
    let marker = *bytes.first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let count = bytes.iter().take_while(|byte| **byte == marker).count();
    (count >= 3).then_some((marker, count))
}

fn normalize_math_line(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut output = String::with_capacity(line.len());
    let mut index = 0usize;
    let mut inline_ticks = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'`' {
            let count = bytes[index..]
                .iter()
                .take_while(|byte| **byte == b'`')
                .count();
            output.push_str(&line[index..index + count]);
            if inline_ticks == 0 {
                inline_ticks = count;
            } else if inline_ticks == count {
                inline_ticks = 0;
            }
            index += count;
            continue;
        }
        if inline_ticks == 0
            && bytes[index] == b'\\'
            && index + 1 < bytes.len()
            && (index == 0 || bytes[index - 1] != b'\\')
        {
            match bytes[index + 1] {
                b'(' | b')' => {
                    output.push('$');
                    index += 2;
                    continue;
                }
                b'[' | b']' => {
                    output.push_str("$$");
                    index += 2;
                    continue;
                }
                _ => {}
            }
        }
        let character = line[index..]
            .chars()
            .next()
            .expect("index remains on a UTF-8 boundary");
        output.push(character);
        index += character.len_utf8();
    }
    output
}

/// Canonicalize the two common LaTeX delimiter families to the dollar form
/// understood consistently by remark-math, the Markdown export path, and
/// KaTeX. Fenced and inline code are intentionally left untouched.
pub fn normalize_math_delimiters(markdown: &str) -> String {
    let mut output = String::with_capacity(markdown.len());
    let mut fence: Option<(u8, usize)> = None;
    for line in markdown.split_inclusive('\n') {
        if let Some((marker, count)) = fence_marker(line) {
            if let Some((open_marker, open_count)) = fence {
                if marker == open_marker && count >= open_count {
                    fence = None;
                }
            } else {
                fence = Some((marker, count));
            }
            output.push_str(line);
        } else if fence.is_some() {
            output.push_str(line);
        } else if let Some(normalized) = normalize_display_environment(line) {
            output.push_str(&normalized);
        } else {
            output.push_str(&normalize_math_line(line));
        }
    }
    output
}

fn normalize_display_environment(line: &str) -> Option<String> {
    static ENVIRONMENT_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(
            r"^([ \t]*)\\(begin|end)\{(equation\*?|displaymath|align\*?|gather\*?)\}[ \t]*(\r?\n)?$",
        )
        .expect("display-environment regex must compile")
    });
    let captures = ENVIRONMENT_RE.captures(line)?;
    let indent = captures.get(1).map(|value| value.as_str()).unwrap_or("");
    let boundary = captures.get(2)?.as_str();
    let environment = captures.get(3)?.as_str();
    let newline = captures.get(4).map(|value| value.as_str()).unwrap_or("");
    let replacement = match (boundary, environment) {
        ("begin", "equation" | "equation*" | "displaymath") => "$$",
        ("end", "equation" | "equation*" | "displaymath") => "$$",
        ("begin", "align" | "align*") => "$$\n\\begin{aligned}",
        ("end", "align" | "align*") => "\\end{aligned}\n$$",
        ("begin", "gather" | "gather*") => "$$\n\\begin{gathered}",
        ("end", "gather" | "gather*") => "\\end{gathered}\n$$",
        _ => return None,
    };
    Some(format!("{indent}{replacement}{newline}"))
}

/// Remove UI-only run telemetry from a report before public export.
pub fn clean_export_markdown(markdown: &str) -> String {
    let mut clean = markdown.to_string();
    while let Some(start) = clean.find(RUN_DETAILS_START) {
        let Some(relative_end) = clean[start + RUN_DETAILS_START.len()..].find(RUN_DETAILS_END)
        else {
            break;
        };
        let end = start + RUN_DETAILS_START.len() + relative_end + RUN_DETAILS_END.len();
        let mut next = String::with_capacity(clean.len() - (end - start));
        next.push_str(clean[..start].trim_end());
        next.push_str("\n\n");
        next.push_str(clean[end..].trim_start());
        clean = next;
    }
    clean
}

/// Strip LLM chain-of-thought preamble from output.
/// Removes lines like "I've now read the full paper. Let me compile..."
/// that appear before the actual content.
fn strip_preamble(text: &str) -> String {
    let trimmed = text.trim();

    // Only strip from the beginning — don't touch mid-text occurrences.
    // Guard against infinite loops (e.g., zero-width matches).
    let mut result = trimmed;
    for _ in 0..50 {
        let stripped = result.trim_start();
        if let Some(m) = PREAMBLE_RE.find(stripped) {
            if m.start() == 0 && m.end() > 0 {
                result = &stripped[m.end()..];
                continue;
            }
        }
        break;
    }

    result.trim().to_string()
}

mod render;
mod summary;

pub(crate) use render::capitalize;
pub use render::render_markdown;
pub use summary::model_price;
use summary::{format_duration, render_run_summary};

#[cfg(test)]
use summary::{estimate_cost, format_cost, provider_in_api_mode};

#[cfg(test)]
mod tests;
