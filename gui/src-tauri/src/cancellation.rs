//! Review cancellation and per-run state, independent of IPC wrappers.
/// Per-run cleanup. A batch holds `PipelineGuard` across several jobs, so each
/// job must close its own log sink and API read/write windows on every return
/// path, including extraction or provider failures.
pub(crate) struct RunStateGuard {
    scope: std::sync::Arc<crate::run_scope::RunScope>,
}

/// Reset per-job state in the current run scope. Cancellation is not reset: a
/// run scope stays cancelled once cancelled, so a stop that lands before the
/// job starts cannot be erased here.
pub(crate) fn begin_run_state() -> RunStateGuard {
    let scope = crate::run_scope::current();
    if scope.is_fallback() {
        scope.reset_cancel();
    }
    scope.reset_pass_cancels();
    scope.reset_usage();
    scope.set_log_sink(None);
    scope.reset_write_budget();
    RunStateGuard { scope }
}

impl Drop for RunStateGuard {
    fn drop(&mut self) {
        self.scope.set_log_sink(None);
        self.scope.reset_pass_cancels();
    }
}

/// Whether a specific pass (step key, e.g. "technical/claude") of the current
/// run was cancelled by the user.
pub fn is_pass_cancelled(pass_key: &str) -> bool {
    crate::run_scope::current().is_pass_cancelled(pass_key)
}

/// Whether the run executing on this task was cancelled.
pub fn is_cancelled() -> bool {
    crate::run_scope::current().is_cancelled()
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

/// Resolve when the current run or the supplied pass is cancelled. A watch
/// channel avoids lost wakeups if cancellation races with request dispatch.
pub async fn wait_for_cancellation(pass_key: Option<&str>) {
    let scope = crate::run_scope::current();
    let mut signal = scope.subscribe();
    loop {
        if scope.is_cancelled() || pass_key.is_some_and(|key| scope.is_pass_cancelled(key)) {
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

pub(crate) fn stop_scopes(scopes: Vec<std::sync::Arc<crate::run_scope::RunScope>>) {
    for scope in scopes {
        scope.cancel();
    }
}

/// Cancel one pass: mark it cancelled (so it won't retry) and kill its
/// subprocesses. Other passes in the wave keep running. `run_key` names the
/// run; without it the pass is cancelled in the calling run, or, from outside
/// any run, in every active run (pass keys repeat across runs).
pub async fn cancel_pass(pass_key: String, run_key: Option<String>) -> Result<(), String> {
    let targets = match run_key {
        Some(key) => {
            vec![crate::run_scope::find(&key).ok_or("That review is no longer running")?]
        }
        None => {
            let current = crate::run_scope::current();
            if current.is_fallback() {
                crate::run_scope::all()
            } else {
                vec![current]
            }
        }
    };
    for scope in targets {
        scope.cancel_pass(&pass_key);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_pipeline_cancellation_error;

    #[tokio::test]
    async fn a_ready_review_operation_wins_over_cancellation() {
        let run = crate::run_scope::ActiveScope::begin("ready-review-operation");
        run.scope().cancel();
        let result = crate::run_scope::scoped(
            run.scope().clone(),
            super::await_or_cancel(std::future::ready("finished"), None),
        )
        .await;
        assert_eq!(result.unwrap(), "finished");
    }

    /// A detached process group, as `llama-server` runs: it has null stdio and
    /// no drop guard runs at exit, so only the scope can stop it.
    #[cfg(unix)]
    #[test]
    fn stopping_runs_kills_a_detached_child_of_a_running_review() {
        use std::time::{Duration, Instant};
        let run = crate::run_scope::ActiveScope::begin("exit-stops-detached-child");
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "sleep 60"]);
        crate::agent_runtime::process::configure_silent_command(&mut command);
        let mut child = command.spawn().unwrap();
        crate::run_scope::enter_blocking(run.scope().clone(), || {
            crate::process_tree::register_child_pid(child.id());
        });

        super::stop_scopes(vec![run.scope().clone()]);

        assert!(run.scope().is_cancelled());
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "the child outlived the stop"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(!status.success());
    }

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
