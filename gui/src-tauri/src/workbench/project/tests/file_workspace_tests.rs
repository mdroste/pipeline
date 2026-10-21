use super::*;

#[test]
fn file_workspace_reads_academic_sources_and_rejects_escape_and_symlinks() {
    let f = fixture();
    fs::write(f.root.join("analysis.do"), "local x 1\nregress y x\n").unwrap();
    let read = |path: &str| {
        read_workspace_file(
            &f.store,
            FileReadRequest {
                workspace_id: f.ws.clone(),
                path: path.into(),
                checkpoint_id: None,
                revision_id: None,
            },
        )
    };
    let file = read("analysis.do").unwrap();
    assert!(file.editable);
    assert_eq!(
        file.scope_root.as_deref(),
        f.root.canonicalize().unwrap().to_str()
    );
    assert_eq!(file.hash, hash(file.text.as_ref().unwrap().as_bytes()));
    for path in [
        "../secret",
        "/tmp/secret",
        ".git/config",
        ".pipeline-tasks/file",
        "a\\b",
    ] {
        assert!(read(path).is_err());
    }
    std::os::unix::fs::symlink(f.root.join("analysis.do"), f.root.join("link.do")).unwrap();
    assert!(read("link.do").is_err());
    for path in [
        "code.ado",
        "code.R",
        "code.jl",
        "code.m",
        "code.py",
        "data.yaml",
    ] {
        assert!(file_workspace::editable_source(path));
    }
}

#[test]
fn file_workspace_captured_links_never_follow_live_files() {
    let f = fixture();
    let revision = import(&f, "Original captured note");
    fs::write(f.root.join("main.md"), "Changed live file").unwrap();
    fs::write(f.root.join("other.md"), "Uncaptured sibling").unwrap();
    let read = |path: &str| {
        read_workspace_file(
            &f.store,
            FileReadRequest {
                workspace_id: f.ws.clone(),
                path: path.into(),
                checkpoint_id: None,
                revision_id: Some(revision.id.clone()),
            },
        )
    };
    let file = read("main.md").unwrap();
    assert_eq!(file.text.as_deref(), Some("Original captured note"));
    assert!(!file.editable);
    assert!(file.external_path.is_none());
    assert!(file.scope_root.is_none());
    assert!(read("other.md").is_err());
    let foreign = fixture();
    assert!(read_workspace_file(
        &f.store,
        FileReadRequest {
            workspace_id: foreign.ws,
            path: "main.md".into(),
            checkpoint_id: None,
            revision_id: Some(revision.id)
        }
    )
    .is_err());
}

#[test]
fn file_workspace_source_tree_assets_use_retained_hashes() {
    let f = fixture();
    fs::create_dir(f.root.join("figures")).unwrap();
    fs::write(f.root.join("main.md"), "![Figure](figures/plot.svg)").unwrap();
    fs::write(
        f.root.join("figures/plot.svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
    )
    .unwrap();
    let revision = research::import_paper(
        &f.store,
        research::ImportPaperRequest {
            workspace_id: f.ws.clone(),
            paper_id: None,
            title: "Tree".into(),
            role: "manuscript".into(),
            path: f.root.to_string_lossy().into_owned(),
            operation_id: id("import").unwrap(),
        },
    )
    .unwrap()
    .revision
    .unwrap();
    fs::write(f.root.join("figures/plot.svg"), "Changed").unwrap();
    let request = || FileReadRequest {
        workspace_id: f.ws.clone(),
        path: "figures/plot.svg".into(),
        checkpoint_id: None,
        revision_id: Some(revision.id.clone()),
    };
    let file = read_workspace_file(&f.store, request()).unwrap();
    assert_eq!(file.mime, "image/svg+xml");
    assert!(file.base64.is_some());
    let stored = f
        .store
        .root_path()
        .join("blobs")
        .join(format!("source-tree-{}", revision.content_hash))
        .join("figures/plot.svg");
    fs::write(stored, "Tampered").unwrap();
    assert!(read_workspace_file(&f.store, request()).is_err());
}

#[test]
fn conversation_file_links_stay_inside_the_session_runtime_root() {
    let f = fixture();
    let session = f
        .store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "Generated files".into(),
            operation_id: id("session").unwrap(),
        })
        .unwrap()
        .record;
    let root = f.store.runtime_root(&session.id).unwrap();
    fs::write(root.join("output.pdf"), b"%PDF fixture").unwrap();
    fs::write(f.root.join("outside.pdf"), b"%PDF outside").unwrap();

    let relative = read_conversation_file(&f.store, &session.id, "output.pdf").unwrap();
    assert_eq!(relative.path, "output.pdf");
    assert_eq!(relative.mime, "application/pdf");
    assert_eq!(relative.base64.as_deref(), Some("JVBERiBmaXh0dXJl"));
    assert_eq!(
        read_conversation_file(
            &f.store,
            &session.id,
            root.join("output.pdf").to_str().unwrap(),
        )
        .unwrap()
        .hash,
        relative.hash
    );
    let snapshot = snapshot_conversation_file(&f.store, &session.id, "output.pdf").unwrap();
    assert!(snapshot.starts_with(f.store.root_path().join("opened-conversation-files")));
    assert_eq!(fs::read(snapshot).unwrap(), b"%PDF fixture");
    assert!(read_conversation_file(&f.store, &session.id, "../outside.pdf").is_err());
    assert!(read_conversation_file(
        &f.store,
        &session.id,
        f.root.join("outside.pdf").to_str().unwrap(),
    )
    .is_err());

    std::os::unix::fs::symlink(f.root.join("outside.pdf"), root.join("linked.pdf")).unwrap();
    assert!(read_conversation_file(&f.store, &session.id, "linked.pdf").is_err());
}
