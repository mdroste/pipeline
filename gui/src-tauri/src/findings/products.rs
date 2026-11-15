use super::{
    legacy::{parse_legacy_markdown_output, parse_legacy_structured_output},
    parse::{bounded_value, parse_structured_output},
    FINDING_SCHEMA_VERSION, MAX_BODY_BYTES, MAX_REFERENCE_BYTES,
};
use crate::models::{FindingSet, PipelineReport, RunProducts, StepOutput, ValidationDisposition};
use crate::pipeline_config::PipelineConfig;
use serde_json::Value;

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

    let findings_output = if config.outputs.findings_step.trim().is_empty() {
        // Like the primary step, an undeclared findings product never comes
        // from one parallel reviewer's output.
        outputs
            .iter()
            .rev()
            .filter(|output| usable(output) && output.phase == "sequential")
            .find(|output| parse_structured_output(output).is_some())
    } else {
        declared_output(outputs, &config.outputs.findings_step)
    };
    let mut findings = findings_output.and_then(parse_structured_output);
    if let Some(findings) = findings
        .as_mut()
        .filter(|_| crate::auto_review::uses_auto_review_contract(config))
    {
        // Paper reviewers read the manuscript as input/main/document.md.
        // Legacy findings sometimes put that filename in source_path
        // alongside a page or node. Preserve the manuscript locator so the
        // verifier checks the paper instead of looking for a source file.
        for evidence in findings
            .findings
            .iter_mut()
            .flat_map(|finding| &mut finding.evidence)
        {
            if matches!(evidence.evidence_type.as_str(), "" | "document")
                && matches!(
                    evidence.source_path.as_str(),
                    "document.md" | "input/main/document.md"
                )
                && evidence.artifact_path.is_empty()
                && evidence.source_hash.is_empty()
                && evidence.line_start.is_none()
                && (evidence.page.is_some()
                    || !evidence.node_id.is_empty()
                    || !evidence.asset_id.is_empty())
            {
                evidence.source_path.clear();
            }
        }
    }
    let validation_dispositions = findings_output
        .map(parse_validation_dispositions)
        .unwrap_or_default();
    let named = config
        .outputs
        .named
        .iter()
        .filter_map(|spec| {
            let output = declared_output(outputs, &spec.step)?;
            let content = if output.structured_json || spec.viewer == "json" {
                crate::pipeline::structured::extract_json(&output.raw_text)
                    .unwrap_or_else(|| Value::String(output.raw_text.clone()))
            } else {
                Value::String(output.raw_text.clone())
            };
            Some(crate::models::NamedRunProduct {
                key: spec.key.clone(),
                source_step_id: output.step_id.clone(),
                media_type: spec.media_type.clone(),
                viewer: spec.viewer.clone(),
                export_policy: spec.export_policy.clone(),
                sensitivity: spec.sensitivity.clone(),
                content,
            })
        })
        .collect();

    let product_version = findings
        .as_ref()
        .map(|findings| findings.schema_version)
        .unwrap_or(FINDING_SCHEMA_VERSION);
    RunProducts {
        schema_version: product_version,
        primary_step_id,
        findings,
        validation_dispositions,
        named,
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
        schema_version: 1,
        primary_step_id,
        findings,
        validation_dispositions: Vec::new(),
        named: Vec::new(),
    };
}

fn parse_validation_dispositions(output: &StepOutput) -> Vec<ValidationDisposition> {
    let Some(value) = crate::pipeline::structured::extract_json(&output.raw_text) else {
        return Vec::new();
    };
    value
        .get("validation_dispositions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let item = item.as_object()?;
            let finding_id = bounded_value(item.get("finding_id"), MAX_REFERENCE_BYTES);
            let disposition = bounded_value(item.get("disposition"), MAX_REFERENCE_BYTES);
            let reason = bounded_value(item.get("reason"), MAX_BODY_BYTES);
            if finding_id.is_empty() || disposition.is_empty() || reason.is_empty() {
                return None;
            }
            Some(ValidationDisposition {
                finding_id,
                disposition,
                reason,
                merged_into: bounded_value(item.get("merged_into"), MAX_REFERENCE_BYTES),
                before_hash: bounded_value(item.get("before_hash"), MAX_REFERENCE_BYTES),
                after_hash: bounded_value(item.get("after_hash"), MAX_REFERENCE_BYTES),
            })
        })
        .collect()
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

pub(super) fn declared_output<'a>(
    outputs: &'a [StepOutput],
    step_id: &str,
) -> Option<&'a StepOutput> {
    let step_id = step_id.trim();
    (!step_id.is_empty()).then_some(())?;
    outputs
        .iter()
        .rev()
        .find(|output| usable(output) && base_step_id(&output.step_id) == step_id)
}

/// Only a sequential step can be the inferred primary. With none, the primary
/// stays empty so `final_output` can apply its failed-report guard, which also
/// keeps the last output of a workflow that has only parallel steps.
fn inferred_primary_output(outputs: &[StepOutput]) -> Option<&StepOutput> {
    outputs
        .iter()
        .rev()
        .find(|output| usable(output) && output.phase == "sequential")
}

fn usable(output: &StepOutput) -> bool {
    !output.skipped && !output.raw_text.trim().is_empty()
}

fn base_step_id(step_id: &str) -> &str {
    step_id.split('/').next().unwrap_or(step_id)
}
