//! Reads why a `codex exec` call failed.
//!
//! Codex reports a failed turn as a JSON event on stdout and can exit 1 with
//! stderr empty or holding only an unrelated log line, such as a rejected token
//! for one of the user's own MCP servers. The stdout event therefore outranks
//! the agent text and stderr. Its wording is kept as sent, because usage-limit
//! classification reads the same text.

const MAX_FAILURE_CHARS: usize = 1_000;

/// The failure message carried by an `error` or `turn.failed` event. Codex
/// passes provider error bodies through as JSON text, so unwrap those.
pub(super) fn codex_failure_detail(event: &serde_json::Value) -> Option<String> {
    let error = event.get("error");
    let detail = event
        .get("message")
        .and_then(|value| value.as_str())
        .or_else(|| error.and_then(|value| value.as_str()))
        .or_else(|| {
            error
                .and_then(|value| value.get("message"))
                .and_then(|value| value.as_str())
        })?;
    let body = serde_json::from_str::<serde_json::Value>(detail).ok();
    let message = body
        .as_ref()
        .and_then(|body| body.get("error"))
        .and_then(|error| error.get("message"))
        .and_then(|message| message.as_str())
        .unwrap_or(detail)
        .trim();
    (!message.is_empty()).then(|| message.chars().take(MAX_FAILURE_CHARS).collect())
}

pub(super) fn codex_failure_hint(
    stdout_failure: Option<&str>,
    text: &str,
    stderr_tail: &[String],
) -> Option<String> {
    stdout_failure
        .map(str::to_string)
        .or_else(|| crate::pipeline::claude::extract_error_hint(text))
        .or_else(|| crate::pipeline::cli_process::last_stderr_hint(stderr_tail))
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNRELATED_MCP_LOG: &str = "2026-11-02T14:27:27.600172Z ERROR codex_rmcp_client::oauth::refresh_transaction: error=failed to refresh OAuth tokens for server cloudflare-bindings: OAuth refresh token was rejected: Server returned error response: invalid_grant: Grant not found";

    #[test]
    fn a_failed_turn_outranks_an_unrelated_mcp_log_line() {
        let event = serde_json::json!({
            "type": "turn.failed",
            "error": {
                "message": "{\"error\": {\"message\": \"The model is not supported when using Codex with a ChatGPT account.\", \"type\": \"invalid_request_error\"}}"
            }
        });
        let detail = codex_failure_detail(&event);
        assert_eq!(
            detail.as_deref(),
            Some("The model is not supported when using Codex with a ChatGPT account.")
        );
        assert_eq!(
            codex_failure_hint(
                detail.as_deref(),
                "I'll start by reading the paper.",
                &[UNRELATED_MCP_LOG.to_string()]
            ),
            detail
        );
    }

    #[test]
    fn plain_failure_events_keep_their_wording() {
        for event in [
            serde_json::json!({ "type": "error", "message": "You've hit your limit." }),
            serde_json::json!({ "type": "turn.failed", "error": "You've hit your limit." }),
            serde_json::json!({
                "type": "turn.failed",
                "error": { "message": " You've hit your limit.\n" }
            }),
        ] {
            let detail = codex_failure_detail(&event).unwrap();
            assert_eq!(detail, "You've hit your limit.");
            assert!(crate::pipeline::provider_error::is_usage_limit_error(
                &detail
            ));
        }
    }

    #[test]
    fn events_without_a_message_report_nothing() {
        assert_eq!(
            codex_failure_detail(&serde_json::json!({ "type": "turn.failed" })),
            None
        );
        assert_eq!(
            codex_failure_detail(&serde_json::json!({ "type": "error", "message": "  " })),
            None
        );
    }

    #[test]
    fn long_failures_are_bounded() {
        let event = serde_json::json!({ "type": "error", "message": "é".repeat(5_000) });
        let detail = codex_failure_detail(&event).unwrap();
        assert_eq!(detail.chars().count(), MAX_FAILURE_CHARS);
    }

    #[test]
    fn output_and_stderr_remain_the_fallbacks() {
        let stderr = ["Error: thread/start failed".to_string()];
        assert_eq!(
            codex_failure_hint(None, "Rate limit reached", &stderr).as_deref(),
            Some("Rate limited. Wait a moment and try again.")
        );
        assert_eq!(
            codex_failure_hint(None, "", &stderr).as_deref(),
            Some("Error: thread/start failed")
        );
        assert_eq!(codex_failure_hint(None, "", &[]), None);
    }
}
