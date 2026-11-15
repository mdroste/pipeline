use super::*;
use crate::workbench::store::CreateWorkspaceRequest;

fn prune(store: &Store, categories: &[&str], apply: bool) -> WorkbenchResult<PrunePlan> {
    let categories: Vec<String> = categories.iter().map(|c| (*c).into()).collect();
    let preview = prune_storage(
        store,
        PruneRequest {
            categories: categories.clone(),
            apply: false,
            expected_preview_token: None,
        },
    )?;
    if !apply {
        return Ok(preview);
    }
    prune_storage(
        store,
        PruneRequest {
            categories,
            apply: true,
            expected_preview_token: Some(preview.preview_token),
        },
    )
}

#[test]
fn prune_requires_a_current_preview_of_categories_and_file_identities() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(temp.path()).unwrap();
    let dir = store.root_path().join("jobs/finished");
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("result.txt");
    fs::write(&file, "first").unwrap();
    let apply = |token, categories| {
        prune_storage(
            &store,
            PruneRequest {
                categories,
                apply: true,
                expected_preview_token: token,
            },
        )
    };
    assert!(apply(None, vec!["job_scratch".into()]).is_err());
    let preview = prune(&store, &["job_scratch"], false).unwrap();
    fs::write(dir.join("new.txt"), "new").unwrap();
    assert!(apply(Some(preview.preview_token), vec!["job_scratch".into()]).is_err());
    let preview = prune(&store, &["job_scratch"], false).unwrap();
    // Same name and length, different file identity.
    let replacement = store.root_path().join("replacement");
    fs::write(&replacement, "other").unwrap();
    fs::rename(replacement, &file).unwrap();
    assert!(apply(Some(preview.preview_token), vec!["job_scratch".into()]).is_err());
    let preview = prune(&store, &["job_scratch"], false).unwrap();
    assert!(apply(
        Some(preview.preview_token.clone()),
        vec!["job_scratch".into(), "blobs_unreferenced".into()]
    )
    .is_err());
    assert!(file.exists());
    assert!(
        apply(Some(preview.preview_token), vec!["job_scratch".into()])
            .unwrap()
            .applied
    );
    assert!(!dir.exists());
}

#[test]
fn failed_trash_commit_leaves_the_original_file_in_place() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(temp.path()).unwrap();
    let file = store.root_path().join("blobs/orphan.txt");
    fs::write(&file, "recoverable work").unwrap();
    store.connection().unwrap().execute_batch(
        "CREATE TABLE commit_failure(value TEXT REFERENCES workspaces(id) DEFERRABLE INITIALLY DEFERRED);
         CREATE TRIGGER defer_failure AFTER INSERT ON storage_trash BEGIN INSERT INTO commit_failure VALUES('missing'); END;"
    ).unwrap();
    assert!(prune(&store, &["blobs_unreferenced"], true).is_err());
    assert_eq!(fs::read_to_string(&file).unwrap(), "recoverable work");
    assert!(list_trash(&store).unwrap().is_empty());
}

#[test]
fn trash_reconciles_interruptions_before_move_and_after_restore() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(temp.path()).unwrap();
    let file = store.root_path().join("blobs/orphan.txt");
    let trash = store.root_path().join("trash/interrupted/blobs/orphan.txt");
    fs::write(&file, "recoverable work").unwrap();
    store.connection().unwrap().execute("INSERT INTO storage_trash(id,category,original_path,trash_path,size_bytes,reason,moved_at,state) VALUES('interrupted','blobs_unreferenced',?1,?2,16,'test','t','trashed')", params![file.to_string_lossy(), trash.to_string_lossy()]).unwrap();
    // Crash after committing the intent, before moving the file.
    let store = Store::open_at(temp.path()).unwrap();
    assert!(list_trash(&store).unwrap().is_empty());
    assert!(file.exists());
    // Crash after moving into trash: the same durable intent exposes it.
    fs::create_dir_all(trash.parent().unwrap()).unwrap();
    store
        .connection()
        .unwrap()
        .execute(
            "UPDATE storage_trash SET state='trashed' WHERE id='interrupted'",
            [],
        )
        .unwrap();
    fs::rename(&file, &trash).unwrap();
    let store = Store::open_at(temp.path()).unwrap();
    assert_eq!(list_trash(&store).unwrap().len(), 1);
    // Simulate an interrupted restore after its rename but before its UPDATE.
    fs::rename(&trash, &file).unwrap();
    let store = Store::open_at(temp.path()).unwrap();
    assert!(list_trash(&store).unwrap().is_empty());
    assert_eq!(fs::read_to_string(&file).unwrap(), "recoverable work");
}

#[test]
fn deleting_a_conversation_removes_its_working_files() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(temp.path()).unwrap();
    let session = store
        .create_session(crate::workbench::store::CreateSessionRequest {
            workspace_id: None,
            title: "Unfiled".into(),
            operation_id: "session".into(),
        })
        .unwrap()
        .record
        .id;
    let root = store.runtime_root(&session).unwrap();
    fs::write(root.join("work.md"), "conversation work").unwrap();
    store
        .delete_session(crate::workbench::store::DeleteSessionRequest {
            session_id: session,
            operation_id: "delete".into(),
        })
        .unwrap();
    assert!(!root.exists());
}

#[test]
fn scratch_pruning_preserves_conversation_files_and_rejects_active_turns() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(temp.path()).unwrap();
    let session = store
        .create_session(crate::workbench::store::CreateSessionRequest {
            workspace_id: None,
            title: "Unfiled".into(),
            operation_id: "session".into(),
        })
        .unwrap()
        .record
        .id;
    let file = store.runtime_root(&session).unwrap().join("work.md");
    fs::write(&file, "conversation work").unwrap();
    let scratch = store.root_path().join("jobs/finished");
    fs::create_dir_all(&scratch).unwrap();
    fs::write(scratch.join("out.txt"), "scratch").unwrap();
    let c = store.connection().unwrap();
    c.execute("INSERT INTO session_bindings(id,session_id,runtime_namespace,provider_thread_id,incarnation,created_at) VALUES('b',?1,'test','thread',1,'t')", [&session]).unwrap();
    c.execute("INSERT INTO turns(id,binding_id,client_submission_id,state,created_at,updated_at) VALUES('t','b','submit','running','t','t')", []).unwrap();
    assert!(prune(&store, &["job_scratch"], true).is_err());
    assert!(file.exists() && scratch.exists());
    c.execute("UPDATE turns SET state='completed' WHERE id='t'", [])
        .unwrap();
    let plan = prune(&store, &["job_scratch"], true).unwrap();
    assert_eq!(plan.entries.len(), 1);
    assert!(file.exists());
    assert!(!scratch.exists());
    assert!(
        !storage_report(&store)
            .unwrap()
            .categories
            .iter()
            .find(|c| c.key == "conversation_files")
            .unwrap()
            .disposable
    );
}

#[test]
fn a_folder_too_large_to_count_is_reported_as_a_lower_bound() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(temp.path()).unwrap();
    let session = store.root_path().join("jobs/conversations/session");
    fs::create_dir_all(&session).unwrap();
    for index in 0..=MAX_WALK_ENTRIES {
        fs::File::create(session.join(index.to_string())).unwrap();
    }
    let scratch = store.root_path().join("jobs/finished-run");
    fs::create_dir_all(&scratch).unwrap();
    fs::write(scratch.join("log.txt"), "scratch").unwrap();
    let category = |report: &StorageReport, key: &str| {
        report
            .categories
            .iter()
            .find(|c| c.key == key)
            .cloned()
            .unwrap()
    };

    // A model's working folder neither hides disk use nor blocks cleanup.
    let report = storage_report(&store).unwrap();
    assert!(category(&report, "conversation_files").partial);
    assert!(!category(&report, "job_scratch").partial);
    assert!(report.note.contains("at least"));
    let plan = prune(&store, &["job_scratch"], true).unwrap();
    assert_eq!(plan.entries.len(), 1);
    assert!(!scratch.exists());

    // Cleanup keeps its limit: it must not act on a listing it couldn't finish.
    fs::rename(&session, store.root_path().join("jobs/huge-run")).unwrap();
    let report = storage_report(&store).unwrap();
    assert!(category(&report, "job_scratch").partial);
    assert!(!category(&report, "conversation_files").partial);
    assert!(prune(&store, &["job_scratch"], false).is_err());
}

#[test]
fn prune_preserves_referenced_evidence_and_is_recoverable() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(temp.path()).unwrap();
    let ws = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Retention".into(),
            root: None,
            operation_id: "create".into(),
        })
        .unwrap()
        .record
        .id;
    let blobs = store.root_path().join("blobs");
    let referenced = blobs.join("aaaa.txt");
    let orphan = blobs.join("bbbb.txt");
    fs::write(&referenced, "evidence").unwrap();
    fs::write(&orphan, "orphan bytes").unwrap();
    store.connection().unwrap().execute("INSERT INTO artifacts(id,workspace_id,content_hash,media_kind,size_bytes,origin,storage_reference,created_at) VALUES('a1',?1,'h','txt',8,'test',?2,'t')", params![ws, referenced.to_string_lossy()]).unwrap();
    let context = store.root_path().join("context");
    fs::create_dir_all(&context).unwrap();
    fs::write(context.join("stale.txt"), "stale").unwrap();
    let report = storage_report(&store).unwrap();
    let cat = |k: &str| {
        report
            .categories
            .iter()
            .find(|c| c.key == k)
            .unwrap()
            .clone()
    };
    assert_eq!(cat("blobs_referenced").entries, 1);
    assert_eq!(cat("blobs_unreferenced").entries, 1);
    assert_eq!(cat("context_unreferenced").entries, 1);
    assert!(!cat("blobs_referenced").disposable);
    assert!(prune(&store, &["blobs_referenced"], true).is_err());
    let preview = prune(
        &store,
        &["blobs_unreferenced", "context_unreferenced"],
        false,
    )
    .unwrap();
    assert_eq!(preview.entries.len(), 2);
    assert!(!preview.applied);
    assert!(orphan.exists());
    let applied = prune(&store, &["blobs_unreferenced"], true).unwrap();
    assert!(applied.applied);
    assert!(!orphan.exists());
    assert!(referenced.exists());
    let trash = list_trash(&store).unwrap();
    assert_eq!(trash.len(), 1);
    restore_trash(&store, &trash[0].id).unwrap();
    assert_eq!(fs::read_to_string(&orphan).unwrap(), "orphan bytes");
    assert!(list_trash(&store).unwrap().is_empty());
    prune(&store, &["blobs_unreferenced"], true).unwrap();
    assert_eq!(empty_trash(&store).unwrap(), 1);
    assert!(!orphan.exists());
    assert!(list_trash(&store).unwrap().is_empty());
    assert!(referenced.exists());
}
