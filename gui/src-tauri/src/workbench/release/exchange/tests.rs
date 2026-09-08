use super::*;
use crate::workbench::project::{DocumentSelection, ProjectAction, ProjectMutation};
use crate::workbench::research::{CreateNoteRequest, ImportPaperRequest};

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
        .map(|i| archive.by_index(i).unwrap().name().to_string())
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
            let name = entry.name().to_string();
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
