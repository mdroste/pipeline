//! Immutable project snapshot storage and verified reads.

use super::{err, hash, id, now, MAX_FILE};
use crate::workbench::store::{outside_folder_error, Store, WorkbenchError, WorkbenchResult};
use rusqlite::params;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

pub(super) fn blob(
    store: &Store,
    workspace_id: &str,
    bytes: &[u8],
    origin: &str,
) -> WorkbenchResult<String> {
    if bytes.len() as u64 > MAX_FILE {
        return Err(WorkbenchError::invalid("Snapshot exceeds 32 MiB"));
    }
    let digest = hash(bytes);
    let reference = store
        .root_path()
        .join("blobs")
        .join(format!("{digest}.project"));
    if !reference.exists() {
        let mut tmp =
            tempfile::NamedTempFile::new_in(store.root_path().join("blobs")).map_err(err)?;
        tmp.write_all(bytes).map_err(err)?;
        tmp.as_file().sync_all().map_err(err)?;
        match tmp.persist_noclobber(&reference) {
            Ok(_) => (),
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(err(e)),
        }
    }
    if hash(&fs::read(&reference).map_err(err)?) != digest {
        return Err(WorkbenchError::invalid("Immutable blob integrity failure"));
    }
    store.connection()?.execute("INSERT OR IGNORE INTO artifacts(id,workspace_id,content_hash,media_kind,size_bytes,origin,storage_reference,created_at) VALUES(?1,?2,?3,'project_snapshot',?4,?5,?6,?7)",params![id("artifact")?,workspace_id,digest,bytes.len() as i64,origin,reference.to_string_lossy(),now()]).map_err(err)?;
    Ok(digest)
}
pub(super) fn blob_read(
    store: &Store,
    workspace_id: &str,
    digest: &str,
) -> WorkbenchResult<Vec<u8>> {
    if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(WorkbenchError::invalid("Invalid snapshot hash"));
    }
    let reference:String=store.connection()?.query_row("SELECT storage_reference FROM artifacts WHERE workspace_id=?1 AND content_hash=?2 AND media_kind='project_snapshot'",params![workspace_id,digest],|r|r.get(0)).map_err(err)?;
    let path = Path::new(&reference);
    if path.parent() != Some(store.root_path().join("blobs").as_path()) {
        return Err(outside_folder_error(
            "This snapshot",
            path,
            "Snapshot is outside immutable storage",
        ));
    }
    let mut f = crate::safety::open_regular_file(path).map_err(err)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut f)
        .take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() as u64 > MAX_FILE || hash(&bytes) != digest {
        return Err(WorkbenchError::invalid("Snapshot is missing or corrupted"));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::*;

    #[test]
    fn undo_reads_its_snapshots_after_the_data_folder_moves() {
        let mut f = fixture();
        fs::write(f.root.join("main.tex"), "before").unwrap();
        let c = copy(&f, &["main.tex"]);
        let body: Checkpoint = decode(&c).unwrap();
        fs::write(Path::new(&body.task_root).join("main.tex"), "after").unwrap();
        let c = capture(&f, &c);
        let applied = mutation(
            &f,
            ProjectAction::Apply {
                checkpoint_id: c.id.clone(),
                paths: vec!["main.tex".into()],
                expected_revision: c.revision,
            },
        );
        let moved = f._temp.path().join("moved");
        fs::rename(f.store.root_path(), &moved).unwrap();
        f.store = Store::open_at(&moved).unwrap();
        mutation(
            &f,
            ProjectAction::Undo {
                application_id: applied.id,
            },
        );
        assert_eq!(
            fs::read_to_string(f.root.join("main.tex")).unwrap(),
            "before"
        );
    }
}
