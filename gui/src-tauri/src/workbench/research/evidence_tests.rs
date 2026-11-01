//! Only real support counts as support: checks speak for what they compared,
//! and a rendered page's receipt keeps a locator instead of the image.

use super::*;
use crate::workbench::store::{CreateSessionRequest, CreateWorkspaceRequest, UpdateSessionRequest};

struct Fixture {
    _temporary: tempfile::TempDir,
    store: Store,
    root: PathBuf,
    workspace_id: String,
    session_id: String,
}

fn fixture() -> Fixture {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("research-root");
    fs::create_dir(&root).unwrap();
    let store = Store::open_at(&temporary.path().join("store")).unwrap();
    let workspace_id = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Evidence".into(),
            root: Some(root.to_string_lossy().into_owned()),
            operation_id: "create-workspace".into(),
        })
        .unwrap()
        .record
        .id;
    let session_id = store
        .create_session(CreateSessionRequest {
            workspace_id: Some(workspace_id.clone()),
            title: "Question".into(),
            operation_id: "create-session".into(),
        })
        .unwrap()
        .record
        .id;
    Fixture {
        _temporary: temporary,
        store,
        root,
        workspace_id,
        session_id,
    }
}

fn execution(f: &Fixture, id: &str, artifact: &str, validation: &str) {
    f.store.connection().unwrap().execute("INSERT INTO research_executions (id, workspace_id, adapter, command_json, cwd, input_manifest_json, dependency_hash, outcome, output_manifest_json, validation_json, snapshot_consistency, created_at) VALUES (?1, ?2, 'command', '[]', ?3, '{}', ?1, 'completed', ?4, ?5, 'complete', 't')", params![id, f.workspace_id, f.root.to_string_lossy(), json!({"artifacts":[{"path":"result.json","artifactId":artifact}]}).to_string(), validation]).unwrap();
}

fn result(f: &Fixture, id: &str, run: &str, estimate: f64) {
    record_structured_result(
        &f.store,
        RecordStructuredResultRequest {
            workspace_id: f.workspace_id.clone(),
            result: ResearchResultV1 {
                result_id: id.into(),
                estimand: "ATE".into(),
                specification_id: "spec".into(),
                sample_id: "sample".into(),
                estimate,
                standard_error: Some(0.1),
                confidence_interval: None,
                n: Some(100),
                units: "points".into(),
                transformation: None,
                uncertainty_method: None,
                source_execution_id: run.into(),
                artifact_locator: "result.json#results/0".into(),
            },
            operation_id: format!("record-{id}"),
        },
    )
    .unwrap();
}

/// Two local runs that agree, with results named `left` and `right`.
fn compared_runs(f: &Fixture) {
    execution(f, "run-left", "artifact-left", "{}");
    execution(f, "run-right", "artifact-right", "{}");
    result(f, "left", "run-left", 1.0);
    result(f, "right", "run-right", 1.01);
}

fn claim(f: &Fixture, operation: &str) -> ClaimRecord {
    propose_claim(
        &f.store,
        ProposeClaimRequest {
            workspace_id: f.workspace_id.clone(),
            paper_id: None,
            claim: "The estimates reproduce.".into(),
            kind: "result".into(),
            origin: "user".into(),
            operation_id: operation.into(),
        },
        false,
    )
    .unwrap()
}

fn evidence_on(
    f: &Fixture,
    claim: &ClaimRecord,
    target_type: &str,
    target_id: &str,
    operation: &str,
    model: bool,
) -> EvidenceRecord {
    propose_evidence(
        &f.store,
        ProposeEvidenceRequest {
            workspace_id: f.workspace_id.clone(),
            claim_version_id: claim.version_id.clone(),
            target_type: target_type.into(),
            target_id: target_id.into(),
            locator: None,
            relation: "supports".into(),
            assessor: "user".into(),
            operation_id: operation.into(),
        },
        model,
    )
    .unwrap()
}

fn verify(f: &Fixture, evidence: &str, tolerance: f64, operation: &str) -> WorkbenchResult<bool> {
    verify_evidence_results(
        &f.store,
        VerifyEvidenceResultsRequest {
            evidence_id: evidence.into(),
            left_result_id: "left".into(),
            right_result_id: "right".into(),
            absolute_tolerance: tolerance,
            rationale: None,
            operation_id: operation.into(),
        },
    )
    .map(|record| record.passed)
}

fn evidence(f: &Fixture, id: &str) -> EvidenceRecord {
    get_evidence(&f.store, &f.workspace_id, id).unwrap()
}

fn paper(f: &Fixture, text: &str, paper_id: Option<String>, operation: &str) -> PaperWithRevision {
    let path = f.root.join("paper.md");
    fs::write(&path, text).unwrap();
    import_paper(
        &f.store,
        ImportPaperRequest {
            workspace_id: f.workspace_id.clone(),
            paper_id,
            title: "Paper".into(),
            role: "manuscript".into(),
            path: path.to_string_lossy().into_owned(),
            operation_id: operation.into(),
        },
    )
    .unwrap()
}

#[test]
fn comparing_unrelated_results_cannot_refresh_stale_evidence() {
    let f = fixture();
    compared_runs(&f);
    let first = paper(&f, "First version.\n", None, "paper-one");
    let revision = first.revision.as_ref().unwrap().id.clone();
    let link = evidence_on(
        &f,
        &claim(&f, "claim"),
        "paper_revision",
        &revision,
        "link",
        false,
    );
    paper(
        &f,
        "Second version.\n",
        Some(first.paper.id.clone()),
        "paper-two",
    );
    let stale = evidence(&f, &link.id);
    assert_eq!(stale.freshness, "stale");

    assert!(verify(&f, &link.id, 0.05, "check").is_err());

    let after = evidence(&f, &link.id);
    assert_eq!(after.freshness, "stale");
    assert_eq!(after.stale_reason, stale.stale_reason);
    assert_eq!(after.assessment, "not_checked");
    let records: i64 = f
        .store
        .connection()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM verification_records", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(records, 0);
}

#[test]
fn a_check_of_its_own_target_records_the_outcome_and_never_changes_freshness() {
    let f = fixture();
    compared_runs(&f);
    let claim = claim(&f, "claim");
    let on_run = evidence_on(&f, &claim, "execution", "run-right", "on-run", false);
    let on_output = evidence_on(&f, &claim, "derivation", "derived", "on-derivation", false);
    f.store
        .connection()
        .unwrap()
        .execute(
            "UPDATE evidence_links SET freshness='stale', stale_reason='Changed' WHERE id=?1",
            [&on_run.id],
        )
        .unwrap();

    assert!(verify(&f, &on_run.id, 0.05, "check").unwrap());
    let checked = evidence(&f, &on_run.id);
    assert_eq!(checked.assessment, "check_passed");
    assert_eq!(checked.freshness, "stale");
    assert_eq!(checked.stale_reason.as_deref(), Some("Changed"));
    assert!(verify(&f, &on_output.id, 0.05, "derivation-check").is_err());
}

#[test]
fn a_check_can_target_an_output_of_a_compared_run() {
    let f = fixture();
    compared_runs(&f);
    f.store.connection().unwrap().execute("INSERT INTO artifacts (id, workspace_id, content_hash, media_kind, size_bytes, origin, storage_reference, created_at) VALUES ('artifact-left', ?1, 'hash', 'json', 1, 'execution_output', '/nowhere', 't'), ('other', ?1, 'other-hash', 'json', 1, 'execution_output', '/nowhere', 't')", [&f.workspace_id]).unwrap();
    let claim = claim(&f, "claim");
    let output = evidence_on(&f, &claim, "artifact", "artifact-left", "output", false);
    let other = evidence_on(&f, &claim, "artifact", "other", "other", false);
    assert!(verify(&f, &output.id, 0.05, "check").unwrap());
    assert!(verify(&f, &other.id, 0.05, "check-other").is_err());
}

#[test]
fn a_passing_check_keeps_a_confirmation_and_a_failing_one_shows_up() {
    let f = fixture();
    compared_runs(&f);
    let link = evidence_on(
        &f,
        &claim(&f, "claim"),
        "execution",
        "run-right",
        "link",
        false,
    );
    confirm_evidence(
        &f.store,
        ConfirmEvidenceRequest {
            evidence_id: link.id.clone(),
            operation_id: "confirm".into(),
        },
    )
    .unwrap();
    assert!(verify(&f, &link.id, 0.05, "passing").unwrap());
    let kept = evidence(&f, &link.id);
    assert_eq!(
        (kept.assessment.as_str(), kept.assessor.as_str()),
        ("human_confirmed", "user")
    );
    assert!(!verify(&f, &link.id, 0.0, "failing").unwrap());
    assert_eq!(evidence(&f, &link.id).assessment, "check_failed");
}

#[test]
fn imported_results_cannot_check_each_other() {
    let f = fixture();
    let stamp = json!({"importedFrom":"sender"}).to_string();
    execution(&f, "run-left", "artifact-left", &stamp);
    execution(&f, "run-right", "artifact-right", &stamp);
    execution(&f, "run-local", "artifact-local", "{}");
    result(&f, "left", "run-left", 1.0);
    result(&f, "right", "run-right", 1.01);
    let link = evidence_on(
        &f,
        &claim(&f, "claim"),
        "execution",
        "run-right",
        "link",
        false,
    );
    assert!(verify(&f, &link.id, 0.05, "both-imported").is_err());
    assert_eq!(evidence(&f, &link.id).assessment, "not_checked");
    // A rerun on this computer is a real check of the sender's number.
    result(&f, "local", "run-local", 1.0);
    let replicated = verify_evidence_results(
        &f.store,
        VerifyEvidenceResultsRequest {
            evidence_id: link.id.clone(),
            left_result_id: "local".into(),
            right_result_id: "right".into(),
            absolute_tolerance: 0.05,
            rationale: None,
            operation_id: "replicated".into(),
        },
    )
    .unwrap();
    assert!(replicated.passed);
    assert_eq!(evidence(&f, &link.id).assessment, "check_passed");
}

#[test]
fn only_confirmed_or_checked_links_count_as_support_for_an_accepted_claim() {
    let f = fixture();
    let paper = paper(&f, "Text.\n", None, "paper");
    let revision = paper.revision.unwrap().id;
    let accepted = |operation: &str| {
        let record = claim(&f, operation);
        set_claim_state(
            &f.store,
            SetClaimStateRequest {
                claim_id: record.id.clone(),
                state: "accepted".into(),
                operation_id: format!("accept-{operation}"),
            },
        )
        .unwrap();
        record
    };
    let unsupported = |id: &str| {
        research_ledger(&f.store, &f.workspace_id)
            .unwrap()
            .unsupported_accepted_claims
            .contains(&id.to_string())
    };
    let set = |id: &str, assessment: &str| {
        f.store
            .connection()
            .unwrap()
            .execute(
                "UPDATE evidence_links SET assessment=?2 WHERE id=?1",
                params![id, assessment],
            )
            .unwrap();
    };

    let guess = accepted("guess");
    evidence_on(&f, &guess, "paper_revision", &revision, "guess-link", true);
    assert!(unsupported(&guess.id), "a model proposal is not support");

    let failed = accepted("failed");
    let link = evidence_on(
        &f,
        &failed,
        "paper_revision",
        &revision,
        "failed-link",
        false,
    );
    set(&link.id, "check_failed");
    assert!(unsupported(&failed.id), "a failed check is not support");
    set(&link.id, "not_checked");
    assert!(unsupported(&failed.id), "an unchecked link is not support");

    let checked = accepted("checked");
    let link = evidence_on(
        &f,
        &checked,
        "paper_revision",
        &revision,
        "checked-link",
        false,
    );
    set(&link.id, "check_passed");
    assert!(!unsupported(&checked.id));

    let confirmed = accepted("confirmed");
    let link = evidence_on(
        &f,
        &confirmed,
        "paper_revision",
        &revision,
        "confirmed-link",
        true,
    );
    confirm_evidence(
        &f.store,
        ConfirmEvidenceRequest {
            evidence_id: link.id,
            operation_id: "confirm".into(),
        },
    )
    .unwrap();
    assert!(!unsupported(&confirmed.id));
}

const ONE_PAGE_PDF: &[u8] = b"%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]>>endobj\ntrailer<</Root 1 0 R>>\n";

fn bound_conversation(f: &Fixture) {
    let session = f.store.session_snapshot(&f.session_id).unwrap().session;
    f.store
        .update_session(UpdateSessionRequest {
            session_id: session.id,
            expected_revision: session.revision,
            operation_id: "preset".into(),
            title: None,
            draft: None,
            overrides: Some(json!({"mode":"inspect"})),
            archived: None,
            preset_id: Some("research_assistant".into()),
            paper_id: None,
            clear_paper: None,
        })
        .unwrap();
    f.store
        .bind_session(&f.session_id, "runtime", "thread")
        .unwrap();
}

fn stored_receipt(f: &Fixture) -> String {
    f.store
        .connection()
        .unwrap()
        .query_row(
            "SELECT result_json FROM tool_receipts WHERE call_id='call'",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn a_rendered_page_receipt_holds_a_locator_and_replays_the_image() {
    if std::process::Command::new("pdftoppm")
        .arg("-v")
        .output()
        .is_err()
    {
        return;
    }
    let f = fixture();
    bound_conversation(&f);
    let path = f.root.join("paper.pdf");
    fs::write(&path, ONE_PAGE_PDF).unwrap();
    let revision = import_paper(
        &f.store,
        ImportPaperRequest {
            workspace_id: f.workspace_id.clone(),
            paper_id: None,
            title: "Paper".into(),
            role: "manuscript".into(),
            path: path.to_string_lossy().into_owned(),
            operation_id: "paper".into(),
        },
    )
    .unwrap()
    .revision
    .unwrap();
    let call = json!({"threadId":"thread","turnId":"turn","callId":"call","tool":"workbench_paper_page","arguments":{"revisionId":revision.id,"page":1}});

    let first = handle_dynamic_tool_call(&f.store, &call).unwrap();
    assert!(first.to_string().contains("base64,"), "{first}");
    let stored = stored_receipt(&f);
    assert!(!stored.contains("base64,"));
    assert!(stored.contains("artifactId"));

    assert_eq!(handle_dynamic_tool_call(&f.store, &call).unwrap(), first);
}

#[test]
fn replaying_a_page_whose_image_is_gone_says_so() {
    let f = fixture();
    bound_conversation(&f);
    let binding: String = f
        .store
        .connection()
        .unwrap()
        .query_row("SELECT id FROM session_bindings", [], |r| r.get(0))
        .unwrap();
    let image = f.store.root_path().join("blobs/page.jpg");
    fs::write(&image, b"jpeg bytes").unwrap();
    let image_hash = hash_bytes(b"jpeg bytes");
    let connection = f.store.connection().unwrap();
    connection.execute("INSERT INTO artifacts (id, workspace_id, content_hash, media_kind, size_bytes, origin, storage_reference, created_at) VALUES ('page', ?1, ?2, 'page_image', 10, 'paper_page_render', ?3, 't')", params![f.workspace_id, image_hash, image.to_string_lossy()]).unwrap();
    let stored = without_page_images(
        &json!({"success":true,"contentItems":[{"type":"inputText","text":json!({"artifactId":"page","imageHash":image_hash}).to_string()},{"type":"inputImage","imageUrl":"data:image/jpeg;base64,AAAA"}]}),
    );
    connection.execute("INSERT INTO tool_receipts (id, workspace_id, session_id, binding_id, provider_turn_id, call_id, tool_name, catalog_version, arguments_json, state, result_json, started_at) VALUES ('receipt', ?1, ?2, ?3, 'turn', 'call', 'workbench_paper_page', 1, '{}', 'completed', ?4, 't')", params![f.workspace_id, f.session_id, binding, stored.to_string()]).unwrap();
    let call = json!({"threadId":"thread","turnId":"turn","callId":"call","tool":"workbench_paper_page","arguments":{"revisionId":"r","page":1}});

    let replay = handle_dynamic_tool_call(&f.store, &call).unwrap();
    assert_eq!(
        replay["contentItems"][1]["imageUrl"],
        format!(
            "data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(b"jpeg bytes")
        )
    );

    fs::remove_file(&image).unwrap();
    let replay = handle_dynamic_tool_call(&f.store, &call).unwrap();
    assert!(replay.to_string().contains("no longer stored"));
    assert!(replay["contentItems"][1].get("imageUrl").is_none());
}

#[test]
fn a_run_cannot_check_evidence_against_itself() {
    let f = fixture();
    compared_runs(&f);
    result(&f, "right-again", "run-right", 1.01);
    let link = evidence_on(
        &f,
        &claim(&f, "claim"),
        "execution",
        "run-right",
        "link",
        false,
    );
    for (left, operation) in [("right", "same-result"), ("right-again", "same-run")] {
        let error = verify_evidence_results(
            &f.store,
            VerifyEvidenceResultsRequest {
                evidence_id: link.id.clone(),
                left_result_id: left.into(),
                right_result_id: "right".into(),
                absolute_tolerance: 0.05,
                rationale: None,
                operation_id: operation.into(),
            },
        )
        .unwrap_err();
        assert!(error.message.contains("same run"), "{error:?}");
    }
    assert_eq!(evidence(&f, &link.id).assessment, "not_checked");
}

#[test]
fn a_passing_check_gives_back_a_confirmation_a_failing_one_displaced() {
    let f = fixture();
    compared_runs(&f);
    let link = evidence_on(
        &f,
        &claim(&f, "claim"),
        "execution",
        "run-right",
        "link",
        false,
    );
    confirm_evidence(
        &f.store,
        ConfirmEvidenceRequest {
            evidence_id: link.id.clone(),
            operation_id: "confirm".into(),
        },
    )
    .unwrap();
    assert!(!verify(&f, &link.id, 0.0, "failing").unwrap());
    assert!(!verify(&f, &link.id, 0.0, "failing-again").unwrap());
    let failed = evidence(&f, &link.id);
    assert_eq!(failed.assessment, "check_failed");
    assert!(verify(&f, &link.id, 0.05, "passing").unwrap());
    let restored = evidence(&f, &link.id);
    assert_eq!(
        (restored.assessment.as_str(), restored.assessor.as_str()),
        ("human_confirmed", "user")
    );
}

#[test]
fn a_passing_check_after_a_failure_confirms_nothing_on_its_own() {
    let f = fixture();
    compared_runs(&f);
    let link = evidence_on(
        &f,
        &claim(&f, "claim"),
        "execution",
        "run-right",
        "link",
        false,
    );
    assert!(!verify(&f, &link.id, 0.0, "failing").unwrap());
    assert!(verify(&f, &link.id, 0.05, "passing").unwrap());
    assert_eq!(evidence(&f, &link.id).assessment, "check_passed");
}

#[test]
fn a_page_number_too_large_for_a_page_is_refused() {
    let f = fixture();
    bound_conversation(&f);
    let call = json!({"threadId":"thread","turnId":"turn","callId":"call","tool":"workbench_paper_page","arguments":{"revisionId":"r","page":4294967297u64}});
    let result = handle_dynamic_tool_call(&f.store, &call);
    assert!(
        format!("{result:?}").contains("That page is not in this paper"),
        "{result:?}"
    );
}
