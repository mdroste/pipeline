//! Version-two archive payload for app-owned conversation working files.
use super::*;

fn io(error: std::io::Error) -> WorkbenchError {
    WorkbenchError::storage("Conversation file archive failed", error)
}
fn directory(path: &Path) -> WorkbenchResult<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => Ok(true),
        Ok(_) => Err(WorkbenchError::invalid(
            "Conversation archive folders must be real directories",
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(io(e)),
    }
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
fn walk(
    root: &Path,
    files: &mut Vec<PathBuf>,
    entries: &mut usize,
    depth: usize,
) -> WorkbenchResult<()> {
    if depth > 32 {
        return Err(WorkbenchError::invalid(
            "Conversation files exceed archive depth limit",
        ));
    }
    for entry in fs::read_dir(root).map_err(io)? {
        *entries += 1;
        if *entries > MAX_ARCHIVE_ENTRIES {
            return Err(WorkbenchError::invalid(
                "Too many conversation archive entries",
            ));
        }
        let entry = entry.map_err(io)?;
        let kind = entry.file_type().map_err(io)?;
        if kind.is_dir() {
            walk(&entry.path(), files, entries, depth + 1)?;
        } else if kind.is_file() {
            files.push(entry.path());
        } else {
            return Err(WorkbenchError::invalid(
                "Conversation backups cannot include symbolic links or special files",
            ));
        }
    }
    Ok(())
}
pub(super) fn stage(
    store: &Store,
    db: &Connection,
    staging: &Path,
) -> WorkbenchResult<Vec<BlobManifest>> {
    let jobs = store.root_path().join("jobs");
    directory(&jobs)?;
    let root = jobs.join("conversations");
    if !directory(&root)? {
        return Ok(Vec::new());
    }
    let mut statement = db
        .prepare("SELECT id FROM sessions ORDER BY id")
        .map_err(|e| WorkbenchError::storage("List archived conversations", e))?;
    let ids = statement
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| WorkbenchError::storage("Read archived conversations", e))?;
    let mut output = Vec::new();
    let mut entries = 0;
    let mut total = 0u64;
    for id in ids {
        let id = id.map_err(|e| WorkbenchError::storage("Decode archived conversation", e))?;
        if !valid_id(&id) {
            return Err(WorkbenchError::invalid(
                "Invalid archived conversation identity",
            ));
        }
        let session = root.join(&id);
        if !directory(&session)? {
            continue;
        }
        let mut files = Vec::new();
        walk(&session, &mut files, &mut entries, 0)?;
        files.sort();
        for source in files {
            if source.strip_prefix(&root).unwrap().components().any(|c| {
                c.as_os_str()
                    .to_str()
                    .is_none_or(|name| name.contains('\\'))
            }) {
                return Err(WorkbenchError::invalid(
                    "Conversation filenames must be portable UTF-8 components",
                ));
            }
            let relative = source
                .strip_prefix(&root)
                .unwrap()
                .to_str()
                .ok_or_else(|| WorkbenchError::invalid("Conversation filename is not UTF-8"))?
                .replace('\\', "/");
            let archive_path = format!("conversation-files/{relative}");
            if !safe_archive_name(&archive_path) {
                return Err(WorkbenchError::invalid("Invalid conversation archive path"));
            }
            let target = staging.join(&archive_path);
            fs::create_dir_all(target.parent().unwrap()).map_err(io)?;
            let count = std::io::copy(
                &mut crate::safety::open_regular_file(&source)
                    .map_err(WorkbenchError::invalid)?
                    .take(MAX_ENTRY_BYTES + 1),
                &mut File::create(&target).map_err(io)?,
            )
            .map_err(io)?;
            total = total.saturating_add(count);
            if count > MAX_ENTRY_BYTES || total > MAX_ARCHIVE_BYTES {
                return Err(WorkbenchError::invalid(
                    "Conversation files exceed archive byte limits",
                ));
            }
            output.push(BlobManifest {
                archive_path,
                original_reference: relative,
                size_bytes: count,
                sha256: hash_reader(File::open(target).map_err(io)?)?,
            });
        }
    }
    Ok(output)
}
pub(super) fn validate(
    files: &[BlobManifest],
    sessions: &[String],
    staging: &Path,
    expected: &mut HashSet<String>,
) -> WorkbenchResult<()> {
    for file in files {
        let Some((session, name)) = file.original_reference.split_once('/') else {
            return Err(WorkbenchError::invalid("Missing conversation file owner"));
        };
        if !valid_id(session)
            || !sessions.iter().any(|id| id == session)
            || name.is_empty()
            || !safe_archive_name(&file.original_reference)
            || file.archive_path != format!("conversation-files/{}", file.original_reference)
            || !expected.insert(file.archive_path.clone())
        {
            return Err(WorkbenchError::invalid(
                "Invalid or duplicate conversation file identity",
            ));
        }
        let path = staging.join(&file.archive_path);
        if fs::metadata(&path).map_err(io)?.len() != file.size_bytes
            || hash_reader(File::open(path).map_err(io)?)? != file.sha256
        {
            return Err(WorkbenchError::invalid(
                "Conversation file failed its size or hash check",
            ));
        }
    }
    Ok(())
}
// Imported payload is installed only in a fresh conversation store. A failed DB
// restore removes this new tree; existing local files are never overwritten.
pub(super) struct Installed {
    path: Option<PathBuf>,
}
impl Installed {
    pub(super) fn commit(&mut self) {
        self.path = None;
    }
}
impl Drop for Installed {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            let _ = fs::remove_dir_all(path);
        }
    }
}
pub(super) fn install(
    store: &Store,
    files: &[BlobManifest],
    staging: &Path,
) -> WorkbenchResult<Installed> {
    if files.is_empty() {
        return Ok(Installed { path: None });
    }
    let jobs = store.root_path().join("jobs");
    directory(&jobs)?;
    let destination = jobs.join("conversations");
    if directory(&destination)? {
        if fs::read_dir(&destination).map_err(io)?.next().is_some() {
            return Err(WorkbenchError::invalid(
                "Restore requires an empty conversation file folder",
            ));
        }
        fs::remove_dir(&destination).map_err(io)?;
    }
    let temp = tempfile::tempdir_in(&jobs).map_err(io)?;
    for file in files {
        let target = temp.path().join(&file.original_reference);
        fs::create_dir_all(target.parent().unwrap()).map_err(io)?;
        fs::copy(staging.join(&file.archive_path), target).map_err(io)?;
    }
    fs::rename(temp.path(), &destination).map_err(io)?;
    Ok(Installed {
        path: Some(destination),
    })
}
