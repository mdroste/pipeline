//! The format-neutral view of one Review that every export renders.

use super::text::{markdown_to_plain, one_line, truncate_chars};
use crate::models::{Finding, FindingEvidence, PipelineReport, ReportQuality};

/// Host evidence-verification states as researchers read them. Unknown or
/// empty statuses are shown as "Not checked"; nothing is promoted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Verification {
    Verified,
    VerifiedWithNormalization,
    PartiallyVerified,
    NotFound,
    NotCheckable,
    #[default]
    Unchecked,
}

impl Verification {
    pub const ALL: [Verification; 6] = [
        Verification::Verified,
        Verification::VerifiedWithNormalization,
        Verification::PartiallyVerified,
        Verification::NotFound,
        Verification::NotCheckable,
        Verification::Unchecked,
    ];

    pub fn from_status(status: &str) -> Self {
        match status.trim() {
            "verified" => Self::Verified,
            "verified_with_normalization" => Self::VerifiedWithNormalization,
            "partially_verified" => Self::PartiallyVerified,
            "not_found" => Self::NotFound,
            "not_checkable" => Self::NotCheckable,
            _ => Self::Unchecked,
        }
    }

    /// Stable token for HTML data attributes and CSS classes.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::VerifiedWithNormalization => "verified-normalized",
            Self::PartiallyVerified => "partial",
            Self::NotFound => "not-found",
            Self::NotCheckable => "not-checkable",
            Self::Unchecked => "unchecked",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Verified => "Quote found",
            Self::VerifiedWithNormalization => "Quote found (formatting differs)",
            Self::PartiallyVerified => "Partial match",
            Self::NotFound => "Quote not found",
            Self::NotCheckable => "Not checkable",
            Self::Unchecked => "Not checked",
        }
    }
}

/// Run facts shown at the top of every export. Callers supply display names
/// only; absolute paths never enter an exported file.
#[derive(Debug, Clone, Default)]
pub struct ReportMetadata {
    pub title: String,
    pub input_name: String,
    pub workflow: String,
    pub date: String,
    pub status: String,
}

#[derive(Debug, Clone, Default)]
pub struct ExportEvidence {
    pub verification: Verification,
    pub page: Option<u32>,
    /// Human-readable location, for example "Page 4 · Section 4.2".
    pub locator: String,
    pub quote: String,
}

#[derive(Debug, Clone, Default)]
pub struct ExportFinding {
    /// Display number, identical to the `#N` used in the Markdown report.
    pub number: usize,
    pub title: String,
    pub category: String,
    /// Normalized `high`, `medium`, `low`, another producer value, or empty.
    pub severity: String,
    pub confidence: String,
    pub problem: String,
    pub consequence: String,
    pub recommended_action: String,
    /// Markdown body. For findings-v2 it repeats the three fields above.
    pub body: String,
    pub evidence: Vec<ExportEvidence>,
}

impl ExportFinding {
    pub fn severity_label(&self) -> String {
        severity_label(&self.severity)
    }

    /// Distinct evidence states present on this finding, in a fixed order.
    pub fn verification_states(&self) -> Vec<Verification> {
        let mut states: Vec<Verification> = self
            .evidence
            .iter()
            .map(|evidence| evidence.verification)
            .collect();
        states.sort();
        states.dedup();
        states
    }

    /// Plain-text paragraphs for a Word comment or PDF note. `evidence`
    /// names the passage the note is attached to, when there is one.
    pub fn note_paragraphs(&self, evidence: Option<&ExportEvidence>) -> Vec<String> {
        let mut paragraphs = vec![self.heading_line()];
        if self.problem.is_empty() {
            let body = markdown_to_plain(&self.body);
            if !body.is_empty() {
                paragraphs.push(truncate_chars(&body, 3_000));
            }
        } else {
            paragraphs.push(format!("Problem: {}", truncate_chars(&self.problem, 2_000)));
            if !self.consequence.is_empty() {
                paragraphs.push(format!(
                    "Consequence: {}",
                    truncate_chars(&self.consequence, 1_500)
                ));
            }
            if !self.recommended_action.is_empty() {
                paragraphs.push(format!(
                    "Recommended action: {}",
                    truncate_chars(&self.recommended_action, 1_500)
                ));
            }
        }
        if let Some(evidence) = evidence {
            paragraphs.push(evidence_line(evidence));
        }
        let mut facts = Vec::new();
        if !self.confidence.is_empty() {
            facts.push(format!("Confidence: {}", self.confidence));
        }
        if !self.category.is_empty() {
            facts.push(format!("Category: {}", self.category));
        }
        if !facts.is_empty() {
            paragraphs.push(facts.join(" · "));
        }
        paragraphs
    }

    /// "#3 · High · Title" — the line that opens every note.
    pub fn heading_line(&self) -> String {
        format!(
            "#{} · {} · {}",
            self.number,
            self.severity_label(),
            one_line(&self.title)
        )
    }
}

pub(super) fn evidence_line(evidence: &ExportEvidence) -> String {
    if evidence.locator.is_empty() {
        format!("Evidence: {}", evidence.verification.label())
    } else {
        format!(
            "Evidence: {} · {}",
            evidence.verification.label(),
            evidence.locator
        )
    }
}

pub(super) fn severity_label(severity: &str) -> String {
    match severity.trim() {
        "" => "Unrated".to_string(),
        "high" => "High".to_string(),
        "medium" => "Medium".to_string(),
        "low" => "Low".to_string(),
        other => crate::output::capitalize(&one_line(other)),
    }
}

/// Everything an export needs from one completed Review.
#[derive(Debug, Clone, Default)]
pub struct ReportExport {
    pub meta: ReportMetadata,
    /// Findings in report order (grouped by category, numbered as in the
    /// Markdown report). Empty when the run has no structured findings.
    pub findings: Vec<ExportFinding>,
    /// The saved report Markdown with UI-only run details removed.
    pub report_markdown: String,
    pub quality: ReportQuality,
}

impl ReportExport {
    pub fn from_report(report: &PipelineReport, markdown: &str, meta: ReportMetadata) -> Self {
        let findings = crate::findings::canonical_findings(report)
            .map(|set| ordered_findings(&set.findings))
            .unwrap_or_default();
        Self {
            meta,
            findings,
            report_markdown: crate::output::clean_export_markdown(markdown)
                .trim()
                .to_string(),
            quality: report.quality.clone(),
        }
    }

    /// Distinct categories in first-appearance order.
    pub fn categories(&self) -> Vec<&str> {
        let mut categories: Vec<&str> = Vec::new();
        for finding in &self.findings {
            if !categories.contains(&finding.category.as_str()) {
                categories.push(finding.category.as_str());
            }
        }
        categories
    }
}

/// Mirror `findings::render_markdown`: group by first-appearing category and
/// number sequentially, so "#7" means the same finding in every format.
fn ordered_findings(findings: &[Finding]) -> Vec<ExportFinding> {
    fn category_of(finding: &Finding) -> &str {
        let category = finding.category.trim();
        if category.is_empty() {
            "Findings"
        } else {
            category
        }
    }
    let mut categories: Vec<&str> = Vec::new();
    for finding in findings {
        let category = category_of(finding);
        if !categories.contains(&category) {
            categories.push(category);
        }
    }
    let mut ordered = Vec::with_capacity(findings.len());
    for category in categories {
        for finding in findings
            .iter()
            .filter(|finding| category_of(finding) == category)
        {
            ordered.push(ExportFinding {
                number: ordered.len() + 1,
                title: one_line(&finding.title),
                category: one_line(category),
                severity: finding.priority.trim().to_ascii_lowercase(),
                confidence: one_line(&finding.confidence),
                problem: finding.problem.trim().to_string(),
                consequence: finding.consequence.trim().to_string(),
                recommended_action: finding.recommended_action.trim().to_string(),
                body: finding.body.trim().to_string(),
                evidence: finding.evidence.iter().map(export_evidence).collect(),
            });
        }
    }
    ordered
}

fn export_evidence(evidence: &FindingEvidence) -> ExportEvidence {
    ExportEvidence {
        verification: Verification::from_status(&evidence.verification_status),
        page: evidence.page,
        locator: locator(evidence),
        quote: evidence.quote.trim().to_string(),
    }
}

/// Human citation only. Internal artifact paths, node ids, and call ids are
/// run-layout details and stay out of shared files.
fn locator(evidence: &FindingEvidence) -> String {
    let mut parts = Vec::new();
    if let Some(page) = evidence.page {
        parts.push(format!("Page {page}"));
    }
    if !evidence.source_path.is_empty() {
        parts.push(one_line(&evidence.source_path));
    }
    match (evidence.line_start, evidence.line_end) {
        (Some(start), Some(end)) if end > start => parts.push(format!("lines {start}–{end}")),
        (Some(start), _) => parts.push(format!("line {start}")),
        _ => {}
    }
    let description = one_line(&evidence.description);
    if !description.is_empty() {
        parts.push(truncate_chars(&description, 300));
    }
    if !evidence.doi.is_empty() {
        parts.push(format!("doi:{}", one_line(&evidence.doi)));
    } else if !evidence.url.is_empty() {
        parts.push(truncate_chars(&one_line(&evidence.url), 300));
    }
    parts.join(" · ")
}
