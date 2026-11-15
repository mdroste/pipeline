use super::{
    FINDING_SCHEMA_VERSION, MAX_BODY_BYTES, MAX_EVIDENCE, MAX_FINDINGS, MAX_LINE,
    MAX_REFERENCE_BYTES, MAX_SOURCES, MAX_TITLE_BYTES,
};
use crate::models::{Finding, FindingEvidence, FindingSet, StepOutput};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::path::{Component, Path};

pub(super) fn parse_structured_output(output: &StepOutput) -> Option<FindingSet> {
    let value = crate::pipeline::structured::extract_json(&output.raw_text)?;
    parse_value(&value, &output.step_id, &output.step_label)
}

pub fn parse_value(
    value: &Value,
    source_step_id: &str,
    source_step_label: &str,
) -> Option<FindingSet> {
    let schema_version = value
        .get("schema_version")
        .and_then(Value::as_u64)
        .and_then(|version| u32::try_from(version).ok())
        .unwrap_or(1);
    let strict_v2 = schema_version == FINDING_SCHEMA_VERSION;
    let taxonomy = value
        .get("taxonomy")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
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
        let mut body = bounded_value(
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
        let problem = bounded_value(object.get("problem"), MAX_BODY_BYTES);
        let consequence = bounded_value(object.get("consequence"), MAX_BODY_BYTES);
        let recommended_action = bounded_value(
            first(object, &["recommended_action", "what_would_help"]),
            MAX_BODY_BYTES,
        );
        if body.is_empty() && strict_v2 {
            body = format!(
                "**Problem.** {problem}\n\n**Consequence.** {consequence}\n\n**Recommended action.** {recommended_action}"
            );
        }
        let mut id = bounded_value(
            first(object, &["id", "issue_id", "issueId", "fingerprint", "key"]),
            MAX_REFERENCE_BYTES,
        );
        if strict_v2 && id.is_empty() {
            return None;
        }
        if id.is_empty() {
            id = (index + 1).to_string();
        }
        if strict_v2 {
            if id.is_empty() || !used_ids.insert(id.clone()) {
                return None;
            }
        } else {
            let base_id = id.clone();
            let mut suffix = 2usize;
            while !used_ids.insert(id.clone()) {
                id = bounded_text(&format!("{base_id}#{suffix}"), MAX_REFERENCE_BYTES);
                suffix += 1;
            }
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
        let reviewer_ids = string_array(
            object.get("reviewer_ids").or_else(|| object.get("sources")),
            MAX_SOURCES,
        );
        let source_call_ids = string_array(object.get("source_call_ids"), MAX_SOURCES);
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
        let confidence = bounded_value(object.get("confidence"), 100);
        let verification_status =
            bounded_value(object.get("verification_status"), MAX_REFERENCE_BYTES);
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
            rank: bounded_u32(object.get("rank"), MAX_FINDINGS as u32)
                .unwrap_or((index + 1) as u32),
            id,
            source_key,
            sources,
            reviewer_ids,
            source_call_ids,
            title: if title.is_empty() {
                bounded_text(&body, MAX_TITLE_BYTES)
            } else {
                title
            },
            category,
            priority,
            confidence,
            verification_status,
            problem,
            consequence,
            recommended_action,
            body,
            evidence,
        });
    }
    Some(FindingSet {
        schema_version: if strict_v2 { FINDING_SCHEMA_VERSION } else { 1 },
        source_step_id: bounded_text(source_step_id, MAX_REFERENCE_BYTES),
        source_step_label: bounded_text(source_step_label, MAX_TITLE_BYTES),
        taxonomy,
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
    let url = bounded_value(object.get("url"), MAX_BODY_BYTES);
    let doi = bounded_value(object.get("doi"), MAX_REFERENCE_BYTES);
    let publisher = bounded_value(object.get("publisher"), MAX_REFERENCE_BYTES);
    let accessed_at = bounded_value(object.get("accessed_at"), MAX_REFERENCE_BYTES);
    let query_id = bounded_value(object.get("query_id"), MAX_REFERENCE_BYTES);
    let call_id = bounded_value(object.get("call_id"), MAX_REFERENCE_BYTES);
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
        && url.is_empty()
        && doi.is_empty()
        && query_id.is_empty()
        && call_id.is_empty()
        && description.is_empty()
        && quote.is_empty()
    {
        return None;
    }
    Some(FindingEvidence {
        evidence_type: bounded_value(object.get("evidence_type"), 100),
        verification_status: bounded_value(object.get("verification_status"), 100),
        page,
        line_start,
        line_end,
        node_id,
        asset_id,
        artifact_path,
        source_path,
        source_hash,
        url,
        doi,
        publisher,
        accessed_at,
        query_id,
        call_id,
        description,
        quote,
    })
}

fn string_array(value: Option<&Value>, maximum: usize) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(maximum)
        .filter_map(|value| {
            let value = bounded_value(Some(value), MAX_REFERENCE_BYTES);
            (!value.is_empty()).then_some(value)
        })
        .collect()
}

fn first<'a>(object: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| object.get(*key))
}

pub(super) fn bounded_value(value: Option<&Value>, maximum: usize) -> String {
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
