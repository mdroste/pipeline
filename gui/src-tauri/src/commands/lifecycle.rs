use super::*;

/// Per-run cleanup. A batch holds `PipelineGuard` across several jobs, so each
/// job must close its own log sink and API read/write windows on every return
/// path, including extraction or provider failures.
pub(super) struct RunStateGuard;

pub(super) fn begin_run_state() -> RunStateGuard {
    CANCEL_FLAG.store(false, std::sync::atomic::Ordering::Release);
    reset_pass_cancels();
    crate::pipeline::logging::reset_run_usage();
    crate::pipeline::logging::set_log_sink(None);
    crate::pipeline::api_common::reset_write_budget();
    RunStateGuard
}

impl Drop for RunStateGuard {
    fn drop(&mut self) {
        crate::pipeline::logging::set_log_sink(None);
        reset_pass_cancels();
    }
}

/// PIDs of active child processes (claude/codex/agy subprocesses).
/// Populated by `register_child_pid`, cleared by `unregister_child_pid`.
pub(super) static CHILD_PIDS: std::sync::Mutex<Vec<u32>> = std::sync::Mutex::new(Vec::new());

#[cfg(windows)]
pub(super) static CHILD_JOBS: std::sync::Mutex<Vec<(u32, usize)>> =
    std::sync::Mutex::new(Vec::new());

/// Minimal Win32 Job Object bindings. Keeping them here avoids making the
/// platform-neutral build depend on a large Windows bindings crate.
#[cfg(windows)]
mod windows_job {
    use std::ffi::c_void;

    type Handle = *mut c_void;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS: i32 = 9;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
    const PROCESS_TERMINATE: u32 = 0x0001;
    const PROCESS_SET_QUOTA: u32 = 0x0100;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    #[repr(C)]
    #[derive(Default)]
    struct BasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimitInformation {
        basic_limit_information: BasicLimitInformation,
        io_info: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> Handle;
        fn SetInformationJobObject(
            job: Handle,
            info_class: i32,
            info: *const c_void,
            length: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        fn TerminateJobObject(job: Handle, exit_code: u32) -> i32;
        fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> Handle;
        fn CloseHandle(handle: Handle) -> i32;
    }

    pub fn assign(process_id: u32) -> Option<usize> {
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return None;
            }
            let mut info = ExtendedLimitInformation::default();
            info.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS,
                (&info as *const ExtendedLimitInformation).cast(),
                std::mem::size_of::<ExtendedLimitInformation>() as u32,
            ) == 0
            {
                CloseHandle(job);
                return None;
            }
            let process = OpenProcess(
                PROCESS_TERMINATE | PROCESS_SET_QUOTA | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                process_id,
            );
            if process.is_null() || AssignProcessToJobObject(job, process) == 0 {
                if !process.is_null() {
                    CloseHandle(process);
                }
                CloseHandle(job);
                return None;
            }
            CloseHandle(process);
            Some(job as usize)
        }
    }

    pub fn terminate(handle: usize) {
        unsafe {
            let job = handle as Handle;
            TerminateJobObject(job, 1);
            CloseHandle(job);
        }
    }

    pub fn close(handle: usize) {
        unsafe {
            CloseHandle(handle as Handle);
        }
    }
}

#[cfg(windows)]
pub(crate) fn register_process_job(pid: u32) {
    if let Some(handle) = windows_job::assign(pid) {
        let mut jobs = CHILD_JOBS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, old)) = jobs
            .iter()
            .find(|(registered, _)| *registered == pid)
            .copied()
        {
            windows_job::close(old);
            jobs.retain(|(registered, _)| *registered != pid);
        }
        jobs.push((pid, handle));
    }
}

#[cfg(not(windows))]
pub(crate) fn register_process_job(_pid: u32) {}

#[cfg(windows)]
pub(crate) fn unregister_process_job(pid: u32) {
    let mut jobs = CHILD_JOBS.lock().unwrap_or_else(|e| e.into_inner());
    let handles: Vec<usize> = jobs
        .iter()
        .filter(|(registered, _)| *registered == pid)
        .map(|(_, handle)| *handle)
        .collect();
    jobs.retain(|(registered, _)| *registered != pid);
    drop(jobs);
    for handle in handles {
        windows_job::close(handle);
    }
}

#[cfg(not(windows))]
pub(crate) fn unregister_process_job(_pid: u32) {}

/// (pass key, pid) for per-pass cancellation. A pass key is the step key
/// ("technical/claude") the executor tags each `call_llm` with.
pub(super) static PASS_PIDS: std::sync::Mutex<Vec<(String, u32)>> =
    std::sync::Mutex::new(Vec::new());
/// Pass keys the user asked to cancel this run. Checked by the executor's retry
/// loop so a cancelled pass fails instead of retrying.
pub(super) static CANCELLED_PASSES: std::sync::Mutex<Vec<String>> =
    std::sync::Mutex::new(Vec::new());

/// Whether a specific pass was cancelled by the user.
pub fn is_pass_cancelled(pass_key: &str) -> bool {
    CANCELLED_PASSES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .any(|k| k == pass_key)
}

/// Clear per-pass cancellation state (called at run start/end).
pub(super) fn reset_pass_cancels() {
    CANCELLED_PASSES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
    PASS_PIDS.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

// ── Batch queue (1.3.1) ─────────────────────────────────────────────
//
// A batch is a list of input paths run one at a time through the normal
// pipeline (parallelism stays *inside* a run). The worker holds the same
// PIPELINE_RUNNING guard as a single run, so the two can never overlap. Job
// state is published to the frontend via `batch:progress` events and pollable
// via `get_batch_status`.

/// One job in a batch. Serialized to the frontend as-is.
#[derive(Clone, serde::Serialize)]
pub struct BatchJob {
    pub path: String,
    pub name: String,
    /// "pending" | "running" | "done" | "failed" | "cancelled".
    pub status: String,
    pub run_id: Option<String>,
    pub error: Option<String>,
    pub duration_secs: u64,
    /// Immutable profile used by this batch job.
    pub profile_id: String,
    /// Content fingerprint shared by every job in one batch.
    pub profile_snapshot_id: String,
}

pub(super) static BATCH: std::sync::Mutex<Vec<BatchJob>> = std::sync::Mutex::new(Vec::new());
pub(super) static BATCH_CANCEL: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub(super) fn basename(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

pub(super) fn extra_input_artifact_path(index: usize, key: &str) -> String {
    let slug: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!(
        "context/inputs/{index:02}_{}.txt",
        if slug.is_empty() { "input" } else { &slug }
    )
}

pub(super) fn emit_batch(app: &crate::emit::EventBus) {
    let jobs = BATCH.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let _ = app.emit_event(
        "batch:progress",
        serde_json::to_value(&jobs).unwrap_or_default(),
    );
}

pub fn is_cancelled() -> bool {
    CANCEL_FLAG.load(std::sync::atomic::Ordering::Acquire)
}

/// Recognize only the cancellation errors Pipeline itself creates. Provider
/// messages are untrusted prose and may legitimately contain words such as
/// "cancelled" (for example, a cancelled subscription); those remain ordinary
/// failures and must not bypass retries or acquire shell interrupt status.
pub fn is_pipeline_cancellation_error(error: &str) -> bool {
    if error == "Pipeline cancelled" || error.starts_with("Pipeline cancelled during ") {
        return true;
    }
    error
        .strip_prefix("Pass '")
        .and_then(|value| value.strip_suffix("' cancelled"))
        .is_some_and(|pass_key| {
            !pass_key.is_empty() && !pass_key.contains('\n') && !pass_key.contains('\r')
        })
}

pub(super) fn cancellation_signal() -> &'static tokio::sync::watch::Sender<u64> {
    CANCEL_SIGNAL.get_or_init(|| tokio::sync::watch::channel(0).0)
}

pub(super) fn signal_cancellation() {
    let epoch = CANCEL_EPOCH.fetch_add(1, std::sync::atomic::Ordering::AcqRel) + 1;
    cancellation_signal().send_replace(epoch);
}

/// Snapshot of the cancellation epoch, captured at run-command entry and
/// compared after `begin_run_state` so a cancel that lands during the launch
/// command's preflight (dependency probes, resumable-run recovery) is
/// re-asserted instead of being erased by the flag reset.
pub(super) fn current_cancel_epoch() -> u64 {
    CANCEL_EPOCH.load(std::sync::atomic::Ordering::Acquire)
}

/// Resolve when the current run or the supplied pass is cancelled. A watch
/// channel avoids lost wakeups if cancellation races with request dispatch.
pub async fn wait_for_cancellation(pass_key: Option<&str>) {
    let mut signal = cancellation_signal().subscribe();
    loop {
        if is_cancelled() || pass_key.is_some_and(is_pass_cancelled) {
            return;
        }
        if signal.changed().await.is_err() {
            return;
        }
    }
}

/// Poll an operation and the run/pass cancellation signal together without
/// requiring Tokio's optional macro feature. Dropping the operation future is
/// safe only for operations whose resources have their own RAII cleanup.
pub async fn await_or_cancel<F, T>(future: F, pass_key: Option<&str>) -> Result<T, String>
where
    F: std::future::Future<Output = T>,
{
    use std::future::Future as _;
    let mut operation = std::pin::pin!(future);
    let mut cancellation = std::pin::pin!(wait_for_cancellation(pass_key));
    std::future::poll_fn(|cx| {
        if let std::task::Poll::Ready(value) = operation.as_mut().poll(cx) {
            return std::task::Poll::Ready(Ok(value));
        }
        if cancellation.as_mut().poll(cx).is_ready() {
            return std::task::Poll::Ready(Err(match pass_key {
                Some(key) => format!("Pass '{key}' cancelled"),
                None => "Pipeline cancelled".to_string(),
            }));
        }
        std::task::Poll::Pending
    })
    .await
}

/// Register a child process PID so it can be killed on cancel. Also attributes
/// the PID to the current pass (if any) for per-pass cancellation.
pub fn register_child_pid(pid: u32) {
    // Callers already skip pid 0 (spawn without a real id), but guard here
    // too so kill_all_children never signals pid 0 / process group 0.
    if pid == 0 {
        return;
    }
    CHILD_PIDS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(pid);
    register_process_job(pid);
    if let Some(pass) = crate::pipeline::logging::current_pass() {
        PASS_PIDS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((pass.clone(), pid));
        // Close the race where cancel_pass observes no PID immediately before
        // this child is registered. A cancelled pass may never acquire a new
        // live subprocess after the cancellation decision has committed.
        if is_pass_cancelled(&pass) {
            kill_process(pid);
        }
    }
}

/// Unregister a child process PID after it exits.
pub fn unregister_child_pid(pid: u32) {
    CHILD_PIDS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|&p| p != pid);
    PASS_PIDS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|(_, p)| *p != pid);
    unregister_process_job(pid);
}

/// Register a managed-engine installer child for Windows job-object tracking
/// only. Installs run outside any pipeline run and own their kill path
/// (`engines::cancel_install`); they must never enter `CHILD_PIDS`, where the
/// end or cancellation of an unrelated run would SIGKILL a half-finished
/// multi-gigabyte install mid-download.
pub(crate) fn register_engine_child_pid(pid: u32) {
    if pid == 0 {
        return;
    }
    register_process_job(pid);
}

/// Counterpart to `register_engine_child_pid` once the installer child exits.
pub(crate) fn unregister_engine_child_pid(pid: u32) {
    unregister_process_job(pid);
}

/// Register an independently owned long-lived child for Windows job-object
/// tracking without adding it to Pipeline's run cancellation registry.
pub(crate) fn register_independent_child_pid(pid: u32) {
    if pid > 0 {
        register_process_job(pid);
    }
}

/// Release independent process-tree tracking after the owner has reaped it.
pub(crate) fn unregister_independent_child_pid(pid: u32) {
    if pid > 0 {
        unregister_process_job(pid);
    }
}

/// Force-stop an independently owned process tree. It is deliberately absent
/// from `CHILD_PIDS`, so ordinary Pipeline cancellation cannot reach it.
pub(crate) fn kill_independent_process(pid: u32) {
    kill_process(pid);
}

/// Cancel one pass: mark it cancelled (so it won't retry) and kill its
/// subprocesses. Other passes in the wave keep running.
#[tauri::command]
pub async fn cancel_pass(pass_key: String) -> Result<(), String> {
    CANCELLED_PASSES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(pass_key.clone());
    signal_cancellation();
    kill_pass_children(&pass_key);
    Ok(())
}

/// Terminate subprocesses attributed to one pass without marking the pass as
/// user-cancelled. Used when the whole logical provider call reaches its
/// deadline, so retry policy can still decide what happens next.
pub(crate) fn kill_pass_children(pass_key: &str) {
    let pids: Vec<u32> = PASS_PIDS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter(|(k, _)| k == pass_key)
        .map(|(_, p)| *p)
        .collect();
    for pid in &pids {
        kill_process(*pid);
    }
}

/// Kill all registered child processes.
pub(crate) fn kill_all_children() {
    let pids = std::mem::take(&mut *CHILD_PIDS.lock().unwrap_or_else(|e| e.into_inner()));
    for pid in pids {
        kill_process(pid);
    }
}

pub(crate) fn kill_process(pid: u32) {
    if pid == 0 {
        return;
    }
    #[cfg(unix)]
    {
        // Validate PID fits in a positive i32 to prevent overflow when negating
        if pid > i32::MAX as u32 {
            return;
        }
        let pid_i32 = pid as i32;
        // Cancellation is a force-stop operation. Signal immediately while
        // the PID is still registered as owned by Pipeline; never schedule a
        // later PID-only escalation, because the child may be reaped and that
        // numeric PID/process-group ID reused by an unrelated process.
        unsafe {
            libc::kill(-pid_i32, libc::SIGKILL);
            libc::kill(pid_i32, libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    {
        let job = {
            let mut jobs = CHILD_JOBS.lock().unwrap_or_else(|e| e.into_inner());
            let handle = jobs
                .iter()
                .find(|(registered, _)| *registered == pid)
                .map(|(_, handle)| *handle);
            jobs.retain(|(registered, _)| *registered != pid);
            handle
        };
        if let Some(handle) = job {
            windows_job::terminate(handle);
            return;
        }
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .creation_flags(0x08000000)
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::is_pipeline_cancellation_error;

    #[test]
    fn cancellation_errors_use_pipeline_owned_messages_only() {
        assert!(is_pipeline_cancellation_error("Pipeline cancelled"));
        assert!(is_pipeline_cancellation_error(
            "Pipeline cancelled during fan-out discovery"
        ));
        assert!(is_pipeline_cancellation_error(
            "Pass 'technical/claude' cancelled"
        ));

        assert!(!is_pipeline_cancellation_error(
            "Provider request cancelled by upstream"
        ));
        assert!(!is_pipeline_cancellation_error(
            "Pipeline task was cancelled unexpectedly"
        ));
        assert!(!is_pipeline_cancellation_error("Pass '' cancelled"));
    }
}
