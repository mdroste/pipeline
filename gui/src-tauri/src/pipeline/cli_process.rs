//! Shared mechanics for non-interactive provider CLI subprocesses.
//!
//! Provider modules remain responsible for arguments and output parsing. This
//! module owns the common bounded stream capture, PID registration, and
//! timeout/termination behavior so those safety rules cannot drift apart.

use std::collections::VecDeque;
use std::process::ExitStatus;
use std::time::{Duration, Instant};
use tokio::io::BufReader;
use tokio::process::{Child, ChildStderr, ChildStdout};
use tokio::task::JoinHandle;

pub const MAX_STDOUT_BYTES: usize = 50_000_000;
const STDERR_TAIL_LINES: usize = 50;

pub fn log(app: &crate::emit::EventBus, line: impl Into<String>) {
    super::logging::emit(app, line.into());
}

pub fn verbose_log(app: &crate::emit::EventBus, line: impl Into<String>) {
    if crate::settings::load().verbose_logging {
        log(app, line);
    }
}

pub fn track_child_started(
    child: &Child,
    app: &crate::emit::EventBus,
    label: &str,
) -> (u32, Instant) {
    let pid = child.id().unwrap_or(0);
    if pid > 0 {
        crate::commands::register_child_pid(pid);
    }
    log(app, format!("{label} started (PID {pid})"));
    (pid, Instant::now())
}

pub fn capture_stderr(
    stderr: Option<ChildStderr>,
    app: crate::emit::EventBus,
    session: Option<super::logging::LogSession>,
) -> JoinHandle<Vec<String>> {
    tokio::spawn(super::logging::with_session_opt(session, async move {
        let mut tail = VecDeque::new();
        if let Some(stderr) = stderr {
            let mut reader = BufReader::new(stderr);
            while let Ok(Some(record)) =
                super::logging::next_bounded_line(&mut reader, super::logging::MAX_CLI_LINE_BYTES)
                    .await
            {
                let line = if record.truncated {
                    format!("{}… [line truncated]", record.text)
                } else {
                    record.text
                };
                if !line.trim().is_empty() {
                    verbose_log(&app, format!("[stderr] {line}"));
                    if tail.len() >= STDERR_TAIL_LINES {
                        tail.pop_front();
                    }
                    tail.push_back(line);
                }
            }
        }
        tail.into_iter().collect()
    }))
}

pub fn capture_text_stdout(
    stdout: Option<ChildStdout>,
    app: crate::emit::EventBus,
    session: Option<super::logging::LogSession>,
) -> JoinHandle<(String, bool)> {
    tokio::spawn(super::logging::with_session_opt(session, async move {
        let mut collected = String::new();
        let mut overflowed = false;
        let mut captured_lines = 0usize;
        if let Some(stdout) = stdout {
            let mut reader = BufReader::new(stdout);
            while let Ok(Some(record)) =
                super::logging::next_bounded_line(&mut reader, super::logging::MAX_CLI_LINE_BYTES)
                    .await
            {
                if record.truncated {
                    if !overflowed {
                        log(&app, "ERROR: provider stdout exceeded its safety limit");
                    }
                    overflowed = true;
                    continue;
                }
                let line = record.text;
                if !overflowed {
                    let needed = line.len().saturating_add(1);
                    if needed > MAX_STDOUT_BYTES.saturating_sub(collected.len()) {
                        log(
                            &app,
                            format!("ERROR: stdout exceeded {} MB", MAX_STDOUT_BYTES / 1_000_000),
                        );
                        overflowed = true;
                        continue;
                    }
                    collected.push_str(&line);
                    collected.push('\n');
                    captured_lines += 1;
                }
                if captured_lines <= 5 && !overflowed {
                    verbose_log(&app, format!("[out] {line}"));
                }
            }
        }
        if captured_lines > 5 {
            verbose_log(&app, format!("[out] ... ({captured_lines} captured lines)"));
        }
        (collected, overflowed)
    }))
}

pub async fn wait_for_child(
    child: &mut Child,
    pid: u32,
    timeout_secs: u64,
    provider: &str,
    binary: &str,
    label: &str,
    app: &crate::emit::EventBus,
) -> Result<ExitStatus, String> {
    match tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait()).await {
        Ok(result) => result.map_err(|error| format!("Failed waiting for {binary}: {error}")),
        Err(_) => {
            if pid > 0 {
                crate::commands::kill_process(pid);
            }
            let _ = child.kill().await;
            let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
            if crate::commands::is_cancelled() {
                log(app, format!("{label} cancelled"));
                return Err("Pipeline cancelled".into());
            }
            log(
                app,
                format!(
                    "ERROR: {provider} call timed out after {timeout_secs}s (PID {pid}), killing process"
                ),
            );
            Err(format!("{provider} call timed out after {timeout_secs}s"))
        }
    }
}

/// Drain both provider pipes after the leader exits. Descendants can inherit a
/// pipe and keep it open forever, so normal EOF gets a short grace period and
/// is then enforced by terminating the still-owned process group/job.
async fn wait_for_stream_tasks<A, B>(
    stdout_task: &JoinHandle<A>,
    stderr_task: &JoinHandle<B>,
    duration: Duration,
) -> bool {
    let deadline = tokio::time::Instant::now() + duration;
    loop {
        if stdout_task.is_finished() && stderr_task.is_finished() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

pub async fn finish_streams<A, B>(
    stdout_task: JoinHandle<A>,
    stderr_task: JoinHandle<B>,
    pid: u32,
    provider: &str,
) -> Result<(A, B), String>
where
    A: Send + 'static,
    B: Send + 'static,
{
    if !wait_for_stream_tasks(&stdout_task, &stderr_task, Duration::from_secs(1)).await {
        if pid > 0 {
            crate::commands::kill_process(pid);
        }
        if !wait_for_stream_tasks(&stdout_task, &stderr_task, Duration::from_secs(2)).await {
            stdout_task.abort();
            stderr_task.abort();
            if pid > 0 {
                crate::commands::unregister_child_pid(pid);
            }
            return Err(format!(
                "{provider} output pipes did not close after termination"
            ));
        }
    }
    if pid > 0 {
        crate::commands::unregister_child_pid(pid);
    }
    let stdout = stdout_task.await;
    let stderr = stderr_task.await;
    Ok((
        stdout.map_err(|error| format!("{provider} stdout reader failed: {error}"))?,
        stderr.map_err(|error| format!("{provider} stderr reader failed: {error}"))?,
    ))
}

pub fn last_stderr_hint(tail: &[String]) -> Option<String> {
    tail.iter()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
}

pub fn emit_stderr_tail(app: &crate::emit::EventBus, tail: &[String]) {
    if tail.is_empty() {
        return;
    }
    log(
        app,
        format!(
            "ERROR: captured stderr from failed call ({} line(s)):",
            tail.len()
        ),
    );
    for line in tail {
        log(app, format!("[stderr] {line}"));
    }
}
