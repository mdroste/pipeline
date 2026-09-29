//! File publication barriers for coordinator-owned receipts and snapshots.
use super::store::{err, Result};
use std::{fs, io::Write, path::Path};

pub(super) fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(err)?;
    #[cfg(not(unix))]
    let _ = path; // Windows publication uses a write-through move below.
    Ok(())
}

pub(super) fn create_directories(path: &Path) -> Result<()> {
    if !path.is_dir() {
        let parent = path.parent().ok_or("Invalid artifact directory")?;
        create_directories(parent)?;
        match fs::create_dir(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && path.is_dir() => {}
            Err(e) => return Err(err(e)),
        }
    }
    // Also retry this barrier if an earlier creation succeeded but its sync failed.
    if let Some(parent) = path.parent() {
        sync_directory(parent)?;
    }
    Ok(())
}

fn rename_file(source: &Path, target: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            MoveFileExW(
                source.as_ptr(),
                target.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(err(std::io::Error::last_os_error()));
        }
    }
    #[cfg(not(windows))]
    fs::rename(source, target).map_err(err)?;
    Ok(())
}
pub(super) fn rename(source: &Path, target: &Path) -> Result<()> {
    rename_file(source, target)?;
    sync_directory(target.parent().ok_or("Invalid artifact path")?)
}

pub(super) fn publish(path: &Path, bytes: &[u8]) -> Result<()> {
    publish_with_barrier(path, bytes, sync_directory)
}

fn publish_with_barrier(
    path: &Path,
    bytes: &[u8],
    barrier: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let parent = path.parent().ok_or("Invalid artifact path")?;
    create_directories(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(err)?;
    file.write_all(bytes).map_err(err)?;
    file.flush().map_err(err)?;
    file.as_file().sync_all().map_err(err)?;
    rename_file(file.path(), path)?;
    barrier(parent)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn result_publication_requires_the_directory_barrier() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("actions/operation/result.json");
        assert!(publish_with_barrier(&path, b"{\"done\":true}", |_| Err(
            "directory sync failed".into()
        ))
        .is_err());
        // A post-rename error is ambiguous, never reported as successful dispatch.
        assert_eq!(fs::read(&path).unwrap(), b"{\"done\":true}");
        publish(&path, b"{\"done\":false}").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"{\"done\":false}");
    }
    #[test]
    fn publication_rejects_invalid_parent_without_replacing_existing_data() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("file");
        fs::write(&file, "keep").unwrap();
        assert!(publish(&file.join("result.json"), b"new").is_err());
        assert_eq!(fs::read_to_string(file).unwrap(), "keep");
    }
}
