//! Upgrade to migration 28: inventory snapshots stop accumulating history,
//! other kinds keep theirs, and conversation reads use indexes.
use crate::workbench::store::{schema::historical_test_database, Store, CURRENT_SCHEMA_VERSION};

fn count(db: &rusqlite::Connection, query: &str) -> i64 {
    db.query_row(query, [], |row| row.get(0)).unwrap()
}

fn plan(db: &rusqlite::Connection, query: &str) -> String {
    let mut statement = db.prepare(&format!("EXPLAIN QUERY PLAN {query}")).unwrap();
    let rows = statement
        .query_map([], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    rows.join("\n")
}

#[test]
fn upgrade_trims_inventory_history_and_indexes_conversation_reads() {
    let temp = tempfile::tempdir().unwrap();
    let old = historical_test_database(temp.path(), 27);
    old.execute_batch(
        "INSERT INTO workspaces(id,name,created_at,updated_at) VALUES('ws','Legacy','t','t');
         INSERT INTO project_records VALUES('inventory_ws','ws','inventory',1,'{\"files\":[]}','t1');
         UPDATE project_records SET revision=2,body_json='{\"files\":[1]}',updated_at='t2' WHERE id='inventory_ws';
         INSERT INTO project_records VALUES('reply','ws','response',1,'{\"text\":\"first\"}','t1');
         UPDATE project_records SET revision=2,body_json='{\"text\":\"second\"}',updated_at='t2' WHERE id='reply';",
    )
    .unwrap();
    // Before the upgrade, every replaced snapshot was archived.
    assert_eq!(
        count(&old, "SELECT COUNT(*) FROM project_record_history"),
        2
    );
    drop(old);

    let store = Store::open_at(temp.path()).unwrap();
    let db = store.connection().unwrap();
    assert_eq!(
        count(&db, "PRAGMA user_version"),
        i64::from(CURRENT_SCHEMA_VERSION)
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM project_record_history WHERE kind='inventory'"
        ),
        0
    );
    db.execute_batch(
        "UPDATE project_records SET revision=3,body_json='{\"files\":[2]}',updated_at='t3' WHERE id='inventory_ws';
         UPDATE project_records SET revision=3,body_json='{\"text\":\"third\"}',updated_at='t3' WHERE id='reply';",
    )
    .unwrap();
    // New inventory snapshots are not archived; response history is intact.
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM project_record_history WHERE object_id='inventory_ws'"
        ),
        0
    );
    let replies = db
        .prepare(
            "SELECT body_json FROM project_record_history WHERE object_id='reply' ORDER BY revision",
        )
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(replies, ["{\"text\":\"first\"}", "{\"text\":\"second\"}"]);

    // A turn's newest items come from the index in order, without a sort.
    for query in [
        "SELECT id FROM transcript_items WHERE turn_id='turn' AND is_final=1 ORDER BY created_at DESC,id DESC LIMIT 5",
        "SELECT MAX(updated_at) FROM transcript_items WHERE turn_id='turn'",
    ] {
        let plan = plan(&db, query);
        assert!(plan.contains("transcript_items_turn"), "{plan}");
        assert!(!plan.contains("TEMP B-TREE"), "{plan}");
    }
    for (query, index) in [
        (
            "SELECT id FROM turns WHERE config_snapshot_id='c'",
            "turns_config_snapshot",
        ),
        (
            "SELECT id FROM turns WHERE context_snapshot_id='c'",
            "turns_context_snapshot",
        ),
        (
            "SELECT id FROM config_snapshots WHERE session_id='s'",
            "config_snapshots_session",
        ),
        (
            "SELECT id FROM context_snapshots WHERE session_id='s'",
            "context_snapshots_session",
        ),
        (
            "SELECT id FROM research_executions WHERE session_id='s'",
            "research_executions_session",
        ),
        (
            "SELECT id FROM research_executions WHERE binding_id='b'",
            "research_executions_binding",
        ),
        (
            "SELECT id FROM tool_receipts WHERE session_id='s'",
            "tool_receipts_session",
        ),
        (
            "SELECT id FROM conversation_tool_receipts WHERE session_id='s'",
            "conversation_tool_receipts_session",
        ),
        (
            "SELECT id FROM review_handoffs WHERE session_id='s'",
            "review_handoffs_session",
        ),
        (
            "SELECT id FROM session_bindings WHERE provider_thread_id='t'",
            "session_bindings_thread",
        ),
        (
            "SELECT id FROM desk_records WHERE supersedes='d'",
            "desk_records_supersedes",
        ),
        (
            "SELECT id FROM desk_note_versions WHERE workspace_id='ws'",
            "desk_note_versions_workspace",
        ),
    ] {
        let plan = plan(&db, query);
        assert!(plan.contains(index), "{plan}");
    }
}
