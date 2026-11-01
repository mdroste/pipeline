use super::{
    export_archive, export_archive_without, import_archive, inspect_archive, missing_backup_files,
    ArchiveReport, ExportArchiveRequest, ImportArchiveRequest,
};
use crate::workbench::store::CreateSessionRequest;
use crate::workbench::store::{Store, WorkbenchResult};
use rusqlite::{params, Connection};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
fn fixture() -> (tempfile::TempDir, Store, String) {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open_at(root.path()).unwrap();
    let session = store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "Research".into(),
            operation_id: "create".into(),
        })
        .unwrap()
        .record
        .id;
    (root, store, session)
}
fn export(store: &Store, root: &Path) -> PathBuf {
    let path = root.join("backup.pwrx");
    export_archive(
        store,
        ExportArchiveRequest {
            path: path.to_string_lossy().into(),
        },
    )
    .unwrap();
    path
}
fn import(store: &Store, path: &Path) -> WorkbenchResult<ArchiveReport> {
    import_archive(
        store,
        ImportArchiveRequest {
            path: path.to_string_lossy().into(),
            root_mappings: BTreeMap::new(),
        },
    )
}
fn rewrite_entries(path: &Path, mut rewrite: impl FnMut(&str, Vec<u8>) -> Vec<u8>) {
    let mut archive = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
    let mut entries = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        entries.push((entry.name().unwrap().into_owned(), bytes));
    }
    drop(archive);
    let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
    for (name, bytes) in entries {
        zip.start_file(&name, SimpleFileOptions::default()).unwrap();
        zip.write_all(&rewrite(&name, bytes)).unwrap();
    }
    zip.finish().unwrap();
}
fn replace_entry(path: &Path, target: &str, replacement: &[u8]) {
    rewrite_entries(path, |name, bytes| {
        if name == target {
            replacement.to_vec()
        } else {
            bytes
        }
    });
}
/// Patch the uncompressed size both headers declare for `name`, leaving the
/// deflate stream (and its CRC) intact: the shape of a decompression bomb.
fn declare_uncompressed_size(path: &Path, name: &str, size: u32) {
    let mut bytes = fs::read(path).unwrap();
    let mut patched = 0;
    for (signature, size_offset, name_offset) in [
        (0x0403_4b50u32, 22usize, 30usize),
        (0x0201_4b50u32, 24usize, 46usize),
    ] {
        let mut at = 0;
        while at + name_offset + name.len() <= bytes.len() {
            if u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) == signature
                && &bytes[at + name_offset..at + name_offset + name.len()] == name.as_bytes()
            {
                bytes[at + size_offset..at + size_offset + 4].copy_from_slice(&size.to_le_bytes());
                patched += 1;
            }
            at += 1;
        }
    }
    assert_eq!(patched, 2, "both zip headers for {name} must be patched");
    fs::write(path, bytes).unwrap();
}
#[test]
fn lying_entry_sizes_are_rejected_before_the_stream_is_expanded() {
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("bomb.pwrx");
    let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("manifest.json", options).unwrap();
    let mut body = br#"{"format":""#.to_vec();
    body.resize(16 * 1024 * 1024, b'a');
    body.extend_from_slice(br#""}"#);
    zip.write_all(&body).unwrap();
    zip.start_file("database/research.sqlite3", options)
        .unwrap();
    zip.write_all(b"not a database").unwrap();
    zip.finish().unwrap();
    declare_uncompressed_size(&path, "manifest.json", 1024);

    let request = ExportArchiveRequest {
        path: path.to_string_lossy().into(),
    };
    let error = inspect_archive(request).unwrap_err();
    assert!(
        error
            .message
            .contains("larger than its declared uncompressed size"),
        "{}",
        error.message
    );
    let empty = tempfile::tempdir().unwrap();
    let store = Store::open_at(empty.path()).unwrap();
    let error = import(&store, &path).unwrap_err();
    assert!(
        error
            .message
            .contains("larger than its declared uncompressed size"),
        "{}",
        error.message
    );
}
#[test]
fn restore_rejects_blob_references_that_traverse_out_of_the_store() {
    let (_root, source, _session) = fixture();
    let research = tempfile::tempdir().unwrap();
    let research_root = research.path().join("research-root");
    fs::create_dir(&research_root).unwrap();
    let workspace = source
        .create_workspace(crate::workbench::store::CreateWorkspaceRequest {
            name: "Research".into(),
            root: Some(research_root.to_string_lossy().into_owned()),
            operation_id: "create-workspace".into(),
        })
        .unwrap()
        .record;
    let paper_path = research_root.join("paper.tex");
    fs::write(&paper_path, "Output rises with productivity.\n").unwrap();
    let paper = crate::workbench::research::import_paper(
        &source,
        crate::workbench::research::ImportPaperRequest {
            workspace_id: workspace.id.clone(),
            paper_id: None,
            title: "Paper".into(),
            role: "manuscript".into(),
            path: paper_path.to_string_lossy().into_owned(),
            operation_id: "paper-one".into(),
        },
    )
    .unwrap();
    let revision = paper.revision.unwrap();
    let text_name = Path::new(revision.text_reference.as_deref().unwrap())
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let output = tempfile::tempdir().unwrap();
    let path = export(&source, output.path());

    let archived_root = {
        let mut archive = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();
        let manifest: Value =
            serde_json::from_reader(archive.by_name("manifest.json").unwrap()).unwrap();
        manifest["workspaceRoots"][0].as_str().unwrap().to_string()
    };
    let import_into = |store: &Store| {
        import_archive(
            store,
            ImportArchiveRequest {
                path: path.to_string_lossy().into(),
                root_mappings: BTreeMap::from([(archived_root.clone(), None)]),
            },
        )
    };
    let read = |store: &Store| {
        crate::workbench::research::paper_read(
            store,
            crate::workbench::research::PaperReadRequest {
                workspace_id: workspace.id.clone(),
                revision_id: revision.id.clone(),
                start: None,
                length: None,
            },
        )
    };

    // Readers must not trust a stored reference that escapes blobs/.
    let clean = tempfile::tempdir().unwrap();
    fs::write(clean.path().join("outside.txt"), "private key material").unwrap();
    let clean_store = Store::open_at(clean.path()).unwrap();
    import_into(&clean_store).unwrap();
    assert!(read(&clean_store).unwrap().text.contains("productivity"));
    let clean_evil = clean
        .path()
        .join("blobs")
        .join("..")
        .join("outside.txt")
        .to_string_lossy()
        .into_owned();
    clean_store
        .connection()
        .unwrap()
        .execute(
            "UPDATE paper_revisions SET text_reference=?1",
            [clean_evil.as_str()],
        )
        .unwrap();
    let error = read(&clean_store).unwrap_err();
    assert!(error.message.contains("escaped"), "{}", error.message);

    // Restore must refuse the reference instead of keeping it.
    let target = tempfile::tempdir().unwrap();
    fs::write(target.path().join("outside.txt"), "private key material").unwrap();
    let evil = target
        .path()
        .join("blobs")
        .join("..")
        .join("outside.txt")
        .to_string_lossy()
        .into_owned();
    rewrite_entries(&path, |name, bytes| match name {
        "manifest.json" => {
            let mut manifest: Value = serde_json::from_slice(&bytes).unwrap();
            for blob in manifest["blobs"].as_array_mut().unwrap() {
                // Exports store portable `workspace://blobs/<name>` references.
                if blob["originalReference"]
                    .as_str()
                    .is_some_and(|reference| reference.ends_with(&text_name))
                {
                    // A manifest reference below the evil path satisfies the
                    // prefix check without rebasing the exact DB reference.
                    blob["originalReference"] = Value::String(format!("{evil}/x"));
                }
            }
            serde_json::to_vec(&manifest).unwrap()
        }
        "database/research.sqlite3" => {
            let scratch = output.path().join("scratch.sqlite3");
            fs::write(&scratch, bytes).unwrap();
            let connection = Connection::open(&scratch).unwrap();
            connection
                .execute(
                    "UPDATE paper_revisions SET text_reference=?1",
                    [evil.as_str()],
                )
                .unwrap();
            drop(connection);
            fs::read(&scratch).unwrap()
        }
        _ => bytes,
    });
    let store = Store::open_at(target.path()).unwrap();
    let error = import_into(&store).unwrap_err();
    assert!(error.message.contains("traversal"), "{}", error.message);
}
#[test]
fn restore_reconciles_unfinished_turns_and_discards_foreign_trash_authority() {
    let (_root, source, session) = fixture();
    let external = tempfile::tempdir().unwrap();
    let original = external.path().join("sentinel");
    fs::write(&original, "keep").unwrap();
    let binding = source
        .bind_session(&session, "runtime", "old-thread")
        .unwrap();
    source.connection().unwrap().execute("INSERT INTO turns(id,binding_id,client_submission_id,provider_turn_id,state,created_at,updated_at) VALUES('turn',?1,'client','native-turn','inProgress','t','t')", [&binding.id]).unwrap();
    let output = tempfile::tempdir().unwrap();
    let path = export(&source, output.path());
    let mut zip = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();
    let mut bytes = Vec::new();
    zip.by_name("database/research.sqlite3")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    drop(zip);
    let db_path = output.path().join("foreign.sqlite3");
    fs::write(&db_path, bytes).unwrap();
    let db = Connection::open(&db_path).unwrap();
    db.execute("INSERT INTO storage_trash(id,category,original_path,trash_path,size_bytes,reason,moved_at,state) VALUES('legacy','context_unreferenced',?1,?2,4,'test','t','trashed')", params![original.to_string_lossy(), external.path().join("trash/sentinel").to_string_lossy()]).unwrap();
    db.close().unwrap();
    replace_entry(
        &path,
        "database/research.sqlite3",
        &fs::read(db_path).unwrap(),
    );
    let target = tempfile::tempdir().unwrap();
    let target = Store::open_at(target.path()).unwrap();
    import(&target, &path).unwrap();
    let db = target.connection().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM storage_trash", [], |r| r
            .get::<_, u32>(0))
            .unwrap(),
        0
    );
    let turn: (String, Option<String>) = db
        .query_row("SELECT state,terminal_at FROM turns", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(turn.0, "interrupted");
    assert!(turn.1.is_some());
    assert_eq!(fs::read_to_string(original).unwrap(), "keep");
}
/// Applies `sql` to the database inside a copy of `archive`, as a crafted file would.
fn tampered(archive: &Path, output: &Path, name: &str, sql: &str) -> PathBuf {
    let copy = output.join(format!("{name}.pwrx"));
    fs::copy(archive, &copy).unwrap();
    rewrite_entries(&copy, |entry, bytes| {
        if entry != "database/research.sqlite3" {
            return bytes;
        }
        let scratch = output.join(format!("{name}.sqlite3"));
        fs::write(&scratch, bytes).unwrap();
        let connection = Connection::open(&scratch).unwrap();
        connection.execute_batch(sql).unwrap();
        connection.close().unwrap();
        fs::read(&scratch).unwrap()
    });
    copy
}
fn assert_destination_untouched(store: &Store, session: &str) {
    assert!(store.session_snapshot(session).is_err());
    let connection = store.connection().unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT (SELECT COUNT(*) FROM workspaces)+(SELECT COUNT(*) FROM sessions)+(SELECT COUNT(*) FROM desk_records)+(SELECT COUNT(*) FROM execution_plan_state)+(SELECT COUNT(*) FROM execution_authorizations)",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name IN ('keep_plan','keep_authorization','extra')",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    assert_eq!(
        fs::read_dir(store.root_path().join("blobs"))
            .unwrap()
            .count(),
        0
    );
    assert!(!store.root_path().join("jobs/conversations").exists());
}
const AUTHORITY: &str = "INSERT INTO workspaces(id,name,created_at,updated_at) VALUES('w','Crafted','t','t');
    INSERT INTO desk_records(id,workspace_id,kind,title,body_json,content_hash,created_at) VALUES('plan','w','execution_plan','Plan','{}','hash','t');
    INSERT INTO execution_plan_state(plan_id,fingerprint,authorized,test_status) VALUES('plan','hash',1,'passed');";
#[test]
fn a_trigger_that_keeps_execution_authority_is_refused_before_any_retirement() {
    let (_root, source, session) = fixture();
    let output = tempfile::tempdir().unwrap();
    let archive = export(&source, output.path());
    let sql = format!(
        "{AUTHORITY}
        CREATE TRIGGER keep_plan BEFORE DELETE ON execution_plan_state BEGIN SELECT RAISE(IGNORE); END;"
    );
    let crafted = tampered(&archive, output.path(), "trigger", &sql);
    let target = tempfile::tempdir().unwrap();
    let store = Store::open_at(target.path()).unwrap();
    let error = import(&store, &crafted).unwrap_err();
    assert!(
        error.message.contains("trigger keep_plan"),
        "{}",
        error.message
    );
    assert_destination_untouched(&store, &session);
}
#[test]
fn a_store_with_no_extra_objects_loses_authority_on_restore() {
    let (_root, source, _session) = fixture();
    let output = tempfile::tempdir().unwrap();
    let archive = export(&source, output.path());
    let crafted = tampered(&archive, output.path(), "plain", AUTHORITY);
    let target = tempfile::tempdir().unwrap();
    let store = Store::open_at(target.path()).unwrap();
    import(&store, &crafted).unwrap();
    assert_eq!(
        store
            .connection()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM execution_plan_state", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    // The archived record of a folder on another computer is not kept either.
    assert_eq!(
        store
            .connection()
            .unwrap()
            .query_row("SELECT root FROM store_location", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        target.path().to_string_lossy()
    );
}
#[test]
fn any_schema_object_pipeline_did_not_create_is_refused() {
    let (_root, source, session) = fixture();
    let output = tempfile::tempdir().unwrap();
    let archive = export(&source, output.path());
    for (name, sql, expected) in [
        ("view", "CREATE VIEW extra AS SELECT 1;", "view extra"),
        ("table", "CREATE TABLE extra(value TEXT);", "table extra"),
        (
            "index",
            "CREATE INDEX extra ON workspaces(name);",
            "index extra",
        ),
        (
            "definition",
            "DROP INDEX transcript_items_page;
             CREATE INDEX transcript_items_page ON transcript_items(binding_id);",
            "index transcript_items_page",
        ),
        (
            "column",
            "ALTER TABLE execution_plan_state ADD COLUMN extra TEXT;",
            "table execution_plan_state",
        ),
        (
            "removed",
            "DROP TABLE store_location;",
            "table store_location",
        ),
        (
            "authorization trigger",
            "CREATE TRIGGER keep_authorization BEFORE DELETE ON execution_authorizations BEGIN SELECT RAISE(IGNORE); END;",
            "trigger keep_authorization",
        ),
        (
            "replaced trigger",
            "DROP TRIGGER desk_note_delete;
             CREATE TRIGGER desk_note_delete AFTER DELETE ON research_notes BEGIN SELECT 1; END;",
            "trigger desk_note_delete",
        ),
    ] {
        let crafted = tampered(&archive, output.path(), name, sql);
        let target = tempfile::tempdir().unwrap();
        let store = Store::open_at(target.path()).unwrap();
        let error = import(&store, &crafted).unwrap_err();
        assert!(error.message.contains(expected), "{name}: {}", error.message);
        assert_destination_untouched(&store, &session);
    }
}
#[test]
fn an_older_archive_is_compared_at_its_own_schema_version() {
    let (_root, source, session) = fixture();
    let output = tempfile::tempdir().unwrap();
    let archive = export(&source, output.path());
    // A plausible rollback to the previous schema that keeps one object from
    // this version, so it matches neither version's reference.
    let crafted = tampered(
        &archive,
        output.path(),
        "older",
        &format!(
            "PRAGMA user_version={};",
            crate::workbench::store::CURRENT_SCHEMA_VERSION - 1
        ),
    );
    rewrite_entries(&crafted, |entry, bytes| {
        if entry != "manifest.json" {
            return bytes;
        }
        let mut manifest: Value = serde_json::from_slice(&bytes).unwrap();
        manifest["storeSchemaVersion"] =
            Value::from(crate::workbench::store::CURRENT_SCHEMA_VERSION - 1);
        serde_json::to_vec(&manifest).unwrap()
    });
    let target = tempfile::tempdir().unwrap();
    let store = Store::open_at(target.path()).unwrap();
    let error = import(&store, &crafted).unwrap_err();
    assert!(
        error.message.contains("store_location"),
        "{}",
        error.message
    );
    assert_destination_untouched(&store, &session);
}
/// A real backup made without a paper's lost text, with its project folder and
/// the reference it left out.
fn backup_with_a_left_out_file(output: &Path) -> (PathBuf, String, String) {
    let (_root, source, _session) = fixture();
    let project = output.join("project");
    fs::create_dir(&project).unwrap();
    fs::write(project.join("paper.tex"), "Output rises.\n").unwrap();
    let workspace = source
        .create_workspace(crate::workbench::store::CreateWorkspaceRequest {
            name: "Research".into(),
            root: Some(project.to_string_lossy().into_owned()),
            operation_id: "workspace".into(),
        })
        .unwrap()
        .record;
    let revision = crate::workbench::research::import_paper(
        &source,
        crate::workbench::research::ImportPaperRequest {
            workspace_id: workspace.id,
            paper_id: None,
            title: "Paper".into(),
            role: "manuscript".into(),
            path: project.join("paper.tex").to_string_lossy().into_owned(),
            operation_id: "paper".into(),
        },
    )
    .unwrap()
    .revision
    .unwrap();
    fs::remove_file(revision.text_reference.unwrap()).unwrap();
    let approved = missing_backup_files(&source)
        .unwrap()
        .into_iter()
        .map(|file| file.reference)
        .collect::<Vec<_>>();
    let path = output.join("left-out.pwrx");
    export_archive_without(
        &source,
        ExportArchiveRequest {
            path: path.to_string_lossy().into(),
        },
        &approved,
    )
    .unwrap();
    (path, workspace.root.unwrap(), approved[0].clone())
}
fn restore_detached(store: &Store, path: &Path, root: &str) -> WorkbenchResult<ArchiveReport> {
    import_archive(
        store,
        ImportArchiveRequest {
            path: path.to_string_lossy().into(),
            root_mappings: BTreeMap::from([(root.to_string(), None)]),
        },
    )
}
fn with_manifest(path: &Path, output: &Path, name: &str, edit: impl Fn(&mut Value)) -> PathBuf {
    let copy = output.join(format!("{name}.pwrx"));
    fs::copy(path, &copy).unwrap();
    rewrite_entries(&copy, |entry, bytes| {
        if entry != "manifest.json" {
            return bytes;
        }
        let mut manifest: Value = serde_json::from_slice(&bytes).unwrap();
        edit(&mut manifest);
        serde_json::to_vec(&manifest).unwrap()
    });
    copy
}
#[test]
fn a_crafted_omission_list_is_refused_and_leaves_the_destination_untouched() {
    let output = tempfile::tempdir().unwrap();
    let (path, root, lost) = backup_with_a_left_out_file(output.path());
    let archived = {
        let mut zip = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();
        let manifest: Value =
            serde_json::from_reader(zip.by_name("manifest.json").unwrap()).unwrap();
        manifest["blobs"][0]["originalReference"]
            .as_str()
            .unwrap()
            .to_string()
    };
    for (name, list) in [
        (
            "traversal",
            vec!["workspace://blobs/../../outside.txt".to_string()],
        ),
        (
            "nested traversal",
            vec!["workspace://blobs/a/../../b".to_string()],
        ),
        ("absolute", vec!["/etc/passwd".to_string()]),
        (
            "other scheme",
            vec!["unavailable://blobs/a.txt".to_string()],
        ),
        ("no name", vec!["workspace://blobs/".to_string()]),
        ("backslash", vec![r"workspace://blobs/a\b".to_string()]),
        (
            "long",
            vec![format!("workspace://blobs/{}", "a".repeat(2000))],
        ),
        ("duplicate", vec![lost.clone(), lost.clone()]),
        ("in the archive", vec![archived.clone()]),
        ("inside an archived file", vec![format!("{archived}/x")]),
        (
            "unused",
            vec!["workspace://blobs/nothing-uses-this.txt".to_string()],
        ),
        (
            "partly unused",
            vec![lost.clone(), "workspace://blobs/nope.txt".to_string()],
        ),
    ] {
        let crafted = with_manifest(&path, output.path(), name, |manifest| {
            manifest["omittedReferences"] = serde_json::json!(list);
        });
        let target = tempfile::tempdir().unwrap();
        fs::write(target.path().join("outside.txt"), "keep").unwrap();
        let store = Store::open_at(target.path()).unwrap();
        let error = restore_detached(&store, &crafted, &root).unwrap_err();
        assert!(
            error.message.contains("left-out files"),
            "{name}: {}",
            error.message
        );
        assert_destination_untouched(&store, "no-session");
        assert_eq!(
            fs::read_to_string(target.path().join("outside.txt")).unwrap(),
            "keep"
        );
    }
    // A list only ever speaks for portable archives.
    let legacy = with_manifest(&path, output.path(), "legacy", |manifest| {
        manifest["originalBlobRoot"] = Value::from("/original/blobs");
    });
    let target = tempfile::tempdir().unwrap();
    let store = Store::open_at(target.path()).unwrap();
    assert!(restore_detached(&store, &legacy, &root)
        .unwrap_err()
        .message
        .contains("left-out files"));
    assert!(inspect_archive(ExportArchiveRequest {
        path: legacy.to_string_lossy().into()
    })
    .is_err());
}
#[test]
fn a_backup_with_omissions_is_held_to_the_schema_and_retirement_checks() {
    let output = tempfile::tempdir().unwrap();
    let (path, root, _lost) = backup_with_a_left_out_file(output.path());
    let trigger = format!(
        "{AUTHORITY}
        CREATE TRIGGER keep_plan BEFORE DELETE ON execution_plan_state BEGIN SELECT RAISE(IGNORE); END;"
    );
    let crafted = tampered(&path, output.path(), "omission-trigger", &trigger);
    let target = tempfile::tempdir().unwrap();
    let store = Store::open_at(target.path()).unwrap();
    let error = restore_detached(&store, &crafted, &root).unwrap_err();
    assert!(
        error.message.contains("trigger keep_plan"),
        "{}",
        error.message
    );
    assert_destination_untouched(&store, "no-session");

    let plain = tampered(&path, output.path(), "omission-plain", AUTHORITY);
    let target = tempfile::tempdir().unwrap();
    let store = Store::open_at(target.path()).unwrap();
    let report = restore_detached(&store, &plain, &root).unwrap();
    assert_eq!(report.omitted_files.len(), 1);
    assert_eq!(
        store
            .connection()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM execution_plan_state", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn conversation_files_round_trip_and_corruption_fails_before_restore() {
    let (root, source, session) = fixture();
    let file = root
        .path()
        .join("jobs/conversations")
        .join(&session)
        .join("figures/result.csv");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "x,y\n1,2\n").unwrap();
    let output = tempfile::tempdir().unwrap();
    let path = export(&source, output.path());
    let target = tempfile::tempdir().unwrap();
    let store = Store::open_at(target.path()).unwrap();
    import(&store, &path).unwrap();
    assert_eq!(
        fs::read(
            target
                .path()
                .join("jobs/conversations")
                .join(&session)
                .join("figures/result.csv")
        )
        .unwrap(),
        fs::read(&file).unwrap()
    );
    replace_entry(
        &path,
        &format!("conversation-files/{session}/figures/result.csv"),
        b"changed",
    );
    let empty = tempfile::tempdir().unwrap();
    let store = Store::open_at(empty.path()).unwrap();
    assert!(import(&store, &path).is_err());
    assert!(store.session_snapshot(&session).is_err());
    assert!(!empty.path().join("jobs/conversations").exists());
}
#[test]
fn export_strips_local_trash_and_restore_never_overwrites_conversation_files() {
    let (root, source, session) = fixture();
    let file = root
        .path()
        .join("jobs/conversations")
        .join(&session)
        .join("notes.txt");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, "archived").unwrap();
    source.connection().unwrap().execute("INSERT INTO storage_trash(id,category,original_path,trash_path,size_bytes,reason,moved_at,state) VALUES('local','context_unreferenced','/old/context/a','/old/trash/local/context/a',4,'test','t','trashed')", []).unwrap();
    let output = tempfile::tempdir().unwrap();
    let path = export(&source, output.path());
    let mut archive = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();
    let mut bytes = Vec::new();
    archive
        .by_name("database/research.sqlite3")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    let db = output.path().join("snapshot.sqlite3");
    fs::write(&db, bytes).unwrap();
    let db = Connection::open(db).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM storage_trash", [], |r| r
            .get::<_, u32>(0))
            .unwrap(),
        0
    );
    let target = tempfile::tempdir().unwrap();
    let store = Store::open_at(target.path()).unwrap();
    let sentinel = target.path().join("jobs/conversations/orphan.txt");
    fs::create_dir_all(sentinel.parent().unwrap()).unwrap();
    fs::write(&sentinel, "keep").unwrap();
    assert!(import(&store, &path).is_err());
    assert_eq!(fs::read_to_string(sentinel).unwrap(), "keep");
    assert!(store.session_snapshot(&session).is_err());
}
#[cfg(unix)]
#[test]
fn links_inside_a_conversation_folder_are_left_out_without_failing_the_backup() {
    let (root, source, session) = fixture();
    let external = tempfile::tempdir().unwrap();
    fs::write(external.path().join("private.txt"), "keep").unwrap();
    let folder = root.path().join("jobs/conversations").join(&session);
    fs::create_dir_all(folder.join(".venv/bin")).unwrap();
    fs::write(folder.join("script.py"), "print(1)").unwrap();
    std::os::unix::fs::symlink("/usr/bin/python3", folder.join(".venv/bin/python")).unwrap();
    std::os::unix::fs::symlink(external.path(), folder.join("outside")).unwrap();
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("backup.pwrx");
    let report = export_archive(
        &source,
        ExportArchiveRequest {
            path: path.to_string_lossy().into(),
        },
    )
    .unwrap();
    assert!(report.portability_note.contains("left out"));
    let target = tempfile::tempdir().unwrap();
    let store = Store::open_at(target.path()).unwrap();
    import(&store, &path).unwrap();
    let restored = target.path().join("jobs/conversations").join(&session);
    assert_eq!(
        fs::read_to_string(restored.join("script.py")).unwrap(),
        "print(1)"
    );
    assert!(!restored.join("outside").exists());
    assert!(!restored.join(".venv/bin/python").exists());
}
#[cfg(unix)]
#[test]
fn conversation_symlinks_cannot_import_external_files_into_backups() {
    let (root, source, session) = fixture();
    let external = tempfile::tempdir().unwrap();
    fs::write(external.path().join("private.txt"), "keep").unwrap();
    let directory = root.path().join("jobs/conversations");
    fs::create_dir_all(&directory).unwrap();
    std::os::unix::fs::symlink(external.path(), directory.join(session)).unwrap();
    let output = tempfile::tempdir().unwrap();
    assert!(export_archive(
        &source,
        ExportArchiveRequest {
            path: output.path().join("backup.pwrx").to_string_lossy().into()
        }
    )
    .is_err());
}

#[cfg(unix)]
#[test]
fn an_unreadable_conversation_folder_is_left_out_without_failing_the_backup() {
    use std::os::unix::fs::PermissionsExt;
    let (root, source, session) = fixture();
    let folder = root.path().join("jobs/conversations").join(&session);
    fs::create_dir_all(folder.join("locked")).unwrap();
    fs::write(folder.join("locked/secret.txt"), "hidden").unwrap();
    fs::write(folder.join("kept.txt"), "kept").unwrap();
    let locked = folder.join("locked");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("backup.pwrx");
    let report = export_archive(
        &source,
        ExportArchiveRequest {
            path: path.to_string_lossy().into(),
        },
    );
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(report.unwrap().portability_note.contains("left out"));
    let target = tempfile::tempdir().unwrap();
    let store = Store::open_at(target.path()).unwrap();
    import(&store, &path).unwrap();
    let restored = target.path().join("jobs/conversations").join(&session);
    assert_eq!(
        fs::read_to_string(restored.join("kept.txt")).unwrap(),
        "kept"
    );
    assert!(!restored.join("locked").exists());
}

#[test]
fn conversation_files_stop_at_every_budget_instead_of_failing_the_backup() {
    use super::conversation_files::{stage, Budget};
    use super::format::{MAX_ARCHIVE_BYTES, MAX_MANIFEST_BYTES};
    let (root, source, session) = fixture();
    let folder = root.path().join("jobs/conversations").join(&session);
    fs::create_dir_all(&folder).unwrap();
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(folder.join(name), "12345").unwrap();
    }
    let staging = tempfile::tempdir().unwrap();
    let connection = source.connection().unwrap();
    let wide = || Budget {
        entries: 10,
        bytes: 100,
        manifest_bytes: 100_000,
    };
    let staged = stage(&source, &connection, staging.path(), wide()).unwrap();
    assert_eq!((staged.files.len(), staged.omitted), (3, 0));

    let staging = tempfile::tempdir().unwrap();
    let bytes = Budget {
        bytes: 11,
        ..wide()
    };
    let staged = stage(&source, &connection, staging.path(), bytes).unwrap();
    assert_eq!((staged.files.len(), staged.omitted), (2, 1));

    // Thousands of small files must not outgrow the manifest restore accepts.
    let staging = tempfile::tempdir().unwrap();
    let manifest = Budget {
        manifest_bytes: 600,
        ..wide()
    };
    let staged = stage(&source, &connection, staging.path(), manifest).unwrap();
    assert!(!staged.files.is_empty() && staged.omitted > 0);
    let listed: usize = staged
        .files
        .iter()
        .map(|file| serde_json::to_string_pretty(file).unwrap().len())
        .sum();
    assert!((listed as u64) < 600);
    assert!(!staging
        .path()
        .join(format!("conversation-files/{session}/c.txt"))
        .exists());

    // The space offered leaves room for everything measured before the files.
    let database = 300 * 1024 * 1024;
    let budget = super::export::conversation_byte_budget(400, database, 1_000);
    assert!(budget + 400 + database + 1_000 + MAX_MANIFEST_BYTES < MAX_ARCHIVE_BYTES);
    assert_eq!(
        super::export::conversation_byte_budget(MAX_ARCHIVE_BYTES, database, 0),
        0
    );
}

#[test]
fn a_backup_keeps_page_locators_instead_of_image_copies_and_still_restores() {
    let (_root, source, session) = fixture();
    let workspace = source
        .create_workspace(crate::workbench::store::CreateWorkspaceRequest {
            name: "Pages".into(),
            root: None,
            operation_id: "workspace".into(),
        })
        .unwrap()
        .record
        .id;
    let locator = serde_json::json!({"artifactId":"page","imageHash":"hash"}).to_string();
    let image = format!("data:image/jpeg;base64,{}", "A".repeat(4 * 1024 * 1024));
    let receipt = serde_json::json!({"success":true,"contentItems":[{"type":"inputText","text":locator},{"type":"inputImage","imageUrl":image}]});
    let connection = source.connection().unwrap();
    connection.execute("INSERT INTO tool_receipts (id, workspace_id, session_id, provider_turn_id, call_id, tool_name, catalog_version, arguments_json, state, result_json, started_at) VALUES ('receipt', ?1, ?2, 'turn', 'call', 'workbench_paper_page', 1, '{}', 'completed', ?3, 't')", params![workspace, session, receipt.to_string()]).unwrap();
    drop(connection);
    let output = tempfile::tempdir().unwrap();
    let path = export(&source, output.path());

    let mut archive = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();
    let mut bytes = Vec::new();
    archive
        .by_name("database/research.sqlite3")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() < 3 * 1024 * 1024, "{}", bytes.len());
    let db = output.path().join("snapshot.sqlite3");
    fs::write(&db, bytes).unwrap();
    let stored: String = Connection::open(db)
        .unwrap()
        .query_row("SELECT result_json FROM tool_receipts", [], |r| r.get(0))
        .unwrap();
    assert!(!stored.contains("base64,"));
    assert!(stored.contains("artifactId"));

    let target = tempfile::tempdir().unwrap();
    let store = Store::open_at(target.path()).unwrap();
    import(&store, &path).unwrap();
}

/// A project whose command, run receipt, and captured plan all sit in `analysis/`.
fn project_with_command() -> (tempfile::TempDir, Store, PathBuf, String) {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    fs::create_dir_all(project.join("analysis")).unwrap();
    let store = Store::open_at(&root.path().join("store")).unwrap();
    let workspace = store
        .create_workspace(crate::workbench::store::CreateWorkspaceRequest {
            name: "Project".into(),
            root: Some(project.to_string_lossy().into_owned()),
            operation_id: "workspace".into(),
        })
        .unwrap()
        .record
        .id;
    let cwd = fs::canonicalize(project.join("analysis")).unwrap();
    let profile = crate::workbench::research::save_execution_profile(
        &store,
        crate::workbench::research::SaveExecutionProfileRequest {
            profile_id: None,
            workspace_id: workspace.clone(),
            name: "Analysis".into(),
            adapter: "command".into(),
            argv: vec!["/bin/sh".into(), "-c".into(), "true".into()],
            cwd: cwd.to_string_lossy().into_owned(),
            environment: serde_json::json!({}),
            inputs: vec![],
            timeout_seconds: 30,
            outputs: vec![],
            expected_revision: None,
            operation_id: "profile".into(),
        },
    )
    .unwrap();
    let connection = store.connection().unwrap();
    connection.execute("INSERT INTO research_executions (id, workspace_id, adapter, command_json, cwd, input_manifest_json, dependency_hash, outcome, output_manifest_json, validation_json, snapshot_consistency, created_at) VALUES ('run', ?1, 'command', '[]', ?2, '{}', 'hash', 'completed', '{}', '{}', 'complete', 't')", params![workspace, cwd.to_string_lossy()]).unwrap();
    connection.execute("INSERT INTO desk_records(id,workspace_id,kind,title,body_json,content_hash,created_at) VALUES('plan',?1,'execution_plan','Plan',?2,'captured-hash','t')", params![workspace, serde_json::json!({"profile":{"cwd":cwd.to_string_lossy()}}).to_string()]).unwrap();
    drop(connection);
    (root, store, cwd, profile.id)
}

#[test]
fn a_remapped_folder_takes_its_commands_receipts_and_plans_with_it() {
    let (_root, source, old_cwd, profile) = project_with_command();
    let old_root = old_cwd.parent().unwrap().to_string_lossy().into_owned();
    let output = tempfile::tempdir().unwrap();
    let path = export(&source, output.path());

    let target = tempfile::tempdir().unwrap();
    let moved = target.path().join("moved");
    fs::create_dir_all(moved.join("analysis")).unwrap();
    let moved = fs::canonicalize(moved).unwrap();
    let store = Store::open_at(&target.path().join("store")).unwrap();
    import_archive(
        &store,
        ImportArchiveRequest {
            path: path.to_string_lossy().into(),
            root_mappings: BTreeMap::from([(old_root, Some(moved.to_string_lossy().into_owned()))]),
        },
    )
    .unwrap();

    let preview = crate::workbench::research::preview_host_execution(&store, &profile).unwrap();
    assert_eq!(Path::new(&preview.cwd), moved.join("analysis"));
    let connection = store.connection().unwrap();
    let run: String = connection
        .query_row("SELECT cwd FROM research_executions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(Path::new(&run), moved.join("analysis"));
    let (body, hash): (String, String) = connection
        .query_row(
            "SELECT body_json,content_hash FROM desk_records WHERE id='plan'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        Path::new(body["profile"]["cwd"].as_str().unwrap()),
        moved.join("analysis")
    );
    assert_eq!(hash, "captured-hash");
}

#[test]
fn a_project_restored_without_a_folder_asks_for_one() {
    let (_root, source, old_cwd, profile) = project_with_command();
    let old_root = old_cwd.parent().unwrap().to_string_lossy().into_owned();
    let output = tempfile::tempdir().unwrap();
    let path = export(&source, output.path());
    let target = tempfile::tempdir().unwrap();
    let store = Store::open_at(target.path()).unwrap();
    import_archive(
        &store,
        ImportArchiveRequest {
            path: path.to_string_lossy().into(),
            root_mappings: BTreeMap::from([(old_root, None)]),
        },
    )
    .unwrap();
    let error = crate::workbench::research::preview_host_execution(&store, &profile).unwrap_err();
    assert!(
        error.message.contains("Attach a folder"),
        "{}",
        error.message
    );
}

#[test]
fn only_a_path_inside_the_old_folder_moves() {
    use super::restore::moved_path;
    assert_eq!(
        moved_path("/old/a/b", "/old", "/new").as_deref(),
        Some(Path::new("/new/a/b").to_str().unwrap())
    );
    assert_eq!(moved_path("/old", "/old", "/new").as_deref(), Some("/new"));
    assert_eq!(
        moved_path(r"C:\old\a", r"C:\old", "/new").as_deref(),
        Some(Path::new("/new/a").to_str().unwrap())
    );
    assert_eq!(moved_path("/older/a", "/old", "/new"), None);
    assert_eq!(moved_path("/old/../etc", "/old", "/new"), None);
}
