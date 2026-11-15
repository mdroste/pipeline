use super::*;
use serde_json::json;

fn insert(s: &Store, kind: &str, id: &str, state: &str, time: i64) {
    let body =
        json!({"name":id,"definition":{"name":id,"prompt":id},"reason":"Saved result"}).to_string();
    let sql = match kind {
        "task" => "INSERT INTO runs(id,operation,fingerprint,revision,state,updated_at,body,due_at) VALUES(?1,?1,'test',0,?2,?3,?4,123)",
        "research" => "INSERT INTO missions(id,operation,fingerprint,revision,state,workspace_id,updated_at,body) VALUES(?1,?1,'test',0,?2,'project',?3,?4)",
        _ => "INSERT INTO discovery_runs(id,operation,fingerprint,revision,state,workspace_id,deadline_at,updated_at,body) VALUES(?1,?1,'test',0,?2,'project',1000,?3,?4)",
    };
    s.connection()
        .unwrap()
        .execute(sql, params![id, state, time, body])
        .unwrap();
}

#[test]
fn combines_owners_without_listing_their_child_tasks() {
    let root = tempfile::tempdir().unwrap();
    let s = Store::open(root.path()).unwrap();
    insert(&s, "task", "same-id", "running", 10);
    insert(&s, "research", "same-id", "waiting", 20);
    insert(&s, "discovery", "portfolio", "awaitingSelection", 30);
    insert(&s, "task", "research-child", "running", 40);
    insert(&s, "task", "discovery-child", "running", 50);
    let c = s.connection().unwrap();
    c.execute("INSERT INTO mission_children(task_id,mission_id,phase,round) VALUES('research-child','same-id','plan',1)", []).unwrap();
    c.execute("INSERT INTO discovery_children(task_id,run_id,phase) VALUES('discovery-child','portfolio','draft')", []).unwrap();
    let rows = s.automations("active", 0).unwrap();
    assert_eq!(
        rows.iter()
            .map(|r| (r.kind.as_str(), r.id.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("discovery", "portfolio"),
            ("research", "same-id"),
            ("task", "same-id")
        ]
    );
    assert_eq!(rows[0].reason.as_deref(), Some("Saved result"));
    assert_eq!(rows[2].due_at, Some(123));
    assert_eq!(rows[1].due_at, None);
}

#[test]
fn terminal_states_go_to_history_and_attention_stays_active() {
    let root = tempfile::tempdir().unwrap();
    let s = Store::open(root.path()).unwrap();
    for (kind, states) in [
        ("task", vec!["finished", "cancelled", "failed"]),
        ("research", vec!["completed", "exhausted", "stopped"]),
        (
            "discovery",
            vec![
                "completed",
                "partial",
                "exhausted",
                "blocked",
                "failed",
                "cancelled",
            ],
        ),
    ] {
        for state in states {
            insert(&s, kind, &format!("{kind}-{state}"), state, 20);
        }
        for state in ["attention", "paused", "running"] {
            insert(&s, kind, &format!("{kind}-{state}"), state, 10);
        }
    }
    assert_eq!(s.automations("history", 0).unwrap().len(), 12);
    assert_eq!(s.automations("active", 0).unwrap().len(), 9);
    assert!(s.automations("scheduled", 0).is_err());
}

#[test]
fn filters_before_pagination_and_pages_all_types_in_one_order() {
    let root = tempfile::tempdir().unwrap();
    let s = Store::open(root.path()).unwrap();
    for n in 0..70 {
        let kind = ["task", "research", "discovery"][n % 3];
        insert(&s, kind, &format!("active-{n:02}"), "running", n as i64);
        insert(
            &s,
            "research",
            &format!("finished-{n:02}"),
            "completed",
            100 + n as i64,
        );
    }
    let first = s.automations("active", 0).unwrap();
    let second = s.automations("active", 50).unwrap();
    assert_eq!(first.len(), 50);
    assert_eq!(second.len(), 20);
    let names: Vec<_> = first.into_iter().chain(second).map(|r| r.name).collect();
    assert_eq!(
        names,
        (0..70)
            .rev()
            .map(|n| format!("active-{n:02}"))
            .collect::<Vec<_>>()
    );
    assert!(s.automations("active", 100).unwrap().is_empty());
}

#[test]
fn unreadable_records_do_not_poison_the_combined_automation_index() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    insert(&store, "task", "healthy", "running", 1);
    insert(&store, "task", "quarantined", "unreadable", 2);
    for kind in ["task", "research", "discovery"] {
        insert(&store, kind, kind, "running", 3);
        let table = match kind {
            "task" => "runs",
            "research" => "missions",
            _ => "discovery_runs",
        };
        store
            .connection()
            .unwrap()
            .execute(
                &format!("UPDATE {table} SET body='not JSON' WHERE id=?1"),
                [kind],
            )
            .unwrap();
    }
    let rows = store.automations("active", 0).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "healthy");
    assert!(store.automations("history", 0).unwrap().is_empty());
}
