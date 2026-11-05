//! Per-run execution scope.
//!
//! A run owns its cancellation state, the child processes it spawned, its
//! console transcript sink, its token totals, its frozen settings snapshot, and
//! its API write quota. These used to be process globals, which limited
//! Pipeline to one run at a time. Each run now executes inside a [`RunScope`]
//! carried as a tokio task-local, and as a thread-local inside blocking work
//! started with [`spawn_blocking`], so several runs can share the process.
//!
//! Task-locals do not propagate across `tokio::spawn`, `JoinSet::spawn`, or onto
//! blocking threads. Run code therefore spawns through [`spawn`], [`spawn_in`],
//! and [`spawn_blocking`], which re-enter the caller's scope. Code outside any
//! run (UI commands, model discovery, maintenance under the exclusive pipeline
//! guard) resolves to one shared fallback scope that behaves like the former
//! globals.

use std::cell::RefCell;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use crate::pipeline::logging::CallUsage;
use crate::settings::Settings;

pub struct RunScope {
    key: String,
    cancelled: AtomicBool,
    /// Advances on every run or pass cancellation, so waiters never miss a
    /// wakeup that races with their subscription.
    epoch: AtomicU64,
    signal: tokio::sync::watch::Sender<u64>,
    child_pids: Mutex<Vec<u32>>,
    /// (pass key, pid) for per-pass cancellation.
    pass_pids: Mutex<Vec<(String, u32)>>,
    /// Pass keys the user cancelled in this run. The executor's retry loop
    /// checks them so a cancelled pass fails instead of retrying.
    cancelled_passes: Mutex<Vec<String>>,
    usage: Mutex<CallUsage>,
    log_sink: Mutex<Option<std::fs::File>>,
    settings: Mutex<Option<(u64, Settings)>>,
    pub(crate) write_count: AtomicUsize,
    pub(crate) write_bytes: AtomicUsize,
    /// The Claude Code executable whose subscription sign-in this run verified
    /// on its first Claude call. Later calls in the run reuse it.
    claude_subscription: tokio::sync::OnceCell<crate::deps::ResolvedCommand>,
}

tokio::task_local! {
    static SCOPE: Arc<RunScope>;
}

thread_local! {
    static BLOCKING_SCOPE: RefCell<Option<Arc<RunScope>>> = const { RefCell::new(None) };
}

static FALLBACK: LazyLock<Arc<RunScope>> = LazyLock::new(|| Arc::new(RunScope::new("shared")));
static ACTIVE: Mutex<Vec<Arc<RunScope>>> = Mutex::new(Vec::new());
static SETTINGS_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// The scope of the run executing on this task or blocking thread, or the
/// shared fallback outside any run.
pub fn current() -> Arc<RunScope> {
    SCOPE
        .try_with(Arc::clone)
        .ok()
        .or_else(|| BLOCKING_SCOPE.with(|cell| cell.borrow().clone()))
        .unwrap_or_else(|| FALLBACK.clone())
}

/// Every registered run scope plus the shared fallback.
pub fn all() -> Vec<Arc<RunScope>> {
    let mut scopes = lock(&ACTIVE).clone();
    scopes.push(FALLBACK.clone());
    scopes
}

/// The registered run scope with this key, if it is still active.
pub fn find(key: &str) -> Option<Arc<RunScope>> {
    lock(&ACTIVE).iter().find(|scope| scope.key == key).cloned()
}

/// Run `fut` inside `scope`.
pub async fn scoped<F: Future>(scope: Arc<RunScope>, fut: F) -> F::Output {
    SCOPE.scope(scope, fut).await
}

/// `tokio::spawn` that keeps the caller's run scope.
pub fn spawn<F>(fut: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(SCOPE.scope(current(), fut))
}

/// `JoinSet::spawn` that keeps the caller's run scope.
pub fn spawn_in<T, F>(tasks: &mut tokio::task::JoinSet<T>, fut: F) -> tokio::task::AbortHandle
where
    T: Send + 'static,
    F: Future<Output = T> + Send + 'static,
{
    tasks.spawn(SCOPE.scope(current(), fut))
}

/// The next finished unit of a `JoinSet`, or `None` when none remain. Once the
/// run is cancelled, units have `grace` to stop on their own and report what
/// they spent, counted from the first wait that sees the cancellation (held in
/// `stop_by`). `Err` means a unit outlasted it: a unit that ignores
/// cancellation must not hold the run open, so the caller stops the rest.
pub async fn join_next_until_stopped<T: 'static>(
    tasks: &mut tokio::task::JoinSet<T>,
    stop_by: &mut Option<tokio::time::Instant>,
    grace: std::time::Duration,
) -> Result<Option<Result<T, tokio::task::JoinError>>, String> {
    let deadline = match *stop_by {
        Some(deadline) => deadline,
        None => match crate::commands::await_or_cancel(tasks.join_next(), None).await {
            Ok(next) => return Ok(next),
            Err(_) => *stop_by.insert(tokio::time::Instant::now() + grace),
        },
    };
    tokio::time::timeout_at(deadline, tasks.join_next())
        .await
        .map_err(|_| "Pipeline cancelled".to_string())
}

/// `spawn_blocking` that keeps the caller's run scope, so cancellation checks,
/// child registration, and settings reads inside the closure see this run.
pub fn spawn_blocking<F, R>(f: F) -> tokio::task::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let scope = current();
    tokio::task::spawn_blocking(move || enter_blocking(scope, f))
}

/// Run blocking work on this thread inside `scope`, restoring the previous
/// thread scope afterwards (including on panic).
pub fn enter_blocking<R>(scope: Arc<RunScope>, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<Arc<RunScope>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            BLOCKING_SCOPE.with(|cell| *cell.borrow_mut() = previous);
        }
    }
    let previous = BLOCKING_SCOPE.with(|cell| cell.borrow_mut().replace(scope));
    let _restore = Restore(previous);
    f()
}

/// A registered scope. Dropping it removes the scope from the registry; the
/// scope itself lives on while running work still holds it.
pub struct ActiveScope(Arc<RunScope>);

impl ActiveScope {
    pub fn begin(key: impl Into<String>) -> Self {
        let scope = Arc::new(RunScope::new(key));
        lock(&ACTIVE).push(scope.clone());
        Self(scope)
    }

    pub fn scope(&self) -> &Arc<RunScope> {
        &self.0
    }
}

impl Drop for ActiveScope {
    fn drop(&mut self) {
        lock(&ACTIVE).retain(|scope| !Arc::ptr_eq(scope, &self.0));
    }
}

/// Stop tracking a child process after it exits. A PID belongs to exactly one
/// scope, but the exit can be observed from code running outside it.
pub fn forget_child_everywhere(pid: u32) {
    for scope in all() {
        scope.forget_child(pid);
    }
}

impl RunScope {
    fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            cancelled: AtomicBool::new(false),
            epoch: AtomicU64::new(0),
            signal: tokio::sync::watch::channel(0).0,
            child_pids: Mutex::new(Vec::new()),
            pass_pids: Mutex::new(Vec::new()),
            cancelled_passes: Mutex::new(Vec::new()),
            usage: Mutex::new(CallUsage::default()),
            log_sink: Mutex::new(None),
            settings: Mutex::new(None),
            write_count: AtomicUsize::new(0),
            write_bytes: AtomicUsize::new(0),
            claude_subscription: tokio::sync::OnceCell::new(),
        }
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn is_fallback(&self) -> bool {
        std::ptr::eq(self, Arc::as_ptr(&FALLBACK))
    }

    // --- Cancellation ---

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<u64> {
        self.signal.subscribe()
    }

    /// Wake every waiter so it re-checks run and pass cancellation.
    pub fn signal(&self) {
        let epoch = self.epoch.fetch_add(1, Ordering::AcqRel) + 1;
        self.signal.send_replace(epoch);
    }

    /// Mark the run cancelled and wake its waiters without killing children.
    pub fn request_cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.signal();
    }

    /// Cancel the run: stop its waiters and force-stop its child processes.
    pub fn cancel(&self) {
        self.request_cancel();
        self.kill_children();
    }

    /// Clear run cancellation. Only the shared fallback is reused across
    /// operations; a run scope stays cancelled once cancelled.
    pub fn reset_cancel(&self) {
        self.cancelled.store(false, Ordering::Release);
    }

    pub fn is_pass_cancelled(&self, pass_key: &str) -> bool {
        lock(&self.cancelled_passes).iter().any(|k| k == pass_key)
    }

    /// Cancel one pass: mark it (so it won't retry), wake waiters, and kill
    /// its subprocesses. Other passes in the wave keep running.
    pub fn cancel_pass(&self, pass_key: &str) {
        lock(&self.cancelled_passes).push(pass_key.to_string());
        self.signal();
        self.kill_pass_children(pass_key);
    }

    pub fn reset_pass_cancels(&self) {
        lock(&self.cancelled_passes).clear();
        lock(&self.pass_pids).clear();
    }

    // --- Child processes ---

    pub fn register_child(&self, pid: u32, pass: Option<String>) {
        lock(&self.child_pids).push(pid);
        if let Some(pass) = pass {
            lock(&self.pass_pids).push((pass.clone(), pid));
            // Close the race where cancel_pass observes no PID immediately
            // before this child is registered. A cancelled pass may never
            // acquire a new live subprocess after that decision committed.
            if self.is_pass_cancelled(&pass) {
                crate::commands::kill_process(pid);
            }
        }
    }

    pub fn forget_child(&self, pid: u32) {
        lock(&self.child_pids).retain(|&p| p != pid);
        lock(&self.pass_pids).retain(|(_, p)| *p != pid);
    }

    pub fn has_child(&self, pid: u32) -> bool {
        lock(&self.child_pids).contains(&pid)
    }

    pub fn kill_children(&self) {
        let pids = std::mem::take(&mut *lock(&self.child_pids));
        for pid in pids {
            crate::commands::kill_process(pid);
        }
    }

    pub fn kill_pass_children(&self, pass_key: &str) {
        let pids: Vec<u32> = lock(&self.pass_pids)
            .iter()
            .filter(|(k, _)| k == pass_key)
            .map(|(_, p)| *p)
            .collect();
        for pid in pids {
            crate::commands::kill_process(pid);
        }
    }

    // --- Usage and transcript ---

    pub fn reset_usage(&self) {
        *lock(&self.usage) = CallUsage::default();
    }

    pub fn add_usage(&self, usage: CallUsage) {
        lock(&self.usage).add_usage(usage);
    }

    pub fn usage(&self) -> CallUsage {
        *lock(&self.usage)
    }

    pub fn set_log_sink(&self, file: Option<std::fs::File>) {
        *lock(&self.log_sink) = file;
    }

    pub fn with_log_sink(&self, write: impl FnOnce(&mut std::fs::File)) {
        if let Some(file) = lock(&self.log_sink).as_mut() {
            write(file);
        }
    }

    // --- Settings snapshot and write quota ---

    /// Freeze `settings` as this run's snapshot; returns the token that
    /// releases exactly this snapshot.
    pub fn freeze_settings(&self, settings: Settings) -> u64 {
        let token = SETTINGS_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        *lock(&self.settings) = Some((token, settings));
        token
    }

    pub fn release_settings(&self, token: u64) {
        let mut snapshot = lock(&self.settings);
        if snapshot.as_ref().map(|(t, _)| *t) == Some(token) {
            *snapshot = None;
        }
    }

    pub fn frozen_settings(&self) -> Option<Settings> {
        lock(&self.settings).as_ref().map(|(_, s)| s.clone())
    }

    pub fn reset_write_budget(&self) {
        self.write_count.store(0, Ordering::SeqCst);
        self.write_bytes.store(0, Ordering::SeqCst);
    }

    // --- Provider sign-in ---

    /// Only meaningful on a run scope: the shared fallback is reused across
    /// operations, so callers must not cache a verification there.
    pub(crate) fn claude_subscription(
        &self,
    ) -> &tokio::sync::OnceCell<crate::deps::ResolvedCommand> {
        &self.claude_subscription
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A local OpenAI-compatible server that lists one model, then accepts
    /// every model call and never answers it. Returns its URL and the number
    /// of model calls received.
    pub(crate) fn hanging_provider() -> (String, Arc<AtomicUsize>) {
        use std::io::{BufRead, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        std::thread::spawn(move || {
            let mut unanswered = Vec::new();
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let mut reader = std::io::BufReader::new(stream);
                let mut request = String::new();
                if reader.read_line(&mut request).is_err() {
                    continue;
                }
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                }
                if request.starts_with("GET") {
                    let reply = r#"{"data":[{"id":"fixture"}]}"#;
                    let _ = write!(
                        reader.into_inner(),
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                        reply.len()
                    );
                } else {
                    counter.fetch_add(1, Ordering::SeqCst);
                    unanswered.push(reader);
                }
            }
        });
        (format!("http://{address}/v1"), calls)
    }

    /// Waits until `calls` reaches `count` on the given provider.
    pub(crate) async fn until_called(calls: &AtomicUsize, count: usize) {
        let started = std::time::Instant::now();
        while calls.load(Ordering::SeqCst) < count {
            assert!(
                started.elapsed() < std::time::Duration::from_secs(20),
                "the provider never received the call"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_on<F: Future>(fut: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(fut)
    }

    #[test]
    fn scopes_isolate_cancellation_children_and_usage() {
        let a = ActiveScope::begin("scope-test-a");
        let b = ActiveScope::begin("scope-test-b");
        // PIDs above i32::MAX are never signalled.
        let pid = u32::MAX - 7;
        block_on(scoped(a.scope().clone(), async {
            current().register_child(pid, Some("technical/claude".into()));
            current().add_usage(CallUsage {
                input_tokens: 5,
                ..Default::default()
            });
        }));
        assert!(a.scope().has_child(pid));
        assert!(!b.scope().has_child(pid));
        assert_eq!(a.scope().usage().input_tokens, 5);
        assert_eq!(b.scope().usage().input_tokens, 0);

        a.scope().cancel_pass("technical/claude");
        assert!(a.scope().is_pass_cancelled("technical/claude"));
        assert!(!b.scope().is_pass_cancelled("technical/claude"));

        a.scope().request_cancel();
        assert!(a.scope().is_cancelled());
        assert!(!b.scope().is_cancelled());

        forget_child_everywhere(pid);
        assert!(!a.scope().has_child(pid));
    }

    #[test]
    fn spawned_and_blocking_work_inherit_the_scope() {
        let run = ActiveScope::begin("scope-test-inherit");
        let key = run.scope().key().to_string();
        let (spawned, blocking) = block_on(scoped(run.scope().clone(), async {
            let spawned = spawn(async { current().key().to_string() }).await.unwrap();
            let blocking = spawn_blocking(|| current().key().to_string())
                .await
                .unwrap();
            (spawned, blocking)
        }));
        assert_eq!(spawned, key);
        assert_eq!(blocking, key);
        assert!(current().is_fallback());
    }

    #[test]
    fn join_set_work_inherits_the_scope() {
        let run = ActiveScope::begin("scope-test-join-set");
        let key = run.scope().key().to_string();
        let joined = block_on(scoped(run.scope().clone(), async {
            let mut tasks = tokio::task::JoinSet::new();
            spawn_in(&mut tasks, async { current().key().to_string() });
            tasks.join_next().await.unwrap().unwrap()
        }));
        assert_eq!(joined, key);
    }

    #[test]
    fn a_unit_that_ignores_cancellation_cannot_hold_the_run_open() {
        let run = ActiveScope::begin("scope-test-stubborn-unit");
        let grace = std::time::Duration::from_millis(50);
        block_on(scoped(run.scope().clone(), async {
            let mut tasks = tokio::task::JoinSet::new();
            spawn_in(&mut tasks, async { "finished" });
            spawn_in(&mut tasks, std::future::pending::<&str>());
            let mut stop_by = None;
            // Nothing stops a unit before the run is cancelled.
            let first = join_next_until_stopped(&mut tasks, &mut stop_by, grace)
                .await
                .unwrap();
            assert_eq!(first.unwrap().unwrap(), "finished");
            assert!(stop_by.is_none());

            current().request_cancel();
            let stopped = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                join_next_until_stopped(&mut tasks, &mut stop_by, grace),
            )
            .await
            .expect("the wait must end with the grace period");
            assert_eq!(stopped.unwrap_err(), "Pipeline cancelled");
            assert_eq!(tasks.len(), 1);
        }));
    }

    /// Every spawn site under these folders either re-enters the run scope or
    /// is named here with the reason it owns no run state. Reviews lost Stop,
    /// usage, and process ownership to a raw `JoinSet::spawn` once already.
    #[test]
    fn review_runtime_spawns_keep_the_run_scope() {
        const RAW_SPAWNS: &[&str] = &[
            "tokio::spawn(",
            "tokio::task::spawn(",
            "tokio::task::spawn_blocking(",
            "tauri::async_runtime::spawn(",
            "tauri::async_runtime::spawn_blocking(",
            "tasks.spawn(",
            "tasks.spawn_blocking(",
            ".spawn(async",
        ];
        // Each entry is one Review-reachable site outside any single run.
        const OWNED_OUTSIDE_A_RUN: &[(&str, usize)] = &[
            // The launch preview runs before a run exists.
            ("pipeline/executor/budget.rs", 1),
            // The shared ChatGPT connection outlives every run that uses it.
            ("pipeline/codex_server/connection.rs", 1),
            // Interrupting a dropped turn outlives its run; it reads no scope.
            ("pipeline/codex_server/invocation.rs", 1),
        ];
        fn visit(dir: &std::path::Path, found: &mut Vec<(String, usize)>, root: &std::path::Path) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if path.file_name().is_some_and(|name| name != "tests") {
                        visit(&path, found, root);
                    }
                    continue;
                }
                let name = path.file_name().unwrap().to_string_lossy();
                if !name.ends_with(".rs") || name.ends_with("tests.rs") {
                    continue;
                }
                let source = std::fs::read_to_string(&path).unwrap();
                // Test modules sit at the end of each file and may spawn freely.
                let production = source.split("#[cfg(test)]\nmod ").next().unwrap();
                let raw = production
                    .lines()
                    .filter(|line| !line.trim_start().starts_with("//"))
                    .filter(|line| RAW_SPAWNS.iter().any(|spawn| line.contains(spawn)))
                    .count();
                if raw > 0 {
                    let relative = path.strip_prefix(root).unwrap();
                    found.push((relative.to_string_lossy().replace('\\', "/"), raw));
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found = Vec::new();
        for module in ["pipeline", "commands/run", "runs", "document_bundle"] {
            visit(&root.join(module), &mut found, &root);
        }
        found.sort();
        let mut expected: Vec<(String, usize)> = OWNED_OUTSIDE_A_RUN
            .iter()
            .map(|(path, count)| (path.to_string(), *count))
            .collect();
        expected.sort();
        assert_eq!(found, expected);
    }

    #[test]
    fn registry_tracks_active_scopes() {
        let run = ActiveScope::begin("scope-test-registry");
        assert!(find("scope-test-registry").is_some());
        drop(run);
        assert!(find("scope-test-registry").is_none());
    }
}
