//! Review-owned disclosure selection and package assembly.
use super::desktop::record_export_path;
use super::files::{
    copy_export_file, copy_export_tree, create_export_stage, finish_export_stage, write_export_json,
};
use super::manifest::finalize_export_metadata;
use super::types::{ExportMode, ExportPackageResult, ExportSelection};
use crate::output;

fn verified_status(status: &str) -> bool {
    matches!(status, "verified" | "verified_with_normalization")
}

fn read_export_text(source: &std::path::Path, name: &str, limit: usize) -> Result<String, String> {
    use std::io::Read;
    let file = super::files::open_export_source(source, std::path::Path::new(name))?;
    let mut text = String::new();
    file.take(limit as u64 + 1)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() > limit {
        return Err(format!("{name} exceeds the export limit"));
    }
    Ok(text)
}

fn read_structured_report(
    source: &std::path::Path,
) -> Result<crate::models::PipelineReport, String> {
    let report_json = read_export_text(source, "report.json", crate::runs::MAX_REPORT_BYTES)?;
    serde_json::from_str(&report_json)
        .map_err(|error| format!("Could not parse structured report: {error}"))
}

/// The findings with verified support, keeping only their verified evidence.
fn verified_findings(report: &crate::models::PipelineReport) -> crate::models::FindingSet {
    let mut findings = report.products.findings.clone().unwrap_or_default();
    findings.findings = findings
        .findings
        .into_iter()
        .filter_map(|mut finding| {
            finding
                .evidence
                .retain(|evidence| verified_status(&evidence.verification_status));
            if verified_status(&finding.verification_status) || !finding.evidence.is_empty() {
                for evidence in &mut finding.evidence {
                    // Internal artifact addresses are not portable and can
                    // reveal the run layout. Human citations remain.
                    evidence.artifact_path.clear();
                }
                Some(finding)
            } else {
                None
            }
        })
        .collect();
    findings
}

/// A report written from verified findings alone. The run's `report.md` is
/// prose that restates every finding, so it cannot be filtered after the fact.
fn verified_report_markdown(findings: &crate::models::FindingSet) -> String {
    let mut report = String::from("# Verified findings\n\n");
    report.push_str(if findings.findings.is_empty() {
        "This Review has no verified findings to share. Unverified findings and the full written report are left out of this package.\n"
    } else {
        "This report lists only findings with verified evidence. Unverified findings and the full written report are left out of this package.\n"
    });
    for (index, finding) in findings.findings.iter().enumerate() {
        report.push_str(&format!("\n## {}. {}\n\n", index + 1, finding.title));
        let labels: Vec<&str> = [&finding.category, &finding.priority]
            .into_iter()
            .map(String::as_str)
            .filter(|label| !label.is_empty())
            .collect();
        if !labels.is_empty() {
            report.push_str(&format!("*{}*\n\n", labels.join(" · ")));
        }
        for (label, text) in [
            ("Problem", &finding.problem),
            ("Consequence", &finding.consequence),
            ("Recommended action", &finding.recommended_action),
        ] {
            if !text.is_empty() {
                report.push_str(&format!("**{label}.** {text}\n\n"));
            }
        }
        if finding.problem.is_empty() && !finding.body.is_empty() {
            report.push_str(&format!("{}\n\n", finding.body));
        }
        for evidence in finding
            .evidence
            .iter()
            .filter(|item| !item.quote.is_empty())
        {
            let page = evidence
                .page
                .map(|page| format!(" (page {page})"))
                .unwrap_or_default();
            let quote = evidence.quote.split_whitespace().collect::<Vec<_>>();
            report.push_str(&format!("> {}{page}\n\n", quote.join(" ")));
        }
    }
    output::normalize_math_delimiters(&output::clean_export_markdown(&report))
}

/// Everything a Shareable package holds. Its report is rendered from verified
/// findings, so that it omits what the package says it omits.
pub(in crate::commands) fn write_shareable_package(
    source: &std::path::Path,
    destination: &std::path::Path,
    manifest: &crate::runs::RunManifest,
) -> Result<(), String> {
    let report = verified_report_markdown(&verified_findings(&read_structured_report(source)?));
    std::fs::write(destination.join("report.md"), report)
        .map_err(|error| format!("Could not write report: {error}"))?;
    write_shareable_products(source, destination, manifest, true, true, false)
}

/// The run's whole report, without the telemetry that exports leave out.
fn write_full_report(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(), String> {
    let report = read_export_text(source, "report.md", crate::runs::MAX_REPORT_BYTES)?;
    std::fs::write(
        destination.join("report.md"),
        output::clean_export_markdown(&report),
    )
    .map_err(|error| format!("Could not write report: {error}"))
}

fn write_shareable_products(
    source: &std::path::Path,
    destination: &std::path::Path,
    manifest: &crate::runs::RunManifest,
    include_findings: bool,
    include_provenance: bool,
    includes_full_report: bool,
) -> Result<(), String> {
    let report = read_structured_report(source)?;
    if include_findings {
        write_export_json(
            destination,
            "verified-findings.json",
            &verified_findings(&report),
        )?;
        write_export_json(destination, "quality.json", &report.quality)?;
        let omissions = if includes_full_report {
            "This package includes the full report.md, with unverified findings and quotes from the source document."
        } else {
            "Shareable exports omit unverified findings, source documents, raw model responses, and logs by default."
        };
        let limitations = if report.quality.limitations.is_empty() {
            format!("# Limitations\n\nNo deterministic limitations were recorded. {omissions}\n")
        } else {
            format!(
                "# Limitations\n\n{}\n\n{omissions}\n",
                report
                    .quality
                    .limitations
                    .iter()
                    .map(|item| format!("- {item}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };
        std::fs::write(destination.join("limitations.md"), limitations)
            .map_err(|error| format!("Could not write limitations: {error}"))?;
    }
    if include_provenance {
        let provenance = serde_json::json!({
            "schemaVersion": 1,
            "runId": manifest.run_id,
            "created": manifest.created,
            "input": {
                "mode": manifest.input_mode,
                "interpretation": manifest.input_interpretation,
                "contentHash": manifest.input_identity.content_hash,
            },
            "profile": { "id": manifest.profile_id, "name": manifest.profile_name },
            "workflow": {
                "fingerprint": manifest.workflow_fingerprint,
                "specialistCatalogRevision": manifest.specialist_catalog_revision,
            },
            "provider": manifest.provider,
            "status": manifest.status,
            "durationSeconds": manifest.duration_secs,
            "usage": manifest.usage,
            "quality": report.quality,
            "redactions": [
                "absolute input paths", "run variables", "credential material",
                "raw model responses", "logs", "source documents"
            ],
        });
        write_export_json(destination, "provenance.json", &provenance)?;
    }
    Ok(())
}

fn custom_artifact_selected(
    artifact: &crate::runs::ArtifactEntry,
    selection: &ExportSelection,
) -> bool {
    let path = artifact.rel_path.as_str();
    if path == "report.md" || path == "findings.json" || path == "manifest.json" {
        return false;
    }
    if path == "context/workflow.json" {
        return selection.workflow;
    }
    if path == "report.json" || artifact.group == "step" || artifact.group == "agent_response" {
        return selection.raw_responses;
    }
    if path.starts_with("logs/") {
        return selection.logs;
    }
    if artifact.group == "source"
        || path.ends_with("context/source-snapshot.json")
        || artifact.group == "document"
        || artifact.group == "pages"
        || artifact.group == "figures"
        || path.starts_with("context/source-evidence/")
        || path == crate::runs::DOCUMENT_TEXT_PATH
    {
        return selection.source_documents;
    }
    selection.supporting_artifacts
        && matches!(artifact.group.as_str(), "product" | "files" | "context")
}

pub(in crate::commands) fn write_custom_payload(
    source: &std::path::Path,
    destination: &std::path::Path,
    manifest: &crate::runs::RunManifest,
    selection: &ExportSelection,
) -> Result<(), String> {
    if selection.report {
        write_full_report(source, destination)?;
    }
    if selection.verified_findings || selection.provenance {
        write_shareable_products(
            source,
            destination,
            manifest,
            selection.verified_findings,
            selection.provenance,
            selection.report,
        )?;
    }
    let mut remaining = super::files::MAX_RUN_EXPORT_BYTES;
    for artifact in &manifest.artifacts {
        if custom_artifact_selected(artifact, selection) {
            copy_export_file(
                source,
                destination,
                &artifact.rel_path,
                Some(artifact),
                &mut remaining,
            )?;
        }
    }
    if selection.source_documents {
        if let Some(pages) = &manifest.page_artifacts {
            for page in 1..=pages.count {
                let relative = pages.rel_path(page)?;
                copy_export_file(source, destination, &relative, None, &mut remaining)?;
            }
        }
    }
    Ok(())
}

fn verified_run_directory(root: &std::path::Path, id: &str) -> Result<std::path::PathBuf, String> {
    let expected = root.join(id);
    let source = expected
        .canonicalize()
        .map_err(|e| format!("Failed to resolve run '{id}': {e}"))?;
    if source != expected || !source.is_dir() {
        return Err("Run export source is invalid".into());
    }
    Ok(source)
}

/// Export a durable run under an explicit disclosure contract. Shareable is
/// intentionally allowlist-only; forensic is intentionally complete and
/// labelled sensitive; custom copies exactly the selected artifact classes.
pub async fn export_run_package(
    run_id: String,
    destination: String,
    mode: ExportMode,
    selection: ExportSelection,
) -> Result<ExportPackageResult, String> {
    tokio::task::spawn_blocking(move || {
        crate::runs::validate_run_id(&run_id)?;
        let runs_root = crate::runs::runs_dir()?
            .canonicalize()
            .map_err(|error| format!("Failed to resolve run storage: {error}"))?;
        let source = verified_run_directory(&runs_root, &run_id)?;
        let manifest = crate::runs::load_manifest(&run_id)?;
        if manifest.run_id != run_id {
            return Err("Review manifest belongs to a different run".into());
        }
        if manifest.status == "running" {
            return Err("Wait for this run to finish before exporting it".into());
        }
        let destination = std::path::PathBuf::from(destination);
        std::fs::create_dir_all(&destination)
            .map_err(|error| format!("Failed to create export destination: {error}"))?;
        let destination = destination
            .canonicalize()
            .map_err(|error| format!("Failed to resolve export destination: {error}"))?;
        if destination.starts_with(&runs_root) {
            return Err(
                "Choose an export destination outside Pipeline's internal run storage".to_string(),
            );
        }
        let stage = create_export_stage(&destination)?;
        match mode {
            ExportMode::Shareable => {
                write_shareable_package(&source, stage.path(), &manifest)?;
            }
            ExportMode::Forensic => {
                copy_export_tree(&source, stage.path())?;
            }
            ExportMode::Custom => {
                if !selection.report
                    && !selection.verified_findings
                    && !selection.provenance
                    && !selection.workflow
                    && !selection.source_documents
                    && !selection.raw_responses
                    && !selection.logs
                    && !selection.supporting_artifacts
                {
                    return Err("Select at least one item for a custom export".to_string());
                }
                write_custom_payload(&source, stage.path(), &manifest, &selection)?;
            }
        }
        let (file_count, bytes, checksum, sensitivity) =
            finalize_export_metadata(stage.path(), &run_id, mode, &selection)?;
        let mode_label = match mode {
            ExportMode::Shareable => "shareable",
            ExportMode::Forensic => "forensic",
            ExportMode::Custom => "custom",
        };
        let exported = finish_export_stage(
            &stage,
            &destination,
            &format!("pipeline-{mode_label}-{run_id}"),
        )?;
        record_export_path(&exported);
        Ok(ExportPackageResult {
            exported_path: exported.to_string_lossy().to_string(),
            file_count,
            bytes,
            checksum,
            mode: mode_label.to_string(),
            sensitivity,
        })
    })
    .await
    .map_err(|error| format!("Run export task failed: {error}"))?
}

/// Legacy in-memory export retained for older frontend builds. It now stages
/// into a new subdirectory, so it cannot overwrite an earlier export or leave
/// a partially written result at the final path.
pub async fn save_all_artifacts(
    dir: String,
    markdown: String,
    extracted_text: String,
    report: crate::models::PipelineReport,
) -> Result<(), String> {
    let destination = std::path::Path::new(&dir);
    let stage = create_export_stage(destination)?;
    let base = stage.path();

    // Final report
    let clean_markdown =
        output::normalize_math_delimiters(&output::clean_export_markdown(&markdown));
    std::fs::write(base.join("report.md"), &clean_markdown)
        .map_err(|e| format!("Failed to write report.md: {e}"))?;

    // Canonical readable document. The argument name remains stable for older
    // frontend builds that still invoke this compatibility command.
    std::fs::write(base.join("document.md"), &extracted_text)
        .map_err(|e| format!("Failed to write document.md: {e}"))?;

    // Orientation map
    let orient_json = serde_json::to_string_pretty(&report.orientation)
        .map_err(|e| format!("Failed to serialize orientation: {e}"))?;
    std::fs::write(base.join("orientation.json"), &orient_json)
        .map_err(|e| format!("Failed to write orientation.json: {e}"))?;

    if let Some(findings) = report.products.findings.as_ref() {
        let findings_json = serde_json::to_string_pretty(findings)
            .map_err(|e| format!("Failed to serialize findings: {e}"))?;
        std::fs::write(base.join("findings.json"), findings_json)
            .map_err(|e| format!("Failed to write findings.json: {e}"))?;
    }

    // Individual step outputs
    let steps_dir = base.join("steps");
    std::fs::create_dir_all(&steps_dir)
        .map_err(|e| format!("Failed to create steps directory: {e}"))?;

    let outputs = report.all_outputs();
    for (i, output) in outputs.iter().enumerate() {
        let slug = output
            .step_id
            .replace('/', "_")
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        let (filename, contents) = if output.structured_json {
            (
                format!("{:02}_{}.json", i + 1, slug),
                output.raw_text.clone(),
            )
        } else {
            let header = format!(
                "# {}\n\n**Phase**: {} · **Agent**: {}\n\n---\n\n",
                output.step_label,
                output.phase,
                if output.agent.is_empty() {
                    "default"
                } else {
                    &output.agent
                },
            );
            (
                format!("{:02}_{}.md", i + 1, slug),
                format!("{}{}", header, output.raw_text),
            )
        };
        std::fs::write(steps_dir.join(&filename), contents)
            .map_err(|e| format!("Failed to write {filename}: {e}"))?;
    }

    finish_export_stage(&stage, destination, "pipeline-core-export")?;
    Ok(())
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn sibling_run_cannot_replace_selected_run() {
        let library = tempfile::tempdir().unwrap();
        let root = library.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("other")).unwrap();
        std::os::unix::fs::symlink(root.join("other"), root.join("selected")).unwrap();
        assert!(verified_run_directory(&root, "selected").is_err());
        assert!(verified_run_directory(&root, "other").is_ok());
    }
    #[test]
    fn captured_sources_follow_only_the_source_documents_choice() {
        for (path, group) in [
            ("source-snapshot/paper.tex", "source"),
            ("context/source-snapshot.json", "context"),
        ] {
            let artifact = crate::runs::ArtifactEntry {
                rel_path: path.into(),
                group: group.into(),
                kind: "text".into(),
                label: String::new(),
                bytes: 0,
                sha256: String::new(),
            };
            for selected in [true, false] {
                let selection = ExportSelection {
                    source_documents: selected,
                    supporting_artifacts: !selected,
                    ..Default::default()
                };
                assert_eq!(custom_artifact_selected(&artifact, &selection), selected);
            }
        }
    }
}
