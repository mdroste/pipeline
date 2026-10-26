// --- Pipeline ---

/// In-process view of the OS run lock. Queued review runs share it, so several
/// can execute at once; batches, automations, and history maintenance take it
/// exclusively. The OS file lock is held while either mode is active, so a GUI
/// and CLI process still never mutate shared run, cache, or retention state
/// concurrently.
struct RunLock {
    shared: usize,
    exclusive: bool,
    file: Option<std::fs::File>,
}

static RUN_LOCK: std::sync::Mutex<RunLock> = std::sync::Mutex::new(RunLock {
    shared: 0,
    exclusive: false,
    file: None,
});

fn run_lock() -> std::sync::MutexGuard<'static, RunLock> {
    RUN_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// True while this process holds the run lock in either mode. Run recovery
/// consults it so a History refresh never rewrites a run this process is
/// executing.
pub fn pipeline_is_running() -> bool {
    let lock = run_lock();
    lock.exclusive || lock.shared > 0
}

fn lock_run_file() -> Result<std::fs::File, String> {
    use fs2::FileExt as _;
    let file = crate::runs::open_run_lock(&crate::runs::run_lock_path()?)?;
    file.try_lock_exclusive()
        .map_err(|e| format!("Another Pipeline process is already running a job ({e})"))?;
    Ok(file)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GuardMode {
    Shared,
    Exclusive,
}

impl RunLock {
    fn enter(
        &mut self,
        mode: GuardMode,
        open: impl FnOnce() -> Result<std::fs::File, String>,
    ) -> Result<(), String> {
        match mode {
            GuardMode::Exclusive if self.exclusive || self.shared > 0 => {
                return Err("A pipeline is already running".into());
            }
            GuardMode::Shared if self.exclusive => {
                return Err("A batch, automation, or history operation is running".into());
            }
            _ => {}
        }
        if self.file.is_none() {
            self.file = Some(open()?);
        }
        match mode {
            GuardMode::Exclusive => self.exclusive = true,
            GuardMode::Shared => self.shared += 1,
        }
        Ok(())
    }

    fn leave(&mut self, mode: GuardMode) {
        match mode {
            GuardMode::Exclusive => self.exclusive = false,
            GuardMode::Shared => self.shared = self.shared.saturating_sub(1),
        }
        if !self.exclusive && self.shared == 0 {
            if let Some(file) = self.file.take() {
                let _ = fs2::FileExt::unlock(&file);
            }
        }
    }
}

/// RAII guard that cleans up the current run scope on drop (including panics)
/// and releases its hold on the run lock.
pub(super) struct PipelineGuard {
    mode: GuardMode,
}

/// Take the run lock exclusively: no review run or other exclusive job may be
/// active, and none can start until this guard drops.
pub(super) fn acquire_pipeline_guard() -> Result<PipelineGuard, String> {
    run_lock().enter(GuardMode::Exclusive, lock_run_file)?;
    Ok(PipelineGuard {
        mode: GuardMode::Exclusive,
    })
}

/// Take one shared run slot. Any number of slots coexist; they wait only for
/// exclusive jobs (batches, automations, history maintenance).
pub(super) fn acquire_run_slot() -> Result<PipelineGuard, String> {
    run_lock().enter(GuardMode::Shared, lock_run_file)?;
    Ok(PipelineGuard {
        mode: GuardMode::Shared,
    })
}

impl Drop for PipelineGuard {
    fn drop(&mut self) {
        let scope = crate::run_scope::current();
        scope.kill_children();
        // Stop mirroring the console to disk (also flushes/closes the file).
        scope.set_log_sink(None);
        scope.reset_pass_cancels();
        run_lock().leave(self.mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_slots_coexist_and_exclude_exclusive_jobs() {
        use fs2::FileExt as _;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run.lock");
        let open = || {
            let file = crate::runs::open_run_lock(&path)?;
            file.try_lock_exclusive().map_err(|e| e.to_string())?;
            Ok(file)
        };
        let mut lock = RunLock {
            shared: 0,
            exclusive: false,
            file: None,
        };
        lock.enter(GuardMode::Shared, open).unwrap();
        lock.enter(GuardMode::Shared, open).unwrap();
        assert!(lock.enter(GuardMode::Exclusive, open).is_err());
        // Another process sees the OS lock while any slot is held.
        let other = crate::runs::open_run_lock(&path).unwrap();
        assert!(other.try_lock_exclusive().is_err());
        lock.leave(GuardMode::Shared);
        assert!(lock.enter(GuardMode::Exclusive, open).is_err());
        lock.leave(GuardMode::Shared);
        assert!(lock.file.is_none());
        lock.enter(GuardMode::Exclusive, open).unwrap();
        assert!(lock.enter(GuardMode::Shared, open).is_err());
        lock.leave(GuardMode::Exclusive);
        other.try_lock_exclusive().unwrap();
    }
}
