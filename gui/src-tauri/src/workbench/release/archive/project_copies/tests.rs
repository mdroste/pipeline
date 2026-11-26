use super::*;
use crate::workbench::{
    project::{self, ProjectAction, ProjectMutation},
    release,
    store::CreateWorkspaceRequest,
};
use std::collections::BTreeMap;

struct Fixture {
    temp: tempfile::TempDir,
    store: Store,
    ws: String,
    cp: String,
    root: PathBuf,
    copy: PathBuf,
}
fn mutate(store: &Store, ws: &str, action: ProjectAction) -> Value {
    project::mutate(
        store,
        ProjectMutation {
            workspace_id: ws.into(),
            operation_id: release::new_id("test").unwrap(),
            action,
        },
    )
    .unwrap()
}
fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("research");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("main.tex"), "original").unwrap();
    let store = Store::open_at(&temp.path().join("store")).unwrap();
    let ws = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Copy backup".into(),
            root: Some(root.to_string_lossy().into()),
            operation_id: "create".into(),
        })
        .unwrap()
        .record
        .id;
    let task = mutate(
        &store,
        &ws,
        ProjectAction::CreateTask {
            objective: "Edit".into(),
            anchor_id: None,
            expected_outputs: vec![],
            expected_checks: vec![],
        },
    );
    let cp = mutate(
        &store,
        &ws,
        ProjectAction::Checkpoint {
            task_id: task["id"].as_str().unwrap().into(),
            paths: vec!["main.tex".into()],
            backend: "copy".into(),
        },
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let copy = project::session_task_root(&store, &ws, &cp).unwrap();
    project::task_session(&store, &ws, &cp).unwrap();
    Fixture {
        temp,
        store,
        ws,
        cp,
        root,
        copy,
    }
}
fn backup(f: &Fixture) -> PathBuf {
    let path = f.temp.path().join("backup.pwrx");
    release::export_archive(
        &f.store,
        release::ExportArchiveRequest {
            path: path.to_string_lossy().into(),
        },
    )
    .unwrap();
    path
}
fn restore(f: &Fixture, archive: &Path) -> Store {
    let target = Store::open_at(&f.temp.path().join("restored")).unwrap();
    let root = f.store.workspace(&f.ws).unwrap().root.unwrap();
    release::import_archive(
        &target,
        release::ImportArchiveRequest {
            path: archive.to_string_lossy().into(),
            root_mappings: BTreeMap::from([(root.clone(), Some(root))]),
        },
    )
    .unwrap();
    target
}
fn apply_and_undo(store: &Store, ws: &str, cp: &str, root: &Path) {
    let captured = mutate(
        store,
        ws,
        ProjectAction::CaptureChanges {
            checkpoint_id: cp.into(),
        },
    );
    let application = mutate(
        store,
        ws,
        ProjectAction::Apply {
            checkpoint_id: cp.into(),
            paths: vec!["main.tex".into()],
            expected_revision: captured["revision"].as_i64().unwrap(),
        },
    );
    assert_eq!(
        fs::read_to_string(root.join("main.tex")).unwrap(),
        "uncaptured edit"
    );
    mutate(
        store,
        ws,
        ProjectAction::Undo {
            application_id: application["id"].as_str().unwrap().into(),
        },
    );
    assert_eq!(
        fs::read_to_string(root.join("main.tex")).unwrap(),
        "original"
    );
}

#[test]
fn uncaptured_private_files_round_trip_and_the_restored_edit_still_applies_and_undoes() {
    let f = fixture();
    fs::write(f.copy.join("main.tex"), "uncaptured edit").unwrap();
    fs::create_dir_all(f.copy.join("generated/empty")).unwrap();
    fs::write(f.copy.join("generated/new.txt"), "new working file").unwrap();
    let archive = backup(&f);
    let restored = restore(&f, &archive);
    let copy = project::session_task_root(&restored, &f.ws, &f.cp).unwrap();
    assert_eq!(
        project::editor_file(&restored, &f.ws, Some(&f.cp), "main.tex")
            .unwrap()
            .content,
        "uncaptured edit"
    );
    assert_eq!(
        fs::read_to_string(copy.join("generated/new.txt")).unwrap(),
        "new working file"
    );
    assert!(copy.join("generated/empty").is_dir());
    // A fresh conversation can bind to the restored checkpoint. Native runtime
    // bindings are deliberately retired by the existing archive restore policy.
    project::task_session(&restored, &f.ws, &f.cp).unwrap();
    apply_and_undo(&restored, &f.ws, &f.cp, &f.root);
}

#[test]
fn legacy_checkpoints_without_private_directory_identity_do_not_block_store_open() {
    let f = fixture();
    f.store.connection().unwrap().execute(
        "UPDATE project_records SET body_json=json_remove(body_json,'$.taskRootIdentity') WHERE id=?1",
        [&f.cp],
    ).unwrap();
    let path = f.store.root_path().to_path_buf();
    drop(f.store);
    Store::open_at(&path).unwrap();
}

#[test]
fn private_copies_remain_usable_after_two_store_moves_but_replacement_is_refused() {
    let f = fixture();
    fs::write(f.copy.join("main.tex"), "uncaptured edit").unwrap();
    let old = f.store.root_path().to_path_buf();
    drop(f.store);
    let first = f.temp.path().join("moved");
    fs::rename(old, &first).unwrap();
    let store = Store::open_at(&first).unwrap();
    assert_eq!(
        project::editor_file(&store, &f.ws, Some(&f.cp), "main.tex")
            .unwrap()
            .content,
        "uncaptured edit"
    );
    drop(store);
    let second = f.temp.path().join("moved-again");
    fs::rename(first, &second).unwrap();
    let store = Store::open_at(&second).unwrap();
    apply_and_undo(&store, &f.ws, &f.cp, &f.root);
    let copy = project::session_task_root(&store, &f.ws, &f.cp).unwrap();
    fs::rename(&copy, copy.with_extension("original")).unwrap();
    fs::create_dir(&copy).unwrap();
    assert!(project::session_task_root(&store, &f.ws, &f.cp).is_err());
}

#[test]
fn invalid_copy_payloads_fail_validation_before_installing_files() {
    let f = fixture();
    let staging = tempfile::tempdir().unwrap();
    let copies = stage(&f.store, &f.store.connection().unwrap(), staging.path()).unwrap();
    let db = f.store.connection().unwrap();
    let mut invalid = copies.clone();
    invalid[0].files[0].path = "../outside".into();
    assert!(validate(&invalid, &db, staging.path(), &mut HashSet::new()).is_err());
    invalid = copies.clone();
    invalid[0].checkpoint_id = "unowned".into();
    assert!(validate(&invalid, &db, staging.path(), &mut HashSet::new()).is_err());
    invalid = copies.clone();
    invalid[0].files[0].sha256 = "wrong".into();
    assert!(validate(&invalid, &db, staging.path(), &mut HashSet::new()).is_err());
    invalid = copies.clone();
    invalid.push(copies[0].clone());
    assert!(validate(&invalid, &db, staging.path(), &mut HashSet::new()).is_err());
}

#[test]
fn missing_unsettled_copy_does_not_replace_an_existing_backup() {
    let f = fixture();
    let archive = backup(&f);
    let original = fs::read(&archive).unwrap();
    fs::rename(&f.copy, f.copy.with_extension("missing")).unwrap();
    assert!(release::export_archive(
        &f.store,
        release::ExportArchiveRequest {
            path: archive.to_string_lossy().into()
        }
    )
    .is_err());
    assert_eq!(fs::read(archive).unwrap(), original);
}

#[test]
fn a_copy_changed_during_staging_keeps_the_previous_backup() {
    let f = fixture();
    let archive = backup(&f);
    let before = fs::read(&archive).unwrap();
    let path = f.copy.join("main.tex");
    AFTER_STAGE.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || fs::write(path, "external edit").unwrap()))
    });
    assert!(release::export_archive(
        &f.store,
        release::ExportArchiveRequest {
            path: archive.to_string_lossy().into()
        }
    )
    .is_err());
    assert_eq!(fs::read(archive).unwrap(), before);
}

#[test]
fn git_copy_content_restores_without_the_original_hosts_git_link() {
    let f = fixture();
    f.store.connection().unwrap().execute("UPDATE project_records SET body_json=json_set(body_json,'$.backend','git') WHERE id=?1", [&f.cp]).unwrap();
    fs::write(f.copy.join(".git"), "gitdir: /another/hosts/repository").unwrap();
    fs::write(f.copy.join("main.tex"), "uncaptured edit").unwrap();
    let restored = restore(&f, &backup(&f));
    let copy = project::session_task_root(&restored, &f.ws, &f.cp).unwrap();
    assert!(!copy.join(".git").exists());
    assert_eq!(
        fs::read_to_string(copy.join("main.tex")).unwrap(),
        "uncaptured edit"
    );
    apply_and_undo(&restored, &f.ws, &f.cp, &f.root);
}

#[test]
fn an_uncommitted_restore_removes_only_its_new_copy_tree() {
    let f = fixture();
    let target = Store::open_at(&f.temp.path().join("destination")).unwrap();
    let staging = tempfile::tempdir().unwrap();
    let db = f.store.connection().unwrap();
    let copies = stage(&f.store, &db, staging.path()).unwrap();
    // Installation changes only this disposable database and destination tree.
    let installed = install(&target, &copies, staging.path(), &db).unwrap();
    assert!(target.root_path().join("project-copies").exists());
    drop(installed);
    assert!(!target.root_path().join("project-copies").exists());
    assert!(f.copy.join("main.tex").exists());
}

#[test]
fn restore_never_overwrites_existing_private_working_files() {
    let f = fixture();
    let staging = tempfile::tempdir().unwrap();
    let db = f.store.connection().unwrap();
    let copies = stage(&f.store, &db, staging.path()).unwrap();
    assert!(install(&f.store, &copies, staging.path(), &db).is_err());
    assert_eq!(
        fs::read_to_string(f.copy.join("main.tex")).unwrap(),
        "original"
    );
}

#[cfg(unix)]
#[test]
fn links_are_refused_and_executable_files_keep_their_mode() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let f = fixture();
    symlink(&f.root, f.copy.join("outside")).unwrap();
    assert!(stage(&f.store, &f.store.connection().unwrap(), f.temp.path()).is_err());
    fs::remove_file(f.copy.join("outside")).unwrap();
    fs::set_permissions(f.copy.join("main.tex"), fs::Permissions::from_mode(0o700)).unwrap();
    let restored = restore(&f, &backup(&f));
    let copy = project::session_task_root(&restored, &f.ws, &f.cp).unwrap();
    assert_ne!(
        fs::metadata(copy.join("main.tex"))
            .unwrap()
            .permissions()
            .mode()
            & 0o111,
        0
    );
}

/// Apply the fixture edit, then remove its copy as an editor save's reclaim does.
fn apply_and_reclaim(f: &Fixture) -> String {
    fs::write(f.copy.join("main.tex"), "uncaptured edit").unwrap();
    let captured = mutate(
        &f.store,
        &f.ws,
        ProjectAction::CaptureChanges {
            checkpoint_id: f.cp.clone(),
        },
    );
    let application = mutate(
        &f.store,
        &f.ws,
        ProjectAction::Apply {
            checkpoint_id: f.cp.clone(),
            paths: vec!["main.tex".into()],
            expected_revision: captured["revision"].as_i64().unwrap(),
        },
    );
    fs::remove_dir_all(&f.copy).unwrap();
    application["id"].as_str().unwrap().to_owned()
}

fn checkpoint_state(store: &Store, ws: &str, cp: &str) -> String {
    let body: String = store
        .connection()
        .unwrap()
        .query_row(
            "SELECT body_json FROM project_records WHERE workspace_id=?1 AND id=?2",
            [ws, cp],
            |row| row.get(0),
        )
        .unwrap();
    serde_json::from_str::<Value>(&body).unwrap()["state"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn export(store: &Store, path: &Path) -> WorkbenchResult<()> {
    release::export_archive(
        store,
        release::ExportArchiveRequest {
            path: path.to_string_lossy().into(),
        },
    )
    .map(|_| ())
}

#[test]
fn undoing_an_edit_whose_copy_was_reclaimed_keeps_backups_working() {
    let f = fixture();
    let application = apply_and_reclaim(&f);
    mutate(
        &f.store,
        &f.ws,
        ProjectAction::Undo {
            application_id: application,
        },
    );
    assert_eq!(checkpoint_state(&f.store, &f.ws, &f.cp), "undone");
    export(&f.store, &f.temp.path().join("after-undo.pwrx")).unwrap();
}

#[test]
fn a_reclaimed_copy_does_not_block_backups_after_a_store_move() {
    let f = fixture();
    apply_and_reclaim(&f);
    let old = f.store.root_path().to_path_buf();
    drop(f.store);
    let moved = f.temp.path().join("moved");
    fs::rename(old, &moved).unwrap();
    let store = Store::open_at(&moved).unwrap();
    assert_eq!(checkpoint_state(&store, &f.ws, &f.cp), "accepted");
    export(&store, &f.temp.path().join("after-move.pwrx")).unwrap();
}

#[test]
fn a_reclaimed_copy_does_not_block_backups_after_a_restore_elsewhere() {
    let f = fixture();
    apply_and_reclaim(&f);
    let restored = restore(&f, &backup(&f));
    export(&restored, &f.temp.path().join("after-restore.pwrx")).unwrap();
}
