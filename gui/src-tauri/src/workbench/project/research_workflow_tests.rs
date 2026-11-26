use super::test_support::*;
use super::*;
use crate::workbench::desk;

#[test]
fn submission_exports_captured_bytes_after_working_files_change() {
    let f = fixture();
    fs::write(f.root.join("main.tex"), "submitted").unwrap();
    let request = submissions::SubmissionRequest {
        title: "Submission one".into(),
        handoff: "Question and reproducibility steps".into(),
        files: BTreeMap::from([("main.tex".into(), hash(b"submitted"))]),
        revisions: vec![],
        records: vec![],
        executions: vec![],
        operation_id: "submission-one".into(),
    };
    let saved = submissions::capture(&f.store, &f.ws, request.clone()).unwrap();
    fs::write(f.root.join("main.tex"), "later edits").unwrap();
    assert_eq!(
        submissions::capture(&f.store, &f.ws, request).unwrap().id,
        saved.id
    );
    let destination = f._temp.path().join("handoff.zip");
    submissions::export(
        &f.store,
        &f.ws,
        &saved.id,
        &saved.content_hash,
        destination.to_str().unwrap(),
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(fs::File::open(&destination).unwrap()).unwrap();
    let mut text = String::new();
    archive
        .by_name("files/main.tex")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert_eq!(text, "submitted");
    assert!(archive.by_name("README.md").is_ok());
    assert!(archive.by_name("manifest.json").is_ok());
    let exported_bytes = fs::read(&destination).unwrap();
    assert!(submissions::export(
        &f.store,
        &f.ws,
        &saved.id,
        "wrong",
        f._temp.path().join("other.zip").to_str().unwrap()
    )
    .is_err());
    assert!(submissions::export(
        &f.store,
        &f.ws,
        &saved.id,
        &saved.content_hash,
        destination.to_str().unwrap()
    )
    .is_err());
    assert_eq!(fs::read(&destination).unwrap(), exported_bytes);
}

#[test]
fn submission_refuses_changed_selection_and_traversal() {
    let f = fixture();
    fs::write(f.root.join("main.tex"), "later").unwrap();
    let mut request = submissions::SubmissionRequest {
        title: "Submission".into(),
        handoff: "".into(),
        files: BTreeMap::from([("main.tex".into(), hash(b"earlier"))]),
        revisions: vec![],
        records: vec![],
        executions: vec![],
        operation_id: "changed-snapshot".into(),
    };
    assert!(submissions::capture(&f.store, &f.ws, request.clone()).is_err());
    assert!(desk::records(&f.store, &f.ws, "submission")
        .unwrap()
        .is_empty());
    request.files = BTreeMap::from([("../outside.tex".into(), hash(b"earlier"))]);
    assert!(submissions::capture(&f.store, &f.ws, request).is_err());
}

#[test]
fn citation_checks_require_opt_in_ignore_outputs_and_deduplicate_inputs() {
    let f = fixture();
    fs::write(f.root.join("main.tex"), "\\cite{missing}").unwrap();
    fs::write(f.root.join("refs.bib"), "@article{known,title={Known}}").unwrap();
    let policy = checks::CheckPolicy {
        enabled: false,
        source_path: "main.tex".into(),
        bibliographies: vec!["refs.bib".into()],
    };
    let saved = checks::save_policy(&f.store, &f.ws, 0, policy.clone()).unwrap();
    assert!(checks::run(&f.store, &f.ws, Some(&["main.tex".into()]))
        .unwrap()
        .is_none());
    checks::save_policy(
        &f.store,
        &f.ws,
        saved.revision,
        checks::CheckPolicy {
            enabled: true,
            ..policy
        },
    )
    .unwrap();
    assert!(checks::run(&f.store, &f.ws, Some(&["main.pdf".into()]))
        .unwrap()
        .is_none());
    let first = checks::run(&f.store, &f.ws, Some(&["main.tex".into()]))
        .unwrap()
        .unwrap();
    assert_eq!(first.body["outcome"], "needs_attention");
    assert_eq!(
        checks::run(&f.store, &f.ws, Some(&["main.tex".into()]))
            .unwrap()
            .unwrap()
            .revision,
        first.revision
    );
    fs::write(f.root.join("refs.bib"), "@article{missing,title={Added}}").unwrap();
    let passed = checks::run(&f.store, &f.ws, Some(&["refs.bib".into()]))
        .unwrap()
        .unwrap();
    assert_eq!(passed.body["outcome"], "passed");
    assert_eq!(
        desk::records(&f.store, &f.ws, "check_receipt")
            .unwrap()
            .len(),
        2
    );
    let _ = home(&f.store, &f.ws).unwrap();
    assert_eq!(
        desk::records(&f.store, &f.ws, "check_receipt")
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn citation_index_preserves_bibliography_and_reports_duplicates_and_versions() {
    let f = fixture();
    let bib = "@article{k,title={Title},author={Author}}\n@book{k,title={Other}}";
    fs::write(f.root.join("refs.bib"), bib).unwrap();
    let result = citations::inspect(
        &f.store,
        &f.ws,
        citations::CitationRequest {
            bibliographies: vec!["refs.bib".into()],
            source_path: None,
            source_text: Some("% \\cite{ignored}\n\\cite{k,absent}".into()),
            checkpoint_id: None,
        },
    )
    .unwrap();
    assert_eq!(result["missing"], json!(["absent"]));
    assert_eq!(result["warnings"].as_array().unwrap().len(), 1);
    assert_eq!(result["versions"]["refs.bib"], hash(bib.as_bytes()));
    assert_eq!(fs::read_to_string(f.root.join("refs.bib")).unwrap(), bib);
}

#[test]
fn dropbox_is_one_transport_and_can_disconnect_when_folder_is_unavailable() {
    let f = fixture();
    let settings = connections::ConnectionSettings {
        provider: "dropbox".into(),
        remote_name: None,
        web_url: Some("https://www.dropbox.com/home/research".into()),
        active: true,
    };
    let connected = connections::save_connection(&f.store, &f.ws, 0, settings.clone()).unwrap();
    assert!(
        connections::check_transport_action(&f.store, &f.ws, &version_control::Action::Push)
            .is_err()
    );
    assert!(connections::save_connection(
        &f.store,
        &f.ws,
        connected.revision,
        connections::ConnectionSettings {
            provider: "git".into(),
            ..settings.clone()
        }
    )
    .is_err());
    fs::rename(&f.root, f.root.with_file_name("moved")).unwrap();
    let disconnected = connections::save_connection(
        &f.store,
        &f.ws,
        connected.revision,
        connections::ConnectionSettings {
            active: false,
            ..settings
        },
    )
    .unwrap();
    assert_eq!(disconnected.body["active"], false);
    assert_eq!(
        disconnected.body["rootIdentity"],
        connected.body["rootIdentity"]
    );
}

#[test]
fn github_comment_import_deduplicates_and_retains_source_identity() {
    let f = fixture();
    let source = desk::insert(&f.store, &f.ws, "github_collaboration", "Checked PR", json!({"repository":"owner/repo","pullRequests":[{"comments":[{"id":"comment-one","body":"Check the estimate","url":"https://github.com/owner/repo/pull/1#comment"}]}]}), None, "gh-test").unwrap();
    let task = github::import_comment(&f.store, &f.ws, &source.id, "comment-one").unwrap();
    assert_eq!(
        github::import_comment(&f.store, &f.ws, &source.id, "comment-one")
            .unwrap()
            .id,
        task.id
    );
    assert!(task.body["objective"]
        .as_str()
        .unwrap()
        .contains(&source.content_hash));
    assert_eq!(task.body["status"], "open");
}
