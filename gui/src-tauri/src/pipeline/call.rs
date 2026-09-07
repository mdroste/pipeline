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
    pub system_prompt: Option<&'a str>,
    pub tools: &'a [String],
    pub output_schema: Option<&'a serde_json::Value>,
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
    pub output_schema: Option<serde_json::Value>,
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
            output_schema: None,
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
            system_prompt: request.system_prompt.map(str::to_string),
            output_format: "text".to_string(),
            output_schema: request.output_schema.cloned(),
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
    /// Present when a durable provider usage limit caused one configured
    /// cross-provider fallback call. Step outputs use this to retain accurate
    /// provider/model provenance for both calls.
    pub usage_limit_fallback: Option<UsageLimitFallback>,
}

#[derive(Debug, Clone)]
pub struct UsageLimitFallback {
    pub provider: String,
    pub resolution: crate::model_catalog::ResolvedModel,
    pub effort: String,
    pub primary_usage: super::logging::CallUsage,
    pub primary_duration_secs: u64,
    pub fallback_usage: super::logging::CallUsage,
    pub fallback_duration_secs: u64,
}

struct ProviderCallResult {
    output: std::result::Result<String, String>,
    usage: super::logging::CallUsage,
    duration_secs: u64,
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
                    usage_limit_fallback: None,
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
pub async fn execute_owned(mut request: OwnedRequest) -> Result {
    sanitize_model_request(&mut request);
    CallTask::spawn(request).join().await
}

/// Convenience for helper paths that need only the provider text. Usage and
/// duration are still emitted/accumulated by the unified runner.
pub async fn execute_text(request: OwnedRequest) -> std::result::Result<String, String> {
    execute_owned(request).await.output
}

fn sanitize_model_request(request: &mut OwnedRequest) {
    if let std::borrow::Cow::Owned(prompt) = crate::safety::strip_span_tags(&request.prompt) {
        request.prompt = prompt;
    }
    if let Some(system_prompt) = request.system_prompt.as_mut() {
        if let std::borrow::Cow::Owned(clean) = crate::safety::strip_span_tags(system_prompt) {
            *system_prompt = clean;
        }
    }
}

async fn execute_provider_once(
    request: &OwnedRequest,
    overrides: &LlmOverrides<'_>,
    provider: Option<&str>,
    label: &str,
    timeout_secs: u64,
) -> ProviderCallResult {
    let tool_refs: Vec<&str> = request.tools.iter().map(String::as_str).collect();
    let read_dirs: Vec<&str> = request.read_dirs.iter().map(String::as_str).collect();
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
            timeout_secs,
            label,
            provider,
            request.cwd.as_deref(),
            &read_dirs,
            overrides,
        ));
        match tokio::time::timeout(
            Duration::from_secs(timeout_secs),
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
                    label, timeout_secs
                ))
            }
        }
    });
    // Type-erase before entering each generic task-local scope. Otherwise the
    // generated TaskLocalFuture states duplicate the entire concrete call
    // future and grow rapidly in debug builds.
    let (output, usage) = super::logging::measure_usage(bounded).await;

    ProviderCallResult {
        output,
        usage,
        duration_secs: started.elapsed().as_secs(),
    }
}

fn emit_usage_limit_event(
    request: &OwnedRequest,
    provider: &str,
    error: &str,
    status: &str,
    fallback_provider: Option<&str>,
    fallback_model: Option<&str>,
) {
    let _ = request.app.emit_event(
        "pipeline:provider-limit",
        serde_json::json!({
            "pass_key": request.pass_key,
            "label": request.log_label,
            "provider": provider,
            "message": error,
            "status": status,
            "fallback_provider": fallback_provider,
            "fallback_model": fallback_model,
        }),
    );
}

async fn execute_inner(request: OwnedRequest) -> Result {
    let mut primary_overrides = LlmOverrides::from_step_strings(
        request.command_model.as_deref().unwrap_or(""),
        request.effort.as_deref().unwrap_or(""),
    );
    primary_overrides.model_display = request.display_model.as_deref();
    primary_overrides.model_policy = request.model_policy.as_deref();
    primary_overrides.model_resolved = request.model_resolved;
    primary_overrides.pdf_attachment = request.pdf_attachment.as_deref();
    primary_overrides.max_output_tokens = request.max_output_tokens;
    primary_overrides.output_schema = request.output_schema.as_ref();
    primary_overrides.write_dir = request.write_dir.as_deref();
    primary_overrides.settings = Some(request.settings.as_ref());
    primary_overrides.shared_context = request.shared_context.clone();

    let total_started = Instant::now();
    let pass_key = request.pass_key.clone();
    super::logging::with_pass(pass_key, async {
        let primary_provider = request
            .agent
            .as_deref()
            .unwrap_or(&request.settings.preferred_provider);
        let primary_provider = if primary_provider.trim().is_empty() {
            "claude"
        } else {
            primary_provider
        };
        let primary = execute_provider_once(
            &request,
            &primary_overrides,
            request.agent.as_deref(),
            &request.log_label,
            request.timeout_secs,
        )
        .await;
        let primary_error = match &primary.output {
            Err(error) => error.clone(),
            Ok(_) => {
                return Result {
                    output: primary.output,
                    usage: primary.usage,
                    duration_secs: total_started.elapsed().as_secs(),
                    usage_limit_fallback: None,
                };
            }
        };
        if super::provider_error::is_non_retryable_error(&primary_error) || !super::provider_error::is_usage_limit_error(&primary_error) {
            return Result {
                output: primary.output,
                usage: primary.usage,
                duration_secs: total_started.elapsed().as_secs(),
                usage_limit_fallback: None,
            };
        }

        let fallback_provider = request.settings.usage_limit_fallback_agent();
        super::logging::emit(
            &request.app,
            format!(
                "ERROR: {} reached an account usage limit during {}: {}",
                primary_provider, request.log_label, primary_error
            ),
        );
        emit_usage_limit_event(
            &request,
            primary_provider,
            &primary_error,
            if fallback_provider.is_some() {
                "fallback_starting"
            } else {
                "exhausted"
            },
            fallback_provider,
            None,
        );

        let Some(fallback_provider) = fallback_provider else {
            return Result {
                output: primary.output,
                usage: primary.usage,
                duration_secs: total_started.elapsed().as_secs(),
                usage_limit_fallback: None,
            };
        };

        if request.pdf_attachment.is_some() && fallback_provider == "local" {
            let fallback_error = "the Local fallback cannot receive the PDF attachment required by this call";
            emit_usage_limit_event(
                &request,
                primary_provider,
                &primary_error,
                "fallback_failed",
                Some(fallback_provider),
                None,
            );
            return Result {
                output: Err(format!(
                    "{primary_error}; usage-limit fallback was not run because {fallback_error}"
                )),
                usage: primary.usage,
                duration_secs: total_started.elapsed().as_secs(),
                usage_limit_fallback: None,
            };
        }

        let selection = request
            .settings
            .usage_limit_fallback_model_selection(fallback_provider);
        let resolution = match crate::commands::await_or_cancel(
            crate::model_catalog::resolve(
                fallback_provider,
                request.settings.as_ref(),
                selection.as_ref(),
            ),
            Some(&request.pass_key),
        )
        .await
        {
            Ok(Ok(resolution)) => resolution,
            Ok(Err(error)) | Err(error) => {
                emit_usage_limit_event(
                    &request,
                    primary_provider,
                    &primary_error,
                    "fallback_failed",
                    Some(fallback_provider),
                    None,
                );
                return Result {
                    output: Err(format!(
                        "{primary_error}; could not resolve usage-limit fallback provider '{fallback_provider}': {error}"
                    )),
                    usage: primary.usage,
                    duration_secs: total_started.elapsed().as_secs(),
                    usage_limit_fallback: None,
                };
            }
        };
        let effort = request
            .settings
            .usage_limit_fallback_effort(fallback_provider);
        let model_policy = resolution.selection.label();
        let mut fallback_overrides = primary_overrides.clone();
        fallback_overrides.model = resolution.command_model.as_deref();
        fallback_overrides.model_display = Some(&resolution.resolved_model);
        fallback_overrides.model_policy = Some(&model_policy);
        fallback_overrides.model_resolved = true;
        fallback_overrides.effort = (!effort.trim().is_empty()).then_some(effort.as_str());

        let elapsed = total_started.elapsed().as_secs();
        let remaining = request.timeout_secs.saturating_sub(elapsed).max(1);
        let fallback_label = format!("{} · usage-limit fallback", request.log_label);
        super::logging::emit(
            &request.app,
            format!(
                "WARNING: {}: continuing with usage-limit fallback {} / {}",
                request.log_label, fallback_provider, resolution.resolved_model
            ),
        );
        let fallback = execute_provider_once(
            &request,
            &fallback_overrides,
            Some(fallback_provider),
            &fallback_label,
            remaining,
        )
        .await;
        let mut total_usage = primary.usage;
        total_usage.add_usage(fallback.usage);
        let status = if fallback.output.is_ok() {
            "recovered"
        } else {
            "fallback_failed"
        };
        emit_usage_limit_event(
            &request,
            primary_provider,
            &primary_error,
            status,
            Some(fallback_provider),
            Some(&resolution.resolved_model),
        );
        let output = fallback.output.map_err(|fallback_error| {
            format!(
                "{primary_error}; usage-limit fallback {fallback_provider} / {} also failed: {fallback_error}",
                resolution.resolved_model
            )
        });

        Result {
            output,
            usage: total_usage,
            duration_secs: total_started.elapsed().as_secs(),
            usage_limit_fallback: Some(UsageLimitFallback {
                provider: fallback_provider.to_string(),
                resolution,
                effort,
                primary_usage: primary.usage,
                primary_duration_secs: primary.duration_secs,
                fallback_usage: fallback.usage,
                fallback_duration_secs: fallback.duration_secs,
            }),
        }
    })
    .await
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
            usage_limit_fallback: None,
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
            system_prompt: None,
            tools: &[],
            output_schema: None,
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
    fn model_request_drops_span_markup_but_keeps_its_contents() {
        let app: crate::emit::EventBus = Arc::new(crate::emit::NullEvents);
        let mut request = OwnedRequest::new(
            &app,
            "sanitize-test",
            "Sanitize test",
            r#"Review <span id="claim-1">this claim</span>."#,
            60,
        );
        request.system_prompt =
            Some("Use <SPAN class='context'>the supplied context</SPAN>.".to_string());

        sanitize_model_request(&mut request);

        assert_eq!(request.prompt, "Review this claim.");
        assert_eq!(
            request.system_prompt.as_deref(),
            Some("Use the supplied context.")
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
