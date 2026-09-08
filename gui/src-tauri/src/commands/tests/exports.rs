//! Exports regression coverage.

use super::*;

#[test]
fn directory_export_is_complete_and_does_not_replace_existing_output() {
    let source = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(source.path().join("context")).unwrap();
    std::fs::write(source.path().join("manifest.json"), b"manifest").unwrap();
    std::fs::write(source.path().join("context/report.md"), b"report").unwrap();
    let destination = tempfile::tempdir().unwrap();
    let existing = destination.path().join("pipeline-run-test");
    std::fs::create_dir(&existing).unwrap();
    std::fs::write(existing.join("keep.txt"), b"keep").unwrap();
    let stage = create_export_stage(destination.path()).unwrap();

    let (files, bytes) = copy_export_tree(source.path(), stage.path()).unwrap();
    let exported = finish_export_stage(&stage, destination.path(), "pipeline-run-test").unwrap();
    drop(stage);

    assert_eq!(files, 2);
    assert_eq!(bytes, 14);
    assert_ne!(exported, existing);
    assert_eq!(std::fs::read(existing.join("keep.txt")).unwrap(), b"keep");
    assert_eq!(
        std::fs::read(exported.join("context/report.md")).unwrap(),
        b"report"
    );
}

#[test]
fn shareable_export_has_an_exact_safe_allowlist_and_no_secret_payloads() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    write_export_test_run(source.path());
    let manifest = export_test_manifest();

    copy_export_file(source.path(), destination.path(), "report.md").unwrap();
    write_shareable_products(source.path(), destination.path(), &manifest, true, true).unwrap();
    finalize_export_metadata(
        destination.path(),
        &manifest.run_id,
        ExportMode::Shareable,
        &ExportSelection::default(),
    )
    .unwrap();

    let files = enumerate_export_files(destination.path()).unwrap();
    let paths = files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            "checksums.sha256",
            "export-manifest.json",
            "limitations.md",
            "provenance.json",
            "quality.json",
            "report.md",
            "verified-findings.json",
        ]
    );
    let exported_text = files
        .iter()
        .map(|file| std::fs::read_to_string(destination.path().join(&file.path)).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!exported_text.contains("TOPSECRET"));
    assert!(exported_text.contains("Verified finding"));
    assert!(!exported_text.contains("context/document.md"));
    assert!(!destination.path().join("report.json").exists());
    assert!(!destination.path().join("logs/run.log").exists());
}

#[test]
fn custom_export_copies_only_explicit_artifact_classes() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    write_export_test_run(source.path());
    let manifest = export_test_manifest();
    let selection = ExportSelection {
        report: true,
        source_documents: true,
        ..Default::default()
    };
    write_custom_payload(source.path(), destination.path(), &manifest, &selection).unwrap();
    let paths = enumerate_export_files(destination.path())
        .unwrap()
        .into_iter()
        .map(|file| file.path)
        .collect::<Vec<_>>();
    assert_eq!(paths, ["context/document.md", "report.md"]);
    assert!(!destination.path().join("report.json").exists());
    assert!(!destination.path().join("artifacts/01_review.md").exists());
    assert!(!destination.path().join("logs/run.log").exists());
}

#[test]
fn forensic_export_enumerates_the_complete_run_and_labels_it_sensitive() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    write_export_test_run(source.path());
    copy_export_tree(source.path(), destination.path()).unwrap();
    finalize_export_metadata(
        destination.path(),
        "export-test-run",
        ExportMode::Forensic,
        &ExportSelection::default(),
    )
    .unwrap();
    let paths = enumerate_export_files(destination.path())
        .unwrap()
        .into_iter()
        .map(|file| file.path)
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            "artifacts/01_review.md",
            "checksums.sha256",
            "context/document.md",
            "export-manifest.json",
            "logs/run.log",
            "manifest.json",
            "report.json",
            "report.md",
        ]
    );
    let export_manifest =
        std::fs::read_to_string(destination.path().join("export-manifest.json")).unwrap();
    assert!(export_manifest.contains(r#""sensitivity": "sensitive""#));
    assert!(export_manifest.contains(r#""includesSourceMaterial": true"#));
    assert!(export_manifest.contains(r#""includesRawResponses": true"#));
    assert!(export_manifest.contains(r#""includesLogs": true"#));
    assert_eq!(
        std::fs::read_to_string(destination.path().join("manifest.json")).unwrap(),
        "TOPSECRET-manifest"
    );
}

#[test]
fn reveal_validation_accepts_only_the_exact_recent_export() {
    let root = tempfile::tempdir().unwrap();
    let exported = root.path().join("package");
    std::fs::create_dir(&exported).unwrap();
    record_export_path(&exported);
    assert_eq!(
        validated_recent_export(&exported.to_string_lossy()).unwrap(),
        exported.canonicalize().unwrap()
    );
    assert!(validated_recent_export(&root.path().to_string_lossy()).is_err());
}

#[test]
fn core_export_writes_only_the_canonical_document_name() {
    let destination = tempfile::tempdir().unwrap();
    let report = PipelineReport {
        orientation: serde_json::Value::Null,
        step_outputs: Vec::new(),
        failed_steps: Vec::new(),
        products: Default::default(),
        quality: Default::default(),
        referee_reports: Vec::new(),
        editor: None,
        report_date: chrono::Local::now().date_naive(),
        paper_hash: "test".into(),
    };

    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(save_all_artifacts(
            destination.path().to_string_lossy().into_owned(),
            "# Report".into(),
            "exact document".into(),
            report,
        ))
        .unwrap();

    let exported = destination.path().join("pipeline-core-export");
    assert_eq!(
        std::fs::read_to_string(exported.join("document.md")).unwrap(),
        "exact document"
    );
    assert!(!exported.join("extracted_text.md").exists());
}

#[test]
fn core_export_includes_the_canonical_findings_product() {
    let destination = tempfile::tempdir().unwrap();
    let report = PipelineReport {
        orientation: serde_json::Value::Null,
        step_outputs: Vec::new(),
        failed_steps: Vec::new(),
        products: crate::models::RunProducts {
            schema_version: 1,
            primary_step_id: "final".into(),
            findings: Some(crate::models::FindingSet {
                schema_version: 1,
                source_step_id: "final".into(),
                source_step_label: "Final".into(),
                taxonomy: Vec::new(),
                findings: Vec::new(),
            }),
            validation_dispositions: Vec::new(),
            named: Vec::new(),
        },
        quality: Default::default(),
        referee_reports: Vec::new(),
        editor: None,
        report_date: chrono::Local::now().date_naive(),
        paper_hash: "test".into(),
    };

    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(save_all_artifacts(
            destination.path().to_string_lossy().into_owned(),
            "# Report".into(),
            "exact document".into(),
            report,
        ))
        .unwrap();

    let findings = std::fs::read_to_string(
        destination
            .path()
            .join("pipeline-core-export/findings.json"),
    )
    .unwrap();
    assert!(findings.contains(r#""source_step_id": "final""#));
}

#[test]
fn core_export_writes_structured_step_artifacts_as_standalone_json() {
    let destination = tempfile::tempdir().unwrap();
    let report = PipelineReport {
        orientation: serde_json::json!({}),
        step_outputs: vec![crate::models::StepOutput {
            step_id: "structured".into(),
            step_label: "Structured result".into(),
            raw_text: "{\n  \"result\": \"ok\"\n}".into(),
            structured_json: true,
            ..Default::default()
        }],
        failed_steps: Vec::new(),
        products: Default::default(),
        quality: Default::default(),
        referee_reports: Vec::new(),
        editor: None,
        report_date: chrono::Local::now().date_naive(),
        paper_hash: "test".into(),
    };

    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(save_all_artifacts(
            destination.path().to_string_lossy().into_owned(),
            "# Report".into(),
            "document".into(),
            report,
        ))
        .unwrap();

    let artifact = std::fs::read_to_string(
        destination
            .path()
            .join("pipeline-core-export/steps/01_structured.json"),
    )
    .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&artifact).unwrap(),
        serde_json::json!({"result": "ok"})
    );
    assert!(!destination
        .path()
        .join("pipeline-core-export/steps/01_structured.md")
        .exists());
}
