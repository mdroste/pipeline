use super::*;
use crate::workbench::store::CreateSessionRequest;
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
        entries.push((entry.name().to_string(), bytes));
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
    assert!(error.message.contains("exceeds 4 MiB"), "{}", error.message);
    let empty = tempfile::tempdir().unwrap();
    let store = Store::open_at(empty.path()).unwrap();
    let error = import(&store, &path).unwrap_err();
    assert!(
        error.message.contains("size does not match"),
        "{}",
        error.message
    );
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
