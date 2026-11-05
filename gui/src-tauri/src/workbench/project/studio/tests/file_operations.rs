use super::*;

#[test]
fn file_operations_run_through_studio_and_stay_on_record() {
    let f = fixture();
    fs::write(f.root.join("one.txt"), "one").unwrap();
    fs::write(f.root.join("two.txt"), "two").unwrap();
    act(
        &f,
        StudioAction::CreateFolder {
            path: "archive".into(),
        },
    );
    let moved = act(
        &f,
        StudioAction::MoveEntry {
            from: "one.txt".into(),
            to: "archive/one.txt".into(),
        },
    );
    assert_eq!(moved["to"], "archive/one.txt");
    assert_eq!(
        fs::read_to_string(f.root.join("archive/one.txt")).unwrap(),
        "one"
    );
    let recorded: i64 = f
        .store
        .connection()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM project_operations WHERE response_json LIKE '%archive/one.txt%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(recorded, 1);
    let collision = studio_mutate(
        &f.store,
        StudioMutation {
            workspace_id: f.ws.clone(),
            operation_id: id("test").unwrap(),
            action: StudioAction::MoveEntry {
                from: "two.txt".into(),
                to: "archive/one.txt".into(),
            },
        },
    )
    .unwrap_err();
    assert!(collision.message.contains("Already exists"));
    assert_eq!(fs::read_to_string(f.root.join("two.txt")).unwrap(), "two");
    assert_eq!(
        fs::read_to_string(f.root.join("archive/one.txt")).unwrap(),
        "one"
    );
    let outside = f._temp.path().join("outside.csv");
    fs::write(&outside, "a,b\n").unwrap();
    act(
        &f,
        StudioAction::ImportFile {
            directory: "archive".into(),
            source: outside.to_string_lossy().into_owned(),
        },
    );
    assert_eq!(
        fs::read_to_string(f.root.join("archive/outside.csv")).unwrap(),
        "a,b\n"
    );
    assert!(fs::metadata(&outside).is_ok());
    let taken = |action| {
        studio_mutate(
            &f.store,
            StudioMutation {
                workspace_id: f.ws.clone(),
                operation_id: id("test").unwrap(),
                action,
            },
        )
        .unwrap_err()
        .message
    };
    assert_eq!(
        taken(StudioAction::ImportFile {
            directory: "archive".into(),
            source: outside.to_string_lossy().into_owned(),
        }),
        "Already exists: archive/outside.csv"
    );
    act(
        &f,
        StudioAction::CreateFile {
            checkpoint_id: None,
            path: "archive/empty.md".into(),
            base64: String::new(),
        },
    );
    assert_eq!(
        taken(StudioAction::CreateFile {
            checkpoint_id: None,
            path: "archive/empty.md".into(),
            base64: String::new(),
        }),
        "Already exists: archive/empty.md"
    );
    assert_eq!(
        fs::metadata(f.root.join("archive/empty.md")).unwrap().len(),
        0
    );
}
