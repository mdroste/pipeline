//! Folder browsing and file operations for a project's working folder. Listings
//! and stats read metadata only, so they ignore the inventory's size limits.
//! Moves and Trash run inside `studio_mutate`, under the project lock, and only
//! on the working folder: task copies and captured revisions stay read-only.
//! That call's `project_operations` row is the record of what moved where.
use super::file_workspace::FileReadRequest;
use super::files::{EntryKind, EntryStat, SafeRoot};
use super::*;

const MAX_LISTED: usize = 5_000;
const MAX_STATS: usize = 64;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileListRequest {
    pub workspace_id: String,
    pub directory: String,
    pub checkpoint_id: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileListEntry {
    pub name: String,
    pub path: String,
    pub kind: &'static str,
    pub size: u64,
    pub modified_ms: i64,
    pub ignored: bool,
    pub executable: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileListing {
    pub directory: String,
    pub entries: Vec<FileListEntry>,
    pub truncated: bool,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileStatRequest {
    pub workspace_id: String,
    pub paths: Vec<String>,
    pub checkpoint_id: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStamp {
    pub path: String,
    pub kind: Option<&'static str>,
    /// Opaque; changes when the file does. `None` when the path is gone.
    pub stamp: Option<String>,
    pub size: u64,
    pub modified_ms: i64,
}

fn kind_label(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::File => "file",
        EntryKind::Folder => "folder",
        EntryKind::Link => "link",
        EntryKind::Other => "other",
    }
}
pub(super) fn base(
    store: &Store,
    workspace_id: &str,
    checkpoint_id: Option<&str>,
) -> WorkbenchResult<PathBuf> {
    match checkpoint_id {
        Some(checkpoint) => session_task_root(store, workspace_id, checkpoint),
        None => root(store, workspace_id),
    }
}
/// The validated folder and relative path for the off-worker format readers.
pub fn scoped_folder(
    store: &Store,
    workspace_id: &str,
    checkpoint_id: Option<&str>,
    path: &str,
) -> WorkbenchResult<PathBuf> {
    scope(store, workspace_id)?;
    files::relative(path)?;
    base(store, workspace_id, checkpoint_id)
}
pub fn search_folder(
    store: &Store,
    workspace_id: &str,
    checkpoint_id: Option<&str>,
) -> WorkbenchResult<PathBuf> {
    scope(store, workspace_id)?;
    base(store, workspace_id, checkpoint_id)
}
/// One folder of the tree. `folder` comes from `search_folder`, so this walk
/// holds no database worker, however slow the volume.
pub fn list_workspace_directory(
    folder: &Path,
    request: FileListRequest,
) -> WorkbenchResult<FileListing> {
    if !request.directory.is_empty() {
        files::relative(&request.directory)?;
    }
    let (entries, truncated) = SafeRoot::open(folder)?.list(&request.directory, MAX_LISTED)?;
    Ok(FileListing {
        entries: entries
            .into_iter()
            .map(|(name, stat)| {
                let path = if request.directory.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{name}", request.directory)
                };
                FileListEntry {
                    ignored: ignored(&path, &[]),
                    kind: kind_label(stat.kind),
                    size: stat.size,
                    modified_ms: stat.modified_ms,
                    executable: stat.executable,
                    name,
                    path,
                }
            })
            .collect(),
        directory: request.directory,
        truncated,
    })
}
/// Stamps for the change checks. Like a listing, this runs off the database
/// workers on a folder from `search_folder`.
pub fn stat_workspace_files(
    folder: &Path,
    request: FileStatRequest,
) -> WorkbenchResult<Vec<FileStamp>> {
    if request.paths.len() > MAX_STATS {
        return Err(WorkbenchError::invalid("Too many paths in one check"));
    }
    let safe = SafeRoot::open(folder)?;
    request
        .paths
        .into_iter()
        .map(|path| {
            let stat = if path.is_empty() {
                Some(safe.root_stat()?)
            } else {
                safe.stat(&path)?
            };
            Ok(FileStamp {
                kind: stat.as_ref().map(|s| kind_label(s.kind)),
                stamp: stat.as_ref().map(EntryStat::stamp),
                size: stat.as_ref().map_or(0, |s| s.size),
                modified_ms: stat.as_ref().map_or(0, |s| s.modified_ms),
                path,
            })
        })
        .collect()
}
fn live_target(store: &Store, request: &FileReadRequest) -> WorkbenchResult<(PathBuf, EntryStat)> {
    scope(store, &request.workspace_id)?;
    if request.revision_id.is_some() {
        return Err(WorkbenchError::invalid(
            "Captured documents have no writable external file",
        ));
    }
    files::relative(&request.path)?;
    let folder = base(
        store,
        &request.workspace_id,
        request.checkpoint_id.as_deref(),
    )?;
    let stat = SafeRoot::open(&folder)?
        .stat(&request.path)?
        .ok_or_else(|| WorkbenchError::invalid(format!("Missing file: {}", request.path)))?;
    Ok((folder.join(&request.path), stat))
}
/// The path Reveal in folder shows. Any listed entry qualifies, so a link is
/// shown beside its siblings, never resolved.
pub fn reveal_target(store: &Store, request: &FileReadRequest) -> WorkbenchResult<PathBuf> {
    live_target(store, request).map(|(path, _)| path)
}
/// The path handed to the system's default application. A program must never be
/// one click away from running, so anything the system would execute is refused.
pub fn external_target(store: &Store, request: &FileReadRequest) -> WorkbenchResult<PathBuf> {
    let (path, stat) = live_target(store, request)?;
    if stat.kind != EntryKind::File {
        return Err(WorkbenchError::invalid("Only regular files can be opened"));
    }
    crate::file_viewer::check_openable(&request.path, stat.executable)
        .map_err(WorkbenchError::invalid)?;
    Ok(crate::file_viewer::system_path(&path))
}

pub(super) fn create_folder(store: &Store, ws: &str, path: &str) -> WorkbenchResult<Value> {
    files::relative(path)?;
    SafeRoot::open(&root(store, ws)?)?.create_folder(path)?;
    Ok(json!({ "path": path }))
}
pub(super) fn move_entry(store: &Store, ws: &str, from: &str, to: &str) -> WorkbenchResult<Value> {
    research::jobs::ensure_paths_free(ws, &root(store, ws)?, [from, to])?;
    SafeRoot::open(&root(store, ws)?)?.move_entry(from, to)?;
    Ok(json!({ "from": from, "to": to }))
}
/// Copy one file from elsewhere on this computer into the project, as a
/// reversible edit. The researcher chose `source` in a dialog or by dropping it.
pub(super) fn import_file(
    store: &Store,
    ws: &str,
    directory: &str,
    source: &str,
) -> WorkbenchResult<Value> {
    let source = Path::new(source);
    if !source.is_absolute() {
        return Err(WorkbenchError::invalid("Choose a file on this computer"));
    }
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| WorkbenchError::invalid("This file name cannot be used in a project"))?;
    let path = if directory.is_empty() {
        name.to_string()
    } else {
        format!("{directory}/{name}")
    };
    let file = crate::safety::open_regular_file(source).map_err(err)?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(WorkbenchError::invalid(
            "Files added here are limited to 32 MB",
        ));
    }
    super::assistant_files::create_file(store, ws, None, &path, &bytes)?;
    Ok(json!({ "path": path }))
}
/// Send one entry to the system Trash. Nothing here deletes permanently: when
/// the system offers no Trash for this volume, the action fails.
pub(super) fn trash_entry(store: &Store, ws: &str, path: &str) -> WorkbenchResult<Value> {
    files::relative(path)?;
    let folder = root(store, ws)?;
    research::jobs::ensure_paths_free(ws, &folder, [path])?;
    let stat = SafeRoot::open(&folder)?
        .stat(path)?
        .ok_or_else(|| WorkbenchError::invalid(format!("Missing file: {path}")))?;
    if !matches!(stat.kind, EntryKind::File | EntryKind::Folder) {
        return Err(WorkbenchError::invalid(
            "Only files and folders can be moved to the Trash",
        ));
    }
    // `SafeRoot::stat` walked the parents without following links. The Trash API
    // takes a pathname, so confirm it still resolves inside the project.
    let target = folder.join(path);
    let parent = target
        .parent()
        .ok_or_else(|| WorkbenchError::invalid("Invalid file path"))?
        .canonicalize()
        .map_err(err)?;
    if !parent.starts_with(folder.canonicalize().map_err(err)?) {
        return Err(WorkbenchError::invalid("This file left the project folder"));
    }
    let name = target
        .file_name()
        .ok_or_else(|| WorkbenchError::invalid("Invalid file path"))?;
    system_trash(&parent.join(name))?;
    Ok(json!({ "path": path }))
}
/// The Windows shell deletes outright on a volume with no Recycle Bin, whatever
/// it was asked, so only a fixed local disk is trusted with an item. `drive_type`
/// answers as `GetDriveTypeW` does for the volume holding a path.
#[cfg(any(windows, test))]
fn require_recycle_bin(path: &str, drive_type: impl FnOnce(&str) -> u32) -> WorkbenchResult<()> {
    const DRIVE_FIXED: u32 = 3;
    let path = crate::file_viewer::windows_system_path(path);
    if path.starts_with(r"\\") || drive_type(&path) != DRIVE_FIXED {
        return Err(WorkbenchError::invalid(
            "This drive has no Recycle Bin, so Pipeline left the item where it is. Nothing was \
             deleted.",
        ));
    }
    Ok(())
}
#[cfg(windows)]
fn drive_type(path: &str) -> u32 {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetVolumePathNameW};
    let path: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    let mut volume = [0u16; 1024];
    // SAFETY: the path is NUL-terminated and the buffer is writable for its
    // stated length. A failed lookup reports an unknown drive.
    unsafe {
        if GetVolumePathNameW(path.as_ptr(), volume.as_mut_ptr(), volume.len() as u32) == 0 {
            return 0;
        }
        GetDriveTypeW(volume.as_ptr())
    }
}
pub(super) fn system_trash(path: &Path) -> WorkbenchResult<()> {
    #[cfg(windows)]
    require_recycle_bin(&path.to_string_lossy(), drive_type)?;
    #[allow(unused_mut)]
    let mut context = trash::TrashContext::default();
    // The default Finder route needs the Automation permission and a prompt.
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos as _};
        context.set_delete_method(DeleteMethod::NsFileManager);
    }
    context.delete(path).map_err(|error| {
        WorkbenchError::storage(
            "The system Trash refused this item; nothing was deleted",
            error,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::files::{EntryKind, SafeRoot};
    use super::{
        list_workspace_directory, require_recycle_bin, stat_workspace_files, FileListRequest,
        FileStatRequest,
    };
    use std::fs;

    #[test]
    fn trash_is_refused_where_a_deletion_would_be_permanent() {
        const FIXED: u32 = 3;
        const REMOVABLE: u32 = 2;
        const REMOTE: u32 = 4;
        const UNKNOWN: u32 = 0;
        let local = r"\\?\C:\Users\Mike\paper\main.tex";
        require_recycle_bin(local, |path| {
            assert_eq!(path, r"C:\Users\Mike\paper\main.tex");
            FIXED
        })
        .unwrap();
        for drive in [REMOVABLE, REMOTE, UNKNOWN] {
            let error = require_recycle_bin(local, |_| drive).unwrap_err();
            assert!(
                error.message.contains("no Recycle Bin"),
                "{}",
                error.message
            );
            assert!(error.message.contains("Nothing was deleted"));
        }
        for share in [r"\\?\UNC\server\share\main.tex", r"\\server\share\main.tex"] {
            assert!(require_recycle_bin(share, |_| panic!("no drive to ask about")).is_err());
        }
    }

    #[test]
    fn listing_and_stamps_read_a_folder_without_the_store() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("a.txt"), "a").unwrap();
        let listing = list_workspace_directory(
            temp.path(),
            FileListRequest {
                workspace_id: "unused".into(),
                directory: String::new(),
                checkpoint_id: None,
            },
        )
        .unwrap();
        assert_eq!(listing.entries[0].path, "a.txt");
        let refused = list_workspace_directory(
            temp.path(),
            FileListRequest {
                workspace_id: "unused".into(),
                directory: "../outside".into(),
                checkpoint_id: None,
            },
        );
        assert!(refused.is_err());
        let stamps = stat_workspace_files(
            temp.path(),
            FileStatRequest {
                workspace_id: "unused".into(),
                paths: vec![String::new(), "a.txt".into(), "gone.txt".into()],
                checkpoint_id: None,
            },
        )
        .unwrap();
        assert!(stamps[0].stamp.is_some() && stamps[1].stamp.is_some());
        assert_eq!(stamps[2].stamp, None);
    }

    #[test]
    fn listing_is_metadata_only_folders_first_and_hides_reserved_names() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("zeta/deep")).unwrap();
        fs::create_dir(root.join(".git")).unwrap();
        fs::create_dir(root.join(".pipeline-tasks")).unwrap();
        fs::write(root.join("alpha.txt"), "a").unwrap();
        fs::write(root.join("Beta.txt"), "b").unwrap();
        fs::write(root.join("zeta/inner.txt"), "c").unwrap();
        let safe = SafeRoot::open(root).unwrap();
        let (entries, truncated) = safe.list("", 100).unwrap();
        let names: Vec<_> = entries.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["zeta", "alpha.txt", "Beta.txt"]);
        assert!(!truncated);
        let (entries, _) = safe.list("zeta", 100).unwrap();
        assert_eq!(entries[0].0, "deep");
        assert_eq!(entries[0].1.kind, EntryKind::Folder);
        assert_eq!(entries[1].1.size, 1);
        let (entries, truncated) = safe.list("", 2).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(truncated);
        assert!(safe.list("missing", 10).is_err());
        assert!(safe.list(".git", 10).is_err());
        assert!(safe.list("../", 10).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn links_are_reported_but_never_followed() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        let outside = temp.path().join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), "secret").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
        std::os::unix::fs::symlink(outside.join("secret.txt"), root.join("link.txt")).unwrap();
        let safe = SafeRoot::open(&root).unwrap();
        let (entries, _) = safe.list("", 10).unwrap();
        assert!(entries.iter().all(|(_, s)| s.kind == EntryKind::Link));
        assert!(safe.list("escape", 10).is_err());
        assert!(safe.stat("escape/secret.txt").is_err());
        assert!(safe.open_read("link.txt").is_err());
        assert!(safe.open_read("escape/secret.txt").is_err());
        assert!(safe.move_entry("link.txt", "moved.txt").is_err());
        assert!(safe.move_entry("escape", "moved").is_err());
        assert!(root.join("link.txt").symlink_metadata().is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn fifos_are_listed_but_never_opened() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("pipe");
        let name = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        // SAFETY: the name is a valid NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        let safe = SafeRoot::open(temp.path()).unwrap();
        assert_eq!(safe.stat("pipe").unwrap().unwrap().kind, EntryKind::Other);
        assert!(safe.open_read("pipe").is_err());
    }

    #[test]
    fn stamps_change_with_contents_and_open_read_has_no_size_limit() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("data.bin");
        let file = fs::File::create(&path).unwrap();
        file.set_len(64 * 1024 * 1024).unwrap();
        let safe = SafeRoot::open(temp.path()).unwrap();
        let (_, stat) = safe.open_read("data.bin").unwrap().unwrap();
        assert_eq!(stat.size, 64 * 1024 * 1024);
        assert!(safe.read("data.bin").is_err());
        let before = safe.stat("data.bin").unwrap().unwrap().stamp();
        assert_eq!(before, stat.stamp());
        file.set_len(5).unwrap();
        assert_ne!(safe.stat("data.bin").unwrap().unwrap().stamp(), before);
        assert_eq!(safe.stat("missing/data.bin").unwrap(), None);
        assert_eq!(safe.stat("gone.bin").unwrap(), None);
    }

    #[test]
    fn moves_never_replace_and_keep_folders_out_of_themselves() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::create_dir(root.join("c")).unwrap();
        fs::write(root.join("one.txt"), "one").unwrap();
        fs::write(root.join("two.txt"), "two").unwrap();
        let safe = SafeRoot::open(root).unwrap();
        let error = safe.move_entry("one.txt", "two.txt").unwrap_err();
        assert!(error.message.contains("Already exists"));
        assert_eq!(fs::read(root.join("two.txt")).unwrap(), b"two");
        assert_eq!(fs::read(root.join("one.txt")).unwrap(), b"one");
        assert!(safe.move_entry("a", "a/b/a").is_err());
        safe.move_entry("one.txt", "c/one.txt").unwrap();
        assert_eq!(fs::read(root.join("c/one.txt")).unwrap(), b"one");
        safe.move_entry("a", "c/a").unwrap();
        assert!(root.join("c/a/b").is_dir());
        assert!(safe.move_entry("c/one.txt", "missing/one.txt").is_err());
        assert!(safe.move_entry("c/one.txt", ".git/one.txt").is_err());
        safe.create_folder("c/new").unwrap();
        assert!(safe.create_folder("c/new").is_err());
        assert!(safe.create_folder("missing/new").is_err());
    }

    #[test]
    fn case_only_renames_succeed_on_every_volume() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("readme.md"), "text").unwrap();
        let safe = SafeRoot::open(temp.path()).unwrap();
        safe.move_entry("readme.md", "README.md").unwrap();
        let (entries, _) = safe.list("", 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "README.md");
    }

    #[test]
    fn a_case_only_rename_may_change_any_letter() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("École.tex"), "text").unwrap();
        let safe = SafeRoot::open(temp.path()).unwrap();
        safe.move_entry("École.tex", "école.tex").unwrap();
        let (entries, _) = safe.list("", 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "école.tex");
    }

    #[test]
    fn a_second_name_for_the_same_file_is_still_a_collision() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("one.txt"), "text").unwrap();
        fs::hard_link(temp.path().join("one.txt"), temp.path().join("two.txt")).unwrap();
        let error = SafeRoot::open(temp.path())
            .unwrap()
            .move_entry("one.txt", "two.txt")
            .unwrap_err();
        assert!(error.message.contains("Already exists"));
        assert!(temp.path().join("one.txt").exists());
    }

    #[test]
    fn a_listing_cut_at_its_limit_keeps_every_folder_it_can() {
        let temp = tempfile::tempdir().unwrap();
        for index in 0..40 {
            fs::write(temp.path().join(format!("file{index}.txt")), "a").unwrap();
        }
        for name in ["first", "last"] {
            fs::create_dir(temp.path().join(name)).unwrap();
        }
        let safe = SafeRoot::open(temp.path()).unwrap();
        let (entries, truncated) = safe.list("", 3).unwrap();
        assert!(truncated);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].0, "first");
        assert_eq!(entries[1].0, "last");
        assert_eq!(entries[2].1.kind, EntryKind::File);
        let (entries, truncated) = safe.list("", 1).unwrap();
        assert!(truncated);
        assert_eq!(entries[0].1.kind, EntryKind::Folder);
        assert!(!safe.list("", 42).unwrap().1);
    }

    #[test]
    fn a_listing_stopped_early_keeps_what_it_read_and_says_it_was_cut_short() {
        let temp = tempfile::tempdir().unwrap();
        for index in 0..10 {
            fs::write(temp.path().join(format!("file{index}.txt")), "a").unwrap();
        }
        let safe = SafeRoot::open(temp.path()).unwrap();
        let (entries, truncated) = safe.list_until("", 100, |_| true).unwrap();
        assert!(entries.is_empty());
        assert!(truncated);
        let mut asked = 0;
        let (entries, truncated) = safe
            .list_until("", 100, |_| {
                asked += 1;
                asked > 6
            })
            .unwrap();
        assert!((1..10).contains(&entries.len()));
        assert!(truncated);
        // A full listing stops looking for folders when its time is up.
        let (entries, truncated) = safe.list_until("", 2, |full| full).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(truncated);
        let (entries, truncated) = safe.list_until("", 100, |_| false).unwrap();
        assert_eq!(entries.len(), 10);
        assert!(!truncated);
    }
}
