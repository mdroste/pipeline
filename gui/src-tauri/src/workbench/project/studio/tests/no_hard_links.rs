//! Runs an editor save on a real volume without hard links. Mount an exFAT disk
//! image and set `PIPELINE_NO_HARD_LINK_FOLDER` to an empty folder on it.
use super::*;

#[test]
#[ignore = "Needs a mounted volume without hard links"]
fn an_editor_save_and_its_undo_work_on_a_real_volume() {
    let folder = PathBuf::from(std::env::var("PIPELINE_NO_HARD_LINK_FOLDER").unwrap());
    let temp = tempfile::tempdir().unwrap();
    fs::write(folder.join("main.tex"), "before").unwrap();
    let store = Store::open_at(&temp.path().join("store")).unwrap();
    let ws = store
        .create_workspace(crate::workbench::store::CreateWorkspaceRequest {
            name: "Removable drive".into(),
            root: Some(folder.to_string_lossy().into_owned()),
            operation_id: "create".into(),
        })
        .unwrap()
        .record
        .id;
    let f = Fixture {
        _temp: temp,
        store,
        ws,
        root: folder,
    };
    let opened = editor_file(&f.store, &f.ws, None, "main.tex").unwrap();
    let saved = act(
        &f,
        StudioAction::SaveText {
            checkpoint_id: None,
            path: "main.tex".into(),
            expected_hash: opened.hash,
            content: "after".into(),
        },
    );
    assert_eq!(
        fs::read_to_string(f.root.join("main.tex")).unwrap(),
        "after"
    );
    mutate(
        &f.store,
        ProjectMutation {
            workspace_id: f.ws.clone(),
            operation_id: id("test").unwrap(),
            action: ProjectAction::Undo {
                application_id: saved["application"]["id"].as_str().unwrap().into(),
            },
        },
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(f.root.join("main.tex")).unwrap(),
        "before"
    );
    let leftovers: Vec<_> = fs::read_dir(&f.root)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".pipeline-apply-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}
