//! Downloads for pinned managed-engine artifacts.
//!
//! The pinned SHA-256 digest, not the serving host, establishes that an
//! artifact is genuine, so this client follows redirects to whichever CDN a
//! release host names. It carries no credentials and stays on HTTPS. Keep it
//! separate from the API clients in `agent_runtime::api::http`, which must
//! never redirect.
//!
//! Transient failures retry and resume from the last byte received. A source
//! that stays unavailable or serves the wrong bytes gives way to the next one.
//! Cancellation and local storage errors stop the download immediately.

use super::*;
use reqwest::header::{CONTENT_RANGE, RANGE, RETRY_AFTER};
use reqwest::{Response, StatusCode};
use std::io::{Seek as _, SeekFrom};
use std::time::{Duration, Instant};

const MAX_REDIRECTS: usize = 10;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// Longest silence tolerated before a connection counts as stalled.
const STALL_TIMEOUT: Duration = Duration::from_secs(60);
/// Failures in a row, without meaningful progress, before a source is dropped.
const MAX_CONSECUTIVE_FAILURES: u32 = 5;
const MEANINGFUL_PROGRESS_BYTES: u64 = 1_000_000;
const PROGRESS_LOG_BYTES: u64 = 100_000_000;
const PROGRESS_REPORT_INTERVAL: Duration = Duration::from_millis(250);
const RETRY_BASE_DELAY: Duration = if cfg!(test) {
    Duration::from_millis(1)
} else {
    Duration::from_secs(2)
};
const MAX_RETRY_DELAY: Duration = Duration::from_secs(60);

enum Failure {
    /// Worth retrying against the same source.
    Transient {
        message: String,
        retry_after: Option<Duration>,
    },
    /// This source cannot supply the artifact, but another one might.
    Source(String),
    /// Cancellation or a local error that no other source can fix.
    Stop(String),
}

/// The staged file and the digest of exactly the bytes written to it.
struct Transfer {
    file: std::fs::File,
    hasher: Sha256,
    bytes: u64,
    total_bytes: Option<u64>,
    meter: Meter,
}

/// Paces progress reports and smooths the transfer rate between them.
struct Meter {
    sampled_at: Instant,
    sampled_bytes: u64,
    bytes_per_second: f64,
}

impl Meter {
    fn new(now: Instant, bytes: u64) -> Self {
        Self {
            sampled_at: now,
            sampled_bytes: bytes,
            bytes_per_second: 0.0,
        }
    }

    /// Returns true when a report is due. Time spent waiting to reconnect
    /// never counts, because each connection starts a new meter.
    fn sample(&mut self, now: Instant, bytes: u64) -> bool {
        let elapsed = now.saturating_duration_since(self.sampled_at);
        if elapsed < PROGRESS_REPORT_INTERVAL {
            return false;
        }
        let rate = bytes.saturating_sub(self.sampled_bytes) as f64 / elapsed.as_secs_f64();
        self.bytes_per_second = if self.bytes_per_second > 0.0 {
            0.8 * self.bytes_per_second + 0.2 * rate
        } else {
            rate
        };
        self.sampled_at = now;
        self.sampled_bytes = bytes;
        true
    }
}

impl Transfer {
    fn create(destination: &Path) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(destination)?;
        Ok(Self {
            file,
            hasher: Sha256::new(),
            bytes: 0,
            total_bytes: None,
            meter: Meter::new(Instant::now(), 0),
        })
    }

    fn restart(&mut self) -> std::io::Result<()> {
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        self.hasher = Sha256::new();
        self.bytes = 0;
        self.total_bytes = None;
        Ok(())
    }
}

struct Job<'a> {
    client: &'a reqwest::Client,
    app: &'a crate::emit::EventBus,
    max_bytes: u64,
    label: &'a str,
}

impl Job<'_> {
    fn report(&self, transfer: &Transfer, retrying: bool) {
        report_download(
            self.app,
            Some(EngineDownloadProgress {
                label: self.label.to_string(),
                received_bytes: transfer.bytes,
                total_bytes: transfer.total_bytes,
                bytes_per_second: if retrying {
                    0
                } else {
                    transfer.meter.bytes_per_second as u64
                },
                retrying,
            }),
        );
    }
}

fn download_client(https_only: bool) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .https_only(https_only)
        .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(STALL_TIMEOUT)
        .user_agent(concat!("pipeline-gui/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| format!("Could not set up the download connection: {error}"))
}

/// Download an artifact from the first source that supplies the pinned bytes.
pub(super) async fn download_verified(
    app: &crate::emit::EventBus,
    sources: &[String],
    destination: &Path,
    expected_sha256: &str,
    max_bytes: u64,
    label: &str,
) -> Result<u64, String> {
    let client = download_client(true)?;
    let job = Job {
        client: &client,
        app,
        max_bytes,
        label,
    };
    fetch_verified(&job, sources, destination, expected_sha256).await
}

async fn fetch_verified(
    job: &Job<'_>,
    sources: &[String],
    destination: &Path,
    expected_sha256: &str,
) -> Result<u64, String> {
    let result = fetch_first_source(job, sources, destination, expected_sha256).await;
    report_download(job.app, None);
    result
}

async fn fetch_first_source(
    job: &Job<'_>,
    sources: &[String],
    destination: &Path,
    expected_sha256: &str,
) -> Result<u64, String> {
    let label = job.label;
    let mut transfer = Transfer::create(destination)
        .map_err(|error| format!("Failed to stage {label}: {error}"))?;
    let mut failures = Vec::new();
    for url in sources {
        log(job.app, format!("Downloading {label} ({url})"));
        job.report(&transfer, false);
        let message = match fetch_source(job, url, &mut transfer, expected_sha256).await {
            Ok(()) => {
                log(
                    job.app,
                    format!("Verified {label} ({} MB)", transfer.bytes / 1_000_000),
                );
                return Ok(transfer.bytes);
            }
            Err(Failure::Stop(message)) => return Err(message),
            Err(Failure::Source(message) | Failure::Transient { message, .. }) => message,
        };
        log(job.app, format!("{label}: {message}"));
        failures.push(message);
        transfer
            .restart()
            .map_err(|error| format!("Failed to reset {label}: {error}"))?;
    }
    Err(format!("{label} download failed: {}", failures.join("; ")))
}

async fn fetch_source(
    job: &Job<'_>,
    url: &str,
    transfer: &mut Transfer,
    expected_sha256: &str,
) -> Result<(), Failure> {
    let host = source_host(url);
    let mut failures = 0u32;
    loop {
        let start = transfer.bytes;
        let (message, retry_after) = match attempt(job, url, host, transfer).await {
            Ok(()) => break,
            Err(Failure::Transient {
                message,
                retry_after,
            }) => (message, retry_after),
            Err(other) => return Err(other),
        };
        failures = if transfer.bytes >= start.saturating_add(MEANINGFUL_PROGRESS_BYTES) {
            1
        } else {
            failures + 1
        };
        if failures >= MAX_CONSECUTIVE_FAILURES {
            return Err(Failure::Source(message));
        }
        let delay = retry_after
            .unwrap_or(RETRY_BASE_DELAY.saturating_mul(1 << (failures - 1)))
            .min(MAX_RETRY_DELAY);
        log(
            job.app,
            format!(
                "{}: {message}. Retrying in {} s",
                job.label,
                delay.as_secs()
            ),
        );
        job.report(transfer, true);
        await_install_operation(tokio::time::sleep(delay))
            .await
            .map_err(Failure::Stop)?;
    }
    transfer
        .file
        .sync_all()
        .map_err(|error| Failure::Stop(format!("Failed to sync {}: {error}", job.label)))?;
    let actual = format!("{:x}", transfer.hasher.clone().finalize());
    if actual != expected_sha256 {
        return Err(Failure::Source(format!(
            "checksum mismatch from {host} (expected {expected_sha256}, got {actual}). \
             Refusing to install."
        )));
    }
    Ok(())
}

/// Issue one request and stream its body, resuming when bytes are already held.
async fn attempt(
    job: &Job<'_>,
    url: &str,
    host: &str,
    transfer: &mut Transfer,
) -> Result<(), Failure> {
    let label = job.label;
    let mut request = job
        .client
        .get(url)
        .timeout(Duration::from_secs(INSTALL_STEP_TIMEOUT_SECS));
    if transfer.bytes > 0 {
        request = request.header(RANGE, format!("bytes={}-", transfer.bytes));
    }
    let mut response = await_install_operation(request.send())
        .await
        .map_err(Failure::Stop)?
        .map_err(|error| request_failure(host, error))?;
    let status = response.status();
    let reset = |transfer: &mut Transfer| {
        transfer
            .restart()
            .map_err(|error| Failure::Stop(format!("Failed to reset {label}: {error}")))
    };
    if status == StatusCode::RANGE_NOT_SATISFIABLE && transfer.bytes > 0 {
        reset(transfer)?;
        return Err(Failure::Transient {
            message: format!("{host} refused to resume the download"),
            retry_after: None,
        });
    }
    if !status.is_success() {
        return Err(status_failure(host, &response));
    }
    // Only a partial response anchored at the held offset may be appended.
    if status == StatusCode::PARTIAL_CONTENT {
        if range_start(&response) != Some(transfer.bytes) {
            reset(transfer)?;
            return Err(Failure::Transient {
                message: format!("{host} resumed the download at the wrong position"),
                retry_after: None,
            });
        }
    } else {
        reset(transfer)?;
    }
    transfer.total_bytes = response
        .content_length()
        .map(|length| transfer.bytes.saturating_add(length));
    if transfer
        .total_bytes
        .is_some_and(|total| total > job.max_bytes)
    {
        return Err(oversize(host, label));
    }
    transfer.meter = Meter::new(Instant::now(), transfer.bytes);
    job.report(transfer, false);

    while let Some(chunk) = await_install_operation(response.chunk())
        .await
        .map_err(Failure::Stop)?
        .map_err(|error| Failure::Transient {
            message: describe_error(host, error),
            retry_after: None,
        })?
    {
        let received = transfer.bytes.saturating_add(chunk.len() as u64);
        if received > job.max_bytes {
            return Err(oversize(host, label));
        }
        transfer.hasher.update(&chunk);
        transfer
            .file
            .write_all(&chunk)
            .map_err(|error| Failure::Stop(format!("Failed to store {label}: {error}")))?;
        if received / PROGRESS_LOG_BYTES > transfer.bytes / PROGRESS_LOG_BYTES {
            log(job.app, format!("{label}: {} MB", received / 1_000_000));
        }
        transfer.bytes = received;
        if transfer.meter.sample(Instant::now(), received) {
            job.report(transfer, false);
        }
    }
    Ok(())
}

fn request_failure(host: &str, error: reqwest::Error) -> Failure {
    // Redirect loops and refused schemes fail the same way on every retry.
    let permanent = error.is_redirect() || error.is_builder();
    let message = describe_error(host, error);
    if permanent {
        Failure::Source(message)
    } else {
        Failure::Transient {
            message,
            retry_after: None,
        }
    }
}

fn status_failure(host: &str, response: &Response) -> Failure {
    let status = response.status();
    let message = format!("{host}: HTTP {status}");
    let transient = status.is_server_error()
        || matches!(
            status,
            StatusCode::REQUEST_TIMEOUT | StatusCode::TOO_EARLY | StatusCode::TOO_MANY_REQUESTS
        );
    if !transient {
        return Failure::Source(message);
    }
    let retry_after = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse().ok())
        .map(Duration::from_secs);
    Failure::Transient {
        message,
        retry_after,
    }
}

fn oversize(host: &str, label: &str) -> Failure {
    Failure::Source(format!(
        "{label} from {host} exceeds its download safety limit"
    ))
}

/// `Content-Range: bytes <start>-<end>/<total>`
fn range_start(response: &Response) -> Option<u64> {
    let value = response.headers().get(CONTENT_RANGE)?.to_str().ok()?;
    let range = value.trim().strip_prefix("bytes ")?;
    range.split_once('-')?.0.trim().parse().ok()
}

fn source_host(url: &str) -> &str {
    let authority = url.split_once("://").map_or(url, |(_, rest)| rest);
    authority.split(['/', '?']).next().unwrap_or(authority)
}

/// Include the underlying cause. The URL is left out because a CDN's redirect
/// target can carry a signed query string.
fn describe_error(host: &str, error: reqwest::Error) -> String {
    let error = error.without_url();
    let mut message = format!("{host}: {error}");
    let mut cause = std::error::Error::source(&error);
    while let Some(inner) = cause {
        message.push_str(&format!(": {inner}"));
        cause = inner.source();
    }
    message
}

#[cfg(test)]
#[path = "download_tests.rs"]
mod tests;
