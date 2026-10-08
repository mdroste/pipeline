use super::*;

fn coordinator_fixture() -> (tempfile::TempDir, Arc<Coordinator>) {
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
