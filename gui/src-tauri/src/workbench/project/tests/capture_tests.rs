//! Task copies that stop partway, and inventories of folders that change underfoot.
use super::*;

/// A committed repository with `a.md` in it, for the Git backend.
#[cfg(unix)]
fn git_fixture() -> Fixture {
    let f = fixture();
    tasks::git(&f.root, &["init"]).unwrap();
    tasks::git(
        &f.root,
        &["config", "user.email", "fixture@example.invalid"],
    )
    .unwrap();
    tasks::git(&f.root, &["config", "user.name", "Pipeline fixture"]).unwrap();
    tasks::git(&f.root, &["config", "commit.gpgsign", "false"]).unwrap();
    fs::write(f.root.join("a.md"), "committed").unwrap();
    tasks::git(&f.root, &["add", "a.md"]).unwrap();
    tasks::git(&f.root, &["commit", "-m", "fixture"]).unwrap();
    f
}

#[test]
#[cfg(unix)]
fn a_git_task_copy_is_unaffected_by_what_the_shell_prints_at_startup() {
    let f = git_fixture();
    let shell = tempfile::tempdir().unwrap();
    fs::write(shell.path().join(".zshrc"), "echo Welcome back\n").unwrap();
    fs::write(shell.path().join(".zprofile"), "echo Good morning\n").unwrap();
    // Only zsh reads this, and any zsh run elsewhere here ignores what it prints.
    std::env::set_var("ZDOTDIR", shell.path());
    let task = task(&f);
    let created = mutate(
        &f.store,
        ProjectMutation {
            workspace_id: f.ws.clone(),
            operation_id: id("test").unwrap(),
            action: ProjectAction::Checkpoint {
                task_id: task.id,
                paths: vec!["a.md".into()],
                backend: "git".into(),
            },
        },
    );
    std::env::remove_var("ZDOTDIR");
    let body: Checkpoint = serde_json::from_value(created.unwrap()["body"].clone()).unwrap();
    assert_eq!(body.backend, "git");
}

/// Stops a capture after its copy exists: the task's anchor has gone, which only
/// the last step reads.
fn fail_capture_late(f: &Fixture, backend: &str) -> Vec<String> {
    fs::write(f.root.join("a.md"), "committed").unwrap();
    let anchor: ProjectRecord = serde_json::from_value(
        mutate(
            &f.store,
            ProjectMutation {
                workspace_id: f.ws.clone(),
                operation_id: id("test").unwrap(),
                action: ProjectAction::CreateTask {
                    objective: "Anchor stand-in".into(),
                    anchor_id: None,
                    expected_outputs: vec![],
                    expected_checks: vec![],
                },
            },
        )
        .unwrap(),
    )
    .unwrap();
    let task = task(f);
    let mut body: ResearchTask = decode(&task).unwrap();
    body.anchor_id = Some(anchor.id);
    let task = put(&f.store, &f.ws, &task.id, "task", task.revision, &body).unwrap();
    let error = mutate(
        &f.store,
        ProjectMutation {
            workspace_id: f.ws.clone(),
            operation_id: id("test").unwrap(),
            action: ProjectAction::Checkpoint {
                task_id: task.id,
                paths: vec!["a.md".into()],
                backend: backend.into(),
            },
        },
    )
    .unwrap_err();
    assert!(!error.message.is_empty());
    fs::read_dir(f.root.join(SCRATCH))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name != ".gitignore")
        .collect()
}

#[test]
fn a_failed_capture_leaves_no_task_folder() {
    assert!(fail_capture_late(&fixture(), "copy").is_empty());
}

#[test]
#[cfg(unix)]
fn a_failed_capture_leaves_no_worktree_or_branch() {
    let f = git_fixture();
    let worktrees = tasks::git(&f.root, &["worktree", "list", "--porcelain"]).unwrap();
    assert!(fail_capture_late(&f, "git").is_empty());
    assert_eq!(
        tasks::git(&f.root, &["worktree", "list", "--porcelain"]).unwrap(),
        worktrees
    );
    let branches = tasks::git(&f.root, &["branch", "--list", "codex/*"]).unwrap();
    assert!(
        branches.is_empty(),
        "{}",
        String::from_utf8_lossy(&branches)
    );
}

#[test]
fn a_file_vanishing_mid_scan_leaves_the_rest_of_the_inventory() {
    let f = fixture();
    fs::write(f.root.join("kept.md"), "kept").unwrap();
    let gone = f.root.join("paper.aux.tmp");
    fs::write(&gone, "temporary").unwrap();
    inventory::AFTER_LISTING.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || {
            let _ = fs::remove_file(&gone);
        }));
    });
    let scanned = inventory_at(&f.root, &[]);
    inventory::AFTER_LISTING.with(|hook| *hook.borrow_mut() = None);
    let scanned = scanned.unwrap();
    assert!(!scanned.complete);
    assert_eq!(scanned.warnings.len(), 1, "{:?}", scanned.warnings);
    let status = |path: &str| {
        scanned
            .files
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.status.clone())
    };
    assert_eq!(status("kept.md").as_deref(), Some("current"));
    assert_eq!(status("paper.aux.tmp").as_deref(), Some("unavailable"));
}

#[test]
#[cfg(unix)]
fn an_unreadable_folder_leaves_the_rest_of_the_inventory() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture();
    fs::write(f.root.join("kept.md"), "kept").unwrap();
    let locked = f.root.join("locked");
    fs::create_dir(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let scanned = inventory_at(&f.root, &[]);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
    let scanned = scanned.unwrap();
    assert!(!scanned.complete);
    assert!(scanned
        .files
        .iter()
        .any(|file| file.path == "locked" && file.status == "unavailable"));
    assert!(scanned.files.iter().any(|file| file.path == "kept.md"));
}

#[test]
#[cfg(unix)]
fn an_unreadable_folder_keeps_its_files_until_it_can_be_read() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture();
    let locked = f.root.join("locked");
    fs::create_dir_all(locked.join("deep")).unwrap();
    fs::write(locked.join("data.csv"), "a,b").unwrap();
    fs::write(locked.join("deep/more.csv"), "c,d").unwrap();
    let refresh =
        |f: &Fixture| -> Inventory { decode(&mutation(f, ProjectAction::Refresh)).unwrap() };
    let row = |inventory: &Inventory, path: &str| {
        inventory
            .files
            .iter()
            .find(|file| file.path == path)
            .cloned()
    };
    let before = refresh(&f);
    let hash = row(&before, "locked/data.csv").unwrap().hash;
    assert!(hash.is_some());

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let first = refresh(&f);
    let second = refresh(&f);
    let check = inventory::check_for_changes(&f.store, &f.ws);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
    check.unwrap();
    for held in [&first, &second] {
        assert!(!held.complete);
        assert!(held
            .warnings
            .iter()
            .any(|w| w.contains("could not be read")));
        assert_eq!(row(held, "locked").unwrap().status, "unavailable");
        let data = row(held, "locked/data.csv").unwrap();
        assert_ne!(data.status, "missing");
        assert_eq!(data.hash, hash);
        assert!(row(held, "locked/deep/more.csv").unwrap().hash.is_some());
    }

    let back = refresh(&f);
    assert!(back.complete);
    assert_eq!(row(&back, "locked/data.csv").unwrap().status, "current");
    assert_eq!(
        row(&back, "locked/deep/more.csv").unwrap().status,
        "current"
    );
    assert!(row(&back, "locked").is_none());
}

#[test]
#[cfg(unix)]
fn a_paper_in_an_unreadable_folder_is_unknown_rather_than_changed() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture();
    // The list has never seen this file, so only the folder check can place it.
    mutation(&f, ProjectAction::Refresh);
    let folder = f.root.join("paper");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("main.md"), "Argument").unwrap();
    let revision = research::import_paper(
        &f.store,
        research::ImportPaperRequest {
            workspace_id: f.ws.clone(),
            paper_id: None,
            title: "Paper".into(),
            role: "manuscript".into(),
            path: folder.join("main.md").to_string_lossy().into_owned(),
            operation_id: id("import").unwrap(),
        },
    )
    .unwrap()
    .revision
    .unwrap();
    mutation(
        &f,
        ProjectAction::SaveHome {
            expected_revision: 0,
            settings: ProjectHomeSettings {
                manuscript_revision_id: Some(revision.id),
                ..Default::default()
            },
        },
    );
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o000)).unwrap();
    mutation(&f, ProjectAction::Refresh);
    let overview = home(&f.store, &f.ws);
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700)).unwrap();
    let overview = overview.unwrap();
    assert_eq!(overview.working_copy_changed, 0);
    assert!(
        overview
            .working_copy_status
            .contains("could not be checked"),
        "{}",
        overview.working_copy_status
    );
}

#[test]
fn a_missing_file_is_listed_once_and_then_leaves_the_inventory() {
    let f = fixture();
    fs::write(f.root.join("kept.md"), "kept").unwrap();
    fs::write(f.root.join("gone.md"), "gone").unwrap();
    let scan = |f: &Fixture| -> Inventory { decode(&mutation(f, ProjectAction::Refresh)).unwrap() };
    let path_listed =
        |inventory: &Inventory| inventory.files.iter().any(|file| file.path == "gone.md");
    assert!(path_listed(&scan(&f)));
    fs::remove_file(f.root.join("gone.md")).unwrap();
    let missing = scan(&f);
    assert!(missing
        .files
        .iter()
        .any(|file| file.path == "gone.md" && file.status == "missing"));
    assert!(!path_listed(&scan(&f)));
    assert!(!path_listed(&scan(&f)));
}

const GENERATED_FILE: &str = ".pipeline-workbench/generated/a/plot.py";

/// An edit as an earlier build left it, ready to review, with a change in the
/// generated folder, which it treated as project content.
fn reviewed_generated_change(f: &Fixture) -> ProjectRecord {
    fs::write(f.root.join("a.md"), "before").unwrap();
    fs::create_dir_all(f.root.join(GENERATED_FILE).parent().unwrap()).unwrap();
    fs::write(f.root.join(GENERATED_FILE), "print('before')\n").unwrap();
    let c = copy(f, &["a.md"]);
    let body: Checkpoint = decode(&c).unwrap();
    fs::write(Path::new(&body.task_root).join("a.md"), "after").unwrap();
    let c = capture(f, &c);
    let mut body: Checkpoint = decode(&c).unwrap();
    let entry = |bytes: &[u8]| {
        let mut entry = body.base["a.md"].clone();
        entry.path = GENERATED_FILE.into();
        entry.size = bytes.len() as u64;
        entry.hash = Some(blob(&f.store, &f.ws, bytes, &c.id).unwrap());
        entry
    };
    let (before, after) = (entry(b"print('before')\n"), entry(b"print('after')\n"));
    body.base.insert(GENERATED_FILE.into(), before);
    body.proposed.insert(GENERATED_FILE.into(), after);
    put(&f.store, &f.ws, &c.id, "checkpoint", c.revision, &body).unwrap()
}

fn accept_all(f: &Fixture, c: &ProjectRecord) -> ProjectRecord {
    mutation(
        f,
        ProjectAction::Apply {
            checkpoint_id: c.id.clone(),
            paths: vec![GENERATED_FILE.into(), "a.md".into()],
            expected_revision: c.revision,
        },
    )
}

fn generated_text(f: &Fixture) -> String {
    fs::read_to_string(f.root.join(GENERATED_FILE)).unwrap()
}

#[test]
fn an_earlier_edit_in_the_generated_folder_can_be_accepted_and_undone() {
    let f = fixture();
    let c = reviewed_generated_change(&f);
    let applied = accept_all(&f, &c);
    assert_eq!(generated_text(&f), "print('after')\n");

    assert!(program_file_hash(&f.store, &f.ws, GENERATED_FILE)
        .unwrap()
        .is_some());

    tasks::undo(&f.store, &f.ws, &applied.id).unwrap();
    assert_eq!(generated_text(&f), "print('before')\n");
    assert_eq!(fs::read_to_string(f.root.join("a.md")).unwrap(), "before");
}

#[test]
fn undo_still_refuses_to_overwrite_a_later_change_in_the_generated_folder() {
    let f = fixture();
    let c = reviewed_generated_change(&f);
    let applied = accept_all(&f, &c);
    fs::write(f.root.join(GENERATED_FILE), "print('rerun')\n").unwrap();
    let error = tasks::undo(&f.store, &f.ws, &applied.id).unwrap_err();
    assert!(error.message.contains("subsequent edits"), "{error:?}");
    assert_eq!(generated_text(&f), "print('rerun')\n");
}

#[test]
#[cfg(unix)]
fn an_interrupted_acceptance_in_the_generated_folder_recovers() {
    let f = fixture();
    let c = reviewed_generated_change(&f);
    let applied = accept_all(&f, &c);
    let mut a: Application = decode(&applied).unwrap();
    a.state = "applying".into();
    put(
        &f.store,
        &f.ws,
        &applied.id,
        "application",
        applied.revision,
        &a,
    )
    .unwrap();
    // The crash came after the file moved aside and before its new bytes landed.
    let n = a
        .files
        .iter()
        .position(|file| file.path == GENERATED_FILE)
        .unwrap();
    let file = f.root.join(GENERATED_FILE);
    fs::write(
        file.with_file_name(format!(".pipeline-apply-{}", a.files[n].claim)),
        "print('before')\n",
    )
    .unwrap();
    fs::remove_file(&file).unwrap();

    tasks::recover(&f.store, &f.ws, &applied.id).unwrap();
    assert_eq!(generated_text(&f), "print('before')\n");
    let settled: Application =
        decode(&record(&f.store, &f.ws, &applied.id, "application").unwrap()).unwrap();
    assert_eq!(settled.state, "rolled_back");
}

#[test]
fn the_record_route_opens_only_the_generated_folder() {
    let f = fixture();
    fs::create_dir_all(f.root.join(GENERATED_FILE).parent().unwrap()).unwrap();
    fs::write(f.root.join(GENERATED_FILE), "print('before')\n").unwrap();
    let records = files::SafeRoot::for_records(&f.root).unwrap();
    assert!(records.optional_read(GENERATED_FILE).unwrap().is_some());
    for path in [".git/config", ".pipeline-tasks/a", ".pipeline-apply-x"] {
        assert!(records.optional_read(path).is_err(), "{path}");
    }
    let tools = files::SafeRoot::open(&f.root).unwrap();
    assert!(tools.optional_read(GENERATED_FILE).is_err());
}

#[test]
fn a_new_capture_never_proposes_a_model_write_in_the_generated_folder() {
    let f = fixture();
    let c = reviewed_generated_change(&f);
    let body: Checkpoint = decode(&c).unwrap();
    let in_copy = Path::new(&body.task_root).join(GENERATED_FILE);
    fs::create_dir_all(in_copy.parent().unwrap()).unwrap();
    fs::write(&in_copy, "print('from the model')\n").unwrap();

    let captured: Checkpoint = decode(&capture(&f, &c)).unwrap();
    assert_eq!(
        captured.proposed[GENERATED_FILE].hash,
        captured.base[GENERATED_FILE].hash
    );
}
