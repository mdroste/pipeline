//! Replace a file without deleting the previous version on failure.
use std::path::Path;
use tempfile::{NamedTempFile, PersistError};

pub fn persist_file(
    file: NamedTempFile,
    path: impl AsRef<Path>,
) -> Result<std::fs::File, PersistError> {
    #[cfg(windows)]
    {
        persist_until(file, path.as_ref(), std::time::Duration::from_secs(2))
    }
    #[cfg(not(windows))]
    file.persist(path)
}

#[cfg(windows)]
fn persist_until(
    mut file: NamedTempFile,
    path: &Path,
    patience: std::time::Duration,
) -> Result<std::fs::File, PersistError> {
    let started = std::time::Instant::now();
    loop {
        match file.persist(path) {
            Ok(file) => return Ok(file),
            Err(error) => {
                // A reader or scanner can temporarily prevent replacement.
                // Keep both the old file and the candidate throughout retries.
                if !matches!(error.error.raw_os_error(), Some(5 | 32 | 33))
                    || started.elapsed() >= patience
                {
                    return Err(error);
                }
                file = error.file;
                std::thread::sleep(
                    std::time::Duration::from_millis(20)
                        .min(patience.saturating_sub(started.elapsed())),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[cfg(windows)]
    fn reader_without_delete_sharing(path: &Path) -> std::fs::File {
        use std::os::windows::fs::OpenOptionsExt as _;
        use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(path)
            .unwrap()
    }

    #[test]
    fn replacement_publishes_the_complete_candidate() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("settings.json");
        std::fs::write(&target, "old").unwrap();
        let mut file = NamedTempFile::new_in(dir.path()).unwrap();
        file.write_all(b"new complete content").unwrap();
        persist_file(file, &target).unwrap();
        assert_eq!(
            std::fs::read_to_string(target).unwrap(),
            "new complete content"
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_temporary_reader_does_not_lose_the_save() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("manifest.json");
        std::fs::write(&target, "old").unwrap();
        let reader = reader_without_delete_sharing(&target);
        let mut file = NamedTempFile::new_in(dir.path()).unwrap();
        file.write_all(b"new").unwrap();
        // First prove that the primitive encounters this host's reader lock.
        let error = file.persist(&target).unwrap_err();
        assert!(matches!(error.error.raw_os_error(), Some(5 | 32 | 33)));
        let (release, released) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || {
            released.recv().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(40));
            drop(reader);
        });
        release.send(()).unwrap();
        persist_file(error.file, &target).unwrap();
        holder.join().unwrap();
        assert_eq!(std::fs::read_to_string(target).unwrap(), "new");
    }

    #[cfg(windows)]
    #[test]
    fn a_persistent_reader_preserves_old_bytes_and_the_candidate() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("manifest.json");
        std::fs::write(&target, "old").unwrap();
        let reader = reader_without_delete_sharing(&target);
        let mut file = NamedTempFile::new_in(dir.path()).unwrap();
        file.write_all(b"new").unwrap();
        let failed =
            persist_until(file, &target, std::time::Duration::from_millis(40)).unwrap_err();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "old");
        assert_eq!(std::fs::read_to_string(failed.file.path()).unwrap(), "new");
        drop(reader);
        persist_file(failed.file, &target).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn an_ordinary_reader_never_loses_the_original_or_candidate() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("manifest.json");
        std::fs::write(&target, "old").unwrap();
        let reader = std::fs::File::open(&target).unwrap();
        let mut file = NamedTempFile::new_in(dir.path()).unwrap();
        file.write_all(b"new").unwrap();
        // Delete sharing permits rename/delete access, but replacing an open
        // destination also depends on the filesystem's rename semantics.
        // Either immediate success or a recoverable refusal must keep data.
        match file.persist(&target) {
            Ok(_) => drop(reader),
            Err(error) => {
                assert!(matches!(error.error.raw_os_error(), Some(5 | 32 | 33)));
                assert_eq!(std::fs::read_to_string(&target).unwrap(), "old");
                assert_eq!(std::fs::read_to_string(error.file.path()).unwrap(), "new");
                drop(reader);
                persist_file(error.file, &target).unwrap();
            }
        }
        assert_eq!(std::fs::read_to_string(target).unwrap(), "new");
    }
}
