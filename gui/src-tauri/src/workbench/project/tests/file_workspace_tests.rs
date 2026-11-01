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

#[test]
fn the_external_editor_refuses_a_program_and_opens_a_source_file() {
    use std::os::unix::fs::PermissionsExt as _;
    let f = fixture();
    for name in ["build.sh", "notes.tex", "marked.tex", "analysis.py"] {
        fs::write(f.root.join(name), "text\n").unwrap();
    }
    fs::set_permissions(f.root.join("marked.tex"), fs::Permissions::from_mode(0o755)).unwrap();
    let open = |path: &str| editor_external_path(&f.store, &f.ws, None, path);
    for program in ["build.sh", "marked.tex"] {
        let error = open(program).unwrap_err();
        assert!(error.message.contains("Reveal its folder"), "{program}");
    }
    assert_eq!(open("analysis.py").is_err(), cfg!(target_os = "macos"));
    assert!(open("notes.tex").unwrap().ends_with("notes.tex"));
}

#[test]
fn conversation_file_links_refuse_programs_and_copy_nothing() {
    use std::os::unix::fs::PermissionsExt as _;
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
    for name in ["run.bat", "setup.jar", "install.pkg", "share.url"] {
        fs::write(root.join(name), "fixture").unwrap();
    }
    fs::write(root.join("results"), "#!/bin/sh\n").unwrap();
    fs::set_permissions(root.join("results"), fs::Permissions::from_mode(0o755)).unwrap();
    let snapshots = f.store.root_path().join("opened-conversation-files");
    for name in [
        "run.bat",
        "setup.jar",
        "install.pkg",
        "share.url",
        "results",
    ] {
        let error = snapshot_conversation_file(&f.store, &session.id, name).unwrap_err();
        assert!(error.message.contains("Reveal its folder"), "{name}");
        // The preview in the conversation still works.
        read_conversation_file(&f.store, &session.id, name).unwrap();
    }
    assert!(!snapshots.exists(), "a refused file was copied");
    fs::write(root.join("table.csv"), "a,b\n").unwrap();
    snapshot_conversation_file(&f.store, &session.id, "table.csv").unwrap();
}

#[test]
fn opening_a_csv_of_any_size_reads_only_its_head() {
    let f = fixture();
    let path = f.root.join("panel.csv");
    fs::write(&path, "id,name\n1,a\n".repeat(1000)).unwrap();
    // Sparse, so the test is quick and a read of the whole file would not be.
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(1 << 42)
        .unwrap();
    let file = open_workspace_file(
        &f.store,
        FileReadRequest {
            workspace_id: f.ws.clone(),
            path: "panel.csv".into(),
            checkpoint_id: None,
            revision_id: None,
        },
    )
    .unwrap();
    assert_eq!(file.kind, "table");
    assert!(file.text.is_none());
    assert!(!file.editable);
    assert!(file.stream_token.is_none());
    assert_eq!(file.hash, file.stamp.unwrap());
}

#[test]
fn common_text_files_without_a_source_extension_can_be_edited() {
    let f = fixture();
    for path in [
        ".gitignore",
        "page.htm",
        "paper.ltx",
        "README",
        "Makefile",
        "sub/.gitignore",
    ] {
        assert!(file_workspace::editable_source(path), "{path}");
    }
    assert!(!file_workspace::editable_source("notes.unknown"));
    assert!(!file_workspace::editable_source("program"));
    for path in [".gitignore", "page.htm", "paper.ltx", "README"] {
        fs::write(f.root.join(path), "some text\n").unwrap();
        let file = read_workspace_file(
            &f.store,
            FileReadRequest {
                workspace_id: f.ws.clone(),
                path: path.into(),
                checkpoint_id: None,
                revision_id: None,
            },
        )
        .unwrap();
        assert!(file.editable, "{path}");
    }
}
