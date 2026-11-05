//! A research data folder that moves keeps working, and a move that can't be
//! repaired never lets a file be called unused.

use super::{
    export_archive, export_archive_without, import_archive, inspect_archive, missing_backup_files,
    refresh_retained_blobs, ArchiveReport, ExportArchiveRequest, ImportArchiveRequest,
};
use crate::workbench::data::{self, DataAcquisition, ImportDataset};
use crate::workbench::project::{preview_snapshot, preview_working_file};
use crate::workbench::release::{
    empty_trash, list_trash, prepare_review_handoff, prune_storage, restore_trash, storage_report,
    PrepareReviewHandoffRequest, PruneRequest, StorageReport,
};
use crate::workbench::research::{
    import_paper, import_source, paper_read, source_read, ImportPaperRequest, ImportSourceRequest,
    PaperReadRequest,
};
use crate::workbench::store::{
    CreateSessionRequest, CreateWorkspaceRequest, Store, WorkbenchResult, REFERENCE_COLUMNS,
};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

struct Fixture {
    temp: tempfile::TempDir,
    store: Store,
    workspace: String,
    session: String,
    revision: String,
    source_version: String,
    snapshot: String,
    dataset: String,
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    fs::create_dir(&project).unwrap();
    let store = Store::open_at(&temp.path().join("original/store")).unwrap();
    let workspace = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Research".into(),
            root: Some(project.to_string_lossy().into_owned()),
            operation_id: "workspace".into(),
        })
        .unwrap()
        .record
        .id;
    let session = store
        .create_session(CreateSessionRequest {
            workspace_id: Some(workspace.clone()),
            title: "Conversation".into(),
            operation_id: "session".into(),
        })
        .unwrap()
        .record
        .id;
    fs::write(
        project.join("paper.tex"),
        "Output rises with productivity.\n",
    )
    .unwrap();
    let revision = import_paper(
        &store,
        ImportPaperRequest {
            workspace_id: workspace.clone(),
            paper_id: None,
            title: "Paper".into(),
            role: "manuscript".into(),
            path: project.join("paper.tex").to_string_lossy().into_owned(),
            operation_id: "paper".into(),
        },
    )
    .unwrap()
    .revision
    .unwrap()
    .id;
    fs::write(project.join("source.txt"), "A cited passage.\n").unwrap();
    let source_version = import_source(
        &store,
        ImportSourceRequest {
            workspace_id: workspace.clone(),
            title: "Source".into(),
            citation_key: None,
            identifiers: json!({}),
            version_label: None,
            path: Some(project.join("source.txt").to_string_lossy().into_owned()),
            locator: None,
            access_state: "full".into(),
            acquired_via: "local_file".into(),
            operation_id: "source".into(),
        },
    )
    .unwrap()
    .source
    .version_id;
    fs::write(project.join("notes.md"), "working notes").unwrap();
    let snapshot = preview_working_file(&store, &workspace, "notes.md")
        .unwrap()
        .hash;
    fs::write(project.join("survey.csv"), "id,income\n1,10\n2,20\n").unwrap();
    let dataset = data::import(
        &store,
        ImportDataset {
            workspace_id: workspace.clone(),
            title: "Survey".into(),
            path: project.join("survey.csv").to_string_lossy().into_owned(),
            acquisition: DataAcquisition {
                provider: "local import".into(),
                source: "fixture".into(),
                retrieved_at: "t".into(),
                requested_vintage: None,
                returned_vintage: None,
                series_ids: vec![],
                units: None,
                frequency: None,
                transformation: None,
            },
            supersedes: None,
            operation_id: "dataset".into(),
            sheet: None,
        },
    )
    .unwrap()
    .id;
    let context = store.root_path().join("context/preview.txt");
    fs::write(&context, "conversation context").unwrap();
    store
        .connection()
        .unwrap()
        .execute(
            "INSERT INTO context_snapshots(id,session_id,schema_version,manifest_json,body_reference,created_at) VALUES('context',?1,1,'{}',?2,'t')",
            params![session, context.to_string_lossy()],
        )
        .unwrap();
    refresh_retained_blobs(&store).unwrap();
    Fixture {
        temp,
        store,
        workspace,
        session,
        revision,
        source_version,
        snapshot,
        dataset,
    }
}

impl Fixture {
    fn moved_root(&self) -> PathBuf {
        self.temp.path().join("new drive/research data")
    }
    /// Moves the folder as a researcher would and leaves it closed.
    fn relocate(&self) {
        fs::create_dir_all(self.moved_root().parent().unwrap()).unwrap();
        fs::rename(self.store.root_path(), self.moved_root()).unwrap();
    }
    fn reopen(&self) -> Store {
        Store::open_at(&self.moved_root()).unwrap()
    }
    fn read_paper(&self, store: &Store) -> WorkbenchResult<String> {
        paper_read(
            store,
            PaperReadRequest {
                workspace_id: self.workspace.clone(),
                revision_id: self.revision.clone(),
                start: None,
                length: None,
            },
        )
        .map(|read| read.text)
    }
    fn assert_everything_reads(&self, store: &Store) {
        assert!(self.read_paper(store).unwrap().contains("productivity"));
        let source = source_read(store, &self.workspace, &self.source_version, 0, 100).unwrap();
        assert!(source.text.contains("cited passage"));
        let snapshot = preview_snapshot(store, &self.workspace, &self.snapshot).unwrap();
        assert_eq!(snapshot.text.as_deref(), Some("working notes"));
        let rows = data::preview_rows(store, &self.workspace, &self.dataset, 0, false).unwrap();
        assert_eq!(rows.total_rows, 2);
    }
}

fn references(store: &Store) -> Vec<String> {
    let connection = store.connection().unwrap();
    let mut all = Vec::new();
    for (table, column) in REFERENCE_COLUMNS {
        let mut statement = connection
            .prepare(&format!(
                "SELECT {column} FROM {table} WHERE {column} IS NOT NULL ORDER BY rowid"
            ))
            .unwrap();
        all.extend(
            statement
                .query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .map(Result::unwrap),
        );
    }
    all
}

fn recorded_root(store: &Store) -> Option<String> {
    use rusqlite::OptionalExtension as _;
    store
        .connection()
        .unwrap()
        .query_row("SELECT root FROM store_location", [], |row| row.get(0))
        .optional()
        .unwrap()
}

fn backups(store: &Store) -> usize {
    fs::read_dir(store.root_path().join("backups"))
        .unwrap()
        .count()
}

fn category(report: &StorageReport, key: &str) -> Option<(usize, bool)> {
    report
        .categories
        .iter()
        .find(|category| category.key == key)
        .map(|category| (category.entries, category.disposable))
}

fn prune_preview(store: &Store, categories: &[&str]) -> WorkbenchResult<()> {
    prune_storage(
        store,
        PruneRequest {
            categories: categories.iter().map(|key| (*key).to_string()).collect(),
            apply: false,
            expected_preview_token: None,
        },
    )
    .map(|_| ())
}

fn prune_applied(store: &Store, categories: &[&str]) -> WorkbenchResult<()> {
    let categories = categories
        .iter()
        .map(|key| (*key).to_string())
        .collect::<Vec<_>>();
    let preview = prune_storage(
        store,
        PruneRequest {
            categories: categories.clone(),
            apply: false,
            expected_preview_token: None,
        },
    )?;
    prune_storage(
        store,
        PruneRequest {
            categories,
            apply: true,
            expected_preview_token: Some(preview.preview_token),
        },
    )
    .map(|_| ())
}

fn export(store: &Store, path: &Path) -> WorkbenchResult<ArchiveReport> {
    export_archive(
        store,
        ExportArchiveRequest {
            path: path.to_string_lossy().into_owned(),
        },
    )
}

fn restore(store: &Store, path: &Path, roots: &[String]) -> WorkbenchResult<ArchiveReport> {
    import_archive(
        store,
        ImportArchiveRequest {
            path: path.to_string_lossy().into_owned(),
            root_mappings: roots
                .iter()
                .map(|root| (root.clone(), None))
                .collect::<BTreeMap<_, _>>(),
        },
    )
}

fn assert_moved_store_is_whole(f: &Fixture, store: &Store) {
    f.assert_everything_reads(store);
    let report = storage_report(store).unwrap();
    assert_eq!(
        category(&report, "blobs_unreferenced").map(|c| c.0),
        Some(0)
    );
    assert_eq!(
        category(&report, "context_unreferenced").map(|c| c.0),
        Some(0)
    );
    assert!(category(&report, "blobs_referenced").unwrap().0 >= 4);
    assert!(category(&report, "context_referenced").unwrap().0 >= 1);
    assert!(category(&report, "blobs_unchecked").is_none());
    assert_eq!(
        recorded_root(store).as_deref(),
        Some(store.root_path().to_string_lossy().as_ref())
    );
    let root = store.root_path().to_string_lossy().into_owned();
    assert!(references(store).iter().all(|r| r.starts_with(&root)));

    let output = tempfile::tempdir().unwrap();
    let archive = output.path().join("backup.pwrx");
    export(store, &archive).unwrap();
    let destination = tempfile::tempdir().unwrap();
    let restored = Store::open_at(destination.path()).unwrap();
    let project = f.temp.path().join("project");
    restore(
        &restored,
        &archive,
        &[fs::canonicalize(project)
            .unwrap()
            .to_string_lossy()
            .into_owned()],
    )
    .unwrap();
    f.assert_everything_reads(&restored);
    let destination_root = destination.path().to_string_lossy().into_owned();
    assert!(references(&restored)
        .iter()
        .all(|r| r.starts_with(&destination_root)));
    assert_eq!(
        recorded_root(&restored).as_deref(),
        Some(destination_root.as_str())
    );
    let report = storage_report(&restored).unwrap();
    assert_eq!(
        category(&report, "blobs_unreferenced").map(|c| c.0),
        Some(0)
    );
}

#[test]
fn a_moved_data_folder_keeps_every_file_in_use_and_restorable() {
    let f = fixture();
    let before = references(&f.store);
    assert!(before.len() >= 8, "{before:?}");
    f.relocate();
    let store = f.reopen();
    assert_moved_store_is_whole(&f, &store);
    assert_eq!(references(&store).len(), before.len());

    // Rewriting references is the only thing a rebase does to the folder, and
    // the database it replaced stays available.
    let relative = |root: &Path| {
        let mut names = fs::read_dir(root.join("blobs"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        names.sort();
        names
    };
    assert_eq!(relative(store.root_path()).len(), 5);
    assert_eq!(backups(&store), 1);

    // A second open finds nothing to do.
    let after = references(&store);
    let reopened = f.reopen();
    assert_eq!(references(&reopened), after);
    assert_eq!(backups(&reopened), 1);
}

#[test]
fn a_folder_moved_before_locations_were_recorded_is_inferred_from_its_references() {
    let f = fixture();
    f.store
        .connection()
        .unwrap()
        .execute("DELETE FROM store_location", [])
        .unwrap();
    f.relocate();
    let store = f.reopen();
    assert_moved_store_is_whole(&f, &store);
}

#[test]
fn a_folder_moved_twice_while_closed_is_still_repaired() {
    let f = fixture();
    f.relocate();
    let elsewhere = f.temp.path().join("third/location");
    fs::create_dir_all(elsewhere.parent().unwrap()).unwrap();
    fs::rename(f.moved_root(), &elsewhere).unwrap();
    let store = Store::open_at(&elsewhere).unwrap();
    f.assert_everything_reads(&store);
    assert_eq!(
        recorded_root(&store).as_deref(),
        Some(elsewhere.to_string_lossy().as_ref())
    );
}

#[test]
fn one_unresolvable_reference_protects_every_research_file() {
    let f = fixture();
    let reference: String = f
        .store
        .connection()
        .unwrap()
        .query_row("SELECT text_reference FROM paper_revisions", [], |row| {
            row.get(0)
        })
        .unwrap();
    let name = Path::new(&reference).file_name().unwrap().to_owned();
    f.relocate();
    let aside = f.temp.path().join("aside");
    fs::rename(f.moved_root().join("blobs").join(&name), &aside).unwrap();
    // Both of these are offered for removal in a healthy store.
    fs::write(f.moved_root().join("blobs/orphan.txt"), "orphan").unwrap();
    fs::write(f.moved_root().join("context/orphan.txt"), "orphan").unwrap();

    let store = f.reopen();
    // Everything that could be rebased was, and the row that could not keeps
    // its old text so a later open can still repair it.
    assert!(source_read(&store, &f.workspace, &f.source_version, 0, 100).is_ok());
    assert_eq!(
        references(&store)
            .iter()
            .filter(|r| !r.starts_with(&*store.root_path().to_string_lossy()))
            .collect::<Vec<_>>(),
        [&reference]
    );
    assert!(recorded_root(&store).unwrap() != store.root_path().to_string_lossy());

    let report = storage_report(&store).unwrap();
    assert!(
        report
            .categories
            .iter()
            .filter(|c| c.key.starts_with("blobs_") || c.key.starts_with("context_"))
            .all(|c| !c.disposable),
        "{:?}",
        report.categories
    );
    assert_eq!(category(&report, "blobs_unreferenced"), Some((0, false)));
    assert_eq!(category(&report, "blobs_unchecked"), Some((1, false)));
    assert_eq!(category(&report, "context_unchecked"), Some((1, false)));
    assert!(
        report.note.contains("Cleanup of research files is off"),
        "{}",
        report.note
    );
    assert!(report.note.contains("may have moved"), "{}", report.note);
    for key in ["blobs_unreferenced", "context_unreferenced"] {
        let error = prune_preview(&store, &[key]).unwrap_err();
        assert!(
            error.message.contains("research files"),
            "{}",
            error.message
        );
    }
    assert!(store.root_path().join("blobs/orphan.txt").exists());

    // The message says what happened, and the same reference is not exported.
    let message = f.read_paper(&store).unwrap_err().message;
    assert!(message.contains("may have moved"), "{message}");
    assert!(!message.contains("escaped"), "{message}");
    let output = tempfile::tempdir().unwrap();
    let archive = output.path().join("backup.pwrx");
    let message = export(&store, &archive).unwrap_err().message;
    assert!(message.contains("No backup was saved"), "{message}");
    assert!(
        message.contains("Readable text of paper “Paper”"),
        "{message}"
    );
    assert!(!archive.exists());

    // Putting the file back repairs the store at the next open.
    fs::rename(&aside, store.root_path().join("blobs").join(&name)).unwrap();
    let store = f.reopen();
    f.assert_everything_reads(&store);
    assert_eq!(
        recorded_root(&store).as_deref(),
        Some(store.root_path().to_string_lossy().as_ref())
    );
    let report = storage_report(&store).unwrap();
    assert_eq!(category(&report, "blobs_unreferenced"), Some((1, true)));
    assert!(category(&report, "blobs_unchecked").is_none());
    export(&store, &archive).unwrap();
}

#[test]
fn trash_cannot_be_emptied_while_a_reference_is_unresolved() {
    let f = fixture();
    fs::write(f.store.root_path().join("blobs/orphan.txt"), "orphan").unwrap();
    let preview = prune_storage(
        &f.store,
        PruneRequest {
            categories: vec!["blobs_unreferenced".into()],
            apply: false,
            expected_preview_token: None,
        },
    )
    .unwrap();
    prune_storage(
        &f.store,
        PruneRequest {
            categories: vec!["blobs_unreferenced".into()],
            apply: true,
            expected_preview_token: Some(preview.preview_token),
        },
    )
    .unwrap();
    f.store
        .connection()
        .unwrap()
        .execute(
            "UPDATE paper_revisions SET text_reference='/elsewhere/blobs/missing.txt'",
            [],
        )
        .unwrap();
    let error = empty_trash(&f.store).unwrap_err();
    assert!(
        error.message.contains("research files"),
        "{}",
        error.message
    );
    assert_eq!(list_trash(&f.store).unwrap().len(), 1);
}

#[test]
fn a_failed_rebase_changes_nothing_and_the_store_still_opens() {
    let f = fixture();
    let before = references(&f.store);
    let recorded = recorded_root(&f.store);
    f.store
        .connection()
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER refuse_rebase BEFORE UPDATE OF body_reference ON context_snapshots BEGIN SELECT RAISE(ABORT,'refused'); END;",
        )
        .unwrap();
    f.relocate();

    let store = f.reopen();
    assert_eq!(references(&store), before);
    assert_eq!(recorded_root(&store), recorded);
    let report = storage_report(&store).unwrap();
    assert_eq!(category(&report, "blobs_unreferenced"), Some((0, false)));
    assert!(report.note.contains("research files"), "{}", report.note);

    store
        .connection()
        .unwrap()
        .execute_batch("DROP TRIGGER refuse_rebase;")
        .unwrap();
    let store = f.reopen();
    f.assert_everything_reads(&store);
}

#[test]
fn trash_moves_with_the_folder_and_rows_from_elsewhere_are_skipped() {
    let f = fixture();
    fs::write(f.store.root_path().join("blobs/orphan.txt"), "orphan").unwrap();
    let preview = prune_storage(
        &f.store,
        PruneRequest {
            categories: vec!["blobs_unreferenced".into()],
            apply: false,
            expected_preview_token: None,
        },
    )
    .unwrap();
    prune_storage(
        &f.store,
        PruneRequest {
            categories: vec!["blobs_unreferenced".into()],
            apply: true,
            expected_preview_token: Some(preview.preview_token),
        },
    )
    .unwrap();
    // Nothing on this computer backs this row, so it stays as it was.
    f.store
        .connection()
        .unwrap()
        .execute(
            "INSERT INTO storage_trash(id,category,original_path,trash_path,size_bytes,reason,moved_at,state) VALUES('foreign','context_unreferenced',?1,?2,4,'test','t','trashed')",
            params!["/elsewhere/context/a", "/elsewhere/trash/foreign/context/a"],
        )
        .unwrap();
    f.relocate();
    let store = f.reopen();

    let report = storage_report(&store).unwrap();
    assert_eq!(report.trash.len(), 1, "{:?}", report.trash);
    let entry = &report.trash[0];
    assert!(Path::new(&entry.original_path).starts_with(store.root_path()));
    restore_trash(&store, &entry.id).unwrap();
    assert_eq!(
        fs::read_to_string(store.root_path().join("blobs/orphan.txt")).unwrap(),
        "orphan"
    );
    assert!(list_trash(&store).unwrap().is_empty());
    assert_eq!(
        store
            .connection()
            .unwrap()
            .query_row(
                "SELECT original_path FROM storage_trash WHERE id='foreign'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "/elsewhere/context/a"
    );
}

fn column(store: &Store, sql: &str) -> Vec<Option<String>> {
    let connection = store.connection().unwrap();
    let mut statement = connection.prepare(sql).unwrap();
    let values = statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    values
}

fn project_root(f: &Fixture) -> String {
    fs::canonicalize(f.temp.path().join("project"))
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

fn manifest_of(path: &Path) -> Value {
    let mut archive = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    let entry = archive.by_name("manifest.json").unwrap();
    serde_json::from_reader(entry).unwrap()
}

/// The stored names of the paper's text and the dataset, read while the
/// fixture's store is still where it was created.
fn paper_text_and_dataset(f: &Fixture) -> Vec<String> {
    [
        "SELECT text_reference FROM paper_revisions",
        "SELECT storage_reference FROM artifacts WHERE origin='dataset'",
    ]
    .map(|sql| {
        let reference = column(&f.store, sql)[0].clone().unwrap();
        let name = Path::new(&reference).file_name().unwrap().to_string_lossy();
        name.into_owned()
    })
    .to_vec()
}

fn lose(root: &Path, names: &[String]) {
    for name in names {
        fs::remove_file(root.join("blobs").join(name)).unwrap();
    }
}

const LOST: [&str; 2] = ["Dataset “Survey”", "Readable text of paper “Paper”"];

#[test]
fn a_backup_can_leave_out_lost_files_and_everything_else_comes_back() {
    let f = fixture();
    let names = paper_text_and_dataset(&f);
    lose(f.store.root_path(), &names);
    let mut lost = names
        .iter()
        .map(|name| format!("workspace://blobs/{name}"))
        .collect::<Vec<_>>();
    lost.sort();
    let output = tempfile::tempdir().unwrap();
    let archive = output.path().join("backup.pwrx");

    // The ordinary path names what's affected and writes nothing.
    let message = export(&f.store, &archive).unwrap_err().message;
    for name in LOST {
        assert!(message.contains(name), "{message}");
    }
    assert!(message.contains("2 saved items"), "{message}");
    assert!(!message.contains("put"), "{message}");
    assert!(!archive.exists());

    let missing = missing_backup_files(&f.store).unwrap();
    assert_eq!(
        missing
            .iter()
            .map(|file| file.description.as_str())
            .collect::<Vec<_>>(),
        LOST
    );

    // A choice covers only the files it named.
    let one = [missing[0].reference.clone()];
    let request = ExportArchiveRequest {
        path: archive.to_string_lossy().into_owned(),
    };
    let message = export_archive_without(&f.store, request.clone(), &one)
        .unwrap_err()
        .message;
    assert!(message.contains(LOST[1]), "{message}");
    assert!(!message.contains(LOST[0]), "{message}");
    assert!(!archive.exists());

    let approved = missing
        .iter()
        .map(|file| file.reference.clone())
        .collect::<Vec<_>>();
    let saved = export_archive_without(&f.store, request.clone(), &approved).unwrap();
    assert_eq!(saved.omitted_files, LOST);
    assert_eq!(saved.blob_count, 3);
    assert_eq!(manifest_of(&archive)["omittedReferences"], json!(lost));
    assert_eq!(inspect_archive(request).unwrap().omitted_file_count, 2);

    let destination = tempfile::tempdir().unwrap();
    let restored = Store::open_at(destination.path()).unwrap();
    let report = restore(&restored, &archive, &[project_root(&f)]).unwrap();
    assert_eq!(report.omitted_files, LOST);
    assert_eq!(report.workspace_count, 1);
    assert_eq!(report.session_count, 1);

    // Everything else came back.
    let source = source_read(&restored, &f.workspace, &f.source_version, 0, 100).unwrap();
    assert!(source.text.contains("cited passage"));
    let snapshot = preview_snapshot(&restored, &f.workspace, &f.snapshot).unwrap();
    assert_eq!(snapshot.text.as_deref(), Some("working notes"));
    assert!(restored.session_snapshot(&f.session).is_ok());

    // The gaps read as gaps, and nothing points at a file that isn't there.
    let message = f.read_paper(&restored).unwrap_err().message;
    assert!(message.contains("no extracted text"), "{message}");
    assert_eq!(
        column(
            &restored,
            "SELECT json_extract(extraction_json,'$.status') FROM paper_revisions WHERE text_reference IS NULL"
        ),
        [Some("failed".to_string())]
    );
    let message = data::preview_rows(&restored, &f.workspace, &f.dataset, 0, false)
        .unwrap_err()
        .message;
    assert!(message.contains("already missing"), "{message}");
    assert_eq!(
        column(
            &restored,
            "SELECT storage_reference FROM artifacts WHERE origin='dataset'"
        ),
        [Some(format!("unavailable://blobs/{}", names[1]))]
    );
    let root = destination.path().to_string_lossy().into_owned();
    for reference in references(&restored) {
        assert!(
            reference.starts_with(&root) || reference.starts_with("unavailable://"),
            "{reference}"
        );
    }

    // The store is whole: cleanup works, a reopen finds nothing to repair, and
    // its own backup restores with nothing further left out.
    let report = storage_report(&restored).unwrap();
    assert_eq!(category(&report, "blobs_unreferenced"), Some((0, true)));
    assert!(category(&report, "blobs_unchecked").is_none());
    let reopened = Store::open_at(destination.path()).unwrap();
    assert_eq!(recorded_root(&reopened).as_deref(), Some(root.as_str()));
    assert_eq!(backups(&reopened), 0);
    let next = output.path().join("next.pwrx");
    assert!(export(&reopened, &next).unwrap().omitted_files.is_empty());
    assert!(manifest_of(&next)["omittedReferences"]
        .as_array()
        .unwrap()
        .is_empty());
    let last = tempfile::tempdir().unwrap();
    let last_store = Store::open_at(last.path()).unwrap();
    assert!(restore(&last_store, &next, &[])
        .unwrap()
        .omitted_files
        .is_empty());
    assert!(source_read(&last_store, &f.workspace, &f.source_version, 0, 100).is_ok());
    assert!(f
        .read_paper(&last_store)
        .unwrap_err()
        .message
        .contains("no extracted text"));
}

#[test]
fn a_file_that_returns_reconnects_at_the_next_open() {
    let f = fixture();
    let name = paper_text_and_dataset(&f).remove(1);
    let aside = f.temp.path().join("aside");
    fs::rename(f.store.root_path().join("blobs").join(&name), &aside).unwrap();
    let output = tempfile::tempdir().unwrap();
    let archive = output.path().join("backup.pwrx");
    let approved = missing_backup_files(&f.store)
        .unwrap()
        .into_iter()
        .map(|file| file.reference)
        .collect::<Vec<_>>();
    export_archive_without(
        &f.store,
        ExportArchiveRequest {
            path: archive.to_string_lossy().into_owned(),
        },
        &approved,
    )
    .unwrap();
    let destination = tempfile::tempdir().unwrap();
    let restored = Store::open_at(destination.path()).unwrap();
    restore(&restored, &archive, &[project_root(&f)]).unwrap();

    // Until it's linked again, the returned file still can't be called unused,
    // and the placeholder itself never counts as a location elsewhere.
    fs::rename(&aside, destination.path().join("blobs").join(&name)).unwrap();
    let report = storage_report(&restored).unwrap();
    assert_eq!(
        category(&report, "blobs_unreferenced").map(|c| c.0),
        Some(0)
    );
    assert!(category(&report, "blobs_unchecked").is_none());

    let reopened = Store::open_at(destination.path()).unwrap();
    let rows = data::preview_rows(&reopened, &f.workspace, &f.dataset, 0, false).unwrap();
    assert_eq!(rows.total_rows, 2);
}

#[test]
fn a_moved_store_with_a_lost_file_backs_up_without_naming_its_old_folder() {
    let f = fixture();
    let old_root = f.store.root_path().to_string_lossy().into_owned();
    let names = paper_text_and_dataset(&f);
    f.relocate();
    lose(&f.moved_root(), &names);
    let store = f.reopen();
    let missing = missing_backup_files(&store).unwrap();
    assert_eq!(missing.len(), 2);
    assert!(missing
        .iter()
        .all(|file| file.reference.starts_with("workspace://blobs/")));
    let output = tempfile::tempdir().unwrap();
    let archive = output.path().join("backup.pwrx");
    let approved = missing
        .into_iter()
        .map(|file| file.reference)
        .collect::<Vec<_>>();
    export_archive_without(
        &store,
        ExportArchiveRequest {
            path: archive.to_string_lossy().into_owned(),
        },
        &approved,
    )
    .unwrap();

    let database = output.path().join("archived.sqlite3");
    let mut zip = zip::ZipArchive::new(fs::File::open(&archive).unwrap()).unwrap();
    std::io::copy(
        &mut zip.by_name("database/research.sqlite3").unwrap(),
        &mut fs::File::create(&database).unwrap(),
    )
    .unwrap();
    let connection = Connection::open(&database).unwrap();
    for (table, column) in [
        ("artifacts", "storage_reference"),
        ("paper_revisions", "text_reference"),
        ("source_versions", "text_reference"),
        ("retained_blobs", "storage_reference"),
    ] {
        let leaked: i64 = connection
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE instr({column},?1)>0"),
                [&old_root],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(leaked, 0, "{table}.{column}");
    }

    let destination = tempfile::tempdir().unwrap();
    let restored = Store::open_at(destination.path()).unwrap();
    assert_eq!(
        restore(&restored, &archive, &[project_root(&f)])
            .unwrap()
            .omitted_files,
        LOST
    );
    assert!(source_read(&restored, &f.workspace, &f.source_version, 0, 100).is_ok());
}

#[test]
fn temporary_files_stay_cleanable_while_research_files_are_protected() {
    let f = fixture();
    // Each kind of Trash entry a store can already hold when the folder moves.
    fs::write(f.store.root_path().join("blobs/trashed.txt"), "keep").unwrap();
    prune_applied(&f.store, &["blobs_unreferenced"]).unwrap();
    fs::create_dir(f.store.root_path().join("jobs/trashed")).unwrap();
    prune_applied(&f.store, &["job_scratch"]).unwrap();
    assert_eq!(list_trash(&f.store).unwrap().len(), 2);
    let names = paper_text_and_dataset(&f);
    f.relocate();
    lose(&f.moved_root(), &names);
    fs::write(f.moved_root().join("blobs/orphan.txt"), "orphan").unwrap();
    fs::create_dir(f.moved_root().join("jobs/finished")).unwrap();
    fs::write(f.moved_root().join("jobs/finished/out.txt"), "scratch").unwrap();

    let store = f.reopen();
    let report = storage_report(&store).unwrap();
    assert_eq!(category(&report, "job_scratch"), Some((1, true)));
    assert_eq!(category(&report, "migration_backups"), Some((1, true)));
    assert_eq!(category(&report, "blobs_unreferenced"), Some((0, false)));
    assert_eq!(category(&report, "blobs_unchecked"), Some((1, false)));
    assert!(
        report.note.contains("can still be removed"),
        "{}",
        report.note
    );

    prune_applied(&store, &["job_scratch", "migration_backups"]).unwrap();
    assert!(!store.root_path().join("jobs/finished").exists());
    assert_eq!(backups(&store), 0);
    assert!(store.root_path().join("blobs/orphan.txt").exists());

    // Asking for research files alongside them refuses the whole request.
    fs::create_dir(store.root_path().join("jobs/later")).unwrap();
    assert!(prune_applied(&store, &["job_scratch", "blobs_unreferenced"]).is_err());
    assert!(store.root_path().join("jobs/later").exists());

    // Emptying Trash frees what nothing can need, and keeps the research file.
    assert_eq!(list_trash(&store).unwrap().len(), 4);
    assert_eq!(empty_trash(&store).unwrap(), 3);
    let trash = list_trash(&store).unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].category, "blobs_unreferenced");
    assert!(empty_trash(&store).is_err());
    restore_trash(&store, &trash[0].id).unwrap();
    assert!(store.root_path().join("blobs/trashed.txt").exists());
}

#[test]
fn prune_checks_references_again_when_it_applies() {
    let f = fixture();
    let orphan = f.store.root_path().join("blobs/orphan.txt");
    fs::write(&orphan, "orphan").unwrap();
    let categories = vec!["blobs_unreferenced".to_string()];
    let preview = prune_storage(
        &f.store,
        PruneRequest {
            categories: categories.clone(),
            apply: false,
            expected_preview_token: None,
        },
    )
    .unwrap();
    f.store
        .connection()
        .unwrap()
        .execute(
            "UPDATE paper_revisions SET text_reference='/elsewhere/blobs/gone.txt'",
            [],
        )
        .unwrap();
    let error = prune_storage(
        &f.store,
        PruneRequest {
            categories,
            apply: true,
            expected_preview_token: Some(preview.preview_token),
        },
    )
    .unwrap_err();
    assert!(
        error.message.contains("research files"),
        "{}",
        error.message
    );
    assert!(orphan.exists());
}

#[test]
fn a_prepared_review_copy_moves_with_the_folder() {
    let f = fixture();
    let paper = column(&f.store, "SELECT id FROM papers")[0]
        .clone()
        .unwrap();
    let prepare = |operation: &str| {
        prepare_review_handoff(
            &f.store,
            PrepareReviewHandoffRequest {
                workspace_id: f.workspace.clone(),
                session_id: None,
                paper_id: paper.clone(),
                metadata: json!({}),
                operation_id: operation.into(),
            },
        )
        .unwrap()
    };
    let kept = prepare("kept");
    let gone = prepare("gone");
    f.relocate();
    fs::remove_dir_all(f.moved_root().join("handoffs").join(&gone.id)).unwrap();

    let store = f.reopen();
    let paths = column(&store, "SELECT staged_path FROM review_handoffs")
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let moved = paths.iter().find(|path| path.contains(&kept.id)).unwrap();
    assert!(Path::new(moved).starts_with(store.root_path().join("handoffs")));
    assert!(Path::new(moved).exists());
    // A copy that's gone is prepared again; it doesn't hold the folder back.
    assert!(paths.contains(&gone.staged_path));
    assert_eq!(
        recorded_root(&store).as_deref(),
        Some(store.root_path().to_string_lossy().as_ref())
    );
}
