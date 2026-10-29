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
        input_identity: Default::default(),
        profile_id: "deep-review".into(),
        profile_name: "Deep Review".into(),
        workflow_source: "profile:deep-review".into(),
        workflow_fingerprint: "sha256:test".into(),
        specialist_catalog_revision: "sha256:catalog".into(),
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
    assert_eq!(s.specialist_catalog_revision, "sha256:catalog");
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
fn input_identity_is_stable_across_input_kinds_and_reruns() {
    let document = input_identity("/papers/main.pdf", "document", "hash-a", None);
    let document_again = input_identity("/papers/main.pdf", "document", "hash-b", None);
    assert_eq!(document.lineage_id, document_again.lineage_id);
    assert_ne!(document.content_hash, document_again.content_hash);

    let moved_tree = input_identity(
        "/new-location/source",
        "source_tree",
        "tree-hash",
        Some(&document),
    );
    assert_eq!(moved_tree.lineage_id, document.lineage_id);
    assert!(moved_tree.selection_key.starts_with("source_tree:"));

    assert_eq!(
        input_identity("", "none", "", None),
        InputIdentity::default()
    );
}

#[test]
fn input_basename_handles_empty_and_paths() {
    assert_eq!(input_basename(""), "(no input)");
    assert_eq!(input_basename("   "), "(no input)");
    assert_eq!(input_basename("/a/b/c.pdf"), "c.pdf");
    assert_eq!(input_basename("relative.tex"), "relative.tex");
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
fn capped_artifact_reader_never_loads_past_limit() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    use std::io::Write as _;
    file.write_all(&vec![b'x'; 4096]).unwrap();
    let (bytes, truncated) = read_at_most(file.path(), 128).unwrap();
    assert_eq!(bytes.len(), 128);
    assert!(truncated);
}

#[test]
fn model_written_programs_are_classified_as_executable_not_binary() {
    for name in [
        "payload.exe",
        "summary.jar",
        "report.pkg",
        "x.dmg",
        "run.bat",
        "a.ps1",
    ] {
        assert_eq!(detect_kind(name, b""), "executable", "{name}");
    }
    assert_eq!(detect_kind("table.xlsx", b""), "binary");
    assert_eq!(detect_kind("script.sh", b""), "code");
}
