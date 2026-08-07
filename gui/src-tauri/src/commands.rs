//! Tauri command handlers.

use crate::models::PipelineReport;
use crate::models::ReportSummary;
use crate::output;
use crate::pipeline::{executor, extract, orient, reconcile};
use crate::pipeline_config::{self, PipelineConfig, ProfileSummary};
use crate::storage;
use std::io::{Read, Write};
use tauri::AppHandle;

/// Maximum file size for imported configs (10 MB). Pipeline configs are
/// small JSON; anything larger is almost certainly the wrong file.
const MAX_IMPORT_SIZE: u64 = 10_000_000;
const MAX_IMPORT_REDIRECTS: usize = 5;
const MAX_RUN_CONTEXT_SIZE: u64 = 64 * 1024 * 1024;

/// Read a file for import, rejecting files above the size limit.
fn read_import_file(path: &str) -> Result<String, String> {
    let file = crate::safety::open_regular_file(std::path::Path::new(path))
        .map_err(|e| format!("Failed to read {path}: {e}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_IMPORT_SIZE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read {path}: {e}"))?;
    if bytes.len() as u64 > MAX_IMPORT_SIZE {
        return Err(format!(
            "File is too large. Import files should be under {} MB.",
            MAX_IMPORT_SIZE / 1_000_000,
        ));
    }
    String::from_utf8(bytes).map_err(|e| format!("Import file is not valid UTF-8: {e}"))
}

fn append_limited(buffer: &mut Vec<u8>, chunk: &[u8], limit: usize) -> Result<(), String> {
    if chunk.len() > limit.saturating_sub(buffer.len()) {
        return Err("The fetched file is too large to be a profile.".to_string());
    }
    buffer.extend_from_slice(chunk);
    Ok(())
}

async fn read_response_limited(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err("The fetched file is too large to be a profile.".to_string());
    }
    let mut bytes =
        Vec::with_capacity(response.content_length().unwrap_or(0).min(limit as u64) as usize);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Fetch failed: {e}"))?
    {
        append_limited(&mut bytes, &chunk, limit)?;
    }
    Ok(bytes)
}

fn import_ip_is_public(address: std::net::IpAddr) -> bool {
    match address {
        std::net::IpAddr::V4(address) => {
            let [first, second, third, _] = address.octets();
            !(address.is_unspecified()
                || address.is_loopback()
                || address.is_private()
                || address.is_link_local()
                || address.is_multicast()
                || address.is_broadcast()
                || first == 0
                || (first == 100 && (64..=127).contains(&second))
                || (first == 192 && second == 0 && third == 0)
                || (first == 192 && second == 0 && third == 2)
                || (first == 192 && second == 88 && third == 99)
                || (first == 198 && (second == 18 || second == 19))
                || (first == 198 && second == 51 && third == 100)
                || (first == 203 && second == 0 && third == 113)
                || first >= 240)
        }
        std::net::IpAddr::V6(address) => {
            if let Some(mapped) = address.to_ipv4_mapped() {
                return import_ip_is_public(std::net::IpAddr::V4(mapped));
            }
            let segments = address.segments();
            // Global IPv6 unicast is 2000::/3. Documentation and 6to4
            // addresses are excluded as well; neither is needed for profile
            // sharing and both complicate destination validation.
            segments[0] & 0xe000 == 0x2000
                && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
                && segments[0] != 0x2002
        }
    }
}

fn validate_import_url_shape(mut url: reqwest::Url) -> Result<reqwest::Url, String> {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
    {
        return Err("Profile URL must use HTTPS and cannot contain credentials".to_string());
    }
    let original_host = url.host_str().unwrap_or_default().to_string();
    let host = original_host.trim_end_matches('.').to_string();
    if host.eq_ignore_ascii_case("localhost")
        || host
            .to_ascii_lowercase()
            .strip_suffix(".localhost")
            .is_some()
    {
        return Err("Profile URLs cannot target localhost or private networks".to_string());
    }
    if host != original_host {
        url.set_host(Some(&host))
            .map_err(|_| "Profile URL contains an invalid host".to_string())?;
    }
    url.set_fragment(None);
    Ok(url)
}

async fn public_import_addresses(
    url: &reqwest::Url,
) -> Result<(String, Vec<std::net::SocketAddr>, bool), String> {
    let host = url
        .host_str()
        .ok_or("Profile URL is missing a host")?
        .trim_end_matches('.')
        .to_string();
    let port = url
        .port_or_known_default()
        .ok_or("Profile URL has no usable port")?;
    let literal = host.parse::<std::net::IpAddr>().ok();
    let mut addresses = if let Some(address) = literal {
        vec![std::net::SocketAddr::new(address, port)]
    } else {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::net::lookup_host((host.as_str(), port)),
        )
        .await
        .map_err(|_| "Profile URL DNS lookup timed out".to_string())?
        .map_err(|error| format!("Profile URL DNS lookup failed: {error}"))?
        .collect::<Vec<_>>()
    };
    addresses.sort_unstable();
    addresses.dedup();
    if addresses.is_empty() {
        return Err("Profile URL host did not resolve to an address".to_string());
    }
    if addresses
        .iter()
        .any(|address| !import_ip_is_public(address.ip()))
    {
        return Err("Profile URLs cannot target localhost or private networks".to_string());
    }
    Ok((host, addresses, literal.is_none()))
}

async fn fetch_public_profile_url(url: &str) -> Result<reqwest::Response, String> {
    let mut current = validate_import_url_shape(
        reqwest::Url::parse(url).map_err(|error| format!("Invalid profile URL: {error}"))?,
    )?;
    for redirect_count in 0..=MAX_IMPORT_REDIRECTS {
        let (host, addresses, pin_dns) = public_import_addresses(&current).await?;
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(15));
        if pin_dns {
            builder = builder.resolve_to_addrs(&host, &addresses);
        }
        let response = builder
            .build()
            .map_err(|error| format!("HTTP client error: {error}"))?
            .get(current.clone())
            .header("User-Agent", "pipeline")
            .send()
            .await
            .map_err(|error| format!("Fetch failed: {error}"))?;
        if !response.status().is_redirection() {
            return Ok(response);
        }
        if redirect_count == MAX_IMPORT_REDIRECTS {
            return Err(format!(
                "Profile URL exceeded the {MAX_IMPORT_REDIRECTS}-redirect safety limit"
            ));
        }
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .ok_or("Profile URL redirect omitted the Location header")?
            .to_str()
            .map_err(|_| "Profile URL redirect Location is not valid text")?;
        let next = validate_import_url_shape(
            current
                .join(location)
                .map_err(|error| format!("Invalid profile URL redirect: {error}"))?,
        )?;
        if current.scheme() == "https" && next.scheme() != "https" {
            return Err("Profile URL redirects cannot downgrade HTTPS to HTTP".to_string());
        }
        current = next;
    }
    unreachable!("redirect loop always returns or advances within its fixed bound")
}

// --- Pipeline ---

static CANCEL_FLAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static CANCEL_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static CANCEL_SIGNAL: std::sync::OnceLock<tokio::sync::watch::Sender<u64>> =
    std::sync::OnceLock::new();

/// Guard: true while a pipeline is running. Prevents concurrent runs.
static PIPELINE_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// RAII guard that cleans up pipeline state on drop (including panics) and
/// holds an OS-level lock so a GUI and CLI process cannot mutate shared run,
/// cache, or retention state concurrently.
struct PipelineGuard {
    lock_file: std::fs::File,
}

fn acquire_pipeline_guard() -> Result<PipelineGuard, String> {
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

/// Foreground GUI/CLI runs execute behind a scheduler boundary just like
/// provider calls. Keeping the join handle owned prevents a dropped Tauri
/// invocation or CLI waiter from detaching a run that still holds the global
/// pipeline lock and child processes.
struct PipelineTask {
    handle: Option<tokio::task::JoinHandle<std::result::Result<serde_json::Value, String>>>,
}

impl PipelineTask {
    fn spawn_future(
        future: impl std::future::Future<Output = std::result::Result<serde_json::Value, String>>
            + Send
            + 'static,
    ) -> Self {
        Self {
            handle: Some(tokio::spawn(future)),
        }
    }

    fn spawn(
        guard: PipelineGuard,
        bus: crate::emit::EventBus,
        paper_path: String,
        diff: bool,
        variables: std::collections::HashMap<String, String>,
        extra_inputs: std::collections::HashMap<String, String>,
        snapshot: Option<RunSnapshot>,
    ) -> Self {
        Self::spawn_future(async move {
            let _guard = guard;
            run_pipeline_inner_with_snapshot(
                &bus,
                &paper_path,
                diff,
                variables,
                extra_inputs,
                snapshot,
            )
            .await
        })
    }

    async fn join(mut self) -> Result<serde_json::Value, String> {
        // Retain ownership while awaiting so Drop can abort instead of detach
        // if the command/CLI waiter itself is cancelled.
        let joined = self
            .handle
            .as_mut()
            .expect("pipeline task handle missing")
            .await;
        let _ = self.handle.take();
        joined.map_err(|error| {
            if error.is_panic() {
                format!("Pipeline task panicked: {error}")
            } else {
                format!("Pipeline task was cancelled unexpectedly: {error}")
            }
        })?
    }
}

impl Drop for PipelineTask {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            CANCEL_FLAG.store(true, std::sync::atomic::Ordering::Release);
            signal_cancellation();
            kill_all_children();
            handle.abort();
        }
    }
}

/// Per-run cleanup. A batch holds `PipelineGuard` across several jobs, so each
/// job must close its own log sink and API read/write windows on every return
/// path, including extraction or provider failures.
struct RunStateGuard;

fn begin_run_state() -> RunStateGuard {
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

/// PIDs of active child processes (claude/gemini/codex subprocesses).
/// Populated by `register_child_pid`, cleared by `unregister_child_pid`.
static CHILD_PIDS: std::sync::Mutex<Vec<u32>> = std::sync::Mutex::new(Vec::new());

#[cfg(windows)]
static CHILD_JOBS: std::sync::Mutex<Vec<(u32, usize)>> = std::sync::Mutex::new(Vec::new());

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
static PASS_PIDS: std::sync::Mutex<Vec<(String, u32)>> = std::sync::Mutex::new(Vec::new());
/// Pass keys the user asked to cancel this run. Checked by the executor's retry
/// loop so a cancelled pass fails instead of retrying.
static CANCELLED_PASSES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Whether a specific pass was cancelled by the user.
pub fn is_pass_cancelled(pass_key: &str) -> bool {
    CANCELLED_PASSES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .any(|k| k == pass_key)
}

/// Clear per-pass cancellation state (called at run start/end).
fn reset_pass_cancels() {
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
    /// Immutable profile used by this batch or watch-folder job.
    pub profile_id: String,
    /// Content fingerprint shared by every job in one batch.
    pub profile_snapshot_id: String,
}

static BATCH: std::sync::Mutex<Vec<BatchJob>> = std::sync::Mutex::new(Vec::new());
static BATCH_CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// ── Watch folder (1.3.4) ────────────────────────────────────────────
//
// A background poll of a folder: files that appear after watching starts are
// run through the pipeline one at a time with the active profile. Dependency-
// free (no file-watcher crate) — a 5s poll is plenty for this use case.

#[derive(Clone, Default, serde::Serialize)]
pub struct WatchStatus {
    pub active: bool,
    /// True while another foreground/batch run owns the global run guard.
    pub paused: bool,
    /// Terminal watcher failure, such as the folder becoming unreadable.
    pub error: Option<String>,
    pub folder: String,
    /// Immutable active profile captured when this watcher started.
    pub profile_id: String,
    /// Fingerprint of the captured profile, settings, variables, and inputs.
    pub profile_snapshot_id: String,
    /// Files processed since watching started (newest last).
    pub processed: Vec<BatchJob>,
    /// Aggregate counts remain accurate when the detailed history is pruned.
    pub processed_total: u64,
    pub failed_total: u64,
}

/// Each start/stop advances the generation. An old task can never observe a
/// later watcher's `active=true` state and revive itself.
static WATCH_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
const MAX_WATCH_HISTORY: usize = 200;
static WATCH_STATE: std::sync::Mutex<WatchStatus> = std::sync::Mutex::new(WatchStatus {
    active: false,
    paused: false,
    error: None,
    folder: String::new(),
    profile_id: String::new(),
    profile_snapshot_id: String::new(),
    processed: Vec::new(),
    processed_total: 0,
    failed_total: 0,
});

fn basename(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn extra_input_artifact_path(index: usize, key: &str) -> String {
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

fn emit_batch(app: &crate::emit::EventBus) {
    let jobs = BATCH.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let _ = app.emit_event(
        "batch:progress",
        serde_json::to_value(&jobs).unwrap_or_default(),
    );
}

pub fn is_cancelled() -> bool {
    CANCEL_FLAG.load(std::sync::atomic::Ordering::Acquire)
}

fn cancellation_signal() -> &'static tokio::sync::watch::Sender<u64> {
    CANCEL_SIGNAL.get_or_init(|| tokio::sync::watch::channel(0).0)
}

fn signal_cancellation() {
    let epoch = CANCEL_EPOCH.fetch_add(1, std::sync::atomic::Ordering::AcqRel) + 1;
    cancellation_signal().send_replace(epoch);
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

/// Create the optional persistent workspace shared by fresh runs and re-runs.
/// Persistence remains best-effort; execution falls back to stdout when the
/// run or artifact directory cannot be created.
fn create_run_workspace(
    app: &crate::emit::EventBus,
    paper_hash: &str,
    pending_meta: crate::runs::RunFinishMeta,
    preprocessing_log: Option<&std::path::Path>,
) -> (Option<crate::runs::RunWriter>, Option<String>) {
    let mut writer = match crate::runs::RunWriter::create_unique(paper_hash) {
        Ok(writer) => Some(writer),
        Err(error) => {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!("WARNING: could not create run directory: {error}")
                }),
            );
            None
        }
    };

    if let Some(writer) = writer.as_mut() {
        if let Err(error) = writer.set_pending_meta(pending_meta) {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!("WARNING: could not update pending run manifest: {error}")
                }),
            );
        }
    }

    if let Some(writer) = writer.as_ref() {
        let logs_dir = writer.dir().join("logs");
        let log_path = logs_dir.join("run.log");
        let opened = std::fs::create_dir_all(&logs_dir).and_then(|()| {
            if let Some(source) = preprocessing_log {
                std::fs::copy(source, &log_path)?;
            }
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
        });
        match opened {
            Ok(file) => crate::pipeline::logging::set_log_sink(Some(file)),
            Err(error) => {
                let _ = app.emit_event(
                    "pipeline:log",
                    serde_json::json!({
                        "line": format!("WARNING: could not open run log file: {error}")
                    }),
                );
            }
        }
    }

    let artifact_dir = writer.as_ref().and_then(|writer| {
        let dir = writer.dir().join("artifacts");
        match std::fs::create_dir_all(&dir) {
            Ok(()) => Some(
                crate::pipeline::claude::normalize_cli_root(&dir.to_string_lossy())
                    .unwrap_or_else(|| dir.to_string_lossy().replace('\\', "/")),
            ),
            Err(error) => {
                let _ = app.emit_event(
                    "pipeline:log",
                    serde_json::json!({
                        "line": format!("WARNING: could not create artifact dir, steps will use stdout output: {error}")
                    }),
                );
                None
            }
        }
    });
    crate::pipeline::api_common::reset_write_budget();
    (writer, artifact_dir)
}

/// Begin mirroring logs before extraction has produced a paper hash/run
/// directory. Failed extractions retain this transcript under
/// ~/.pipeline/logs/preprocessing; successful runs adopt it as run.log.
fn start_preprocessing_log() -> Option<std::path::PathBuf> {
    let root = dirs::home_dir()?
        .join(".pipeline")
        .join("logs")
        .join("preprocessing");
    std::fs::create_dir_all(&root).ok()?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f");
    let path = root.join(format!("extract-{stamp}-{}.log", std::process::id()));
    let file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .ok()?;
    crate::pipeline::logging::set_log_sink(Some(file));

    // Retain a bounded set of failed/pre-run transcripts. Successful ones are
    // moved into their run directory below.
    if let Ok(entries) = std::fs::read_dir(&root) {
        let mut files: Vec<_> = entries
            .flatten()
            .filter_map(|entry| {
                let metadata = entry.metadata().ok()?;
                metadata.is_file().then_some((
                    metadata.modified().unwrap_or(std::time::UNIX_EPOCH),
                    entry.path(),
                ))
            })
            .collect();
        files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
        for (_, stale) in files.into_iter().skip(20) {
            let _ = std::fs::remove_file(stale);
        }
    }
    Some(path)
}

fn write_run_input_file(
    root: &std::path::Path,
    prefix: &str,
    suffix: &str,
    content: &str,
    label: &str,
) -> Result<(tempfile::NamedTempFile, String), String> {
    let mut file = tempfile::Builder::new()
        .prefix(prefix)
        .suffix(suffix)
        .tempfile_in(root)
        .map_err(|error| format!("Failed to create {label} temp file: {error}"))?;
    let clean_content = crate::safety::strip_span_tags(content);
    file.write_all(clean_content.as_bytes())
        .map_err(|error| format!("Failed to write {label} temp file: {error}"))?;
    file.flush()
        .map_err(|error| format!("Failed to flush {label} temp file: {error}"))?;
    let path = crate::pipeline::claude::normalize_cli_root(&file.path().to_string_lossy())
        .ok_or_else(|| format!("Failed to resolve {label} temp file"))?;
    Ok((file, path))
}

/// Write a completed run's artifacts and manifest into the run directory:
/// numbered per-step files, `report.md`, `report.json` (structured, for resume),
/// any model-written files, the console log, and the manifest. Returns the run
/// id on success. Shared by fresh runs and re-runs.
fn finalize_run(
    app: &crate::emit::EventBus,
    mut w: crate::runs::RunWriter,
    report: &PipelineReport,
    markdown: &str,
    meta: crate::runs::RunFinishMeta,
) -> Option<String> {
    let outputs = report.all_outputs();
    for (i, output) in outputs.iter().enumerate() {
        let slug = output
            .step_id
            .replace('/', "_")
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        let header = format!(
            "# {}\n\n**Phase**: {} · **Agent**: {}\n\n---\n\n",
            output.step_label,
            output.phase,
            if output.agent.is_empty() {
                "default"
            } else {
                &output.agent
            },
        );
        let rel = format!("artifacts/{:02}_{}.md", i + 1, slug);
        if let Err(e) = w.add_text(
            &rel,
            &output.step_label,
            "step",
            &format!("{}{}", header, output.raw_text),
        ) {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!("WARNING: {e}") }),
            );
        }
    }
    if let Err(e) = w.add_text("report.md", "Report", "report", markdown) {
        let _ = app.emit_event(
            "pipeline:log",
            serde_json::json!({ "line": format!("WARNING: {e}") }),
        );
    }
    // Structured report, so a re-run can reload prior step outputs.
    let report_json_durable = serde_json::to_string_pretty(report)
        .map_err(|error| format!("Failed to serialize report data: {error}"))
        .and_then(|json| w.add_text("report.json", "Report data", "context", &json))
        .map_err(|error| {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!("WARNING: {error}") }),
            );
        })
        .is_ok();
    // Structured checkpoints are only needed until report.json is durable.
    // Keep the final artifact tree uncluttered; interrupted runs retain their
    // checkpoint directory for recovery and inspection.
    if report_json_durable {
        let _ = std::fs::remove_dir_all(w.dir().join("artifacts").join("checkpoints"));
    }
    let extra_files = w.register_unlisted("artifacts", "files");
    if extra_files > 0 {
        let _ = app.emit_event(
            "pipeline:log",
            serde_json::json!({
                "line": format!("Registered {extra_files} model-written supporting files")
            }),
        );
    }
    match w.compact_page_artifacts() {
        Ok(count) if count > 0 => {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!("Compacted {count} page records into one lazy artifact index")
                }),
            );
        }
        Ok(_) => {}
        Err(error) => {
            // Keep the ordinary per-page manifest entries as a compatible
            // fallback if an imported or legacy filename is irregular.
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!("WARNING: could not compact page artifact index: {error}")
                }),
            );
        }
    }
    // Close the console transcript before registering it so the file is complete.
    crate::pipeline::logging::set_log_sink(None);
    let _ = w.register_existing("logs/run.log", "Console log", "context");

    match w.finish(meta) {
        Ok(manifest) => Some(manifest.run_id),
        Err(e) => {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!("WARNING: could not write run manifest: {e}")
                }),
            );
            None
        }
    }
}

/// Purge old runs beyond the retention cap (0 = keep all), logging how many.
fn enforce_retention(app: &crate::emit::EventBus, keep: usize, max_bytes: u64) {
    if keep == 0 && max_bytes == 0 {
        return;
    }
    if let Ok(n) = crate::runs::purge_runs_with_limits(keep, max_bytes) {
        if n > 0 {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!("Removed {n} old run(s) to satisfy history retention limits")
                }),
            );
        }
    }
}

struct RunCompletion {
    input_path: String,
    input_mode: String,
    profile_name: String,
    variables: std::collections::HashMap<String, String>,
    extra_inputs: std::collections::HashMap<String, String>,
    extra_input_sources: std::collections::HashMap<String, String>,
    parent_run_id: Option<String>,
}

/// Finalize persistence, retention, events, and the public response in one
/// place so fresh runs and re-runs cannot drift in status or metadata rules.
#[allow(clippy::too_many_arguments)]
fn complete_run(
    app: &crate::emit::EventBus,
    writer: Option<crate::runs::RunWriter>,
    report: &PipelineReport,
    markdown: &str,
    extracted_text: &str,
    elapsed: std::time::Duration,
    settings: &crate::settings::Settings,
    completion: RunCompletion,
) -> serde_json::Value {
    let status = if report.failed_steps.is_empty() {
        "done"
    } else {
        "partial"
    };
    let meta = crate::runs::RunFinishMeta {
        input_path: completion.input_path,
        input_mode: completion.input_mode,
        profile_id: settings.active_profile.clone(),
        profile_name: completion.profile_name,
        provider: settings.preferred_provider.clone(),
        status: status.to_string(),
        duration_secs: elapsed.as_secs(),
        usage: crate::pipeline::logging::run_usage(),
        step_count: report.all_outputs().len() as u32,
        failed_steps: report
            .failed_steps
            .iter()
            .map(|failure| failure.step_label.clone())
            .collect(),
        variables: completion.variables,
        extra_inputs: completion.extra_inputs,
        extra_input_sources: completion.extra_input_sources,
        parent_run_id: completion.parent_run_id,
    };
    let run_id = writer.and_then(|writer| finalize_run(app, writer, report, markdown, meta));
    enforce_retention(
        app,
        settings.max_saved_runs as usize,
        settings.max_saved_run_bytes,
    );
    app.emit_event(
        "pipeline:stage",
        serde_json::json!({
            "stage": "done",
            "id": "done",
            "label": "Complete",
            "stepIds": [],
        }),
    )
    .ok();

    serde_json::json!({
        "report": report,
        "markdown": markdown,
        "extracted_text": extracted_text,
        "run_id": run_id,
        "status": status,
    })
}

#[tauri::command]
pub async fn run_pipeline(
    app: AppHandle,
    paper_path: String,
    diff: bool,
    variables: Option<std::collections::HashMap<String, String>>,
    extra_inputs: Option<std::collections::HashMap<String, String>>,
    expected_profile_snapshot_id: Option<String>,
) -> Result<serde_json::Value, String> {
    // Prevent concurrent pipeline runs from corrupting shared state
    let _ = crate::runs::recover_resumable_runs();
    let variables = variables.unwrap_or_default();
    let extra_inputs = extra_inputs.unwrap_or_default();
    crate::safety::validate_runtime_context(&variables, "Run variables")?;
    crate::safety::validate_runtime_context(&extra_inputs, "Named input paths")?;
    let snapshot = load_run_snapshot()?;
    validate_primary_input_path(&snapshot.config, Some(&paper_path))?;
    validate_named_input_paths(&snapshot.config, &extra_inputs, true)?;
    let snapshot = bind_runtime_snapshot(snapshot, &variables, &extra_inputs)?;
    let snapshot = bind_foreground_launch(snapshot, &paper_path, diff)?;
    let expected_profile_snapshot_id = expected_profile_snapshot_id
        .as_deref()
        .filter(|expected| !expected.is_empty())
        .ok_or(
            "Run setup has not been prepared. Review the execution plan before launching the run.",
        )?;
    if expected_profile_snapshot_id != snapshot.fingerprint {
        return Err(
            "The active profile or run options changed after the execution plan was prepared. Review the updated plan and run again."
                .to_string(),
        );
    }
    let dependencies =
        check_snapshot_dependencies(&snapshot, diff, Some(&paper_path), &extra_inputs).await?;
    require_snapshot_dependencies(&dependencies)?;
    let guard = acquire_pipeline_guard()?;
    let bus = crate::emit::from_app(app);
    PipelineTask::spawn(
        guard,
        bus,
        paper_path,
        diff,
        variables,
        extra_inputs,
        Some(snapshot),
    )
    .join()
    .await
}

/// Headless entry point for the CLI: run the active profile over one input with
/// the given event sink (no Tauri `AppHandle`). Returns the same JSON as the
/// GUI command. Not a Tauri command — called directly from the CLI binary.
pub async fn run_headless(
    bus: crate::emit::EventBus,
    paper_path: &str,
    variables: std::collections::HashMap<String, String>,
    extra_inputs: std::collections::HashMap<String, String>,
) -> Result<serde_json::Value, String> {
    let _ = crate::runs::recover_resumable_runs();
    let guard = acquire_pipeline_guard()?;
    PipelineTask::spawn(
        guard,
        bus,
        paper_path.to_string(),
        false,
        variables,
        extra_inputs,
        None,
    )
    .join()
    .await
}

#[derive(Clone)]
struct RunSnapshot {
    settings: crate::settings::Settings,
    config: PipelineConfig,
    profile_name: String,
    config_fingerprint: String,
    fingerprint: String,
}

fn settings_for_snapshot_fingerprint(
    settings: &crate::settings::Settings,
) -> crate::settings::Settings {
    use sha2::{Digest as _, Sha256};

    let mut fingerprint_settings = settings.clone();
    for (label, secret) in [
        ("anthropic", &mut fingerprint_settings.anthropic_api_key),
        ("openai", &mut fingerprint_settings.openai_api_key),
        ("google", &mut fingerprint_settings.google_api_key),
        ("local", &mut fingerprint_settings.local_api_key),
    ] {
        if !secret.is_empty() {
            let mut secret_hash = Sha256::new();
            secret_hash.update(b"pipeline run snapshot secret v1\0");
            secret_hash.update(label.as_bytes());
            secret_hash.update(b"\0");
            secret_hash.update(secret.as_bytes());
            *secret = format!("<digest:{:x}>", secret_hash.finalize());
        }
    }
    fingerprint_settings
}

fn load_run_snapshot() -> Result<RunSnapshot, String> {
    use sha2::{Digest as _, Sha256};

    let settings = crate::settings::load_persisted_required().map_err(|e| {
        format!("Cannot start run because settings could not be loaded safely: {e}")
    })?;
    let (config, profile_name) =
        pipeline_config::load_required_profile_for(&settings.active_profile)?;
    let fingerprint_settings = settings_for_snapshot_fingerprint(&settings);
    let encoded = serde_json::to_vec(&(&fingerprint_settings, &config))
        .map_err(|e| format!("Could not fingerprint run configuration: {e}"))?;
    let digest = format!("{:x}", Sha256::digest(encoded));
    let fingerprint = digest[..16].to_string();
    Ok(RunSnapshot {
        settings,
        config,
        profile_name,
        config_fingerprint: fingerprint.clone(),
        fingerprint,
    })
}

fn bind_runtime_snapshot(
    mut snapshot: RunSnapshot,
    variables: &std::collections::HashMap<String, String>,
    extra_inputs: &std::collections::HashMap<String, String>,
) -> Result<RunSnapshot, String> {
    use sha2::{Digest as _, Sha256};
    let variables = variables
        .iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    let extra_inputs = extra_inputs
        .iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    let encoded = serde_json::to_vec(&(
        snapshot.config_fingerprint.as_str(),
        variables,
        extra_inputs,
    ))
    .map_err(|error| format!("Could not fingerprint runtime options: {error}"))?;
    let digest = format!("{:x}", Sha256::digest(encoded));
    snapshot.fingerprint = digest[..16].to_string();
    Ok(snapshot)
}

fn bind_foreground_launch(
    mut snapshot: RunSnapshot,
    paper_path: &str,
    diff: bool,
) -> Result<RunSnapshot, String> {
    use sha2::{Digest as _, Sha256};
    let encoded = serde_json::to_vec(&(snapshot.fingerprint.as_str(), paper_path, diff))
        .map_err(|error| format!("Could not fingerprint launch options: {error}"))?;
    let digest = format!("{:x}", Sha256::digest(encoded));
    snapshot.fingerprint = digest[..16].to_string();
    Ok(snapshot)
}

async fn check_snapshot_dependencies(
    snapshot: &RunSnapshot,
    diff: bool,
    input_path: Option<&str>,
    extra_inputs: &std::collections::HashMap<String, String>,
) -> Result<crate::deps::DepsReport, String> {
    let settings = snapshot.settings.clone();
    let config = snapshot.config.clone();
    let input_path = input_path.map(str::to_string);
    let extra_inputs = extra_inputs.clone();
    tokio::task::spawn_blocking(move || {
        crate::deps::check_snapshot(settings, config, diff, input_path, extra_inputs)
    })
    .await
    .map_err(|error| format!("Dependency check failed: {error}"))
}

fn require_snapshot_dependencies(report: &crate::deps::DepsReport) -> Result<(), String> {
    if report.ready {
        return Ok(());
    }
    let blockers = report
        .deps
        .iter()
        .filter(|dependency| !crate::deps::dependency_ready(dependency))
        .map(|dependency| format!("{}: {}", dependency.name, dependency.hint))
        .collect::<Vec<_>>();
    Err(format!(
        "The captured workflow is not ready to run. {}",
        blockers.join(" ")
    ))
}

fn require_document_input(snapshot: &RunSnapshot, operation: &str) -> Result<(), String> {
    let input_mode = snapshot.config.extraction.input_mode.trim();
    if !matches!(input_mode, "" | "document") {
        return Err(format!(
            "{operation} requires a document-input profile; the active profile uses '{}' input",
            input_mode
        ));
    }
    Ok(())
}

/// Validate a concrete primary selection against the captured workflow before
/// dependency probing or model work starts. Only the legacy empty input mode
/// auto-detects files versus folders; explicit modes enforce their declared
/// path kind so a stale selection cannot silently change workflow semantics.
fn validate_primary_input_path(
    config: &PipelineConfig,
    input_path: Option<&str>,
) -> Result<(), String> {
    let Some(input_path) = input_path else {
        // Setup/schema reads intentionally omit the path. The launch preflight
        // and run command both call this again with the concrete selection.
        return Ok(());
    };
    let input_mode = config.extraction.input_mode.trim();
    if input_mode == "none" {
        if input_path.trim().is_empty() {
            return Ok(());
        }
        return Err("The active workflow does not accept a primary input".to_string());
    }
    if input_path.trim().is_empty() {
        return Err("The active workflow requires a primary input".to_string());
    }
    let metadata = std::fs::metadata(input_path)
        .map_err(|error| format!("The selected input is unavailable: {error}"))?;
    match input_mode {
        "document" if !metadata.is_file() => {
            return Err(
                "The active workflow requires a document, but the selected input is not a regular file"
                    .to_string(),
            );
        }
        "folder" if !metadata.is_dir() => {
            return Err(
                "The active workflow requires a folder, but the selected input is not a folder"
                    .to_string(),
            );
        }
        "" if !metadata.is_file() && !metadata.is_dir() => {
            return Err("The selected input is not a regular file or folder".to_string());
        }
        _ => {}
    }
    Ok(())
}

fn validate_named_input_paths(
    config: &PipelineConfig,
    extra_inputs: &std::collections::HashMap<String, String>,
    require_required: bool,
) -> Result<(), String> {
    for slot in &config.extraction.extra_inputs {
        let path = extra_inputs
            .get(&slot.key)
            .map(|path| path.trim())
            .filter(|path| !path.is_empty());
        let Some(path) = path else {
            if require_required && slot.required {
                let label = if slot.label.trim().is_empty() {
                    slot.key.as_str()
                } else {
                    slot.label.as_str()
                };
                return Err(format!("Missing required input '{label}'"));
            }
            continue;
        };
        let label = if slot.label.trim().is_empty() {
            slot.key.as_str()
        } else {
            slot.label.as_str()
        };
        let metadata = std::fs::metadata(path)
            .map_err(|error| format!("Named input '{label}' is unavailable: {error}"))?;
        match slot.mode.as_str() {
            "folder" if !metadata.is_dir() => {
                return Err(format!(
                    "Named input '{label}' requires a folder, but the selected input is not a folder"
                ));
            }
            "document" if !metadata.is_file() => {
                return Err(format!(
                    "Named input '{label}' requires a document, but the selected input is not a regular file"
                ));
            }
            "document" => {
                let supported = std::path::Path::new(path)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| {
                        matches!(
                            extension.to_ascii_lowercase().as_str(),
                            "pdf" | "tex" | "docx"
                        )
                    });
                if !supported {
                    return Err(format!(
                        "Named input '{label}' must be a PDF, TeX, or DOCX document"
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

async fn run_pipeline_inner_with_snapshot(
    app: &crate::emit::EventBus,
    paper_path: &str,
    diff: bool,
    provided_vars: std::collections::HashMap<String, String>,
    provided_inputs: std::collections::HashMap<String, String>,
    snapshot: Option<RunSnapshot>,
) -> Result<serde_json::Value, String> {
    let _run_state = begin_run_state();
    let pipeline_start = std::time::Instant::now();
    // One immutable settings/profile snapshot defines the entire run. UI
    // edits made while providers are working take effect only on the next run.
    let snapshot = match snapshot {
        Some(snapshot) => snapshot,
        None => load_run_snapshot()?,
    };
    validate_primary_input_path(&snapshot.config, Some(paper_path))?;
    validate_named_input_paths(&snapshot.config, &provided_inputs, true)?;
    let RunSnapshot {
        settings,
        config,
        profile_name,
        ..
    } = snapshot;
    let _settings_snapshot = crate::settings::freeze_for_run(settings.clone());

    let paper = std::path::Path::new(paper_path);

    // Effective run-time variables: the profile's declared defaults, overlaid
    // with whatever the caller provided. Unknown provided keys are kept (a
    // prompt may reference an ad-hoc var).
    let mut variables: std::collections::HashMap<String, String> = config
        .variables
        .iter()
        .map(|v| (v.key.clone(), v.default.clone()))
        .collect();
    variables.extend(provided_vars);
    crate::safety::validate_runtime_context(&variables, "Run variables")?;
    crate::safety::validate_runtime_context(&provided_inputs, "Named input paths")?;
    crate::safety::validate_run_budget(&config, &settings)?;
    let preprocessing_log = start_preprocessing_log();
    crate::pipeline::logging::emit(app, format!("Preparing document bundle from {paper_path}"));

    // Extract paper text
    app.emit_event(
        "pipeline:stage",
        serde_json::json!({
            "stage": "extracting",
            "id": "extracting",
            "label": if config.extraction.input_mode == "none" {
                "Prepare run"
            } else {
                "Extract input"
            },
            "stepIds": [],
        }),
    )
    .ok();
    app.emit_event(
        "pipeline:preprocess",
        serde_json::json!({
            "phase": "extract",
            "status": "running",
        }),
    )
    .ok();
    let extract_start = std::time::Instant::now();
    let input_mode = extract::effective_input_mode(&config.extraction.input_mode, paper_path);
    let extraction_result = match input_mode {
        "folder" => extract::ingest_folder_async(paper_path).await,
        "none" => Ok(extract::ingest_none()),
        _ => extract::extract(app, paper_path, &config.extraction).await,
    };
    let extraction = match extraction_result {
        Ok(value) => value,
        Err(error) => {
            crate::pipeline::logging::emit(
                app,
                format!("ERROR: document preparation failed: {error}"),
            );
            if let Some(path) = preprocessing_log.as_ref() {
                crate::pipeline::logging::emit(
                    app,
                    format!("Preprocessing log retained at {}", path.display()),
                );
            }
            return Err(error);
        }
    };
    let extract_secs = extract_start.elapsed().as_secs();
    crate::pipeline::logging::emit(
        app,
        format!(
            "Extracted via {} ({} chars, {}s)",
            extraction.method,
            extraction.text.len(),
            extract_secs
        ),
    );
    for note in &extraction.quality_notes {
        crate::pipeline::logging::emit(app, format!("WARNING: {note}"));
    }

    // Close/flush the preprocessing sink before copying it into the newly
    // addressable run directory.
    crate::pipeline::logging::set_log_sink(None);
    let (mut run_writer, artifact_write_dir) = create_run_workspace(
        app,
        &extraction.paper_hash,
        crate::runs::RunFinishMeta {
            input_path: paper_path.to_string(),
            input_mode: input_mode.to_string(),
            profile_id: settings.active_profile.clone(),
            profile_name: profile_name.clone(),
            provider: settings.preferred_provider.clone(),
            variables: variables.clone(),
            ..Default::default()
        },
        preprocessing_log.as_deref(),
    );
    if run_writer.is_some() {
        if let Some(path) = preprocessing_log.as_ref() {
            let _ = std::fs::remove_file(path);
        }
    }
    if let Some(w) = run_writer.as_mut() {
        if let Err(e) = w.add_text(
            "context/extracted_text.md",
            "Extracted text",
            "context",
            &extraction.text,
        ) {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!("WARNING: {e}")
                }),
            );
        }
        if extraction.method == "marker" {
            match crate::pipeline::extract::read_marker_structure_json(&extraction.paper_hash) {
                Ok(Some(structure)) => {
                    if let Err(e) = w.add_text(
                        "context/marker_structure.json",
                        "Marker structure",
                        "context",
                        &structure,
                    ) {
                        let _ = app.emit_event(
                            "pipeline:log",
                            serde_json::json!({
                                "line": format!("WARNING: {e}")
                            }),
                        );
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!("WARNING: could not retain Marker structure: {e}")
                        }),
                    );
                }
            }
        } else if extraction.method == "paddleocr-vl" {
            match crate::pipeline::extract::read_paddle_structure_json(&extraction.paper_hash) {
                Ok(Some(structure)) => {
                    if let Err(e) = w.add_text(
                        "context/paddle_structure.json",
                        "PaddleOCR-VL structure",
                        "context",
                        &structure,
                    ) {
                        let _ = app.emit_event(
                            "pipeline:log",
                            serde_json::json!({
                                "line": format!("WARNING: {e}")
                            }),
                        );
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!(
                                "WARNING: could not retain PaddleOCR-VL structure: {e}"
                            )
                        }),
                    );
                }
            }
        }
    }

    // Render page images for PDF inputs, and for LaTeX projects that have a
    // compiled companion PDF. The semantic LaTeX source remains primary, but
    // the page renders preserve the visual evidence needed to inspect figures,
    // tables, equation layout, and extraction quality.
    let visual_pdf = if extraction
        .source_path
        .to_ascii_lowercase()
        .ends_with(".pdf")
    {
        Some(std::path::PathBuf::from(&extraction.source_path))
    } else {
        crate::document_bundle::companion_pdf(&extraction.source_path)
    };
    if input_mode == "document" {
        if let (Some(pdf), Some(w)) = (visual_pdf, run_writer.as_mut()) {
            let out_dir = w.dir().join("artifacts").join("pages");
            let rendered = tokio::task::spawn_blocking(move || {
                crate::pipeline::extract::render_pdf_pages(
                    &pdf,
                    &out_dir,
                    crate::pipeline::extract::MAX_RENDERED_PDF_PAGES,
                )
            })
            .await;
            match rendered {
                Ok(Ok(rendered)) => {
                    let names = rendered.names;
                    let count = names.len();
                    for name in &names {
                        let page_num = std::path::Path::new(name)
                            .file_stem()
                            .and_then(|stem| stem.to_str())
                            .unwrap_or(name)
                            .rsplit('-')
                            .next()
                            .and_then(|n| n.parse::<u32>().ok());
                        let label = match page_num {
                            Some(n) => format!("Page {n}"),
                            None => name.clone(),
                        };
                        if let Err(e) =
                            w.register_existing(&format!("artifacts/pages/{name}"), &label, "pages")
                        {
                            let _ = app.emit_event(
                                "pipeline:log",
                                serde_json::json!({
                                    "line": format!("WARNING: {e}")
                                }),
                            );
                        }
                    }
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!("Rendered {count} page images into the run artifacts")
                        }),
                    );
                    if rendered.truncated {
                        let _ = app.emit_event(
                            "pipeline:log",
                            serde_json::json!({
                                "line": format!(
                                    "WARNING: page image rendering reached the {}-page cap; a longer PDF is truncated in the artifact view",
                                    crate::pipeline::extract::MAX_RENDERED_PDF_PAGES
                                )
                            }),
                        );
                    }
                }
                Ok(Err(e)) => {
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!("WARNING: page image rendering skipped: {e}")
                        }),
                    );
                }
                Err(e) => {
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!("WARNING: page image rendering task failed: {e}")
                        }),
                    );
                }
            }
        }
    }

    // When marker did the extraction, collect the figure images it emitted
    // into the run artifacts. Best-effort.
    if extraction.method == "marker" {
        if let Some(w) = run_writer.as_mut() {
            let images = match crate::pipeline::extract::marker_image_files(&extraction.paper_hash)
            {
                Ok(images) => images,
                Err(error) => {
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!("WARNING: marker image discovery stopped: {error}")
                        }),
                    );
                    Vec::new()
                }
            };
            if !images.is_empty() {
                let figures_dir = w.dir().join("artifacts").join("figures");
                if let Err(e) = std::fs::create_dir_all(&figures_dir) {
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!("WARNING: could not create figures dir: {e}")
                        }),
                    );
                } else {
                    let mut copied = 0usize;
                    for src in &images {
                        let Some(name) = src.file_name().and_then(|n| n.to_str()) else {
                            continue;
                        };
                        if std::fs::copy(src, figures_dir.join(name)).is_ok()
                            && w.register_existing(
                                &format!("artifacts/figures/{name}"),
                                name,
                                "figures",
                            )
                            .is_ok()
                        {
                            copied += 1;
                        }
                    }
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!("Collected {copied} figure images from marker output")
                        }),
                    );
                }
            }
        }
    }
    if is_cancelled() {
        return Err("Pipeline cancelled".into());
    }

    // Build the durable, source-neutral document model after visual artifacts
    // have been collected. It will be enriched with orientation metadata and
    // persisted after the orientation stage below.
    let mut document_bundle = None;
    if let Some(w) = run_writer.as_mut() {
        match crate::document_bundle::build(&extraction, w.dir()) {
            Ok(build) => {
                for (rel_path, label, group) in &build.added_artifacts {
                    if let Err(error) = w.register_existing(rel_path, label, group) {
                        let _ = app.emit_event(
                            "pipeline:log",
                            serde_json::json!({
                                "line": format!("WARNING: {error}")
                            }),
                        );
                    }
                }
                document_bundle = Some(build.bundle);
            }
            Err(error) => {
                let _ = app.emit_event(
                    "pipeline:log",
                    serde_json::json!({
                        "line": format!(
                            "WARNING: could not build the structured document bundle: {error}"
                        )
                    }),
                );
            }
        }
    }

    // Cache the extracted text by paper hash so users can inspect it after the run.
    // Temp files vanish when the process exits; the cache persists until deleted.
    let cached_paper_path = match cache_paper_text(&extraction.paper_hash, &extraction.text) {
        Ok(p) => Some(p),
        Err(e) => {
            // Caching is best-effort; a failure here shouldn't stop the run.
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!("WARNING: failed to cache extracted text: {e}")
                }),
            );
            None
        }
    };

    // Keep every model-readable transient for this run under one private
    // directory. CLI providers can grant this root without exposing unrelated
    // files in the process-wide temp directory.
    let run_input_dir = tempfile::Builder::new()
        .prefix("pipeline_run_inputs_")
        .tempdir()
        .map_err(|e| format!("Failed to create run input directory: {e}"))?;
    let scoped_source =
        match extract::stage_selected_source(paper, input_mode, run_input_dir.path()) {
            Ok(context) => context,
            Err(error) => {
                app.emit_event(
                    "pipeline:log",
                    serde_json::json!({
                        "line": format!(
                            "WARNING: source context is unavailable to review steps: {error}"
                        )
                    }),
                )
                .ok();
                extract::ScopedSourceContext::default()
            }
        };
    let scoped_source_path = scoped_source
        .source_path
        .as_deref()
        .and_then(|path| crate::pipeline::claude::normalize_cli_root(&path.to_string_lossy()))
        .unwrap_or_default();
    let scoped_source_read_root = scoped_source
        .read_root
        .as_deref()
        .and_then(|path| crate::pipeline::claude::normalize_cli_root(&path.to_string_lossy()));
    app.emit_event(
        "pipeline:preprocess",
        serde_json::json!({
            "phase": "extract",
            "status": "done",
            "method": extraction.method,
            "chars": extraction.text.len(),
            "elapsed_secs": extract_secs,
            "paper_hash": extraction.paper_hash,
            "cached_path": cached_paper_path,
        }),
    )
    .ok();

    // Build orientation map (optional)
    let mut _orient_tmp = None; // hold tempfile alive
    let orientation;
    let orientation_path;

    if config.use_orientation {
        app.emit_event(
            "pipeline:stage",
            serde_json::json!({
                "stage": "orienting",
                "id": "orienting",
                "label": "Build orientation map",
                "stepIds": [],
            }),
        )
        .ok();
        app.emit_event(
            "pipeline:preprocess",
            serde_json::json!({
                "phase": "orient",
                "status": "running",
            }),
        )
        .ok();
        let orient_start = std::time::Instant::now();
        let survey_template =
            orient::resolve_survey_template(&config.orientation_prompt, input_mode);
        orientation = orient::build_orientation_map(
            app,
            &extraction,
            survey_template.as_deref(),
            scoped_source_read_root.as_deref(),
        )
        .await?;
        let orient_secs = orient_start.elapsed().as_secs();
        app.emit_event(
            "pipeline:log",
            serde_json::json!({
                "line": format!("Orientation map built ({}s)", orient_secs)
            }),
        )
        .ok();
        if is_cancelled() {
            return Err("Pipeline cancelled".into());
        }

        let orientation_json = serde_json::to_string(&orientation)
            .map_err(|e| format!("Failed to serialize orientation map: {e}"))?;
        let (orient_file, path) = write_run_input_file(
            run_input_dir.path(),
            "pipeline_orient_",
            ".json",
            &orientation_json,
            "orientation",
        )?;
        orientation_path = path;
        if let Some(w) = run_writer.as_mut() {
            if let Err(e) = w.add_text(
                "context/orientation.json",
                "Orientation map",
                "context",
                &orientation_json,
            ) {
                let _ = app.emit_event(
                    "pipeline:log",
                    serde_json::json!({
                        "line": format!("WARNING: {e}")
                    }),
                );
            }
        }
        let orient_bytes = orientation_json.len();
        app.emit_event(
            "pipeline:log",
            serde_json::json!({
                "line": format!("Orientation map written to temp file ({orient_bytes} bytes)")
            }),
        )
        .ok();
        app.emit_event(
            "pipeline:preprocess",
            serde_json::json!({
                "phase": "orient",
                "status": "done",
                "bytes": orient_bytes,
                "elapsed_secs": orient_secs,
            }),
        )
        .ok();
        _orient_tmp = Some(orient_file);
    } else {
        app.emit_event(
            "pipeline:log",
            serde_json::json!({
                "line": "Orientation map disabled for this profile"
            }),
        )
        .ok();
        app.emit_event(
            "pipeline:preprocess",
            serde_json::json!({
                "phase": "orient",
                "status": "skipped",
            }),
        )
        .ok();
        orientation = serde_json::to_value(crate::models::OrientationMap::empty(&extraction.text))
            .map_err(|e| format!("Failed to build orientation placeholder: {e}"))?;
        orientation_path = String::new();
    }

    // Orientation supplies semantic labels and page references that are useful
    // additions to the deterministic extraction. Persist three views:
    // canonical JSON, streaming JSONL blocks, and a readable Markdown view.
    let mut bundle_json = None;
    let mut bundle_markdown = None;
    if let Some(bundle) = document_bundle.as_mut() {
        bundle.enrich_from_orientation(&orientation);
        if let Err(error) = bundle.validate() {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!("WARNING: document bundle validation failed: {error}")
                }),
            );
        } else {
            let markdown = bundle.to_markdown_with_text(&extraction.text);
            if let Some(w) = run_writer.as_mut() {
                if let Err(error) = w.add_text(
                    "context/document.md",
                    "Readable document",
                    "document",
                    &markdown,
                ) {
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!("WARNING: {error}")
                        }),
                    );
                }
            }
            bundle_markdown = Some(markdown);
            match bundle
                .to_json_pretty()
                .and_then(|json| bundle.to_jsonl().map(|jsonl| (json, jsonl)))
            {
                Ok((json, jsonl)) => {
                    if let Some(w) = run_writer.as_mut() {
                        for result in [
                            w.add_text(
                                "context/document_bundle.json",
                                "Document bundle",
                                "document",
                                &json,
                            ),
                            w.add_text(
                                "context/blocks.jsonl",
                                "Document blocks",
                                "document",
                                &jsonl,
                            ),
                        ] {
                            if let Err(error) = result {
                                let _ = app.emit_event(
                                    "pipeline:log",
                                    serde_json::json!({
                                        "line": format!("WARNING: {error}")
                                    }),
                                );
                            }
                        }
                    }
                    bundle_json = Some(json);
                }
                Err(error) => {
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!(
                                "WARNING: structured document context was omitted: {error}"
                            )
                        }),
                    );
                }
            }
        }
    }

    // New steps receive the readable bundle view. Runs where bundle
    // persistence was unavailable retain the legacy extracted-text behavior.
    let paper_document_text = bundle_markdown.as_deref().unwrap_or(&extraction.text);
    let (_paper_tmp, paper_text_path) = write_run_input_file(
        run_input_dir.path(),
        "pipeline_document_",
        ".md",
        paper_document_text,
        "document-view",
    )?;
    let mut _bundle_tmp = None;
    let document_bundle_path = if let Some(json) = bundle_json.as_deref() {
        let (file, path) = write_run_input_file(
            run_input_dir.path(),
            "pipeline_document_bundle_",
            ".json",
            json,
            "document-bundle",
        )?;
        _bundle_tmp = Some(file);
        path
    } else {
        String::new()
    };

    // {paper_type} resolves only for paper-shaped surveys; empty otherwise.
    let paper_type = crate::models::paper_view(&orientation)
        .map(|v| v.metadata.paper_type.to_string())
        .unwrap_or_default();
    let survey_hint = crate::models::survey_hint(&orientation);

    // Extra named inputs (1.2.4): extract each declared slot's file through the
    // same cascade, write the text to a temp file, and expose its path to
    // prompts as {input:key}. Temp files are held alive until the run finishes.
    let mut resolved_inputs: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut persisted_inputs: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut extra_tmps: Vec<tempfile::NamedTempFile> = Vec::new();
    let mut extra_sources: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (input_index, slot) in config.extraction.extra_inputs.iter().enumerate() {
        let provided = provided_inputs
            .get(&slot.key)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());
        let path = match provided {
            Some(p) => p,
            None => {
                if slot.required {
                    let name = if slot.label.is_empty() {
                        &slot.key
                    } else {
                        &slot.label
                    };
                    return Err(format!("Missing required input '{name}'"));
                }
                continue;
            }
        };
        extra_sources.insert(slot.key.clone(), path.to_string());
        if let Some(writer) = run_writer.as_mut() {
            let _ = writer.record_extra_input_source(&slot.key, path);
        }
        let ex = match slot.mode.as_str() {
            "folder" => extract::ingest_folder_async(path).await?,
            _ => extract::extract(app, path, &config.extraction).await?,
        };
        if let Some(w) = run_writer.as_mut() {
            let rel_path = extra_input_artifact_path(input_index, &slot.key);
            let label = if slot.label.is_empty() {
                &slot.key
            } else {
                &slot.label
            };
            if w.add_text(&rel_path, label, "context", &ex.text).is_ok()
                && w.record_extra_input(&slot.key, &rel_path).is_ok()
            {
                persisted_inputs.insert(slot.key.clone(), rel_path);
            }
        }
        let (tf, p) = write_run_input_file(
            run_input_dir.path(),
            "pipeline_input_",
            ".txt",
            &ex.text,
            &format!("input '{}'", slot.key),
        )?;
        resolved_inputs.insert(slot.key.clone(), p);
        extra_tmps.push(tf);
        app.emit_event("pipeline:log", serde_json::json!({
            "line": format!("Extra input '{}' extracted via {} ({} chars)", slot.key, ex.method, ex.text.len())
        })).ok();
    }
    if is_cancelled() {
        return Err("Pipeline cancelled".into());
    }

    let result = executor::execute_steps(
        app,
        &config,
        &orientation_path,
        &orientation,
        &paper_text_path,
        &document_bundle_path,
        &scoped_source_path,
        &paper_type,
        &survey_hint,
        &variables,
        &resolved_inputs,
        &extra_sources,
        &std::collections::HashMap::new(), // no preloaded steps for a fresh run
        artifact_write_dir.as_deref(),
        &settings,
    )
    .await?;
    drop(extra_tmps); // keep temp files alive until steps have run

    // Steps are done — close the write window before rendering/reconciling.

    if is_cancelled() {
        return Err("Pipeline cancelled".into());
    }

    let report = PipelineReport {
        orientation,
        step_outputs: result.outputs,
        failed_steps: result.failed_steps,
        referee_reports: vec![],
        editor: None,
        report_date: chrono::Local::now().date_naive(),
        paper_hash: extraction.paper_hash.clone(),
    };

    // Optional diff
    let mut diff_text = None;
    if diff || settings.auto_revision_reconciliation {
        if let Ok(Some(prior)) =
            crate::runs::load_latest_report_for_input(paper_path, &extraction.paper_hash)
        {
            if let Ok(dt) = reconcile::reconcile(app, &prior, &report).await {
                diff_text = Some(dt);
            }
        }
    }
    let elapsed = pipeline_start.elapsed();
    let markdown = output::render_markdown(&report, diff_text.as_deref(), elapsed, &settings);

    Ok(complete_run(
        app,
        run_writer,
        &report,
        &markdown,
        &extraction.text,
        elapsed,
        &settings,
        RunCompletion {
            input_path: paper_path.to_string(),
            input_mode: input_mode.to_string(),
            profile_name,
            variables,
            extra_inputs: persisted_inputs,
            extra_input_sources: extra_sources,
            parent_run_id: None,
        },
    ))
}

// --- Resume / partial re-run (1.3.2) ---

/// Re-run a past run, reusing its cached extraction and orientation and (for
/// partial modes) its successful step outputs, so only the necessary steps
/// re-execute. Uses the *active* profile's steps, so editing a prompt and
/// re-running is cheap. Modes:
///   - `from_step = Some(id)`: reuse steps before `id`; re-run `id` onward.
///   - `only_failed = true`: reuse successful steps except those downstream of
///     a failed step.
///   - neither: reuse only extraction/orientation; re-run every step.
#[tauri::command]
pub async fn rerun_run(
    app: AppHandle,
    run_id: String,
    from_step: Option<String>,
    only_failed: bool,
) -> Result<serde_json::Value, String> {
    crate::runs::recover_resumable_runs()?;
    let guard = acquire_pipeline_guard()?;
    let bus = crate::emit::from_app(app);
    PipelineTask::spawn_future(async move {
        let _guard = guard;
        rerun_run_inner(&bus, &run_id, from_step, only_failed).await
    })
    .join()
    .await
}

fn read_run_file(run_id: &str, rel: &str) -> Result<String, String> {
    crate::runs::validate_run_id(run_id)?;
    if rel.is_empty()
        || std::path::Path::new(rel).is_absolute()
        || rel.split(['/', '\\']).any(|part| part == "..")
    {
        return Err("Invalid run-relative path".to_string());
    }
    let run_dir = crate::runs::runs_dir()?
        .join(run_id)
        .canonicalize()
        .map_err(|_| "Run not found".to_string())?;
    let path = run_dir
        .join(rel)
        .canonicalize()
        .map_err(|e| format!("Cannot resolve {rel} from run: {e}"))?;
    if !path.starts_with(&run_dir) {
        return Err("Run-relative path resolves outside the run directory".to_string());
    }
    let file = crate::safety::open_regular_file(&path)
        .map_err(|e| format!("Cannot read {rel} from run: {e}"))?;
    let mut bytes = Vec::with_capacity(64 * 1024);
    file.take(MAX_RUN_CONTEXT_SIZE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read {rel} from run: {e}"))?;
    if bytes.len() as u64 > MAX_RUN_CONTEXT_SIZE {
        return Err(format!(
            "{rel} exceeds the {} MB run-context safety limit",
            MAX_RUN_CONTEXT_SIZE / 1024 / 1024
        ));
    }
    String::from_utf8(bytes).map_err(|e| format!("{rel} is not valid UTF-8: {e}"))
}

fn load_run_report(run_id: &str) -> Result<PipelineReport, String> {
    let json = read_run_file(run_id, "report.json")
        .map_err(|_| "This run predates comparison support (no report.json).".to_string())?;
    serde_json::from_str(&json).map_err(|e| format!("Invalid report.json: {e}"))
}

fn base_step_id(step_id: &str) -> String {
    step_id.split('/').next().unwrap_or(step_id).to_string()
}

fn failed_base_ids(failures: &[crate::models::StepFailure]) -> std::collections::HashSet<String> {
    failures
        .iter()
        .map(|failure| base_step_id(&failure.step_id))
        .collect()
}

fn resume_seed_ids(
    enabled_ids: &[String],
    report: &PipelineReport,
) -> std::collections::HashSet<String> {
    let mut seeds = failed_base_ids(&report.failed_steps);
    let completed: std::collections::HashSet<String> = report
        .step_outputs
        .iter()
        .map(|output| base_step_id(&output.step_id))
        .collect();
    // Recovery reports use a synthetic run-level failure when the process
    // stopped without returning an ExecutionResult. Missing enabled steps are
    // therefore the authoritative restart point.
    seeds.extend(
        enabled_ids
            .iter()
            .filter(|step_id| !completed.contains(*step_id))
            .cloned(),
    );
    seeds
}

fn collect_preloaded_outputs(
    outputs: &[crate::models::StepOutput],
    rerun: &std::collections::HashSet<String>,
) -> std::collections::HashMap<String, Vec<crate::models::StepOutput>> {
    let mut preloaded = std::collections::HashMap::<String, Vec<crate::models::StepOutput>>::new();
    for output in outputs.iter().filter(|output| !output.skipped) {
        let base = base_step_id(&output.step_id);
        if !rerun.contains(&base) {
            preloaded.entry(base).or_default().push(output.clone());
        }
    }
    preloaded
}

async fn rerun_run_inner(
    app: &crate::emit::EventBus,
    parent_run_id: &str,
    from_step: Option<String>,
    only_failed: bool,
) -> Result<serde_json::Value, String> {
    let _run_state = begin_run_state();
    let start = std::time::Instant::now();
    let settings = crate::settings::load_persisted_required()
        .map_err(|e| format!("Cannot re-run because settings could not be loaded safely: {e}"))?;
    let _settings_snapshot = crate::settings::freeze_for_run(settings.clone());

    let parent = crate::runs::load_manifest(parent_run_id)?;
    let report_json = read_run_file(parent_run_id, "report.json")
        .map_err(|_| "This run predates re-run support (no report.json). Re-run is only available for runs created after upgrading.".to_string())?;
    let parent_report: PipelineReport = serde_json::from_str(&report_json)
        .map_err(|e| format!("Invalid parent report.json: {e}"))?;
    let extracted_text = read_run_file(parent_run_id, "context/extracted_text.md")?;
    let mut orientation_value: serde_json::Value = parent_report.orientation.clone();

    // Active profile drives the re-run (edited prompts take effect).
    let (config, profile_name) =
        pipeline_config::load_required_profile_for(&settings.active_profile)?;
    crate::safety::validate_runtime_context(&parent.variables, "Re-run variables")?;
    crate::safety::validate_runtime_context(
        &parent.extra_input_sources,
        "Re-run named input paths",
    )?;
    crate::safety::validate_runtime_context(&parent.extra_inputs, "Re-run captured input paths")?;
    crate::safety::validate_run_budget(&config, &settings)?;

    // Keep all reconstructed model-readable inputs in one private root.
    // If the original selection has since disappeared, cached text can still
    // support steps that did not select the source artifact.
    let run_input_dir = tempfile::Builder::new()
        .prefix("pipeline_run_inputs_")
        .tempdir()
        .map_err(|e| format!("Failed to create run input directory: {e}"))?;
    let original_source = std::path::Path::new(&parent.input_path);
    let scoped_source = if parent.input_mode != "none" && !original_source.exists() {
        app.emit_event(
            "pipeline:log",
            serde_json::json!({
                "line": "WARNING: the original source is no longer available; source selectors will be empty"
            }),
        )
        .ok();
        extract::ScopedSourceContext::default()
    } else {
        match extract::stage_selected_source(
            original_source,
            &parent.input_mode,
            run_input_dir.path(),
        ) {
            Ok(context) => context,
            Err(error) => {
                app.emit_event(
                    "pipeline:log",
                    serde_json::json!({
                        "line": format!(
                            "WARNING: source context is unavailable to review steps: {error}"
                        )
                    }),
                )
                .ok();
                extract::ScopedSourceContext::default()
            }
        }
    };
    let scoped_source_path = scoped_source
        .source_path
        .as_deref()
        .and_then(|path| crate::pipeline::claude::normalize_cli_root(&path.to_string_lossy()))
        .unwrap_or_default();
    let scoped_source_read_root = scoped_source
        .read_root
        .as_deref()
        .and_then(|path| crate::pipeline::claude::normalize_cli_root(&path.to_string_lossy()));

    // A run can stop after extraction but before its orientation map becomes
    // durable. Rebuild only that missing preprocessing stage; otherwise reuse
    // the parent's cached map exactly.
    if orientation_value.is_null() {
        if config.use_orientation {
            app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": "Resume: rebuilding the orientation map that did not complete"
                }),
            )
            .ok();
            app.emit_event(
                "pipeline:stage",
                serde_json::json!({
                    "stage": "orienting",
                    "id": "orienting",
                    "label": "Build orientation map",
                    "stepIds": [],
                }),
            )
            .ok();
            let extraction = crate::models::ExtractionResult {
                text: extracted_text.clone(),
                method: "resumed-cache".to_string(),
                source_path: parent.input_path.clone(),
                paper_hash: parent_report.paper_hash.clone(),
                quality_notes: Vec::new(),
            };
            let survey_template =
                orient::resolve_survey_template(&config.orientation_prompt, &parent.input_mode);
            orientation_value = orient::build_orientation_map(
                app,
                &extraction,
                survey_template.as_deref(),
                scoped_source_read_root.as_deref(),
            )
            .await?;
            if is_cancelled() {
                return Err("Pipeline cancelled".into());
            }
        } else {
            orientation_value =
                serde_json::to_value(crate::models::OrientationMap::empty(&extracted_text))
                    .map_err(|error| {
                        format!("Failed to rebuild the orientation placeholder: {error}")
                    })?;
        }
    }

    // Determine which steps to re-run vs. reuse.
    let enabled_ids: Vec<String> = config
        .steps
        .iter()
        .filter(|s| s.enabled)
        .map(|s| s.id.clone())
        .collect();
    let rerun: std::collections::HashSet<String> = if let Some(fs) = &from_step {
        match enabled_ids.iter().position(|id| id == fs) {
            Some(k) => enabled_ids[k..].iter().cloned().collect(),
            None => enabled_ids.iter().cloned().collect(), // unknown step → full re-run
        }
    } else if only_failed {
        let seeds = resume_seed_ids(&enabled_ids, &parent_report);
        let mut set = seeds.clone();
        set.extend(crate::pipeline::executor::dependents_of(&config, &seeds));
        set
    } else {
        enabled_ids.iter().cloned().collect()
    };

    // Preload the reused steps' outputs (keyed by base id).
    let preloaded = collect_preloaded_outputs(&parent_report.step_outputs, &rerun);
    app.emit_event("pipeline:log", serde_json::json!({
        "line": format!("Re-run of {parent_run_id}: reusing {} step(s), re-running the rest", preloaded.len())
    })).ok();

    let extra_input_sources = parent.extra_input_sources.clone();
    let (mut run_writer, artifact_write_dir) = create_run_workspace(
        app,
        &parent_report.paper_hash,
        crate::runs::RunFinishMeta {
            input_path: parent.input_path.clone(),
            input_mode: parent.input_mode.clone(),
            profile_id: settings.active_profile.clone(),
            profile_name: profile_name.clone(),
            provider: settings.preferred_provider.clone(),
            variables: parent.variables.clone(),
            parent_run_id: Some(parent_run_id.to_string()),
            ..Default::default()
        },
        None,
    );
    if let Some(writer) = run_writer.as_mut() {
        for (key, path) in &extra_input_sources {
            let _ = writer.record_extra_input_source(key, path);
        }
    }
    let resumed_extraction = crate::models::ExtractionResult {
        text: extracted_text.clone(),
        method: "resumed-cache".to_string(),
        source_path: parent.input_path.clone(),
        paper_hash: parent_report.paper_hash.clone(),
        quality_notes: Vec::new(),
    };
    let mut rerun_bundle_json = None;
    let mut rerun_document_text = extracted_text.clone();
    if let Some(w) = run_writer.as_mut() {
        let _ = w.add_text(
            "context/extracted_text.md",
            "Extracted text",
            "context",
            &extracted_text,
        );
        let orient_json = serde_json::to_string_pretty(&orientation_value).unwrap_or_default();
        let _ = w.add_text(
            "context/orientation.json",
            "Orientation map",
            "context",
            &orient_json,
        );

        // Prefer the parent's canonical bundle so a partial re-run keeps the
        // exact document model it was based on. Copy its visual assets into
        // the new run so the bundle remains self-contained. Older runs fall
        // back to a bundle rebuilt from their cached extraction.
        let mut bundle = read_run_file(parent_run_id, "context/document_bundle.json")
            .ok()
            .and_then(|json| {
                serde_json::from_str::<crate::document_bundle::DocumentBundle>(&json).ok()
            })
            .filter(|bundle| bundle.validate().is_ok());
        if let Some(parent_bundle) = bundle.as_ref() {
            if let Ok(parent_root) = crate::runs::runs_dir() {
                let parent_root = parent_root.join(parent_run_id);
                for asset in &parent_bundle.assets {
                    let source = parent_root.join(&asset.rel_path);
                    let destination = w.dir().join(&asset.rel_path);
                    if !source.is_file() {
                        continue;
                    }
                    if let Some(directory) = destination.parent() {
                        let _ = std::fs::create_dir_all(directory);
                    }
                    if std::fs::copy(&source, &destination).is_ok() {
                        let group = if asset.kind == "page" {
                            "pages"
                        } else {
                            "figures"
                        };
                        let _ = w.register_existing(&asset.rel_path, &asset.label, group);
                    }
                }
            }
        } else if let Ok(build) = crate::document_bundle::build(&resumed_extraction, w.dir()) {
            for (rel_path, label, group) in &build.added_artifacts {
                let _ = w.register_existing(rel_path, label, group);
            }
            bundle = Some(build.bundle);
        }
        if let Some(bundle) = bundle.as_mut() {
            bundle.enrich_from_orientation(&orientation_value);
            if let Err(error) = bundle.validate() {
                let _ = app.emit_event(
                    "pipeline:log",
                    serde_json::json!({
                        "line": format!("WARNING: document bundle validation failed: {error}")
                    }),
                );
            } else {
                let markdown = bundle.to_markdown_with_text(&extracted_text);
                rerun_document_text = markdown.clone();
                if let Err(error) = w.add_text(
                    "context/document.md",
                    "Readable document",
                    "document",
                    &markdown,
                ) {
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({ "line": format!("WARNING: {error}") }),
                    );
                }
                match bundle
                    .to_json_pretty()
                    .and_then(|json| bundle.to_jsonl().map(|jsonl| (json, jsonl)))
                {
                    Ok((json, jsonl)) => {
                        let _ = w.add_text(
                            "context/document_bundle.json",
                            "Document bundle",
                            "document",
                            &json,
                        );
                        let _ = w.add_text(
                            "context/blocks.jsonl",
                            "Document blocks",
                            "document",
                            &jsonl,
                        );
                        rerun_bundle_json = Some(json);
                    }
                    Err(error) => {
                        let _ = app.emit_event(
                            "pipeline:log",
                            serde_json::json!({
                                "line": format!(
                                    "WARNING: structured document context was omitted: {error}"
                                )
                            }),
                        );
                    }
                }
            }
        }
    }

    // Extracted-text + orientation + named-input temp files for the steps to
    // Read, confined to the same private root as the staged source context.
    let (_text_tmp, paper_text_path) = write_run_input_file(
        run_input_dir.path(),
        "pipeline_document_",
        ".md",
        &rerun_document_text,
        "document-view",
    )?;
    let mut _bundle_tmp = None;
    let document_bundle_path = if let Some(json) = rerun_bundle_json.as_deref() {
        let (file, path) = write_run_input_file(
            run_input_dir.path(),
            "pipeline_document_bundle_",
            ".json",
            json,
            "document-bundle",
        )?;
        _bundle_tmp = Some(file);
        path
    } else {
        String::new()
    };

    let orient_json = serde_json::to_string(&orientation_value).unwrap_or_default();
    let (_orient_tmp, orientation_path) = write_run_input_file(
        run_input_dir.path(),
        "pipeline_orient_",
        ".json",
        &orient_json,
        "orientation",
    )?;

    // Rehydrate named inputs from the parent run's captured copies. Required
    // inputs from older runs that predate capture fail explicitly instead of
    // silently substituting an empty string into the prompt.
    let mut resolved_inputs = std::collections::HashMap::new();
    let mut persisted_inputs = std::collections::HashMap::new();
    let mut extra_tmps: Vec<tempfile::NamedTempFile> = Vec::new();
    for (input_index, slot) in config.extraction.extra_inputs.iter().enumerate() {
        let Some(parent_rel) = parent.extra_inputs.get(&slot.key) else {
            if slot.required {
                let label = if slot.label.is_empty() {
                    &slot.key
                } else {
                    &slot.label
                };
                return Err(format!(
                    "Re-run requires named input '{label}', but the parent run did not capture it. Start a new run and select the input again."
                ));
            }
            continue;
        };
        let content = read_run_file(parent_run_id, parent_rel)
            .map_err(|e| format!("Cannot restore named input '{}': {e}", slot.key))?;
        let rel_path = extra_input_artifact_path(input_index, &slot.key);
        if let Some(w) = run_writer.as_mut() {
            let label = if slot.label.is_empty() {
                &slot.key
            } else {
                &slot.label
            };
            if w.add_text(&rel_path, label, "context", &content).is_ok()
                && w.record_extra_input(&slot.key, &rel_path).is_ok()
            {
                persisted_inputs.insert(slot.key.clone(), rel_path);
            }
        }
        let (temp, path) = write_run_input_file(
            run_input_dir.path(),
            "pipeline_input_",
            ".txt",
            &content,
            &format!("restored input '{}'", slot.key),
        )?;
        resolved_inputs.insert(slot.key.clone(), path);
        extra_tmps.push(temp);
    }

    let paper_type = crate::models::paper_view(&orientation_value)
        .map(|v| v.metadata.paper_type.to_string())
        .unwrap_or_default();
    let survey_hint = crate::models::survey_hint(&orientation_value);
    let variables = parent.variables.clone();

    let result = executor::execute_steps(
        app,
        &config,
        &orientation_path,
        &orientation_value,
        &paper_text_path,
        &document_bundle_path,
        &scoped_source_path,
        &paper_type,
        &survey_hint,
        &variables,
        &resolved_inputs,
        &extra_input_sources,
        &preloaded,
        artifact_write_dir.as_deref(),
        &settings,
    )
    .await?;
    drop(extra_tmps);
    if is_cancelled() {
        return Err("Pipeline cancelled".into());
    }

    let report = PipelineReport {
        orientation: orientation_value,
        step_outputs: result.outputs,
        failed_steps: result.failed_steps,
        referee_reports: vec![],
        editor: None,
        report_date: chrono::Local::now().date_naive(),
        paper_hash: parent_report.paper_hash.clone(),
    };
    let elapsed = start.elapsed();
    let markdown = output::render_markdown(&report, None, elapsed, &settings);

    Ok(complete_run(
        app,
        run_writer,
        &report,
        &markdown,
        &extracted_text,
        elapsed,
        &settings,
        RunCompletion {
            input_path: parent.input_path,
            input_mode: parent.input_mode,
            profile_name,
            variables,
            extra_inputs: persisted_inputs,
            extra_input_sources,
            parent_run_id: Some(parent_run_id.to_string()),
        },
    ))
}

// --- Run artifacts ---

#[tauri::command]
pub async fn get_run_manifest(run_id: String) -> Result<crate::runs::RunManifest, String> {
    crate::runs::load_manifest(&run_id)
}

#[tauri::command]
pub async fn read_artifact(
    run_id: String,
    rel_path: String,
) -> Result<crate::runs::ArtifactContent, String> {
    crate::runs::read_artifact(&run_id, &rel_path)
}

#[tauri::command]
pub async fn read_page_artifact(
    run_id: String,
    page: u32,
) -> Result<crate::runs::ArtifactContent, String> {
    crate::runs::read_page_artifact(&run_id, page)
}

/// The structured report for a past run (for run-vs-run comparison).
#[tauri::command]
pub async fn get_run_report(run_id: String) -> Result<PipelineReport, String> {
    load_run_report(&run_id)
}

/// LLM reconciliation of two runs: which concerns were addressed, which remain,
/// what's new. Orders the two by creation time (older = "prior"). Guards
/// against a concurrent pipeline run.
#[tauri::command]
pub async fn reconcile_runs(
    app: AppHandle,
    run_a: String,
    run_b: String,
) -> Result<String, String> {
    let _guard = acquire_pipeline_guard()?;
    CANCEL_FLAG.store(false, std::sync::atomic::Ordering::Release);

    let report_a = load_run_report(&run_a)?;
    let report_b = load_run_report(&run_b)?;
    // Older run is the "prior"; fall back to the given order if timestamps tie.
    let (prior, current) = match (
        crate::runs::load_manifest(&run_a).ok(),
        crate::runs::load_manifest(&run_b).ok(),
    ) {
        (Some(ma), Some(mb)) if mb.created < ma.created => (&report_b, &report_a),
        _ => (&report_a, &report_b),
    };
    let bus = crate::emit::from_app(app);
    reconcile::reconcile(&bus, prior, current).await
}

/// List past runs (newest first) for the history page.
#[tauri::command]
pub async fn list_runs() -> Result<Vec<crate::runs::RunSummary>, String> {
    tokio::task::spawn_blocking(|| {
        // A cancelled run is finalized after the original command returns,
        // so History refresh is also a recovery point in the current process.
        let _ = crate::runs::recover_resumable_runs();
        crate::runs::list_runs()
    })
    .await
    .map_err(|e| format!("Run listing task failed: {e}"))?
}

/// Rename / retag a past run.
#[tauri::command]
pub async fn update_run_meta(
    run_id: String,
    title: String,
    tags: Vec<String>,
) -> Result<(), String> {
    crate::runs::update_run_meta(&run_id, &title, &tags)
}

/// Delete a past run and all its artifacts.
#[tauri::command]
pub async fn delete_run(run_id: String) -> Result<(), String> {
    crate::runs::delete_run(&run_id)
}

/// Number of runs on disk and total bytes they occupy.
#[tauri::command]
pub async fn runs_disk_usage() -> Result<crate::runs::RunsDiskUsage, String> {
    tokio::task::spawn_blocking(crate::runs::disk_usage)
        .await
        .map_err(|e| format!("Disk-usage task failed: {e}"))?
}

/// Preview the completed runs selected by the supplied retention limits.
/// This performs the same bounded disk scan as purge without deleting data.
#[tauri::command]
pub async fn preview_purge_runs(
    keep: u32,
    max_bytes: u64,
) -> Result<crate::runs::RunPurgePreview, String> {
    tokio::task::spawn_blocking(move || {
        crate::runs::preview_purge_runs_with_limits(keep as usize, max_bytes)
    })
    .await
    .map_err(|e| format!("Purge-preview task failed: {e}"))?
}

/// Apply configured count/byte history limits now. Refuses to run concurrently
/// with a pipeline so the active run cannot be selected for deletion.
#[tauri::command]
pub async fn purge_runs(keep: u32, max_bytes: u64, preview_token: String) -> Result<usize, String> {
    let _guard = acquire_pipeline_guard()?;
    crate::runs::purge_runs_with_expected_preview(keep as usize, max_bytes, &preview_token)
}

/// Write arbitrary text to a path (used by "Save console to file").
#[tauri::command]
pub async fn save_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| format!("Failed to write file: {e}"))
}

/// Read a run's per-issue annotations (JSON string, "{}" if none).
#[tauri::command]
pub async fn get_annotations(run_id: String) -> Result<String, String> {
    crate::runs::read_annotations(&run_id)
}

/// Extract (id, title, body) triples from any issues-shaped step output in a
/// report — mirrors the frontend's `parseIssues`.
fn extract_issues_from_report(report: &PipelineReport) -> Vec<(String, String, String)> {
    for output in report.step_outputs.iter().rev() {
        if output.skipped {
            continue;
        }
        let Some(value) = crate::pipeline::structured::extract_json(&output.raw_text) else {
            continue;
        };
        let arr = if let Some(a) = value.as_array() {
            a.clone()
        } else if let Some(a) = value.get("issues").and_then(|v| v.as_array()) {
            a.clone()
        } else {
            continue;
        };
        let mut issues = Vec::new();
        for (i, item) in arr.iter().enumerate() {
            let Some(obj) = item.as_object() else {
                continue;
            };
            let id = obj
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| (i + 1).to_string());
            let title = obj
                .get("title")
                .or_else(|| obj.get("summary"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let body = obj
                .get("body")
                .or_else(|| obj.get("description"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !title.is_empty() || !body.is_empty() {
                issues.push((id, title, body));
            }
        }
        if !issues.is_empty() {
            return issues;
        }
    }
    Vec::new()
}

/// Collect the issues a reviewer rejected (annotation status "reject") across
/// all runs of `profile_id`, as short "title — body" lines. Capped.
fn collect_rejected_issues(profile_id: &str) -> Vec<String> {
    let runs = crate::runs::list_runs().unwrap_or_default();
    let mut out = Vec::new();
    for r in runs.iter().filter(|r| r.profile_id == profile_id) {
        let ann_str = crate::runs::read_annotations(&r.run_id).unwrap_or_else(|_| "{}".to_string());
        let ann: serde_json::Value =
            serde_json::from_str(&ann_str).unwrap_or(serde_json::json!({}));
        let Some(obj) = ann.as_object() else { continue };
        let rejected: Vec<String> = obj
            .iter()
            .filter(|(_, v)| v.get("status").and_then(|s| s.as_str()) == Some("reject"))
            .map(|(k, _)| k.clone())
            .collect();
        if rejected.is_empty() {
            continue;
        }
        let Ok(report) = load_run_report(&r.run_id) else {
            continue;
        };
        let issues = extract_issues_from_report(&report);
        for id in rejected {
            if let Some((_, title, body)) = issues.iter().find(|(iid, _, _)| *iid == id) {
                let mut snippet = body.replace('\n', " ");
                if snippet.chars().count() > 240 {
                    snippet = snippet.chars().take(240).collect::<String>() + "…";
                }
                out.push(format!("- {title} — {snippet}"));
            }
        }
        if out.len() >= 40 {
            break;
        }
    }
    out
}

/// Draft a calibration addendum for the active profile's synthesis step from
/// the issues the reviewer has rejected, so the reviewer stops flagging them.
/// Returns the drafted text plus which step it targets. Guards against a
/// concurrent run (it makes one LLM call).
#[tauri::command]
pub async fn draft_calibration(app: AppHandle) -> Result<serde_json::Value, String> {
    let settings = crate::settings::load_persisted_required().map_err(|e| {
        format!("Cannot draft calibration because settings could not be loaded safely: {e}")
    })?;
    let config = pipeline_config::load();
    let rejected = collect_rejected_issues(&settings.active_profile);
    if rejected.is_empty() {
        return Err("No rejected issues found for this profile yet. Reject some issues in the Issues view first.".into());
    }
    let target = config
        .steps
        .iter()
        .rev()
        .find(|s| s.enabled && s.phase == pipeline_config::Phase::Sequential)
        .ok_or("This profile has no sequential (synthesis) step to calibrate.")?;

    let _guard = acquire_pipeline_guard()?;
    CANCEL_FLAG.store(false, std::sync::atomic::Ordering::Release);

    let list = rejected.join("\n");
    let prompt = format!(
        "A reviewer rejected the following issues from past reviews as not worth flagging:\n\n{list}\n\n\
         Write 2–4 sentences to append to a review-consolidation prompt that instruct the reviewer to stop \
         flagging issues of these kinds in future. Identify the shared patterns concretely (topic, severity, \
         or type) rather than listing the specific items. Output only the sentences — no preamble, no headings."
    );
    let timeout = settings.step_timeout_secs.max(60);
    let bus = crate::emit::from_app(app);
    let mut request = crate::pipeline::call::OwnedRequest::new(
        &bus,
        "calibration",
        "Prompt calibration",
        prompt,
        timeout,
    );
    request.settings = std::sync::Arc::new(settings);
    let raw = crate::pipeline::call::execute_text(request).await?;
    let addendum = output::strip_to_report(&raw);

    Ok(serde_json::json!({
        "addendum": addendum.trim(),
        "target_step_id": target.id,
        "target_label": target.label,
        "rejected_count": rejected.len(),
    }))
}

/// Save a run's per-issue annotations (validated JSON).
#[tauri::command]
pub async fn save_annotations(run_id: String, content: String) -> Result<(), String> {
    crate::runs::write_annotations(&run_id, &content)
}

#[tauri::command]
pub async fn cancel_pipeline() -> Result<(), String> {
    CANCEL_FLAG.store(true, std::sync::atomic::Ordering::Release);
    signal_cancellation();
    kill_all_children();
    Ok(())
}

// --- Batch queue ---

/// Start a batch: run each input path through the pipeline sequentially with
/// the active profile and (optionally) shared variable values. Returns
/// immediately; progress arrives via `batch:progress` events and
/// `get_batch_status`. Fails if any run (single or batch) is already active.
#[tauri::command]
pub async fn start_batch(
    app: AppHandle,
    paths: Vec<String>,
    variables: Option<std::collections::HashMap<String, String>>,
    extra_inputs: Option<std::collections::HashMap<String, String>>,
    expected_profile_config_snapshot_id: Option<String>,
) -> Result<(), String> {
    let paths: Vec<String> = paths.into_iter().filter(|p| !p.trim().is_empty()).collect();
    if paths.is_empty() {
        return Err("No inputs to run".into());
    }
    // Capture and validate the complete run definition once. Later profile,
    // provider, or prompt edits apply only to a subsequent batch.
    let snapshot = load_run_snapshot()?;
    let expected_profile_config_snapshot_id = expected_profile_config_snapshot_id
        .as_deref()
        .filter(|expected| !expected.is_empty())
        .ok_or("Batch setup has not been prepared. Reload the active workflow before starting.")?;
    if expected_profile_config_snapshot_id != snapshot.config_fingerprint {
        return Err(
            "The active profile or settings changed while batch inputs were being collected. Reload the batch setup and try again."
                .to_string(),
        );
    }
    require_document_input(&snapshot, "Batch processing")?;
    for path in &paths {
        validate_primary_input_path(&snapshot.config, Some(path))?;
    }
    let vars = variables.unwrap_or_default();
    let inputs = extra_inputs.unwrap_or_default();
    crate::safety::validate_runtime_context(&vars, "Batch variables")?;
    crate::safety::validate_runtime_context(&inputs, "Batch named input paths")?;
    validate_named_input_paths(&snapshot.config, &inputs, true)?;
    let dependency_input = paths
        .iter()
        .find(|path| {
            !std::path::Path::new(path)
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("tex") || extension.eq_ignore_ascii_case("docx")
                })
        })
        .or_else(|| paths.first());
    let dependencies = check_snapshot_dependencies(
        &snapshot,
        false,
        dependency_input.map(String::as_str),
        &inputs,
    )
    .await?;
    require_snapshot_dependencies(&dependencies)?;
    let snapshot = bind_runtime_snapshot(snapshot, &vars, &inputs)?;
    let guard = acquire_pipeline_guard()?;
    let batch_profile_id = snapshot.settings.active_profile.clone();
    let batch_snapshot_id = snapshot.fingerprint.clone();
    BATCH_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);
    {
        let mut jobs = BATCH.lock().unwrap_or_else(|e| e.into_inner());
        *jobs = paths
            .iter()
            .map(|p| BatchJob {
                path: p.clone(),
                name: basename(p),
                status: "pending".to_string(),
                run_id: None,
                error: None,
                duration_secs: 0,
                profile_id: batch_profile_id.clone(),
                profile_snapshot_id: batch_snapshot_id.clone(),
            })
            .collect();
    }
    let bus = crate::emit::from_app(app);
    let run_bus = crate::emit::background(&bus);
    let _ = bus.emit_event(
        "pipeline:log",
        serde_json::json!({"line": format!(
            "Batch snapshot: profile '{}' ({})",
            batch_profile_id, batch_snapshot_id
        )}),
    );
    emit_batch(&bus);

    tauri::async_runtime::spawn(async move {
        // RAII guard clears PIPELINE_RUNNING and per-run state even on panic.
        let _guard = guard;
        for (i, path) in paths.iter().enumerate() {
            if BATCH_CANCEL.load(std::sync::atomic::Ordering::Acquire) {
                mark_remaining_cancelled(i);
                break;
            }
            set_job(i, |j| j.status = "running".to_string());
            emit_batch(&bus);

            let started = std::time::Instant::now();
            let result = run_pipeline_inner_with_snapshot(
                &run_bus,
                path,
                false,
                vars.clone(),
                inputs.clone(),
                Some(snapshot.clone()),
            )
            .await;
            let secs = started.elapsed().as_secs();

            match result {
                Ok(v) => {
                    let run_id = v
                        .get("run_id")
                        .and_then(|r| r.as_str())
                        .map(|s| s.to_string());
                    let partial = v.get("status").and_then(|s| s.as_str()) == Some("partial");
                    set_job(i, |j| {
                        j.status = if partial { "failed" } else { "done" }.to_string();
                        j.run_id = run_id.clone();
                        if partial {
                            j.error = Some("One or more pipeline steps failed".to_string());
                        }
                        j.duration_secs = secs;
                    });
                }
                Err(e) => {
                    let cancelled = e.to_lowercase().contains("cancelled");
                    set_job(i, |j| {
                        j.status = if cancelled { "cancelled" } else { "failed" }.to_string();
                        j.error = Some(e.clone());
                        j.duration_secs = secs;
                    });
                }
            }
            emit_batch(&bus);
        }
        let _ = bus.emit_event("batch:done", serde_json::Value::Null);
    });

    Ok(())
}

fn set_job(i: usize, f: impl FnOnce(&mut BatchJob)) {
    let mut jobs = BATCH.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(j) = jobs.get_mut(i) {
        f(j);
    }
}

fn mark_remaining_cancelled(from: usize) {
    let mut jobs = BATCH.lock().unwrap_or_else(|e| e.into_inner());
    for j in jobs.iter_mut().skip(from) {
        if j.status == "pending" || j.status == "running" {
            j.status = "cancelled".to_string();
        }
    }
}

/// Current batch job list (empty if no batch has run this session).
#[tauri::command]
pub async fn get_batch_status() -> Result<Vec<BatchJob>, String> {
    Ok(BATCH.lock().unwrap_or_else(|e| e.into_inner()).clone())
}

/// Cancel a running batch: stop the current run and skip the rest.
#[tauri::command]
pub async fn cancel_batch() -> Result<(), String> {
    BATCH_CANCEL.store(true, std::sync::atomic::Ordering::Release);
    CANCEL_FLAG.store(true, std::sync::atomic::Ordering::Release);
    signal_cancellation();
    kill_all_children();
    Ok(())
}

/// Input files (PDF/LaTeX/Word) directly under `dir`, non-recursive and sorted;
/// hidden files skipped. Shared by "queue this folder" and the watcher.
fn scan_input_files(dir: &str) -> Result<Vec<String>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("Cannot read folder: {e}"))?;
    let mut files: Vec<String> = Vec::new();
    let mut walk = crate::safety::WalkBudget::new("Input folder scan");
    for entry in entries.flatten() {
        walk.entry()?;
        let path = entry.path();
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        if matches!(ext.as_str(), "pdf" | "tex" | "docx") {
            files.push(path.to_string_lossy().replace('\\', "/"));
        }
    }
    files.sort();
    Ok(files)
}

/// List input files (PDF/LaTeX/Word) directly under `dir`, for "queue this folder".
#[tauri::command]
pub async fn list_input_files(dir: String) -> Result<Vec<String>, String> {
    scan_input_files(&dir)
}

// --- Watch folder ---

fn emit_watch(app: &crate::emit::EventBus) {
    let state = WATCH_STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let _ = app.emit_event(
        "watch:status",
        serde_json::to_value(&state).unwrap_or_default(),
    );
}

fn push_watch_job(state: &mut WatchStatus, job: BatchJob) {
    state.processed_total = state.processed_total.saturating_add(1);
    if job.status == "failed" {
        state.failed_total = state.failed_total.saturating_add(1);
    }
    state.processed.push(job);
    if state.processed.len() > MAX_WATCH_HISTORY {
        let excess = state.processed.len() - MAX_WATCH_HISTORY;
        state.processed.drain(..excess);
    }
}

/// Start watching `folder`: files that appear from now on are run through the
/// pipeline with the active profile, one at a time. Files already present are
/// treated as the baseline and not run.
#[tauri::command]
pub async fn start_watch(
    app: AppHandle,
    folder: String,
    variables: Option<std::collections::HashMap<String, String>>,
    extra_inputs: Option<std::collections::HashMap<String, String>>,
    expected_profile_config_snapshot_id: Option<String>,
) -> Result<(), String> {
    if folder.trim().is_empty() {
        return Err("No folder to watch".into());
    }
    // A watcher is one durable execution definition, not a series of runs
    // against whatever profile happens to be active at each poll.
    let snapshot = load_run_snapshot()?;
    let expected_profile_config_snapshot_id = expected_profile_config_snapshot_id
        .as_deref()
        .filter(|expected| !expected.is_empty())
        .ok_or("Watch setup has not been prepared. Reload the active workflow before starting.")?;
    if expected_profile_config_snapshot_id != snapshot.config_fingerprint {
        return Err(
            "The active profile or settings changed while watch inputs were being collected. Reload the watch setup and try again."
                .to_string(),
        );
    }
    require_document_input(&snapshot, "Folder watching")?;
    let vars = variables.unwrap_or_default();
    let inputs = extra_inputs.unwrap_or_default();
    crate::safety::validate_runtime_context(&vars, "Watch variables")?;
    crate::safety::validate_runtime_context(&inputs, "Watch named input paths")?;
    validate_named_input_paths(&snapshot.config, &inputs, true)?;
    let dependencies = check_snapshot_dependencies(&snapshot, false, None, &inputs).await?;
    require_snapshot_dependencies(&dependencies)?;
    let snapshot = bind_runtime_snapshot(snapshot, &vars, &inputs)?;
    let watch_profile_id = snapshot.settings.active_profile.clone();
    let watch_snapshot_id = snapshot.fingerprint.clone();
    // Capture the baseline once. A second asynchronous scan would classify
    // files added in between as pre-existing and silently miss them.
    let baseline = scan_input_files(&folder)?;
    let generation = {
        let mut st = WATCH_STATE.lock().unwrap_or_else(|e| e.into_inner());
        if st.active {
            return Err("Already watching a folder".into());
        }
        let generation = WATCH_GENERATION.fetch_add(1, std::sync::atomic::Ordering::AcqRel) + 1;
        *st = WatchStatus {
            active: true,
            paused: false,
            error: None,
            folder: folder.clone(),
            profile_id: watch_profile_id.clone(),
            profile_snapshot_id: watch_snapshot_id.clone(),
            processed: Vec::new(),
            processed_total: 0,
            failed_total: 0,
        };
        generation
    };
    let bus = crate::emit::from_app(app);
    let run_bus = crate::emit::background(&bus);
    let _ = bus.emit_event(
        "pipeline:log",
        serde_json::json!({"line": format!(
            "Watch snapshot: profile '{}' ({})",
            watch_profile_id, watch_snapshot_id
        )}),
    );
    emit_watch(&bus);

    tauri::async_runtime::spawn(async move {
        // Baseline: files already present are not (re)processed.
        let mut seen: std::collections::HashSet<String> = baseline.into_iter().collect();
        let mut observations: std::collections::HashMap<
            String,
            (u64, Option<std::time::SystemTime>),
        > = std::collections::HashMap::new();
        while WATCH_GENERATION.load(std::sync::atomic::Ordering::Acquire) == generation {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            if WATCH_GENERATION.load(std::sync::atomic::Ordering::Acquire) != generation {
                break;
            }
            let current = match scan_input_files(&folder) {
                Ok(files) => files,
                Err(error) => {
                    let message =
                        format!("Watch stopped because the folder could not be read: {error}");
                    {
                        let mut state = WATCH_STATE.lock().unwrap_or_else(|e| e.into_inner());
                        state.active = false;
                        state.paused = false;
                        state.error = Some(message.clone());
                    }
                    let _ = bus.emit_event("pipeline:log", serde_json::json!({"line": message}));
                    emit_watch(&bus);
                    break;
                }
            };
            let current_set: std::collections::HashSet<String> = current.iter().cloned().collect();
            // Removed files no longer need tombstones. This bounds watcher
            // bookkeeping by the current folder contents rather than by all
            // names ever observed during a long-running session.
            seen.retain(|path| current_set.contains(path));
            observations.retain(|path, _| current_set.contains(path));
            for path in current {
                if seen.contains(&path) {
                    continue;
                }
                // Require two consecutive scans with the same size and mtime;
                // files copied into a watched folder are otherwise submitted
                // while still incomplete and never retried.
                let fingerprint = match std::fs::metadata(&path) {
                    Ok(meta) => (meta.len(), meta.modified().ok()),
                    Err(_) => continue,
                };
                if observations.insert(path.clone(), fingerprint).as_ref() != Some(&fingerprint) {
                    continue;
                }
                // Skip this cycle if any run is active; retry next poll (don't
                // mark as seen, so it's picked up once free).
                let guard = match acquire_pipeline_guard() {
                    Ok(guard) => guard,
                    Err(_) => {
                        let changed = {
                            let mut state = WATCH_STATE.lock().unwrap_or_else(|e| e.into_inner());
                            let changed = !state.paused;
                            state.paused = true;
                            changed
                        };
                        if changed {
                            emit_watch(&bus);
                        }
                        break;
                    }
                };
                let resumed = {
                    let mut state = WATCH_STATE.lock().unwrap_or_else(|e| e.into_inner());
                    let resumed = state.paused;
                    state.paused = false;
                    resumed
                };
                if resumed {
                    emit_watch(&bus);
                }
                if WATCH_GENERATION.load(std::sync::atomic::Ordering::Acquire) != generation {
                    drop(guard);
                    break;
                }
                let name = basename(&path);
                let started = std::time::Instant::now();
                let result = run_pipeline_inner_with_snapshot(
                    &run_bus,
                    &path,
                    false,
                    vars.clone(),
                    inputs.clone(),
                    Some(snapshot.clone()),
                )
                .await;
                let duration_secs = started.elapsed().as_secs();
                drop(guard);
                if WATCH_GENERATION.load(std::sync::atomic::Ordering::Acquire) != generation {
                    break;
                }
                seen.insert(path.clone());
                observations.remove(&path);
                let job = match result {
                    Ok(v) if v.get("status").and_then(|s| s.as_str()) != Some("partial") => {
                        BatchJob {
                            path: path.clone(),
                            name,
                            status: "done".to_string(),
                            run_id: v
                                .get("run_id")
                                .and_then(|r| r.as_str())
                                .map(|s| s.to_string()),
                            error: None,
                            duration_secs,
                            profile_id: watch_profile_id.clone(),
                            profile_snapshot_id: watch_snapshot_id.clone(),
                        }
                    }
                    Ok(v) => BatchJob {
                        path: path.clone(),
                        name,
                        status: "failed".to_string(),
                        run_id: v
                            .get("run_id")
                            .and_then(|r| r.as_str())
                            .map(|s| s.to_string()),
                        error: Some("One or more pipeline steps failed".to_string()),
                        duration_secs,
                        profile_id: watch_profile_id.clone(),
                        profile_snapshot_id: watch_snapshot_id.clone(),
                    },
                    Err(e) => BatchJob {
                        path: path.clone(),
                        name,
                        status: "failed".to_string(),
                        run_id: None,
                        error: Some(e),
                        duration_secs,
                        profile_id: watch_profile_id.clone(),
                        profile_snapshot_id: watch_snapshot_id.clone(),
                    },
                };
                let mut state = WATCH_STATE.lock().unwrap_or_else(|e| e.into_inner());
                push_watch_job(&mut state, job);
                drop(state);
                emit_watch(&bus);
                if WATCH_GENERATION.load(std::sync::atomic::Ordering::Acquire) != generation {
                    break;
                }
            }
        }
        if WATCH_GENERATION.load(std::sync::atomic::Ordering::Acquire) == generation {
            let mut state = WATCH_STATE.lock().unwrap_or_else(|e| e.into_inner());
            state.active = false;
            state.paused = false;
            state.profile_id.clear();
            state.profile_snapshot_id.clear();
            drop(state);
            emit_watch(&bus);
        }
    });
    Ok(())
}

fn mark_watch_stopped(state: &mut WatchStatus) {
    state.active = false;
    state.paused = false;
    state.error = None;
    state.profile_id.clear();
    state.profile_snapshot_id.clear();
}

/// Stop watching (the current file, if any, finishes first) and publish the
/// transition immediately. The retired task observes the generation change
/// and deliberately emits nothing, so the command itself owns this event.
#[tauri::command]
pub async fn stop_watch(app: AppHandle) -> Result<WatchStatus, String> {
    WATCH_GENERATION.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    let updated = {
        let mut state = WATCH_STATE.lock().unwrap_or_else(|e| e.into_inner());
        mark_watch_stopped(&mut state);
        state.clone()
    };
    let bus = crate::emit::from_app(app);
    let _ = bus.emit_event(
        "watch:status",
        serde_json::to_value(&updated).unwrap_or_default(),
    );
    Ok(updated)
}

#[tauri::command]
pub async fn get_watch_status() -> Result<WatchStatus, String> {
    Ok(WATCH_STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone())
}

// --- File I/O ---

#[tauri::command]
pub async fn save_report_md(path: String, markdown: String) -> Result<(), String> {
    let clean = output::normalize_math_delimiters(&output::clean_export_markdown(&markdown));
    std::fs::write(&path, clean).map_err(|e| format!("Failed to write file: {e}"))
}

const MAX_RUN_EXPORT_FILES: usize = 20_000;
const MAX_RUN_EXPORT_DIRS: usize = 10_000;
const MAX_RUN_EXPORT_BYTES: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRunArtifactsResult {
    pub exported_path: String,
    pub file_count: u64,
    pub bytes: u64,
}

fn create_export_stage(destination: &std::path::Path) -> Result<tempfile::TempDir, String> {
    std::fs::create_dir_all(destination).map_err(|error| {
        format!(
            "Failed to create export destination '{}': {error}",
            destination.display()
        )
    })?;
    if !destination.is_dir() {
        return Err(format!(
            "Export destination '{}' is not a directory",
            destination.display()
        ));
    }
    tempfile::Builder::new()
        .prefix(".pipeline-export-staging-")
        .tempdir_in(destination)
        .map_err(|error| format!("Failed to create export staging directory: {error}"))
}

fn finish_export_stage(
    stage: &tempfile::TempDir,
    destination: &std::path::Path,
    name: &str,
) -> Result<std::path::PathBuf, String> {
    for suffix in 0..1_000usize {
        let candidate_name = if suffix == 0 {
            name.to_string()
        } else {
            format!("{name}-{}", suffix + 1)
        };
        let candidate = destination.join(candidate_name);
        if candidate.exists() {
            continue;
        }
        match std::fs::rename(stage.path(), &candidate) {
            Ok(()) => return Ok(candidate),
            Err(_) if candidate.exists() => continue,
            Err(error) => {
                return Err(format!(
                    "Failed to finalize export '{}': {error}",
                    candidate.display()
                ));
            }
        }
    }
    Err("Could not choose a unique export directory name".to_string())
}

fn copy_export_tree(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(u64, u64), String> {
    let mut pending = vec![(source.to_path_buf(), destination.to_path_buf())];
    let mut file_count = 0u64;
    let mut directory_count = 1usize;
    let mut total_bytes = 0u64;
    while let Some((source_dir, destination_dir)) = pending.pop() {
        std::fs::create_dir_all(&destination_dir).map_err(|error| {
            format!(
                "Failed to create export directory '{}': {error}",
                destination_dir.display()
            )
        })?;
        let entries = std::fs::read_dir(&source_dir).map_err(|error| {
            format!(
                "Failed to read run directory '{}': {error}",
                source_dir.display()
            )
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("Failed to read run entry: {error}"))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("Failed to inspect run entry: {error}"))?;
            if file_type.is_symlink() {
                return Err(format!(
                    "Run export refused symbolic link '{}'",
                    entry.path().display()
                ));
            }
            let target = destination_dir.join(entry.file_name());
            if file_type.is_dir() {
                directory_count = directory_count.saturating_add(1);
                if directory_count > MAX_RUN_EXPORT_DIRS {
                    return Err(format!(
                        "Run export exceeds the {MAX_RUN_EXPORT_DIRS}-directory safety limit"
                    ));
                }
                pending.push((entry.path(), target));
                continue;
            }
            if !file_type.is_file() {
                return Err(format!(
                    "Run export refused non-regular file '{}'",
                    entry.path().display()
                ));
            }
            file_count = file_count.saturating_add(1);
            if file_count as usize > MAX_RUN_EXPORT_FILES {
                return Err(format!(
                    "Run export exceeds the {MAX_RUN_EXPORT_FILES}-file safety limit"
                ));
            }
            let mut input = crate::safety::open_regular_file(&entry.path())?;
            let size = input
                .metadata()
                .map_err(|error| format!("Failed to inspect run artifact: {error}"))?
                .len();
            if total_bytes.saturating_add(size) > MAX_RUN_EXPORT_BYTES {
                return Err(format!(
                    "Run export exceeds the {} GB safety limit",
                    MAX_RUN_EXPORT_BYTES / 1024 / 1024 / 1024
                ));
            }
            let mut output = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&target)
                .map_err(|error| {
                    format!(
                        "Failed to create exported artifact '{}': {error}",
                        target.display()
                    )
                })?;
            let remaining = MAX_RUN_EXPORT_BYTES.saturating_sub(total_bytes);
            let mut limited = std::io::Read::take(&mut input, remaining.saturating_add(1));
            let copied = std::io::copy(&mut limited, &mut output)
                .map_err(|error| format!("Failed to copy run artifact: {error}"))?;
            if copied != size || copied > remaining {
                return Err(format!(
                    "Run artifact '{}' changed or exceeded limits during export",
                    entry.path().display()
                ));
            }
            total_bytes = total_bytes.saturating_add(copied);
        }
    }
    Ok((file_count, total_bytes))
}

/// Export one durable run as a complete, self-contained directory. All files
/// are copied into a hidden staging directory first, then atomically renamed
/// to a new unique child of the user-selected destination.
#[tauri::command]
pub async fn export_run_artifacts(
    run_id: String,
    destination: String,
) -> Result<ExportRunArtifactsResult, String> {
    tokio::task::spawn_blocking(move || {
        crate::runs::validate_run_id(&run_id)?;
        let manifest = crate::runs::load_manifest(&run_id)?;
        if manifest.status == "running" {
            return Err("Wait for this run to finish before exporting it".to_string());
        }
        let runs_root = crate::runs::runs_dir()?
            .canonicalize()
            .map_err(|error| format!("Failed to resolve run storage: {error}"))?;
        let source = runs_root
            .join(&run_id)
            .canonicalize()
            .map_err(|error| format!("Failed to resolve run '{run_id}': {error}"))?;
        if !source.starts_with(&runs_root) || !source.is_dir() {
            return Err("Run export source is invalid".to_string());
        }
        let destination = std::path::PathBuf::from(destination);
        std::fs::create_dir_all(&destination)
            .map_err(|error| format!("Failed to create export destination: {error}"))?;
        let destination = destination
            .canonicalize()
            .map_err(|error| format!("Failed to resolve export destination: {error}"))?;
        if destination.starts_with(&runs_root) {
            return Err(
                "Choose an export destination outside Pipeline's internal run storage".to_string(),
            );
        }
        let stage = create_export_stage(&destination)?;
        let (file_count, bytes) = copy_export_tree(&source, stage.path())?;
        let exported =
            finish_export_stage(&stage, &destination, &format!("pipeline-run-{run_id}"))?;
        Ok(ExportRunArtifactsResult {
            exported_path: exported.to_string_lossy().to_string(),
            file_count,
            bytes,
        })
    })
    .await
    .map_err(|error| format!("Run export task failed: {error}"))?
}

/// Legacy in-memory export retained for older frontend builds. It now stages
/// into a new subdirectory, so it cannot overwrite an earlier export or leave
/// a partially written result at the final path.
#[tauri::command]
pub async fn save_all_artifacts(
    dir: String,
    markdown: String,
    extracted_text: String,
    report: crate::models::PipelineReport,
) -> Result<(), String> {
    let destination = std::path::Path::new(&dir);
    let stage = create_export_stage(destination)?;
    let base = stage.path();

    // Final report
    let clean_markdown =
        output::normalize_math_delimiters(&output::clean_export_markdown(&markdown));
    std::fs::write(base.join("report.md"), &clean_markdown)
        .map_err(|e| format!("Failed to write report.md: {e}"))?;

    // Extracted text
    std::fs::write(base.join("extracted_text.md"), &extracted_text)
        .map_err(|e| format!("Failed to write extracted_text.md: {e}"))?;

    // Orientation map
    let orient_json = serde_json::to_string_pretty(&report.orientation)
        .map_err(|e| format!("Failed to serialize orientation: {e}"))?;
    std::fs::write(base.join("orientation.json"), &orient_json)
        .map_err(|e| format!("Failed to write orientation.json: {e}"))?;

    // Individual step outputs
    let steps_dir = base.join("steps");
    std::fs::create_dir_all(&steps_dir)
        .map_err(|e| format!("Failed to create steps directory: {e}"))?;

    let outputs = report.all_outputs();
    for (i, output) in outputs.iter().enumerate() {
        let slug = output
            .step_id
            .replace('/', "_")
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        let filename = format!("{:02}_{}.md", i + 1, slug);
        let header = format!(
            "# {}\n\n**Phase**: {} · **Agent**: {}\n\n---\n\n",
            output.step_label,
            output.phase,
            if output.agent.is_empty() {
                "default"
            } else {
                &output.agent
            },
        );
        std::fs::write(
            steps_dir.join(&filename),
            format!("{}{}", header, output.raw_text),
        )
        .map_err(|e| format!("Failed to write {filename}: {e}"))?;
    }

    finish_export_stage(&stage, destination, "pipeline-core-export")?;
    Ok(())
}

/// Stash the last export path so we can clean it up on the next export.
static LAST_EXPORT_PATH: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

pub(crate) fn cleanup_print_export() {
    if let Ok(mut previous) = LAST_EXPORT_PATH.lock() {
        if let Some(path) = previous.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub(crate) fn cleanup_stale_print_exports() {
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(24 * 60 * 60))
        .unwrap_or(std::time::UNIX_EPOCH);
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten().take(1_000) {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("pipeline_report_") || !name.ends_with(".html") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_file()
            && metadata
                .modified()
                .map(|time| time < cutoff)
                .unwrap_or(false)
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn offline_katex_css() -> Result<String, String> {
    use base64::Engine as _;

    const KATEX_CSS: &str = include_str!("../../node_modules/katex/dist/katex.min.css");
    const FONTS: &[(&str, &[u8])] = &[
        (
            "KaTeX_AMS-Regular",
            include_bytes!("../../node_modules/katex/dist/fonts/KaTeX_AMS-Regular.woff2"),
        ),
        (
            "KaTeX_Caligraphic-Bold",
            include_bytes!("../../node_modules/katex/dist/fonts/KaTeX_Caligraphic-Bold.woff2"),
        ),
        (
            "KaTeX_Caligraphic-Regular",
            include_bytes!("../../node_modules/katex/dist/fonts/KaTeX_Caligraphic-Regular.woff2"),
        ),
        (
            "KaTeX_Fraktur-Bold",
            include_bytes!("../../node_modules/katex/dist/fonts/KaTeX_Fraktur-Bold.woff2"),
        ),
        (
            "KaTeX_Fraktur-Regular",
            include_bytes!("../../node_modules/katex/dist/fonts/KaTeX_Fraktur-Regular.woff2"),
        ),
        (
            "KaTeX_Main-Bold",
            include_bytes!("../../node_modules/katex/dist/fonts/KaTeX_Main-Bold.woff2"),
        ),
        (
            "KaTeX_Main-BoldItalic",
            include_bytes!("../../node_modules/katex/dist/fonts/KaTeX_Main-BoldItalic.woff2"),
        ),
        (
            "KaTeX_Main-Italic",
            include_bytes!("../../node_modules/katex/dist/fonts/KaTeX_Main-Italic.woff2"),
        ),
        (
            "KaTeX_Main-Regular",
            include_bytes!("../../node_modules/katex/dist/fonts/KaTeX_Main-Regular.woff2"),
        ),
        (
            "KaTeX_Math-BoldItalic",
            include_bytes!("../../node_modules/katex/dist/fonts/KaTeX_Math-BoldItalic.woff2"),
        ),
        (
            "KaTeX_Math-Italic",
            include_bytes!("../../node_modules/katex/dist/fonts/KaTeX_Math-Italic.woff2"),
        ),
        (
            "KaTeX_SansSerif-Bold",
            include_bytes!("../../node_modules/katex/dist/fonts/KaTeX_SansSerif-Bold.woff2"),
        ),
        (
            "KaTeX_SansSerif-Italic",
            include_bytes!("../../node_modules/katex/dist/fonts/KaTeX_SansSerif-Italic.woff2"),
        ),
        (
            "KaTeX_SansSerif-Regular",
            include_bytes!("../../node_modules/katex/dist/fonts/KaTeX_SansSerif-Regular.woff2"),
        ),
        (
            "KaTeX_Script-Regular",
            include_bytes!("../../node_modules/katex/dist/fonts/KaTeX_Script-Regular.woff2"),
        ),
        (
            "KaTeX_Size1-Regular",
            include_bytes!("../../node_modules/katex/dist/fonts/KaTeX_Size1-Regular.woff2"),
        ),
        (
            "KaTeX_Size2-Regular",
            include_bytes!("../../node_modules/katex/dist/fonts/KaTeX_Size2-Regular.woff2"),
        ),
        (
            "KaTeX_Size3-Regular",
            include_bytes!("../../node_modules/katex/dist/fonts/KaTeX_Size3-Regular.woff2"),
        ),
        (
            "KaTeX_Size4-Regular",
            include_bytes!("../../node_modules/katex/dist/fonts/KaTeX_Size4-Regular.woff2"),
        ),
        (
            "KaTeX_Typewriter-Regular",
            include_bytes!("../../node_modules/katex/dist/fonts/KaTeX_Typewriter-Regular.woff2"),
        ),
    ];

    let mut css = KATEX_CSS.to_string();
    for (name, bytes) in FONTS {
        let original = format!(
            "src:url(fonts/{name}.woff2) format(\"woff2\"),\
             url(fonts/{name}.woff) format(\"woff\"),\
             url(fonts/{name}.ttf) format(\"truetype\")"
        );
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        let embedded = format!("src:url(\"data:font/woff2;base64,{encoded}\") format(\"woff2\")");
        if !css.contains(&original) {
            return Err(format!(
                "Bundled KaTeX CSS does not match embedded font {name}"
            ));
        }
        css = css.replace(&original, &embedded);
    }
    if css.contains("url(fonts/") || css.contains("http://") || css.contains("https://") {
        return Err("Bundled KaTeX CSS still contains an external font reference".to_string());
    }
    Ok(css)
}

fn build_print_report_html(markdown: &str) -> Result<String, String> {
    use pulldown_cmark::{html, Options, Parser};

    let markdown = output::normalize_math_delimiters(&output::clean_export_markdown(markdown));
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(&markdown, options);
    let mut html_body = String::new();
    html::push_html(&mut html_body, parser);

    // Sanitize HTML to strip <script>, event handlers, and other XSS vectors
    // that could be injected via LLM output (e.g. prompt injection in paper text).
    // ammonia's defaults allow standard text/table/list elements. Images keep
    // their alt text but lose `src`, so opening the temporary print document
    // cannot fetch model-authored remote or local subresources.
    let html_body = ammonia::Builder::default()
        .rm_tag_attributes("img", &["src"])
        .clean(&html_body)
        .to_string();

    // KaTeX code, CSS, and every WOFF2 face are compiled into this one HTML
    // document. The print path therefore performs no network or file fetches.
    const KATEX_JS: &str = include_str!("../../node_modules/katex/dist/katex.min.js");
    const AUTO_RENDER_JS: &str =
        include_str!("../../node_modules/katex/dist/contrib/auto-render.min.js");
    let katex_css = offline_katex_css()?;

    let mut html_doc = String::with_capacity(
        katex_css.len() + KATEX_JS.len() + AUTO_RENDER_JS.len() + html_body.len() + 2048,
    );
    html_doc.push_str("<!DOCTYPE html>\n<html><head>\n<meta charset=\"utf-8\">\n<title>Pipeline Report</title>\n<style>\n");
    html_doc.push_str(&katex_css);
    html_doc.push_str("\n</style>\n<script>\n");
    html_doc.push_str(KATEX_JS);
    html_doc.push_str("\n</script>\n<script>\n");
    html_doc.push_str(AUTO_RENDER_JS);
    html_doc.push_str(concat!(
        "\n</script>\n<style>\n",
        "body { font-family: \"Times New Roman\", Times, serif; max-width: 48em; margin: 2em auto; padding: 0 1em; line-height: 1.5; color: #111; }\n",
        "h1 { font-size: 1.4em; } h2 { font-size: 1.2em; border-bottom: 1px solid #ccc; padding-bottom: 0.2em; }\n",
        "h3 { font-size: 1.05em; } hr { border: none; border-top: 1px solid #ddd; margin: 1.5em 0; }\n",
        "table { border-collapse: collapse; width: 100%; margin: 1em 0; }\n",
        "th, td { border: 1px solid #ccc; padding: 0.4em 0.6em; text-align: left; }\n",
        "th { background: #f5f5f5; }\n",
        "code { background: #f4f4f4; padding: 0.1em 0.3em; border-radius: 3px; font-size: 0.9em; }\n",
        "pre { background: #f4f4f4; padding: 1em; overflow-x: auto; border-radius: 4px; }\n",
        "pre code { background: none; padding: 0; }\n",
        "blockquote { border-left: 3px solid #ccc; margin: 1em 0; padding: 0.5em 1em; color: #555; }\n",
        "@media print { body { margin: 0; max-width: none; } }\n",
        "</style>\n</head><body>\n",
    ));
    html_doc.push_str(&html_body);
    // Rendering is synchronous, but embedded webfonts are not. Wait for the
    // font set before the final paint and print dialog.
    html_doc.push_str(concat!(
        "\n<script>",
        "renderMathInElement(document.body,{delimiters:[",
        "{left:'$$',right:'$$',display:true},",
        "{left:'$',right:'$',display:false},",
        "{left:'\\\\(',right:'\\\\)',display:false},",
        "{left:'\\\\[',right:'\\\\]',display:true}",
        "]});",
        "var fontsReady=(document.fonts&&document.fonts.ready)?document.fonts.ready:Promise.resolve();",
        "fontsReady.then(function(){requestAnimationFrame(function(){window.print();});});",
        "</script>\n</body></html>",
    ));
    Ok(html_doc)
}

#[tauri::command]
pub async fn print_report_html(markdown: String) -> Result<(), String> {
    let html_doc = build_print_report_html(&markdown)?;
    // Clean up previous export file
    cleanup_print_export();

    let mut tmp = tempfile::Builder::new()
        .prefix("pipeline_report_")
        .suffix(".html")
        .tempfile()
        .map_err(|e| format!("Failed to create temp file: {e}"))?;
    tmp.write_all(html_doc.as_bytes())
        .map_err(|e| format!("Failed to write HTML: {e}"))?;
    tmp.flush().map_err(|e| format!("Failed to flush: {e}"))?;

    let path = tmp.into_temp_path();
    let path_buf = path
        .keep()
        .map_err(|e| format!("Failed to persist temp file: {e}"))?;

    if let Ok(mut prev) = LAST_EXPORT_PATH.lock() {
        *prev = Some(path_buf.clone());
    }

    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg("--")
        .arg(&path_buf)
        .spawn()
        .map_err(|e| format!("Failed to open browser: {e}"))?;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer")
            .arg(&path_buf)
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| format!("Failed to open browser: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open")
        .arg("--")
        .arg(&path_buf)
        .spawn()
        .map_err(|e| format!("Failed to open browser: {e}"))?;

    Ok(())
}

/// Open ~/.pipeline/ in the OS file manager. The path is resolved server-side
/// (never passed from the frontend) so there is nothing to sanitize.
#[tauri::command]
pub async fn open_pipeline_dir() -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let dir = home.join(".pipeline");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create .pipeline dir: {e}"))?;
    let path_str = dir.to_string_lossy().to_string();

    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg("--")
        .arg(&path_str)
        .spawn()
        .map_err(|e| format!("Failed to open folder: {e}"))?;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer")
            .arg(&path_str)
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open")
        .arg("--")
        .arg(&path_str)
        .spawn()
        .map_err(|e| format!("Failed to open folder: {e}"))?;

    Ok(())
}

// --- History / Settings ---

#[derive(serde::Serialize)]
pub struct HistoryResponse {
    pub reports: Vec<ReportSummary>,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub async fn list_history() -> Result<HistoryResponse, String> {
    let result = storage::list_reports()?;
    Ok(HistoryResponse {
        reports: result.summaries,
        warnings: result.warnings,
    })
}

#[tauri::command]
pub async fn check_deps() -> Result<crate::deps::DepsReport, String> {
    tokio::task::spawn_blocking(crate::deps::check_all)
        .await
        .map_err(|e| format!("Dependency check failed: {e}"))
}

#[derive(serde::Serialize)]
pub struct SettingsResponse {
    pub settings: crate::settings::Settings,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub async fn get_settings() -> Result<SettingsResponse, String> {
    let (settings, warnings) = crate::settings::load_with_warnings();
    Ok(SettingsResponse { settings, warnings })
}

#[tauri::command]
pub async fn save_settings(settings: crate::settings::Settings) -> Result<(), String> {
    crate::settings::save_preserving_active(&settings)
}

/// Discover models for the provider's currently active transport. Settings are
/// accepted from the unsaved Settings screen so entering/removing an API key
/// immediately switches between CLI and API catalogs.
#[tauri::command]
pub async fn get_model_catalog(
    provider: String,
    settings: Option<crate::settings::Settings>,
    refresh: Option<bool>,
) -> Result<crate::model_catalog::ModelCatalog, String> {
    let settings = settings
        .unwrap_or_else(crate::settings::load_persisted)
        .normalized();
    crate::model_catalog::discover(&provider, &settings, refresh.unwrap_or(false)).await
}

// --- Pipeline config ---

#[tauri::command]
pub async fn get_pipeline_config() -> Result<PipelineConfig, String> {
    Ok(pipeline_config::load())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPlanResponse {
    pub profile_id: String,
    pub profile_config_snapshot_id: String,
    pub profile_snapshot_id: String,
    pub input_mode: String,
    pub variables: Vec<crate::pipeline_config::VarSpec>,
    pub input_slots: Vec<crate::pipeline_config::InputSlot>,
    pub readiness: crate::deps::DepsReport,
    pub stages: Vec<executor::ExecutionPlanStage>,
}

/// Return the canonical scheduler timeline for one immutable active-profile
/// snapshot. The frontend must not independently infer dependency waves.
#[tauri::command]
pub async fn get_execution_plan(
    variables: Option<std::collections::HashMap<String, String>>,
    extra_inputs: Option<std::collections::HashMap<String, String>>,
    expected_profile_config_snapshot_id: Option<String>,
    diff: Option<bool>,
    paper_path: Option<String>,
) -> Result<ExecutionPlanResponse, String> {
    let variables = variables.unwrap_or_default();
    let extra_inputs = extra_inputs.unwrap_or_default();
    crate::safety::validate_runtime_context(&variables, "Planned run variables")?;
    crate::safety::validate_runtime_context(&extra_inputs, "Planned named input paths")?;
    let snapshot = load_run_snapshot()?;
    if expected_profile_config_snapshot_id
        .as_deref()
        .filter(|expected| !expected.is_empty())
        .is_some_and(|expected| expected != snapshot.config_fingerprint)
    {
        return Err(
            "The active profile or settings changed while run inputs were being collected. Reload the run setup and try again."
                .to_string(),
        );
    }
    validate_primary_input_path(&snapshot.config, paper_path.as_deref())?;
    // This command is also used to discover the profile's input slots before
    // the user has filled them. Validate every concrete selection here; the
    // execution commands enforce that all required slots are present.
    validate_named_input_paths(&snapshot.config, &extra_inputs, false)?;
    let diff = diff.unwrap_or(false);
    let readiness =
        check_snapshot_dependencies(&snapshot, diff, paper_path.as_deref(), &extra_inputs).await?;
    let profile_config_snapshot_id = snapshot.config_fingerprint.clone();
    let input_mode = paper_path
        .as_deref()
        .map(|path| {
            extract::effective_input_mode(&snapshot.config.extraction.input_mode, path).to_string()
        })
        .unwrap_or_else(|| match snapshot.config.extraction.input_mode.trim() {
            "" => "document".to_string(),
            mode => mode.to_string(),
        });
    let variable_specs = snapshot.config.variables.clone();
    let input_slots = snapshot.config.extraction.extra_inputs.clone();
    let snapshot = bind_runtime_snapshot(snapshot, &variables, &extra_inputs)?;
    let snapshot = bind_foreground_launch(snapshot, paper_path.as_deref().unwrap_or(""), diff)?;
    let stages = executor::execution_plan(&snapshot.config)?;
    Ok(ExecutionPlanResponse {
        profile_id: snapshot.settings.active_profile,
        profile_config_snapshot_id,
        profile_snapshot_id: snapshot.fingerprint,
        input_mode,
        variables: variable_specs,
        input_slots,
        readiness,
        stages,
    })
}

#[tauri::command]
pub async fn save_pipeline_config(
    config: PipelineConfig,
    profile_id: Option<String>,
) -> Result<(), String> {
    let target = profile_id.unwrap_or_else(pipeline_config::get_active_profile_id);
    pipeline_config::save_for(&target, &config)
}

#[tauri::command]
pub async fn get_default_parallel_template() -> String {
    pipeline_config::default_parallel_template()
}

/// Compiled-in default text for a named prompt (no user overrides applied).
/// Backs the editor's "reset to …" actions, e.g. the generic vs paper-review
/// context templates and survey prompts.
#[tauri::command]
pub async fn get_default_prompt(name: String) -> Result<String, String> {
    crate::prompts::compiled_default(&name)
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Unknown prompt: {name}"))
}

#[tauri::command]
pub async fn reset_pipeline_config() -> Result<PipelineConfig, String> {
    Ok(pipeline_config::reset_defaults())
}

// --- Profiles ---

#[tauri::command]
pub async fn list_profiles() -> Result<Vec<ProfileSummary>, String> {
    pipeline_config::list_profiles()
}

#[tauri::command]
pub async fn get_active_profile() -> Result<String, String> {
    Ok(pipeline_config::get_active_profile_id())
}

#[tauri::command]
pub async fn create_profile(name: String) -> Result<ProfileSummary, String> {
    pipeline_config::create_profile(&name)
}

#[tauri::command]
pub async fn duplicate_profile(
    source_id: String,
    new_name: String,
) -> Result<ProfileSummary, String> {
    pipeline_config::duplicate_profile(&source_id, &new_name)
}

#[tauri::command]
pub async fn rename_profile(id: String, new_name: String) -> Result<ProfileSummary, String> {
    pipeline_config::rename_profile(&id, &new_name)
}

#[tauri::command]
pub async fn delete_profile(id: String) -> Result<(), String> {
    pipeline_config::delete_profile(&id)
}

#[tauri::command]
pub async fn switch_profile(id: String) -> Result<PipelineConfig, String> {
    pipeline_config::switch_profile(&id)
}

#[tauri::command]
pub async fn export_item(path: String, json: String) -> Result<(), String> {
    std::fs::write(&path, json).map_err(|e| format!("Failed to write: {e}"))
}

#[tauri::command]
pub async fn import_item(path: String) -> Result<serde_json::Value, String> {
    let content = read_import_file(&path)?;
    let envelope = pipeline_config::import_envelope(&content)?;
    serde_json::to_value(&envelope).map_err(|e| format!("Serialize error: {e}"))
}

#[tauri::command]
pub async fn export_profile(id: String, path: String) -> Result<(), String> {
    let json = pipeline_config::export_profile_data(&id)?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to write: {e}"))
}

/// Import a profile from a parsed envelope, checking the schema version.
/// Shared by file and URL import.
fn import_profile_envelope(
    envelope: pipeline_config::ExportEnvelope,
) -> Result<ProfileSummary, String> {
    match envelope {
        pipeline_config::ExportEnvelope::Profile { schema_version, name, steps, merge, context_cache, use_orientation, orientation_prompt, extraction, parallel_context_template, variables } => {
            if schema_version > pipeline_config::CURRENT_SCHEMA_VERSION {
                return Err(format!(
                    "This profile was made with a newer version of Pipeline (schema v{schema_version}). Update the app to import it."
                ));
            }
            pipeline_config::import_profile_data(&name, steps, merge, context_cache, use_orientation, orientation_prompt, extraction, parallel_context_template, variables)
        }
        pipeline_config::ExportEnvelope::Step { .. } => {
            Err("This file contains a single step, not a profile. Use Import on the pipeline page to add it to the current profile.".into())
        }
        pipeline_config::ExportEnvelope::Bundle { .. } => {
            Err("This file is a full bundle. Use Import All to restore it.".into())
        }
    }
}

/// Fetch a profile JSON from a public HTTPS URL and import it. For sharing profiles
/// by link (a lab, a syllabus, a gist).
#[tauri::command]
pub async fn import_profile_from_url(url: String) -> Result<ProfileSummary, String> {
    let resp = fetch_public_profile_url(&url).await?;
    if !resp.status().is_success() {
        return Err(format!("Fetch failed: HTTP {}", resp.status()));
    }
    let bytes = read_response_limited(resp, MAX_IMPORT_SIZE as usize).await?;
    let content = String::from_utf8_lossy(&bytes).to_string();
    let envelope = pipeline_config::import_envelope(&content)
        .map_err(|e| format!("The URL did not contain a valid profile: {e}"))?;
    import_profile_envelope(envelope)
}

#[tauri::command]
pub async fn import_profile(path: String) -> Result<ProfileSummary, String> {
    let content = read_import_file(&path)?;
    let envelope = pipeline_config::import_envelope(&content)?;
    import_profile_envelope(envelope)
}

#[tauri::command]
pub async fn export_bundle(path: String) -> Result<(), String> {
    let json = pipeline_config::export_bundle()?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to write: {e}"))
}

#[tauri::command]
pub async fn import_bundle(path: String) -> Result<(), String> {
    let content = read_import_file(&path)?;
    pipeline_config::import_bundle(&content)
}

#[tauri::command]
pub async fn check_for_update() -> Result<crate::updates::UpdateInfo, String> {
    crate::updates::check().await
}

// ── Preprocessing artifact cache ────────────────────────────────────
//
// The extracted paper text is cached at ~/.pipeline/cache/papers/{hash}.txt
// so users can inspect the exact text the LLMs received, even after the
// pipeline run completes and the temp files are gone. Inspecting helps
// catch extraction failures (garbled equations, missing pages) before they
// confuse the referees.

fn paper_cache_dir() -> Result<std::path::PathBuf, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let dir = home.join(".pipeline").join("cache").join("papers");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create cache dir {}: {e}", dir.display()))?;
    Ok(dir)
}

fn validate_paper_hash(hash: &str) -> Result<(), String> {
    if hash.is_empty() {
        return Err("Empty paper hash".into());
    }
    if !hash.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("Invalid paper hash".into());
    }
    Ok(())
}

/// Write the extracted text to ~/.pipeline/cache/papers/{hash}.txt and return
/// the absolute path. Best-effort: callers should not abort on failure.
fn cache_paper_text(paper_hash: &str, text: &str) -> Result<String, String> {
    validate_paper_hash(paper_hash)?;
    let dir = paper_cache_dir()?;
    let path = dir.join(format!("{paper_hash}.txt"));
    std::fs::write(&path, text).map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    // The canonical copy also lives in each retained run. Bound this
    // convenience cache by count and aggregate bytes.
    const MAX_CACHE_FILES: usize = 100;
    const MAX_CACHE_BYTES: u64 = 500_000_000;
    let mut entries: Vec<(std::time::SystemTime, u64, std::path::PathBuf)> =
        std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let meta = entry.metadata().ok()?;
                if !meta.is_file() {
                    return None;
                }
                Some((
                    meta.modified().unwrap_or(std::time::UNIX_EPOCH),
                    meta.len(),
                    entry.path(),
                ))
            })
            .collect();
    entries.sort_by_key(|entry| entry.0);
    let mut total: u64 = entries.iter().map(|entry| entry.1).sum();
    let mut count = entries.len();
    for (_, bytes, old_path) in entries {
        if count <= MAX_CACHE_FILES && total <= MAX_CACHE_BYTES {
            break;
        }
        if old_path != path && std::fs::remove_file(old_path).is_ok() {
            count -= 1;
            total = total.saturating_sub(bytes);
        }
    }
    Ok(path.to_string_lossy().replace('\\', "/"))
}

/// Read the cached extracted text for a given paper hash. Returns an empty
/// result with `cached: false` when the cache miss is expected (no prior run).
#[tauri::command]
pub async fn read_cached_paper_text(paper_hash: String) -> Result<serde_json::Value, String> {
    validate_paper_hash(&paper_hash)?;
    let dir = paper_cache_dir()?;
    let path = dir.join(format!("{paper_hash}.txt"));
    if !path.exists() {
        return Ok(serde_json::json!({ "cached": false, "text": "" }));
    }
    let file = crate::safety::open_regular_file(&path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let mut bytes = Vec::with_capacity(64 * 1024);
    file.take(MAX_RUN_CONTEXT_SIZE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let truncated = bytes.len() as u64 > MAX_RUN_CONTEXT_SIZE;
    if truncated {
        bytes.truncate(MAX_RUN_CONTEXT_SIZE as usize);
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Ok(serde_json::json!({
        "cached": true,
        "text": text,
        "truncated": truncated,
        "path": path.to_string_lossy()
    }))
}

// --- Managed local engines ---

/// Status of every installable engine. The disk-usage walk can touch
/// multi-GB trees, so it runs off the async runtime.
#[tauri::command]
pub async fn list_engines() -> Result<Vec<crate::engines::EngineStatus>, String> {
    tokio::task::spawn_blocking(crate::engines::engine_statuses)
        .await
        .map_err(|e| format!("Engine status task failed: {e}"))
}

#[tauri::command]
pub async fn install_engine(app: AppHandle, engine_id: String) -> Result<(), String> {
    crate::engines::install_engine(&app, &engine_id).await
}

#[tauri::command]
pub async fn uninstall_engine(app: AppHandle, engine_id: String) -> Result<(), String> {
    crate::engines::uninstall_engine(&app, &engine_id).await
}

#[tauri::command]
pub async fn retired_marker_status() -> Result<crate::engines::RetiredMarkerStatus, String> {
    tokio::task::spawn_blocking(crate::engines::retired_marker_status)
        .await
        .map_err(|e| format!("Retired Marker status task failed: {e}"))
}

#[tauri::command]
pub async fn remove_retired_marker(app: AppHandle) -> Result<(), String> {
    crate::engines::remove_retired_marker(&app).await
}

#[tauri::command]
pub fn cancel_engine_install() {
    crate::engines::cancel_install();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_test_config() -> PipelineConfig {
        PipelineConfig {
            steps: Vec::new(),
            merge: Default::default(),
            context_cache: Default::default(),
            use_orientation: false,
            orientation_prompt: String::new(),
            extraction: Default::default(),
            parallel_context_template: String::new(),
            variables: Vec::new(),
        }
    }

    fn test_snapshot(config: PipelineConfig) -> RunSnapshot {
        RunSnapshot {
            settings: crate::settings::Settings::default(),
            config,
            profile_name: "test".to_string(),
            config_fingerprint: "test-config".to_string(),
            fingerprint: "test-runtime".to_string(),
        }
    }

    #[test]
    fn batch_and_watch_accept_legacy_default_document_mode() {
        let mut config = empty_test_config();
        config.extraction.input_mode.clear();
        assert!(require_document_input(&test_snapshot(config), "Batch processing").is_ok());

        let mut config = empty_test_config();
        config.extraction.input_mode = "document".to_string();
        assert!(require_document_input(&test_snapshot(config), "Folder watching").is_ok());

        for mode in ["folder", "none"] {
            let mut config = empty_test_config();
            config.extraction.input_mode = mode.to_string();
            assert!(require_document_input(&test_snapshot(config), "Batch processing").is_err());
        }
    }

    #[test]
    fn primary_input_validation_enforces_explicit_mode_path_kinds() {
        let root = tempfile::tempdir().unwrap();
        let document = root.path().join("paper.pdf");
        std::fs::write(&document, b"fixture").unwrap();
        let document = document.to_str().unwrap();
        let folder = root.path().to_str().unwrap();

        let mut config = empty_test_config();
        config.extraction.input_mode.clear();
        assert!(validate_primary_input_path(&config, Some(document)).is_ok());
        assert!(validate_primary_input_path(&config, Some(folder)).is_ok());

        config.extraction.input_mode = "document".to_string();
        assert!(validate_primary_input_path(&config, Some(document)).is_ok());
        assert!(validate_primary_input_path(&config, Some(folder)).is_err());

        config.extraction.input_mode = "folder".to_string();
        assert!(validate_primary_input_path(&config, Some(folder)).is_ok());
        assert!(validate_primary_input_path(&config, Some(document)).is_err());

        config.extraction.input_mode = "none".to_string();
        assert!(validate_primary_input_path(&config, Some("")).is_ok());
        assert!(validate_primary_input_path(&config, Some(document)).is_err());

        config.extraction.input_mode = "document".to_string();
        assert!(validate_primary_input_path(&config, Some("")).is_err());
        assert!(validate_primary_input_path(&config, None).is_ok());
    }

    #[test]
    fn named_input_validation_matches_backend_extraction_contract() {
        let root = tempfile::tempdir().unwrap();
        let document = root.path().join("APPENDIX.PDF");
        let unsupported = root.path().join("notes.txt");
        std::fs::write(&document, b"fixture").unwrap();
        std::fs::write(&unsupported, b"fixture").unwrap();
        let folder = root.path().join("sources");
        std::fs::create_dir(&folder).unwrap();

        let mut config = empty_test_config();
        config.extraction.extra_inputs = vec![
            crate::pipeline_config::InputSlot {
                key: "appendix".to_string(),
                label: "Appendix".to_string(),
                mode: "document".to_string(),
                required: true,
            },
            crate::pipeline_config::InputSlot {
                key: "sources".to_string(),
                label: "Sources".to_string(),
                mode: "folder".to_string(),
                required: false,
            },
        ];

        assert!(validate_named_input_paths(&config, &Default::default(), false).is_ok());
        assert!(validate_named_input_paths(&config, &Default::default(), true).is_err());

        let valid = std::collections::HashMap::from([
            (
                "appendix".to_string(),
                document.to_string_lossy().to_string(),
            ),
            ("sources".to_string(), folder.to_string_lossy().to_string()),
        ]);
        assert!(validate_named_input_paths(&config, &valid, true).is_ok());

        let wrong_extension = std::collections::HashMap::from([(
            "appendix".to_string(),
            unsupported.to_string_lossy().to_string(),
        )]);
        assert!(validate_named_input_paths(&config, &wrong_extension, true).is_err());

        let wrong_document_kind = std::collections::HashMap::from([(
            "appendix".to_string(),
            folder.to_string_lossy().to_string(),
        )]);
        assert!(validate_named_input_paths(&config, &wrong_document_kind, true).is_err());

        let wrong_folder_kind = std::collections::HashMap::from([
            (
                "appendix".to_string(),
                document.to_string_lossy().to_string(),
            ),
            (
                "sources".to_string(),
                document.to_string_lossy().to_string(),
            ),
        ]);
        assert!(validate_named_input_paths(&config, &wrong_folder_kind, true).is_err());
    }

    #[test]
    fn headless_run_future_stays_behind_scheduler_boundary() {
        let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
        let future = run_headless(bus, "", Default::default(), Default::default());
        let size = std::mem::size_of_val(&future);
        assert!(
            size <= 8 * 1024,
            "run_headless future grew to {size} bytes; keep orchestration behind PipelineTask"
        );
    }

    #[test]
    fn runtime_snapshot_identity_is_order_stable_and_value_sensitive() {
        let snapshot = || RunSnapshot {
            settings: crate::settings::Settings::default(),
            config: empty_test_config(),
            profile_name: "test".to_string(),
            config_fingerprint: "profile-base".to_string(),
            fingerprint: "profile-base".to_string(),
        };
        let variables_a = std::collections::HashMap::from([
            ("alpha".to_string(), "one".to_string()),
            ("beta".to_string(), "two".to_string()),
        ]);
        let variables_b = std::collections::HashMap::from([
            ("beta".to_string(), "two".to_string()),
            ("alpha".to_string(), "one".to_string()),
        ]);
        let inputs = std::collections::HashMap::from([(
            "rubric".to_string(),
            "/tmp/rubric.pdf".to_string(),
        )]);
        let first = bind_runtime_snapshot(snapshot(), &variables_a, &inputs)
            .unwrap()
            .fingerprint;
        let reordered = bind_runtime_snapshot(snapshot(), &variables_b, &inputs)
            .unwrap()
            .fingerprint;
        assert_eq!(first, reordered);

        let changed = bind_runtime_snapshot(
            snapshot(),
            &std::collections::HashMap::from([
                ("alpha".to_string(), "changed".to_string()),
                ("beta".to_string(), "two".to_string()),
            ]),
            &inputs,
        )
        .unwrap()
        .fingerprint;
        assert_ne!(first, changed);

        let first_launch = bind_foreground_launch(
            bind_runtime_snapshot(snapshot(), &variables_a, &inputs).unwrap(),
            "/papers/a.pdf",
            false,
        )
        .unwrap()
        .fingerprint;
        let other_path = bind_foreground_launch(
            bind_runtime_snapshot(snapshot(), &variables_a, &inputs).unwrap(),
            "/papers/b.pdf",
            false,
        )
        .unwrap()
        .fingerprint;
        let other_diff = bind_foreground_launch(
            bind_runtime_snapshot(snapshot(), &variables_a, &inputs).unwrap(),
            "/papers/a.pdf",
            true,
        )
        .unwrap()
        .fingerprint;
        assert_ne!(first_launch, other_path);
        assert_ne!(first_launch, other_diff);
    }

    #[test]
    fn snapshot_fingerprint_tracks_secrets_without_exposing_them() {
        let mut settings = crate::settings::Settings {
            openai_api_key: "raw-secret-one".to_string(),
            ..Default::default()
        };
        let first = settings_for_snapshot_fingerprint(&settings);
        let serialized = serde_json::to_string(&first).unwrap();
        assert!(!serialized.contains("raw-secret-one"));
        assert!(first.openai_api_key.starts_with("<digest:"));

        settings.openai_api_key = "raw-secret-two".to_string();
        let second = settings_for_snapshot_fingerprint(&settings);
        assert_ne!(first.openai_api_key, second.openai_api_key);
    }

    #[test]
    fn basename_extracts_filename() {
        assert_eq!(basename("/papers/main.pdf"), "main.pdf");
        assert_eq!(basename("relative.tex"), "relative.tex");
        assert_eq!(basename(""), "");
    }

    #[test]
    fn model_readable_temp_files_exclude_span_markup() {
        let root = tempfile::tempdir().unwrap();
        let (file, _) = write_run_input_file(
            root.path(),
            "context_",
            ".md",
            r#"<span id="page-1">Readable text</span>"#,
            "test context",
        )
        .unwrap();
        let content = std::fs::read_to_string(file.path()).unwrap();

        assert_eq!(content, "Readable text");
    }

    #[test]
    fn print_html_is_self_contained_and_waits_for_fonts() {
        let html = build_print_report_html(
            "An equation: $y=x$.\n\n![Remote figure](https://example.invalid/pixel.png)",
        )
        .unwrap();
        assert!(html.contains("data:font/woff2;base64,"));
        assert!(html.contains("document.fonts.ready"));
        assert!(html.contains("Remote figure"));
        assert!(!html.contains("https://example.invalid/pixel.png"));
        assert!(!html.contains("url(fonts/"));
        assert!(!html.contains("cdn.jsdelivr"));
        assert!(!html.contains("<script src="));
        assert!(!html.contains("<link rel=\"stylesheet\""));
    }

    #[test]
    fn directory_export_is_complete_and_does_not_replace_existing_output() {
        let source = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(source.path().join("context")).unwrap();
        std::fs::write(source.path().join("manifest.json"), b"manifest").unwrap();
        std::fs::write(source.path().join("context/report.md"), b"report").unwrap();
        let destination = tempfile::tempdir().unwrap();
        let existing = destination.path().join("pipeline-run-test");
        std::fs::create_dir(&existing).unwrap();
        std::fs::write(existing.join("keep.txt"), b"keep").unwrap();
        let stage = create_export_stage(destination.path()).unwrap();

        let (files, bytes) = copy_export_tree(source.path(), stage.path()).unwrap();
        let exported =
            finish_export_stage(&stage, destination.path(), "pipeline-run-test").unwrap();
        drop(stage);

        assert_eq!(files, 2);
        assert_eq!(bytes, 14);
        assert_ne!(exported, existing);
        assert_eq!(std::fs::read(existing.join("keep.txt")).unwrap(), b"keep");
        assert_eq!(
            std::fs::read(exported.join("context/report.md")).unwrap(),
            b"report"
        );
    }

    #[test]
    fn rerun_cache_preserves_every_output_for_a_base_step() {
        let outputs = vec![
            crate::models::StepOutput {
                step_id: "review/claude".into(),
                raw_text: "one".into(),
                ..Default::default()
            },
            crate::models::StepOutput {
                step_id: "review/gemini".into(),
                raw_text: "two".into(),
                ..Default::default()
            },
        ];
        let cache = collect_preloaded_outputs(&outputs, &Default::default());
        assert_eq!(cache["review"].len(), 2);
    }

    #[test]
    fn composite_failure_ids_normalize_to_the_base_step() {
        let failures = vec![crate::models::StepFailure {
            step_id: "review/gemini".into(),
            step_label: "Review (Gemini)".into(),
            error: "failed".into(),
        }];
        let seeds = failed_base_ids(&failures);
        assert_eq!(seeds, ["review".to_string()].into_iter().collect());
    }

    #[test]
    fn resume_starts_at_the_first_missing_step_after_recovery() {
        let report = PipelineReport {
            orientation: serde_json::Value::Null,
            step_outputs: vec![crate::models::StepOutput {
                step_id: "technical".into(),
                raw_text: "complete".into(),
                ..Default::default()
            }],
            failed_steps: vec![crate::models::StepFailure {
                step_id: "__run_cancelled__".into(),
                step_label: "Run cancelled".into(),
                error: "cancelled".into(),
            }],
            referee_reports: Vec::new(),
            editor: None,
            report_date: chrono::Local::now().date_naive(),
            paper_hash: "test".into(),
        };
        let enabled = vec![
            "technical".to_string(),
            "empirical".to_string(),
            "synthesis".to_string(),
        ];
        let seeds = resume_seed_ids(&enabled, &report);
        assert!(!seeds.contains("technical"));
        assert!(seeds.contains("empirical"));
        assert!(seeds.contains("synthesis"));
    }

    #[test]
    fn chunked_import_limit_is_enforced_before_appending() {
        let mut buffer = vec![0u8; 8];
        assert!(append_limited(&mut buffer, &[1, 2], 10).is_ok());
        assert_eq!(buffer.len(), 10);
        assert!(append_limited(&mut buffer, &[3], 10).is_err());
        assert_eq!(buffer.len(), 10);
    }

    #[test]
    fn profile_url_import_rejects_non_public_destinations() {
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.0.1",
            "169.254.1.1",
            "100.64.0.1",
            "192.0.2.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.1",
            "::1",
            "fe80::1",
            "fc00::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
        ] {
            let address = address.parse().unwrap();
            assert!(!import_ip_is_public(address), "{address}");
        }
        for address in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
            let address = address.parse().unwrap();
            assert!(import_ip_is_public(address), "{address}");
        }
    }

    #[test]
    fn profile_url_import_rejects_credentials_and_localhost() {
        for url in [
            "file:///tmp/profile.json",
            "http://example.com/profile.json",
            "http://localhost/profile.json",
            "http://worker.localhost/profile.json",
            "https://user:secret@example.com/profile.json",
        ] {
            let parsed = reqwest::Url::parse(url).unwrap();
            assert!(validate_import_url_shape(parsed).is_err(), "{url}");
        }
        let normalized = validate_import_url_shape(
            reqwest::Url::parse("https://example.com./profile.json#fragment").unwrap(),
        )
        .unwrap();
        assert_eq!(normalized.host_str(), Some("example.com"));
        assert!(normalized.fragment().is_none());
    }

    #[test]
    fn watch_history_is_bounded_to_recent_jobs() {
        let mut state = WatchStatus::default();
        for index in 0..(MAX_WATCH_HISTORY + 5) {
            push_watch_job(
                &mut state,
                BatchJob {
                    path: index.to_string(),
                    name: index.to_string(),
                    status: if index == 1 { "failed" } else { "done" }.into(),
                    run_id: None,
                    error: None,
                    duration_secs: 0,
                    profile_id: String::new(),
                    profile_snapshot_id: String::new(),
                },
            );
        }
        assert_eq!(state.processed.len(), MAX_WATCH_HISTORY);
        assert_eq!(state.processed.first().unwrap().path, "5");
        assert_eq!(state.processed_total, (MAX_WATCH_HISTORY + 5) as u64);
        assert_eq!(state.failed_total, 1);
    }

    #[test]
    fn stopping_watch_clears_live_state_but_retains_history() {
        let mut state = WatchStatus {
            active: true,
            paused: true,
            error: Some("old failure".to_string()),
            folder: "/papers/inbox".to_string(),
            profile_id: "profile-a".to_string(),
            profile_snapshot_id: "snapshot-a".to_string(),
            processed: vec![BatchJob {
                path: "/papers/inbox/paper.pdf".to_string(),
                name: "paper.pdf".to_string(),
                status: "done".to_string(),
                run_id: Some("run-a".to_string()),
                error: None,
                duration_secs: 1,
                profile_id: "profile-a".to_string(),
                profile_snapshot_id: "snapshot-a".to_string(),
            }],
            processed_total: 1,
            failed_total: 0,
        };

        mark_watch_stopped(&mut state);

        assert!(!state.active);
        assert!(!state.paused);
        assert!(state.error.is_none());
        assert!(state.profile_id.is_empty());
        assert!(state.profile_snapshot_id.is_empty());
        assert_eq!(state.folder, "/papers/inbox");
        assert_eq!(state.processed.len(), 1);
        assert_eq!(state.processed_total, 1);
    }

    // Exercises the batch job-status helpers over the global BATCH state. This
    // is the only test that touches BATCH, so parallel test runs can't race it.
    #[test]
    fn batch_helpers_update_and_cancel_jobs() {
        {
            let mut jobs = BATCH.lock().unwrap();
            *jobs = vec![
                BatchJob {
                    path: "a".into(),
                    name: "a".into(),
                    status: "done".into(),
                    run_id: None,
                    error: None,
                    duration_secs: 0,
                    profile_id: String::new(),
                    profile_snapshot_id: String::new(),
                },
                BatchJob {
                    path: "b".into(),
                    name: "b".into(),
                    status: "running".into(),
                    run_id: None,
                    error: None,
                    duration_secs: 0,
                    profile_id: String::new(),
                    profile_snapshot_id: String::new(),
                },
                BatchJob {
                    path: "c".into(),
                    name: "c".into(),
                    status: "pending".into(),
                    run_id: None,
                    error: None,
                    duration_secs: 0,
                    profile_id: String::new(),
                    profile_snapshot_id: String::new(),
                },
            ];
        }
        set_job(1, |j| {
            j.status = "done".into();
            j.run_id = Some("r1".into());
        });
        mark_remaining_cancelled(1);
        let jobs = BATCH.lock().unwrap();
        assert_eq!(jobs[0].status, "done"); // untouched
        assert_eq!(jobs[1].status, "done"); // set_job ran before cancel; already terminal
        assert_eq!(jobs[1].run_id.as_deref(), Some("r1"));
        assert_eq!(jobs[2].status, "cancelled"); // pending → cancelled
    }
}
