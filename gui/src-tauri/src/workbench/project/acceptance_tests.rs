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

/// An edit of two files, the first with two separate changes.
fn edit_with_two_hunks(f: &Fixture) -> ProjectRecord {
    fs::write(f.root.join("a.md"), "one\nmiddle\ntwo\n").unwrap();
    fs::write(f.root.join("b.md"), "other\n").unwrap();
    let c = copy(f, &["a.md", "b.md"]);
    let body: Checkpoint = decode(&c).unwrap();
    let task = Path::new(&body.task_root);
    fs::write(task.join("a.md"), "ONE\nmiddle\nTWO\n").unwrap();
    fs::write(task.join("b.md"), "OTHER\n").unwrap();
    capture(f, &c)
}
fn accept_changes(f: &Fixture, c: &ProjectRecord, path: &str, content: &str) -> ProjectRecord {
    mutation(
        f,
        ProjectAction::AcceptChanges {
            checkpoint_id: c.id.clone(),
            path: path.into(),
            content: content.into(),
            expected_revision: c.revision,
        },
    )
}
fn reread(f: &Fixture, c: &ProjectRecord) -> ProjectRecord {
    record(&f.store, &f.ws, &c.id, "checkpoint").unwrap()
}
fn changed_paths(c: &ProjectRecord) -> Vec<String> {
    let body: Checkpoint = decode(c).unwrap();
    body.proposed
        .iter()
        .filter(|(path, file)| body.base[*path].hash != file.hash)
        .map(|(path, _)| path.clone())
        .collect()
}
#[test]
fn accepting_part_of_a_file_shrinks_the_edit_and_leaves_it_acceptable() {
    let f = fixture();
    let c = edit_with_two_hunks(&f);
    accept_changes(&f, &c, "a.md", "ONE\nmiddle\ntwo\n");
    assert_eq!(
        fs::read_to_string(f.root.join("a.md")).unwrap(),
        "ONE\nmiddle\ntwo\n"
    );
    let c = reread(&f, &c);
    let body: Checkpoint = decode(&c).unwrap();
    assert_eq!(body.state, "review");
    assert_eq!(body.base["a.md"].size, 15);
    assert_eq!(
        blob_read(&f.store, &f.ws, body.base["a.md"].hash.as_ref().unwrap()).unwrap(),
        b"ONE\nmiddle\ntwo\n"
    );
    assert_eq!(changed_paths(&c), ["a.md", "b.md"]);

    // The second hunk of the same file completes that file and drops it.
    accept_changes(&f, &c, "a.md", "ONE\nmiddle\nTWO\n");
    let c = reread(&f, &c);
    assert_eq!(changed_paths(&c), ["b.md"]);

    // A file-level acceptance of the rest of the edit still applies.
    mutation(
        &f,
        ProjectAction::Apply {
            checkpoint_id: c.id.clone(),
            paths: vec!["b.md".into()],
            expected_revision: c.revision,
        },
    );
    assert_eq!(fs::read_to_string(f.root.join("b.md")).unwrap(), "OTHER\n");
    assert_eq!(
        decode::<Checkpoint>(&reread(&f, &c)).unwrap().state,
        "accepted"
    );
}
#[test]
fn accepting_every_change_through_the_diff_finishes_the_edit() {
    let f = fixture();
    let c = edit_with_two_hunks(&f);
    accept_changes(&f, &c, "a.md", "ONE\nmiddle\nTWO\n");
    let c = reread(&f, &c);
    accept_changes(&f, &c, "b.md", "OTHER\n");
    let c = reread(&f, &c);
    assert!(changed_paths(&c).is_empty());
    assert_eq!(decode::<Checkpoint>(&c).unwrap().state, "accepted");
}
#[test]
fn a_file_level_acceptance_can_finish_a_partly_accepted_file() {
    let f = fixture();
    let c = edit_with_two_hunks(&f);
    accept_changes(&f, &c, "a.md", "ONE\nmiddle\ntwo\n");
    let c = reread(&f, &c);
    let applied = mutation(
        &f,
        ProjectAction::Apply {
            checkpoint_id: c.id.clone(),
            paths: vec!["a.md".into(), "b.md".into()],
            expected_revision: c.revision,
        },
    );
    assert_eq!(
        fs::read_to_string(f.root.join("a.md")).unwrap(),
        "ONE\nmiddle\nTWO\n"
    );
    assert_eq!(fs::read_to_string(f.root.join("b.md")).unwrap(), "OTHER\n");
    // Undoing it returns to the partly accepted file, not the original.
    mutation(
        &f,
        ProjectAction::Undo {
            application_id: applied.id,
        },
    );
    assert_eq!(
        fs::read_to_string(f.root.join("a.md")).unwrap(),
        "ONE\nmiddle\ntwo\n"
    );
    assert_eq!(
        decode::<Checkpoint>(&reread(&f, &c)).unwrap().state,
        "review"
    );
}
#[test]
fn external_changes_are_refused_after_a_partial_acceptance() {
    let f = fixture();
    let c = edit_with_two_hunks(&f);
    fs::write(f.root.join("a.md"), "edited elsewhere\n").unwrap();
    let refused = tasks::accept_changes(
        &f.store,
        &f.ws,
        &c.id,
        "a.md",
        "ONE\nmiddle\ntwo\n",
        c.revision,
    )
    .unwrap_err();
    assert!(refused.message.contains("External change"));
    assert_eq!(
        fs::read_to_string(f.root.join("a.md")).unwrap(),
        "edited elsewhere\n"
    );
    assert_eq!(reread(&f, &c).revision, c.revision);

    fs::write(f.root.join("a.md"), "one\nmiddle\ntwo\n").unwrap();
    accept_changes(&f, &c, "a.md", "ONE\nmiddle\ntwo\n");
    let c = reread(&f, &c);
    fs::write(f.root.join("a.md"), "edited elsewhere\n").unwrap();
    for error in [
        tasks::accept_changes(
            &f.store,
            &f.ws,
            &c.id,
            "a.md",
            "ONE\nmiddle\nTWO\n",
            c.revision,
        ),
        tasks::apply(&f.store, &f.ws, &c.id, vec!["a.md".into()], c.revision),
    ] {
        assert!(error.unwrap_err().message.contains("External change"));
    }
    assert_eq!(
        fs::read_to_string(f.root.join("a.md")).unwrap(),
        "edited elsewhere\n"
    );
}
#[test]
fn undoing_accepted_changes_restores_the_file_and_the_diff_in_order() {
    let f = fixture();
    let c = edit_with_two_hunks(&f);
    let original_base = decode::<Checkpoint>(&c).unwrap().base["a.md"].clone();
    let first = accept_changes(&f, &c, "a.md", "ONE\nmiddle\ntwo\n");
    let c = reread(&f, &c);
    let second = accept_changes(&f, &c, "a.md", "ONE\nmiddle\nTWO\n");

    // The later acceptance is in the way of the earlier one.
    assert!(tasks::undo(&f.store, &f.ws, &first.id).is_err());
    mutation(
        &f,
        ProjectAction::Undo {
            application_id: second.id,
        },
    );
    assert_eq!(
        fs::read_to_string(f.root.join("a.md")).unwrap(),
        "ONE\nmiddle\ntwo\n"
    );
    mutation(
        &f,
        ProjectAction::Undo {
            application_id: first.id,
        },
    );
    assert_eq!(
        fs::read_to_string(f.root.join("a.md")).unwrap(),
        "one\nmiddle\ntwo\n"
    );
    let c = reread(&f, &c);
    let body: Checkpoint = decode(&c).unwrap();
    assert_eq!(body.base["a.md"].hash, original_base.hash);
    assert_eq!(body.base["a.md"].size, original_base.size);

    // Undoing left nothing behind that blocks a fresh acceptance.
    mutation(
        &f,
        ProjectAction::Apply {
            checkpoint_id: c.id.clone(),
            paths: vec!["a.md".into(), "b.md".into()],
            expected_revision: c.revision,
        },
    );
    assert_eq!(
        fs::read_to_string(f.root.join("a.md")).unwrap(),
        "ONE\nmiddle\nTWO\n"
    );
}
#[test]
fn undoing_the_last_change_reopens_an_edit_finished_through_the_diff() {
    let f = fixture();
    let c = edit_with_two_hunks(&f);
    accept_changes(&f, &c, "b.md", "OTHER\n");
    let c = reread(&f, &c);
    let last = accept_changes(&f, &c, "a.md", "ONE\nmiddle\nTWO\n");
    let c = reread(&f, &c);
    assert_eq!(decode::<Checkpoint>(&c).unwrap().state, "accepted");
    mutation(
        &f,
        ProjectAction::Undo {
            application_id: last.id,
        },
    );
    let c = reread(&f, &c);
    assert_eq!(decode::<Checkpoint>(&c).unwrap().state, "review");
    assert_eq!(changed_paths(&c), ["a.md"]);
}
#[test]
fn accepted_changes_leave_the_rest_of_the_edit_open_to_discard_or_continue() {
    let f = fixture();
    let c = edit_with_two_hunks(&f);
    accept_changes(&f, &c, "a.md", "ONE\nmiddle\ntwo\n");
    let c = reread(&f, &c);
    assert!(session_task_root(&f.store, &f.ws, &c.id).is_ok());
    let discarded = mutation(
        &f,
        ProjectAction::Reject {
            checkpoint_id: c.id.clone(),
        },
    );
    assert_eq!(decode::<Checkpoint>(&discarded).unwrap().state, "rejected");
    assert_eq!(
        fs::read_to_string(f.root.join("a.md")).unwrap(),
        "ONE\nmiddle\ntwo\n"
    );
}
#[test]
fn an_interrupted_partial_acceptance_blocks_the_edit_until_recovered() {
    let f = fixture();
    let c = edit_with_two_hunks(&f);
    let body: Checkpoint = decode(&c).unwrap();
    let before = body.base["a.md"].hash.clone();
    let after = blob(&f.store, &f.ws, b"ONE\nmiddle\ntwo\n", "test").unwrap();
    let a = Application {
        checkpoint_id: c.id.clone(),
        root_identity: body.root_identity.clone(),
        state: "applying".into(),
        files: vec![AppliedFile {
            path: "a.md".into(),
            before,
            after: Some(after),
            before_executable: false,
            after_executable: false,
            claim: "crashed-0".into(),
            state: "pending".into(),
        }],
        direction: "accept_changes".into(),
        error: None,
        checks_valid: false,
    };
    put(&f.store, &f.ws, "crashed", "application", 0, &a).unwrap();
    assert!(tasks::accept_changes(
        &f.store,
        &f.ws,
        &c.id,
        "a.md",
        "ONE\nmiddle\ntwo\n",
        c.revision
    )
    .is_err());
    assert!(tasks::apply(&f.store, &f.ws, &c.id, vec!["b.md".into()], c.revision).is_err());
    tasks::recover(&f.store, &f.ws, "crashed").unwrap();
    accept_changes(&f, &reread(&f, &c), "a.md", "ONE\nmiddle\ntwo\n");
}
#[test]
fn partial_acceptance_refuses_stale_and_unchanged_requests() {
    let f = fixture();
    let c = edit_with_two_hunks(&f);
    let accept = |path: &str, content: &str, revision: i64| {
        tasks::accept_changes(&f.store, &f.ws, &c.id, path, content, revision)
            .unwrap_err()
            .message
    };
    assert!(accept("a.md", "x\n", c.revision + 1).contains("refresh"));
    assert!(accept("new.md", "x\n", c.revision).contains("changed file"));
    assert!(accept("a.md", "one\nmiddle\ntwo\n", c.revision).contains("Select a change"));
}

#[test]
fn an_acceptance_stopped_before_its_edit_was_updated_points_at_undo() {
    let f = fixture();
    let c = edit_with_two_hunks(&f);
    let applied = accept_changes(&f, &c, "a.md", "ONE\nmiddle\ntwo\n");
    // The state a crash leaves after the file and journal, before the edit.
    let current = reread(&f, &c);
    let mut stopped: Checkpoint = decode(&current).unwrap();
    let before: Checkpoint = decode(&c).unwrap();
    stopped.base = before.base;
    stopped.limitations = before.limitations;
    let c = put(
        &f.store,
        &f.ws,
        &c.id,
        "checkpoint",
        current.revision,
        &stopped,
    )
    .unwrap();

    let attempt = |f: &Fixture, c: &ProjectRecord| {
        mutate(
            &f.store,
            ProjectMutation {
                workspace_id: f.ws.clone(),
                operation_id: id("test").unwrap(),
                action: ProjectAction::AcceptChanges {
                    checkpoint_id: c.id.clone(),
                    path: "a.md".into(),
                    content: "ONE\nmiddle\nTWO\n".into(),
                    expected_revision: c.revision,
                },
            },
        )
    };
    let message = attempt(&f, &c).unwrap_err().message;
    assert!(message.contains("was interrupted"), "{message}");
    assert!(message.contains("Undo"), "{message}");
    let message = mutate(
        &f.store,
        ProjectMutation {
            workspace_id: f.ws.clone(),
            operation_id: id("test").unwrap(),
            action: ProjectAction::Apply {
                checkpoint_id: c.id.clone(),
                paths: vec!["a.md".into()],
                expected_revision: c.revision,
            },
        },
    )
    .unwrap_err()
    .message;
    assert!(message.contains("was interrupted"), "{message}");

    mutation(
        &f,
        ProjectAction::Undo {
            application_id: applied.id,
        },
    );
    let c = reread(&f, &c);
    attempt(&f, &c).unwrap();
    assert_eq!(
        fs::read_to_string(f.root.join("a.md")).unwrap(),
        "ONE\nmiddle\nTWO\n"
    );
}
