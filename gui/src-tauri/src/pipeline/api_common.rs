//! Shared types and the tool-use loop for direct API calls.

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::LazyLock;

/// Maximum tool-call round-trips before giving up.
const MAX_TOOL_ITERATIONS: usize = 15;

/// Minimum remaining seconds before starting another API request.
/// Avoids wasting tokens on a request that will almost certainly time out.
const MIN_REMAINING_SECS: u64 = 10;
const MAX_API_RESPONSE_BYTES: usize = 64 * 1024 * 1024;
const MAX_API_ERROR_BYTES: usize = 1024 * 1024;
const MAX_TOOL_CALLS: usize = 64;
const MAX_TOOL_ARGUMENT_BYTES: usize = 1024 * 1024;
const MAX_TOOL_ARGUMENT_BYTES_TOTAL: usize = 4 * 1024 * 1024;
const MAX_TOOL_HISTORY_MESSAGES: usize = 128;
const MAX_TOOL_HISTORY_BYTES: usize = 64 * 1024 * 1024;

/// Shared HTTP client for all direct API calls.
/// `reqwest::Client` wraps an `Arc` internally, so cloning is cheap.
/// Reusing a single client enables TCP/TLS connection pooling across
/// pipeline steps that hit the same API host.
pub static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);
/// Custom/local endpoints must never redirect requests carrying prompts or
/// bearer tokens to a different origin.
pub static LOCAL_HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("building the loopback-only HTTP client cannot fail")
});
/// HTTPS custom endpoints may use an enterprise proxy, but still cannot
/// redirect a request containing prompts or credentials to another origin.
pub static CUSTOM_HTTPS_HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("building the custom HTTPS client cannot fail")
});

pub fn custom_endpoint_client(base_url: &str) -> &'static reqwest::Client {
    if base_url.trim_start().starts_with("https://") {
        &CUSTOM_HTTPS_HTTP_CLIENT
    } else {
        &LOCAL_HTTP_CLIENT
    }
}

/// Maximum text file size for Read tool calls (5 MB).
const MAX_READ_SIZE: usize = 5 * 1024 * 1024;

/// Maximum PDF file size for attachments (32 MB — matches Anthropic's document limit).
const MAX_PDF_SIZE: usize = 32 * 1024 * 1024;
/// Maximum PDF file size for Read tool calls. Base64 inflation (×4/3) must
/// leave the encoded document inside Anthropic's 32 MB request cap with
/// headroom for the rest of the conversation.
const MAX_TOOL_PDF_SIZE: usize = 20 * 1024 * 1024;
/// Maximum image size for a visual document-asset read.
const MAX_IMAGE_SIZE: usize = 20 * 1024 * 1024;
/// Cumulative direct-API Read budget per model call. This bounds repeated
/// reads of the same large PDF across tool iterations.
const MAX_TOOL_READ_BYTES: usize = 40 * 1024 * 1024;
const MAX_TOOL_READ_CALLS: usize = 32;
const MAX_BATCH_TEXT_REQUESTS: usize = 16;
const MAX_BATCH_TEXT_ITEM_BYTES: usize = 256 * 1024;
const DEFAULT_BATCH_TEXT_ITEM_BYTES: usize = 64 * 1024;
const MAX_BATCH_TEXT_BYTES: usize = 1024 * 1024;
const MAX_BATCH_ASSET_REQUESTS: usize = 8;
/// Cross-provider multimodal requests have lower practical limits after
/// base64/JSON expansion. Keep a batch comfortably below those request caps.
const MAX_BATCH_ASSET_RAW_BYTES: usize = 12 * 1024 * 1024;
pub const MAX_HOSTED_WEB_SEARCH_USES: usize = 5;

/// Default output-token ceiling for step calls when no override is set.
/// CLI transports have no comparable cap; 16384 deterministically truncated
/// long consolidation reports. Matches the order of magnitude of
/// `extract::pdf::EXTRACTION_MAX_OUTPUT_TOKENS`.
pub const DEFAULT_STEP_MAX_OUTPUT_TOKENS: u32 = 32_000;

/// Per-provider tool-result media constraints for the tool loop.
/// `pdf_reads` is false where the transport's tool-result converter discards
/// PDF content (OpenAI/Google), so `.pdf` Reads fail fast before any bytes
/// are read or charged. `encoded_media_budget` caps cumulative base64
/// image/PDF tool-result bytes per model call: Anthropic and OpenAI accept
/// ~32 MB request bodies, Google ~20 MB, and the running conversation needs
/// headroom around the media.
struct MediaPolicy {
    pdf_reads: bool,
    encoded_media_budget: usize,
}

const ANTHROPIC_MEDIA_POLICY: MediaPolicy = MediaPolicy {
    pdf_reads: true,
    encoded_media_budget: 24 * 1024 * 1024,
};
const OPENAI_MEDIA_POLICY: MediaPolicy = MediaPolicy {
    pdf_reads: false,
    encoded_media_budget: 24 * 1024 * 1024,
};
const GOOGLE_MEDIA_POLICY: MediaPolicy = MediaPolicy {
    pdf_reads: false,
    encoded_media_budget: 14 * 1024 * 1024,
};

/// Maximum size of a single Write tool call (5 MB — reports are ~100 KB;
/// this leaves room for data artifacts without letting a runaway model
/// fill the disk).
const MAX_WRITE_SIZE: usize = 5 * 1024 * 1024;

/// Maximum number of Write tool calls per pipeline run.
const MAX_WRITES_PER_RUN: usize = 200;
const MAX_WRITE_BYTES_PER_RUN: usize = 100 * 1024 * 1024;

// ── Logging ────────────────────────────────────────────────────────

pub fn log(app: &crate::emit::EventBus, line: impl Into<String>) {
    super::logging::emit(app, line.into());
}

pub fn verbose_log(app: &crate::emit::EventBus, line: impl Into<String>) {
    if crate::settings::load().verbose_logging {
        log(app, line);
    }
}

async fn await_or_cancel<F, T>(future: F, pass_key: Option<&str>) -> Result<T, String>
where
    F: std::future::Future<Output = T>,
{
    let mut operation = std::pin::pin!(future);
    let mut cancellation = std::pin::pin!(crate::commands::wait_for_cancellation(pass_key));
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

fn append_api_chunk(buffer: &mut Vec<u8>, chunk: &[u8], limit: usize) -> Result<(), String> {
    if chunk.len() > limit.saturating_sub(buffer.len()) {
        return Err(format!(
            "API response exceeded the {} MB safety limit",
            limit / 1024 / 1024
        ));
    }
    buffer.extend_from_slice(chunk);
    Ok(())
}

struct LimitedJsonCounter {
    bytes: usize,
    limit: usize,
    overflowed: bool,
}

impl std::io::Write for LimitedJsonCounter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes) {
            self.overflowed = true;
            return Err(std::io::Error::other("serialized value exceeded limit"));
        }
        self.bytes += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn serialized_size_limited<T: Serialize + ?Sized>(
    value: &T,
    limit: usize,
    label: &str,
) -> Result<usize, String> {
    let mut counter = LimitedJsonCounter {
        bytes: 0,
        limit,
        overflowed: false,
    };
    if let Err(error) = serde_json::to_writer(&mut counter, value) {
        if counter.overflowed {
            return Err(format!("{label} exceeded the {limit} byte safety limit"));
        }
        return Err(format!("Failed to serialize {label}: {error}"));
    }
    Ok(counter.bytes)
}

fn validate_tool_history<T: Serialize>(messages: &[T], provider: &str) -> Result<(), String> {
    if messages.len() > MAX_TOOL_HISTORY_MESSAGES {
        return Err(format!(
            "{provider} tool history exceeded the {MAX_TOOL_HISTORY_MESSAGES}-message safety limit"
        ));
    }
    serialized_size_limited(
        messages,
        MAX_TOOL_HISTORY_BYTES,
        &format!("{provider} tool history"),
    )?;
    Ok(())
}

pub(crate) async fn response_bytes_limited(
    mut response: reqwest::Response,
    limit: usize,
    pass_key: Option<&str>,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(format!(
            "API response exceeded the {} MB safety limit",
            limit / 1024 / 1024
        ));
    }
    let mut bytes =
        Vec::with_capacity(response.content_length().unwrap_or(0).min(limit as u64) as usize);
    loop {
        let chunk = await_or_cancel(response.chunk(), pass_key)
            .await?
            .map_err(|e| format!("Failed to read API response: {e}"))?;
        let Some(chunk) = chunk else { break };
        append_api_chunk(&mut bytes, &chunk, limit)?;
    }
    Ok(bytes)
}

mod execution;
mod loops;
mod models;
mod retry;

use execution::{
    anthropic_tool_result_content, append_openai_image_message, append_openai_tool_result,
    execute_tool, google_tool_result_parts,
};
use models::{
    anthropic_hosted_search_count, anthropic_output_incomplete, direct_tool_kind,
    google_hosted_search_count, google_output_incomplete, openai_output_incomplete,
    read_bytes_limited, read_file_for_tool_limited, read_image_for_tool, read_pdf_for_tool,
    slice_text_range, text_line_bounds, validate_tool_path,
};
use retry::{format_api_error, send_with_status_retry};

#[cfg(test)]
use execution::{enforce_media_budget, execute_read_assets_batch, execute_read_text_batch};
#[cfg(test)]
use models::{WRITE_BYTES, WRITE_COUNT};
#[cfg(test)]
use retry::transient_api_status;

pub(crate) use execution::{execute_native_tool, ToolBudget};
pub use loops::*;
pub use models::*;

#[cfg(test)]
mod tests;
