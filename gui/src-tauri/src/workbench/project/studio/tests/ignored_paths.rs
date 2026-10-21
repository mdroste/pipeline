//! Finding 13 (docs/bug-triage-2026-10.md): paths that task capture ignores
//! cannot be edited or checkpointed, so acceptance never unlinks them.
use super::*;

#[cfg(windows)]
#[test]
fn public_file_readers_reject_ancestor_junctions() {
    let f = fixture();
    let outside = f._temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("private.md"), "outside sentinel").unwrap();
    assert!(std::process::Command::new("cmd")
        .args(["/D", "/C", "mklink", "/J"])
        .arg(f.root.join("linked"))
        .arg(&outside)
        .status()
        .unwrap()
        .success());
    let session = f
        .store
        .create_session(crate::workbench::store::CreateSessionRequest {
            workspace_id: Some(f.ws.clone()),
            title: "Files".into(),
            operation_id: "files".into(),
        })
        .unwrap()
        .record;
    assert!(read_workspace_file(
        &f.store,
        FileReadRequest {
            workspace_id: f.ws.clone(),
            path: "linked/private.md".into(),
            checkpoint_id: None,
            revision_id: None
        }
    )
    .is_err());
    assert!(read_conversation_file(&f.store, &session.id, "linked/private.md").is_err());
    fs::write(f.root.join("ordinary.md"), "inside").unwrap();
    assert_eq!(
        read_conversation_file(&f.store, &session.id, "ordinary.md")
            .unwrap()
            .text
            .as_deref(),
        Some("inside")
    );
}

#[test]
fn metadata_case_aliases_are_refused_by_read_edit_save_and_checkpoint() {
    let f = fixture();
    for directory in [
        ".git",
        ".pipeline-tasks",
        ".pipeline-apply-recovery",
        "target",
        "node_modules",
    ] {
        fs::create_dir_all(f.root.join(directory)).unwrap();
        fs::write(f.root.join(directory).join("notes.md"), "original metadata").unwrap();
        let task = tasks::create_task(
            &f.store,
            &f.ws,
            "Boundary test".into(),
            None,
            vec![],
            vec![],
        )
        .unwrap();
        for alias in [
            directory.to_ascii_uppercase(),
            format!("{} ", directory.to_ascii_uppercase()),
            format!("{}.", directory.to_ascii_uppercase()),
        ] {
            let path = format!("{alias}/notes.md");
            if files::reserved_component(&alias) {
                assert!(
                    read_workspace_file(
                        &f.store,
                        FileReadRequest {
                            workspace_id: f.ws.clone(),
                            path: path.clone(),
                            checkpoint_id: None,
                            revision_id: None
                        }
                    )
                    .is_err(),
                    "read {path}"
                );
            }
            assert!(
                editor_file(&f.store, &f.ws, None, &path).is_err(),
                "editor {path}"
            );
            assert!(
                studio_mutate(
                    &f.store,
                    StudioMutation {
                        workspace_id: f.ws.clone(),
                        operation_id: id("reject").unwrap(),
                        action: StudioAction::SaveText {
                            checkpoint_id: None,
                            path: path.clone(),
                            expected_hash: hash(b"original metadata"),
                            content: "changed".into()
                        }
                    }
                )
                .is_err(),
                "save {path}"
            );
            assert!(
                tasks::checkpoint(&f.store, &f.ws, &task.id, vec![path.clone()], "copy").is_err(),
                "checkpoint {path}"
            );
            assert_eq!(
                fs::read_to_string(f.root.join(directory).join("notes.md")).unwrap(),
                "original metadata"
            );
        }
    }
    assert!(files::relative("notes/ordinary.md").is_ok());
    for path in [
        ".GiT/config",
        ".pIpElInE-TaSkS/file",
        "notes/.PiPeLiNe-ApPlY-claim",
    ] {
        assert!(files::relative(path).is_err(), "{path}");
    }
}

#[test]
fn ignored_paths_cannot_be_edited_or_captured_into_a_deletion() {
    let f = fixture();
    fs::create_dir_all(f.root.join("target")).unwrap();
    fs::write(f.root.join("target/notes.md"), "orig").unwrap();
    let expected = match editor_file(&f.store, &f.ws, None, "target/notes.md") {
        Ok(opened) => opened.hash,
        Err(_) => hash(b"orig"),
    };
    assert!(save_text(
        &f.store,
        &f.ws,
        None,
        "target/notes.md",
        &expected,
        "edited"
    )
    .is_err());
    assert_eq!(
        fs::read_to_string(f.root.join("target/notes.md")).unwrap(),
        "orig"
    );
    let task = tasks::create_task(
        &f.store,
        &f.ws,
        "Edit target/notes.md".into(),
        None,
        vec!["target/notes.md".into()],
        vec![],
    )
    .unwrap();
    assert!(tasks::checkpoint(
        &f.store,
        &f.ws,
        &task.id,
        vec!["target/notes.md".into()],
        "copy"
    )
    .is_err());
}
