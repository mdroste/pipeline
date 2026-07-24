//! One measured, attributed LLM invocation.
//!
//! Retry policy and output interpretation belong to callers. Every provider
//! invocation is converted to an owned request and run in its own supervised
//! Tokio task. The scheduler boundary keeps provider polling off the much
//! deeper Tauri/orchestration stack, while boxed futures keep task-local
//! logging scopes from embedding (and duplicating) the full dispatcher state.

use super::claude::{call_llm, LlmOverrides};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

type BoxCallFuture<'a> =
    Pin<Box<dyn Future<Output = std::result::Result<String, String>> + Send + 'a>>;

/// Borrowed convenience request used by the executor and merge paths. It is
/// immediately converted to [`OwnedRequest`] before any scheduler boundary.
pub struct Request<'a> {
    pub app: &'a crate::emit::EventBus,
    pub pass_key: &'a str,
    pub log_label: &'a str,
    pub prompt: &'a str,
    pub tools: &'a [String],
    pub timeout_secs: u64,
    pub agent: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub read_dirs: &'a [String],
    pub write_dir: Option<&'a str>,
    pub command_model: Option<&'a str>,
    pub display_model: &'a str,
    pub model_policy: &'a str,
    pub effort: &'a str,
    pub settings: &'a crate::settings::Settings,
    pub shared_context: Option<Arc<crate::pipeline::context_cache::PreparedContext>>,
}

/// Fully owned input to one provider call. Keeping all data owned makes the
/// call `Send + 'static`, so every caller (including sequential/orientation
/// paths) gets the same independently scheduled task boundary.
pub struct OwnedRequest {
    pub app: crate::emit::EventBus,
    pub pass_key: String,
    pub log_label: String,
    pub prompt: String,
    pub tools: Vec<String>,
    pub system_prompt: Option<String>,
    pub output_format: String,
    pub timeout_secs: u64,
    pub agent: Option<String>,
    pub cwd: Option<String>,
    pub read_dirs: Vec<String>,
    pub write_dir: Option<String>,
    pub command_model: Option<String>,
    pub display_model: Option<String>,
    pub model_policy: Option<String>,
    pub effort: Option<String>,
    pub model_resolved: bool,
    pub settings: Arc<crate::settings::Settings>,
    pub shared_context: Option<Arc<crate::pipeline::context_cache::PreparedContext>>,
    pub pdf_attachment: Option<std::path::PathBuf>,
    pub max_output_tokens: Option<u32>,
}

impl OwnedRequest {
    /// Construct a helper/orientation/extraction call whose model is resolved
    /// by the dispatcher. Step calls use the borrowed [`Request`] conversion
    /// because their model policy was already resolved by the executor.
    pub fn new(
        app: &crate::emit::EventBus,
        pass_key: impl Into<String>,
        log_label: impl Into<String>,
        prompt: impl Into<String>,
        timeout_secs: u64,
    ) -> Self {
        Self {
            app: app.clone(),
            pass_key: pass_key.into(),
            log_label: log_label.into(),
            prompt: prompt.into(),
            tools: Vec::new(),
            system_prompt: None,
            output_format: "text".to_string(),
            timeout_secs,
            agent: None,
            cwd: None,
            read_dirs: Vec::new(),
            write_dir: None,
            command_model: None,
            display_model: None,
            model_policy: None,
            effort: None,
            model_resolved: false,
            settings: Arc::new(crate::settings::load()),
            shared_context: None,
            pdf_attachment: None,
            max_output_tokens: None,
        }
    }
}

impl From<Request<'_>> for OwnedRequest {
    fn from(request: Request<'_>) -> Self {
        Self {
            app: request.app.clone(),
            pass_key: request.pass_key.to_string(),
            log_label: request.log_label.to_string(),
            prompt: request.prompt.to_string(),
            tools: request.tools.to_vec(),
            system_prompt: None,
            output_format: "text".to_string(),
            timeout_secs: request.timeout_secs,
            agent: request.agent.map(str::to_string),
            cwd: request.cwd.map(str::to_string),
            read_dirs: request.read_dirs.to_vec(),
            write_dir: request.write_dir.map(str::to_string),
            command_model: request.command_model.map(str::to_string),
            display_model: Some(request.display_model.to_string()),
            model_policy: Some(request.model_policy.to_string()),
            effort: Some(request.effort.to_string()),
            model_resolved: true,
            settings: Arc::new(request.settings.clone()),
            shared_context: request.shared_context,
            pdf_attachment: None,
            max_output_tokens: None,
        }
    }
}

pub struct Result {
    pub output: std::result::Result<String, String>,
    pub usage: super::logging::CallUsage,
    pub duration_secs: u64,
}

/// A call task must never detach: dropping its waiter kills registered
/// subprocesses and aborts the task. Normal cancellation is handled inside
/// the task first so providers retain their existing grace period to reap
/// children and drain output pipes.
struct CallTask {
    handle: Option<tokio::task::JoinHandle<Result>>,
    pass_key: String,
    started: Instant,
}

impl CallTask {
    fn spawn(request: OwnedRequest) -> Self {
        let pass_key = request.pass_key.clone();
        Self {
            handle: Some(tokio::spawn(async move { execute_inner(request).await })),
            pass_key,
            started: Instant::now(),
        }
    }

    #[cfg(test)]
    fn spawn_test(
        pass_key: impl Into<String>,
        future: impl Future<Output = Result> + Send + 'static,
    ) -> Self {
        Self {
            handle: Some(tokio::spawn(future)),
            pass_key: pass_key.into(),
            started: Instant::now(),
        }
    }

    async fn join(mut self) -> Result {
        // Keep the handle inside `self` while awaiting. If this join future is
        // dropped, `Drop` still owns the handle and cannot accidentally detach
        // the provider task.
        let joined = self
            .handle
            .as_mut()
            .expect("call task handle missing")
            .await;
        let _ = self.handle.take();
        match joined {
            Ok(result) => result,
            Err(error) => {
                crate::commands::kill_pass_children(&self.pass_key);
                Result {
                    output: Err(if error.is_panic() {
                        format!("Provider task panicked: {error}")
                    } else {
                        format!("Provider task was cancelled unexpectedly: {error}")
                    }),
                    usage: super::logging::CallUsage::default(),
                    duration_secs: self.started.elapsed().as_secs(),
                }
            }
        }
    }
}

impl Drop for CallTask {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            crate::commands::kill_pass_children(&self.pass_key);
            handle.abort();
        }
    }
}

/// Execute a step/merge call after converting its borrowed inputs to owned
/// task data. The returned future stays small because it only awaits a
/// supervised `JoinHandle`.
pub async fn execute(request: Request<'_>) -> Result {
    execute_owned(request.into()).await
}

/// Execute any owned call through the single scheduler-rooted dispatch path.
pub async fn execute_owned(request: OwnedRequest) -> Result {
    CallTask::spawn(request).join().await
}

/// Convenience for helper paths that need only the provider text. Usage and
/// duration are still emitted/accumulated by the unified runner.
pub async fn execute_text(request: OwnedRequest) -> std::result::Result<String, String> {
    execute_owned(request).await.output
}

async fn execute_inner(request: OwnedRequest) -> Result {
    let tool_refs: Vec<&str> = request.tools.iter().map(String::as_str).collect();
    let read_dirs: Vec<&str> = request.read_dirs.iter().map(String::as_str).collect();
    let mut overrides = LlmOverrides::from_step_strings(
        request.command_model.as_deref().unwrap_or(""),
        request.effort.as_deref().unwrap_or(""),
    );
    overrides.model_display = request.display_model.as_deref();
    overrides.model_policy = request.model_policy.as_deref();
    overrides.model_resolved = request.model_resolved;
    overrides.pdf_attachment = request.pdf_attachment.as_deref();
    overrides.max_output_tokens = request.max_output_tokens;
    overrides.write_dir = request.write_dir.as_deref();
    overrides.settings = Some(request.settings.as_ref());
    overrides.shared_context = request.shared_context;

    let started = Instant::now();
    let bounded: BoxCallFuture<'_> = Box::pin(async {
        // Heap-pin the cross-provider state machine. In debug builds, placing
        // it beneath generic task-local wrappers caused large nested poll
        // frames on the orchestration worker stack.
        let mut call = Box::pin(call_llm(
            &request.app,
            &request.prompt,
            &tool_refs,
            request.system_prompt.as_deref(),
            &request.output_format,
            request.timeout_secs,
            &request.log_label,
            request.agent.as_deref(),
            request.cwd.as_deref(),
            &read_dirs,
            &overrides,
        ));
        match tokio::time::timeout(
            Duration::from_secs(request.timeout_secs),
            crate::commands::await_or_cancel(call.as_mut(), Some(&request.pass_key)),
        )
        .await
        {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                // Cancellation already terminates registered children. Keep
                // polling briefly so CLI providers can reap them cleanly.
                let _ = tokio::time::timeout(Duration::from_secs(5), call.as_mut()).await;
                Err(error)
            }
            Err(_) => {
                crate::commands::kill_pass_children(&request.pass_key);
                // Keep polling briefly after termination so CLI providers can
                // reap their child and drain/close reader tasks.
                let _ = tokio::time::timeout(Duration::from_secs(5), call.as_mut()).await;
                Err(format!(
                    "{} timed out after {}s (including context preparation and fallbacks)",
                    request.log_label, request.timeout_secs
                ))
            }
        }
    });
    // Type-erase before entering each generic task-local scope. Otherwise the
    // generated TaskLocalFuture states duplicate the entire concrete call
    // future and grow rapidly in debug builds.
    let measured = Box::pin(super::logging::measure_usage(bounded));
    let scoped = Box::pin(super::logging::with_pass(
        request.pass_key.clone(),
        measured,
    ));
    let (output, usage) = scoped.await;

    Result {
        output,
        usage,
        duration_secs: started.elapsed().as_secs(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn empty_result() -> Result {
        Result {
            output: Ok(String::new()),
            usage: super::super::logging::CallUsage::default(),
            duration_secs: 0,
        }
    }

    #[test]
    fn public_execute_future_stays_small() {
        let app: crate::emit::EventBus = Arc::new(crate::emit::NullEvents);
        let settings = crate::settings::Settings::default();
        let future = execute(Request {
            app: &app,
            pass_key: "size-test",
            log_label: "Size test",
            prompt: "",
            tools: &[],
            timeout_secs: 60,
            agent: None,
            cwd: None,
            read_dirs: &[],
            write_dir: None,
            command_model: None,
            display_model: "test",
            model_policy: "test",
            effort: "",
            settings: &settings,
            shared_context: None,
        });
        let size = std::mem::size_of_val(&future);
        assert!(
            size <= 8 * 1024,
            "call::execute future grew to {size} bytes; keep provider state behind boxed/spawned boundaries"
        );
    }

    #[test]
    fn boxed_call_context_runs_on_a_small_debug_stack() {
        std::thread::Builder::new()
            .name("small-stack-call-test".to_string())
            .stack_size(256 * 1024)
            .spawn(|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                runtime.block_on(async {
                    let operation: BoxCallFuture<'_> = Box::pin(async {
                        for _ in 0..32 {
                            tokio::task::yield_now().await;
                        }
                        Ok("done".to_string())
                    });
                    let measured = Box::pin(super::super::logging::measure_usage(operation));
                    let scoped = Box::pin(super::super::logging::with_pass(
                        "small".to_string(),
                        measured,
                    ));
                    assert_eq!(scoped.await.0.unwrap(), "done");
                });
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn task_panic_becomes_a_call_error() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let task = CallTask::spawn_test("panic-test", async {
                    panic!("intentional call-task panic");
                });
                let result = task.join().await;
                assert!(result.output.unwrap_err().contains("panicked"));
            });
    }

    #[test]
    fn dropping_waiter_aborts_in_flight_task() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                struct DropFlag(Arc<AtomicBool>);
                impl Drop for DropFlag {
                    fn drop(&mut self) {
                        self.0.store(true, Ordering::SeqCst);
                    }
                }

                let dropped = Arc::new(AtomicBool::new(false));
                let task_dropped = dropped.clone();
                let task = CallTask::spawn_test("drop-test", async move {
                    let _flag = DropFlag(task_dropped);
                    std::future::pending::<()>().await;
                    empty_result()
                });
                tokio::task::yield_now().await;
                drop(task);
                for _ in 0..20 {
                    if dropped.load(Ordering::SeqCst) {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
                assert!(dropped.load(Ordering::SeqCst));
            });
    }
}
