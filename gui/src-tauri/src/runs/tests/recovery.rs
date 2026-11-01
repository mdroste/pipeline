use super::*;

#[test]
fn document_path_selection_is_versioned_at_the_legacy_boundary() {
    let temp = tempfile::tempdir().unwrap();
    let writer = RunWriter::create_in(temp.path(), "versioned_document_run").unwrap();
    let run_dir = writer.dir().to_path_buf();
    fs::create_dir_all(run_dir.join("context")).unwrap();
    fs::write(run_dir.join(DOCUMENT_TEXT_PATH), "current document").unwrap();
    fs::write(
        run_dir.join(LEGACY_EXTRACTED_TEXT_PATH),
        "legacy exact extraction",
    )
    .unwrap();

    let current = writer.current_manifest();
    assert_eq!(
        captured_document_rel_path(&run_dir, &current).unwrap(),
        DOCUMENT_TEXT_PATH
    );
    fs::remove_file(run_dir.join(DOCUMENT_TEXT_PATH)).unwrap();
    assert!(captured_document_rel_path(&run_dir, &current).is_err());

    let mut legacy = current.clone();
    legacy.artifact_schema_version = 0;
    assert_eq!(
        captured_document_rel_path(&run_dir, &legacy).unwrap(),
        LEGACY_EXTRACTED_TEXT_PATH
    );

    fs::write(run_dir.join(DOCUMENT_TEXT_PATH), "current document").unwrap();
    fs::remove_file(run_dir.join(LEGACY_EXTRACTED_TEXT_PATH)).unwrap();
    assert!(captured_document_rel_path(&run_dir, &legacy).is_err());
}

#[test]
fn abrupt_run_recovery_builds_report_from_step_checkpoints() {
    let temp = tempfile::tempdir().unwrap();
    let mut writer = RunWriter::create_in(temp.path(), "abc123_run").unwrap();
    writer
        .add_text(DOCUMENT_TEXT_PATH, "Readable document", "document", "paper")
        .unwrap();
    writer
        .add_text(
            "context/orientation.json",
            "Orientation map",
            "context",
            r#"{"metadata":{"title":"Test"}}"#,
        )
        .unwrap();
    let run_dir = writer.dir().to_path_buf();
    let checkpoint_dir = run_dir.join("artifacts/checkpoints");
    fs::create_dir_all(&checkpoint_dir).unwrap();
    let output = crate::models::StepOutput {
        step_id: "technical".to_string(),
        step_label: "Technical".to_string(),
        phase: "parallel".to_string(),
        raw_text: "Recovered analysis".to_string(),
        ..Default::default()
    };
    fs::write(
        checkpoint_dir.join("0000_technical.json"),
        serde_json::to_vec_pretty(&output).unwrap(),
    )
    .unwrap();
    // Simulate an abort: Drop never gets the opportunity to mark the run
    // failed or persist its in-memory artifact index.
    std::mem::forget(writer);

    let manifest_json = fs::read_to_string(run_dir.join("manifest.json")).unwrap();
    let mut manifest: RunManifest = serde_json::from_str(&manifest_json).unwrap();
    assert_eq!(manifest.status, "running");
    assert!(recover_resumable_run_dir(&run_dir, &mut manifest).unwrap());

    assert_eq!(manifest.status, "interrupted");
    assert_eq!(manifest.step_count, 1);
    let report: crate::models::PipelineReport =
        serde_json::from_str(&fs::read_to_string(run_dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(report.step_outputs.len(), 1);
    assert_eq!(report.step_outputs[0].raw_text, "Recovered analysis");
    assert!(run_dir.join("report.md").is_file());
}

#[test]
fn abrupt_auto_review_recovery_does_not_publish_a_specialist_checkpoint() {
    let temp = tempfile::tempdir().unwrap();
    let mut writer = RunWriter::create_in(temp.path(), "structured_recovery").unwrap();
    writer
        .add_text(DOCUMENT_TEXT_PATH, "Readable document", "document", "paper")
        .unwrap();
    let mut workflow: serde_json::Value = serde_json::from_str(
        &crate::pipeline_config::workflow_template()
            .unwrap()
            .canonical_json,
    )
    .unwrap();
    workflow["outputs"]["findings_step"] = serde_json::json!("synthesis");
    workflow["steps"][1]["output_schema"] = serde_json::json!({
        "type": "object",
        crate::pipeline::structured::SCHEMA_REFERENCE_KEY: "findings-v1",
    });
    let workflow =
        crate::pipeline_config::parse_workflow_document_strict(&workflow.to_string()).unwrap();
    writer
        .add_text(
            "context/workflow.json",
            "Workflow snapshot",
            "context",
            &workflow.canonical_json,
        )
        .unwrap();
    let run_dir = writer.dir().to_path_buf();
    let checkpoint_dir = run_dir.join("artifacts/checkpoints");
    fs::create_dir_all(&checkpoint_dir).unwrap();
    let specialist = crate::models::StepOutput {
        step_id: "analysis/codex".into(),
        step_label: "Independent analysis".into(),
        phase: "parallel".into(),
        structured_json: true,
        raw_text: r#"{"findings":[{"title":"T","in_the_paper":"Claim","problem":"Problem","consequence":"Consequence","what_would_help":"Fix","evidence":[]}]}"#.into(),
        ..Default::default()
    };
    fs::write(
        checkpoint_dir.join("0000_analysis_codex.json"),
        serde_json::to_vec_pretty(&specialist).unwrap(),
    )
    .unwrap();
    std::mem::forget(writer);

    let mut manifest: RunManifest =
        serde_json::from_str(&fs::read_to_string(run_dir.join("manifest.json")).unwrap()).unwrap();
    assert!(recover_resumable_run_dir(&run_dir, &mut manifest).unwrap());

    let report: crate::models::PipelineReport =
        serde_json::from_str(&fs::read_to_string(run_dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(report.products.primary_step_id, "synthesis");
    assert!(report.products.findings.is_none());
}

#[test]
fn cancelled_run_becomes_resumable_from_its_last_checkpoint() {
    let temp = tempfile::tempdir().unwrap();
    let mut writer = RunWriter::create_in(temp.path(), "def456_run").unwrap();
    writer
        .set_pending_meta(RunFinishMeta {
            input_path: "/papers/test.pdf".to_string(),
            input_mode: "document".to_string(),
            profile_id: "deep-review".to_string(),
            profile_name: "Deep Review".to_string(),
            ..Default::default()
        })
        .unwrap();
    writer
        .add_text(DOCUMENT_TEXT_PATH, "Readable document", "document", "paper")
        .unwrap();
    writer
        .add_text(
            "context/orientation.json",
            "Orientation map",
            "context",
            r#"{"metadata":{"title":"Test"}}"#,
        )
        .unwrap();
    let run_dir = writer.dir().to_path_buf();
    let checkpoint_dir = run_dir.join("artifacts/checkpoints");
    fs::create_dir_all(&checkpoint_dir).unwrap();
    let output = crate::models::StepOutput {
        step_id: "technical".to_string(),
        step_label: "Technical".to_string(),
        raw_text: "Durable work".to_string(),
        ..Default::default()
    };
    fs::write(
        checkpoint_dir.join("0000_technical.json"),
        serde_json::to_vec_pretty(&output).unwrap(),
    )
    .unwrap();

    let mut manifest = writer.current_manifest();
    manifest.status = "cancelled".to_string();
    write_manifest(&run_dir, &manifest).unwrap();
    std::mem::forget(writer);

    assert!(recover_resumable_run_dir(&run_dir, &mut manifest).unwrap());
    assert_eq!(manifest.status, "cancelled");
    assert_eq!(manifest.step_count, 1);
    assert_eq!(manifest.failed_steps, vec!["Run cancelled", "technical"]);
    assert!(run_has_resume_files(&run_dir, &manifest));
    let report: crate::models::PipelineReport =
        serde_json::from_str(&fs::read_to_string(run_dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(report.step_outputs[0].raw_text, "Durable work");
    assert_eq!(report.failed_steps[0].step_label, "Run cancelled");
}

#[test]
fn failed_run_without_a_captured_document_is_not_resumable() {
    let temp = tempfile::tempdir().unwrap();
    let writer = RunWriter::create_in(temp.path(), "no_context_run").unwrap();
    let run_dir = writer.dir().to_path_buf();
    let mut manifest = writer.current_manifest();
    manifest.status = "failed".to_string();
    write_manifest(&run_dir, &manifest).unwrap();
    std::mem::forget(writer);

    assert!(recover_resumable_run_dir(&run_dir, &mut manifest).is_err());
    assert!(!run_has_resume_files(&run_dir, &manifest));
    assert!(!run_dir.join("report.json").exists());
}

#[test]
fn a_run_that_died_before_its_document_stops_showing_as_running() {
    let temp = tempfile::tempdir().unwrap();
    let writer = RunWriter::create_in(temp.path(), "no_document_run").unwrap();
    let run_dir = writer.dir().to_path_buf();
    let mut manifest = writer.current_manifest();
    std::mem::forget(writer);
    assert_eq!(manifest.status, "running");

    assert!(recover_resumable_run_dir(&run_dir, &mut manifest).is_err());

    let saved: RunManifest =
        serde_json::from_str(&fs::read_to_string(run_dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(saved.status, "interrupted");
    assert!(!run_has_resume_files(&run_dir, &saved));
}

#[test]
fn a_run_that_died_after_its_report_stops_showing_as_running() {
    let temp = tempfile::tempdir().unwrap();
    let writer = RunWriter::create_in(temp.path(), "reported_run").unwrap();
    let run_dir = writer.dir().to_path_buf();
    let mut manifest = writer.current_manifest();
    std::mem::forget(writer);
    fs::write(
        run_dir.join("report.json"),
        serde_json::to_vec(&empty_report()).unwrap(),
    )
    .unwrap();

    assert!(!recover_resumable_run_dir(&run_dir, &mut manifest).unwrap());

    let saved: RunManifest =
        serde_json::from_str(&fs::read_to_string(run_dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(saved.status, "interrupted");
}

fn empty_report() -> crate::models::PipelineReport {
    crate::models::PipelineReport {
        orientation: serde_json::Value::Null,
        step_outputs: Vec::new(),
        failed_steps: Vec::new(),
        products: Default::default(),
        quality: Default::default(),
        referee_reports: Vec::new(),
        editor: None,
        report_date: chrono::Local::now().date_naive(),
        paper_hash: "hash".to_string(),
    }
}

fn stopped_run_with_a_checkpoint(root: &Path, status: &str) -> PathBuf {
    let mut writer = RunWriter::create_in(root, "stopped_run").unwrap();
    writer
        .add_text(DOCUMENT_TEXT_PATH, "Readable document", "document", "paper")
        .unwrap();
    let run_dir = writer.dir().to_path_buf();
    let checkpoint_dir = run_dir.join("artifacts/checkpoints");
    fs::create_dir_all(&checkpoint_dir).unwrap();
    let output = crate::models::StepOutput {
        step_id: "technical".to_string(),
        raw_text: "Durable work".to_string(),
        ..Default::default()
    };
    fs::write(
        checkpoint_dir.join("wave_0000_technical.json"),
        serde_json::to_vec(&serde_json::json!({
            "checkpointVersion": 1, "steps": ["technical"], "outputs": [output]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut manifest = writer.current_manifest();
    manifest.status = status.to_string();
    write_manifest(&run_dir, &manifest).unwrap();
    std::mem::forget(writer);
    run_dir
}

#[test]
fn a_stopped_run_recovers_on_its_own_while_another_review_holds_the_lock() {
    use fs2::FileExt as _;
    let temp = tempfile::tempdir().unwrap();
    let run_dir = stopped_run_with_a_checkpoint(temp.path(), "cancelled");
    // The store-wide pass stands aside for the active review.
    let lock_path = temp.path().join("run.lock");
    let active_review = open_run_lock(&lock_path).unwrap();
    active_review.try_lock_exclusive().unwrap();
    assert_eq!(
        recover_resumable_runs_at(temp.path(), &lock_path).unwrap(),
        0
    );
    assert!(!run_dir.join("report.json").exists());

    assert!(recover_stopped_run_dir(&run_dir).unwrap());

    let report: crate::models::PipelineReport =
        serde_json::from_str(&fs::read_to_string(run_dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(report.step_outputs[0].raw_text, "Durable work");
}

#[test]
fn single_run_recovery_leaves_a_running_run_alone() {
    let temp = tempfile::tempdir().unwrap();
    let run_dir = stopped_run_with_a_checkpoint(temp.path(), "running");

    assert!(!recover_stopped_run_dir(&run_dir).unwrap());

    assert!(!run_dir.join("report.json").exists());
    let saved: RunManifest =
        serde_json::from_str(&fs::read_to_string(run_dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(saved.status, "running");
}

#[test]
fn recovery_preserves_checkpointed_failure_identity() {
    let temp = tempfile::tempdir().unwrap();
    let mut writer = RunWriter::create_in(temp.path(), "failed_step_run").unwrap();
    writer
        .add_text(DOCUMENT_TEXT_PATH, "Readable document", "document", "paper")
        .unwrap();
    let run_dir = writer.dir().to_path_buf();
    let checkpoint_dir = run_dir.join("artifacts/checkpoints");
    fs::create_dir_all(&checkpoint_dir).unwrap();
    fs::write(
        checkpoint_dir.join("failure_empirical.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "failure": crate::models::StepFailure {
                step_id: "empirical/antigravity".to_string(),
                step_label: "Empirical [Antigravity]".to_string(),
                phase: "parallel".to_string(),
                error: "provider failed".to_string(),
            }
        }))
        .unwrap(),
    )
    .unwrap();
    let mut manifest = writer.current_manifest();
    manifest.status = "failed".to_string();
    write_manifest(&run_dir, &manifest).unwrap();
    std::mem::forget(writer);

    assert!(recover_resumable_run_dir(&run_dir, &mut manifest).unwrap());
    let report: crate::models::PipelineReport =
        serde_json::from_str(&fs::read_to_string(run_dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(report.failed_steps[0].step_id, "empirical/antigravity");
    assert_eq!(manifest.failed_steps, vec!["Empirical [Antigravity]"]);
}

#[test]
fn foreign_but_valid_manifest_is_preserved_not_quarantined() {
    // Valid JSON that isn't a RunManifest (e.g. written by a newer version) is
    // preserved untouched so a downgrade cannot rewrite healthy runs as failed.
    assert!(manifest_is_foreign_but_valid(
        Some("{\"schema_from_the_future\": 99}"),
        true
    ));
    // Genuinely broken bytes (not JSON) are eligible for recovery.
    assert!(!manifest_is_foreign_but_valid(Some("{not json"), true));
    // A missing/unreadable manifest is an orphan directory, also recoverable.
    assert!(!manifest_is_foreign_but_valid(None, false));
    assert!(!manifest_is_foreign_but_valid(Some("{}"), false));
}

#[test]
fn tombstoned_directory_is_finished_deleting_not_resurrected() {
    let temp = tempfile::tempdir().unwrap();
    let remnant = temp.path().join("half-deleted-run");
    fs::create_dir(&remnant).unwrap();
    fs::write(remnant.join(DELETE_TOMBSTONE), "{}").unwrap();
    fs::write(remnant.join("locked.bin"), "leftover").unwrap();

    assert!(recover_orphan_manifest(&remnant).is_none());
    assert!(!remnant.exists());
}

#[test]
fn manifestless_directory_is_recovered_as_failed() {
    let temp = tempfile::tempdir().unwrap();
    let orphan = temp.path().join("old-run");
    fs::create_dir(&orphan).unwrap();
    fs::write(orphan.join("partial.log"), "unfinished").unwrap();

    let manifest = recover_orphan_manifest(&orphan).unwrap();
    assert_eq!(manifest.status, "failed");
    assert!(orphan.join("manifest.json").is_file());
}

#[test]
fn manifestless_current_document_recovers_current_artifact_schema() {
    let temp = tempfile::tempdir().unwrap();
    let orphan = temp.path().join("current-run");
    fs::create_dir_all(orphan.join("context")).unwrap();
    fs::write(orphan.join(DOCUMENT_TEXT_PATH), "exact document").unwrap();

    let manifest = recover_orphan_manifest(&orphan).unwrap();
    assert_eq!(
        manifest.artifact_schema_version,
        CURRENT_ARTIFACT_SCHEMA_VERSION
    );
    assert_eq!(
        captured_document_rel_path(&orphan, &manifest).unwrap(),
        DOCUMENT_TEXT_PATH
    );
}

#[test]
fn recovery_requires_logical_receipts_and_atomically_overlays_final_waves() {
    for (published, unit_count) in [(false, 1), (false, 2), (true, 2)] {
        let temp = tempfile::tempdir().unwrap();
        let mut writer = RunWriter::create_in(temp.path(), "wave-recovery").unwrap();
        writer
            .add_text(DOCUMENT_TEXT_PATH, "Document", "document", "paper")
            .unwrap();
        let run = writer.dir().to_path_buf();
        let checkpoints = run.join("artifacts/checkpoints");
        fs::create_dir_all(&checkpoints).unwrap();
        let unit = crate::models::StepOutput {
            step_id: "technical/claude".into(),
            step_label: "Technical".into(),
            raw_text: "first result".into(),
            ..Default::default()
        };
        fs::write(
            checkpoints.join("0000_unit.json"),
            serde_json::to_vec(&unit).unwrap(),
        )
        .unwrap();
        let second = crate::models::StepOutput {
            step_id: "technical/codex".into(),
            raw_text: "second result".into(),
            ..unit.clone()
        };
        if unit_count == 2 {
            fs::write(
                checkpoints.join("0001_unit.json"),
                serde_json::to_vec(&second).unwrap(),
            )
            .unwrap();
        }
        // A failed/incomplete publication is ignored without deleting either
        // previously committed unit. A complete publication replaces them once.
        if published {
            let merged = crate::models::StepOutput {
                step_id: "technical".into(),
                raw_text: "merged result".into(),
                ..unit.clone()
            };
            fs::write(checkpoints.join("wave_0000.json"), serde_json::to_vec(&serde_json::json!({"checkpointVersion":1,"steps":["technical"],"outputs":[merged]})).unwrap()).unwrap();
        } else {
            fs::write(checkpoints.join("wave_0000.json"), "{incomplete write").unwrap();
        }
        let mut manifest: RunManifest =
            serde_json::from_str(&fs::read_to_string(run.join("manifest.json")).unwrap()).unwrap();
        assert!(recover_resumable_run_dir(&run, &mut manifest).unwrap());
        let report: crate::models::PipelineReport =
            serde_json::from_str(&fs::read_to_string(run.join("report.json")).unwrap()).unwrap();
        assert_eq!(
            report.step_outputs.len(),
            if published { 1 } else { unit_count }
        );
        assert_eq!(
            report.failed_steps.iter().any(|f| f.step_id == "technical"),
            !published
        );
        assert!(checkpoints.join("0000_unit.json").exists());
    }
}

#[test]
fn recovery_yields_to_the_run_lock_held_by_the_pipeline_guard() {
    use fs2::FileExt as _;
    let temp = tempfile::tempdir().unwrap();
    let runs = temp.path().join("runs");
    let lock_path = temp.path().join("run.lock");
    fs::create_dir_all(&runs).unwrap();
    let mut writer = RunWriter::create_in(&runs, "abc123_live").unwrap();
    writer
        .add_text(DOCUMENT_TEXT_PATH, "Readable document", "document", "paper")
        .unwrap();
    let run_dir = writer.dir().to_path_buf();
    std::mem::forget(writer);

    // The same lock file the pipeline guard takes, held as the guard holds it.
    let guard = open_run_lock(&lock_path).unwrap();
    guard.try_lock_exclusive().unwrap();
    assert_eq!(recover_resumable_runs_at(&runs, &lock_path).unwrap(), 0);
    let manifest: RunManifest =
        serde_json::from_str(&fs::read_to_string(run_dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest.status, "running");
    assert!(!run_dir.join("report.json").exists());

    fs2::FileExt::unlock(&guard).unwrap();
    assert_eq!(recover_resumable_runs_at(&runs, &lock_path).unwrap(), 1);
    let manifest: RunManifest =
        serde_json::from_str(&fs::read_to_string(run_dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest.status, "interrupted");
}

#[test]
fn a_running_run_is_recovered_however_long_the_walk_over_the_store_takes() {
    let temp = tempfile::tempdir().unwrap();
    let runs = temp.path().join("runs");
    fs::create_dir_all(&runs).unwrap();
    for index in 0..40 {
        let mut writer = RunWriter::create_in(&runs, &format!("abc123_finished_{index}")).unwrap();
        writer
            .add_text(DOCUMENT_TEXT_PATH, "Readable document", "document", "paper")
            .unwrap();
        let dir = writer.dir().to_path_buf();
        let mut manifest = writer.current_manifest();
        std::mem::forget(writer);
        manifest.status = "done".to_string();
        write_manifest(&dir, &manifest).unwrap();
    }
    let mut writer = RunWriter::create_in(&runs, "abc123_interrupted").unwrap();
    writer
        .add_text(DOCUMENT_TEXT_PATH, "Readable document", "document", "paper")
        .unwrap();
    let run_dir = writer.dir().to_path_buf();
    std::mem::forget(writer);

    assert_eq!(
        recover_runs_in(&runs, store_walk().overdue()).unwrap(),
        1,
        "an overdue store walk must still reach every run"
    );
    let manifest: RunManifest =
        serde_json::from_str(&fs::read_to_string(run_dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest.status, "interrupted");
}
