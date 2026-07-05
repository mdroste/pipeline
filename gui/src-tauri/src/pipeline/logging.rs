//! Per-call log session tagging.
//!
//! Every `pipeline:log` event carries an optional `session` id and `label` so
//! the frontend can separate the interleaved output of concurrently running
//! headless LLM invocations. A tokio task-local holds the active session for
//! the duration of one `call_llm` invocation; the free `log()` helpers in each
//! provider module route through `emit()` here, which stamps the event with the
//! current session, its label, and a coarse severity `level`.
//!
//! Task-locals do not propagate across `tokio::spawn`, so the subprocess
//! stdout/stderr reader tasks capture the session with `current()` and re-enter
//! it via `with_session_opt()`. Log calls made outside any session scope
//! (extraction, orchestration) emit with `session: null` and land in the
//! frontend's shared "General" bucket / master view.

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter};

/// The active log session: a process-unique id plus the call's display label
/// (e.g. "Orientation map", "Step: Technical (Claude)").
#[derive(Clone)]
pub struct LogSession {
    pub id: u64,
    pub label: String,
}

tokio::task_local! {
    static LOG_SESSION: LogSession;
}

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

/// Allocate a fresh, process-unique session id.
pub fn next_session_id() -> u64 {
    NEXT_SESSION.fetch_add(1, Ordering::Relaxed)
}

/// Run `fut` with an active log session. Every `emit()` call made while this
/// future is polled on the current task is tagged with `id` and `label`.
pub async fn with_session<F: Future>(id: u64, label: impl Into<String>, fut: F) -> F::Output {
    LOG_SESSION
        .scope(LogSession { id, label: label.into() }, fut)
        .await
}

/// The session active on the current task, if any. Cheap to clone.
pub fn current() -> Option<LogSession> {
    LOG_SESSION.try_with(|s| s.clone()).ok()
}

/// Re-enter a captured session on a spawned task (task-locals do not propagate
/// across `tokio::spawn`). `None` runs the future unscoped.
pub async fn with_session_opt<F: Future>(session: Option<LogSession>, fut: F) -> F::Output {
    match session {
        Some(s) => LOG_SESSION.scope(s, fut).await,
        None => fut.await,
    }
}

/// Coarse severity for frontend coloring, derived from the message prefix so
/// existing call sites keep working unchanged.
fn classify(line: &str) -> &'static str {
    if line.starts_with("ERROR") {
        "error"
    } else if line.starts_with("WARNING") {
        "warn"
    } else if line.starts_with("[stderr]") {
        "stderr"
    } else if line.starts_with("[out]") || line.starts_with("[codex]") || line.starts_with("[api]") {
        "stdout"
    } else {
        "info"
    }
}

/// Emit a `pipeline:usage` event with token counts for the current call,
/// tagged with the active session. Calls with no token data (both zero) are
/// skipped — CLI text-mode providers can't report usage.
pub fn emit_usage(app: &AppHandle, input_tokens: u64, output_tokens: u64) {
    if input_tokens == 0 && output_tokens == 0 {
        return;
    }
    let (session, label) = match current() {
        Some(s) => (Some(s.id), Some(s.label)),
        None => (None, None),
    };
    app.emit(
        "pipeline:usage",
        serde_json::json!({
            "session": session,
            "label": label,
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
        }),
    )
    .ok();
}

/// Emit a `pipeline:log` event tagged with the current session (if any).
pub fn emit(app: &AppHandle, line: String) {
    let (session, label) = match current() {
        Some(s) => (Some(s.id), Some(s.label)),
        None => (None, None),
    };
    let level = classify(&line);
    app.emit(
        "pipeline:log",
        serde_json::json!({
            "line": line,
            "session": session,
            "label": label,
            "level": level,
        }),
    )
    .ok();
}
