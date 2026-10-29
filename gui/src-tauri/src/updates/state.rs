//! Device-local update-check preference and cache in `~/.pipeline/updates.json`.
//!
//! Like `storage.json`, the file is small, read with a size bound, and
//! replaced atomically. It is device policy, so it stays out of
//! `settings.json`, settings bundles, research archives, and Review snapshots.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use super::github::Release;

const FILE_NAME: &str = "updates.json";
const MAX_STATE_BYTES: u64 = 16 * 1024;

/// Serializes read-modify-write cycles within this process.
static LOCK: Mutex<()> = Mutex::new(());

/// Unknown fields are ignored rather than rejected, so a file written by a
/// later version never resets the researcher's preference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct State {
    /// Daily checks in official builds. Manual checks work regardless.
    pub automatic: bool,
    pub last_checked_at: Option<DateTime<Utc>>,
    /// Latest stable release reported by the last successful check.
    pub latest: Option<Release>,
    /// Consecutive failed checks that reached GitHub.
    pub failures: u32,
    /// Automatic checks wait until this time after an unusable response.
    pub retry_after: Option<DateTime<Utc>>,
    /// No check, automatic or manual, is sent before this time.
    pub rate_limited_until: Option<DateTime<Utc>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            automatic: true,
            last_checked_at: None,
            latest: None,
            failures: 0,
            retry_after: None,
            rate_limited_until: None,
        }
    }
}

fn path() -> Result<PathBuf, String> {
    Ok(crate::storage::local_root()?.join(FILE_NAME))
}

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|error| error.into_inner())
}

pub(super) fn load() -> Result<State, String> {
    let _guard = lock();
    read(&path()?)
}

/// Apply `change` to the saved state. An unreadable file is left untouched,
/// so damage never silently turns automatic checks back on.
pub(super) fn update(change: impl FnOnce(&mut State)) -> Result<State, String> {
    let _guard = lock();
    let path = path()?;
    let mut state = read(&path)?;
    change(&mut state);
    write(&path, &state)?;
    Ok(state)
}

/// Save the researcher's choice, replacing an unreadable file.
pub(super) fn set_automatic(enabled: bool) -> Result<State, String> {
    let _guard = lock();
    let path = path()?;
    let mut state = read(&path).unwrap_or_default();
    state.automatic = enabled;
    write(&path, &state)?;
    Ok(state)
}

fn read(path: &Path) -> Result<State, String> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(State::default()),
        Err(e) => return Err(format!("Cannot read update settings: {e}")),
        Ok(_) => {}
    }
    let file = crate::safety::open_regular_file(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_STATE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read update settings: {e}"))?;
    if bytes.len() as u64 > MAX_STATE_BYTES {
        return Err("Update settings exceed the size limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("Cannot read update settings: {e}"))
}

fn write(path: &Path, state: &State) -> Result<(), String> {
    let directory = path
        .parent()
        .ok_or("Update settings have no parent folder")?;
    std::fs::create_dir_all(directory)
        .map_err(|e| format!("Cannot create local settings folder: {e}"))?;
    let mut file = tempfile::NamedTempFile::new_in(directory)
        .map_err(|e| format!("Cannot save update settings: {e}"))?;
    serde_json::to_writer_pretty(&mut file, state).map_err(|e| e.to_string())?;
    file.write_all(b"\n").map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path)
        .map_err(|e| format!("Cannot save update settings: {}", e.error))?;
    #[cfg(unix)]
    std::fs::File::open(directory)
        .and_then(|dir| dir.sync_all())
        .map_err(|e| format!("Cannot sync update settings: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample() -> State {
        State {
            automatic: false,
            last_checked_at: Some(Utc.with_ymd_and_hms(2026, 10, 28, 12, 0, 0).unwrap()),
            latest: Some(Release {
                version: "1.2.0".into(),
                name: "Pipeline v1.2.0".into(),
                release_url: "https://github.com/mdroste/pipeline/releases/tag/v1.2.0".into(),
                download_url: None,
                published_at: Some("2026-10-27T18:00:00Z".into()),
            }),
            failures: 2,
            retry_after: None,
            rate_limited_until: None,
        }
    }

    #[test]
    fn missing_file_enables_automatic_checks() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read(&dir.path().join(FILE_NAME)).unwrap(), State::default());
        assert!(State::default().automatic);
    }

    #[test]
    fn state_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        write(&path, &sample()).unwrap();
        assert_eq!(read(&path).unwrap(), sample());
    }

    #[test]
    fn partial_and_newer_files_keep_the_preference() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        std::fs::write(&path, r#"{"automatic": false, "future_field": 1}"#).unwrap();
        let state = read(&path).unwrap();
        assert!(!state.automatic);
        assert_eq!(state.last_checked_at, None);
    }

    #[test]
    fn corrupt_or_oversize_files_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        std::fs::write(&path, "{unfinished").unwrap();
        assert!(read(&path).is_err());
        std::fs::write(&path, " ".repeat(MAX_STATE_BYTES as usize + 1)).unwrap();
        assert!(read(&path).unwrap_err().contains("size limit"));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_file_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("elsewhere.json");
        write(&target, &sample()).unwrap();
        let path = dir.path().join(FILE_NAME);
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(read(&path).is_err());
    }
}
