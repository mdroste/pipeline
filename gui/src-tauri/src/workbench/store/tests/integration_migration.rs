//! Upgrade a populated pre-integration database, including history and references.
use crate::workbench::store::{schema::historical_test_database, Store, CURRENT_SCHEMA_VERSION};
use rusqlite::params;

#[test]
fn integration_kinds_upgrade_without_losing_history_references_or_search() {
    let temp = tempfile::tempdir().unwrap();
    let old = historical_test_database(temp.path(), 26);
    old.execute_batch(
        "INSERT INTO workspaces(id,name,created_at,updated_at) VALUES('ws','Legacy','t','t');
         INSERT INTO project_records VALUES('note','ws','theory',1,'{\"extension\":7}','t1');
         UPDATE project_records SET revision=2,body_json='{\"extension\":8}',updated_at='t2' WHERE id='note';
         INSERT INTO desk_records VALUES('first','ws','collection','First','{\"opaque\":true}','hash1',NULL,'t1');
         INSERT INTO desk_records VALUES('second','ws','collection','Second','{}','hash2','first','t2');
         INSERT INTO desk_records VALUES('plan','ws','experiment_plan','Plan','{}','plan-hash',NULL,'t3');
         INSERT INTO experiment_runs(plan_id,state,updated_at) VALUES('plan','ready','t3');",
    ).unwrap();
    drop(old);

    let store = Store::open_at(temp.path()).unwrap();
    let db = store.connection().unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .unwrap(),
        CURRENT_SCHEMA_VERSION
    );
    let retained: (i64, String) = db
        .query_row(
            "SELECT revision,body_json FROM project_records WHERE id='note'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(retained, (2, "{\"extension\":8}".into()));
    let original: String = db
        .query_row(
            "SELECT body_json FROM project_record_history WHERE object_id='note' AND revision=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(original, "{\"extension\":7}");
    assert_eq!(
        db.query_row(
            "SELECT supersedes FROM desk_records WHERE id='second'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "first"
    );
    assert_eq!(
        db.query_row(
            "SELECT body_json FROM desk_records WHERE id='first'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "{\"opaque\":true}"
    );
    assert_eq!(
        db.query_row("SELECT plan_id FROM experiment_runs", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "plan"
    );

    for kind in [
        "check_policy",
        "check_state",
        "connection",
        "github_comment",
    ] {
        db.execute(
            "INSERT INTO project_records VALUES(?1,'ws',?1,1,'{}','new')",
            [kind],
        )
        .unwrap();
    }
    for kind in [
        "github_collaboration",
        "submission",
        "source_selection",
        "response_context",
        "check_receipt",
    ] {
        db.execute(
            "INSERT INTO desk_records VALUES(?1,'ws',?1,?1,'{}','new-hash',NULL,'new')",
            [kind],
        )
        .unwrap();
    }
    for table in ["project_records", "desk_records"] {
        assert!(db
            .execute(&format!("UPDATE {table} SET kind='unknown_kind'"), [])
            .is_err());
    }
    let generation = || {
        db.query_row(
            "SELECT generation FROM research_index_state WHERE workspace_id='ws'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
    };
    let before = generation();
    db.execute(
        "UPDATE project_records SET revision=3,body_json='{}' WHERE id='note'",
        [],
    )
    .unwrap();
    assert!(generation() > before);
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM project_record_history WHERE object_id='note'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
    let before = generation();
    db.execute(
        "UPDATE desk_records SET title='Updated' WHERE id='second'",
        [],
    )
    .unwrap();
    assert!(generation() > before);
    let before = generation();
    db.execute(
        "INSERT INTO project_records VALUES('inventory','ws','inventory',1,'{}','t')",
        [],
    )
    .unwrap();
    assert_eq!(
        generation(),
        before,
        "inventory must remain excluded from invalidation"
    );
    assert!(db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM research_search_sources WHERE id=?1)",
            params!["source_selection"],
            |r| r.get::<_, bool>(0)
        )
        .unwrap());
    assert!(db
        .prepare("PRAGMA foreign_key_check")
        .unwrap()
        .query([])
        .unwrap()
        .next()
        .unwrap()
        .is_none());
    assert!(db
        .execute(
            "INSERT INTO experiment_runs(plan_id,state,updated_at) VALUES('missing','ready','t')",
            []
        )
        .is_err());
}
