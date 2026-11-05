//! Project fixtures shared by the platform-neutral and Unix-only tests.
use super::super::store::CreateWorkspaceRequest;
use super::*;
pub(super) struct Fixture {
    pub(super) _temp: tempfile::TempDir,
    pub(super) store: Store,
    pub(super) root: PathBuf,
    pub(super) ws: String,
}
pub(super) fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("research");
    fs::create_dir(&root).unwrap();
    let store = Store::open_at(&temp.path().join("store")).unwrap();
    let ws = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Research".into(),
            root: Some(root.to_string_lossy().into_owned()),
            operation_id: "create".into(),
        })
        .unwrap()
        .record
        .id;
    Fixture {
        _temp: temp,
        store,
        root,
        ws,
    }
}
pub(super) fn mutation(f: &Fixture, action: ProjectAction) -> ProjectRecord {
    serde_json::from_value(
        mutate(
            &f.store,
            ProjectMutation {
                workspace_id: f.ws.clone(),
                operation_id: id("test").unwrap(),
                action,
            },
        )
        .unwrap(),
    )
    .unwrap()
}
pub(super) fn task(f: &Fixture) -> ProjectRecord {
    mutation(
        f,
        ProjectAction::CreateTask {
            objective: "Check the main result".into(),
            anchor_id: None,
            expected_outputs: vec!["Revised manuscript".into()],
            expected_checks: vec!["Inspect changes".into()],
        },
    )
}
pub(super) fn copy(f: &Fixture, paths: &[&str]) -> ProjectRecord {
    let task = task(f);
    mutation(
        f,
        ProjectAction::Checkpoint {
            task_id: task.id,
            paths: paths.iter().map(|p| p.to_string()).collect(),
            backend: "copy".into(),
        },
    )
}
pub(super) fn capture(f: &Fixture, c: &ProjectRecord) -> ProjectRecord {
    mutation(
        f,
        ProjectAction::CaptureChanges {
            checkpoint_id: c.id.clone(),
        },
    )
}
