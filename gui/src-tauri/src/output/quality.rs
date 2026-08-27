use crate::document_bundle::DocumentBundle;
use crate::models::{ExtractionResult, ReportQuality, RunProducts, StepFailure, StepOutput};
use crate::pipeline::orient::OrientationSample;
use crate::pipeline_config::PipelineConfig;
use sha2::{Digest as _, Sha256};
use std::collections::BTreeSet;

/// Build the provider-independent report quality ledger from durable inputs.
pub fn build_report_quality(
    extraction: &ExtractionResult,
    bundle: Option<&DocumentBundle>,
    orientation_sample: &OrientationSample,
    config: &PipelineConfig,
    outputs: &[StepOutput],
    failures: &[StepFailure],
    products: &RunProducts,
) -> ReportQuality {
    let completed_steps = unique_sorted(
        outputs
            .iter()
            .filter(|output| !output.skipped && !output.raw_text.trim().is_empty())
            .map(|output| output.step_id.clone()),
    );
    let skipped_steps = unique_sorted(
        outputs
            .iter()
            .filter(|output| output.skipped)
            .map(|output| output.step_id.clone()),
    );
    let failed_steps = unique_sorted(failures.iter().map(|failure| failure.step_id.clone()));
    let requested_capabilities = unique_sorted(
        config
            .steps
            .iter()
            .filter(|step| step.enabled)
            .flat_map(|step| step.tools.iter().cloned()),
    );

    let mut observed = BTreeSet::new();
    let mut providers = BTreeSet::new();
    let mut schema_retry_events = 0u32;
    for output in outputs.iter().filter(|output| !output.skipped) {
        if output.tool_calls.text_file > 0 {
            observed.insert("Read".to_string());
        }
        if output.tool_calls.image > 0 {
            observed.insert("Image".to_string());
        }
        if output.tool_calls.web > 0 {
            observed.insert("WebSearch".to_string());
        }
        if output.tool_calls.shell_or_other > 0 {
            observed.insert("Other".to_string());
        }
        if output.tool_calls.unknown > 0 {
            observed.insert("Unknown".to_string());
        }
        if !output.provider.trim().is_empty() {
            providers.insert(output.provider.clone());
        }
        schema_retry_events =
            schema_retry_events.saturating_add(output.attempt_count.saturating_sub(1));
    }

    let mut verified_evidence = 0u32;
    let mut partially_verified_evidence = 0u32;
    let mut unverified_evidence = 0u32;
    if let Some(findings) = products.findings.as_ref() {
        for evidence in findings
            .findings
            .iter()
            .flat_map(|finding| finding.evidence.iter())
        {
            match evidence.verification_status.as_str() {
                "verified" | "verified_with_normalization" => {
                    verified_evidence = verified_evidence.saturating_add(1)
                }
                "partially_verified" | "partial" => {
                    partially_verified_evidence = partially_verified_evidence.saturating_add(1)
                }
                _ => unverified_evidence = unverified_evidence.saturating_add(1),
            }
        }
    }
    let manual_review_items = products
        .validation_dispositions
        .iter()
        .filter(|entry| {
            matches!(
                entry.disposition.as_str(),
                "unverified_missing_evidence" | "deferred_manual_review"
            )
        })
        .count() as u32;

    let mut extraction_notes = extraction.quality_notes.clone();
    if let Some(bundle) = bundle {
        extraction_notes.extend(bundle.quality.iter().map(|note| {
            if note.scope.trim().is_empty() {
                note.message.clone()
            } else {
                format!("{}: {}", note.scope, note.message)
            }
        }));
    }
    extraction_notes.sort();
    extraction_notes.dedup();

    let mut limitations = Vec::new();
    if orientation_sample.omitted_bytes > 0 {
        limitations.push(format!(
            "Orientation used {} of {} extracted bytes; {} bytes were outside the deterministic routing sample.",
            orientation_sample.included_bytes,
            orientation_sample.included_bytes + orientation_sample.omitted_bytes,
            orientation_sample.omitted_bytes
        ));
    }
    for note in &extraction_notes {
        limitations.push(format!("Extraction: {note}"));
    }
    if !failed_steps.is_empty() {
        limitations.push(format!(
            "Failed or dependency-blocked steps: {}.",
            failed_steps.join(", ")
        ));
    }
    if !skipped_steps.is_empty() {
        limitations.push(format!(
            "Conditionally skipped steps: {}.",
            skipped_steps.join(", ")
        ));
    }
    if schema_retry_events > 0 {
        limitations.push(format!(
            "Provider responses required {schema_retry_events} additional attempt(s)."
        ));
    }
    if partially_verified_evidence > 0 || unverified_evidence > 0 {
        limitations.push(format!(
            "Evidence requiring caution: {partially_verified_evidence} partially verified and {unverified_evidence} unverified item(s)."
        ));
    }
    if manual_review_items > 0 {
        limitations.push(format!(
            "{manual_review_items} finding(s) were deferred for manual review or lacked verifiable evidence."
        ));
    }

    let status = if failures.is_empty() {
        if extraction_notes.is_empty()
            && schema_retry_events == 0
            && partially_verified_evidence == 0
            && unverified_evidence == 0
            && manual_review_items == 0
        {
            "done"
        } else {
            "degraded"
        }
    } else if completed_steps.is_empty() {
        "failed"
    } else {
        "partial"
    }
    .to_string();

    let reproducibility_hash = reproducibility_hash(
        &extraction.paper_hash,
        config,
        orientation_sample,
        outputs,
        products,
    );
    ReportQuality {
        schema_version: 1,
        status,
        extraction_method: extraction.method.clone(),
        input_bytes: extraction.text.len() as u64,
        orientation_included_bytes: orientation_sample.included_bytes as u64,
        orientation_omitted_bytes: orientation_sample.omitted_bytes as u64,
        recovered_pages: bundle.map_or(0, |value| value.pages.len() as u32),
        recovered_blocks: bundle.map_or(0, |value| value.nodes.len() as u32),
        recovered_assets: bundle.map_or(0, |value| value.assets.len() as u32),
        extraction_notes,
        completed_steps,
        failed_steps,
        skipped_steps,
        requested_capabilities,
        observed_capabilities: observed.into_iter().collect(),
        external_providers: providers.into_iter().collect(),
        schema_retry_events,
        verified_evidence,
        partially_verified_evidence,
        unverified_evidence,
        manual_review_items,
        limitations,
        reproducibility_hash,
    }
}

fn unique_sorted(values: impl Iterator<Item = String>) -> Vec<String> {
    values.collect::<BTreeSet<_>>().into_iter().collect()
}

fn reproducibility_hash(
    paper_hash: &str,
    config: &PipelineConfig,
    sample: &OrientationSample,
    outputs: &[StepOutput],
    products: &RunProducts,
) -> String {
    let payload = serde_json::json!({
        "paper_hash": paper_hash,
        "workflow": config,
        "orientation_ranges": sample.ranges,
        "outputs": outputs,
        "products": products,
    });
    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_is_deterministic_and_exposes_unverified_evidence() {
        let extraction = ExtractionResult {
            text: "input".into(),
            method: "test".into(),
            source_path: "input.md".into(),
            paper_hash: "abc".into(),
            quality_notes: Vec::new(),
        };
        let sample = crate::pipeline::orient::orientation_sample(&extraction.text);
        let products = RunProducts {
            schema_version: 2,
            primary_step_id: "review".into(),
            findings: Some(crate::models::FindingSet {
                findings: vec![crate::models::Finding {
                    evidence: vec![crate::models::FindingEvidence {
                        verification_status: "unverified".into(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }),
            validation_dispositions: Vec::new(),
            named: Vec::new(),
        };
        let config = crate::pipeline_config::workflow_template().unwrap().config;
        let first = build_report_quality(&extraction, None, &sample, &config, &[], &[], &products);
        let second = build_report_quality(&extraction, None, &sample, &config, &[], &[], &products);
        assert_eq!(first, second);
        assert_eq!(first.status, "degraded");
        assert_eq!(first.unverified_evidence, 1);
    }
}
