use super::*;

fn empty_test_config() -> PipelineConfig {
    PipelineConfig {
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
    }
}

fn export_test_manifest() -> crate::runs::RunManifest {
    serde_json::from_value(serde_json::json!({
        "run_id": "export-test-run",
        "created": "2026-08-23T12:00:00Z",
        "input_path": "/Users/private/TOPSECRET-paper.pdf",
        "input_mode": "document",
        "profile_id": "review",
        "profile_name": "Review",
        "provider": "codex",
        "status": "done",
        "variables": { "private_note": "TOPSECRET-variable" },
        "artifacts": [
            { "rel_path": "report.md", "label": "Report", "kind": "markdown", "bytes": 1, "sha256": "a", "group": "report" },
            { "rel_path": "report.json", "label": "Report data", "kind": "json", "bytes": 1, "sha256": "b", "group": "context" },
            { "rel_path": "context/document.md", "label": "Document", "kind": "markdown", "bytes": 1, "sha256": "c", "group": "document" },
            { "rel_path": "artifacts/01_review.md", "label": "Review", "kind": "markdown", "bytes": 1, "sha256": "d", "group": "step" },
            { "rel_path": "logs/run.log", "label": "Log", "kind": "text", "bytes": 1, "sha256": "e", "group": "context" }
        ]
    }))
    .unwrap()
}

fn write_export_test_run(source: &std::path::Path) {
    std::fs::create_dir_all(source.join("context")).unwrap();
    std::fs::create_dir_all(source.join("artifacts")).unwrap();
    std::fs::create_dir_all(source.join("logs")).unwrap();
    std::fs::write(source.join("report.md"), "# Safe rendered report\n").unwrap();
    std::fs::write(source.join("context/document.md"), "TOPSECRET-source-bytes").unwrap();
    std::fs::write(
        source.join("artifacts/01_review.md"),
        "TOPSECRET-raw-response",
    )
    .unwrap();
    std::fs::write(source.join("logs/run.log"), "TOPSECRET-log").unwrap();
    std::fs::write(source.join("manifest.json"), "TOPSECRET-manifest").unwrap();
    let report = PipelineReport {
        orientation: serde_json::Value::Null,
        step_outputs: Vec::new(),
        failed_steps: Vec::new(),
        products: crate::models::RunProducts {
            schema_version: 1,
            findings: Some(crate::models::FindingSet {
                schema_version: 1,
                findings: vec![
                    crate::models::Finding {
                        id: "verified".into(),
                        title: "Verified finding".into(),
                        verification_status: "verified".into(),
                        evidence: vec![crate::models::FindingEvidence {
                            verification_status: "verified_with_normalization".into(),
                            page: Some(4),
                            quote: "Supported statement".into(),
                            artifact_path: "context/document.md".into(),
                            ..Default::default()
                        }],
                        ..Default::default()
                    },
                    crate::models::Finding {
                        id: "unverified".into(),
                        title: "TOPSECRET-unverified-finding".into(),
                        verification_status: "unverified".into(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            ..Default::default()
        },
        quality: crate::models::ReportQuality {
            status: "done".into(),
            verified_evidence: 1,
            unverified_evidence: 1,
            limitations: vec!["One item remains unverified.".into()],
            ..Default::default()
        },
        referee_reports: Vec::new(),
        editor: None,
        report_date: chrono::Local::now().date_naive(),
        paper_hash: "test".into(),
    };
    std::fs::write(
        source.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}

// Exercises the batch job-status helpers over the global BATCH state. This
// is the only test that touches BATCH, so parallel test runs can't race it.

mod exports;

mod imports;

mod launch;

mod print;

mod recovery;

mod snapshots;
