//! One measured, attributed LLM invocation.
//!
//! Retry policy and output interpretation belong to callers, but every
//! executor path constructs overrides, attaches the pass key, and measures
//! usage and duration here.

use super::claude::{call_llm, LlmOverrides};

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
    pub effort: &'a str,
    pub settings: &'a crate::settings::Settings,
    pub shared_context: Option<std::sync::Arc<crate::pipeline::context_cache::PreparedContext>>,
}

pub struct Result {
    pub output: std::result::Result<String, String>,
    pub usage: super::logging::CallUsage,
    pub duration_secs: u64,
}

pub async fn execute(request: Request<'_>) -> Result {
    let tool_refs: Vec<&str> = request.tools.iter().map(String::as_str).collect();
    let read_dirs: Vec<&str> = request.read_dirs.iter().map(String::as_str).collect();
    let mut overrides =
        LlmOverrides::from_step_strings(request.command_model.unwrap_or(""), request.effort);
    overrides.model_resolved = true;
    overrides.write_dir = request.write_dir;
    overrides.settings = Some(request.settings);
    overrides.shared_context = request.shared_context;

    let started = std::time::Instant::now();
    let bounded = async {
        let mut call = std::pin::pin!(call_llm(
            request.app,
            request.prompt,
            &tool_refs,
            None,
            "text",
            request.timeout_secs,
            request.log_label,
            request.agent,
            request.cwd,
            &read_dirs,
            &overrides,
        ));
        match tokio::time::timeout(
            std::time::Duration::from_secs(request.timeout_secs),
            crate::commands::await_or_cancel(call.as_mut(), Some(request.pass_key)),
        )
        .await
        {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                // Cancellation already terminates registered children. Keep
                // polling briefly so CLI providers can reap them cleanly.
                let _ =
                    tokio::time::timeout(std::time::Duration::from_secs(5), call.as_mut()).await;
                Err(error)
            }
            Err(_) => {
                crate::commands::kill_pass_children(request.pass_key);
                // Keep polling briefly after termination so CLI providers can
                // reap their child and drain/close reader tasks. The provider
                // result is intentionally discarded: the logical deadline
                // has already elapsed.
                let _ =
                    tokio::time::timeout(std::time::Duration::from_secs(5), call.as_mut()).await;
                Err(format!(
                    "{} timed out after {}s (including context preparation and fallbacks)",
                    request.log_label, request.timeout_secs
                ))
            }
        }
    };
    let (output, usage) = super::logging::with_pass(
        request.pass_key.to_string(),
        super::logging::measure_usage(bounded),
    )
    .await;

    Result {
        output,
        usage,
        duration_secs: started.elapsed().as_secs(),
    }
}
