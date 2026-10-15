use super::*;

#[test]
fn finished_review_runs_reconcile_from_their_captured_document() {
    let temp = tempfile::tempdir().unwrap();
    let mut writer = crate::runs::RunWriter::create_in(temp.path(), "abc123_review").unwrap();
    writer
        .add_text(
            crate::runs::DOCUMENT_TEXT_PATH,
            "Readable document",
            "document",
            "Captured paper text",
        )
        .unwrap();
    let report = crate::models::PipelineReport {
        orientation: serde_json::json!({}),
        step_outputs: vec![],
        failed_steps: vec![],
        products: Default::default(),
        quality: Default::default(),
        referee_reports: vec![],
        editor: None,
        report_date: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        paper_hash: "hash".into(),
    };
    writer
        .add_text(
            "report.json",
            "Report data",
            "context",
            &serde_json::to_string(&report).unwrap(),
        )
        .unwrap();
    writer
        .add_text("report.md", "Report", "report", "# Report")
        .unwrap();
    let run_dir = writer.dir().to_path_buf();
    let manifest: crate::runs::RunManifest =
        serde_json::from_str(&std::fs::read_to_string(run_dir.join("manifest.json")).unwrap())
            .unwrap();
    let value = recovered_value(&run_dir, "abc123_review", &manifest).unwrap();
    assert_eq!(value["extracted_text"], "Captured paper text");
    assert_eq!(value["markdown"], "# Report");
    assert_eq!(value["run_id"], "abc123_review");
}
