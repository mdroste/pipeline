use super::*;

#[test]
fn progress_is_scoped_to_the_submission_bounded_and_excludes_reasoning_and_final_json() {
    let (_temp, store, session) = super::super::tests::fixture();
    super::super::tests::finished_turn(
        &store,
        &session,
        "old",
        vec![json!({"id":"old","type":"agentMessage","phase":"commentary","text":"Old turn"})],
    );
    let mut items = (0..40).map(|n| json!({"id":format!("update-{n}"),"type":"agentMessage","phase":"commentary","text":"x".repeat(2000)})).collect::<Vec<_>>();
    items.push(json!({"id":"secret","type":"reasoning","text":"Hidden reasoning"}));
    items.push(json!({"id":"final","type":"agentMessage","phase":"final_answer","text":"{\"manuscript\":\"private answer\"}"}));
    super::super::tests::finished_turn(&store, &session, "new", items);
    let snapshot = read(&store, &session, "task-new").unwrap();
    assert_eq!(snapshot.entries.len(), 30);
    assert!(snapshot.entries.iter().all(|e| e.label.len() <= 500));
    let value = serde_json::to_string(&snapshot).unwrap();
    assert!(!value.contains("Old turn"));
    assert!(!value.contains("Hidden reasoning"));
    assert!(!value.contains("private answer"));
    assert!(snapshot.last_activity_at.is_some());
    assert!(read(&store, &session, "task-not-submitted")
        .unwrap()
        .entries
        .is_empty());
}

#[test]
fn streamed_activity_updates_only_its_own_turns_timestamp() {
    let (_temp, store, session) = super::super::tests::fixture();
    let binding = store
        .bind_session(&session, "test-runtime", "live-thread")
        .unwrap();
    store
        .record_turn_submission(&binding.id, "task-live", "live-turn")
        .unwrap();
    store
        .connection()
        .unwrap()
        .execute("UPDATE turns SET updated_at='2000-01-01T00:00:00Z'", [])
        .unwrap();
    observe(&NormalizedEvent::AgentMessageDelta {
        epoch: 1,
        thread_id: "other-thread".into(),
        turn_id: "live-turn".into(),
        item_id: "i".into(),
        delta: "private".into(),
    });
    let old = read(&store, &session, "task-live")
        .unwrap()
        .last_activity_at;
    observe(&NormalizedEvent::AgentMessageDelta {
        epoch: 1,
        thread_id: "live-thread".into(),
        turn_id: "live-turn".into(),
        item_id: "i".into(),
        delta: "private".into(),
    });
    let next = read(&store, &session, "task-live").unwrap();
    assert!(next.last_activity_at > old);
    assert!(next.entries.is_empty());
}
