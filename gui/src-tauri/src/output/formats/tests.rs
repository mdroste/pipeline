//! Round-trip tests for the shareable report formats.

use super::*;
use std::io::{Read, Write};

mod docx;
mod html;
mod pdf;

fn evidence(quote: &str, page: Option<u32>, status: &str) -> ExportEvidence {
    ExportEvidence {
        verification: Verification::from_status(status),
        page,
        locator: page.map(|page| format!("Page {page}")).unwrap_or_default(),
        quote: quote.to_string(),
    }
}

fn finding(
    number: usize,
    title: &str,
    severity: &str,
    evidence: Vec<ExportEvidence>,
) -> ExportFinding {
    ExportFinding {
        number,
        title: title.to_string(),
        category: if number.is_multiple_of(2) {
            "Exposition".to_string()
        } else {
            "Correctness".to_string()
        },
        severity: severity.to_string(),
        confidence: "medium".to_string(),
        problem: format!("Problem for finding {number}."),
        consequence: "The conclusion may not follow.".to_string(),
        recommended_action: "Report the corrected estimate.".to_string(),
        body: String::new(),
        evidence,
    }
}

fn export_with(findings: Vec<ExportFinding>) -> ReportExport {
    ReportExport {
        meta: ReportMetadata {
            title: "Review of Minimum Wages & Employment".to_string(),
            input_name: "draft.docx".to_string(),
            workflow: "Paper Review".to_string(),
            date: "2026-10-28".to_string(),
            status: "Done".to_string(),
        },
        findings,
        report_markdown: "# Referee Report\n\nFull text.".to_string(),
        quality: crate::models::ReportQuality {
            schema_version: 1,
            limitations: vec!["Equations were extracted as text.".to_string()],
            ..Default::default()
        },
    }
}

/// Every part must be well-formed XML with matching end tags.
fn assert_well_formed(xml: &str) {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().check_end_names = true;
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(_) => {}
            Err(error) => panic!("malformed XML: {error}\n{xml}"),
        }
    }
}

fn zip_parts(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|index| {
            let mut entry = archive.by_index(index).unwrap();
            let mut content = Vec::new();
            entry.read_to_end(&mut content).unwrap();
            (entry.name().unwrap().into_owned(), content)
        })
        .collect()
}

fn zip_raw(bytes: &[u8], name: &str) -> (Vec<u8>, u32, u64) {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let index = archive.index_for_name(name).unwrap();
    let mut entry = archive.by_index_raw(index).unwrap();
    let crc = entry.crc32();
    let size = entry.size();
    let mut raw = Vec::new();
    entry.read_to_end(&mut raw).unwrap();
    (raw, crc, size)
}

fn part(bytes: &[u8], name: &str) -> String {
    let (_, content) = zip_parts(bytes)
        .into_iter()
        .find(|(part, _)| part == name)
        .unwrap_or_else(|| panic!("missing part {name}"));
    String::from_utf8(content).unwrap()
}

fn docx_text(bytes: &[u8]) -> String {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("document.docx");
    std::fs::File::create(&path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
    crate::document_bundle::extract_docx_text(&path).unwrap()
}

#[test]
fn report_model_numbers_findings_like_the_markdown_report() {
    use crate::models::{Finding, FindingEvidence, FindingSet, PipelineReport, RunProducts};
    let raw = |id: &str, category: &str| Finding {
        id: id.to_string(),
        title: format!("Title {id}"),
        category: category.to_string(),
        priority: "high".to_string(),
        body: format!("Body {id}"),
        evidence: vec![FindingEvidence {
            verification_status: "verified_with_normalization".to_string(),
            page: Some(4),
            line_start: Some(10),
            line_end: Some(12),
            artifact_path: "artifacts/by-step/x.json".to_string(),
            node_id: "n-17".to_string(),
            description: "Section 4.2".to_string(),
            quote: " exact words ".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let report: PipelineReport = serde_json::from_value(serde_json::json!({
        "report_date": "2026-10-28"
    }))
    .unwrap();
    let report = PipelineReport {
        products: RunProducts {
            schema_version: 1,
            findings: Some(FindingSet {
                findings: vec![raw("a", "Second"), raw("b", "First"), raw("c", "Second")],
                ..Default::default()
            }),
            ..Default::default()
        },
        ..report
    };
    let export = ReportExport::from_report(
        &report,
        "# Report\n<!-- PIPELINE RUN DETAILS START -->\nsecret\n<!-- PIPELINE RUN DETAILS END -->\nBody",
        ReportMetadata::default(),
    );
    let order: Vec<(usize, &str)> = export
        .findings
        .iter()
        .map(|finding| (finding.number, finding.title.as_str()))
        .collect();
    assert_eq!(order, vec![(1, "Title a"), (2, "Title c"), (3, "Title b")]);
    let item = &export.findings[0].evidence[0];
    assert_eq!(item.verification, Verification::VerifiedWithNormalization);
    assert_eq!(item.locator, "Page 4 · lines 10–12 · Section 4.2");
    assert_eq!(item.quote, "exact words");
    assert!(!export.report_markdown.contains("secret"));
    assert_eq!(export.categories(), vec!["Second", "First"]);
}

#[test]
fn verification_labels_cover_every_host_status_and_default_to_not_checked() {
    for (status, slug) in [
        ("verified", "verified"),
        ("verified_with_normalization", "verified-normalized"),
        ("partially_verified", "partial"),
        ("not_found", "not-found"),
        ("not_checkable", "not-checkable"),
        ("unverified", "unchecked"),
        ("", "unchecked"),
        ("something else", "unchecked"),
    ] {
        assert_eq!(Verification::from_status(status).slug(), slug);
    }
    assert!(Verification::ALL
        .iter()
        .all(|state| !state.label().is_empty()));
}
