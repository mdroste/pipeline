use super::*;

#[test]
fn project_sessions_are_filtered_before_pagination() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path()).unwrap();
    let mut wanted = Vec::new();
    for index in 0..115 {
        let session = if (2..63).contains(&index) {
            "unrelated"
        } else {
            "own"
        };
        let mut run = store
            .create(
                &format!("list-{index}"),
                chain(vec![delay("one")]),
                Scope {
                    session_id: Some(session.into()),
                    ..Default::default()
                },
                json!({}),
                0,
                true,
            )
            .unwrap();
        run.state = if index >= 63 { "running" } else { "finished" }.into();
        store.save(&mut run, "finished", "Test result").unwrap();
        // Deterministic order without waiting for the wall clock.
        store
            .connection()
            .unwrap()
            .execute(
                "UPDATE runs SET updated_at=?2 WHERE id=?1",
                rusqlite::params![run.id, index],
            )
            .unwrap();
        if index < 2 {
            wanted.push(run.id);
        }
    }
    assert!(store
        .list("session", None, 0)
        .unwrap()
        .iter()
        .all(|run| !wanted.contains(&run.id)));
    let scoped = store.project_activity(&["own".into()], 0).unwrap();
    assert_eq!(scoped.len(), 2);
    assert!(scoped.iter().all(|run| wanted.contains(&run.id)));
    assert_eq!(store.project_activity(&["own".into()], 1).unwrap().len(), 1);
    assert!(store.project_activity(&[], 0).unwrap().is_empty());
}
