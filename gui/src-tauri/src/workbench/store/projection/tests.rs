//! Replays, late events and repeated ids through the store's event projection.
use crate::agent_runtime::events::NormalizedEvent;
use crate::workbench::store::{CreateSessionRequest, SessionBinding, Store};
use serde_json::json;

fn store() -> (tempfile::TempDir, Store) {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open_at(&directory.path().join("store")).unwrap();
    (directory, store)
}

fn session(store: &Store, title: &str) -> String {
    store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: title.into(),
            operation_id: format!("create-{}", title.replace(' ', "-")),
        })
        .unwrap()
        .record
        .id
}

fn item(binding: &SessionBinding, turn: &str, id: &str, text: &str, done: bool) -> NormalizedEvent {
    let item = json!({"id":id,"type":"agentMessage","text":text});
    let (epoch, thread_id, turn_id, item_id, item_kind) = (
        1,
        binding.provider_thread_id.clone(),
        turn.to_string(),
        id.to_string(),
        "agentMessage".to_string(),
    );
    if done {
        NormalizedEvent::ItemCompleted {
            epoch,
            thread_id,
            turn_id,
            item_id,
            item_kind,
            item,
        }
    } else {
        NormalizedEvent::ItemStarted {
            epoch,
            thread_id,
            turn_id,
            item_id,
            item_kind,
            item,
        }
    }
}

fn turn_event(binding: &SessionBinding, turn: &str, status: Option<&str>) -> NormalizedEvent {
    match status {
        Some(status) => NormalizedEvent::TurnCompleted {
            epoch: 1,
            thread_id: binding.provider_thread_id.clone(),
            turn_id: turn.into(),
            status: status.into(),
            turn: json!({"id":turn,"status":status,"usage":{"total":7}}),
        },
        None => NormalizedEvent::TurnStarted {
            epoch: 1,
            thread_id: binding.provider_thread_id.clone(),
            turn_id: turn.into(),
            turn: json!({"id":turn,"status":"inProgress"}),
        },
    }
}

fn texts(store: &Store, session: &str) -> Vec<(String, bool)> {
    store
        .transcript_page(session, None)
        .unwrap()
        .items
        .into_iter()
        .map(|item| (item.payload["text"].as_str().unwrap().into(), item.is_final))
        .collect()
}

#[test]
fn two_threads_can_repeat_an_item_id_in_one_epoch() {
    let (_directory, store) = store();
    let first = session(&store, "first");
    let second = session(&store, "second");
    let one = store.bind_session(&first, "codex", "thread-1").unwrap();
    let two = store.bind_session(&second, "codex", "thread-2").unwrap();
    for (binding, text) in [(&one, "First answer"), (&two, "Second answer")] {
        store
            .project_codex_event(&binding.id, &item(binding, "turn-1", "item-1", text, true))
            .unwrap();
    }
    assert_eq!(texts(&store, &first), [("First answer".into(), true)]);
    assert_eq!(texts(&store, &second), [("Second answer".into(), true)]);
}

#[test]
fn a_successor_thread_repeating_an_id_keeps_both_items_in_order() {
    let (_directory, store) = store();
    let id = session(&store, "successor");
    let old = store.bind_session(&id, "codex", "thread-1").unwrap();
    store
        .project_codex_event(&old.id, &item(&old, "turn-1", "item-1", "Earlier", true))
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let new = store.bind_session(&id, "codex", "thread-2").unwrap();
    store
        .project_codex_event(&new.id, &item(&new, "turn-1", "item-1", "Later", true))
        .unwrap();
    assert_eq!(
        texts(&store, &id),
        [("Earlier".into(), true), ("Later".into(), true)]
    );
    assert_eq!(store.conversation_snapshot(&id).unwrap().turns.len(), 2);
}

#[test]
fn a_replay_on_one_binding_is_skipped_and_writes_no_second_row() {
    let (_directory, store) = store();
    let id = session(&store, "replay");
    let binding = store.bind_session(&id, "codex", "thread-1").unwrap();
    let event = item(&binding, "turn-1", "item-1", "Answer", true);
    let first = store.project_codex_event(&binding.id, &event).unwrap();
    assert_eq!(
        store.project_codex_event(&binding.id, &event).unwrap(),
        first
    );
    assert_eq!(texts(&store, &id).len(), 1);
}

#[test]
fn items_journaled_under_the_older_ids_are_not_written_twice() {
    let (_directory, store) = store();
    let id = session(&store, "legacy");
    let binding = store.bind_session(&id, "codex", "thread-1").unwrap();
    store
        .project_codex_event(
            &binding.id,
            &item(&binding, "turn-1", "item-1", "Answer", true),
        )
        .unwrap();
    {
        // What the earlier build left behind: the same projection journaled
        // under an id that names neither the binding nor the thread.
        let connection = store.connection().unwrap();
        let changed = connection
            .execute(
                "UPDATE change_log SET operation_id = 'codex-1-item-item-1-completed' WHERE entity_type = 'codex_projection'",
                [],
            )
            .unwrap();
        assert_eq!(changed, 1);
    }
    let replay = item(&binding, "turn-1", "item-1", "Answer", true);
    store.project_codex_event(&binding.id, &replay).unwrap();
    store.project_codex_event(&binding.id, &replay).unwrap();
    assert_eq!(texts(&store, &id), [("Answer".into(), true)]);
    let rows: i64 = store
        .connection()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM transcript_items", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(rows, 1);

    // A later connection on the same thread takes the item over in place.
    let later = store.bind_session(&id, "codex-later", "thread-1").unwrap();
    assert_ne!(later.id, binding.id);
    store
        .project_codex_event(&later.id, &item(&later, "turn-1", "item-1", "Answer", true))
        .unwrap();
    assert_eq!(texts(&store, &id), [("Answer".into(), true)]);
    assert_eq!(store.conversation_snapshot(&id).unwrap().turns.len(), 1);
}

#[test]
fn a_late_started_copy_keeps_the_finished_item() {
    let (_directory, store) = store();
    let id = session(&store, "late item");
    let binding = store.bind_session(&id, "codex", "thread-1").unwrap();
    let finished = item(&binding, "turn-1", "item-1", "The full answer", true);
    let started = item(&binding, "turn-1", "item-1", "", false);
    store.project_codex_event(&binding.id, &finished).unwrap();
    store.project_codex_event(&binding.id, &started).unwrap();
    assert_eq!(texts(&store, &id), [("The full answer".into(), true)]);
}

#[test]
fn a_started_item_still_grows_until_it_finishes() {
    let (_directory, store) = store();
    let id = session(&store, "running item");
    let binding = store.bind_session(&id, "codex", "thread-1").unwrap();
    for (text, done) in [("", false), ("Draft", false), ("Final", true)] {
        let mut event = item(&binding, "turn-1", "item-1", text, done);
        // Each copy of one item arrives under its own epoch's operation id.
        if let NormalizedEvent::ItemStarted { epoch, .. } = &mut event {
            *epoch = text.len() as u64 + 2;
        }
        store.project_codex_event(&binding.id, &event).unwrap();
        assert_eq!(texts(&store, &id), [(text.into(), done)]);
    }
}

#[test]
fn a_replayed_start_leaves_an_ended_turn_ended() {
    let (_directory, store) = store();
    let id = session(&store, "late turn");
    let binding = store.bind_session(&id, "codex", "thread-1").unwrap();
    let start = turn_event(&binding, "turn-1", None);
    store.project_codex_event(&binding.id, &start).unwrap();
    store
        .project_codex_event(&binding.id, &turn_event(&binding, "turn-1", Some("failed")))
        .unwrap();
    let read = |store: &Store| {
        let connection = store.connection().unwrap();
        connection
            .query_row(
                "SELECT state, usage_json, terminal_at FROM turns",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .unwrap()
    };
    let ended = read(&store);
    assert_eq!(ended.0, "failed");
    assert!(ended.2.is_some());
    // The same start under a later epoch has an operation id of its own.
    let NormalizedEvent::TurnStarted {
        thread_id,
        turn_id,
        turn,
        ..
    } = start
    else {
        unreachable!()
    };
    let replay: NormalizedEvent = NormalizedEvent::TurnStarted {
        epoch: 2,
        thread_id,
        turn_id,
        turn,
    };
    store.project_codex_event(&binding.id, &replay).unwrap();
    assert_eq!(read(&store), ended);
}
