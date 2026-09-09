use std::future::Future;
use std::io::Write;
use std::pin::Pin;
use std::process::Stdio;
use std::time::Duration;
use tempfile::TempDir;
use tokio::process::Command;

pub use super::cli_process::MAX_STDOUT_BYTES;
use super::cli_process::{
    capture_stderr, capture_text_stdout, emit_stderr_tail, finish_streams, last_stderr_hint, log,
    track_child_started, verbose_log, wait_for_child,
};

type BoxedProviderFuture<'a> = Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>>;

/// Maximum characters to pass as a direct CLI argument.
/// Beyond this we write to a temp file and tell Claude to read it.
pub(crate) const MAX_DIRECT_PROMPT_LENGTH: usize = 4000;
const MAX_EVENT_TEXT_PREVIEW_BYTES: usize = 16 * 1024;
const MAX_LIVE_ARTIFACT_FILES: usize = 1_000;
const MAX_LIVE_ARTIFACT_BYTES: u64 = 300 * 1024 * 1024;
const MAX_LIVE_ARTIFACT_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// Per-call model/effort overrides. When fields are `Some(non-empty)`, they
/// take precedence over the corresponding global settings for this single call.
/// Empty strings are treated as "no override" so callers can pass
/// step.model.as_str() directly.
#[derive(Default, Clone, Debug)]
pub struct LlmOverrides<'a> {
    pub model: Option<&'a str>,
    /// Human-readable resolved model provenance for the console. This can be
    /// populated even when CLI Automatic deliberately omits the model flag.
    pub model_display: Option<&'a str>,
    pub model_policy: Option<&'a str>,
    pub effort: Option<&'a str>,
    /// The caller already resolved the structured model policy. When true,
    /// `model: None` intentionally means CLI Automatic and must not fall back
    /// to a legacy settings string.
    pub model_resolved: bool,
    /// PDF to attach to the request on direct-API paths. CLI paths ignore
    /// this — there the prompt references the file path and the CLI's Read
    /// tool handles PDFs natively.
    pub pdf_attachment: Option<&'a std::path::Path>,
    /// Max output tokens on direct-API paths. The default (16384) suits
    /// analysis steps; full-document transcription needs more. CLI paths
    /// ignore this.
    pub max_output_tokens: Option<u32>,
    /// Optional schema for the terminal artifact. Every provider transport
    /// receives a native structured-output constraint when this is present;
    /// Pipeline still validates the original schema after the call.
    pub output_schema: Option<&'a serde_json::Value>,
    /// The run's artifact directory (absolute, forward slashes). When set,
    /// the call is allowed to write files — confined to this directory by
    /// each provider's sandbox mechanism — and callers should also set the
    /// subprocess cwd to this directory. `None` = read-only call.
    pub write_dir: Option<&'a str>,
    /// Immutable settings snapshot captured at run start. When absent (for
    /// standalone helper calls), settings are loaded normally.
    pub settings: Option<&'a crate::settings::Settings>,
    /// Run-local paper/orientation prefix. Direct APIs place it before the
    /// task-specific prompt; supported CLIs fork a warmed base session.
    pub shared_context: Option<std::sync::Arc<crate::pipeline::context_cache::PreparedContext>>,
}

impl<'a> LlmOverrides<'a> {
    pub fn from_step_strings(model: &'a str, effort: &'a str) -> Self {
        Self {
            model: if model.trim().is_empty() {
                None
            } else {
                Some(model)
            },
            effort: if effort.trim().is_empty() {
                None
            } else {
                Some(effort)
            },
            ..Default::default()
        }
    }
}

mod artifacts;
mod claude_cli;
mod cli_workspace;
mod dispatch;
mod process;

use artifacts::{event_text_preview, supervise_artifact_writes};
pub use claude_cli::*;
pub(crate) use cli_workspace::*;
pub use dispatch::*;
pub use process::*;

#[cfg(test)]
use artifacts::check_live_artifact_quota;
#[cfg(test)]
use claude_cli::{
    absolute_rule_path, append_claude_output_schema, claude_failure_hint, cli_disallowed_tools,
    fork_failure_uses_fallback, is_cli_session_capability_error, parse_claude_result,
    CLAUDE_LOGIN_HINT,
};
#[cfg(test)]
use dispatch::{request_effort, request_provider_label};

#[cfg(test)]
mod tests;
