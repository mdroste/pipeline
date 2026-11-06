//! Finish local chat deletion even when file cleanup fails or the app exits after SQL commits.
use super::*;

impl Store {
    pub(super) fn queue_session_cleanup(&self, session: &str) -> WorkbenchResult<PathBuf> {
        // A device-local marker survives a crash between the SQL commit and file cleanup.
        let marker_dir = self.root.join("session-cleanup");
        secure_directory(&marker_dir)?;
        let marker = marker_dir.join(session);
        match std::fs::File::create_new(&marker) {
            Ok(file) => crate::safety::sync_file(&file).map_err(|e| {
                WorkbenchError::storage("Could not queue conversation file cleanup", e)
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => {
                return Err(WorkbenchError::storage(
                    "Could not queue conversation file cleanup",
                    e,
                ))
            }
        }
        #[cfg(unix)]
        crate::safety::sync_file(
            &std::fs::File::open(&marker_dir)
                .map_err(|e| WorkbenchError::storage("Could not open cleanup queue", e))?,
        )
        .map_err(|e| WorkbenchError::storage("Could not persist cleanup queue", e))?;
        Ok(marker)
    }

    pub(super) fn cleanup_session_files(
        &self,
        session: &str,
        marker: &Path,
    ) -> WorkbenchResult<()> {
        let profile = crate::workbench::browser::profile_path(session).map_err(|e| {
            WorkbenchError::storage("Cannot locate conversation browser profile", e)
        })?;
        Self::finish_session_cleanup(
            &self.root.join("jobs/conversations").join(session),
            &profile,
            marker,
        )
    }

    fn finish_session_cleanup(
        working: &Path,
        browser: &Path,
        marker: &Path,
    ) -> WorkbenchResult<()> {
        for directory in [working, browser] {
            if let Err(error) = std::fs::remove_dir_all(directory) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    return Err(WorkbenchError::storage("The conversation was deleted. Its local files could not be fully removed. Pipeline will retry on restart", error));
                }
            }
        }
        std::fs::remove_file(marker)
            .map_err(|e| WorkbenchError::storage("Could not finish conversation cleanup", e))
    }

    pub(super) fn retry_session_cleanup(&self) -> WorkbenchResult<()> {
        let markers = match std::fs::read_dir(self.root.join("session-cleanup")) {
            Ok(rows) => rows,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                return Err(WorkbenchError::storage(
                    "Could not read conversation cleanup queue",
                    e,
                ))
            }
        };
        let mut c = self.connection()?;
        // Also waits for any concurrent deletion that already created its marker.
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| WorkbenchError::storage("Could not reopen conversation cleanup", e))?;
        for entry in markers.flatten() {
            let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if validate_id("session id", &id).is_err() {
                continue;
            }
            let deleted: bool = tx
                .query_row(
                    "SELECT NOT EXISTS(SELECT 1 FROM sessions WHERE id=?1) AND EXISTS(SELECT 1 FROM change_log WHERE entity_id=?1 AND action='session_deleted')",
                    [&id],
                    |r| r.get(0),
                )
                .map_err(|e| {
                    WorkbenchError::storage("Could not inspect pending conversation cleanup", e)
                })?;
            if !deleted {
                // The delete transaction rolled back before the process stopped.
                let _ = std::fs::remove_file(entry.path());
            } else if let Err(error) = self.cleanup_session_files(&id, &entry.path()) {
                eprintln!("{}", error.message);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[test]
fn failed_session_file_cleanup_keeps_marker_and_retries_browser_profile() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("pending");
    let working = temp.path().join("work");
    let browser = temp.path().join("profile");
    std::fs::write(&marker, []).unwrap();
    std::fs::write(&working, []).unwrap();
    std::fs::create_dir(&browser).unwrap();
    std::fs::write(browser.join("Cookies"), b"saved").unwrap();
    assert!(Store::finish_session_cleanup(&working, &browser, &marker).is_err());
    assert!(marker.exists());
    std::fs::remove_file(&working).unwrap();
    Store::finish_session_cleanup(&working, &browser, &marker).unwrap();
    assert!(!browser.exists());
    assert!(!marker.exists());
}

#[cfg(test)]
#[test]
fn reopening_a_store_finishes_deletion_after_a_cleanup_failure() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open_at(temp.path()).unwrap();
    let id = s
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "scratch".into(),
            operation_id: "new".into(),
        })
        .unwrap()
        .record
        .id;
    let parent = temp.path().join("jobs/conversations");
    std::fs::create_dir_all(&parent).unwrap();
    let blocked = parent.join(&id);
    std::fs::write(&blocked, b"unexpected file instead of folder").unwrap();
    let result = s
        .delete_session(DeleteSessionRequest {
            session_id: id.clone(),
            operation_id: "delete".into(),
        })
        .unwrap_err();
    assert_eq!(result.code, "session_cleanup_pending");
    assert!(s.session_snapshot(&id).is_err());
    let marker = temp.path().join("session-cleanup").join(&id);
    assert!(marker.exists());
    std::fs::remove_file(blocked).unwrap();
    drop(s);
    Store::open_at(temp.path()).unwrap();
    assert!(!marker.exists());
}
