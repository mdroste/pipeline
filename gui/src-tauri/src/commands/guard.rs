use super::*;

// --- Pipeline ---

pub(super) static CANCEL_FLAG: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
pub(super) static CANCEL_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub(super) static CANCEL_SIGNAL: std::sync::OnceLock<tokio::sync::watch::Sender<u64>> =
    std::sync::OnceLock::new();

/// Guard: true while a pipeline is running. Prevents concurrent runs.
pub(super) static PIPELINE_RUNNING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// RAII guard that cleans up pipeline state on drop (including panics) and
/// holds an OS-level lock so a GUI and CLI process cannot mutate shared run,
/// cache, or retention state concurrently.
pub(super) struct PipelineGuard {
    lock_file: std::fs::File,
}

pub(super) fn acquire_pipeline_guard() -> Result<PipelineGuard, String> {
    use fs2::FileExt as _;
    if PIPELINE_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("A pipeline is already running".into());
    }
    let result = (|| {
        let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
        let dir = home.join(".pipeline");
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create Pipeline data directory: {e}"))?;
        let lock_file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join("run.lock"))
            .map_err(|e| format!("Failed to open the Pipeline run lock: {e}"))?;
        lock_file
            .try_lock_exclusive()
            .map_err(|e| format!("Another Pipeline process is already running a job ({e})"))?;
        Ok(PipelineGuard { lock_file })
    })();
    if result.is_err() {
        PIPELINE_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    }
    result
}

impl Drop for PipelineGuard {
    fn drop(&mut self) {
        kill_all_children();
        PIPELINE_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
        // Stop mirroring the console to disk (also flushes/closes the file).
        crate::pipeline::logging::set_log_sink(None);
        reset_pass_cancels();
        let _ = fs2::FileExt::unlock(&self.lock_file);
    }
}
