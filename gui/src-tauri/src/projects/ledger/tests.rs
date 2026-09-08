use super::*;

fn occurrence(
    key: &str,
    run_id: &str,
    title: &str,
    section: &str,
    input_mode: &str,
) -> ProjectIssueOccurrence {
    ProjectIssueOccurrence {
        key: key.to_string(),
        run_id: run_id.to_string(),
        issue_id: "stable-finding".to_string(),
        source_key: String::new(),
        observed_at: format!("2026-08-0{}T00:00:00Z", run_id.len()),
        profile_id: "review".to_string(),
        profile_name: "Review".to_string(),
        input_name: "input".to_string(),
        input_mode: input_mode.to_string(),
        input_interpretation: input_mode.to_string(),
        step_id: "synthesis".to_string(),
        step_label: "Synthesis".to_string(),
        title: title.to_string(),
        severity: "high".to_string(),
        section: section.to_string(),
        body: "Details".to_string(),
        evidence: Vec::new(),
        annotation_status: String::new(),
        annotation_note: String::new(),
    }
}

#[test]
fn conservative_matching_is_report_type_neutral_and_preserves_decisions() {
    let first = occurrence(
        "occ-aaaaaaaa",
        "run_1",
        "Unbounded retry loop",
        "worker.rs",
        "folder",
    );
    let second = occurrence(
        "occ-bbbbbbbb",
        "run_22",
        "Unbounded retry loop",
        "worker.rs",
        "document",
    );
    let mut ledger = sync_ledger(
        empty_ledger("project"),
        "project",
        vec![first, second],
        Vec::new(),
        &HashSet::new(),
    );
    assert_eq!(ledger.issues.len(), 1);
    assert_eq!(ledger.issues[0].occurrences.len(), 2);
    ledger.issues[0].status = "dismissed".to_string();
    ledger.issues[0].decision_updated = "2026-08-10T00:00:00Z".to_string();

    let refreshed = sync_ledger(
        ledger,
        "project",
        vec![occurrence(
            "occ-bbbbbbbb",
            "run_22",
            "Unbounded retry loop",
            "worker.rs",
            "folder",
        )],
        Vec::new(),
        &HashSet::new(),
    );
    assert_eq!(refreshed.issues.len(), 1);
    assert_eq!(refreshed.issues[0].status, "dismissed");
    assert_eq!(refreshed.issues[0].occurrences.len(), 1);
}

#[test]
fn decisions_survive_purged_and_unreadable_runs() {
    let first = occurrence(
        "occ-aaaaaaaa",
        "run_1",
        "Unbounded retry loop",
        "worker.rs",
        "document",
    );
    let mut ledger = sync_ledger(
        empty_ledger("project"),
        "project",
        vec![first.clone()],
        Vec::new(),
        &HashSet::new(),
    );
    ledger.issues[0].status = "dismissed".to_string();
    ledger.issues[0].decision_updated = "2026-08-10T00:00:00Z".to_string();
    ledger.issues[0].note = "Intentional design".to_string();

    // run_1 could not be re-scanned (unreadable report): retained, not dropped.
    let warned = sync_ledger(
        ledger,
        "project",
        Vec::new(),
        Vec::new(),
        &HashSet::from(["run_1".to_string()]),
    );
    assert_eq!(warned.issues.len(), 1);
    assert!(warned.issues[0].archived);
    assert_eq!(warned.issues[0].occurrences.len(), 1);

    // run_1 was purged from the project entirely: the decision, note, and
    // last-known occurrences survive as an archived issue.
    let purged = sync_ledger(warned, "project", Vec::new(), Vec::new(), &HashSet::new());
    assert_eq!(purged.issues.len(), 1);
    assert!(purged.issues[0].archived);
    assert_eq!(purged.issues[0].status, "dismissed");
    assert_eq!(purged.issues[0].note, "Intentional design");
    assert_eq!(purged.issues[0].occurrences.len(), 1);

    // A fresh observation of the same occurrence reactivates the issue.
    let reobserved = sync_ledger(purged, "project", vec![first], Vec::new(), &HashSet::new());
    assert!(!reobserved.issues[0].archived);
    assert_eq!(reobserved.issues[0].status, "dismissed");
}

#[test]
fn short_generic_titles_do_not_merge_without_a_stable_source_key() {
    let mut first = occurrence("occ-aaaaaaaa", "run_1", "Missing test", "", "folder");
    let mut second = occurrence("occ-bbbbbbbb", "run_22", "Missing test", "", "folder");
    first.issue_id = "1".to_string();
    second.issue_id = "2".to_string();
    let ledger = sync_ledger(
        empty_ledger("project"),
        "project",
        vec![first, second],
        Vec::new(),
        &HashSet::new(),
    );
    assert_eq!(ledger.issues.len(), 2);
}

#[test]
fn generated_ordinal_ids_do_not_override_conservative_matching() {
    let mut first = occurrence(
        "occ-aaaaaaaa",
        "run_1",
        "Missing authorization check",
        "src/api.rs",
        "folder",
    );
    let mut second = occurrence(
        "occ-bbbbbbbb",
        "run_22",
        "Unbounded retry policy",
        "src/worker.rs",
        "folder",
    );
    first.issue_id = "issue-1".to_string();
    second.issue_id = "issue-1".to_string();
    let ledger = sync_ledger(
        empty_ledger("project"),
        "project",
        vec![first, second],
        Vec::new(),
        &HashSet::new(),
    );
    assert_eq!(ledger.issues.len(), 2);
}

#[test]
fn run_annotations_can_mark_an_automatic_regression() {
    let mut first = occurrence(
        "occ-aaaaaaaa",
        "run_1",
        "Identification assumption is unstated",
        "Model",
        "document",
    );
    first.annotation_status = "done".to_string();
    let second = occurrence(
        "occ-bbbbbbbb",
        "run_22",
        "Identification assumption is unstated",
        "Model",
        "document",
    );
    let ledger = sync_ledger(
        empty_ledger("project"),
        "project",
        vec![first, second],
        Vec::new(),
        &HashSet::new(),
    );
    assert_eq!(ledger.issues[0].status, "regressed");
}

#[test]
fn unsafe_evidence_paths_are_not_retained() {
    let value = serde_json::json!({
        "artifact_path": "../outside.txt",
        "page": 4,
        "description": "Relevant source"
    });
    let evidence = parse_evidence(&value, &HashSet::new()).unwrap();
    assert_eq!(evidence.page, Some(4));
    assert!(evidence.artifact_path.is_empty());
}

#[test]
fn source_tree_evidence_resolves_saved_paths_and_retains_lines() {
    let value = serde_json::json!({
        "file": "src/worker.rs",
        "line": 41,
        "line_end": 45,
        "label": "Retry branch"
    });
    let paths = HashSet::from(["artifacts/source/src/worker.rs"]);
    let evidence = parse_evidence(&value, &paths).unwrap();
    assert_eq!(evidence.artifact_path, "artifacts/source/src/worker.rs");
    assert_eq!(evidence.line_start, Some(41));
    assert_eq!(evidence.line_end, Some(45));
    assert_eq!(evidence.description, "Retry branch");
}

#[test]
fn generic_issue_aliases_and_duplicate_ids_match_run_annotations() {
    let report: crate::models::PipelineReport = serde_json::from_value(serde_json::json!({
        "step_outputs": [{
            "step_id": "audit",
            "step_label": "Source audit",
            "raw_text": r#"{"issues":[
                {"issue_id":"issue-1","message":"Retry loop has no bound","priority":"critical","file":"src/worker.rs","explanation":"Terminal failures retry forever.","line":41},
                {"issue_id":"issue-1","summary":"Missing authorization check","level":"medium","path":"src/api.rs","detail":"The handler trusts the caller."}
            ]}"#
        }]
    }))
    .unwrap();
    let manifest: crate::runs::RunManifest = serde_json::from_value(serde_json::json!({
        "run_id": "run_source",
        "created": "2026-08-10T00:00:00Z",
        "input_path": "/work/repository",
        "input_mode": "folder",
        "input_interpretation": "source_tree",
        "profile_id": "code-review",
        "profile_name": "Code Review",
        "provider": "codex",
        "artifacts": [{
            "rel_path": "artifacts/source/src/worker.rs",
            "label": "worker.rs",
            "kind": "code",
            "bytes": 100,
            "sha256": "abc",
            "group": "source"
        }]
    }))
    .unwrap();
    let annotations = HashMap::from([(
        "issue-1#2".to_string(),
        ("done".to_string(), "Fixed in the next revision".to_string()),
    )]);

    let occurrences = extract_occurrences(&report, &manifest, &annotations);
    assert_eq!(occurrences.len(), 2);
    assert_eq!(occurrences[0].title, "Retry loop has no bound");
    assert_eq!(occurrences[0].severity, "high");
    assert_eq!(occurrences[0].section, "src/worker.rs");
    assert_eq!(
        occurrences[0].evidence[0].artifact_path,
        "artifacts/source/src/worker.rs"
    );
    assert_eq!(occurrences[0].evidence[0].line_start, Some(41));
    assert_eq!(occurrences[1].issue_id, "issue-1#2");
    assert_eq!(occurrences[1].annotation_status, "done");
}
