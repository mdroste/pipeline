use super::{parse::parse_value, MAX_FINDINGS};
use crate::models::{Finding, FindingSet, StepOutput};
use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

static MARKDOWN_FINDING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\*\*#([0-9]+)\.\s+(.+?)\*\*\s*$").expect("legacy finding-title regex is invalid")
});

/// Legacy inference has no workflow contract to identify the published step.
/// Do not mistake the distinctive per-reviewer Auto Review artifact for the
/// consolidated product when a run stopped before synthesis or validation.
pub(super) fn parse_legacy_structured_output(output: &StepOutput) -> Option<FindingSet> {
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

pub(super) fn parse_legacy_markdown_output(output: &StepOutput) -> Option<FindingSet> {
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
        schema_version: 1,
        source_step_id: output.step_id.clone(),
        source_step_label: output.step_label.clone(),
        taxonomy: Vec::new(),
        findings,
    })
}
