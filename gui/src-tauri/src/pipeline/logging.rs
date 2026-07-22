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
use std::io::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub const MAX_CLI_LINE_BYTES: usize = 8 * 1024 * 1024;

pub struct BoundedLine {
    pub text: String,
    pub truncated: bool,
}

/// Read one newline-delimited record without allowing `lines()` to allocate an
/// attacker-controlled line before the caller's total-output cap can run.
pub async fn next_bounded_line<R>(
    reader: &mut R,
    limit: usize,
) -> std::io::Result<Option<BoundedLine>>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    use tokio::io::AsyncBufReadExt as _;
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    let mut truncated = false;
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return if bytes.is_empty() && !truncated {
                Ok(None)
            } else {
                Ok(Some(BoundedLine {
                    text: String::from_utf8_lossy(&bytes).into_owned(),
                    truncated,
                }))
            };
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |position| position + 1);
        let content_len = newline.unwrap_or(available.len());
        let remaining = limit.saturating_sub(bytes.len());
        let keep = content_len.min(remaining);
        bytes.extend_from_slice(&available[..keep]);
        truncated |= keep < content_len;
        reader.consume(consumed);
        if newline.is_some() {
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
            return Ok(Some(BoundedLine {
                text: String::from_utf8_lossy(&bytes).into_owned(),
                truncated,
            }));
        }
    }
}

/// The active log session: a process-unique id plus the call's display label
/// (e.g. "Orientation map", "Step: Technical (Claude)").
#[derive(Clone)]
pub struct LogSession {
    pub id: u64,
    pub label: String,
}

/// Token usage for one LLM call (or accumulated across several).
#[derive(Clone, Copy, Default, Debug, serde::Serialize, serde::Deserialize)]
pub struct CallUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

impl CallUsage {
    pub fn add(&mut self, input: u64, output: u64) {
        self.input_tokens += input;
        self.output_tokens += output;
    }
}

tokio::task_local! {
    static LOG_SESSION: LogSession;
    // Set by `measure_usage`; `emit_usage` folds each call's tokens into it so a
    // caller can read back exactly what one step consumed without threading the
    // count through `call_llm`'s return type.
    static USAGE_ACCUM: Arc<Mutex<CallUsage>>;
    // The pipeline pass (step key, e.g. "technical/claude") currently running on
    // this task. `register_child_pid` reads it so a specific pass's subprocess
    // can be cancelled by name. Set by the executor around each `call_llm`.
    static PASS_KEY: String;
}

/// Run `fut` tagged with a pass key so child processes it spawns can be
/// attributed to (and cancelled by) that pass.
pub async fn with_pass<F: Future>(key: String, fut: F) -> F::Output {
    PASS_KEY.scope(key, fut).await
}

/// The pass key active on the current task, if any.
pub fn current_pass() -> Option<String> {
    PASS_KEY.try_with(|k| k.clone()).ok()
}

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

/// Run-wide token total. Reset at the start of each run; every `emit_usage`
/// call adds to it, so the manifest can record the whole run's usage (steps
/// plus orientation, merge, extraction — everything, not just the steps).
static RUN_USAGE: Mutex<CallUsage> = Mutex::new(CallUsage {
    input_tokens: 0,
    output_tokens: 0,
});

/// A file the console log is mirrored to for the duration of a run. `emit`
/// appends every line here (timestamped) in addition to sending the event, so
/// the run directory keeps a persistent transcript. Set after the run dir is
/// created, cleared when the run ends.
static LOG_SINK: Mutex<Option<std::fs::File>> = Mutex::new(None);

/// Reset the run-wide token total. Call once at the start of a run.
pub fn reset_run_usage() {
    *RUN_USAGE.lock().unwrap_or_else(|e| e.into_inner()) = CallUsage::default();
}

/// The run-wide token total accumulated since the last `reset_run_usage`.
pub fn run_usage() -> CallUsage {
    *RUN_USAGE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Direct the console transcript to `file` (or `None` to stop mirroring).
pub fn set_log_sink(file: Option<std::fs::File>) {
    *LOG_SINK.lock().unwrap_or_else(|e| e.into_inner()) = file;
}

/// Run `fut` while accumulating the token usage of every `call_llm` it makes,
/// returning the future's output alongside the total. Nesting is fine (the
/// task-locals are independent), but the accumulator only sees calls polled on
/// the same task — the executor uses it around a single `call_llm`.
pub async fn measure_usage<F: Future>(fut: F) -> (F::Output, CallUsage) {
    let cell = Arc::new(Mutex::new(CallUsage::default()));
    let out = USAGE_ACCUM.scope(cell.clone(), fut).await;
    let usage = *cell.lock().unwrap_or_else(|e| e.into_inner());
    (out, usage)
}

/// Allocate a fresh, process-unique session id.
pub fn next_session_id() -> u64 {
    NEXT_SESSION.fetch_add(1, Ordering::Relaxed)
}

/// Run `fut` with an active log session. Every `emit()` call made while this
/// future is polled on the current task is tagged with `id` and `label`.
pub async fn with_session<F: Future>(id: u64, label: impl Into<String>, fut: F) -> F::Output {
    LOG_SESSION
        .scope(
            LogSession {
                id,
                label: label.into(),
            },
            fut,
        )
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
    } else if line.starts_with("[out]") || line.starts_with("[codex]") || line.starts_with("[api]")
    {
        "stdout"
    } else {
        "info"
    }
}

/// Fold a call's token usage into the active per-call accumulator (if any) and
/// the run-wide total. Split out from `emit_usage` so it can be tested without
/// an `AppHandle`.
fn record_usage(input_tokens: u64, output_tokens: u64) {
    let _ = USAGE_ACCUM.try_with(|cell| {
        cell.lock()
            .unwrap_or_else(|e| e.into_inner())
            .add(input_tokens, output_tokens);
    });
    RUN_USAGE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .add(input_tokens, output_tokens);
}

/// Emit a `pipeline:usage` event with token counts for the current call,
/// tagged with the active session. Calls with no token data (both zero) are
/// skipped — CLI text-mode providers can't report usage.
pub fn emit_usage(app: &crate::emit::EventBus, input_tokens: u64, output_tokens: u64) {
    if input_tokens == 0 && output_tokens == 0 {
        return;
    }
    // Fold into the per-call accumulator (if a `measure_usage` scope is active)
    // and the run-wide total, before emitting the event.
    record_usage(input_tokens, output_tokens);
    let (session, label) = match current() {
        Some(s) => (Some(s.id), Some(s.label)),
        None => (None, None),
    };
    app.emit_event(
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

/// Emit a `pipeline:log` event tagged with the current session (if any), and
/// mirror it to the run's on-disk transcript when a sink is set.
pub fn emit(app: &crate::emit::EventBus, line: String) {
    let (session, label) = match current() {
        Some(s) => (Some(s.id), Some(s.label)),
        None => (None, None),
    };
    let level = classify(&line);
    write_to_sink(&line, level, session, label.as_deref());
    app.emit_event(
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

/// Append one line to the on-disk transcript, if mirroring is active. Best
/// effort: a write failure is dropped silently so logging never breaks a run.
fn write_to_sink(line: &str, level: &str, session: Option<u64>, label: Option<&str>) {
    let mut guard = LOG_SINK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(file) = guard.as_mut() {
        let ts = chrono::Local::now().format("%H:%M:%S");
        let tag = match (session, label) {
            (Some(id), Some(l)) => format!(" [{id}:{l}]"),
            (Some(id), None) => format!(" [{id}]"),
            _ => String::new(),
        };
        let _ = writeln!(file, "[{ts}] [{level}]{tag} {line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_by_prefix() {
        assert_eq!(classify("ERROR: boom"), "error");
        assert_eq!(classify("WARNING: hmm"), "warn");
        assert_eq!(classify("[stderr] noise"), "stderr");
        assert_eq!(classify("[out] progress"), "stdout");
        assert_eq!(classify("just info"), "info");
    }

    #[test]
    fn bounded_line_reader_drains_without_overallocating() {
        use tokio::io::AsyncWriteExt as _;
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let (reader, mut writer) = tokio::io::duplex(32);
                let writer_task = tokio::spawn(async move {
                    writer.write_all(&[b'x'; 200]).await.unwrap();
                    writer.write_all(b"\nnext\n").await.unwrap();
                });
                let mut reader = tokio::io::BufReader::new(reader);
                let first = next_bounded_line(&mut reader, 16).await.unwrap().unwrap();
                assert_eq!(first.text.len(), 16);
                assert!(first.truncated);
                let second = next_bounded_line(&mut reader, 16).await.unwrap().unwrap();
                assert_eq!(second.text, "next");
                assert!(!second.truncated);
                writer_task.await.unwrap();
            });
    }

    // These assert the task-local per-call accumulator, which is isolated per
    // test task. The RUN_USAGE global is deliberately not asserted here because
    // tests run in parallel and share it. (tokio's `macros` feature isn't
    // enabled, so we drive the futures on a small current-thread runtime.)
    fn block_on<F: Future>(fut: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(fut)
    }

    #[test]
    fn measure_usage_sums_calls_in_scope() {
        let (_, usage) = block_on(measure_usage(async {
            record_usage(100, 20);
            record_usage(50, 10);
        }));
        assert_eq!(usage.input_tokens, 150);
        assert_eq!(usage.output_tokens, 30);
    }

    #[test]
    fn measure_usage_zero_when_no_calls() {
        let (_, usage) = block_on(measure_usage(async {}));
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
    }

    #[test]
    fn record_usage_outside_scope_does_not_panic() {
        // No active accumulator — must be a no-op for the task-local, and still
        // fold into the run total.
        record_usage(1, 1);
    }
}
