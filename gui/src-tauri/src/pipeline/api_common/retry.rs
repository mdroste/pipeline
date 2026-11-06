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
#[allow(clippy::too_many_arguments)]
pub(super) async fn send_with_status_retry(
    app: &crate::emit::EventBus,
    provider: &str,
    label: &str,
    request: reqwest::RequestBuilder,
    pass_key: Option<&str>,
    model: &str,
    output_tokens: u64,
    mut logical_start: bool,
) -> Result<
    (
        reqwest::Response,
        crate::pipeline::executor::budget_dispatch::Reservation,
    ),
    String,
> {
    // JSON bodies are buffered by reqwest. Include attachments and tools in
    // the rough token estimate without copying or decoding their contents.
    let bytes = request
        .try_clone()
        .and_then(|builder| builder.build().ok())
        .and_then(|request| {
            request
                .body()
                .and_then(|body| body.as_bytes())
                .map(<[u8]>::len)
        })
        .unwrap_or(0);
    let mut admit = || {
        crate::pipeline::executor::budget_dispatch::reserve_as(
            Some((model, (bytes as u64).div_ceil(4), output_tokens)),
            std::mem::take(&mut logical_start),
            if provider.eq_ignore_ascii_case("local") {
                crate::usage_allowance::Class::Local
            } else {
                crate::usage_allowance::Class::Api
            },
        )
    };

    for backoff_secs in TRANSIENT_STATUS_BACKOFF_SECS {
        // A non-cloneable (streaming) body cannot be retried; fall through to
        // the single attempt below.
        let Some(attempt) = request.try_clone() else {
            break;
        };
        let receipt = admit()?;
        let resp = await_or_cancel(attempt.send(), pass_key)
            .await?
            .map_err(|e| format_http_error(provider, &e))?;
        let status = resp.status().as_u16();
        if !transient_api_status(status) {
            return Ok((resp, receipt));
        }
        receipt.settle(model, 0, 0, 0, 0);
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
    let receipt = admit()?;
    await_or_cancel(request.send(), pass_key)
        .await?
        .map(|resp| (resp, receipt))
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

/// Providers name quota exhaustion in structured fields, not always in the
/// message, so OpenAI's `code` and `type` and Google's `status` stay beside it.
/// A plain request-rate `code` must not read as exhaustion.
fn api_error_detail(body: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return body.chars().take(200).collect();
    };
    let error = value.get("error");
    let Some(message) = error
        .and_then(|e| e.get("message").or(Some(e)))
        .or_else(|| value.get("message"))
    else {
        return body.chars().take(200).collect();
    };
    let message = message.to_string().trim_matches('"').to_string();
    let quota_code = ["code", "type", "status"]
        .iter()
        .filter_map(|field| error?.get(field)?.as_str())
        .find(|code| crate::pipeline::provider_error::is_usage_limit_error(code));
    match quota_code {
        Some(code) if !message.contains(code) => format!("{code}: {message}"),
        _ => message,
    }
}

pub(super) fn format_api_error(provider: &str, status: u16, body: &str) -> String {
    let detail = api_error_detail(body);

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

#[cfg(test)]
mod admission_tests {
    use super::*;
    #[tokio::test]
    async fn a_rate_limit_retry_cannot_send_another_request_after_the_last_slot() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            assert!(stream.read(&mut buffer).await.unwrap() > 0);
            stream.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
            drop(stream);
            listener
        });
        let scope = crate::run_scope::ActiveScope::begin("http-budget-regression");
        crate::run_scope::scoped(scope.scope().clone(), async {
            let settings = crate::settings::Settings {
                review_max_calls: 1,
                ..Default::default()
            };
            let _settings = crate::settings::freeze_for_run(settings);
            let app: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
            let request = reqwest::Client::new()
                .post(format!("http://{address}/chat"))
                .body("test");
            let error = send_with_status_retry(
                &app, "fixture", "fan-out", request, None, "fixture", 8, true,
            )
            .await
            .err()
            .unwrap();
            assert!(
                error.starts_with(crate::pipeline::executor::budget::BUDGET_STOP_PREFIX),
                "{error}"
            );
            assert_eq!(scope.scope().usage().provider_attempts, 1);
        })
        .await;
        let listener = server.await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
    }
}
