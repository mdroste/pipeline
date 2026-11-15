//! Exercise the boundaries between coordinator publication and real owner receipts.
//! No provider, network request or host process is needed to simulate process loss.
use super::coordinator::{coordinator_fixture, drive};
use super::*;
use crate::workbench::tasks;
use adapters::contract::{lookup, Kind, Outcome};
use state::Advance;

#[tokio::test]
async fn every_worker_recovers_only_its_own_committed_attempt() {
    let _guard = exclusive_turn_state().await;
    let w = disposable_workspace();
    let session =
        crate::workbench::discovery::source(&w, &store::id(), "Recovery matrix", false).unwrap();
    let scope = adapters::prepare(&chain(vec![delay("x")]), Some(session.clone()), &json!({}))
        .await
        .unwrap();
    let ws = scope.workspace_id.clone().unwrap();
    let (_tmp, m) = coordinator_fixture();
    let literal = Binding::Literal {
        value: json!("Paper"),
    };
    for action in [
        Action::Snapshot {
            input: literal.clone(),
            filename: "paper.md".into(),
            require_change: false,
        },
        Action::Workspace {
            prompt: "Write".into(),
            model: None,
            effort: None,
        },
        Action::Review {
            profile_id: "review".into(),
            input: literal.clone(),
            interpretation: "document".into(),
            variables: Default::default(),
        },
        Action::Check {
            profile_id: "check".into(),
        },
        Action::CapturedCheck {
            plan_id: "plan".into(),
        },
        Action::LiteratureLookup {
            query: "growth".into(),
        },
        Action::Deliver {
            input: literal.clone(),
        },
    ] {
        let kind = Kind::of(&action);
        let plan = chain(vec![step("leaf", action.clone())]);
        let mut run = m
            .store
            .create(&store::id(), plan, scope.clone(), json!({}), now(), true)
            .unwrap();
        let Advance::Ready(ready) = state::advance(&run.chain, &mut run.progress, &json!({}))
        else {
            panic!()
        };
        done(&mut run.progress, &ready[0], Value::Null);
        let rec = run.progress.receipts.get_mut("leaf").unwrap();
        rec.action_kind = Some(kind);
        rec.state = "running".into();
        rec.started_at = now();
        rec.finished_at = None;
        let rec = rec.clone();
        let op = rec.operation.clone();
        run.progress.actions = 1;
        run.state = "running".into();
        run.progress.outputs = json!({});
        m.store.save(&mut run, "stepStarted", "fixture").unwrap();
        let dir = m.store.root.join("actions").join(&op);
        assert_eq!(
            lookup(&m.store, &scope, &action, &rec).await.unwrap(),
            Outcome::NotSubmitted
        );
        adapters::contract::mark(&m.store, &op, kind).unwrap();
        assert_eq!(
            lookup(&m.store, &scope, &action, &rec).await.unwrap(),
            Outcome::Unknown
        );
        // A receipt from a different owner cannot settle the operation.
        if kind != Kind::Deliver {
            tasks::deliver(&w, &session, &op, json!("unrelated exchange")).unwrap();
        }
        match kind {
            Kind::Snapshot => {
                assert_eq!(
                    lookup(&m.store, &scope, &action, &rec).await.unwrap(),
                    Outcome::Unknown
                );
                let artifact =
                    adapters::snapshot(&m.store, &scope, json!("Revision"), "paper.md").unwrap();
                adapters::atomic_json(&dir.join("snapshot.json"), &artifact).unwrap();
            }
            Kind::Deliver => {
                tasks::deliver(&w, &session, &op, json!("delivered")).unwrap();
            }
            Kind::LiteratureLookup => {
                crate::workbench::acquisition::record_lookup(
                    &w,
                    &ws,
                    "growth",
                    Ok(Vec::new()),
                    &op,
                )
                .unwrap();
            }
            Kind::Workspace => {
                let thread = format!("test-{op}");
                let b = w
                    .bind_session(&session, crate::workbench::api::NAMESPACE, &thread)
                    .unwrap();
                w.record_turn_submission(&b.id, &format!("task-{op}"), "turn")
                    .unwrap();
                assert_eq!(
                    lookup(&m.store, &scope, &action, &rec).await.unwrap(),
                    Outcome::Unknown
                );
                w.project_codex_event(
                    &b.id,
                    &crate::agent_runtime::events::NormalizedEvent::TurnCompleted {
                        epoch: 1,
                        thread_id: thread,
                        turn_id: "turn".into(),
                        status: "completed".into(),
                        turn: json!({"id":"turn","status":"completed"}),
                    },
                )
                .unwrap();
                // A native idea team has no durable conversation reply of its
                // own. Even a matching turn cannot stand in for its team result.
                adapters::contract::mark_team(&m.store, &op).unwrap();
                assert_eq!(
                    lookup(&m.store, &scope, &action, &rec).await.unwrap(),
                    Outcome::Unknown
                );
                adapters::atomic_json(
                    &dir.join("team-result.json"),
                    &json!({"completedIdeaAgents":2}),
                )
                .unwrap();
                assert_eq!(
                    lookup(&m.store, &scope, &action, &rec).await.unwrap(),
                    Outcome::Completed(json!({"completedIdeaAgents":2}))
                );
                adapters::contract::mark(&m.store, &op, kind).unwrap();
            }
            Kind::Check | Kind::CapturedCheck => {
                let c = w.connection().unwrap();
                c.execute("INSERT INTO research_executions(id,workspace_id,adapter,command_json,cwd,input_manifest_json,dependency_hash,outcome,output_manifest_json,validation_json,snapshot_consistency,created_at) VALUES(?1,?2,'python','[]','/tmp','{}','test','running','{}','{}','complete','2026-09-07')",rusqlite::params![op,ws]).unwrap();
                c.execute("INSERT INTO execution_jobs(execution_id,operation_id,request_hash,owner,process_instance,created_at) VALUES(?1,?1,'test','detached',?1,'2026-09-07')",[&op]).unwrap();
                let lock = std::fs::File::create(
                    crate::workbench::research::jobs::instance_lock_path(&w, &op).unwrap(),
                )
                .unwrap();
                fs2::FileExt::try_lock_exclusive(&lock).unwrap();
                assert_eq!(
                    lookup(&m.store, &scope, &action, &rec).await.unwrap(),
                    Outcome::Running
                );
                drop(lock);
                assert_eq!(
                    lookup(&m.store, &scope, &action, &rec).await.unwrap(),
                    Outcome::Unknown
                );
                c.execute(
                    "UPDATE research_executions SET outcome='completed' WHERE id=?1",
                    [&op],
                )
                .unwrap();
            }
            Kind::Review => {
                adapters::atomic_json(&dir.join("review-input.json"), &json!({"hash":"frozen"}))
                    .unwrap();
                adapters::atomic_json(&dir.join("review-raw.json"),&json!({"report":{"quality":crate::models::ReportQuality::default(),"products":crate::models::RunProducts::default(),"report_date":"2026-01-01","paper_hash":"abc","failed_steps":[]},"markdown":"Review"})).unwrap();
            }
            Kind::Control => unreachable!(),
        }
        m.store.recover().unwrap(); // interrupted between owner commit and coordinator adoption
        let mut recovered = m.store.get(&run.id).unwrap();
        m.recover_results(&mut recovered).await.unwrap();
        assert_eq!(
            recovered.progress.receipts["leaf"].state, "completed",
            "{kind:?}"
        );
        let result = recovered.progress.outputs["leaf"].clone();
        assert!(!result.is_null());
        // A journal alone remains sufficient if the owner has since moved on.
        assert_eq!(
            lookup(&m.store, &scope, &action, &rec).await.unwrap(),
            Outcome::Completed(result.clone())
        );
        recovered.requeue_after_reconcile();
        m.save(&mut recovered, "reconciled", "Recovered")
            .await
            .unwrap();
        m.store.recover().unwrap();
        assert_eq!(
            m.store.get(&run.id).unwrap().progress.receipts["leaf"].operation,
            op
        );
        // Late completion for that attempt cannot publish a second time.
        m.publish_completion(&run.id, "leaf", &op, Ok(json!("wrong")))
            .await;
        assert_eq!(
            m.store.get(&run.id).unwrap().progress.outputs["leaf"],
            result
        );
        assert_eq!(m.store.get(&run.id).unwrap().progress.actions, 1);
    }
}

#[tokio::test]
async fn a_completion_write_failure_recovers_without_restart_or_worker_replay() {
    let (_temp, m) = coordinator_fixture();
    let plan = chain(vec![step(
        "snapshot",
        Action::Snapshot {
            input: Binding::Literal {
                value: json!("Paper"),
            },
            filename: "paper.md".into(),
            require_change: false,
        },
    )]);
    let run = m
        .store
        .create(
            "write-fault",
            plan,
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    m.store.connection().unwrap().execute_batch("CREATE TRIGGER reject_completion BEFORE INSERT ON events WHEN NEW.kind='stepCompleted' BEGIN SELECT RAISE(ABORT,'completion fault'); END;").unwrap();
    m.tick().await.unwrap();
    let started = m.store.get(&run.id).unwrap();
    let op = started.progress.receipts["snapshot"].operation.clone();
    let path = m.store.root.join("actions").join(&op).join("result.json");
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !path.is_file() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await
        }
    })
    .await
    .unwrap();
    // Force the same fault on the real completion path before removing it.
    assert!(m
        .complete(&run.id, "snapshot", Err("after journal".into()))
        .await
        .unwrap_err()
        .contains("completion fault"));
    assert_eq!(
        m.store.get(&run.id).unwrap().progress.receipts["snapshot"].state,
        "running"
    );
    m.store
        .connection()
        .unwrap()
        .execute_batch("DROP TRIGGER reject_completion")
        .unwrap();
    let end = drive(&m, &run.id, "finished").await;
    assert_eq!(end.progress.actions, 1);
    assert_eq!(end.progress.receipts["snapshot"].operation, op);
    assert_eq!(
        m.store
            .events(&run.id, 0)
            .unwrap()
            .iter()
            .filter(|e| e.kind == "stepCompleted")
            .count(),
        1
    );
}

#[tokio::test]
async fn nested_expiry_counts_pause_and_restart_and_rejects_late_input_and_retry() {
    let (_tmp, m) = coordinator_fixture();
    let mut inner = chain(vec![
        step(
            "ask",
            Action::Input {
                prompt: "Input?".into(),
                timeout_seconds: None,
            },
        ),
        delay("must_not_start"),
    ]);
    inner.limits.deadline_hours = 1;
    inner.limits.action_timeout_secs = 30;
    let plan = chain(vec![step(
        "group",
        Action::Chain {
            chain: Box::new(inner),
        },
    )]);
    let run = m
        .store
        .create("late-group", plan, Scope::default(), json!({}), now(), true)
        .unwrap();
    let waiting = drive(&m, &run.id, "waiting").await;
    let cutoff = waiting.progress.group_deadlines["group"];
    assert_eq!(waiting.due_at, Some(cutoff));
    let mut paused = commands::control(&m, run.id.clone(), waiting.revision, "pause".into())
        .await
        .unwrap();
    paused
        .progress
        .group_deadlines
        .insert("group".into(), now() - 1);
    m.store
        .save(&mut paused, "fixture", "clock advanced while paused")
        .unwrap();
    Store::open(&m.store.root).unwrap().recover().unwrap();
    assert!(m
        .store
        .signal(&run.id, "group/ask", "late", json!("answer"))
        .is_err());
    commands::control(&m, run.id.clone(), paused.revision, "resume".into())
        .await
        .unwrap();
    m.tick().await.unwrap();
    let mut expired = m.store.get(&run.id).unwrap();
    assert_eq!(expired.state, "attention");
    assert!(expired.reason.as_deref().unwrap().contains("Group group"));
    assert!(m.store.retry(&mut expired).is_err());
    assert_eq!(expired.progress.actions, 1);
    assert!(!expired
        .progress
        .receipts
        .contains_key("group/must_not_start"));
}

#[test]
fn loop_iterations_get_distinct_clocks_and_inherit_the_smallest_active_timeout() {
    let mut inner = chain(vec![delay("leaf")]);
    inner.limits.deadline_hours = 1;
    inner.limits.action_timeout_secs = 90;
    let mut mid = chain(vec![step(
        "inner",
        Action::Chain {
            chain: Box::new(inner),
        },
    )]);
    mid.limits.action_timeout_secs = 60;
    let mut plan = chain(vec![step(
        "loop",
        Action::ForEach {
            input: Binding::Literal {
                value: json!([1, 2]),
            },
            max_items: 2,
            steps: vec![step(
                "mid",
                Action::Chain {
                    chain: Box::new(mid),
                },
            )],
        },
    )]);
    let mut p = state::Progress::default();
    let Advance::Ready(a) = state::advance_at(&plan, &mut p, &json!({}), 100) else {
        panic!()
    };
    assert_eq!(a[0].timeout_secs, 60);
    assert_eq!(a[0].cutoff(&p, 4000, 100), 160); // active timeout
    assert_eq!(a[0].cutoff(&p, 115, 100), 115); // root deadline
    assert_eq!(a[0].cutoff(&p, 4000, 3695), 3700); // five seconds remain in this group
    assert_eq!(p.group_deadlines["loop/0/mid/inner"], 3700);
    done(&mut p, &a[0], json!("done"));
    let Advance::Ready(b) = state::advance_at(&plan, &mut p, &json!({}), 200) else {
        panic!()
    };
    assert!(!p.group_deadlines.contains_key("loop/0/mid"));
    assert_eq!(p.group_deadlines["loop/1/mid/inner"], 3800);
    assert_eq!(b[0].timeout_secs, 60);
    plan.limits.action_timeout_secs = 30;
    let Advance::Ready(b) = state::advance_at(&plan, &mut p, &json!({}), 250) else {
        panic!()
    };
    assert_eq!(b[0].timeout_secs, 30);
    assert!(matches!(
        state::advance_at(&plan, &mut p, &json!({}), 3800),
        Advance::Attention(_)
    ));
}

#[test]
fn invalid_nested_time_limits_are_rejected_at_preparation() {
    for (hours, seconds) in [(0, 30), (8761, 30), (1, 0), (1, 28801)] {
        let mut g = chain(vec![delay("leaf")]);
        g.limits.deadline_hours = hours;
        g.limits.action_timeout_secs = seconds;
        assert!(definition::validate(&chain(vec![step(
            "g",
            Action::Chain { chain: Box::new(g) }
        )]))
        .is_err());
    }
}

#[tokio::test]
async fn an_archived_pre_upgrade_attempt_still_sets_the_groups_first_deadline() {
    let (_tmp, m) = coordinator_fixture();
    let mut g = chain(vec![delay("leaf")]);
    g.limits.deadline_hours = 1;
    let plan = chain(vec![step("g", Action::Chain { chain: Box::new(g) })]);
    let mut run = m
        .store
        .create(
            "legacy-clock",
            plan,
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    run.progress.actions = 1;
    let mut old = failed_attempt(&m.store, "failed").progress.receipts["action"].clone();
    old.address = "g/leaf".into();
    old.step_id = "leaf".into();
    old.started_at = now() - 7200;
    // Old Retry removed that receipt and left its only timestamp in the event.
    m.store
        .save(
            &mut run,
            "retry",
            &json!({"detail":"retry","attempts":[old]}).to_string(),
        )
        .unwrap();
    m.store.recover().unwrap();
    m.tick().await.unwrap();
    let after = m.store.get(&run.id).unwrap();
    assert_eq!(after.state, "attention");
    assert_eq!(after.progress.actions, 1);
    assert!(after.progress.receipts.is_empty());
    assert!(after.reason.unwrap().contains("Group g"));
}

#[tokio::test]
async fn a_late_journal_is_inspectable_but_needs_an_explicit_decision_to_advance() {
    let (_tmp, m) = coordinator_fixture();
    let action = Action::Snapshot {
        input: Binding::Literal {
            value: json!("text"),
        },
        filename: "text.md".into(),
        require_change: false,
    };
    let mut run = m
        .store
        .create(
            "late-result",
            chain(vec![step("snapshot", action)]),
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    let Advance::Ready(ready) = state::advance(&run.chain, &mut run.progress, &json!({})) else {
        panic!()
    };
    done(&mut run.progress, &ready[0], Value::Null);
    let r = run.progress.receipts.get_mut("snapshot").unwrap();
    r.action_kind = Some(Kind::Snapshot);
    r.state = "running".into();
    let op = r.operation.clone();
    run.progress.actions = 1;
    run.state = "running".into();
    m.store.save(&mut run, "started", "Snapshot").unwrap();
    let file = adapters::snapshot(&m.store, &run.scope, json!("late"), "text.md").unwrap();
    adapters::atomic_json(
        &m.store.root.join("actions").join(&op).join("result.json"),
        &file,
    )
    .unwrap();
    m.publish_completion(
        &run.id,
        "snapshot",
        &op,
        Err(scheduler::TIME_LIMIT_REACHED.into()),
    )
    .await;
    let timed = m.store.get(&run.id).unwrap();
    assert_eq!(timed.state, "attention");
    assert_eq!(
        m.store.step_output(&run.id, "snapshot", Some(&op)).unwrap(),
        file
    );
    let adopted = commands::control(&m, run.id.clone(), timed.revision, "retry".into())
        .await
        .unwrap();
    assert_eq!(adopted.progress.actions, 1);
    assert_eq!(adopted.progress.outputs["snapshot"], file);
}
