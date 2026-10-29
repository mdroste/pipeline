use super::*;

#[test]
fn input_signal_is_consumed_once_and_cannot_resume_cancelled_task() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let mut run = s
        .create(
            "create",
            chain(vec![step(
                "answer",
                Action::Input {
                    prompt: "Which calibration?".into(),
                    timeout_seconds: None,
                },
            )]),
            Scope::default(),
            json!({}),
            0,
            true,
        )
        .unwrap();
    run.state = "waiting".into();
    run.progress.receipts.insert(
        "answer".into(),
        Receipt {
            sequence: 0,
            address: "answer".into(),
            step_id: "answer".into(),
            label: "answer".into(),
            state: "input".into(),
            operation: store::id(),
            started_at: 0,
            finished_at: None,
            wake_at: None,
            output: None,
            error: None,
            child: None,
        },
    );
    s.save(&mut run, "wait", "waiting").unwrap();
    let first = s
        .signal(&run.id, "answer", "signal", json!("baseline"))
        .unwrap();
    let second = s
        .signal(&run.id, "answer", "signal", json!("baseline"))
        .unwrap();
    assert_eq!(first.revision, second.revision);
    assert_eq!(first.progress.outputs["answer"], "baseline");
    assert!(s
        .signal(&run.id, "answer", "different", json!("alternative"))
        .is_err());
}

#[test]
fn store_recovery_preserves_uncertain_actions_without_replay() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let c = chain(vec![delay("one")]);
    let mut run = s
        .create("create", c.clone(), Scope::default(), json!({}), 0, true)
        .unwrap();
    let duplicate = s
        .create("create", c, Scope::default(), json!({}), 0, true)
        .unwrap();
    assert_eq!(run.id, duplicate.id);
    run.state = "running".into();
    run.progress.receipts.insert(
        "one".into(),
        Receipt {
            sequence: 0,
            address: "one".into(),
            step_id: "one".into(),
            label: "one".into(),
            state: "running".into(),
            operation: store::id(),
            started_at: 0,
            finished_at: None,
            wake_at: None,
            output: None,
            error: None,
            child: None,
        },
    );
    s.save(&mut run, "launch", "launched").unwrap();
    s.recover().unwrap();
    let recovered = s.get(&run.id).unwrap();
    assert_eq!(recovered.state, "attention");
    assert_eq!(recovered.progress.receipts["one"].state, "unknown");
    assert!(s.ready_ids(now()).unwrap().is_empty());
}

#[test]
fn stale_revision_cannot_overwrite_input() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let mut a = s
        .create(
            "c",
            chain(vec![delay("x")]),
            Scope::default(),
            json!({}),
            0,
            true,
        )
        .unwrap();
    let mut b = a.clone();
    s.save(&mut a, "pause", "a").unwrap();
    assert!(s.save(&mut b, "resume", "b").is_err());
}

#[test]
fn retry_retains_attempts_and_operation_scoped_outputs_after_reload() {
    for status in ["failed", "unknown"] {
        let temp = tempfile::tempdir().unwrap();
        let s = Store::open(temp.path()).unwrap();
        let mut run = failed_attempt(&s, status);
        let original = run.progress.receipts["action"].clone();
        let mut stale = run.clone();
        let old_result = json!({"text":"Full original result"});
        let original_path = s
            .root
            .join("actions")
            .join(&original.operation)
            .join("result.json");
        if status == "failed" {
            adapters::atomic_json(&original_path, &old_result).unwrap();
        }
        s.retry(&mut run).unwrap();
        assert!(s.retry(&mut stale).is_err());
        assert_eq!(run.progress.actions, 1);
        assert!(run.progress.receipts.is_empty());
        assert_eq!(run.state, "queued");
        // An uncertain result can arrive after the retry and still be inspected.
        if status == "unknown" {
            adapters::atomic_json(&original_path, &old_result).unwrap();
        }
        let mut replacement = original.clone();
        replacement.sequence = 2;
        replacement.operation = store::id();
        replacement.state = "completed".into();
        replacement.error = None;
        replacement.output = Some(json!({"text":"Replacement result"}));
        run.progress
            .receipts
            .insert("action".into(), replacement.clone());
        run.progress.actions = 2;
        run.state = "finished".into();
        s.save(&mut run, "finished", "Replacement completed")
            .unwrap();
        let reopened = Store::open(temp.path()).unwrap();
        let events = reopened.events(&run.id, 0).unwrap();
        let attempts: Vec<_> = events.iter().flat_map(|e| &e.attempts).collect();
        assert_eq!(attempts.len(), 1);
        assert_eq!(
            serde_json::to_value(attempts[0]).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        assert_eq!(
            reopened
                .step_output(&run.id, "action", Some(&original.operation))
                .unwrap(),
            old_result
        );
        assert_eq!(
            reopened.step_output(&run.id, "action", None).unwrap(),
            replacement.output.clone().unwrap()
        );
        assert_eq!(
            reopened
                .step_receipt(&run.id, "action", Some(&replacement.operation))
                .unwrap()
                .sequence,
            2
        );
        assert!(reopened
            .step_output(&run.id, "wrong-address", Some(&original.operation))
            .is_err());
        let other = reopened
            .create(
                "other-task",
                chain(vec![delay("action")]),
                Scope::default(),
                json!({}),
                0,
                true,
            )
            .unwrap();
        assert!(reopened
            .step_output(&other.id, "action", Some(&original.operation))
            .is_err());
    }
}

#[test]
fn retry_rolls_back_receipt_removal_when_journal_write_fails() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let mut run = failed_attempt(&s, "unknown");
    let before = serde_json::to_value(&run).unwrap();
    s.connection().unwrap().execute_batch("CREATE TRIGGER fail_retry BEFORE INSERT ON events WHEN NEW.kind='retry' BEGIN SELECT RAISE(ABORT,'injected journal failure'); END;").unwrap();
    assert!(s
        .retry(&mut run)
        .unwrap_err()
        .contains("injected journal failure"));
    assert_eq!(serde_json::to_value(&run).unwrap(), before);
    assert_eq!(
        serde_json::to_value(s.get(&run.id).unwrap()).unwrap(),
        before
    );
    assert!(!s
        .events(&run.id, 0)
        .unwrap()
        .iter()
        .any(|e| e.kind == "retry"));
}

#[test]
fn legacy_retry_events_remain_readable() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let mut run = failed_attempt(&s, "failed");
    s.save(&mut run, "retry", "Task updated").unwrap();
    let events = s.events(&run.id, 0).unwrap();
    assert_eq!(events.last().unwrap().detail, "Task updated");
    assert!(events.last().unwrap().attempts.is_empty());
    assert!(s
        .step_output(&run.id, "action", Some("missing-operation"))
        .is_err());
}

#[test]
fn paused_active_actions_are_recovered_too() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let mut r = s
        .create(
            "pause",
            chain(vec![delay("a")]),
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    r.state = "paused".into();
    r.progress.receipts.insert(
        "a".into(),
        Receipt {
            sequence: 0,
            address: "a".into(),
            step_id: "a".into(),
            label: "a".into(),
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
    s.save(&mut r, "pause", "Paused").unwrap();
    s.recover().unwrap();
    assert_eq!(s.get(&r.id).unwrap().state, "attention");
}
