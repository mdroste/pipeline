//! Project home settings added for the derived overview.
use super::super::store::CreateWorkspaceRequest;
use super::*;

#[test]
fn home_reads_one_snapshot_and_releases_it_before_filesystem_work() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(&temp.path().join("store")).unwrap();
    let ws = store
        .create_legacy_workspace(CreateWorkspaceRequest {
            name: "Snapshot".into(),
            root: None,
            operation_id: "snapshot_workspace".into(),
        })
        .unwrap()
        .record
        .id;
    let conn = store.connection().unwrap();
    conn.execute("INSERT INTO research_notes(id,workspace_id,kind,body,state,origin,pinned,revision,created_at,updated_at) VALUES('note',?1,'question','OLD_SENTINEL','accepted','user',1,1,'t','t')", [&ws]).unwrap();
    let before = crate::workbench::store::connection_opens();
    let prepared = home::prepare_home(&store, &ws).unwrap();
    assert_eq!(crate::workbench::store::connection_opens() - before, 1);
    // The snapshot transaction is over. A concurrent writer can commit before
    // filesystem status is checked, without mixing note text and context.
    conn.execute(
        "UPDATE research_notes SET body='NEW_SENTINEL',revision=2 WHERE id='note'",
        [],
    )
    .unwrap();
    let snapshot = prepared.finish().unwrap();
    assert_eq!(snapshot.notes[0].body, "OLD_SENTINEL");
    assert!(snapshot.context_preview.contains("OLD_SENTINEL"));
    assert!(!snapshot.context_preview.contains("NEW_SENTINEL"));
    let before = crate::workbench::store::connection_opens();
    let current = home(&store, &ws).unwrap();
    assert_eq!(crate::workbench::store::connection_opens() - before, 1);
    assert_eq!(current.notes[0].body, "NEW_SENTINEL");
    let before = crate::workbench::store::connection_opens();
    assert_eq!(
        project_context(&store, &ws).unwrap(),
        current.context_preview
    );
    assert_eq!(crate::workbench::store::connection_opens() - before, 1);
}

#[test]
fn target_date_is_optional_validated_and_readable_from_older_settings() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(&temp.path().join("store")).unwrap();
    let ws = store
        .create_legacy_workspace(CreateWorkspaceRequest {
            name: "Research".into(),
            root: None,
            operation_id: "create".into(),
        })
        .unwrap()
        .record
        .id;
    let save = |expected_revision: i64, settings: ProjectHomeSettings| {
        mutate(
            &store,
            ProjectMutation {
                workspace_id: ws.clone(),
                operation_id: id("test").unwrap(),
                action: ProjectAction::SaveHome {
                    expected_revision,
                    settings,
                },
            },
        )
    };
    // Settings saved before the field existed still decode.
    let legacy: ProjectHomeSettings = serde_json::from_value(json!({
        "manuscriptRevisionId": null,
        "baselineExecutionId": null
    }))
    .unwrap();
    assert_eq!(legacy.target_date, None);

    let saved = save(
        0,
        ProjectHomeSettings {
            target_date: Some("2026-11-15".into()),
            target_label: "Resubmission".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(saved["body"]["targetDate"], "2026-11-15");
    assert_eq!(legacy.stage, None);
    let staged = save(
        saved["revision"].as_i64().unwrap(),
        ProjectHomeSettings {
            kind: Some("theory".into()),
            stage: Some("revising".into()),
            venue: "Econometrica".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(staged["body"]["stage"], "revising");
    assert!(save(
        staged["revision"].as_i64().unwrap(),
        ProjectHomeSettings {
            stage: Some("published".into()),
            ..Default::default()
        },
    )
    .is_err());
    let saved = staged;
    assert!(save(
        saved["revision"].as_i64().unwrap(),
        ProjectHomeSettings {
            target_date: Some("next spring".into()),
            ..Default::default()
        },
    )
    .is_err());
    // No paper is chosen, so there is nothing to compare a folder against.
    assert_eq!(home(&store, &ws).unwrap().working_copy_changed, 0);
}

#[test]
fn put_refuses_a_record_id_owned_by_another_workspace() {
    struct Fixture {
        store: Store,
        ws: String,
    }
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(&temp.path().join("store")).unwrap();
    let workspace = |name: &str| {
        store
            .create_legacy_workspace(CreateWorkspaceRequest {
                name: name.into(),
                root: None,
                operation_id: format!("create-{name}"),
            })
            .unwrap()
            .record
            .id
    };
    let ws = workspace("Research");
    let other = workspace("Other");
    let f = Fixture { store, ws };
    put(
        &f.store,
        &f.ws,
        "zotero_x",
        "bibliography",
        0,
        &serde_json::json!({"owner":"A"}),
    )
    .unwrap();
    let error = put(
        &f.store,
        &other,
        "zotero_x",
        "bibliography",
        0,
        &serde_json::json!({"owner":"B"}),
    )
    .unwrap_err();
    assert!(
        error.message.contains("another project"),
        "{}",
        error.message
    );
    assert!(put(
        &f.store,
        &f.ws,
        "zotero_x",
        "task",
        0,
        &serde_json::json!({"owner":"A-task"}),
    )
    .is_err());
    let kept = record(&f.store, &f.ws, "zotero_x", "bibliography").unwrap();
    assert_eq!(kept.body["owner"], "A");
    assert_eq!(kept.revision, 1);
    assert!(record(&f.store, &other, "zotero_x", "bibliography").is_err());
}
