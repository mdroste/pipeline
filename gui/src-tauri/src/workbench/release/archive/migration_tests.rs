use super::*;

fn legacy_archive(root: &Path, declared_schema: u32, fail_migration: bool) -> PathBuf {
    let database = root.join("legacy.sqlite3");
    let c = Connection::open(&database).unwrap();
    for sql in [
        include_str!("../../migrations/001_initial.sql"),
        include_str!("../../migrations/002_independent_workspaces.sql"),
        include_str!("../../migrations/003_research_harness.sql"),
        include_str!("../../migrations/004_native_instruction_sources.sql"),
        include_str!("../../migrations/005_release_readiness.sql"),
        include_str!("../../migrations/006_project_surface.sql"),
        include_str!("../../migrations/007_research_studio.sql"),
        include_str!("../../migrations/008_theory_notes.sql"),
        include_str!("../../migrations/009_project_exchange.sql"),
        include_str!("../../migrations/010_preferences.sql"),
        include_str!("../../migrations/011_research_desk.sql"),
        include_str!("../../migrations/012_task_exchanges.sql"),
    ] {
        c.execute_batch(sql).unwrap();
    }
    c.pragma_update(None, "user_version", 12).unwrap();
    c.execute("INSERT INTO workspaces(id,name,created_at,updated_at) VALUES('legacy-workspace','Legacy research','t','t')", []).unwrap();
    c.execute("INSERT INTO sessions(id,workspace_id,title,created_at,updated_at) VALUES('legacy-session','legacy-workspace','Legacy conversation','t','t')", []).unwrap();
    let content = b"retained evidence";
    let reference = "/original/blobs/evidence.txt";
    let hash = hash_reader(&content[..]).unwrap();
    c.execute("INSERT INTO artifacts(id,workspace_id,content_hash,media_kind,size_bytes,origin,storage_reference,created_at) VALUES('legacy-artifact','legacy-workspace',?1,'text',?2,'import',?3,'t')", params![hash, content.len() as i64, reference]).unwrap();
    c.execute("INSERT INTO desk_records(id,workspace_id,kind,title,body_json,content_hash,created_at) VALUES('legacy-collection','legacy-workspace','collection','Reading','{}','hash','t')", []).unwrap();
    if fail_migration {
        c.execute("CREATE TABLE experiment_runs(id TEXT)", [])
            .unwrap();
    }
    c.close().unwrap();
    let manifest = ArchiveManifest {
        format: "pipeline-workspace-research".into(),
        format_version: 1,
        store_schema_version: declared_schema,
        created_at: "t".into(),
        original_blob_root: "/original/blobs".into(),
        workspace_roots: vec![],
        conversation_files: vec![],
        blobs: vec![BlobManifest {
            archive_path: "blobs/evidence.txt".into(),
            original_reference: reference.into(),
            size_bytes: content.len() as u64,
            sha256: hash,
        }],
        portability_note: "Schema-12 regression fixture".into(),
    };
    let path = root.join("legacy.pwrx");
    let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
    for (name, bytes) in [
            ("manifest.json", serde_json::to_vec(&manifest).unwrap()),
            ("database/research.sqlite3", fs::read(database).unwrap()),
            ("transcripts/legacy-session.md", b"# Legacy conversation\n".to_vec()),
            ("transcripts/legacy-session.json", serde_json::to_vec(&json!({"schemaVersion":1,"session":{"id":"legacy-session","title":"Legacy conversation"},"items":[]})).unwrap()),
            ("blobs/evidence.txt", content.to_vec()),
        ] {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(&bytes).unwrap();
        }
    zip.finish().unwrap();
    path
}

#[test]
fn prior_schema_archive_migrates_and_rebases_retained_evidence() {
    let source = tempfile::tempdir().unwrap();
    let path = legacy_archive(source.path(), 12, false);
    let archive_before = fs::read(&path).unwrap();
    assert_eq!(
        inspect_archive(ExportArchiveRequest {
            path: path.to_string_lossy().into()
        })
        .unwrap()
        .store_schema_version,
        12
    );
    let target = tempfile::tempdir().unwrap();
    let store = Store::open_at(target.path()).unwrap();
    import_archive(
        &store,
        ImportArchiveRequest {
            path: path.to_string_lossy().into(),
            root_mappings: BTreeMap::new(),
        },
    )
    .unwrap();
    assert_eq!(
        store.workspace("legacy-workspace").unwrap().name,
        "Legacy research"
    );
    assert_eq!(
        store
            .session_snapshot("legacy-session")
            .unwrap()
            .session
            .title,
        "Legacy conversation"
    );
    let c = store.connection().unwrap();
    let reference: String = c
        .query_row(
            "SELECT storage_reference FROM artifacts WHERE id='legacy-artifact'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(Path::new(&reference).starts_with(store.root_path().join("blobs")));
    assert_eq!(fs::read(reference).unwrap(), b"retained evidence");
    assert_eq!(
        c.query_row(
            "SELECT COUNT(*) FROM desk_records WHERE id='legacy-collection'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        c.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        CURRENT_SCHEMA_VERSION
    );
    assert_eq!(fs::read(path).unwrap(), archive_before);
}

#[test]
fn failed_archive_migration_leaves_the_empty_destination_untouched() {
    let source = tempfile::tempdir().unwrap();
    let path = legacy_archive(source.path(), 12, true);
    let target = tempfile::tempdir().unwrap();
    let store = Store::open_at(target.path()).unwrap();
    let error = import_archive(
        &store,
        ImportArchiveRequest {
            path: path.to_string_lossy().into(),
            root_mappings: BTreeMap::new(),
        },
    )
    .unwrap_err();
    assert!(
        error.message.contains("migrate research programs"),
        "{}",
        error.message
    );
    assert!(store.workspace("legacy-workspace").is_err());
    assert_eq!(
        fs::read_dir(store.root_path().join("blobs"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        store
            .connection()
            .unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        CURRENT_SCHEMA_VERSION
    );
}

#[test]
fn archive_rejects_newer_and_mismatched_schema_declarations() {
    for declared in [CURRENT_SCHEMA_VERSION + 1, 11] {
        let source = tempfile::tempdir().unwrap();
        let path = legacy_archive(source.path(), declared, false);
        if declared > CURRENT_SCHEMA_VERSION {
            assert!(inspect_archive(ExportArchiveRequest {
                path: path.to_string_lossy().into()
            })
            .is_err());
        }
        let target = tempfile::tempdir().unwrap();
        let store = Store::open_at(target.path()).unwrap();
        assert!(import_archive(
            &store,
            ImportArchiveRequest {
                path: path.to_string_lossy().into(),
                root_mappings: BTreeMap::new()
            }
        )
        .is_err());
        assert!(store.workspace("legacy-workspace").is_err());
    }
}
