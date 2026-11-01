use super::*;

#[test]
fn calendar_skips_spring_gap_and_uses_first_autumn_fold() {
    use chrono::{TimeZone, Utc};
    let spring = Trigger::Calendar {
        timezone: "America/New_York".into(),
        hour: 2,
        minute: 30,
        weekdays: vec![6],
    };
    let after = Utc
        .with_ymd_and_hms(2026, 3, 8, 0, 0, 0)
        .unwrap()
        .timestamp();
    assert_eq!(
        spring.next(after).unwrap(),
        Some(
            Utc.with_ymd_and_hms(2026, 3, 15, 6, 30, 0)
                .unwrap()
                .timestamp()
        )
    );
    let autumn = Trigger::Calendar {
        timezone: "America/New_York".into(),
        hour: 1,
        minute: 30,
        weekdays: vec![6],
    };
    let after = Utc
        .with_ymd_and_hms(2026, 11, 1, 0, 0, 0)
        .unwrap()
        .timestamp();
    let first = autumn.next(after).unwrap().unwrap();
    assert_eq!(
        first,
        Utc.with_ymd_and_hms(2026, 11, 1, 5, 30, 0)
            .unwrap()
            .timestamp()
    );
    assert_eq!(
        autumn.next(first).unwrap(),
        Some(
            Utc.with_ymd_and_hms(2026, 11, 8, 6, 30, 0)
                .unwrap()
                .timestamp()
        )
    );
}

#[test]
fn missed_intervals_coalesce_and_overlap_does_not_spawn() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    s.save_schedule(
        Schedule {
            id: "schedule".into(),
            revision: 0,
            name: "Scheduled".into(),
            enabled: true,
            chain: chain(vec![delay("x")]),
            scope: Scope::default(),
            inputs: json!({}),
            trigger: Trigger::Interval {
                seconds: 60,
                anchor: 0,
            },
            next_due_at: Some(60),
            created_at: 0,
            tzdb_version: chrono_tz::IANA_TZDB_VERSION.into(),
        },
        None,
    )
    .unwrap();
    s.fire_schedules(600).unwrap();
    s.fire_schedules(1200).unwrap();
    let runs = s.list("active", None, 0).unwrap();
    assert_eq!(runs.len(), 1);
    let run = s.get(&runs[0].id).unwrap();
    assert_eq!(run.occurrence, Some(600));
}

#[test]
fn blocked_schedule_page_does_not_starve_later_eligible_schedule() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    for index in 0..33 {
        s.save_schedule(
            Schedule {
                id: format!("schedule-{index:02}"),
                revision: 0,
                name: "Scheduled".into(),
                enabled: true,
                chain: chain(vec![delay("x")]),
                scope: Scope::default(),
                inputs: json!({}),
                trigger: Trigger::Interval {
                    seconds: 60,
                    anchor: 0,
                },
                next_due_at: Some(60),
                created_at: 0,
                tzdb_version: chrono_tz::IANA_TZDB_VERSION.into(),
            },
            None,
        )
        .unwrap();
    }
    s.fire_schedules(600).unwrap();
    let c = s.connection().unwrap();
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM runs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        32
    );
    // The earlier 32 cursors are overdue again, but all have live occurrences.
    s.fire_schedules(1200).unwrap();
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM runs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        33
    );
    assert_eq!(
        c.query_row(
            "SELECT at FROM occurrences WHERE schedule_id='schedule-32' AND disposition='queued'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1200
    );
    assert_eq!(s.schedules().unwrap()[0].next_due_at, Some(660));
    s.fire_schedules(1800).unwrap();
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM runs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        33
    );
    // Releasing one blocked occurrence creates only its coalesced latest run.
    let run_id: String = c
        .query_row(
            "SELECT id FROM runs WHERE schedule_id='schedule-00'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut run = s.get(&run_id).unwrap();
    run.state = "finished".into();
    s.save(&mut run, "finished", "Finished").unwrap();
    s.fire_schedules(1800).unwrap();
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM runs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        34
    );
    assert_eq!(c.query_row("SELECT MAX(at) FROM occurrences WHERE schedule_id='schedule-00' AND disposition='queued'", [], |r| r.get::<_, i64>(0)).unwrap(), 1800);
}

#[test]
fn input_waits_do_not_poll_and_future_timers_are_not_ready() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let mut r = s
        .create(
            "wait",
            chain(vec![delay("a")]),
            Scope::default(),
            json!({}),
            now() + 600,
            true,
        )
        .unwrap();
    assert!(s.ready_ids(now()).unwrap().is_empty());
    r.state = "waiting".into();
    r.due_at = None;
    s.save(&mut r, "wait", "Input").unwrap();
    assert!(s.ready_ids(now()).unwrap().is_empty());
    assert_eq!(s.next_due().unwrap(), Some(r.deadline_at));
}

#[test]
fn schedule_activation_and_draft_consumption_are_atomic() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let mut r = s
        .create(
            "schedule",
            chain(vec![delay("a")]),
            Scope::default(),
            json!({}),
            now(),
            false,
        )
        .unwrap();
    let mut stale = r.clone();
    let trigger = Trigger::Interval {
        seconds: 60,
        anchor: now(),
    };
    s.activate_schedule(&mut r, trigger.clone()).unwrap();
    assert!(s.activate_schedule(&mut stale, trigger).is_err());
    assert_eq!(s.schedules().unwrap().len(), 1);
    assert_eq!(s.get(&r.id).unwrap().state, "finished");
}

#[test]
fn coalescing_calendar_after_years_chooses_a_recent_occurrence() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let trigger = Trigger::Calendar {
        timezone: "UTC".into(),
        hour: 9,
        minute: 0,
        weekdays: vec![0, 1, 2, 3, 4, 5, 6],
    };
    s.save_schedule(
        Schedule {
            id: "calendar".into(),
            revision: 0,
            name: "Long sleep".into(),
            enabled: true,
            chain: chain(vec![delay("a")]),
            scope: Scope::default(),
            inputs: json!({}),
            next_due_at: trigger.next(0).unwrap(),
            trigger,
            created_at: 0,
            tzdb_version: chrono_tz::IANA_TZDB_VERSION.into(),
        },
        None,
    )
    .unwrap();
    s.fire_schedules(now()).unwrap();
    let rows = s.list("active", None, 0).unwrap();
    let r = s.get(&rows[0].id).unwrap();
    assert!(now() - r.occurrence.unwrap() < 86400);
}

fn select_overrides(w: &crate::workbench::store::Store, session: &str, overrides: Value) {
    let current = w.session_snapshot(session).unwrap().session;
    w.update_session(crate::workbench::store::UpdateSessionRequest {
        session_id: session.into(),
        expected_revision: current.revision,
        operation_id: store::id(),
        title: None,
        draft: None,
        overrides: Some(overrides),
        archived: None,
        preset_id: None,
        paper_id: None,
        clear_paper: None,
    })
    .unwrap();
}

/// A schedule prepared the way a researcher prepares one, on a real conversation.
async fn prepared_schedule() -> (crate::workbench::store::Store, String, Schedule) {
    let w = super::disposable_workspace();
    let operation = store::id();
    let session = crate::workbench::discovery::source(&w, &operation, "Weekly", false).unwrap();
    let plan = chain(vec![delay("x")]);
    let inputs = json!({});
    let scope = adapters::prepare(&plan, Some(session.clone()), &inputs)
        .await
        .unwrap();
    let schedule = Schedule {
        id: operation,
        revision: 0,
        name: "Weekly".into(),
        enabled: true,
        chain: plan,
        scope,
        inputs,
        trigger: Trigger::Interval {
            seconds: 60,
            anchor: 0,
        },
        next_due_at: Some(now() - 1),
        created_at: 0,
        tzdb_version: chrono_tz::IANA_TZDB_VERSION.into(),
    };
    (w, session, schedule)
}

#[tokio::test]
async fn occurrences_survive_ordinary_research_activity() {
    let (w, session, schedule) = prepared_schedule().await;
    let before = crate::workbench::tasks::binding(&w, &session).unwrap();
    crate::workbench::tasks::deliver(&w, &session, "weekly-result", json!({"report":"New"}))
        .unwrap();
    let after = crate::workbench::tasks::binding(&w, &session).unwrap();
    assert_ne!(before.harness_fingerprint, after.harness_fingerprint);

    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("tasks")).unwrap();
    let owner = std::fs::File::create(store.root.join("test-owner")).unwrap();
    store.save_schedule(schedule, None).unwrap();
    let coordinator = Arc::new(Coordinator {
        store,
        gate: tokio::sync::Mutex::new(()),
        wake: Notify::new(),
        active: Mutex::new(HashMap::new()),
        stopping: AtomicBool::new(false),
        background: AtomicBool::new(false),
        _owner: owner,
        app: None,
    });
    coordinator.tick().await.unwrap();
    let rows = coordinator.store.list("active", None, 0).unwrap();
    let run = coordinator.store.get(&rows[0].id).unwrap();
    assert_ne!(run.state, "attention", "{:?}", run.reason);
    assert!(run.progress.decisions.contains_key("occurrenceSession"));
    assert_ne!(run.scope.session_id.as_ref(), Some(&session));
}

#[tokio::test]
async fn a_changed_connection_fails_an_occurrence_and_cannot_be_refreshed_away() {
    let (w, session, schedule) = prepared_schedule().await;
    select_overrides(&w, &session, json!({"runtime":"claude"}));
    let source = adapters::task_binding(&schedule.scope).unwrap();
    assert!(
        crate::workbench::tasks::occurrence_session(&w, &source, &store::id(), "Weekly")
            .unwrap_err()
            .message
            .contains("connection")
    );
    assert!(commands::refreshed_scope(&schedule)
        .await
        .unwrap_err()
        .contains("connection"));
}

#[tokio::test]
async fn changed_permissions_fail_until_the_schedule_is_refreshed() {
    let (w, session, mut schedule) = prepared_schedule().await;
    select_overrides(&w, &session, json!({"commandNetwork":true}));
    let source = adapters::task_binding(&schedule.scope).unwrap();
    let current = crate::workbench::tasks::binding(&w, &session).unwrap();
    assert_ne!(current.permission_profile, source.permission_profile);
    assert!(
        crate::workbench::tasks::occurrence_session(&w, &source, &store::id(), "Weekly")
            .unwrap_err()
            .message
            .contains("Use current conversation settings")
    );
    schedule.scope = commands::refreshed_scope(&schedule).await.unwrap();
    let refreshed = adapters::task_binding(&schedule.scope).unwrap();
    crate::workbench::tasks::occurrence_session(&w, &refreshed, &store::id(), "Weekly").unwrap();
}

#[tokio::test]
async fn access_changes_outside_the_permission_profile_fail_until_refreshed() {
    for (name, overrides) in [
        ("mode", json!({"mode":"edit"})),
        ("web search", json!({"webSearch":true})),
        ("browser", json!({"browser":true})),
        ("command sites", json!({"commandDomains":"example.org"})),
    ] {
        let (w, session, mut schedule) = prepared_schedule().await;
        select_overrides(&w, &session, json!({"mode":"inspect"}));
        schedule.scope = commands::refreshed_scope(&schedule).await.unwrap();
        let source = adapters::task_binding(&schedule.scope).unwrap();
        let mut widened = json!({"mode":"inspect"});
        widened
            .as_object_mut()
            .unwrap()
            .extend(overrides.as_object().unwrap().clone());
        select_overrides(&w, &session, widened);
        let current = crate::workbench::tasks::binding(&w, &session).unwrap();
        assert_ne!(current.harness_fingerprint, source.harness_fingerprint);
        assert!(
            crate::workbench::tasks::occurrence_session(&w, &source, &store::id(), "Weekly")
                .is_err(),
            "{name} changed what a scheduled run may do without stopping it"
        );
        schedule.scope = commands::refreshed_scope(&schedule).await.unwrap();
        let refreshed = adapters::task_binding(&schedule.scope).unwrap();
        crate::workbench::tasks::occurrence_session(&w, &refreshed, &store::id(), "Weekly")
            .unwrap();
    }
}

#[tokio::test]
async fn a_schedule_without_a_recorded_identity_keeps_its_exact_check_until_refreshed() {
    let (w, session, mut schedule) = prepared_schedule().await;
    schedule.scope.permission_profile = None;
    schedule.scope.preset_id = None;
    schedule.scope.access = None;
    let saved = adapters::task_binding(&schedule.scope).unwrap();
    crate::workbench::tasks::occurrence_session(&w, &saved, &store::id(), "Weekly").unwrap();
    select_overrides(&w, &session, json!({"mode":"edit"}));
    assert!(
        crate::workbench::tasks::occurrence_session(&w, &saved, &store::id(), "Weekly").is_err()
    );
    schedule.scope = commands::refreshed_scope(&schedule).await.unwrap();
    assert!(schedule.scope.permission_profile.is_some());
    let refreshed = adapters::task_binding(&schedule.scope).unwrap();
    crate::workbench::tasks::occurrence_session(&w, &refreshed, &store::id(), "Weekly").unwrap();
}

#[tokio::test]
async fn a_slot_blocked_task_records_its_wait_once() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("tasks")).unwrap();
    let owner = std::fs::File::create(store.root.join("test-owner")).unwrap();
    let coordinator = Arc::new(Coordinator {
        store,
        gate: tokio::sync::Mutex::new(()),
        wake: Notify::new(),
        active: Mutex::new(HashMap::new()),
        stopping: AtomicBool::new(false),
        background: AtomicBool::new(false),
        _owner: owner,
        app: None,
    });
    let run = coordinator
        .store
        .create(
            "queued-op",
            chain(vec![step(
                "w",
                Action::Workspace {
                    prompt: "Inspect".into(),
                    model: None,
                    effort: None,
                },
            )]),
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    // Another Workspace step holds the only slot for the whole wait.
    let (holder, _watch) = watch::channel(false);
    coordinator
        .active
        .lock()
        .unwrap()
        .insert("other".into(), ("workspace".into(), holder));
    let started = coordinator.store.get(&run.id).unwrap().revision;
    for round in 0..3 {
        if round > 0 {
            // ready_ids selects by due_at <= now; the wait re-polls every 2 s.
            coordinator
                .store
                .touch_due(&run.id, Some(now() - 1))
                .unwrap();
        }
        coordinator.tick().await.unwrap();
        let current = coordinator.store.get(&run.id).unwrap();
        assert_eq!(current.state, "queued");
        assert_eq!(
            current.reason.as_deref(),
            Some("Waiting for an execution slot")
        );
    }
    let queued_events: i64 = coordinator
        .store
        .connection()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM events WHERE run_id=?1 AND kind='queued'",
            [&run.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(queued_events, 1);
    assert_eq!(
        coordinator.store.get(&run.id).unwrap().revision,
        started + 1
    );
}

fn fixture_coordinator(store: Store) -> Arc<Coordinator> {
    let owner = std::fs::File::create(store.root.join("test-owner")).unwrap();
    Arc::new(Coordinator {
        store,
        gate: tokio::sync::Mutex::new(()),
        wake: Notify::new(),
        active: Mutex::new(HashMap::new()),
        stopping: AtomicBool::new(false),
        background: AtomicBool::new(false),
        _owner: owner,
        app: None,
    })
}

#[test]
fn a_blocked_schedule_names_its_run_and_a_free_one_names_none() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let schedule = interval_schedule("weekly", now() - 1);
    s.save_schedule(schedule, None).unwrap();
    assert!(s.schedule_views().unwrap()[0].blocking_run.is_none());
    s.fire_schedules(now()).unwrap();
    let id = s.list("active", None, 0).unwrap()[0].id.clone();
    let mut run = s.get(&id).unwrap();
    run.state = "attention".into();
    s.save(&mut run, "attention", "Needs a decision").unwrap();
    let views = s.schedule_views().unwrap();
    let blocking = views[0].blocking_run.as_ref().unwrap();
    assert_eq!(
        (blocking.id.as_str(), blocking.state.as_str()),
        (id.as_str(), "attention")
    );
    let value = serde_json::to_value(&views[0]).unwrap();
    assert_eq!(value["blockingRun"]["state"], "attention");
    assert_eq!(value["name"], "weekly");
    run.state = "finished".into();
    s.save(&mut run, "finished", "Done").unwrap();
    assert!(s.schedule_views().unwrap()[0].blocking_run.is_none());
}

fn interval_schedule(id: &str, due: i64) -> Schedule {
    Schedule {
        id: id.into(),
        revision: 0,
        name: id.into(),
        enabled: true,
        chain: chain(vec![delay("x")]),
        scope: Scope::default(),
        inputs: json!({}),
        trigger: Trigger::Interval {
            seconds: 60,
            anchor: 0,
        },
        next_due_at: Some(due),
        created_at: 0,
        tzdb_version: chrono_tz::IANA_TZDB_VERSION.into(),
    }
}

#[test]
fn a_deleted_schedule_starts_nothing_and_its_runs_stay_in_history() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    s.save_schedule(interval_schedule("weekly", now() - 1), None)
        .unwrap();
    s.fire_schedules(now()).unwrap();
    let id = s.list("active", None, 0).unwrap()[0].id.clone();
    let mut run = s.get(&id).unwrap();
    run.state = "finished".into();
    s.save(&mut run, "finished", "Done").unwrap();
    assert!(s.delete_schedule("weekly", 7).is_err());
    assert_eq!(s.schedules().unwrap().len(), 1);
    s.delete_schedule("weekly", 0).unwrap();
    assert!(s.schedules().unwrap().is_empty());
    s.fire_schedules(now() + 120).unwrap();
    assert!(s.list("active", None, 0).unwrap().is_empty());
    assert_eq!(s.list("history", None, 0).unwrap().len(), 1);
    assert!(!s.events(&id, 0).unwrap().is_empty());
}

#[test]
fn one_unreadable_schedule_does_not_hold_back_the_others() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    s.save_schedule(interval_schedule("a-bad", now() - 2), None)
        .unwrap();
    s.save_schedule(interval_schedule("b-good", now() - 1), None)
        .unwrap();
    s.connection()
        .unwrap()
        .execute("UPDATE schedules SET body='{}' WHERE id='a-bad'", [])
        .unwrap();
    assert!(s.fire_schedules(now()).is_err());
    let rows = s.list("active", None, 0).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].schedule_id.as_deref(), Some("b-good"));
}

#[tokio::test]
async fn a_blocked_side_by_side_branch_records_its_wait_once() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path()).unwrap();
    let branch = |id: &str| {
        vec![step(
            id,
            Action::Workspace {
                prompt: "Inspect".into(),
                model: None,
                effort: None,
            },
        )]
    };
    let mut run = store
        .create(
            "side-by-side",
            chain(vec![step(
                "parallel",
                Action::Parallel {
                    branches: vec![branch("a"), branch("b")],
                },
            )]),
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    // One branch runs and holds the single conversation slot.
    let Advance::Ready(ready) = state::advance(&run.chain, &mut run.progress, &run.inputs) else {
        panic!()
    };
    let running = &ready[0];
    run.progress.receipts.insert(
        running.address.clone(),
        Receipt {
            sequence: 1,
            address: running.address.clone(),
            step_id: running.step.id.clone(),
            label: running.step.label.clone(),
            state: "running".into(),
            operation: store::id(),
            started_at: now(),
            finished_at: None,
            wake_at: None,
            output: None,
            error: None,
            child: None,
        },
    );
    run.state = "running".into();
    run.reason = Some(running.step.label.clone());
    store.save(&mut run, "stepStarted", "First branch").unwrap();
    let c = fixture_coordinator(store);
    let id = run.id.clone();
    let (holder, _watch) = watch::channel(false);
    c.active.lock().unwrap().insert(
        format!("{id}/{}", running.address),
        ("workspace".into(), holder),
    );
    for _ in 0..4 {
        c.store.touch_due(&id, Some(now() - 1)).unwrap();
        c.tick().await.unwrap();
    }
    let stored = c.store.get(&id).unwrap();
    assert_eq!(stored.state, "running");
    assert_eq!(stored.revision, run.revision + 1);
    let waits = c
        .store
        .events(&id, 0)
        .unwrap()
        .iter()
        .filter(|e| e.kind == "queued")
        .count();
    assert_eq!(waits, 1);
}
