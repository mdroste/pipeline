//! HTTP retry and error policy for direct API providers.

use super::*;

/// Backoff before each retry of a transient status, when the response
/// carries no usable Retry-After header.
const TRANSIENT_STATUS_BACKOFF_SECS: [u64; 2] = [2, 8];
/// Ceiling on a server-requested Retry-After delay.
const MAX_RETRY_AFTER_SECS: u64 = 60;

/// Rate-limit and overload statuses worth an in-client retry.
pub(super) fn transient_api_status(status: u16) -> bool {
    matches!(status, 429 | 503 | 529)
}

/// Parse a delta-seconds Retry-After header, capped at MAX_RETRY_AFTER_SECS.
fn retry_after_delay(resp: &reqwest::Response) -> Option<std::time::Duration> {
    resp.headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(|secs| std::time::Duration::from_secs(secs.min(MAX_RETRY_AFTER_SECS)))
}

/// Send a request, retrying HTTP 429/503/529 up to two extra attempts with
/// Retry-After-aware, cancellation-aware backoff. The final failing response
/// is returned unchanged so callers keep their existing error formatting.
pub(super) async fn send_with_status_retry(
    app: &crate::emit::EventBus,
    provider: &str,
    label: &str,
    request: reqwest::RequestBuilder,
    pass_key: Option<&str>,
) -> Result<reqwest::Response, String> {
    for backoff_secs in TRANSIENT_STATUS_BACKOFF_SECS {
        // A non-cloneable (streaming) body cannot be retried; fall through to
        // the single attempt below.
        let Some(attempt) = request.try_clone() else {
            break;
        };
        let resp = await_or_cancel(attempt.send(), pass_key)
            .await?
            .map_err(|e| format_http_error(provider, &e))?;
        let status = resp.status().as_u16();
        if !transient_api_status(status) {
            return Ok(resp);
        }
        let delay = retry_after_delay(&resp)
            .unwrap_or_else(|| std::time::Duration::from_secs(backoff_secs));
        log(
            app,
            format!(
                "{label}: {provider} returned HTTP {status}; retrying in {}s",
                delay.as_secs()
            ),
        );
        await_or_cancel(tokio::time::sleep(delay), pass_key).await?;
    }
    await_or_cancel(request.send(), pass_key)
        .await?
        .map_err(|e| format_http_error(provider, &e))
}

fn format_http_error(provider: &str, e: &reqwest::Error) -> String {
    if e.is_timeout() {
        format!("{provider} API request timed out")
    } else if e.is_connect() {
        format!("Failed to connect to {provider} API: {e}")
    } else {
        format!("{provider} API request failed: {e}")
    }
}

pub(super) fn format_api_error(provider: &str, status: u16, body: &str) -> String {
    // Try to extract a message from JSON error body
    let detail = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.get("message").or(Some(e)))
                .or_else(|| v.get("message"))
                .map(|m| m.to_string().trim_matches('"').to_string())
        })
        .unwrap_or_else(|| body.chars().take(200).collect());

    match status {
        401 => format!("{provider}: Invalid API key. Check your key in Settings."),
        429 if crate::pipeline::provider_error::is_usage_limit_error(&detail) => {
            format!("{provider}: Usage limit reached: {detail}")
        }
        429 => format!("{provider}: Rate limited. Wait a moment and try again."),
        529 | 503 => format!("{provider}: Service overloaded. Try again in a few minutes."),
        _ => format!("{provider} API error (HTTP {status}): {detail}"),
    }
}
