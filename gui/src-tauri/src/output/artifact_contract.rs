use regex::Regex;
use std::sync::LazyLock;

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

/// Generate a fresh identifier for the legacy report-file compatibility path.
/// New terminal artifacts use provider-native JSON Schema constraints.
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

/// Presentation instructions apply only to authored Markdown, never opaque
/// identifiers, structured data, or evidence quoted verbatim from the source.
const REPORT_MATH_FORMAT: &str = r#"For authored Markdown report text and Markdown-bearing finding fields (titles, bodies, problems, consequences, and recommended actions), use `$...$` for inline mathematics, including individual variables, and `$$` on separate lines for display equations. Use TeX for symbols, scripts, and accents, for example `$\kappa_w = c\lambda_w(1-\beta)$` and `$\hat{\sigma}_K$`. Preserve complete formulas and their delimiters through consolidation and validation. Escape literal currency dollar signs as `\$` in the decoded Markdown. Apply the response format's normal string escaping to backslashes; do not double-escape the decoded Markdown. These formatting rules do not apply to identifiers, URLs, arbitrary structured data, code, or verbatim evidence quotations; preserve those exactly."#;

/// Provider-neutral reminder for a schema-bearing artifact. The schema travels
/// through the provider's native structured-output channel and remains the
/// only source of truth for serialization and field structure.
pub fn structured_output_format(write_dir: Option<&str>) -> String {
    let artifact_note = write_dir
        .map(|dir| {
            format!(
                " Supporting files may be saved under {dir}/files/, but the schema-bearing primary artifact must be returned in the final response."
            )
        })
        .unwrap_or_default();
    format!(
        "RESPONSE CONTRACT:\n\
         Complete the requested primary artifact using every applicable field in the response schema supplied by Pipeline.{artifact_note}\n{REPORT_MATH_FORMAT}"
    )
}

/// Portable schema used when the durable artifact is text (normally
/// Markdown). The provider constrains the transport envelope; Pipeline owns
/// extraction and stores only the `content` string.
pub fn text_artifact_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "title": "Text artifact",
        "description": "Transport envelope for the complete requested text artifact.",
        "required": ["content"],
        "properties": {
            "content": {
                "type": "string",
                "description": "The complete requested text artifact, without a second transport envelope."
            }
        }
    })
}

/// Task-level reminder for a schema-backed text artifact. The native schema's
/// `content` description owns the transport detail.
pub fn text_artifact_output_format(write_dir: Option<&str>, content_kind: &str) -> String {
    let artifact_note = write_dir
        .map(|dir| {
            format!(
                " Supporting files may be saved under {dir}/files/, but the primary artifact must be returned in the final response."
            )
        })
        .unwrap_or_default();
    format!(
        "RESPONSE CONTRACT:\n\
         These instructions supersede any earlier output-only or report-file instructions. \
         Complete the {content_kind} using the response schema supplied by Pipeline; do not create a second artifact envelope.{artifact_note}\n{REPORT_MATH_FORMAT}"
    )
}

/// Strictly decode a provider-constrained text artifact. This deliberately
/// rejects fence/preamble recovery and also retains the report-level empty and
/// refusal checks that predate structured transport.
pub fn extract_text_artifact(text: &str) -> Result<String, String> {
    let canonical = crate::pipeline::structured::canonicalize(&text_artifact_schema(), text)?;
    let value: serde_json::Value = serde_json::from_str(&canonical)
        .map_err(|error| format!("failed to decode canonical text artifact: {error}"))?;
    let content = value
        .get("content")
        .and_then(serde_json::Value::as_str)
        .expect("the text artifact schema requires a string content field")
        .trim();
    if content.is_empty() {
        return Err("structured text artifact was empty".to_string());
    }
    if apparent_refusal(content) {
        return Err("structured text artifact contained an apparent refusal".to_string());
    }
    Ok(content.to_string())
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
