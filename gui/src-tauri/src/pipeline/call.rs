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

    let started = std::time::Instant::now();
    let (output, usage) = super::logging::with_pass(
        request.pass_key.to_string(),
        super::logging::measure_usage(call_llm(
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
        )),
    )
    .await;

    Result {
        output,
        usage,
        duration_secs: started.elapsed().as_secs(),
    }
}
