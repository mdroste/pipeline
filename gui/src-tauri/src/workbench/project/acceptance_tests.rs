//! Task copies, acceptance, undo, and crash recovery on every supported platform.
use super::test_support::*;
use super::*;

#[test]
fn task_copy_acceptance_and_undo_preserve_unrelated_work() {
    let f = fixture();
    fs::write(f.root.join("main.tex"), "dirty initial text").unwrap();
    fs::write(f.root.join("other.md"), "unrelated edit").unwrap();
    let c = copy(&f, &["main.tex"]);
    let body: Checkpoint = decode(&c).unwrap();
    let task = Path::new(&body.task_root);
    fs::write(task.join("main.tex"), "proposal").unwrap();
    fs::write(task.join("new.md"), "new artifact").unwrap();
    assert_eq!(
        fs::read_to_string(f.root.join("main.tex")).unwrap(),
        "dirty initial text"
    );
    let c = capture(&f, &c);
    let applied = mutation(
        &f,
        ProjectAction::Apply {
            checkpoint_id: c.id.clone(),
            paths: vec!["main.tex".into(), "new.md".into()],
            expected_revision: c.revision,
        },
    );
    assert_eq!(
        fs::read_to_string(f.root.join("main.tex")).unwrap(),
        "proposal"
    );
    assert_eq!(
        fs::read_to_string(f.root.join("other.md")).unwrap(),
        "unrelated edit"
    );
    mutation(
        &f,
        ProjectAction::Undo {
            application_id: applied.id,
        },
    );
    assert_eq!(
        fs::read_to_string(f.root.join("main.tex")).unwrap(),
        "dirty initial text"
    );
    assert!(!f.root.join("new.md").exists());
}
#[test]
fn partial_acceptance_invalidates_checks_and_external_changes_block_overwrite() {
    let f = fixture();
    for name in ["a.md", "b.md"] {
        fs::write(f.root.join(name), "original").unwrap();
    }
    let c = copy(&f, &["a.md", "b.md"]);
    let body: Checkpoint = decode(&c).unwrap();
    for name in ["a.md", "b.md"] {
        fs::write(Path::new(&body.task_root).join(name), "proposal").unwrap();
    }
    let c = capture(&f, &c);
    fs::write(f.root.join("b.md"), "external").unwrap();
    let result = tasks::apply(
        &f.store,
        &f.ws,
        &c.id,
        vec!["a.md".into(), "b.md".into()],
        c.revision,
    );
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(f.root.join("a.md")).unwrap(), "original");
    let applied = mutation(
        &f,
        ProjectAction::Apply {
            checkpoint_id: c.id.clone(),
            paths: vec!["a.md".into()],
            expected_revision: c.revision,
        },
    );
    assert!(!decode::<Application>(&applied).unwrap().checks_valid);
    assert_eq!(fs::read_to_string(f.root.join("b.md")).unwrap(), "external");
    fs::write(f.root.join("a.md"), "later edit").unwrap();
    assert!(tasks::undo(&f.store, &f.ws, &applied.id).is_err());
    assert_eq!(
        fs::read_to_string(f.root.join("a.md")).unwrap(),
        "later edit"
    );
}
#[test]
fn recovery_reconciles_crash_between_file_replacement_and_db_update() {
    let f = fixture();
    fs::write(f.root.join("a.md"), "original").unwrap();
    let c = copy(&f, &["a.md"]);
    let body: Checkpoint = decode(&c).unwrap();
    let before = body.base["a.md"].hash.clone();
    let after = blob(&f.store, &f.ws, b"proposal", "test").unwrap();
    let a = Application {
        checkpoint_id: c.id.clone(),
        root_identity: body.root_identity.clone(),
        state: "applying".into(),
        files: vec![AppliedFile {
            path: "a.md".into(),
            before: before.clone(),
            after: Some(after),
            before_executable: false,
            after_executable: false,
            claim: "crashed-0".into(),
            state: "pending".into(),
        }],
        direction: "accept".into(),
        error: None,
        checks_valid: false,
    };
    put(&f.store, &f.ws, "crashed", "application", 0, &a).unwrap();
    files::SafeRoot::open(&f.root)
        .unwrap()
        .replace(
            "a.md",
            before.as_deref(),
            false,
            Some(b"proposal"),
            false,
            "crashed-0",
        )
        .unwrap();
    let reopened = Store::open_at(f.store.root_path()).unwrap();
    let result = tasks::recover(&reopened, &f.ws, "crashed").unwrap();
    assert_eq!(decode::<Application>(&result).unwrap().state, "rolled_back");
    assert_eq!(fs::read_to_string(f.root.join("a.md")).unwrap(), "original");
}
#[test]
fn acceptance_creates_folders_removes_deleted_files_and_leaves_no_claims() {
    let f = fixture();
    fs::write(f.root.join("draft.md"), "draft").unwrap();
    fs::write(f.root.join("obsolete.md"), "old").unwrap();
    let c = copy(&f, &["draft.md", "obsolete.md"]);
    let body: Checkpoint = decode(&c).unwrap();
    let task = Path::new(&body.task_root);
    fs::create_dir_all(task.join("figures/notes")).unwrap();
    fs::write(task.join("figures/notes/new.md"), "nested").unwrap();
    fs::remove_file(task.join("obsolete.md")).unwrap();
    let c = capture(&f, &c);
    let applied = mutation(
        &f,
        ProjectAction::Apply {
            checkpoint_id: c.id.clone(),
            paths: vec!["figures/notes/new.md".into(), "obsolete.md".into()],
            expected_revision: c.revision,
        },
    );
    let claims = |dir: &Path| {
        fs::read_dir(dir).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".pipeline-apply-")
        })
    };
    assert_eq!(
        fs::read_to_string(f.root.join("figures/notes/new.md")).unwrap(),
        "nested"
    );
    assert!(!f.root.join("obsolete.md").exists());
    assert_eq!(
        fs::read_to_string(f.root.join("draft.md")).unwrap(),
        "draft"
    );
    assert!(!claims(&f.root) && !claims(&f.root.join("figures/notes")));
    mutation(
        &f,
        ProjectAction::Undo {
            application_id: applied.id,
        },
    );
    assert_eq!(
        fs::read_to_string(f.root.join("obsolete.md")).unwrap(),
        "old"
    );
    assert!(!f.root.join("figures/notes/new.md").exists());
    assert!(!claims(&f.root) && !claims(&f.root.join("figures/notes")));
}
