//! File creation shares acceptance, conflict detection, and undo with text edits.
use super::test_support::*;
use super::*;

fn create(
    f: &Fixture,
    checkpoint: Option<&str>,
    path: &str,
    bytes: &[u8],
    operation: &str,
) -> WorkbenchResult<Value> {
    studio_mutate(
        &f.store,
        StudioMutation {
            workspace_id: f.ws.clone(),
            operation_id: operation.into(),
            action: StudioAction::CreateFile {
                checkpoint_id: checkpoint.map(str::to_owned),
                path: path.into(),
                base64: base64::engine::general_purpose::STANDARD.encode(bytes),
            },
        },
    )
}

#[test]
fn new_binary_files_are_recoverable_and_never_replace_existing_paths() {
    let f = fixture();
    let created = create(&f, None, "downloads/data.bin", &[0, 255, 1], "create").unwrap();
    assert_eq!(
        fs::read(f.root.join("downloads/data.bin")).unwrap(),
        [0, 255, 1]
    );
    assert_eq!(
        create(&f, None, "downloads/data.bin", &[0, 255, 1], "create").unwrap(),
        created
    );
    assert!(create(&f, None, "downloads/data.bin", b"clobber", "other").is_err());
    let application = created["application"]["id"].as_str().unwrap();
    tasks::undo(&f.store, &f.ws, application).unwrap();
    assert!(!f.root.join("downloads/data.bin").exists());
}

#[test]
fn new_working_copy_files_wait_for_acceptance() {
    let f = fixture();
    let cp = copy(&f, &[]);
    let created = create(&f, Some(&cp.id), "new/paper.md", b"Draft", "create").unwrap();
    assert!(created["application"].is_null());
    assert!(!f.root.join("new/paper.md").exists());
    let cp = record(&f.store, &f.ws, &cp.id, "checkpoint").unwrap();
    tasks::apply(
        &f.store,
        &f.ws,
        &cp.id,
        vec!["new/paper.md".into()],
        cp.revision,
    )
    .unwrap();
    assert_eq!(fs::read(f.root.join("new/paper.md")).unwrap(), b"Draft");
}

#[test]
fn creates_reject_traversal_metadata_and_linked_parents() {
    let f = fixture();
    for path in ["../escape", "/tmp/escape", ".git/config"] {
        assert!(create(&f, None, path, b"bad", path).is_err());
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(f._temp.path(), f.root.join("linked")).unwrap();
        assert!(create(&f, None, "linked/escape", b"bad", "linked").is_err());
        assert!(!f._temp.path().join("escape").exists());
    }
}
