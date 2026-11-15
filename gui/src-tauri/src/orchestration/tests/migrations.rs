use super::*;

#[test]
fn task_store_upgrades_old_schemas_and_reopens_current_data() {
    for version in 1..=5 {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        let run = store
            .create(
                "migration",
                chain(vec![delay("wait")]),
                Scope::default(),
                json!({}),
                0,
                false,
            )
            .unwrap();
        let c = store.connection().unwrap();
        if version < 5 {
            c.execute_batch("DROP TABLE usage_calls;").unwrap();
        }
        if version < 4 {
            c.execute_batch("DROP TABLE damaged_records;").unwrap();
        }
        if version < 3 {
            c.execute_batch("DROP TABLE discovery_selections; DROP TABLE discovery_events; DROP TABLE discovery_children; DROP TABLE discovery_records; DROP TABLE discovery_runs;").unwrap();
        }
        if version < 2 {
            c.execute_batch("DROP TABLE mission_answers; DROP TABLE mission_events; DROP TABLE mission_children; DROP TABLE missions;").unwrap();
        }
        c.pragma_update(None, "user_version", version).unwrap();
        drop(c);
        let upgraded = Store::open(temp.path()).unwrap();
        assert_eq!(upgraded.get(&run.id).unwrap().name, run.name);
        let c = upgraded.connection().unwrap();
        assert_eq!(
            c.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            5
        );
        c.execute(
            "INSERT INTO damaged_records VALUES ('run', 'preserved', 'diagnostic')",
            [],
        )
        .unwrap();
        c.execute("INSERT INTO usage_calls(task_id,owner_id,address,operation,class,charge) VALUES (?1,'owner','wait','call','api',7)", [&run.id]).unwrap();
        for (target, filename) in [
            (4, "tasks-before-repair-v4.sqlite3"),
            (5, "tasks-before-usage-v5.sqlite3"),
        ] {
            if version < target {
                let backup = rusqlite::Connection::open(temp.path().join(filename)).unwrap();
                assert_eq!(
                    backup
                        .query_row("SELECT COUNT(*) FROM runs WHERE id=?1", [&run.id], |r| r
                            .get::<_, i64>(0))
                        .unwrap(),
                    1
                );
                assert_eq!(
                    backup
                        .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                        .unwrap(),
                    target - 1
                );
            }
        }
        drop(c);
        let reopened = Store::open(temp.path()).unwrap();
        reopened.recover().unwrap();
        assert_eq!(reopened.automations("active", 0).unwrap().len(), 1);
        let c = reopened.connection().unwrap();
        assert_eq!(
            c.query_row("SELECT charge FROM usage_calls", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            7
        );
        assert_eq!(
            c.query_row("SELECT diagnostic FROM damaged_records", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "diagnostic"
        );
    }
}

#[test]
fn task_store_rejects_future_schemas() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path()).unwrap();
    store
        .connection()
        .unwrap()
        .pragma_update(None, "user_version", 6)
        .unwrap();
    assert_eq!(
        Store::open(temp.path()).unwrap_err(),
        "Task store was written by a newer Pipeline version"
    );
}
