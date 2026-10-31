use crate::document_bundle::DocumentBundle;
use crate::models::{ExtractionResult, ReportQuality, RunProducts, StepFailure, StepOutput};
use crate::pipeline::extract::is_withheld_image_text_note;
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
        // attempt_count sums every provider call of the step (agents, merge,
        // fan-out units); only per-call retries are schema retries.
        let retries = if output.calls.is_empty() {
            output.attempt_count.saturating_sub(1)
        } else {
            output.calls.iter().fold(0u32, |total, call| {
                total.saturating_add(call.attempt_count.saturating_sub(1))
            })
        };
        schema_retry_events = schema_retry_events.saturating_add(retries);
    }

    let mut verified_evidence = 0u32;
    let mut partially_verified_evidence = 0u32;
    let mut unverified_evidence = 0u32;
    let mut not_checkable_evidence = 0u32;
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
                "not_checkable" => {
                    not_checkable_evidence = not_checkable_evidence.saturating_add(1)
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
        // Extraction-scoped bundle notes restate `extraction.quality_notes`;
        // leaving them unprefixed lets the dedup below remove the copies.
        extraction_notes.extend(bundle.quality.iter().map(|note| {
            if note.scope.trim().is_empty() || note.scope == "extraction" {
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
    if not_checkable_evidence > 0 {
        limitations.push(format!(
            "{not_checkable_evidence} evidence item(s) could not be checked automatically."
        ));
    }
    if manual_review_items > 0 {
        limitations.push(format!(
            "{manual_review_items} finding(s) were deferred for manual review or lacked verifiable evidence."
        ));
    }

    let status = if failures.is_empty() {
        if extraction_notes
            .iter()
            .all(|note| is_withheld_image_text_note(note))
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
        not_checkable_evidence,
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

    #[test]
    fn not_checkable_evidence_is_reported_without_degrading_the_run() {
        let extraction = ExtractionResult {
            text: "input".into(),
            method: "test".into(),
            source_path: "input.md".into(),
            paper_hash: "abc".into(),
            quality_notes: Vec::new(),
        };
        let sample = crate::pipeline::orient::orientation_sample(&extraction.text);
        let products = |statuses: &[&str]| RunProducts {
            schema_version: 2,
            primary_step_id: "review".into(),
            findings: Some(crate::models::FindingSet {
                findings: vec![crate::models::Finding {
                    evidence: statuses
                        .iter()
                        .map(|status| crate::models::FindingEvidence {
                            verification_status: (*status).into(),
                            ..Default::default()
                        })
                        .collect(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            validation_dispositions: Vec::new(),
            named: Vec::new(),
        };
        let config = crate::pipeline_config::workflow_template().unwrap().config;
        let quality = |statuses: &[&str]| {
            build_report_quality(
                &extraction,
                None,
                &sample,
                &config,
                &[],
                &[],
                &products(statuses),
            )
        };

        let checked = quality(&["verified", "not_checkable", "not_checkable"]);
        assert_eq!(checked.status, "done");
        assert_eq!(checked.not_checkable_evidence, 2);
        assert_eq!(checked.unverified_evidence, 0);
        assert!(checked
            .limitations
            .contains(&"2 evidence item(s) could not be checked automatically.".to_string()));

        // A missing quote and a wrong page are still real evidence problems.
        for status in ["not_found", "partially_verified"] {
            assert_eq!(quality(&["not_checkable", status]).status, "degraded");
        }
    }

    #[test]
    fn extraction_notes_mirrored_by_the_bundle_are_reported_once() {
        let note = "Page 45: recovered incomplete layout recognition.";
        let extraction = ExtractionResult {
            text: "input".into(),
            method: "test".into(),
            source_path: "input.md".into(),
            paper_hash: "abc".into(),
            quality_notes: vec![note.into()],
        };
        let run = tempfile::tempdir().unwrap();
        let bundle = crate::document_bundle::build(&extraction, run.path())
            .unwrap()
            .bundle;
        let sample = crate::pipeline::orient::orientation_sample(&extraction.text);
        let products = RunProducts {
            schema_version: 2,
            primary_step_id: "review".into(),
            findings: None,
            validation_dispositions: Vec::new(),
            named: Vec::new(),
        };
        let config = crate::pipeline_config::workflow_template().unwrap().config;
        let quality = build_report_quality(
            &extraction,
            Some(&bundle),
            &sample,
            &config,
            &[],
            &[],
            &products,
        );
        assert_eq!(quality.extraction_notes, vec![note.to_string()]);
        assert_eq!(
            quality
                .limitations
                .iter()
                .filter(|limitation| limitation.contains(note))
                .count(),
            1
        );
    }

    #[test]
    fn multi_call_steps_count_retries_per_call_not_per_step() {
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
            findings: None,
            validation_dispositions: Vec::new(),
            named: Vec::new(),
        };
        let config = crate::pipeline_config::workflow_template().unwrap().config;
        let call = |attempt_count: u32| crate::models::StepCallRecord {
            attempt_count,
            ..Default::default()
        };
        let first_try = StepOutput {
            step_id: "review".into(),
            raw_text: "merged".into(),
            attempt_count: 3,
            calls: vec![call(1), call(1), call(1)],
            ..Default::default()
        };
        let quality = build_report_quality(
            &extraction,
            None,
            &sample,
            &config,
            &[first_try],
            &[],
            &products,
        );
        assert_eq!(quality.schema_retry_events, 0);
        assert_eq!(quality.status, "done");
        let retried = StepOutput {
            step_id: "review".into(),
            raw_text: "merged".into(),
            attempt_count: 4,
            calls: vec![call(2), call(1), call(1)],
            ..Default::default()
        };
        let quality = build_report_quality(
            &extraction,
            None,
            &sample,
            &config,
            &[retried],
            &[],
            &products,
        );
        assert_eq!(quality.schema_retry_events, 1);
    }

    #[test]
    fn withheld_image_text_is_listed_without_degrading_the_run() {
        let note = "Image text withheld as unverified: 2 image region(s) on page(s) 74 returned the kind of data the recognizer makes up for plots.";
        let quality = |notes: &[&str]| {
            let extraction = ExtractionResult {
                text: "input".into(),
                method: "test".into(),
                source_path: "input.md".into(),
                paper_hash: "abc".into(),
                quality_notes: notes.iter().map(|note| (*note).to_string()).collect(),
            };
            let sample = crate::pipeline::orient::orientation_sample(&extraction.text);
            let products = RunProducts {
                schema_version: 2,
                primary_step_id: "review".into(),
                findings: None,
                validation_dispositions: Vec::new(),
                named: Vec::new(),
            };
            let config = crate::pipeline_config::workflow_template().unwrap().config;
            build_report_quality(&extraction, None, &sample, &config, &[], &[], &products)
        };

        let withheld = quality(&[note]);
        assert_eq!(withheld.status, "done");
        assert_eq!(withheld.extraction_notes, vec![note.to_string()]);
        assert!(withheld
            .limitations
            .contains(&format!("Extraction: {note}")));

        // Any other extraction note still marks the run.
        let recovered = "Page 45: recovered incomplete layout recognition.";
        assert_eq!(quality(&[note, recovered]).status, "degraded");
    }
}
