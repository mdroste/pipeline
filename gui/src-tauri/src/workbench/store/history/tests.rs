use super::*;
fn fixture() -> (tempfile::TempDir, Store, String) {
    let t = tempfile::tempdir().unwrap();
    let s = Store::open_at(&t.path().join("store")).unwrap();
    let session = s
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "test".into(),
            operation_id: "create".into(),
        })
        .unwrap()
        .record
        .id;
    s.connection().unwrap().execute("INSERT INTO session_bindings(id,session_id,runtime_namespace,provider_thread_id,incarnation,created_at) VALUES('binding',?1,'audit','native',1,'now')",[&session]).unwrap();
    (t, s, session)
}
#[test]
fn long_history_pages_and_exports_without_loss_at_equal_timestamps() {
    let (_t, s, id) = fixture();
    let mut c = s.connection().unwrap();
    let tx = c.transaction().unwrap();
    for i in 0..501 {
        tx.execute("INSERT INTO turns(id,binding_id,client_submission_id,provider_turn_id,state,created_at,updated_at,terminal_at) VALUES(?1,'binding',?1,?1,'completed','now','now','now')",[format!("turn-{i:05}")]).unwrap();
    }
    for i in 0..10_000 {
        tx.execute("INSERT INTO transcript_items(id,binding_id,provider_item_id,item_kind,payload_json,is_final,created_at,updated_at) VALUES(?1,'binding',?1,'agentMessage','{}',1,'now','now')",[format!("item-{i:05}")]).unwrap();
    }
    tx.commit().unwrap();
    let snap = s.conversation_snapshot(&id).unwrap();
    assert_eq!(snap.turns.len(), 500);
    assert_eq!(snap.turns.first().unwrap().id, "turn-00001");
    assert_eq!(snap.turns.last().unwrap().id, "turn-00500");
    assert_eq!(snap.items.len(), PAGE_ITEMS);
    assert_eq!(snap.items.last().unwrap().id, "item-09999");
    let mut ids = snap.items.iter().map(|i| i.id.clone()).collect::<Vec<_>>();
    let mut cursor = snap.older_cursor;
    // A new arrival must not shift the keyset for older pages.
    c.execute("INSERT INTO transcript_items(id,binding_id,provider_item_id,item_kind,payload_json,is_final,created_at,updated_at) VALUES('new','binding','new','agentMessage','{}',1,'zzz','zzz')",[]).unwrap();
    while let Some(before) = cursor {
        let page = s.transcript_page(&id, Some(&before)).unwrap();
        assert!(page.items.len() <= PAGE_ITEMS);
        ids.extend(page.items.into_iter().map(|i| i.id));
        cursor = page.next_cursor;
    }
    assert_eq!(ids.len(), 10_000);
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 10_000);
    let mut exported = Vec::new();
    s.export_transcript(&id, |item| {
        exported.push(item.id);
        Ok(())
    })
    .unwrap();
    assert_eq!(exported.len(), 10_001);
    assert_eq!(exported.first().unwrap(), "item-00000");
    assert_eq!(exported.last().unwrap(), "new");
    assert!(s.session_snapshot(&id).is_ok());
}
#[test]
fn page_byte_limit_and_successor_deduplication_remain_bounded() {
    let (_t, s, id) = fixture();
    let c = s.connection().unwrap();
    let payload = json!({"text":"x".repeat(3*1024*1024)}).to_string();
    for i in 0..3 {
        c.execute("INSERT INTO transcript_items(id,binding_id,provider_item_id,item_kind,payload_json,is_final,created_at,updated_at) VALUES(?1,'binding',?1,'agentMessage',?2,1,'now','now')",params![format!("item-{i}"),payload]).unwrap();
    }
    let page = s.transcript_page(&id, None).unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(
        s.transcript_page(&id, page.next_cursor.as_ref())
            .unwrap()
            .items
            .len(),
        1
    );
    c.execute("INSERT INTO session_bindings(id,session_id,runtime_namespace,provider_thread_id,incarnation,created_at) VALUES('successor',?1,'audit','successor-thread',2,'now')",[&id]).unwrap();
    c.execute("INSERT INTO transcript_items(id,binding_id,provider_item_id,item_kind,payload_json,is_final,created_at,updated_at) VALUES('replacement','successor','item-2','agentMessage','{}',1,'now','now')",[]).unwrap();
    let page = s.transcript_page(&id, None).unwrap();
    assert!(!page.items.iter().any(|i| i.id == "item-2"));
    assert!(page.items.iter().any(|i| i.id == "replacement"));
}

#[test]
fn a_page_is_chosen_without_reading_payloads() {
    let (_t, s, id) = fixture();
    let c = s.connection().unwrap();
    let plan = c
        .prepare(&format!("EXPLAIN QUERY PLAN {}", page_query("<", "DESC")))
        .unwrap()
        .query_map(params![id, None::<String>, None::<String>, 10], |row| {
            row.get::<_, String>(3)
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(
        plan.iter()
            .any(|row| row.contains("COVERING INDEX transcript_items_page")),
        "{plan:?}"
    );
}

#[test]
fn the_message_walk_reads_no_tool_payloads_and_chooses_pages_from_the_index() {
    let (_t, s, id) = fixture();
    let c = s.connection().unwrap();
    let plan = c
        .prepare(&format!("EXPLAIN QUERY PLAN {}", message_page_query()))
        .unwrap()
        .query_map(params![id, None::<String>, None::<String>, 10], |row| {
            row.get::<_, String>(3)
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(
        plan.iter()
            .any(|row| row.contains("COVERING INDEX transcript_items_page")),
        "{plan:?}"
    );
    for (item, kind, is_final) in [
        ("tool", "toolCall", 1),
        ("streaming", "agentMessage", 0),
        ("final", "agentMessage", 1),
    ] {
        c.execute("INSERT INTO transcript_items(id,binding_id,provider_item_id,item_kind,payload_json,is_final,created_at,updated_at) VALUES(?1,'binding',?1,?2,'{}',?3,'now','now')",params![item,kind,is_final]).unwrap();
    }
    let mut visited = Vec::new();
    s.visit_messages_newest_first(&id, |kind, payload| {
        visited.push((kind.to_owned(), payload.to_owned()));
        true
    })
    .unwrap();
    assert_eq!(visited, [("agentMessage".to_owned(), "{}".to_owned())]);
}
