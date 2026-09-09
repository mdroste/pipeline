use super::*;

/// Retain each corrupt version under a unique name, serialized with writers.
/// Recheck the exact bytes under the lock so a concurrent repair is never moved.
pub(super) fn quarantine_corrupt_file(path: &std::path::Path, expected: &str) -> Option<PathBuf> {
    let result = (|| -> Result<Option<PathBuf>, String> {
        use std::io::Write as _;
        let (_process_guard, _lock_file) = acquire_settings_write_lock(path)?;
        if read_settings_file(path)? != expected {
            return Ok(None);
        }
        let mut backup = tempfile::Builder::new()
            .prefix("settings.corrupt-")
            .tempfile_in(path.parent().ok_or("Settings path has no parent")?)
            .map_err(|e| e.to_string())?;
        backup
            .write_all(expected.as_bytes())
            .map_err(|e| e.to_string())?;
        backup.as_file().sync_all().map_err(|e| e.to_string())?;
        let (_, backup_path) = backup.keep().map_err(|e| e.to_string())?;
        fs::remove_file(path).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        fs::File::open(path.parent().unwrap())
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(Some(backup_path))
    })();
    match result {
        Ok(path) => path,
        Err(e) => {
            eprintln!("WARNING: could not quarantine corrupt settings: {e}");
            None
        }
    }
}
