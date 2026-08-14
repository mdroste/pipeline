use super::*;

#[test]
fn detect_kind_by_extension_and_sniff() {
    assert_eq!(detect_kind("report.md", b""), "markdown");
    assert_eq!(detect_kind("steps/01_technical.md", b""), "markdown");
    assert_eq!(detect_kind("orientation.json", b""), "json");
    assert_eq!(detect_kind("analysis.py", b""), "code");
    assert_eq!(detect_kind("data.csv", b""), "csv");
    assert_eq!(detect_kind("fig.png", b""), "image");
    assert_eq!(detect_kind("paper.pdf", b""), "pdf");
    assert_eq!(detect_kind("README", b"plain text"), "text");
    assert_eq!(detect_kind("blob", b"\x00\x01\x02"), "binary");
}

#[test]
fn response_journal_paths_get_reader_friendly_labels() {
    assert_eq!(
        agent_response_label(
            "technical-codex--0123456789ab--attempt-01-terminal-rejected-envelope.md"
        ),
        "Technical codex · Attempt 1 · Terminal · Rejected boundaries"
    );
    assert_eq!(
        agent_response_label(
            "merge-technical--0123456789ab--attempt-02-compatibility-file-accepted.md"
        ),
        "Merge technical · Attempt 2 · Compatibility file · Accepted"
    );
    assert_eq!(
        agent_response_label("synthesis--0123456789ab--attempt-02-terminal-rejected-content.md"),
        "Synthesis · Attempt 2 · Terminal · Rejected content"
    );
}

#[test]
fn run_id_validation() {
    assert!(validate_run_id("abc123_20260702-120000").is_ok());
    assert!(validate_run_id("").is_err());
    assert!(validate_run_id("../escape").is_err());
    assert!(validate_run_id("a/b").is_err());
}

#[test]
fn read_artifact_rejects_traversal() {
    assert!(read_artifact("some-run", "../other/file.md").is_err());
    assert!(read_artifact("some-run", "/etc/passwd").is_err());
    assert!(read_artifact("some-run", "a/../../b").is_err());
}

#[test]
fn completed_page_entries_compact_without_removing_files() {
    let root = tempfile::tempdir().unwrap();
    let mut writer = RunWriter::create_in(root.path(), "compact-pages").unwrap();
    writer
        .add_text("report.md", "Report", "report", "# Report")
        .unwrap();
    fs::create_dir_all(writer.dir().join("artifacts/pages")).unwrap();
    for page in 1..=3 {
        let rel_path = format!("artifacts/pages/page-{page:02}.jpg");
        fs::write(writer.dir().join(&rel_path), [0xff, 0xd8, page as u8]).unwrap();
        writer
            .register_existing(&rel_path, &format!("Page {page}"), "pages")
            .unwrap();
    }

    assert_eq!(writer.compact_page_artifacts().unwrap(), 3);
    let manifest = writer.current_manifest();
    assert_eq!(manifest.artifacts.len(), 1);
    assert_eq!(
        manifest.page_artifacts,
        Some(PageArtifactIndex {
            count: 3,
            digit_width: 2,
            extension: "jpg".to_string(),
            total_bytes: 9,
        })
    );
    assert_eq!(
        manifest
            .page_artifacts
            .as_ref()
            .unwrap()
            .rel_path(2)
            .unwrap(),
        "artifacts/pages/page-02.jpg"
    );
    assert_eq!(manifest.to_summary().artifact_count, 4);
    assert!(writer.dir().join("artifacts/pages/page-03.jpg").is_file());
}

#[test]
fn bounded_utf8_reader_stops_at_the_limit() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    fs::write(temp.path(), b"123456789").unwrap();
    assert_eq!(
        read_utf8_at_most(temp.path(), 9, "test").unwrap(),
        "123456789"
    );
    assert!(read_utf8_at_most(temp.path(), 8, "test").is_err());
}

#[test]
fn annotations_require_a_bounded_json_object() {
    assert!(validate_annotation_content(r#"{"a":{"status":"done"}}"#).is_ok());
    assert!(validate_annotation_content("[]").is_err());
    assert!(validate_annotation_content("not-json").is_err());
    assert!(validate_annotation_content(&"x".repeat(MAX_ANNOTATION_BYTES + 1)).is_err());
}

#[test]
fn retention_preview_matches_count_and_byte_limits() {
    // Newest first. The running oldest entry is protected, so the next
    // oldest completed run is selected when the count limit is exceeded.
    let plan = retention_plan_from_state(
        &[
            ("newest".to_string(), false, 10),
            ("middle".to_string(), false, 20),
            ("oldest".to_string(), true, 30),
        ],
        2,
        0,
    );
    let preview = plan.preview;
    assert_eq!(preview.delete_count, 1);
    assert_eq!(preview.delete_bytes, 20);
    assert_eq!(preview.remaining_count, 2);
    assert_eq!(preview.remaining_bytes, 40);
    assert!(!preview.preview_token.is_empty());
    assert_eq!(plan.candidates, vec!["middle".to_string()]);

    let preview = retention_plan_from_state(
        &[
            ("newest".to_string(), false, 10),
            ("middle".to_string(), false, 20),
            ("oldest".to_string(), false, 30),
        ],
        0,
        25,
    )
    .preview;
    assert_eq!(preview.delete_count, 2);
    assert_eq!(preview.delete_bytes, 50);
    assert_eq!(preview.remaining_count, 1);
    assert_eq!(preview.remaining_bytes, 10);
}

#[test]
fn retention_never_deletes_the_newest_run() {
    // A byte limit smaller than a single run must not delete the run that
    // just completed; only the older runs are candidates.
    let plan = retention_plan_from_state(
        &[
            ("newest".to_string(), false, 100),
            ("middle".to_string(), false, 20),
            ("oldest".to_string(), false, 30),
        ],
        0,
        50,
    );
    assert_eq!(
        plan.candidates,
        vec!["oldest".to_string(), "middle".to_string()]
    );
    assert_eq!(plan.preview.remaining_count, 1);
    assert_eq!(plan.preview.remaining_bytes, 100);
}

#[test]
fn retention_preview_token_binds_the_confirmed_deletion_plan() {
    let initial = retention_plan_from_state(
        &[
            ("newest".to_string(), false, 10),
            ("oldest".to_string(), false, 20),
        ],
        1,
        0,
    )
    .preview;
    let unchanged = retention_plan_from_state(
        &[
            ("newest".to_string(), false, 10),
            ("oldest".to_string(), false, 20),
        ],
        1,
        0,
    )
    .preview;
    let changed_candidate = retention_plan_from_state(
        &[
            ("newest".to_string(), false, 10),
            ("different-oldest".to_string(), false, 20),
        ],
        1,
        0,
    )
    .preview;
    let changed_size = retention_plan_from_state(
        &[
            ("newest".to_string(), false, 10),
            ("oldest".to_string(), false, 21),
        ],
        1,
        0,
    )
    .preview;

    assert_eq!(initial.preview_token, unchanged.preview_token);
    assert_ne!(initial.preview_token, changed_candidate.preview_token);
    assert_ne!(initial.preview_token, changed_size.preview_token);
}

#[test]
fn old_manifest_without_metadata_loads() {
    // A pre-1.1 manifest has none of the run-level metadata fields.
    let json = r#"{
            "run_id": "abc_20260101-000000",
            "created": "2026-01-01T00:00:00+00:00",
            "input_path": "/papers/main.pdf",
            "input_mode": "document",
            "profile_id": "deep-review",
            "profile_name": "Deep Review",
            "provider": "claude",
            "artifacts": []
        }"#;
    let m: RunManifest = serde_json::from_str(json).unwrap();
    assert_eq!(m.status, "");
    assert_eq!(m.duration_secs, 0);
    assert_eq!(m.usage.input_tokens, 0);
    assert_eq!(m.usage.model_round_trips, 0);
    assert!(m.usage.tool_calls.is_empty());
    assert!(m.tags.is_empty());
    assert!(m.page_artifacts.is_none());
    assert_eq!(m.artifact_schema_version, 0);
    // Summary fills a sensible default status and derives the input name.
    let s = m.to_summary();
    assert_eq!(s.status, "done");
    assert_eq!(s.input_name, "main.pdf");
}

#[test]
fn summary_carries_metrics_and_basename() {
    let m = RunManifest {
        artifact_schema_version: CURRENT_ARTIFACT_SCHEMA_VERSION,
        run_id: "r1".into(),
        created: "2026-07-07T10:00:00+00:00".into(),
        input_path: "/home/u/paper.tex".into(),
        input_mode: "document".into(),
        input_interpretation: "document".into(),
        profile_id: "deep-review".into(),
        profile_name: "Deep Review".into(),
        provider: "claude".into(),
        artifacts: vec![],
        page_artifacts: None,
        status: "partial".into(),
        duration_secs: 125,
        usage: crate::pipeline::logging::CallUsage {
            input_tokens: 1000,
            output_tokens: 200,
            cached_input_tokens: 700,
            cache_write_input_tokens: 100,
            model_round_trips: 12,
            tool_calls: crate::models::ToolCallCounts {
                text_file: 5,
                web: 2,
                unknown: 1,
                ..Default::default()
            },
            ..Default::default()
        },
        step_count: 6,
        failed_steps: vec!["Empirical".into()],
        title: "My run".into(),
        tags: vec!["urgent".into()],
        variables: std::collections::HashMap::new(),
        extra_inputs: std::collections::HashMap::new(),
        extra_input_sources: std::collections::HashMap::new(),
        parent_run_id: None,
    };
    let s = m.to_summary();
    assert_eq!(s.input_name, "paper.tex");
    assert_eq!(s.status, "partial");
    assert_eq!(s.input_tokens, 1000);
    assert_eq!(s.output_tokens, 200);
    assert_eq!(s.cached_input_tokens, 700);
    assert_eq!(s.cache_write_input_tokens, 100);
    assert_eq!(s.model_round_trips, 12);
    assert_eq!(s.tool_calls.total(), 8);
    assert_eq!(s.tool_calls.unknown, 1);
    assert_eq!(s.step_count, 6);
    assert_eq!(s.failed_steps, vec!["Empirical".to_string()]);
    assert_eq!(s.title, "My run");
}

#[test]
fn input_basename_handles_empty_and_paths() {
    assert_eq!(input_basename(""), "(no input)");
    assert_eq!(input_basename("   "), "(no input)");
    assert_eq!(input_basename("/a/b/c.pdf"), "c.pdf");
    assert_eq!(input_basename("relative.tex"), "relative.tex");
}

#[test]
fn unfinished_writer_keeps_a_failed_manifest_and_artifact_index() {
    let temp = tempfile::tempdir().unwrap();
    {
        let mut writer = RunWriter::create_in(temp.path(), "run-1").unwrap();
        writer
            .add_text("context/input.md", "Input", "context", "hello")
            .unwrap();
        writer
            .record_extra_input("letter", "context/input.md")
            .unwrap();
        writer
            .record_extra_input_source("letter", "/inputs/letter.docx")
            .unwrap();
    }

    let content = fs::read_to_string(temp.path().join("run-1/manifest.json")).unwrap();
    let manifest: RunManifest = serde_json::from_str(&content).unwrap();
    assert_eq!(manifest.status, "failed");
    assert_eq!(
        manifest.artifact_schema_version,
        CURRENT_ARTIFACT_SCHEMA_VERSION
    );
    assert_eq!(manifest.artifacts.len(), 1);
    assert_eq!(
        manifest.extra_inputs.get("letter").map(String::as_str),
        Some("context/input.md")
    );
    assert_eq!(
        manifest
            .extra_input_sources
            .get("letter")
            .map(String::as_str),
        Some("/inputs/letter.docx")
    );
}

#[test]
fn canonical_document_is_durable_before_orientation_or_bundle_work() {
    let temp = tempfile::tempdir().unwrap();
    let run_dir;
    {
        let mut writer = RunWriter::create_in(temp.path(), "early_document_run").unwrap();
        writer
            .add_text(
                DOCUMENT_TEXT_PATH,
                "Readable document",
                "document",
                "exact extracted text\n",
            )
            .unwrap();
        run_dir = writer.dir().to_path_buf();
        // Dropping here models any bundle/orientation error after the
        // extraction has been captured but before report finalization.
    }

    let manifest: RunManifest =
        serde_json::from_str(&fs::read_to_string(run_dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest.status, "failed");
    assert_eq!(
        captured_document_rel_path(&run_dir, &manifest).unwrap(),
        DOCUMENT_TEXT_PATH
    );
    assert_eq!(
        fs::read_to_string(run_dir.join(DOCUMENT_TEXT_PATH)).unwrap(),
        "exact extracted text\n"
    );
    assert!(!run_dir.join(LEGACY_EXTRACTED_TEXT_PATH).exists());
}

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
    assert_eq!(manifest.failed_steps, vec!["Run cancelled"]);
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
fn finished_writer_is_not_overwritten_by_drop() {
    let temp = tempfile::tempdir().unwrap();
    let writer = RunWriter::create_in(temp.path(), "run-2").unwrap();
    writer
        .finish(RunFinishMeta {
            status: "done".into(),
            ..Default::default()
        })
        .unwrap();

    let content = fs::read_to_string(temp.path().join("run-2/manifest.json")).unwrap();
    let manifest: RunManifest = serde_json::from_str(&content).unwrap();
    assert_eq!(manifest.status, "done");
}

#[test]
fn oversized_manifest_is_rejected_without_replacing_last_valid_copy() {
    let temp = tempfile::tempdir().unwrap();
    let mut writer = RunWriter::create_in(temp.path(), "bounded-manifest").unwrap();
    writer
        .meta
        .variables
        .insert("huge".to_string(), "x".repeat(MAX_MANIFEST_BYTES));
    assert!(writer.persist_current().is_err());

    let content = fs::read_to_string(temp.path().join("bounded-manifest/manifest.json")).unwrap();
    let manifest: RunManifest = serde_json::from_str(&content).unwrap();
    assert!(manifest.variables.is_empty());
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
fn external_open_paths_strip_windows_verbatim_prefixes() {
    assert_eq!(
        normalize_external_path(r"\\?\C:\Users\Mike\.pipeline\runs\r1\report.md"),
        "C:/Users/Mike/.pipeline/runs/r1/report.md"
    );
    assert_eq!(
        normalize_external_path(r"\\?\UNC\server\share\report.md"),
        "//server/share/report.md"
    );
    assert_eq!(
        normalize_external_path("/Users/mike/report.md"),
        "/Users/mike/report.md"
    );
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
fn capped_artifact_reader_never_loads_past_limit() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    use std::io::Write as _;
    file.write_all(&vec![b'x'; 4096]).unwrap();
    let (bytes, truncated) = read_at_most(file.path(), 128).unwrap();
    assert_eq!(bytes.len(), 128);
    assert!(truncated);
}

#[test]
fn run_ids_are_unique_and_directories_are_exclusive() {
    assert_ne!(new_run_id("abc"), new_run_id("abc"));
    let temp = tempfile::tempdir().unwrap();
    let writer = RunWriter::create_in(temp.path(), "same-id").unwrap();
    assert!(RunWriter::create_in(temp.path(), "same-id").is_err());
    drop(writer);
}

#[test]
fn unlisted_artifact_cap_deletes_every_excess_file() {
    let temp = tempfile::tempdir().unwrap();
    let mut writer = RunWriter::create_in(temp.path(), "quota-run").unwrap();
    let artifacts = writer.dir().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    for index in 0..505 {
        fs::write(artifacts.join(format!("{index:04}.txt")), "x").unwrap();
    }

    assert_eq!(writer.register_unlisted("artifacts", "files"), 500);
    assert_eq!(fs::read_dir(&artifacts).unwrap().count(), 500);
}

#[test]
#[cfg(unix)]
fn unlisted_artifact_scan_removes_fifo_without_opening_it() {
    use std::os::unix::ffi::OsStrExt as _;

    let temp = tempfile::tempdir().unwrap();
    let mut writer = RunWriter::create_in(temp.path(), "fifo-run").unwrap();
    let artifacts = writer.dir().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let fifo = artifacts.join("blocked.pipe");
    let fifo_c = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);

    assert_eq!(writer.register_unlisted("artifacts", "files"), 0);
    assert!(!fifo.exists());
}
