use super::format::{ArchiveManifest, BlobManifest};
use super::{
    export_archive, hash_reader, import_archive, inspect_archive, ExportArchiveRequest,
    ImportArchiveRequest,
};
use crate::workbench::store::{reference_schema, Store, CURRENT_SCHEMA_VERSION};
use rusqlite::{params, Connection};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;

fn legacy_archive(root: &Path, declared_schema: u32, conflicting_table: bool) -> PathBuf {
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
    if conflicting_table {
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
        omitted_references: vec![],
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
fn a_table_that_would_break_migration_is_refused_before_it_runs() {
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
        error
            .message
            .contains("doesn't match what Pipeline creates (table experiment_runs)"),
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
fn an_archive_from_every_prior_schema_restores() {
    for version in 1..=CURRENT_SCHEMA_VERSION {
        let source = tempfile::tempdir().unwrap();
        let database = source.path().join("research.sqlite3");
        reference_schema(version)
            .unwrap()
            .execute("VACUUM INTO ?1", [database.to_string_lossy()])
            .unwrap();
        let c = Connection::open(&database).unwrap();
        c.pragma_update(None, "user_version", version).unwrap();
        c.close().unwrap();
        let manifest = ArchiveManifest {
            format: "pipeline-workspace-research".into(),
            format_version: 1,
            store_schema_version: version,
            created_at: "t".into(),
            original_blob_root: "workspace://blobs".into(),
            workspace_roots: vec![],
            conversation_files: vec![],
            omitted_references: vec![],
            blobs: vec![],
            portability_note: "Prior schema fixture".into(),
        };
        let path = source.path().join("prior.pwrx");
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, bytes) in [
            ("manifest.json", serde_json::to_vec(&manifest).unwrap()),
            ("database/research.sqlite3", fs::read(&database).unwrap()),
        ] {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(&bytes).unwrap();
        }
        zip.finish().unwrap();

        let target = tempfile::tempdir().unwrap();
        let store = Store::open_at(target.path()).unwrap();
        import_archive(
            &store,
            ImportArchiveRequest {
                path: path.to_string_lossy().into(),
                root_mappings: BTreeMap::new(),
            },
        )
        .unwrap_or_else(|error| panic!("schema {version}: {}", error.message));
        assert_eq!(
            store
                .connection()
                .unwrap()
                .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            CURRENT_SCHEMA_VERSION
        );
    }
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

/// A store from before the journal was cleared, as `apply` left it: the journal
/// repeats the receipt on every run, including ones that still need it.
fn jobs_with_journals(c: &Connection) {
    c.execute("INSERT INTO workspaces(id,name,revision,created_at,updated_at) VALUES('w','Project',1,'now','now')", []).unwrap();
    for (id, outcome) in [
        ("finished", "completed"),
        ("failed", "failed"),
        ("stopped", "interrupted"),
        ("late", "timed_out"),
        ("unknown", "outcome_unknown"),
        ("crashed", "running"),
    ] {
        c.execute("INSERT INTO research_executions(id,workspace_id,adapter,command_json,cwd,input_manifest_json,dependency_hash,outcome,output_manifest_json,validation_json,snapshot_consistency,created_at) VALUES(?1,'w','command','[]','/tmp','{}','hash',?2,'{}','{}','uncertain','now')",[id,outcome]).unwrap();
        c.execute("INSERT INTO execution_jobs(execution_id,operation_id,request_hash,owner,process_instance,created_at,finalization_json) VALUES(?1,?1,'hash','detached','earlier','now','{\"stdout\":\"output\"}')",[id]).unwrap();
    }
}
fn journals(c: &Connection) -> Vec<String> {
    let mut q = c
        .prepare("SELECT execution_id FROM execution_jobs WHERE finalization_json IS NOT NULL ORDER BY execution_id")
        .unwrap();
    let ids = q
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    ids
}

#[test]
fn opening_an_older_store_keeps_journals_only_for_runs_without_a_final_outcome() {
    let root = tempfile::tempdir().unwrap();
    let older = root.path().join("older.sqlite3");
    reference_schema(19)
        .unwrap()
        .execute("VACUUM INTO ?1", [older.to_string_lossy()])
        .unwrap();
    let mut c = Connection::open(&older).unwrap();
    jobs_with_journals(&c);
    crate::workbench::store::migrate(&mut c, 19).unwrap();
    assert_eq!(journals(&c), ["crashed", "unknown"]);
    let version: u32 = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, CURRENT_SCHEMA_VERSION);
}

#[test]
fn a_backup_from_before_the_journal_was_cleared_restores_and_stays_reconcilable() {
    let source = tempfile::tempdir().unwrap();
    let database = source.path().join("research.sqlite3");
    reference_schema(19)
        .unwrap()
        .execute("VACUUM INTO ?1", [database.to_string_lossy()])
        .unwrap();
    let c = Connection::open(&database).unwrap();
    c.pragma_update(None, "user_version", 19u32).unwrap();
    jobs_with_journals(&c);
    c.close().unwrap();
    let manifest = ArchiveManifest {
        format: "pipeline-workspace-research".into(),
        format_version: 1,
        store_schema_version: 19,
        created_at: "t".into(),
        original_blob_root: "workspace://blobs".into(),
        workspace_roots: vec![],
        conversation_files: vec![],
        omitted_references: vec![],
        blobs: vec![],
        portability_note: "Journal fixture".into(),
    };
    let path = source.path().join("older.pwrx");
    let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
    for (name, bytes) in [
        ("manifest.json", serde_json::to_vec(&manifest).unwrap()),
        ("database/research.sqlite3", fs::read(&database).unwrap()),
    ] {
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();

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
        journals(&store.connection().unwrap()),
        ["crashed", "unknown"]
    );
    let jobs = crate::workbench::research::jobs::list_jobs(&store, "w").unwrap();
    let reconcilable = |id: &str| {
        jobs.iter()
            .find(|j| j.execution.id == id)
            .unwrap()
            .can_reconcile
    };
    assert!(reconcilable("unknown") && reconcilable("crashed"));
    assert!(!reconcilable("finished"));
}

#[test]
fn a_backup_leaves_out_finished_job_journals_and_the_space_they_took() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open_at(root.path()).unwrap();
    let c = store.connection().unwrap();
    jobs_with_journals(&c);
    c.execute(
        "UPDATE execution_jobs SET finalization_json=hex(zeroblob(4000000)) WHERE execution_id='finished'",
        [],
    )
    .unwrap();
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("backup.pwrx");
    export_archive(
        &store,
        ExportArchiveRequest {
            path: path.to_string_lossy().into_owned(),
        },
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();
    let extracted = output.path().join("extracted.sqlite3");
    std::io::copy(
        &mut archive.by_name("database/research.sqlite3").unwrap(),
        &mut File::create(&extracted).unwrap(),
    )
    .unwrap();
    assert_eq!(
        journals(&Connection::open(&extracted).unwrap()),
        ["crashed", "unknown"]
    );
    assert!(fs::metadata(&extracted).unwrap().len() < 2_000_000);
}
