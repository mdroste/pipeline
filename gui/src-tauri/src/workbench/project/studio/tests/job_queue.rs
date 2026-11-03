//! A job in a project leaves the rest of that project usable: other jobs wait
//! their turn, and only the files a job declared are held back.
use super::*;

fn job(f: &Fixture, profile: research::ExecutionProfile) -> research::ResearchExecution {
    research::jobs::queue(
        &f.store,
        research::RunExecutionRequest {
            plan_id: None,
            profile_id: profile.id,
            session_id: None,
            test_only: true,
            operation_id: id("job").unwrap(),
        },
        "detached",
        None,
    )
    .unwrap()
}
async fn wait_until(mut done: impl FnMut() -> bool) {
    for _ in 0..400 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("The job did not reach the expected state");
}
fn project_action(store: &Store, ws: &str, action: ProjectAction) -> WorkbenchResult<Value> {
    mutate(
        store,
        ProjectMutation {
            workspace_id: ws.into(),
            operation_id: id("test").unwrap(),
            action,
        },
    )
}
fn outcome(f: &Fixture, id: &str) -> String {
    execution(&f.store, &f.ws, id).unwrap().outcome
}
/// Runs until the test writes `go`, so it holds the project for as long as needed.
const HOLD: &str =
    "printf started > first.out; while [ ! -e go ]; do sleep 0.02; done; printf done >> first.out";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_job_waits_while_undeclared_work_continues_and_declared_files_stay_held() {
    let _gate = JOB_TEST_GATE.lock().await;
    let f = fixture();
    fs::write(f.root.join("input.txt"), "data").unwrap();
    fs::write(f.root.join("notes.md"), "notes").unwrap();
    let first = profile(&f, HOLD, vec!["input.txt".into()], vec!["first.out".into()]);
    let second = profile(
        &f,
        "cat first.out > second.out",
        vec![],
        vec!["second.out".into()],
    );
    let copy = {
        let task = project_action(
            &f.store,
            &f.ws,
            ProjectAction::CreateTask {
                objective: "Edit the declared input".into(),
                anchor_id: None,
                expected_outputs: vec![],
                expected_checks: vec![],
            },
        )
        .unwrap();
        project_action(
            &f.store,
            &f.ws,
            ProjectAction::Checkpoint {
                task_id: task["id"].as_str().unwrap().into(),
                paths: vec!["input.txt".into()],
                backend: "copy".into(),
            },
        )
        .unwrap()
    };
    let copy_id = copy["id"].as_str().unwrap().to_owned();
    fs::write(
        Path::new(copy["body"]["taskRoot"].as_str().unwrap()).join("input.txt"),
        "edited",
    )
    .unwrap();
    let captured = project_action(
        &f.store,
        &f.ws,
        ProjectAction::CaptureChanges {
            checkpoint_id: copy_id.clone(),
        },
    )
    .unwrap();

    let a = job(&f, first);
    let b = job(&f, second);
    assert_eq!(
        (a.outcome.as_str(), b.outcome.as_str()),
        ("queued", "queued")
    );
    research::jobs::launch_pending();
    wait_until(|| f.root.join("first.out").exists()).await;
    wait_until(|| outcome(&f, &a.id) == "running").await;

    let opened = editor_file(&f.store, &f.ws, None, "notes.md").unwrap();
    act(
        &f,
        StudioAction::SaveText {
            checkpoint_id: None,
            path: "notes.md".into(),
            expected_hash: opened.hash,
            content: "saved during a job".into(),
        },
    );
    assert_eq!(
        fs::read_to_string(f.root.join("notes.md")).unwrap(),
        "saved during a job"
    );
    project_action(
        &f.store,
        &f.ws,
        ProjectAction::CreateTask {
            objective: "Created during a job".into(),
            anchor_id: None,
            expected_outputs: vec![],
            expected_checks: vec![],
        },
    )
    .unwrap();
    assert_eq!(
        program_file_hash(&f.store, &f.ws, "input.txt").unwrap(),
        Some(crate::workbench::desk::hash(b"data"))
    );

    let opened = editor_file(&f.store, &f.ws, None, "input.txt").unwrap();
    let refused = studio_mutate(
        &f.store,
        StudioMutation {
            workspace_id: f.ws.clone(),
            operation_id: id("test").unwrap(),
            action: StudioAction::SaveText {
                checkpoint_id: None,
                path: "input.txt".into(),
                expected_hash: opened.hash,
                content: "changed under the job".into(),
            },
        },
    )
    .unwrap_err();
    assert_eq!(
        refused.message,
        "The local job \"fixture\" is using input.txt. Wait for it to finish or stop it, then try again."
    );
    let accept = ProjectAction::Apply {
        checkpoint_id: copy_id.clone(),
        paths: vec!["input.txt".into()],
        expected_revision: captured["revision"].as_i64().unwrap(),
    };
    let refused = project_action(&f.store, &f.ws, accept.clone()).unwrap_err();
    assert!(
        refused.message.contains("is using input.txt"),
        "{refused:?}"
    );
    assert_eq!(
        fs::read_to_string(f.root.join("input.txt")).unwrap(),
        "data"
    );
    assert_eq!(outcome(&f, &b.id), "queued");
    assert!(!f.root.join("second.out").exists());

    fs::write(f.root.join("go"), "").unwrap();
    wait_until(|| !research::jobs::workspace_active(&f.ws)).await;
    assert_eq!(outcome(&f, &a.id), "completed");
    assert_eq!(outcome(&f, &b.id), "completed");
    assert_eq!(
        fs::read_to_string(f.root.join("second.out")).unwrap(),
        "starteddone",
        "the second job started only after the first finished"
    );
    project_action(&f.store, &f.ws, accept).unwrap();
    assert_eq!(
        fs::read_to_string(f.root.join("input.txt")).unwrap(),
        "edited"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stopping_a_waiting_job_ends_it_now_and_the_next_job_still_waits() {
    let _gate = JOB_TEST_GATE.lock().await;
    let f = fixture();
    let first = job(&f, profile(&f, HOLD, vec![], vec!["first.out".into()]));
    let stopped = job(&f, profile(&f, "printf ran > stopped.out", vec![], vec![]));
    let third = job(&f, profile(&f, "cat first.out > third.out", vec![], vec![]));
    research::jobs::launch_pending();
    wait_until(|| f.root.join("first.out").exists()).await;
    research::cancel_execution(
        &f.store,
        research::CancelExecutionRequest {
            execution_id: stopped.id.clone(),
        },
    )
    .unwrap();
    wait_until(|| outcome(&f, &stopped.id) == "interrupted").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(outcome(&f, &third.id), "queued");
    assert!(!f.root.join("third.out").exists());
    fs::write(f.root.join("go"), "").unwrap();
    wait_until(|| !research::jobs::workspace_active(&f.ws)).await;
    assert_eq!(outcome(&f, &first.id), "completed");
    assert!(!f.root.join("stopped.out").exists());
    assert_eq!(
        fs::read_to_string(f.root.join("third.out")).unwrap(),
        "starteddone"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn git_actions_wait_for_a_local_job_unless_they_stay_clear_of_its_files() {
    use crate::workbench::project::version_control::{claim_project, Action};
    let _gate = JOB_TEST_GATE.lock().await;
    let f = fixture();
    fs::write(f.root.join("input.txt"), "data").unwrap();
    let running = job(
        &f,
        profile(&f, HOLD, vec!["input.txt".into()], vec!["first.out".into()]),
    );
    research::jobs::launch_pending();
    wait_until(|| f.root.join("first.out").exists()).await;
    let action = |value: Value| serde_json::from_value::<Action>(value).unwrap();
    let claim = |value: Value| claim_project(&f.store, &f.ws, &f.root, &action(value));

    for whole_project in [
        json!({ "kind": "discard", "paths": ["notes.md"] }),
        json!({ "kind": "undoCommit" }),
        json!({ "kind": "switchBranch", "name": "other" }),
        json!({ "kind": "pull" }),
        json!({ "kind": "resolve", "path": "notes.md", "choice": "theirs" }),
        json!({ "kind": "abortOperation" }),
    ] {
        let refused = claim(whole_project.clone()).unwrap_err();
        assert!(
            refused
                .message
                .contains("\"fixture\" is using this project's files"),
            "{whole_project}: {refused:?}"
        );
    }
    let half_written = json!({
        "kind": "commit", "paths": ["notes.md", "first.out"], "summary": "s", "description": null,
    });
    let refused = claim(half_written).unwrap_err();
    assert!(
        refused.message.contains("is using first.out"),
        "{refused:?}"
    );

    drop(
        claim(json!({
            "kind": "commit", "paths": ["notes.md"], "summary": "s", "description": null,
        }))
        .expect("a commit clear of the job's files goes ahead"),
    );
    drop(claim(json!({ "kind": "createBranch", "name": "other" })).unwrap());
    let refused = claim(json!({ "kind": "ignore", "path": "first.out" })).unwrap_err();
    assert!(
        refused.message.contains("is using first.out"),
        "{refused:?}"
    );

    fs::write(f.root.join("go"), "").unwrap();
    wait_until(|| !research::jobs::workspace_active(&f.ws)).await;
    assert_eq!(outcome(&f, &running.id), "completed");
    claim(json!({ "kind": "switchBranch", "name": "other" }))
        .expect("the project is free once the job ends");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_gets_no_credit_for_an_output_the_job_ahead_of_it_wrote() {
    let _gate = JOB_TEST_GATE.lock().await;
    let f = fixture();
    fs::write(f.root.join("shared.out"), "old").unwrap();
    let writer = job(
        &f,
        profile(
            &f,
            "printf new > shared.out",
            vec![],
            vec!["shared.out".into()],
        ),
    );
    let idle = job(&f, profile(&f, "exit 0", vec![], vec!["shared.out".into()]));
    research::jobs::launch_pending();
    wait_until(|| !research::jobs::workspace_active(&f.ws)).await;
    assert_eq!(outcome(&f, &writer.id), "completed");
    let idle = execution(&f.store, &f.ws, &idle.id).unwrap();
    assert_eq!(idle.outcome, "failed");
    assert_eq!(
        idle.validation["unchangedPreexistingOutputs"],
        json!(["shared.out"])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_writer_never_finds_the_project_free_while_a_job_is_being_queued() {
    let _gate = JOB_TEST_GATE.lock().await;
    let f = fixture();
    let p = profile(&f, "exit 0", vec![], vec![]);
    let (store, ws) = (f.store.clone(), f.ws.clone());
    let checked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let saw = checked.clone();
    research::jobs::BEFORE_LOCK_RELEASE.with(|probe| {
        probe.set(Some(Box::new(move || {
            assert!(
                lock(&store, &ws).is_err(),
                "the project stays locked until the job is listed"
            );
            assert!(research::jobs::ensure_project_free(&ws).is_err());
            saw.store(true, std::sync::atomic::Ordering::SeqCst);
        })));
    });
    let queued = job(&f, p);
    assert!(checked.load(std::sync::atomic::Ordering::SeqCst));
    assert!(lock(&f.store, &f.ws).is_ok(), "released once listed");
    research::jobs::launch_pending();
    wait_until(|| !research::jobs::workspace_active(&f.ws)).await;
    assert_eq!(outcome(&f, &queued.id), "completed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_that_fails_while_being_queued_is_never_listed_and_frees_the_project() {
    let _gate = JOB_TEST_GATE.lock().await;
    let f = fixture();
    let p = profile(&f, "exit 0", vec![], vec![]);
    let connection = f.store.connection().unwrap();
    connection
        .execute_batch("CREATE TRIGGER refuse_job BEFORE INSERT ON execution_jobs BEGIN SELECT RAISE(ABORT,'refused'); END;")
        .unwrap();
    let request = |p: &research::ExecutionProfile| research::RunExecutionRequest {
        plan_id: None,
        profile_id: p.id.clone(),
        session_id: None,
        test_only: true,
        operation_id: id("job").unwrap(),
    };
    assert!(research::jobs::queue(&f.store, request(&p), "detached", None).is_err());
    assert!(!research::jobs::workspace_active(&f.ws));
    assert!(lock(&f.store, &f.ws).is_ok());
    connection
        .execute_batch("DROP TRIGGER refuse_job;")
        .unwrap();
    let queued = job(&f, p);
    research::jobs::launch_pending();
    wait_until(|| !research::jobs::workspace_active(&f.ws)).await;
    assert_eq!(outcome(&f, &queued.id), "completed");
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "Runs local latexmk against a disposable manuscript"]
fn live_latexmk_rebuild_with_nothing_changed_is_a_finished_build() {
    let f = fixture();
    fs::write(
        f.root.join("main.tex"),
        "\\documentclass{article}\n\\begin{document}\nA retained result.\n\\end{document}\n",
    )
    .unwrap();
    let config = BuildConfig {
        name: "Paper".into(),
        checkpoint_id: None,
        directory: ".".into(),
        root_document: "main.tex".into(),
        expected_pdf: "main.pdf".into(),
        engine: "latexmk".into(),
        timeout_seconds: 60,
        inputs: vec!["main.tex".into()],
        advanced_argv: None,
    };
    let record = save_build(&f.store, &f.ws, None, 0, config).unwrap();
    let profile: research::ExecutionProfile =
        serde_json::from_value(record.body["profile"].clone()).unwrap();
    let build = || {
        let preview = research::preview_host_execution(&f.store, &profile.id).unwrap();
        research::authorize_host_execution(&f.store, &profile.id, &preview.fingerprint).unwrap();
        research::run_execution(
            &f.store,
            research::RunExecutionRequest {
                plan_id: None,
                profile_id: profile.id.clone(),
                session_id: None,
                test_only: true,
                operation_id: id("build").unwrap(),
            },
        )
        .unwrap()
    };
    let first = build();
    assert_eq!(first.outcome, "completed", "{first:?}");
    let second = build();
    assert_eq!(second.outcome, "completed", "{:?}", second.validation);
    assert_eq!(
        second.validation["upToDateOutputs"],
        json!(["main.pdf", "main.log", "main.synctex.gz"])
    );
    let receipt = inspect_build(&f.store, &f.ws, &second.id).unwrap();
    assert!(receipt["pdf"]["revision"]["id"].is_string(), "{receipt}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn staging_a_publication_writes_nothing_into_a_copy_a_job_works_in() {
    let _gate = JOB_TEST_GATE.lock().await;
    let f = fixture();
    fs::write(f.root.join("paper.md"), "text").unwrap();
    let task = project_action(
        &f.store,
        &f.ws,
        ProjectAction::CreateTask {
            objective: "Prepare the publication".into(),
            anchor_id: None,
            expected_outputs: vec![],
            expected_checks: vec![],
        },
    )
    .unwrap();
    let copy = project_action(
        &f.store,
        &f.ws,
        ProjectAction::Checkpoint {
            task_id: task["id"].as_str().unwrap().into(),
            paths: vec!["paper.md".into()],
            backend: "copy".into(),
        },
    )
    .unwrap();
    let copy_id = copy["id"].as_str().unwrap().to_owned();
    let task_root = PathBuf::from(copy["body"]["taskRoot"].as_str().unwrap());
    let inside = research::save_execution_profile(
        &f.store,
        research::SaveExecutionProfileRequest {
            profile_id: None,
            workspace_id: f.ws.clone(),
            name: "fixture".into(),
            adapter: "command".into(),
            argv: vec!["/bin/sh".into(), "-c".into(), HOLD.into()],
            cwd: task_root.to_string_lossy().into_owned(),
            environment: json!({}),
            inputs: vec![],
            timeout_seconds: 10,
            outputs: vec!["first.out".into()],
            expected_revision: None,
            operation_id: id("profile").unwrap(),
        },
    )
    .unwrap();
    let preview = research::preview_host_execution(&f.store, &inside.id).unwrap();
    research::authorize_host_execution(&f.store, &inside.id, &preview.fingerprint).unwrap();
    let running = job(&f, inside);
    research::jobs::launch_pending();
    wait_until(|| task_root.join("first.out").exists()).await;

    let refused = stage_publication(&f.store, &f.ws, &copy_id, "out.md", None, b"staged");
    let message = refused.unwrap_err().message;
    assert!(message.contains("this working copy"), "{message}");
    assert!(!task_root.join("out.md").exists());

    fs::write(task_root.join("go"), "").unwrap();
    wait_until(|| !research::jobs::workspace_active(&f.ws)).await;
    assert_eq!(outcome(&f, &running.id), "completed");
    stage_publication(&f.store, &f.ws, &copy_id, "out.md", None, b"staged").unwrap();
    assert_eq!(fs::read(task_root.join("out.md")).unwrap(), b"staged");
}

fn journal(f: &Fixture, id: &str) -> Option<String> {
    f.store
        .connection()
        .unwrap()
        .query_row(
            "SELECT finalization_json FROM execution_jobs WHERE execution_id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn a_job_left_by_an_earlier_launch_is_marked_at_startup_and_reconciling_clears_its_journal() {
    let f = fixture();
    let (e, _) = exported_result(&f, 1., "units");
    let c = f.store.connection().unwrap();
    c.execute("INSERT INTO execution_jobs(execution_id,operation_id,request_hash,owner,process_instance,created_at,finalization_json) VALUES(?1,'operation','hash','detached','previous-instance','now',?2)",params![e.id,json!({"outcome":e.outcome,"endedAt":e.ended_at,"exitStatus":e.exit_status,"stdout":e.stdout,"stderr":e.stderr,"outputs":e.output_manifest,"validation":e.validation}).to_string()]).unwrap();
    c.execute(
        "UPDATE research_executions SET outcome='running' WHERE id=?1",
        [&e.id],
    )
    .unwrap();

    research::jobs::reconcile_abandoned(&f.store).unwrap();
    assert_eq!(outcome(&f, &e.id), "outcome_unknown");
    assert!(journal(&f, &e.id).is_some());

    let restored = research::jobs::reconcile_job(&f.store, &f.ws, &e.id).unwrap();
    assert_eq!(restored.outcome, "completed");
    assert_eq!(restored.stdout, e.stdout);
    assert_eq!(journal(&f, &e.id), None);
}

#[test]
fn a_job_of_this_launch_is_left_alone_at_startup() {
    let f = fixture();
    let (e, _) = exported_result(&f, 1., "units");
    let c = f.store.connection().unwrap();
    c.execute("INSERT INTO execution_jobs(execution_id,operation_id,request_hash,owner,process_instance,created_at) VALUES(?1,'operation','hash','detached',?2,'now')",params![e.id,research::jobs::process_instance()]).unwrap();
    c.execute(
        "UPDATE research_executions SET outcome='running',validation_json=json_set(validation_json,'$.processInstance',?2) WHERE id=?1",
        params![e.id, research::jobs::process_instance()],
    )
    .unwrap();
    research::jobs::reconcile_abandoned(&f.store).unwrap();
    assert_eq!(outcome(&f, &e.id), "running");
}

fn finished_run(f: &Fixture, command: &str) -> research::ResearchExecution {
    let p = profile(f, command, vec![], vec![]);
    research::run_execution(
        &f.store,
        research::RunExecutionRequest {
            plan_id: None,
            profile_id: p.id,
            session_id: None,
            test_only: true,
            operation_id: id("run").unwrap(),
        },
    )
    .unwrap()
}
const PAGE: usize = 64 * 1024;

#[test]
fn a_saved_log_pages_on_whole_characters() {
    let f = fixture();
    // The two bytes of the accent fall on either side of the page limit.
    let e = finished_run(
        &f,
        r"head -c 65535 /dev/zero | tr '\0' a; printf '\303\251 after'",
    );
    let page = |offset| research::jobs::log_page(&f.store, &f.ws, &e.id, "stdout", offset).unwrap();
    let first = page(0);
    assert_eq!(first.next_offset, PAGE - 1);
    assert!(!first.text.contains(char::REPLACEMENT_CHARACTER));
    let second = page(first.next_offset);
    assert_eq!(second.text, "é after");
    // An offset from the live log can land inside a character of the saved text.
    assert_eq!(page(PAGE).text, " after");
}

#[test]
fn a_live_log_holds_back_a_character_still_arriving() {
    let f = fixture();
    let e = finished_run(&f, "exit 0");
    let page = |offset| research::jobs::log_page(&f.store, &f.ws, &e.id, "stdout", offset).unwrap();
    let euro = "€".as_bytes();
    research::jobs::append_log(&e.id, "stdout", b"ab");
    research::jobs::append_log(&e.id, "stdout", &euro[..2]);
    let first = page(0);
    assert_eq!((first.text.as_str(), first.next_offset), ("ab", 2));
    research::jobs::append_log(&e.id, "stdout", &euro[2..]);
    let second = page(first.next_offset);
    assert_eq!((second.text.as_str(), second.next_offset), ("€", 5));
}

/// A job row owned by another instance, in the given state.
fn job_of(f: &Fixture, operation: &str, instance: &str, state: &str) -> String {
    let (e, _) = exported_result(f, 1., operation);
    let c = f.store.connection().unwrap();
    c.execute("INSERT INTO execution_jobs(execution_id,operation_id,request_hash,owner,process_instance,created_at) VALUES(?1,?2,'hash','detached',?3,'now')",params![e.id,operation,instance]).unwrap();
    c.execute(
        "UPDATE research_executions SET outcome=?2 WHERE id=?1",
        params![e.id, state],
    )
    .unwrap();
    e.id
}
/// The lock another open Pipeline would hold on its own file.
fn alive_elsewhere(f: &Fixture, instance: &str) -> (PathBuf, fs::File) {
    use fs2::FileExt as _;
    let path = research::jobs::instance_lock_path(&f.store, instance).unwrap();
    let file = fs::File::create(&path).unwrap();
    file.try_lock_exclusive().unwrap();
    (path, file)
}

#[test]
fn jobs_of_another_open_pipeline_are_left_alone_until_it_ends() {
    let f = fixture();
    let (path, held) = alive_elsewhere(&f, "instance_other");
    let running = job_of(&f, "running", "instance_other", "running");
    let queued = job_of(&f, "queued", "instance_other", "queued");

    research::jobs::reconcile_abandoned(&f.store).unwrap();
    research::jobs::list_jobs(&f.store, &f.ws).unwrap();
    research::jobs::complete_tool_result(&f.store, &running).unwrap();
    assert_eq!(outcome(&f, &running), "running");
    assert_eq!(outcome(&f, &queued), "queued");
    assert!(research::jobs::instance_alive(&f.store, "instance_other"));

    drop(held);
    research::jobs::reconcile_abandoned(&f.store).unwrap();
    assert_eq!(outcome(&f, &running), "outcome_unknown");
    assert_eq!(outcome(&f, &queued), "outcome_unknown");
    assert!(!path.exists());
    assert!(!research::jobs::instance_alive(&f.store, "instance_other"));
}

#[test]
fn a_saved_instance_id_never_reaches_a_file_outside_the_store() {
    let f = fixture();
    let outside = f.store.root_path().with_file_name("outside.lock");
    fs::write(&outside, "kept").unwrap();
    let id = job_of(&f, "crafted", "../outside", "running");
    assert_eq!(
        research::jobs::instance_lock_path(&f.store, "../outside"),
        None
    );
    research::jobs::reconcile_abandoned(&f.store).unwrap();
    assert_eq!(outcome(&f, &id), "outcome_unknown");
    assert_eq!(fs::read_to_string(&outside).unwrap(), "kept");
}

#[test]
fn queueing_a_job_marks_this_process_alive_to_a_second_launch() {
    use fs2::FileExt as _;
    let f = fixture();
    research::jobs::hold_instance(&f.store).unwrap();
    research::jobs::hold_instance(&f.store).unwrap();
    let path =
        research::jobs::instance_lock_path(&f.store, research::jobs::process_instance()).unwrap();
    assert!(fs::File::open(&path).unwrap().try_lock_exclusive().is_err());
}
