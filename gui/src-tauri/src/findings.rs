//! Canonical, input-neutral findings published by completed runs.
//!
//! Step responses remain immutable provider evidence. This module normalizes a
//! declared findings product (or a conservative legacy representation) into a
//! single durable shape consumed by report rendering, annotations, and
//! Projects. Input-specific differences live only in evidence locators.

use crate::models::{
    Finding, FindingEvidence, FindingSet, PipelineReport, RunProducts, StepOutput,
};
use crate::pipeline_config::PipelineConfig;
use regex::Regex;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::path::{Component, Path};
use std::sync::LazyLock;

pub const FINDING_SCHEMA_VERSION: u32 = 1;
const MAX_FINDINGS: usize = 1_000;
const MAX_EVIDENCE: usize = 50;
const MAX_SOURCES: usize = 8;
const MAX_TITLE_BYTES: usize = 1_000;
const MAX_BODY_BYTES: usize = 20_000;
const MAX_REFERENCE_BYTES: usize = 1_000;
const MAX_LINE: u32 = 10_000_000;

/// Canonical report sections used by the Automatic Paper Review consolidation
/// contract. The schema enum is the source of truth; prompts reference these
/// labels rather than restating them as free text.
pub const CATEGORIES: [&str; 4] = [
    "Correctness and Internal Consistency",
    "Methodological and Evidentiary Concerns",
    "Contribution and Scope",
    "Exposition and Organization",
];

static MARKDOWN_FINDING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\*\*#([0-9]+)\.\s+(.+?)\*\*\s*$").expect("legacy finding-title regex is invalid")
});

/// The shared evidence-locator array contract. Descriptions carry the locator
/// semantics so any profile adopting the schema gets the same guidance the
/// stock prompts rely on.
pub fn evidence_schema() -> Value {
    serde_json::json!({
        "type": "array",
        "maxItems": MAX_EVIDENCE,
        "description": "Locators supporting the finding. Use only references present in the supplied material; never invent a locator.",
        "items": {
            "type": "object",
            "properties": {
                "page": {"type": "integer", "description": "1-based page number in the rendered document."},
                "line_start": {"type": "integer", "description": "1-based first line of a cited source range."},
                "line_end": {"type": "integer", "description": "1-based last line of the range; at least line_start."},
                "node_id": {"type": "string", "description": "DocumentBundle node id from the structure index."},
                "asset_id": {"type": "string", "description": "DocumentBundle asset id of a cited figure, table, or page render."},
                "artifact_path": {"type": "string", "description": "Path relative to the saved run's artifacts."},
                "source_path": {"type": "string", "description": "Path relative to the selected source root."},
                "source_hash": {"type": "string", "description": "Content fingerprint of the cited source file, when known."},
                "description": {"type": "string", "description": "Human-readable location, such as 'Section 4.2, Theorem 3'."},
                "quote": {"type": "string", "description": "Short verbatim quote from the cited location."}
            }
        }
    })
}

/// The host-owned schema used by Automatic Paper Review and available to any
/// profile that explicitly publishes findings.
pub fn output_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "title": "Consolidated findings",
        "required": ["findings"],
        "properties": {
            "findings": {
                "type": "array",
                "maxItems": 40,
                "description": "Findings in decreasing order of centrality to the main claims; array order conveys importance.",
                "items": {
                    "type": "object",
                    "required": ["id", "title", "category", "body", "evidence"],
                    "properties": {
                        "id": {"type": "string", "minLength": 1, "description": "Stable descriptive identifier preserved across consolidation and validation; never an ordinal."},
                        "source_key": {"type": "string", "description": "Stable producer key (rule id, check id) when a deterministic producer supplies one."},
                        "sources": {
                            "type": "array",
                            "maxItems": MAX_SOURCES,
                            "description": "Report ids of the reviewer analyses supporting this finding.",
                            "items": {"type": "string"}
                        },
                        "title": {"type": "string", "minLength": 1, "description": "Specific one-line title."},
                        "category": {
                            "type": "string",
                            "enum": CATEGORIES,
                            "description": "Exactly one canonical report section."
                        },
                        "priority": {
                            "type": "string",
                            "enum": ["high", "medium", "low"],
                            "description": "Optional coarse priority; array order remains the authoritative ranking."
                        },
                        "body": {"type": "string", "description": "Markdown preserving the paper evidence, the problem, its consequence, and what would address it."},
                        "evidence": evidence_schema()
                    }
                }
            }
        }
    })
}

/// Resolve the products declared by a concrete execution config. Profiles
/// written before publication contracts existed retain the old final-step
/// behavior and are conservatively adapted when they already return issues.
pub fn build_run_products(config: &PipelineConfig, outputs: &[StepOutput]) -> RunProducts {
    let configured_primary = config.outputs.primary_step.trim();
    let primary_step_id = if configured_primary.is_empty() {
        inferred_primary_output(outputs)
            .map(|output| output.step_id.clone())
            .unwrap_or_default()
    } else {
        configured_primary.to_string()
    };

    let findings = if config.outputs.findings_step.trim().is_empty() {
        outputs
            .iter()
            .rev()
            .filter(|output| usable(output))
            .find_map(parse_structured_output)
    } else {
        declared_output(outputs, &config.outputs.findings_step).and_then(parse_structured_output)
    };

    RunProducts {
        schema_version: FINDING_SCHEMA_VERSION,
        primary_step_id,
        findings,
    }
}

/// Populate products for an old report without consulting mutable profile
/// state. Structured issue JSON wins; strict stock Auto Review Markdown is a
/// deterministic last resort. The report's raw outputs are never modified.
pub fn ensure_legacy_products(report: &mut PipelineReport) {
    if report.products.schema_version > 0 {
        return;
    }
    let primary_step_id = inferred_primary_output(&report.step_outputs)
        .map(|output| output.step_id.clone())
        .unwrap_or_default();
    let findings = report
        .step_outputs
        .iter()
        .rev()
        .filter(|output| usable(output))
        .find_map(|output| {
            parse_legacy_structured_output(output).or_else(|| parse_legacy_markdown_output(output))
        });
    report.products = RunProducts {
        schema_version: FINDING_SCHEMA_VERSION,
        primary_step_id,
        findings,
    };
}

pub fn canonical_findings(report: &PipelineReport) -> Option<FindingSet> {
    if report.products.schema_version > 0 {
        return report.products.findings.clone();
    }
    report
        .step_outputs
        .iter()
        .rev()
        .filter(|output| usable(output))
        .find_map(|output| {
            parse_legacy_structured_output(output).or_else(|| parse_legacy_markdown_output(output))
        })
}

fn declared_output<'a>(outputs: &'a [StepOutput], step_id: &str) -> Option<&'a StepOutput> {
    let step_id = step_id.trim();
    (!step_id.is_empty()).then_some(())?;
    outputs
        .iter()
        .rev()
        .find(|output| usable(output) && base_step_id(&output.step_id) == step_id)
}

fn inferred_primary_output(outputs: &[StepOutput]) -> Option<&StepOutput> {
    outputs
        .iter()
        .rev()
        .find(|output| usable(output) && output.phase == "sequential")
        .or_else(|| outputs.iter().rev().find(|output| usable(output)))
}

fn usable(output: &StepOutput) -> bool {
    !output.skipped && !output.raw_text.trim().is_empty()
}

fn base_step_id(step_id: &str) -> &str {
    step_id.split('/').next().unwrap_or(step_id)
}

fn parse_structured_output(output: &StepOutput) -> Option<FindingSet> {
    let value = crate::pipeline::structured::extract_json(&output.raw_text)?;
    parse_value(&value, &output.step_id, &output.step_label)
}

/// Legacy inference has no workflow contract to identify the published step.
/// Do not mistake the distinctive per-reviewer Auto Review artifact for the
/// consolidated product when a run stopped before synthesis or validation.
fn parse_legacy_structured_output(output: &StepOutput) -> Option<FindingSet> {
    let value = crate::pipeline::structured::extract_json(&output.raw_text)?;
    let specialist = value
        .get("findings")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            !items.is_empty()
                && items.iter().all(|item| {
                    item.as_object().is_some_and(|object| {
                        object.contains_key("problem") && object.contains_key("what_would_help")
                    })
                })
        });
    if specialist {
        return None;
    }
    parse_value(&value, &output.step_id, &output.step_label)
}

pub fn parse_value(
    value: &Value,
    source_step_id: &str,
    source_step_label: &str,
) -> Option<FindingSet> {
    let items = value
        .as_array()
        .or_else(|| value.get("findings").and_then(Value::as_array))
        .or_else(|| value.get("issues").and_then(Value::as_array))?;
    if items.len() > MAX_FINDINGS {
        return None;
    }
    let mut findings = Vec::with_capacity(items.len());
    let mut used_ids = HashSet::new();
    for (index, item) in items.iter().enumerate() {
        let object = item.as_object()?;
        let title = bounded_value(
            first(object, &["title", "summary", "message", "name"]),
            MAX_TITLE_BYTES,
        );
        let body = bounded_value(
            first(
                object,
                &[
                    "body",
                    "description",
                    "detail",
                    "explanation",
                    "rationale",
                    "recommendation",
                ],
            ),
            MAX_BODY_BYTES,
        );
        if title.is_empty() && body.is_empty() {
            return None;
        }
        let mut id = bounded_value(
            first(object, &["id", "issue_id", "issueId", "fingerprint", "key"]),
            MAX_REFERENCE_BYTES,
        );
        if id.is_empty() {
            id = (index + 1).to_string();
        }
        let base_id = id.clone();
        let mut suffix = 2usize;
        while !used_ids.insert(id.clone()) {
            id = bounded_text(&format!("{base_id}#{suffix}"), MAX_REFERENCE_BYTES);
            suffix += 1;
        }
        let source_key = bounded_value(
            first(object, &["source_key", "sourceKey", "rule_id", "ruleId"]),
            MAX_REFERENCE_BYTES,
        );
        let sources = match object.get("sources") {
            Some(Value::Array(values)) => values
                .iter()
                .take(MAX_SOURCES)
                .filter_map(|value| {
                    let source = bounded_value(Some(value), MAX_REFERENCE_BYTES);
                    (!source.is_empty()).then_some(source)
                })
                .collect(),
            _ => Vec::new(),
        };
        let category = bounded_value(
            first(
                object,
                &[
                    "category",
                    "section",
                    "location",
                    "criterion",
                    "file",
                    "file_path",
                    "filePath",
                    "path",
                ],
            ),
            MAX_REFERENCE_BYTES,
        );
        let priority = normalize_priority(&bounded_value(
            first(object, &["priority", "severity", "level"]),
            100,
        ));
        let evidence = match object.get("evidence") {
            Some(Value::Array(values)) => values
                .iter()
                .take(MAX_EVIDENCE)
                .filter_map(parse_evidence)
                .collect(),
            Some(value @ Value::Object(_)) => parse_evidence(value).into_iter().collect(),
            _ => parse_evidence(item).into_iter().collect(),
        };
        findings.push(Finding {
            id,
            source_key,
            sources,
            title: if title.is_empty() {
                bounded_text(&body, MAX_TITLE_BYTES)
            } else {
                title
            },
            category,
            priority,
            body,
            evidence,
        });
    }
    Some(FindingSet {
        schema_version: FINDING_SCHEMA_VERSION,
        source_step_id: bounded_text(source_step_id, MAX_REFERENCE_BYTES),
        source_step_label: bounded_text(source_step_label, MAX_TITLE_BYTES),
        findings,
    })
}

fn parse_evidence(value: &Value) -> Option<FindingEvidence> {
    let object = value.as_object()?;
    let page = bounded_u32(first(object, &["page", "page_number", "pageNumber"]), 5_000);
    let line_start = bounded_u32(
        first(object, &["line_start", "lineStart", "line"]),
        MAX_LINE,
    );
    let line_end = bounded_u32(first(object, &["line_end", "lineEnd"]), MAX_LINE)
        .filter(|end| line_start.is_some_and(|start| *end >= start));
    let node_id = bounded_value(first(object, &["node_id", "nodeId"]), MAX_REFERENCE_BYTES);
    let asset_id = bounded_value(first(object, &["asset_id", "assetId"]), MAX_REFERENCE_BYTES);
    let artifact_path = safe_relative_value(
        first(object, &["artifact_path", "artifactPath", "rel_path"]),
        MAX_REFERENCE_BYTES,
    );
    let source_path = safe_relative_value(
        first(
            object,
            &[
                "source_path",
                "sourcePath",
                "file",
                "file_path",
                "filePath",
                "path",
            ],
        ),
        MAX_REFERENCE_BYTES,
    );
    let source_hash = bounded_value(
        first(object, &["source_hash", "sourceHash"]),
        MAX_REFERENCE_BYTES,
    );
    let description = bounded_value(
        first(object, &["description", "label", "location"]),
        MAX_BODY_BYTES,
    );
    let quote = bounded_value(object.get("quote"), MAX_BODY_BYTES);
    if page.is_none()
        && line_start.is_none()
        && line_end.is_none()
        && node_id.is_empty()
        && asset_id.is_empty()
        && artifact_path.is_empty()
        && source_path.is_empty()
        && source_hash.is_empty()
        && description.is_empty()
        && quote.is_empty()
    {
        return None;
    }
    Some(FindingEvidence {
        page,
        line_start,
        line_end,
        node_id,
        asset_id,
        artifact_path,
        source_path,
        source_hash,
        description,
        quote,
    })
}

fn parse_legacy_markdown_output(output: &StepOutput) -> Option<FindingSet> {
    let mut category = String::new();
    let mut findings = Vec::new();
    let mut current: Option<(String, String, String)> = None;
    let flush = |current: &mut Option<(String, String, String)>, findings: &mut Vec<Finding>| {
        if let Some((id, title, body)) = current.take() {
            findings.push(Finding {
                id,
                title,
                category: String::new(),
                body: body.trim().to_string(),
                ..Default::default()
            });
        }
    };

    for line in output.raw_text.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            flush(&mut current, &mut findings);
            category = heading.trim().to_string();
            continue;
        }
        if let Some(captures) = MARKDOWN_FINDING_RE.captures(line.trim()) {
            flush(&mut current, &mut findings);
            let id = captures.get(1)?.as_str().to_string();
            let title = captures.get(2)?.as_str().trim().to_string();
            current = Some((id, title, String::new()));
            if let Some(finding) = findings.last_mut() {
                if finding.category.is_empty() {
                    finding.category = category.clone();
                }
            }
            continue;
        }
        if let Some((_, _, body)) = current.as_mut() {
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str(line);
        }
    }
    flush(&mut current, &mut findings);
    // The flush closure cannot see the active category, so fill it by a second
    // strict pass that tracks each title's heading. This also avoids accepting
    // arbitrary Markdown lists as a legacy finding product.
    let mut heading = String::new();
    let mut index = 0usize;
    for line in output.raw_text.lines() {
        if let Some(value) = line.strip_prefix("## ") {
            heading = value.trim().to_string();
        } else if MARKDOWN_FINDING_RE.is_match(line.trim()) {
            if let Some(finding) = findings.get_mut(index) {
                finding.category = heading.clone();
            }
            index += 1;
        }
    }
    if findings.is_empty() || findings.len() > MAX_FINDINGS {
        return None;
    }
    Some(FindingSet {
        schema_version: FINDING_SCHEMA_VERSION,
        source_step_id: output.step_id.clone(),
        source_step_label: output.step_label.clone(),
        findings,
    })
}

/// Deterministic human presentation of a canonical findings product. Findings
/// are grouped under their category in order of first appearance, so
/// centrality-ordered arrays never repeat section headers; within a category,
/// array order carries importance. Display numbers follow the rendered order
/// and are deliberately not identities.
pub fn render_markdown(findings: &FindingSet) -> String {
    if findings.findings.is_empty() {
        return "No findings.".to_string();
    }
    fn category_of(finding: &Finding) -> &str {
        let category = finding.category.trim();
        if category.is_empty() {
            "Findings"
        } else {
            category
        }
    }
    let mut categories: Vec<&str> = Vec::new();
    for finding in &findings.findings {
        let category = category_of(finding);
        if !categories.contains(&category) {
            categories.push(category);
        }
    }
    let mut markdown = String::new();
    let mut number = 0usize;
    for category in categories {
        if !markdown.is_empty() {
            markdown.push('\n');
        }
        markdown.push_str("## ");
        markdown.push_str(category);
        markdown.push_str("\n\n");
        for finding in findings
            .findings
            .iter()
            .filter(|finding| category_of(finding) == category)
        {
            number += 1;
            markdown.push_str(&format!("**#{}. {}**\n\n", number, finding.title.trim()));
            markdown.push_str(finding.body.trim());
            markdown.push_str("\n\n");
        }
    }
    markdown.trim().to_string()
}

fn first<'a>(object: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| object.get(*key))
}

fn bounded_value(value: Option<&Value>, maximum: usize) -> String {
    match value {
        Some(Value::String(value)) => bounded_text(value, maximum),
        Some(Value::Number(value)) => bounded_text(&value.to_string(), maximum),
        _ => String::new(),
    }
}

fn bounded_text(value: &str, maximum: usize) -> String {
    let value = value.trim();
    if value.len() <= maximum {
        return value.to_string();
    }
    let mut end = maximum;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn bounded_u32(value: Option<&Value>, maximum: u32) -> Option<u32> {
    value
        .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0 && *value <= maximum)
}

fn safe_relative_value(value: Option<&Value>, maximum: usize) -> String {
    let value = bounded_value(value, maximum);
    if value.is_empty()
        || value.starts_with('/')
        || value.contains('\\')
        || value.contains(':')
        || !Path::new(&value)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        String::new()
    } else {
        value
    }
}

fn normalize_priority(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "critical" | "major" | "severe" | "high" => "high".to_string(),
        "moderate" | "medium" | "warning" => "medium".to_string(),
        "minor" | "low" | "suggestion" | "info" | "informational" => "low".to_string(),
        other => bounded_text(other, 100),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(text: &str) -> StepOutput {
        StepOutput {
            step_id: "final".into(),
            step_label: "Final".into(),
            phase: "sequential".into(),
            raw_text: text.into(),
            ..Default::default()
        }
    }

    #[test]
    fn parses_existing_issues_and_source_evidence() {
        let parsed = parse_structured_output(&output(r#"{"issues":[{"id":"rule-a","title":"Broken invariant","severity":"major","section":"src/lib.rs","body":"Details","evidence":[{"source_path":"src/lib.rs","line_start":4,"line_end":7}]}]}"#)).unwrap();
        assert_eq!(parsed.findings[0].priority, "high");
        assert_eq!(parsed.findings[0].evidence[0].source_path, "src/lib.rs");
    }

    #[test]
    fn parses_and_renders_stock_auto_markdown() {
        let parsed = parse_legacy_markdown_output(&output(
            "## Correctness and Internal Consistency\n\n**#1. Sign error in Proposition 2**\n\nThe proof changes sign.\n\n**#2. Missing support condition**\n\nThe claim is too broad.",
        )).unwrap();
        assert_eq!(parsed.findings.len(), 2);
        assert_eq!(
            parsed.findings[0].category,
            "Correctness and Internal Consistency"
        );
        assert!(render_markdown(&parsed).contains("**#2. Missing support condition**"));
    }

    #[test]
    fn narrative_markdown_is_not_misclassified() {
        assert!(parse_legacy_markdown_output(&output("## Summary\n\nOrdinary prose.")).is_none());
    }

    #[test]
    fn interleaved_categories_render_grouped_without_duplicate_headers() {
        let set = FindingSet {
            schema_version: FINDING_SCHEMA_VERSION,
            findings: vec![
                Finding {
                    id: "sign".into(),
                    title: "Sign error".into(),
                    category: CATEGORIES[0].into(),
                    body: "A".into(),
                    ..Default::default()
                },
                Finding {
                    id: "power".into(),
                    title: "Underpowered design".into(),
                    category: CATEGORIES[1].into(),
                    body: "B".into(),
                    ..Default::default()
                },
                Finding {
                    id: "ref".into(),
                    title: "Broken cross-reference".into(),
                    category: CATEGORIES[0].into(),
                    body: "C".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let markdown = render_markdown(&set);
        // One header per category, first-appearance order, renumbered in
        // rendered order.
        assert_eq!(markdown.matches("## Correctness").count(), 1);
        assert!(
            markdown.find("## Correctness").unwrap() < markdown.find("## Methodological").unwrap()
        );
        assert!(markdown.contains("**#2. Broken cross-reference**"));
        assert!(markdown.contains("**#3. Underpowered design**"));
    }

    #[test]
    fn sources_are_parsed_and_bounded() {
        let parsed = parse_structured_output(&output(
            r#"{"findings":[{"id":"a","title":"T","category":"Contribution and Scope","body":"B","sources":["auto_exposition","economics_econometrics"],"evidence":[]}]}"#,
        ))
        .unwrap();
        assert_eq!(
            parsed.findings[0].sources,
            vec!["auto_exposition", "economics_econometrics"]
        );
    }

    #[test]
    fn legacy_products_do_not_promote_structured_specialist_reports() {
        let mut report = PipelineReport {
            orientation: Value::Null,
            step_outputs: vec![output(
                r#"{"findings":[{"title":"T","in_the_paper":"Claim","problem":"Problem","consequence":"Consequence","what_would_help":"Fix","evidence":[]}]}"#,
            )],
            failed_steps: Vec::new(),
            products: RunProducts::default(),
            referee_reports: Vec::new(),
            editor: None,
            report_date: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            paper_hash: String::new(),
        };
        report.step_outputs[0].phase = "parallel".into();
        report.step_outputs[0].structured_json = true;

        ensure_legacy_products(&mut report);

        assert!(report.products.findings.is_none());
    }

    #[test]
    fn canonical_schema_is_portable_and_matches_its_own_product() {
        let schema = output_schema();
        crate::pipeline::structured::provider_schema(&schema).unwrap();
        let product = serde_json::json!({"findings": [{
            "id": "sign-error", "title": "Sign error",
            "category": CATEGORIES[0], "priority": "high",
            "sources": ["auto_consistency"],
            "body": "Body", "evidence": [{"page": 4, "quote": "q"}]
        }]});
        crate::pipeline::structured::validate(&schema, &product).unwrap();
        // Unknown categories and empty ids are rejected by the contract.
        let bad_category = serde_json::json!({"findings": [{
            "id": "x", "title": "T", "category": "Novel Category",
            "body": "B", "evidence": []
        }]});
        assert!(crate::pipeline::structured::validate(&schema, &bad_category).is_err());
        let empty_id = serde_json::json!({"findings": [{
            "id": "", "title": "T", "category": CATEGORIES[0],
            "body": "B", "evidence": []
        }]});
        assert!(crate::pipeline::structured::validate(&schema, &empty_id).is_err());
    }

    #[test]
    fn empty_structured_findings_are_a_valid_product() {
        let parsed = parse_structured_output(&output(r#"{"findings":[]}"#)).unwrap();
        assert!(parsed.findings.is_empty());
        assert_eq!(render_markdown(&parsed), "No findings.");
    }

    #[test]
    fn an_explicit_findings_step_never_falls_back_to_another_step() {
        let mut earlier = output(r#"{"issues":[{"id":"old","title":"Old","body":"Old"}]}"#);
        earlier.step_id = "earlier".into();
        let mut declared = output("not structured");
        declared.step_id = "declared".into();
        let mut config = PipelineConfig {
            steps: Vec::new(),
            merge: Default::default(),
            outputs: Default::default(),
            context_cache: Default::default(),
            use_orientation: true,
            orientation_prompt: String::new(),
            orientation_schema: None,
            extraction: Default::default(),
            parallel_context_template: String::new(),
            variables: Vec::new(),
        };
        config.outputs.primary_step = "declared".into();
        config.outputs.findings_step = "declared".into();

        let products = build_run_products(&config, &[earlier, declared]);
        assert_eq!(products.primary_step_id, "declared");
        assert!(products.findings.is_none());
    }
}
