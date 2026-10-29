//! Immutable Review results, abandoned pins and recovery conversion.
use super::atomic_json;
use crate::orchestration::store::{err, Result, Store};
use serde_json::{json, Value};
use std::{io::Read, path::Path};

pub(crate) fn normalize_review(value: Value, artifact: Value) -> Result<Value> {
    let report: crate::models::PipelineReport =
        serde_json::from_value(value["report"].clone()).map_err(err)?;
    let findings = report.products.findings.as_ref();
    let high = findings
        .map(|f| f.findings.iter().filter(|f| f.priority == "high").count())
        .unwrap_or(0);
    let unknown = findings
        .map(|f| {
            f.findings
                .iter()
                .filter(|f| !["high", "medium", "low"].contains(&f.priority.as_str()))
                .count()
        })
        .unwrap_or(1);
    let complete =
        report.quality.status == "done" && report.failed_steps.is_empty() && findings.is_some();
    let mut markdown = value["markdown"].as_str().unwrap_or_default().to_string();
    if markdown.len() > 128 * 1024 {
        let mut n = 128 * 1024;
        while !markdown.is_char_boundary(n) {
            n -= 1;
        }
        markdown.truncate(n);
        markdown.push_str(
            "\n[Report excerpt truncated; open the retained Review run for the full report.]",
        );
    }
    let text = value["extracted_text"]
        .as_str()
        .filter(|v| v.len() <= 128 * 1024);
    let mut selected = Vec::new();
    let mut bytes = 0usize;
    if let Some(product) = findings {
        for finding in &product.findings {
            let value = serde_json::to_value(finding).map_err(err)?;
            let size = value.to_string().len();
            if bytes + size > 128 * 1024 || selected.len() >= 200 {
                break;
            }
            bytes += size;
            selected.push(value);
        }
    }
    Ok(
        json!({"revisionTextAvailable":text.is_some(),"findingsTruncated":findings.is_some_and(|f|f.findings.len()>selected.len()),"runId":value["run_id"],"complete":complete,"highPriorityCount":high,"unknownPriorityCount":unknown,"report":markdown,"paperText":text,"reviewedArtifact":artifact,"quality":report.quality,"findings":selected}),
    )
}

/// Reconstruct abandonment from durable stop/retry records, including a crash
/// after Task state committed but before its Review pin was released.
pub fn reconcile_abandoned_reviews(store: &Store) -> Result<()> {
    for operation in store.abandoned_operations()? {
        if operation.len() != 32 || !operation.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Invalid abandoned Task operation".into());
        }
        let path = store
            .root
            .join("actions")
            .join(&operation)
            .join("review-run.json");
        if !path.try_exists().map_err(err)? {
            continue;
        }
        let mut bytes = Vec::new();
        crate::safety::open_regular_file(&path)?
            .take(4097)
            .read_to_end(&mut bytes)
            .map_err(err)?;
        if bytes.len() > 4096 {
            return Err("Review ownership journal exceeds its limit".into());
        }
        let metadata: Value = serde_json::from_slice(&bytes).map_err(err)?;
        crate::commands::orchestration::release_abandoned_pin(
            metadata["runId"]
                .as_str()
                .ok_or("Review ownership journal omitted its run")?,
            &operation,
        )?;
    }
    Ok(())
}

pub fn recover_review(store: &Store, operation: &str) -> Result<Option<Value>> {
    let root = store.root.join("actions").join(operation);
    let input = root.join("review-input.json");
    if !input.is_file() {
        return Ok(None);
    }
    let read = |path: &Path| -> Result<Value> {
        serde_json::from_slice(&std::fs::read(path).map_err(err)?).map_err(err)
    };
    let artifact = read(&input)?;
    let raw = root.join("review-raw.json");
    let result = if raw.is_file() {
        Some(read(&raw)?)
    } else if root.join("review-run.json").is_file() {
        let metadata = read(&root.join("review-run.json"))?;
        crate::commands::orchestration::recovered_result(
            metadata["runId"]
                .as_str()
                .ok_or("Review receipt omitted run identity")?,
        )?
    } else {
        None
    };
    if let Some(value) = result {
        atomic_json(&raw, &value)?;
        let result = normalize_review(value, artifact)?;
        atomic_json(&root.join("result.json"), &result)?;
        Ok(Some(result))
    } else {
        Ok(None)
    }
}
