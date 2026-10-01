//! Project home settings added for the derived overview.
use super::super::store::CreateWorkspaceRequest;
use super::*;

#[test]
fn target_date_is_optional_validated_and_readable_from_older_settings() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(&temp.path().join("store")).unwrap();
    let ws = store
        .create_workspace(CreateWorkspaceRequest {
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
