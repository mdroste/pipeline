use super::*;

pub(super) fn coordinator_fixture() -> (tempfile::TempDir, Arc<Coordinator>) {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path()).unwrap();
    let owner = std::fs::File::create(temp.path().join("owner.lock")).unwrap();
    let manager = Arc::new(Coordinator {
        store,
        gate: tokio::sync::Mutex::new(()),
        wake: Notify::new(),
        active: Mutex::new(HashMap::new()),
        stopping: AtomicBool::new(false),
        background: AtomicBool::new(false),
        _owner: owner,
        app: None,
    });
    (temp, manager)
}
async fn drive(manager: &Arc<Coordinator>, id: &str, state: &str) -> TaskRun {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            manager.tick().await.unwrap();
            let run = manager.store.get(id).unwrap();
            if run.state == state {
                return run;
            }
            if run.state == "attention" {
                panic!("{:?}", run.reason);
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}
#[tokio::test]
async fn recovery_adopts_the_committed_result_before_inspecting_child_artifacts() {
    let (_temp, m) = coordinator_fixture();
    let mut run = m
        .store
        .create(
            "recover-result",
            chain(vec![delay("step")]),
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    let Advance::Ready(ready) = state::advance(&run.chain, &mut run.progress, &run.inputs) else {
        panic!()
    };
    done(&mut run.progress, &ready[0], json!({}));
    let receipt = run.progress.receipts.get_mut("step").unwrap();
    receipt.state = "unknown".into();
    let path = m.store.root.join("actions").join(&receipt.operation);
    adapters::atomic_json(
        &path.join("result.json"),
        &json!({"answer":"Recorded result"}),
    )
    .unwrap();
    std::fs::write(path.join("review-input.json"), "damaged child metadata").unwrap();
    m.recover_results(&mut run).await.unwrap();
    assert_eq!(run.progress.receipts["step"].state, "completed");
    assert_eq!(run.progress.outputs["step"]["answer"], "Recorded result");
}
#[tokio::test]
async fn coordinator_wait_signal_snapshot_and_finish_use_real_store_and_artifacts() {
    let (_temp, m) = coordinator_fixture();
    let chain = chain(vec![
        step(
            "input",
            Action::Input {
                prompt: "Supply paper".into(),
                timeout_seconds: None,
            },
        ),
        step(
            "paper",
            Action::Snapshot {
                input: output("input", ""),
                filename: "paper.md".into(),
                require_change: false,
            },
        ),
    ]);
    let run = m
        .store
        .create("flow", chain, Scope::default(), json!({}), now(), true)
        .unwrap();
    let waiting = drive(&m, &run.id, "waiting").await;
    assert!(m.active.lock().unwrap().is_empty());
    assert_eq!(waiting.progress.actions, 1);
    m.store
        .signal(&run.id, "input", "input-op", json!("A complete paper"))
        .unwrap();
    let finished = drive(&m, &run.id, "finished").await;
    assert_eq!(finished.progress.actions, 2);
    assert_eq!(finished.progress.receipts.len(), 2);
    let artifact = &finished.progress.outputs["paper"];
    assert_eq!(
        std::fs::read_to_string(adapters::artifact_path(&m.store, artifact).unwrap()).unwrap(),
        "A complete paper"
    );
    let receipt = &finished.progress.receipts["paper"];
    assert!(m
        .store
        .root
        .join("actions")
        .join(&receipt.operation)
        .join("result.json")
        .is_file());
}
#[tokio::test]
async fn coordinator_parallel_waits_release_all_slots_and_join_once() {
    let (_temp, m) = coordinator_fixture();
    let c = chain(vec![
        step(
            "parallel",
            Action::Parallel {
                branches: vec![
                    vec![step("zero", Action::Delay { seconds: 0 })],
                    vec![step(
                        "answer",
                        Action::Input {
                            prompt: "Continue?".into(),
                            timeout_seconds: None,
                        },
                    )],
                ],
            },
        ),
        step(
            "result",
            Action::Snapshot {
                input: output("answer", ""),
                filename: "answer.md".into(),
                require_change: false,
            },
        ),
    ]);
    let r = m
        .store
        .create("parallel-flow", c, Scope::default(), json!({}), now(), true)
        .unwrap();
    m.tick().await.unwrap();
    m.tick().await.unwrap();
    let waiting = m.store.get(&r.id).unwrap();
    assert_eq!(waiting.state, "waiting");
    assert_eq!(
        waiting.progress.receipts["parallel/0/zero"].state,
        "completed"
    );
    assert!(m.active.lock().unwrap().is_empty());
    m.store
        .signal(&r.id, "parallel/1/answer", "signal", json!("Yes"))
        .unwrap();
    let done = drive(&m, &r.id, "finished").await;
    assert_eq!(done.progress.actions, 3);
}
#[tokio::test]
async fn paused_timer_does_not_advance_until_resumed() {
    let (_temp, m) = coordinator_fixture();
    let mut r = m
        .store
        .create(
            "paused-timer",
            chain(vec![step("wait", Action::Delay { seconds: 0 })]),
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    m.tick().await.unwrap();
    r = m.store.get(&r.id).unwrap();
    r.state = "paused".into();
    m.store.save(&mut r, "pause", "Paused").unwrap();
    m.tick().await.unwrap();
    assert_eq!(
        m.store.get(&r.id).unwrap().progress.receipts["wait"].state,
        "timer"
    );
    r.state = "queued".into();
    r.due_at = Some(now());
    m.store.save(&mut r, "resume", "Resumed").unwrap();
    assert_eq!(drive(&m, &r.id, "finished").await.progress.actions, 1);
}

#[tokio::test]
async fn context_overflow_before_dispatch_enters_attention_without_starting_child() {
    let (_temp, m) = coordinator_fixture();
    let mut run = m
        .store
        .create(
            "budget",
            chain(vec![step(
                "snapshot",
                Action::Snapshot {
                    input: Binding::Literal {
                        value: json!("unused"),
                    },
                    filename: "out.md".into(),
                    require_change: false,
                },
            )]),
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    run.progress.outputs = json!({"large":""});
    let overhead = serde_json::to_vec(&run.progress).unwrap().len();
    run.progress.outputs["large"] = json!("x".repeat(state::MAX_PROGRESS_BYTES - overhead - 1));
    m.store.save(&mut run, "fixture", "Near budget").unwrap();
    m.tick().await.unwrap();
    let saved = m.store.get(&run.id).unwrap();
    assert_eq!(saved.state, "attention");
    assert!(saved.progress.receipts.is_empty());
    assert!(m.active.lock().unwrap().is_empty());
    assert!(state::context_within_budget(&saved.progress));
}
#[tokio::test]
async fn oversized_progress_never_replaces_the_durable_record() {
    let (_temp, m) = coordinator_fixture();
    let mut run = m
        .store
        .create(
            "oversize",
            chain(vec![delay("wait")]),
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    let revision = run.revision;
    let original_outputs = run.progress.outputs.clone();
    run.progress.outputs = json!({"large":"x".repeat(state::MAX_PROGRESS_BYTES)});
    assert!(m.store.save(&mut run, "test", "Oversized").is_err());
    let saved = m.store.get(&run.id).unwrap();
    assert_eq!(saved.revision, revision);
    assert_eq!(saved.progress.outputs, original_outputs);
    m.save(&mut run, "stepCompleted", "Oversized result")
        .await
        .unwrap();
    assert_eq!(run.state, "attention");
    assert!(state::context_within_budget(&run.progress));
}

#[tokio::test]
async fn abandonment_is_recoverable_from_stop_and_retry_records() {
    let (_temp, m) = coordinator_fixture();
    let mut run = m
        .store
        .create(
            "abandoned",
            chain(vec![delay("work")]),
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    let Advance::Ready(ready) = state::advance(&run.chain, &mut run.progress, &run.inputs) else {
        panic!()
    };
    done(&mut run.progress, &ready[0], json!({}));
    let operation = run.progress.receipts["work"].operation.clone();
    run.state = "attention".into();
    run.progress.receipts.get_mut("work").unwrap().state = "unknown".into();
    m.store.save(&mut run, "fixture", "uncertain").unwrap();
    assert!(m.store.abandoned_operations().unwrap().is_empty());
    m.store.retry(&mut run).unwrap();
    assert_eq!(m.store.abandoned_operations().unwrap(), vec![operation]);
    let mut run = m
        .store
        .create(
            "stopped",
            chain(vec![delay("wait")]),
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    let Advance::Ready(ready) = state::advance(&run.chain, &mut run.progress, &run.inputs) else {
        panic!()
    };
    done(&mut run.progress, &ready[0], json!({}));
    let operation = run.progress.receipts["wait"].operation.clone();
    run.state = "cancelled".into();
    m.store.save(&mut run, "stop", "stopped").unwrap();
    assert!(m.store.abandoned_operations().unwrap().contains(&operation));
}

#[tokio::test]
async fn terminal_workspace_failure_is_retryable_and_recoverable() {
    let (_temp, m) = coordinator_fixture();
    let mut run = m
        .store
        .create(
            "failure",
            chain(vec![delay("step")]),
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    let Advance::Ready(ready) = state::advance(&run.chain, &mut run.progress, &run.inputs) else {
        panic!()
    };
    done(&mut run.progress, &ready[0], json!({}));
    run.progress.receipts.get_mut("step").unwrap().state = "running".into();
    m.save(&mut run, "fixture", "fixture").await.unwrap();
    m.complete(&run.id, "step", Ok(json!({"state":"failed"})))
        .await
        .unwrap();
    let run = m.store.get(&run.id).unwrap();
    assert_eq!(run.progress.receipts["step"].state, "failed");
    assert_eq!(run.state, "attention");
    let mut recovered = run.clone();
    let receipt = recovered.progress.receipts.get_mut("step").unwrap();
    receipt.state = "unknown".into();
    adapters::atomic_json(
        &m.store
            .root
            .join("actions")
            .join(&receipt.operation)
            .join("result.json"),
        &json!({"state":"interrupted"}),
    )
    .unwrap();
    m.recover_results(&mut recovered).await.unwrap();
    assert_eq!(recovered.progress.receipts["step"].state, "failed");
}

#[tokio::test]
async fn a_wait_past_its_deadline_takes_late_input_and_finishes() {
    let (_temp, m) = coordinator_fixture();
    let c = chain(vec![step(
        "ask",
        Action::Input {
            prompt: "Supply the data".into(),
            timeout_seconds: None,
        },
    )]);
    let run = m
        .store
        .create("expire", c, Scope::default(), json!({}), now(), true)
        .unwrap();
    let mut waiting = drive(&m, &run.id, "waiting").await;
    waiting.deadline_at = now() - 1;
    m.store
        .save(&mut waiting, "fixture", "Past deadline")
        .unwrap();
    m.tick().await.unwrap();
    let expired = m.store.get(&run.id).unwrap();
    assert_eq!(expired.state, "attention");
    assert_eq!(expired.progress.receipts["ask"].state, "input");
    m.store
        .signal(&run.id, "ask", "late", json!("Data"))
        .unwrap();
    let finished = drive(&m, &run.id, "finished").await;
    assert_eq!(finished.progress.outputs["ask"], "Data");
}

/// Old finished runs with the given journal and blob, opened or not.
pub(super) fn finished_run(
    m: &Coordinator,
    name: &str,
    seen: bool,
    age: i64,
    blob: &str,
) -> TaskRun {
    let mut run = m
        .store
        .create(
            name,
            chain(vec![delay("step")]),
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    let operation = store::id();
    let digest = blob.to_string();
    let hash = digest.clone();
    run.progress.receipts.insert(
        "step".into(),
        Receipt {
            sequence: 1,
            address: "step".into(),
            step_id: "step".into(),
            label: "Step".into(),
            state: "completed".into(),
            operation: operation.clone(),
            started_at: 0,
            finished_at: Some(1),
            wake_at: None,
            output: Some(json!({"hash":hash})),
            error: None,
            child: None,
        },
    );
    run.state = "finished".into();
    m.store.save(&mut run, "finished", "Done").unwrap();
    let journal = m.store.root.join("actions").join(&operation);
    adapters::atomic_json(&journal.join("result.json"), &json!({"hash":digest})).unwrap();
    if seen {
        m.store.mark_seen(&run.id).unwrap();
    }
    m.store
        .connection()
        .unwrap()
        .execute(
            "UPDATE runs SET updated_at=?2 WHERE id=?1",
            rusqlite::params![run.id, now() - age],
        )
        .unwrap();
    run
}

#[cfg(unix)]
fn old_blob(m: &Coordinator, digest: &str) -> std::path::PathBuf {
    let dir = m.store.root.join("blobs").join(digest);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("paper.md"), "A paper").unwrap();
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3 * 86400);
    std::fs::File::open(&dir)
        .unwrap()
        .set_modified(old)
        .unwrap();
    dir
}

#[cfg(unix)]
#[tokio::test]
async fn retention_removes_only_opened_old_runs_and_blobs_nothing_else_names() {
    const DAY: i64 = 86_400;
    let (_temp, m) = coordinator_fixture();
    let digest = |c: char| c.to_string().repeat(64);
    let (shared, kept, removed) = (digest('a'), digest('b'), digest('c'));
    let shared_blob = old_blob(&m, &shared);
    let kept_blob = old_blob(&m, &kept);
    let removed_blob = old_blob(&m, &removed);
    let tree_blob = old_blob(&m, &format!("tree-{}", digest('d')));

    let opened = finished_run(&m, "opened", true, 40 * DAY, &removed);
    let sharing = finished_run(&m, "sharing", true, 40 * DAY, &shared);
    let unseen = finished_run(&m, "unseen", false, 40 * DAY, &kept);
    let recent = finished_run(&m, "recent", true, DAY, &shared);
    let journal = m
        .store
        .root
        .join("actions")
        .join(&opened.progress.receipts["step"].operation);
    assert!(journal.is_dir());

    m.tick().await.unwrap();

    assert!(m.store.get(&opened.id).is_err());
    assert!(m.store.get(&sharing.id).is_err());
    assert!(m.store.get(&unseen.id).is_ok(), "unseen result kept");
    assert!(m.store.get(&recent.id).is_ok(), "recent result kept");
    assert!(!journal.exists());
    assert!(shared_blob.is_dir(), "a surviving run still names this");
    assert!(kept_blob.is_dir());
    assert!(!removed_blob.exists());
    assert!(!tree_blob.exists());
    assert!(m.store.events(&opened.id, 0).unwrap().is_empty());
}

#[tokio::test]
async fn retention_waits_while_work_is_in_progress_and_keeps_a_new_blob() {
    const DAY: i64 = 86_400;
    let (_temp, m) = coordinator_fixture();
    let run = finished_run(&m, "opened", true, 40 * DAY, &"e".repeat(64));
    let fresh = m.store.root.join("blobs").join("f".repeat(64));
    std::fs::create_dir_all(&fresh).unwrap();
    let (stop, _) = watch::channel(false);
    m.active
        .lock()
        .unwrap()
        .insert("other/step".into(), ("other".into(), stop));
    m.tick().await.unwrap();
    assert!(m.store.get(&run.id).is_ok());
    m.active.lock().unwrap().clear();
    m.tick().await.unwrap();
    assert!(m.store.get(&run.id).is_err());
    assert!(fresh.is_dir());
}

#[tokio::test]
async fn retention_leaves_managed_actions_schedules_and_a_seen_result_run() {
    const DAY: i64 = 86_400;
    let (_temp, m) = coordinator_fixture();
    let managed = finished_run(&m, "managed", true, 40 * DAY, &"a".repeat(64));
    let seed = finished_run(&m, "seed", true, 40 * DAY, &"b".repeat(64));
    let c = m.store.connection().unwrap();
    c.execute("INSERT INTO missions(id,operation,fingerprint,revision,state,workspace_id,updated_at,body) VALUES('mission','op','f',0,'completed','w',0,'{}')",[]).unwrap();
    c.execute(
        "INSERT INTO mission_children(task_id,mission_id,phase,round) VALUES(?1,'mission','plan',0)",
        [&managed.id],
    )
    .unwrap();
    let schedule = Schedule {
        id: seed.id.clone(),
        revision: 0,
        name: "Weekly".into(),
        enabled: false,
        chain: chain(vec![delay("x")]),
        scope: Scope::default(),
        inputs: json!({}),
        trigger: Trigger::Interval {
            seconds: 60,
            anchor: 0,
        },
        next_due_at: None,
        created_at: 0,
        tzdb_version: chrono_tz::IANA_TZDB_VERSION.into(),
    };
    m.store.save_schedule(schedule, None).unwrap();
    m.tick().await.unwrap();
    assert!(m.store.get(&managed.id).is_ok());
    assert!(m.store.get(&seed.id).is_ok());
}

#[test]
fn opening_a_run_in_progress_is_not_seeing_its_result() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let run = s
        .create(
            "open",
            chain(vec![delay("x")]),
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    s.mark_seen(&run.id).unwrap();
    let count = |s: &Store| -> i64 {
        s.connection()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM events WHERE kind='seen'", [], |r| {
                r.get(0)
            })
            .unwrap()
    };
    assert_eq!(count(&s), 0);
    let mut run = s.get(&run.id).unwrap();
    run.state = "finished".into();
    s.save(&mut run, "finished", "Done").unwrap();
    s.mark_seen(&run.id).unwrap();
    s.mark_seen(&run.id).unwrap();
    assert_eq!(count(&s), 1);
    // The researcher's Activity log is unchanged by it.
    assert!(s
        .events(&run.id, 0)
        .unwrap()
        .iter()
        .all(|e| e.kind != "seen"));
}
