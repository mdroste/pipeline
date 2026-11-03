//! Project records keep a sender's decisions out of the recipient's store, as
//! notes, claims, and evidence do. Each case is exported from one project and
//! imported into another through a real package.
use super::*;
use crate::workbench::project::{
    binding_coverage, editor_file, project_index, record, records, studio_mutate, theory_overview,
    LiteratureNote, NumericBinding, ResultRef, StudioAction, StudioMutation, TheoryCheck,
    TheoryNote,
};
use serde_json::json;

fn act(store: &Store, ws: &str, operation: &str, action: StudioAction) -> Value {
    studio_mutate(
        store,
        StudioMutation {
            workspace_id: ws.into(),
            operation_id: operation.into(),
            action,
        },
    )
    .unwrap()
}
fn id_of(value: &Value) -> String {
    value["id"].as_str().unwrap().to_string()
}

/// A confirmed link, a supported note with a passing check, a checked source, and
/// an accepted edit, each made the way the researcher makes it.
fn decided_records(temp: &tempfile::TempDir, store: &Store, ws: &str, root: &Path) -> PathBuf {
    populate(store, ws, root);
    let connection = store.connection().unwrap();
    connection.execute("INSERT INTO research_executions (id, workspace_id, adapter, command_json, cwd, input_manifest_json, dependency_hash, outcome, output_manifest_json, validation_json, snapshot_consistency, created_at) VALUES ('execution_sent', ?1, 'command', '[]', ?2, '{\"files\":[]}', 'hash', 'completed', '{\"artifacts\":[]}', '{}', 'complete', 't')", params![ws, root.to_string_lossy()]).unwrap();
    let result = json!({"resultId":"headline","estimand":"slope","specificationId":"main","sampleId":"sample","estimate":0.13,"standardError":null,"confidenceInterval":null,"n":100,"units":"log points","transformation":null,"uncertaintyMethod":"OLS","sourceExecutionId":"execution_sent","artifactLocator":"results.json"});
    connection.execute("INSERT INTO structured_results VALUES('result_sent','execution_sent','headline',?1,'t')", [result.to_string()]).unwrap();
    drop(connection);
    let anchor = records(store, ws, "anchor").unwrap().remove(0);
    act(
        store,
        ws,
        "bind",
        StudioAction::BindNumber {
            id: None,
            expected_revision: 0,
            binding: NumericBinding {
                result: ResultRef {
                    execution_id: "execution_sent".into(),
                    result_id: "headline".into(),
                },
                anchor_id: anchor.id,
                role: "prose".into(),
                component: "estimate".into(),
                printed: "0.13".into(),
                precision: 2,
                reported_units: "log points".into(),
                origin: "manual".into(),
                confirmed: true,
                imported_from: None,
                imported_status: None,
            },
        },
    );
    let note = act(
        store,
        ws,
        "theory",
        StudioAction::SaveTheory {
            id: None,
            expected_revision: 0,
            note: TheoryNote {
                kind: "proposition".into(),
                title: "Existence".into(),
                statement: "An equilibrium exists.".into(),
                body: String::new(),
                assumptions: vec![],
                assumption_ids: vec![],
                anchor_ids: vec![],
                related_ids: vec![],
                unresolved_steps: vec![],
                status: "supported".into(),
                rejection_reason: String::new(),
                origin: "manual".into(),
                promotions: vec![],
                imported_from: None,
                imported_status: None,
            },
        },
    );
    act(
        store,
        ws,
        "check",
        StudioAction::SaveCheck {
            id: None,
            expected_revision: 0,
            check: TheoryCheck {
                theory_id: id_of(&note),
                method: "analytical_argument".into(),
                outcome: "passed".into(),
                summary: "Fixed-point argument".into(),
                domain: String::new(),
                tolerance: None,
                precision: None,
                execution_id: None,
                recipe_run_id: None,
                anchor_ids: vec![],
                claims_generality: true,
                origin: "manual".into(),
            },
        },
    );
    act(
        store,
        ws,
        "literature",
        StudioAction::SaveLiterature {
            id: None,
            expected_revision: 0,
            note: LiteratureNote {
                question: "Who showed this?".into(),
                statement: "Smith shows it.".into(),
                citation_key: "smith".into(),
                source_version_id: None,
                start: None,
                end: None,
                quote: String::new(),
                identity_checked: true,
                support: "unsupported".into(),
                method: "manual".into(),
                related_version_ids: vec![],
            },
        },
    );
    fs::write(root.join("draft.tex"), "first").unwrap();
    let opened = editor_file(store, ws, None, "draft.tex").unwrap();
    act(
        store,
        ws,
        "save",
        StudioAction::SaveText {
            checkpoint_id: None,
            path: "draft.tex".into(),
            expected_hash: opened.hash,
            content: "second".into(),
        },
    );
    let path = temp.path().join("records.pwex");
    export_to(
        store,
        ws,
        &path,
        ExchangeSelection {
            record_kinds: [
                "anchor",
                "binding",
                "theory",
                "check",
                "literature",
                "application",
                "checkpoint",
            ]
            .map(String::from)
            .to_vec(),
            execution_ids: vec!["execution_sent".into()],
            ..Default::default()
        },
    );
    path
}
fn only(store: &Store, ws: &str, kind: &str) -> project::ProjectRecord {
    let mut all = records(store, ws, kind).unwrap();
    assert_eq!(all.len(), 1, "{kind}");
    all.remove(0)
}

#[test]
fn a_senders_project_decisions_arrive_held_back_and_marked() {
    let (temp, store, ws, root) = fixture();
    let path = decided_records(&temp, &store, &ws, &root);
    let imported = import_new(&store, &path, "import-1").workspace_id;

    let binding = only(&store, &imported, "binding");
    assert_eq!(binding.body["confirmed"], false);
    assert_eq!(binding.body["importedStatus"], "confirmed");
    assert!(binding.body["importedFrom"].is_string());
    let coverage = binding_coverage(&store, &imported).unwrap();
    assert_ne!(coverage[0]["state"], "current");
    assert!(coverage[0]["reasons"][0]
        .as_str()
        .unwrap()
        .starts_with("Imported link"));

    let note = only(&store, &imported, "theory");
    assert_eq!(note.body["status"], "open");
    assert_eq!(note.body["importedStatus"], "supported");
    let overview = theory_overview(&store, &imported).unwrap();
    let evidence = &overview.evidence[&note.id];
    assert_eq!((evidence.analytical, evidence.imported), (0, 1));
    assert!(evidence.label.starts_with("Only imported checks"));

    let literature = only(&store, &imported, "literature");
    assert_eq!(literature.body["checklist"]["identity"], "sender_reported");
    assert_eq!(literature.body["note"]["identityChecked"], false);
    assert!(literature.body["importedFrom"].is_string());

    let application = only(&store, &imported, "application");
    assert_eq!(application.body["state"], "imported");
    assert_eq!(application.body["importedStatus"], "applied");
    assert_eq!(
        only(&store, &imported, "checkpoint").body["state"],
        "imported"
    );
}

#[test]
fn an_interrupted_edit_from_a_sender_is_not_interrupted_here() {
    let (temp, store, ws, root) = fixture();
    let path = decided_records(&temp, &store, &ws, &root);
    store.connection().unwrap().execute("UPDATE project_records SET body_json=json_set(body_json,'$.state','recovery_required') WHERE kind='application' AND workspace_id=?1",[&ws]).unwrap();
    let path = path.with_file_name("interrupted.pwex");
    export_to(
        &store,
        &ws,
        &path,
        ExchangeSelection {
            record_kinds: vec!["application".into(), "checkpoint".into()],
            ..Default::default()
        },
    );
    let imported = import_new(&store, &path, "import-1").workspace_id;
    let application = only(&store, &imported, "application");
    assert_eq!(application.body["state"], "imported");
    assert_eq!(application.body["importedStatus"], "recovery_required");
    let index = project_index(&store).unwrap();
    let row = index.iter().find(|p| p.id == imported).unwrap();
    assert_eq!(row.interrupted_edits, 0);
}

#[test]
fn importing_the_same_package_again_finds_nothing_changed() {
    let (temp, store, ws, root) = fixture();
    let path = decided_records(&temp, &store, &ws, &root);
    let imported = import_new(&store, &path, "import-1").workspace_id;
    let again = import_into(&store, &path, &imported, "import-2");
    assert_eq!(again.conflicts, 0);
    assert_eq!(only(&store, &imported, "binding").body["confirmed"], false);
}

#[test]
fn confirming_here_is_the_researchers_own_decision_and_reimport_stays_quiet() {
    let (temp, store, ws, root) = fixture();
    let path = decided_records(&temp, &store, &ws, &root);
    let imported = import_new(&store, &path, "import-1").workspace_id;
    let held = only(&store, &imported, "binding");

    // The form sends back what it was given, markers included.
    let mut binding: NumericBinding = serde_json::from_value(held.body.clone()).unwrap();
    binding.reported_units = "log pts".into();
    act(
        &store,
        &imported,
        "unconfirmed-edit",
        StudioAction::BindNumber {
            id: Some(held.id.clone()),
            expected_revision: held.revision,
            binding: binding.clone(),
        },
    );
    let edited = only(&store, &imported, "binding");
    assert_eq!(edited.body["importedStatus"], "confirmed");

    binding.reported_units = "log points".into();
    binding.confirmed = true;
    act(
        &store,
        &imported,
        "confirm",
        StudioAction::BindNumber {
            id: Some(held.id.clone()),
            expected_revision: edited.revision,
            binding,
        },
    );
    let confirmed = only(&store, &imported, "binding");
    assert_eq!(confirmed.body["confirmed"], true);
    assert!(confirmed.body.get("importedStatus").is_none());
    assert!(confirmed.body.get("importedFrom").is_none());
    assert_eq!(
        import_into(&store, &path, &imported, "import-2").conflicts,
        0
    );
    assert_eq!(only(&store, &imported, "binding").body["confirmed"], true);
}

#[test]
fn taking_an_imported_record_does_not_confirm_it_for_the_recipient() {
    let (temp, store, ws, root) = fixture();
    let path = decided_records(&temp, &store, &ws, &root);
    let imported = import_new(&store, &path, "import-1").workspace_id;
    let held = only(&store, &imported, "binding");
    let mut binding: NumericBinding = serde_json::from_value(held.body.clone()).unwrap();
    binding.reported_units = "log pts".into();
    act(
        &store,
        &imported,
        "local-edit",
        StudioAction::BindNumber {
            id: Some(held.id.clone()),
            expected_revision: held.revision,
            binding,
        },
    );
    assert_eq!(
        import_into(&store, &path, &imported, "import-2").conflicts,
        1
    );
    let conflict = list_conflicts(&store, &imported).unwrap().remove(0);
    resolve_conflict(&store, &imported, &conflict.id, true).unwrap();

    let taken = record(&store, &imported, &held.id, "binding").unwrap();
    assert_eq!(taken.body["reportedUnits"], "log points");
    assert_eq!(taken.body["confirmed"], false);
    assert_eq!(taken.body["importedStatus"], "confirmed");
    assert!(taken.body["importedFrom"].is_string());
    assert_eq!(
        import_into(&store, &path, &imported, "import-3").conflicts,
        0
    );
}

#[test]
fn recording_a_check_here_counts_it_as_evidence() {
    let (temp, store, ws, root) = fixture();
    let path = decided_records(&temp, &store, &ws, &root);
    let imported = import_new(&store, &path, "import-1").workspace_id;
    let check = only(&store, &imported, "check");
    let note = only(&store, &imported, "theory");
    let body: project::TheoryCheckRecord = serde_json::from_value(check.body.clone()).unwrap();
    assert_eq!(body.imported_status.as_deref(), Some("passed"));
    act(
        &store,
        &imported,
        "own-check",
        StudioAction::SaveCheck {
            id: Some(check.id.clone()),
            expected_revision: check.revision,
            check: body.check,
        },
    );
    let overview = theory_overview(&store, &imported).unwrap();
    let evidence = &overview.evidence[&note.id];
    assert_eq!((evidence.analytical, evidence.imported), (1, 0));
}

#[cfg(unix)]
#[test]
fn a_package_lists_a_receipts_declared_inputs_and_says_it_cannot_reproduce_them() {
    let (temp, store, ws, root) = fixture();
    fs::write(root.join("panel.csv"), "id,y\n1,2\n").unwrap();
    let profile = research::save_execution_profile(
        &store,
        research::SaveExecutionProfileRequest {
            profile_id: None,
            workspace_id: ws.clone(),
            name: "Declared input".into(),
            adapter: "command".into(),
            argv: vec!["/bin/sh".into(), "-c".into(), "wc -l panel.csv".into()],
            cwd: root.to_string_lossy().into_owned(),
            environment: json!({}),
            inputs: vec!["panel.csv".into()],
            timeout_seconds: 10,
            outputs: vec![],
            expected_revision: None,
            operation_id: "profile".into(),
        },
    )
    .unwrap();
    let preview = research::preview_host_execution(&store, &profile.id).unwrap();
    research::authorize_host_execution(&store, &profile.id, &preview.fingerprint).unwrap();
    let receipt = research::run_execution(
        &store,
        research::RunExecutionRequest {
            plan_id: None,
            profile_id: profile.id,
            session_id: None,
            test_only: true,
            operation_id: "run".into(),
        },
    )
    .unwrap();

    let path = temp.path().join("receipt.pwex");
    export_to(
        &store,
        &ws,
        &path,
        ExchangeSelection {
            execution_ids: vec![receipt.id.clone()],
            ..Default::default()
        },
    );
    let mut archive = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();
    let manifest: Value =
        serde_json::from_reader(archive.by_name("manifest.json").unwrap()).unwrap();
    let references = manifest["externalReferences"].as_array().unwrap();
    assert!(
        references
            .iter()
            .any(|r| r["kind"] == "execution_input"
                && r["id"] == format!("{}:panel.csv", receipt.id)),
        "{references:?}"
    );
    assert!(manifest["limitations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l.as_str().unwrap().contains("not reproducible")));
    drop(archive);
    import_new(&store, &path, "import-receipt");
}
