//! Review Claude Code adapter and scoped CLI workspace.
use super::providers::LlmOverrides;
use std::io::Write;
use std::process::Stdio;
use tempfile::TempDir;

pub use super::cli_process::MAX_STDOUT_BYTES;
use super::cli_process::{
    capture_stderr, capture_text_stdout, emit_stderr_tail, finish_streams, last_stderr_hint, log,
    track_child_started, verbose_log, wait_for_child,
};

/// Maximum characters to pass as a direct CLI argument.
/// Beyond this we write to a temp file and tell Claude to read it.
pub(crate) const MAX_DIRECT_PROMPT_LENGTH: usize = 4000;
mod claude_cli;
mod cli_workspace;

pub use claude_cli::*;
pub(crate) use cli_workspace::*;

#[cfg(test)]
use super::providers::artifacts::{
    check_live_artifact_quota, event_text_preview, MAX_EVENT_TEXT_PREVIEW_BYTES,
    MAX_LIVE_ARTIFACT_FILE_BYTES,
};
#[cfg(test)]
use super::providers::{request_effort, request_provider_label};
#[cfg(test)]
use claude_cli::{
    absolute_rule_path, append_claude_output_schema, claude_failure_hint, cli_disallowed_tools,
    fork_failure_uses_fallback, is_cli_session_capability_error, parse_claude_result,
    CLAUDE_LOGIN_HINT,
};

#[cfg(test)]
mod tests;
