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
