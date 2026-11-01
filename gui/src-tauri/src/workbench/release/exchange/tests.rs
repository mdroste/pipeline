use super::*;
use crate::workbench::project::{self, DocumentSelection, ProjectAction, ProjectMutation};
use crate::workbench::research::{self, CreateNoteRequest, ImportPaperRequest};
use crate::workbench::store::CreateWorkspaceRequest;
use rusqlite::params;
use std::collections::BTreeSet;
use std::io::Write;
use zip::write::SimpleFileOptions;

fn fixture() -> (tempfile::TempDir, Store, String, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let store = Store::open_at(&temp.path().join("store")).unwrap();
    let ws = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Origin".into(),
            root: Some(root.to_string_lossy().into_owned()),
            operation_id: "create".into(),
        })
        .unwrap()
        .record
        .id;
    (temp, store, ws, root)
}
fn populate(store: &Store, ws: &str, root: &Path) -> (String, String, String) {
    fs::write(root.join("main.md"), "The effect is 0.13 log points.").unwrap();
    let paper = research::import_paper(
        store,
        ImportPaperRequest {
            workspace_id: ws.into(),
            paper_id: None,
            title: "Main".into(),
            role: "manuscript".into(),
            path: root.join("main.md").to_string_lossy().into_owned(),
            operation_id: "paper".into(),
        },
    )
    .unwrap();
    let revision = paper.revision.unwrap();
    let selection = DocumentSelection {
        revision_id: revision.id.clone(),
        revision_hash: revision.content_hash.clone(),
        start: Some(0),
        end: Some(30),
        page: None,
        region: None,
        quote: "The effect is 0.13 log points.".into(),
    };
    let anchor: project::ProjectRecord = serde_json::from_value(
        project::mutate(
            store,
            ProjectMutation {
                workspace_id: ws.into(),
                operation_id: "annotate".into(),
                action: ProjectAction::Annotate {
                    selection,
                    body: "Headline".into(),
                },
            },
        )
        .unwrap(),
    )
    .unwrap();
    let task: project::ProjectRecord = serde_json::from_value(
        project::mutate(
            store,
            ProjectMutation {
                workspace_id: ws.into(),
                operation_id: "task".into(),
                action: ProjectAction::CreateTask {
                    objective: "Check the headline".into(),
                    anchor_id: Some(anchor.id.clone()),
                    expected_outputs: vec![],
                    expected_checks: vec!["Recompute".into()],
                },
            },
        )
        .unwrap(),
    )
    .unwrap();
    let note = research::create_note(
        store,
        CreateNoteRequest {
            workspace_id: ws.into(),
            paper_id: None,
            kind: "decision".into(),
            body: "Use log points".into(),
            state: Some("accepted".into()),
            origin: "user".into(),
            pinned: true,
            operation_id: "note".into(),
        },
        false,
    )
    .unwrap();
    (task.id, anchor.id, note.id)
}
fn selection() -> ExchangeSelection {
    ExchangeSelection {
        include_notes: true,
        record_kinds: vec!["task".into()],
        ..Default::default()
    }
}

#[test]
fn export_closes_dependencies_and_excludes_private_state() {
    let (temp, store, ws, root) = fixture();
    let (task, anchor, note) = populate(&store, &ws, &root);
    store
        .create_session(crate::workbench::store::CreateSessionRequest {
            workspace_id: Some(ws.clone()),
            title: "Private chat".into(),
            operation_id: "session".into(),
        })
        .unwrap();
    let preview = preview_exchange(&store, &ws, &selection()).unwrap();
    let kinds: BTreeSet<_> = preview.objects.iter().map(|o| o.kind.as_str()).collect();
    assert_eq!(kinds, BTreeSet::from(["anchor", "note", "paper", "task"]));
    assert_eq!(preview.blob_count, 1);
    let path = temp.path().join("out").join("origin.pwex");
    let report = export_exchange(
        &store,
        ExportExchangeRequest {
            workspace_id: ws.clone(),
            path: path.to_string_lossy().into_owned(),
            selection: selection(),
        },
    )
    .unwrap();
    assert_eq!(report.object_count, 4);
    let mut archive = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();
    let names: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().unwrap().into_owned())
        .collect();
    assert!(names.iter().any(|n| n == "README.md"));
    assert!(names
        .iter()
        .any(|n| n == &format!("objects/task/{task}.json")));
    assert!(names
        .iter()
        .any(|n| n == &format!("objects/anchor/{anchor}.json")));
    assert!(names
        .iter()
        .any(|n| n == &format!("objects/note/{note}.json")));
    assert!(!names
        .iter()
        .any(|n| n.contains("transcript") || n.contains("session") || n.contains("codex")));
    let mut readme = String::new();
    archive
        .by_name("README.md")
        .unwrap()
        .read_to_string(&mut readme)
        .unwrap();
    assert!(readme.contains("Check the headline"));
    assert!(readme.contains("Conversations, transcripts"));
    let inspection = inspect_exchange(&store, &path.to_string_lossy()).unwrap();
    assert!(inspection.own_export);
    assert_eq!(inspection.counts["task"], 1);
    assert!(export_exchange(
        &store,
        ExportExchangeRequest {
            workspace_id: ws,
            path: store
                .root_path()
                .join("inside.pwex")
                .to_string_lossy()
                .into_owned(),
            selection: selection(),
        }
    )
    .is_err());
}

#[test]
fn import_into_nonempty_store_remaps_is_idempotent_and_records_conflicts() {
    let (temp, store, ws, root) = fixture();
    let (task, _anchor, note) = populate(&store, &ws, &root);
    let path = temp.path().join("origin.pwex");
    export_exchange(
        &store,
        ExportExchangeRequest {
            workspace_id: ws.clone(),
            path: path.to_string_lossy().into_owned(),
            selection: selection(),
        },
    )
    .unwrap();
    let target = ExchangeTarget::NewWorkspace {
        name: "Coauthor copy".into(),
        root: None,
    };
    let preview = preview_import(&store, &path.to_string_lossy(), &target).unwrap();
    assert_eq!(preview.remapped, 4);
    assert!(preview.conflicts.is_empty());
    let report = import_exchange(
        &store,
        ImportExchangeRequest {
            path: path.to_string_lossy().into_owned(),
            target: target.clone(),
            operation_id: "import-1".into(),
        },
    )
    .unwrap();
    assert_eq!((report.remapped, report.conflicts, report.blobs), (4, 0, 1));
    let imported_ws = report.workspace_id.clone();
    assert_ne!(imported_ws, ws);
    let tasks = project::records(&store, &imported_ws, "task").unwrap();
    assert_eq!(tasks.len(), 1);
    assert_ne!(tasks[0].id, task);
    let imported_anchor = tasks[0].body["anchorId"].as_str().unwrap().to_string();
    let anchor = project::record(&store, &imported_ws, &imported_anchor, "anchor").unwrap();
    let papers = research::list_papers(&store, &imported_ws).unwrap();
    assert_eq!(papers.len(), 1);
    assert_eq!(
        anchor.body["selection"]["revisionId"],
        papers[0].revision.as_ref().unwrap().id
    );
    let read = project::read_document(
        &store,
        &imported_ws,
        papers[0].revision.as_ref().unwrap().id.as_str(),
        0,
        None,
    )
    .unwrap();
    assert!(read.text.contains("0.13 log points"));
    let original_tasks = project::records(&store, &ws, "task").unwrap();
    assert_eq!(original_tasks[0].id, task);
    assert_eq!(
        store
            .list_sessions(Some(&imported_ws), true)
            .unwrap()
            .sessions
            .len(),
        0
    );

    let again = ExchangeTarget::ExistingWorkspace {
        workspace_id: imported_ws.clone(),
    };
    let preview = preview_import(&store, &path.to_string_lossy(), &again).unwrap();
    assert_eq!(
        (preview.identical, preview.new, preview.remapped),
        (4, 0, 0)
    );
    let report = import_exchange(
        &store,
        ImportExchangeRequest {
            path: path.to_string_lossy().into_owned(),
            target: again.clone(),
            operation_id: "import-2".into(),
        },
    )
    .unwrap();
    assert_eq!((report.identical, report.conflicts), (4, 0));

    let notes = research::list_notes(&store, &imported_ws, true).unwrap();
    assert_eq!(notes.len(), 1);
    assert_ne!(notes[0].id, note);
    research::update_note(
        &store,
        research::UpdateNoteRequest {
            note_id: notes[0].id.clone(),
            expected_revision: notes[0].revision,
            body: Some("Use percent instead".into()),
            state: None,
            pinned: None,
            operation_id: "edit".into(),
        },
    )
    .unwrap();
    let report = import_exchange(
        &store,
        ImportExchangeRequest {
            path: path.to_string_lossy().into_owned(),
            target: again,
            operation_id: "import-3".into(),
        },
    )
    .unwrap();
    assert_eq!(report.conflicts, 1);
    let notes = research::list_notes(&store, &imported_ws, true).unwrap();
    assert_eq!(notes[0].body, "Use percent instead");
    let conflicts = list_conflicts(&store, &imported_ws).unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].object_kind, "note");
    assert_eq!(conflicts[0].imported["body"], "Use log points");
    assert_eq!(
        conflicts[0].base.as_ref().unwrap()["body"],
        "Use log points"
    );
    assert_eq!(conflicts[0].local["body"], "Use percent instead");
    let resolved = resolve_conflict(&store, &imported_ws, &conflicts[0].id, true).unwrap();
    assert_eq!(resolved.state, "took_imported");
    let notes = research::list_notes(&store, &imported_ws, true).unwrap();
    assert_eq!(notes[0].body, "Use log points");
    assert!(resolve_conflict(&store, &imported_ws, &conflicts[0].id, true).is_err());
    let retained: i64 = store
        .connection()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM retained_blobs WHERE reference_type='exchange_import'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(retained >= 1);
}

#[test]
fn corrupt_and_foreign_packages_are_rejected() {
    let (temp, store, ws, root) = fixture();
    populate(&store, &ws, &root);
    let path = temp.path().join("origin.pwex");
    export_exchange(
        &store,
        ExportExchangeRequest {
            workspace_id: ws.clone(),
            path: path.to_string_lossy().into_owned(),
            selection: selection(),
        },
    )
    .unwrap();
    let tampered = temp.path().join("tampered.pwex");
    {
        let mut archive = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();
        let mut zip = zip::ZipWriter::new(File::create(&tampered).unwrap());
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).unwrap();
            let name = entry.name().unwrap().into_owned();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            if name.starts_with("blobs/") {
                bytes.push(b'!');
            }
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(&bytes).unwrap();
        }
        zip.finish().unwrap();
    }
    let error = inspect_exchange(&store, &tampered.to_string_lossy()).unwrap_err();
    assert!(error.message.contains("hash"));
    let other = temp.path().join("other.pwrx");
    fs::write(&other, b"not a package").unwrap();
    assert!(inspect_exchange(&store, &other.to_string_lossy()).is_err());
    assert!(preview_import(
        &store,
        &path.to_string_lossy(),
        &ExchangeTarget::ExistingWorkspace {
            workspace_id: "missing".into()
        }
    )
    .is_err());
}

#[test]
fn reimporting_a_ledger_package_is_identical_not_conflicting() {
    let (temp, store, ws, root) = fixture();
    populate(&store, &ws, &root);
    let revision = research::list_papers(&store, &ws).unwrap()[0]
        .revision
        .clone()
        .unwrap();
    let claim = research::propose_claim(
        &store,
        research::ProposeClaimRequest {
            workspace_id: ws.clone(),
            paper_id: None,
            claim: "The effect is 0.13 log points".into(),
            kind: "empirical".into(),
            origin: "user".into(),
            operation_id: "claim".into(),
        },
        false,
    )
    .unwrap();
    research::propose_evidence(
        &store,
        research::ProposeEvidenceRequest {
            workspace_id: ws.clone(),
            claim_version_id: claim.version_id.clone(),
            target_type: "paper_revision".into(),
            target_id: revision.id.clone(),
            locator: None,
            relation: "supports".into(),
            assessor: "user".into(),
            operation_id: "evidence".into(),
        },
        false,
    )
    .unwrap();
    let path = temp.path().join("ledger.pwex");
    export_exchange(
        &store,
        ExportExchangeRequest {
            workspace_id: ws.clone(),
            path: path.to_string_lossy().into_owned(),
            selection: ExchangeSelection {
                include_ledger: true,
                ..selection()
            },
        },
    )
    .unwrap();
    let target = ExchangeTarget::NewWorkspace {
        name: "Coauthor copy".into(),
        root: None,
    };
    let report = import_exchange(
        &store,
        ImportExchangeRequest {
            path: path.to_string_lossy().into_owned(),
            target,
            operation_id: "import-ledger-1".into(),
        },
    )
    .unwrap();
    assert_eq!(report.conflicts, 0);
    let again = ExchangeTarget::ExistingWorkspace {
        workspace_id: report.workspace_id.clone(),
    };
    let preview = preview_import(&store, &path.to_string_lossy(), &again).unwrap();
    assert!(
        preview.conflicts.is_empty(),
        "{:?}",
        preview
            .conflicts
            .iter()
            .map(|c| (&c.kind, &c.outcome))
            .collect::<Vec<_>>()
    );
    assert!(preview
        .decisions
        .iter()
        .any(|d| d.kind == "evidence" && d.outcome == "identical"));
    let report = import_exchange(
        &store,
        ImportExchangeRequest {
            path: path.to_string_lossy().into_owned(),
            target: again,
            operation_id: "import-ledger-2".into(),
        },
    )
    .unwrap();
    assert_eq!(report.conflicts, 0);
    assert!(list_conflicts(&store, &report.workspace_id)
        .unwrap()
        .is_empty());
}

#[test]
fn retired_claims_and_large_note_sets_survive_export_and_reimport() {
    let (temp, store, ws, root) = fixture();
    populate(&store, &ws, &root);
    for index in 0..520 {
        research::create_note(
            &store,
            CreateNoteRequest {
                workspace_id: ws.clone(),
                paper_id: None,
                kind: "decision".into(),
                body: format!("Observation {index}"),
                state: None,
                origin: "user".into(),
                pinned: false,
                operation_id: format!("bulk-note-{index}"),
            },
            false,
        )
        .unwrap();
    }
    let claim = research::propose_claim(
        &store,
        research::ProposeClaimRequest {
            workspace_id: ws.clone(),
            paper_id: None,
            claim: "Welfare rises with trade".into(),
            kind: "empirical".into(),
            origin: "user".into(),
            operation_id: "claim-bulk".into(),
        },
        false,
    )
    .unwrap();
    let selection = ExchangeSelection {
        include_ledger: true,
        ..selection()
    };
    // Every selected object is packaged, not only the first page of a list view.
    let preview = preview_exchange(&store, &ws, &selection).unwrap();
    assert!(
        preview.external_references.is_empty(),
        "{:?}",
        preview.external_references
    );
    assert_eq!(
        preview.objects.iter().filter(|o| o.kind == "note").count(),
        521
    );
    let path = temp.path().join("bulk.pwex");
    export_exchange(
        &store,
        ExportExchangeRequest {
            workspace_id: ws.clone(),
            path: path.to_string_lossy().into_owned(),
            selection,
        },
    )
    .unwrap();
    research::set_claim_state(
        &store,
        research::SetClaimStateRequest {
            claim_id: claim.id.clone(),
            state: "retired".into(),
            operation_id: "retire".into(),
        },
    )
    .unwrap();
    // Re-importing into the same Workspace records a conflict for the retired
    // claim instead of aborting on its primary key.
    let report = import_exchange(
        &store,
        ImportExchangeRequest {
            path: path.to_string_lossy().into_owned(),
            target: ExchangeTarget::ExistingWorkspace {
                workspace_id: ws.clone(),
            },
            operation_id: "reimport-retired".into(),
        },
    )
    .unwrap();
    assert_eq!(report.conflicts, 1);
    let conflicts = list_conflicts(&store, &ws).unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].object_kind, "claim");
}

fn export_to(store: &Store, ws: &str, path: &Path, selection: ExchangeSelection) {
    export_exchange(
        store,
        ExportExchangeRequest {
            workspace_id: ws.into(),
            path: path.to_string_lossy().into_owned(),
            selection,
        },
    )
    .unwrap();
}
fn import_new(store: &Store, path: &Path, operation: &str) -> ImportReport {
    import_exchange(
        store,
        ImportExchangeRequest {
            path: path.to_string_lossy().into_owned(),
            target: ExchangeTarget::NewWorkspace {
                name: "Coauthor copy".into(),
                root: None,
            },
            operation_id: operation.into(),
        },
    )
    .unwrap()
}
fn import_into(store: &Store, path: &Path, ws: &str, operation: &str) -> ImportReport {
    import_exchange(
        store,
        ImportExchangeRequest {
            path: path.to_string_lossy().into_owned(),
            target: ExchangeTarget::ExistingWorkspace {
                workspace_id: ws.into(),
            },
            operation_id: operation.into(),
        },
    )
    .unwrap()
}

/// A sender's store with an accepted note and claim, a confirmed link with a
/// passing check, and a completed run.
fn decided_package(temp: &tempfile::TempDir, store: &Store, ws: &str, root: &Path) -> PathBuf {
    populate(store, ws, root);
    let claim = research::propose_claim(
        store,
        research::ProposeClaimRequest {
            workspace_id: ws.into(),
            paper_id: None,
            claim: "The effect is 0.13 log points".into(),
            kind: "empirical".into(),
            origin: "user".into(),
            operation_id: "claim".into(),
        },
        false,
    )
    .unwrap();
    research::set_claim_state(
        store,
        research::SetClaimStateRequest {
            claim_id: claim.id.clone(),
            state: "accepted".into(),
            operation_id: "accept".into(),
        },
    )
    .unwrap();
    let connection = store.connection().unwrap();
    connection.execute("INSERT INTO research_executions (id, workspace_id, adapter, command_json, cwd, input_manifest_json, dependency_hash, outcome, output_manifest_json, validation_json, snapshot_consistency, created_at) VALUES ('execution_sent', ?1, 'command', '[]', '/sender', '{}', 'hash', 'completed', '{\"artifacts\":[]}', '{}', 'complete', 't')", [ws]).unwrap();
    let evidence = research::propose_evidence(
        store,
        research::ProposeEvidenceRequest {
            workspace_id: ws.into(),
            claim_version_id: claim.version_id,
            target_type: "execution".into(),
            target_id: "execution_sent".into(),
            locator: None,
            relation: "supports".into(),
            assessor: "user".into(),
            operation_id: "evidence".into(),
        },
        false,
    )
    .unwrap();
    research::confirm_evidence(
        store,
        research::ConfirmEvidenceRequest {
            evidence_id: evidence.id.clone(),
            operation_id: "confirm".into(),
        },
    )
    .unwrap();
    connection.execute("INSERT INTO verification_records (id, evidence_link_id, method, checker_identity, input_hashes_json, observed_result_json, limitations, passed, created_at) VALUES ('verification_sent', ?1, 'comparison', 'sender', '[]', '{\"passed\":true}', 'none', 1, 't')", [&evidence.id]).unwrap();
    let path = temp.path().join("decided.pwex");
    export_to(
        store,
        ws,
        &path,
        ExchangeSelection {
            include_ledger: true,
            ..selection()
        },
    );
    path
}

#[test]
fn a_senders_decisions_arrive_as_proposals_with_a_visible_marker() {
    let (temp, store, ws, root) = fixture();
    let path = decided_package(&temp, &store, &ws, &root);
    let imported = import_new(&store, &path, "import-1").workspace_id;

    let notes = research::list_notes(&store, &imported, true).unwrap();
    assert_eq!(notes[0].state, "proposed");
    assert_eq!(notes[0].imported_status.as_deref(), Some("accepted"));
    let session = store
        .create_session(crate::workbench::store::CreateSessionRequest {
            workspace_id: Some(imported.clone()),
            title: "Recipient".into(),
            operation_id: "session".into(),
        })
        .unwrap()
        .record;
    let session = store
        .update_session(crate::workbench::store::UpdateSessionRequest {
            session_id: session.id,
            expected_revision: session.revision,
            operation_id: "preset".into(),
            title: None,
            draft: None,
            overrides: None,
            archived: None,
            preset_id: Some("research_assistant".into()),
            paper_id: None,
            clear_paper: None,
        })
        .unwrap()
        .record
        .id;
    let context = research::resolve_harness(&store, &session).unwrap();
    assert!(!context.context_preview.contains("Use log points"));

    let ledger = research::research_ledger(&store, &imported).unwrap();
    assert_eq!(ledger.claims[0].workflow_state, "proposed");
    assert_eq!(
        ledger.claims[0].imported_status.as_deref(),
        Some("accepted")
    );
    assert_eq!(ledger.evidence[0].assessment, "not_checked");
    assert_eq!(
        ledger.evidence[0].imported_status.as_deref(),
        Some("human_confirmed")
    );
    let connection = store.connection().unwrap();
    let (validation, observed): (String, String) = connection
        .query_row(
            "SELECT e.validation_json,v.observed_result_json FROM research_executions e, verification_records v WHERE e.workspace_id=?1 AND v.evidence_link_id=?2",
            params![imported, ledger.evidence[0].id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(validation.contains("importedFrom"));
    assert!(observed.contains("importedFrom"));

    // Holding the sender's status back is not a change to the content.
    let again = import_into(&store, &path, &imported, "import-2");
    assert_eq!((again.conflicts, again.new), (0, 0));

    // The recipient's own decisions are theirs, and still match the sender's.
    research::update_note(
        &store,
        research::UpdateNoteRequest {
            note_id: notes[0].id.clone(),
            expected_revision: notes[0].revision,
            body: None,
            state: Some("accepted".into()),
            pinned: None,
            operation_id: "own-acceptance".into(),
        },
    )
    .unwrap();
    let confirmed = research::confirm_evidence(
        &store,
        research::ConfirmEvidenceRequest {
            evidence_id: ledger.evidence[0].id.clone(),
            operation_id: "own-confirmation".into(),
        },
    )
    .unwrap();
    assert_eq!(confirmed.imported_status, None);
    assert_eq!(
        import_into(&store, &path, &imported, "import-3").conflicts,
        0
    );
    let context = research::resolve_harness(&store, &session).unwrap();
    assert!(context.context_preview.contains("Use log points"));
}

#[test]
fn taking_an_imported_note_does_not_accept_it_for_the_recipient() {
    let (temp, store, ws, root) = fixture();
    let path = decided_package(&temp, &store, &ws, &root);
    let imported = import_new(&store, &path, "import-1").workspace_id;
    let note = research::list_notes(&store, &imported, true)
        .unwrap()
        .remove(0);
    research::update_note(
        &store,
        research::UpdateNoteRequest {
            note_id: note.id.clone(),
            expected_revision: note.revision,
            body: Some("Use percent".into()),
            state: Some("accepted".into()),
            pinned: None,
            operation_id: "edit".into(),
        },
    )
    .unwrap();
    assert_eq!(
        import_into(&store, &path, &imported, "import-2").conflicts,
        1
    );
    let conflict = list_conflicts(&store, &imported).unwrap().remove(0);
    resolve_conflict(&store, &imported, &conflict.id, true).unwrap();
    let note = research::list_notes(&store, &imported, true)
        .unwrap()
        .remove(0);
    assert_eq!(note.body, "Use log points");
    assert_eq!(note.state, "proposed");
    assert_eq!(note.imported_status.as_deref(), Some("accepted"));
}

const ONE_PAGE_PDF: &[u8] = b"%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]>>endobj\ntrailer<</Root 1 0 R>>\n";

fn pdf_paper(store: &Store, ws: &str, root: &Path) -> research::PaperWithRevision {
    fs::write(root.join("paper.pdf"), ONE_PAGE_PDF).unwrap();
    research::import_paper(
        store,
        ImportPaperRequest {
            workspace_id: ws.into(),
            paper_id: None,
            title: "Draft".into(),
            role: "manuscript".into(),
            path: root.join("paper.pdf").to_string_lossy().into_owned(),
            operation_id: "pdf".into(),
        },
    )
    .unwrap()
}

#[test]
fn a_package_with_paper_files_opens_as_a_paper_on_the_other_side() {
    let (temp, store, ws, root) = fixture();
    let paper = pdf_paper(&store, &ws, &root);
    let chosen = ExchangeSelection {
        paper_ids: vec![paper.paper.id.clone()],
        include_compiled_pdfs: true,
        ..Default::default()
    };
    let preview = preview_exchange(&store, &ws, &chosen).unwrap();
    assert!(preview.external_references.is_empty());
    assert!(!preview
        .limitations
        .iter()
        .any(|l| l.contains("Paper files")));
    let path = temp.path().join("papers.pwex");
    export_to(&store, &ws, &path, chosen);

    let imported = import_new(&store, &path, "import").workspace_id;
    let copy = research::list_papers(&store, &imported).unwrap().remove(0);
    let revision = copy.revision.unwrap();
    let (kind, reference, origin): (String, String, String) = store
        .connection()
        .unwrap()
        .query_row(
            "SELECT media_kind,storage_reference,origin FROM artifacts WHERE workspace_id=?1 AND content_hash=?2",
            params![imported, revision.content_hash],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((kind.as_str(), origin.as_str()), ("pdf", "exchange_import"));
    assert_eq!(fs::read(reference).unwrap(), ONE_PAGE_PDF);
    let handoff = crate::workbench::release::prepare_review_handoff(
        &store,
        crate::workbench::release::PrepareReviewHandoffRequest {
            workspace_id: imported.clone(),
            session_id: None,
            paper_id: copy.paper.id,
            metadata: serde_json::json!({}),
            operation_id: "handoff".into(),
        },
    );
    assert!(handoff.is_ok(), "{:?}", handoff.err());
    if std::process::Command::new("pdftoppm")
        .arg("-v")
        .output()
        .is_ok()
    {
        let page = research::paper_page_image(&store, &imported, &revision.id, 1).unwrap();
        assert!(page.to_string().contains("base64,"));
    }
}

#[test]
fn leaving_paper_files_out_is_stated_and_an_oversized_one_refuses_the_export() {
    let (temp, store, ws, root) = fixture();
    let paper = pdf_paper(&store, &ws, &root);
    let without = ExchangeSelection {
        paper_ids: vec![paper.paper.id.clone()],
        ..Default::default()
    };
    let preview = preview_exchange(&store, &ws, &without).unwrap();
    assert!(preview
        .limitations
        .iter()
        .any(|l| l.contains("Paper files")));

    store
        .connection()
        .unwrap()
        .execute(
            "UPDATE artifacts SET size_bytes=?1 WHERE workspace_id=?2",
            params![65 * 1024 * 1024, ws],
        )
        .unwrap();
    let with = ExchangeSelection {
        include_compiled_pdfs: true,
        ..without
    };
    assert!(!preview_exchange(&store, &ws, &with)
        .unwrap()
        .external_references
        .is_empty());
    let target = temp.path().join("too-big.pwex");
    let error = export_exchange(
        &store,
        ExportExchangeRequest {
            workspace_id: ws.clone(),
            path: target.to_string_lossy().into_owned(),
            selection: with,
        },
    )
    .unwrap_err();
    assert!(error.message.contains("Draft"), "{}", error.message);
    assert!(!target.exists());
}

fn bulk_sources(store: &Store, ws: &str, count: usize) {
    let mut connection = store.connection().unwrap();
    let tx = connection.transaction().unwrap();
    for index in 0..count {
        tx.execute("INSERT INTO sources(id,workspace_id,title,identifiers_json,created_at,updated_at) VALUES(?1,?2,?3,'{}','t',?4)", params![format!("source_{index}"), ws, format!("Source {index}"), format!("{index:06}")]).unwrap();
        tx.execute("INSERT INTO source_versions(id,source_id,access_state,acquired_via,created_at) VALUES(?1,?2,'metadata','bibtex','t')", params![format!("version_{index}"), format!("source_{index}")]).unwrap();
    }
    tx.commit().unwrap();
}

#[test]
fn every_selected_source_and_paper_is_packaged_past_the_list_limit() {
    let (temp, store, ws, _root) = fixture();
    bulk_sources(&store, &ws, 501);
    {
        let mut connection = store.connection().unwrap();
        let tx = connection.transaction().unwrap();
        for index in 0..501 {
            tx.execute("INSERT INTO papers(id,workspace_id,title,role,created_at,updated_at) VALUES(?1,?2,?3,'other','t',?4)", params![format!("paper_{index}"), ws, format!("Paper {index}"), format!("{index:06}")]).unwrap();
        }
        tx.commit().unwrap();
    }
    let chosen = ExchangeSelection {
        include_sources: true,
        paper_ids: (0..501).map(|index| format!("paper_{index}")).collect(),
        ..Default::default()
    };
    let preview = preview_exchange(&store, &ws, &chosen).unwrap();
    let count = |kind: &str| preview.objects.iter().filter(|o| o.kind == kind).count();
    assert_eq!((count("source"), count("paper")), (501, 501));
    assert!(
        preview.external_references.is_empty(),
        "{:?}",
        preview.external_references
    );
    let path = temp.path().join("bulk.pwex");
    export_to(&store, &ws, &path, chosen);
    // The oldest rows compare as themselves, not as unreadable conflicts.
    let report = import_into(&store, &path, &ws, "reimport");
    assert_eq!((report.identical, report.conflicts), (1002, 0));
}

#[test]
fn a_selection_over_the_object_limit_is_refused_by_name() {
    let (_temp, store, ws, _root) = fixture();
    bulk_sources(&store, &ws, 5001);
    let error = preview_exchange(
        &store,
        &ws,
        &ExchangeSelection {
            include_sources: true,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(error.message.contains("source 5001"), "{}", error.message);
}

/// Copies a package, passing every entry through `change`, which may rename it.
fn rewritten(
    path: &Path,
    out: &Path,
    change: impl Fn(String, Vec<u8>) -> (String, Vec<u8>),
) -> PathBuf {
    let mut archive = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
    let mut zip = zip::ZipWriter::new(File::create(out).unwrap());
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let name = entry.name().unwrap().into_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        let (name, bytes) = change(name, bytes);
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();
    out.to_path_buf()
}

#[test]
fn a_blob_is_stored_under_its_own_hash_whatever_the_package_calls_it() {
    let (temp, store, ws, root) = fixture();
    populate(&store, &ws, &root);
    let path = temp.path().join("origin.pwex");
    export_to(&store, &ws, &path, selection());
    // The text of a paper the recipient may import later, under a name the
    // package's own bytes don't match.
    let known = "a".repeat(64);
    let blob = zip::ZipArchive::new(File::open(&path).unwrap())
        .unwrap()
        .file_names()
        .map(|name| name.unwrap().into_owned())
        .find(|name| name.starts_with("blobs/") && name.ends_with(".txt"))
        .unwrap();
    let real = blob
        .trim_start_matches("blobs/")
        .trim_end_matches(".txt")
        .to_string();
    let renamed = format!("blobs/{known}.txt");
    let planted = rewritten(&path, &temp.path().join("planted.pwex"), |name, bytes| {
        if name == blob {
            return (renamed.clone(), bytes);
        }
        let text = String::from_utf8_lossy(&bytes).replace(&blob, &renamed);
        (name, text.into_bytes())
    });
    let other = temp.path().join("other-store");
    let recipient = Store::open_at(&other).unwrap();
    import_new(&recipient, &planted, "import");
    assert!(!other.join("blobs").join(format!("{known}.txt")).exists());
    assert!(other.join("blobs").join(format!("{real}.txt")).exists());
    let reference: String = recipient
        .connection()
        .unwrap()
        .query_row("SELECT text_reference FROM paper_revisions", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(reference.ends_with(&format!("{real}.txt")), "{reference}");
}

#[test]
fn a_failed_import_into_a_new_project_leaves_no_project() {
    let (temp, store, ws, root) = fixture();
    populate(&store, &ws, &root);
    let path = temp.path().join("origin.pwex");
    export_to(&store, &ws, &path, selection());
    // This record passes planning and stops the import inside its transaction.
    let broken = rewritten(&path, &temp.path().join("broken.pwex"), |name, bytes| {
        if !name.starts_with("objects/task/") {
            return (name, bytes);
        }
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["body"] = Value::Null;
        (name, value.to_string().into_bytes())
    });
    let other = temp.path().join("other-store");
    let recipient = Store::open_at(&other).unwrap();
    let import = |path: &Path| {
        import_exchange(
            &recipient,
            ImportExchangeRequest {
                path: path.to_string_lossy().into_owned(),
                target: ExchangeTarget::NewWorkspace {
                    name: "Coauthor copy".into(),
                    root: None,
                },
                operation_id: "import".into(),
            },
        )
    };
    let error = import(&broken).unwrap_err();
    assert!(error.message.contains("has no body"), "{}", error.message);
    assert!(recipient
        .list_workspaces(true)
        .unwrap()
        .workspaces
        .is_empty());
    // The refused attempt used up nothing: the same request can run again.
    let report = import(&path).unwrap();
    let projects = recipient.list_workspaces(true).unwrap().workspaces;
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].id, report.workspace_id);
}

/// A note edited here after importing `path`, so importing it again conflicts.
fn conflicting_note(store: &Store, path: &Path) -> (String, String) {
    let imported = import_new(store, path, "import-1").workspace_id;
    let note = research::list_notes(store, &imported, true)
        .unwrap()
        .remove(0);
    research::update_note(
        store,
        research::UpdateNoteRequest {
            note_id: note.id.clone(),
            expected_revision: note.revision,
            body: Some("Use percent".into()),
            state: None,
            pinned: None,
            operation_id: "edit".into(),
        },
    )
    .unwrap();
    (imported, note.id)
}

#[test]
fn an_old_open_conflict_stays_listed_and_can_be_resolved() {
    let (temp, store, ws, root) = fixture();
    let path = decided_package(&temp, &store, &ws, &root);
    let (imported, _) = conflicting_note(&store, &path);
    import_into(&store, &path, &imported, "import-2");
    let open = list_conflicts(&store, &imported).unwrap().remove(0);
    let conn = store.connection().unwrap();
    conn.execute(
        "UPDATE exchange_conflicts SET recorded_at='2000-01-01' WHERE id=?1",
        [&open.id],
    )
    .unwrap();
    for n in 0..501 {
        conn.execute(
            "INSERT INTO exchange_conflicts(id,import_id,workspace_id,object_kind,object_id,local_json,imported_json,state,recorded_at) SELECT ?1,import_id,workspace_id,object_kind,object_id,local_json,imported_json,'kept_local','2030-01-01' FROM exchange_conflicts WHERE id=?2",
            params![format!("settled-{n}"), open.id],
        )
        .unwrap();
    }
    drop(conn);
    let listed = list_conflicts(&store, &imported).unwrap();
    assert_eq!(listed.len(), 500);
    assert_eq!(listed[0].id, open.id);
    resolve_conflict(&store, &imported, &open.id, false).unwrap();
    // A settled conflict past the list can still be read back by its ID.
    assert!(!list_conflicts(&store, &imported)
        .unwrap()
        .iter()
        .any(|c| c.id == "settled-0" && c.state == "open"));
    let error = resolve_conflict(&store, &imported, "settled-0", false).unwrap_err();
    assert!(error.message.contains("already resolved"), "{error:?}");
}

#[test]
fn taking_an_imported_copy_refuses_one_with_no_content() {
    let (temp, store, ws, root) = fixture();
    let path = decided_package(&temp, &store, &ws, &root);
    let (imported, note) = conflicting_note(&store, &path);
    let emptied = rewritten(&path, &temp.path().join("empty.pwex"), |name, bytes| {
        if !name.starts_with("objects/note/") {
            return (name, bytes);
        }
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value.as_object_mut().unwrap().remove("body");
        (name, value.to_string().into_bytes())
    });
    import_into(&store, &emptied, &imported, "import-2");
    let conflict = list_conflicts(&store, &imported).unwrap().remove(0);
    let error = resolve_conflict(&store, &imported, &conflict.id, true).unwrap_err();
    assert!(error.message.contains("Imported note"), "{error:?}");
    let body: String = store
        .connection()
        .unwrap()
        .query_row(
            "SELECT body FROM research_notes WHERE id=?1",
            [&note],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(body, "Use percent");
    assert_eq!(list_conflicts(&store, &imported).unwrap()[0].state, "open");
    // A record with no body must not write `null` over the local one.
    let c = store.connection().unwrap();
    c.execute(
        "UPDATE exchange_conflicts SET object_kind='task',object_id=(SELECT id FROM project_records WHERE workspace_id=?2 AND kind='task' LIMIT 1) WHERE id=?1",
        params![conflict.id, imported],
    )
    .unwrap();
    drop(c);
    let error = resolve_conflict(&store, &imported, &conflict.id, true).unwrap_err();
    assert!(error.message.contains("no content"), "{error:?}");
}

#[test]
fn a_decision_to_keep_the_local_copy_is_not_asked_again() {
    let (temp, store, ws, root) = fixture();
    let path = decided_package(&temp, &store, &ws, &root);
    let (imported, note) = conflicting_note(&store, &path);
    assert_eq!(
        import_into(&store, &path, &imported, "import-2").conflicts,
        1
    );
    let conflict = list_conflicts(&store, &imported).unwrap().remove(0);
    resolve_conflict(&store, &imported, &conflict.id, false).unwrap();
    assert_eq!(
        import_into(&store, &path, &imported, "import-3").conflicts,
        0
    );
    assert_eq!(list_conflicts(&store, &imported).unwrap().len(), 1);
    // A new local edit is a new question.
    let current = research::list_notes(&store, &imported, true)
        .unwrap()
        .into_iter()
        .find(|n| n.id == note)
        .unwrap();
    research::update_note(
        &store,
        research::UpdateNoteRequest {
            note_id: note,
            expected_revision: current.revision,
            body: Some("Use basis points".into()),
            state: None,
            pinned: None,
            operation_id: "edit-again".into(),
        },
    )
    .unwrap();
    assert_eq!(
        import_into(&store, &path, &imported, "import-4").conflicts,
        1
    );
}
