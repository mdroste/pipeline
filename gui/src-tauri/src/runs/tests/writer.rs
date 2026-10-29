use super::*;

#[test]
fn cited_source_files_become_bounded_run_artifacts() {
    let source = tempfile::tempdir().unwrap();
    fs::create_dir_all(source.path().join("src")).unwrap();
    fs::write(source.path().join("src/lib.rs"), "fn invariant() {}\n").unwrap();
    let runs = tempfile::tempdir().unwrap();
    let mut writer = RunWriter::create_in(runs.path(), "source-evidence").unwrap();
    let evidence = crate::models::FindingEvidence {
        source_path: "src/lib.rs".into(),
        line_start: Some(1),
        ..Default::default()
    };
    let mut products = crate::models::RunProducts {
        findings: Some(crate::models::FindingSet {
            findings: vec![crate::models::Finding {
                id: "invariant".into(),
                title: "Broken invariant".into(),
                evidence: vec![evidence.clone(), evidence],
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };

    assert!(writer
        .capture_source_evidence(&mut products, source.path())
        .is_empty());
    let findings = products.findings.unwrap();
    let artifact_path = "context/source-evidence/src/lib.rs";
    assert!(findings.findings[0]
        .evidence
        .iter()
        .all(|item| item.artifact_path == artifact_path));
    assert_eq!(
        fs::read_to_string(writer.dir().join(artifact_path)).unwrap(),
        "fn invariant() {}\n"
    );
    let manifest = writer.current_manifest();
    assert_eq!(
        manifest
            .artifacts
            .iter()
            .filter(|artifact| artifact.rel_path == artifact_path)
            .count(),
        1
    );
}

#[test]
fn source_evidence_capture_rejects_path_traversal() {
    let source = tempfile::tempdir().unwrap();
    let runs = tempfile::tempdir().unwrap();
    let mut writer = RunWriter::create_in(runs.path(), "unsafe-source-evidence").unwrap();
    let mut products = crate::models::RunProducts {
        findings: Some(crate::models::FindingSet {
            findings: vec![crate::models::Finding {
                id: "unsafe".into(),
                title: "Unsafe path".into(),
                evidence: vec![crate::models::FindingEvidence {
                    source_path: "../secret.txt".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };

    let warnings = writer.capture_source_evidence(&mut products, source.path());
    assert!(warnings.iter().any(|warning| warning.contains("unsafe")));
    assert!(products.findings.unwrap().findings[0].evidence[0]
        .artifact_path
        .is_empty());
    assert!(writer.current_manifest().artifacts.is_empty());
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

#[test]
fn captured_tree_registration_keeps_every_file_past_the_model_output_cap() {
    let temp = tempfile::tempdir().unwrap();
    let mut writer = RunWriter::create_in(temp.path(), "captured-run").unwrap();
    let tree = writer.dir().join("source-snapshot/tree");
    fs::create_dir_all(&tree).unwrap();
    for index in 0..501 {
        fs::write(tree.join(format!("{index:04}.tex")), "x").unwrap();
    }
    assert_eq!(
        writer
            .register_captured_tree("source-snapshot", "source")
            .unwrap(),
        501
    );
    assert_eq!(fs::read_dir(&tree).unwrap().count(), 501);
}

#[test]
fn failed_finish_preserves_the_last_valid_manifest_and_captured_document() {
    let temp = tempfile::tempdir().unwrap();
    let mut writer = RunWriter::create_in(temp.path(), "failed-finish-contract").unwrap();
    writer
        .add_text(
            DOCUMENT_TEXT_PATH,
            "Readable document",
            "document",
            "captured paper",
        )
        .unwrap();
    let run_dir = writer.dir().to_path_buf();
    let before = fs::read(run_dir.join("manifest.json")).unwrap();
    let mut meta = RunFinishMeta {
        status: "done".into(),
        ..Default::default()
    };
    meta.variables
        .insert("too-large".into(), "x".repeat(MAX_MANIFEST_BYTES));
    assert!(writer.finish(meta).unwrap_err().contains("safety limit"));
    assert_eq!(fs::read(run_dir.join("manifest.json")).unwrap(), before);
    assert_eq!(
        fs::read_to_string(run_dir.join(DOCUMENT_TEXT_PATH)).unwrap(),
        "captured paper"
    );
    let manifest: RunManifest = serde_json::from_slice(&before).unwrap();
    assert_eq!(manifest.status, "running");
    assert!(!run_dir.join("report.json").exists());
}

#[test]
fn manifest_publication_failure_keeps_existing_destination_and_cleans_staging() {
    let temp = tempfile::tempdir().unwrap();
    let writer = RunWriter::create_in(temp.path(), "publication-failure-contract").unwrap();
    let run_dir = writer.dir().to_path_buf();
    let manifest = writer.current_manifest();
    fs::remove_file(run_dir.join("manifest.json")).unwrap();
    fs::create_dir(run_dir.join("manifest.json")).unwrap();
    fs::write(run_dir.join("manifest.json/existing"), "retain").unwrap();
    let error = write_manifest(&run_dir, &manifest).unwrap_err();
    assert!(error.contains("Failed to publish manifest"), "{error}");
    assert_eq!(
        fs::read_to_string(run_dir.join("manifest.json/existing")).unwrap(),
        "retain"
    );
    assert!(fs::read_dir(&run_dir).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".tmp")));
    drop(writer);
    assert_eq!(
        fs::read_to_string(run_dir.join("manifest.json/existing")).unwrap(),
        "retain"
    );
}
