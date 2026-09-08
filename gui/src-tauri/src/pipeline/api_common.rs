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

// ── Tool definitions ───────────────────────────────────────────────

/// The Read tool schema sent to APIs.
#[derive(Debug, Clone, Serialize)]
pub struct ReadToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

impl Default for ReadToolDef {
    fn default() -> Self {
        Self {
            name: "Read".to_string(),
            description: "Read the contents of a file at the given absolute path. Returns the file's text content.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Absolute file path to read"
                    }
                },
                "required": ["file_path"]
            }),
        }
    }
}

/// Batch text reader for collecting several bounded evidence ranges in one
/// model round trip. `Read` remains available for compatibility and unusual
/// single-file cases.
#[derive(Debug, Clone, Serialize)]
pub struct ReadTextBatchToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

impl Default for ReadTextBatchToolDef {
    fn default() -> Self {
        Self {
            name: "ReadTextBatch".to_string(),
            description: format!(
                "Read up to {MAX_BATCH_TEXT_REQUESTS} bounded UTF-8 text ranges in one call. \
                 Prefer this over several sequential Read calls. Exact duplicate requests are \
                 returned once. Select ranges with 1-based start_line/end_line, or with offset \
                 (never both); use next_offset from a truncated result to continue."
            ),
            input_schema: serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "requests": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": MAX_BATCH_TEXT_REQUESTS,
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "file_path": {
                                    "type": "string",
                                    "description": "Absolute path to a UTF-8 text file"
                                },
                                "offset": {
                                    "type": "integer",
                                    "minimum": 0,
                                    "description": "Optional UTF-8 byte offset. Mutually exclusive with start_line/end_line. Use a prior result's next_offset."
                                },
                                "start_line": {
                                    "type": "integer",
                                    "minimum": 1,
                                    "description": "Optional 1-based first line. Defaults to line 1 when only end_line is given."
                                },
                                "end_line": {
                                    "type": "integer",
                                    "minimum": 1,
                                    "description": "Optional inclusive 1-based last line. Requires line-range mode and must not precede start_line."
                                },
                                "max_bytes": {
                                    "type": "integer",
                                    "minimum": 1,
                                    "maximum": MAX_BATCH_TEXT_ITEM_BYTES,
                                    "description": "Maximum bytes to return for this range"
                                }
                            },
                            "required": ["file_path"]
                        }
                    }
                },
                "required": ["requests"]
            }),
        }
    }
}

/// Multimodal asset reader. Kept separate from `Read` so providers receive
/// image bytes as image content rather than accidentally decoding them as
/// UTF-8 text.
#[derive(Debug, Clone, Serialize)]
pub struct DocumentAssetToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

impl Default for DocumentAssetToolDef {
    fn default() -> Self {
        Self {
            name: "ReadDocumentAsset".to_string(),
            description: "Read an image referenced by a DocumentBundle and return it as visual input. Use the absolute path formed from the bundle's run root and the asset rel_path.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Absolute path of a PNG, JPEG, GIF, or WebP document asset"
                    }
                },
                "required": ["file_path"]
            }),
        }
    }
}

/// Batch multimodal reader. Providers receive every unique image in one tool
/// result so the model can compare pages without one round trip per asset.
#[derive(Debug, Clone, Serialize)]
pub struct DocumentAssetsBatchToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

impl Default for DocumentAssetsBatchToolDef {
    fn default() -> Self {
        Self {
            name: "ReadDocumentAssetsBatch".to_string(),
            description: format!(
                "Read up to {MAX_BATCH_ASSET_REQUESTS} document images in one call. Prefer this \
                 for comparing tables, figures, or rendered pages. Exact duplicate files are \
                 attached once. Only PNG, JPEG, GIF, and WebP are accepted; aggregate raw image \
                 data is capped at {} MiB.",
                MAX_BATCH_ASSET_RAW_BYTES / 1024 / 1024
            ),
            input_schema: serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "file_paths": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": MAX_BATCH_ASSET_REQUESTS,
                        "items": {
                            "type": "string",
                            "description": "Absolute path to a document image"
                        }
                    }
                },
                "required": ["file_paths"]
            }),
        }
    }
}

/// The Write tool schema sent to APIs. Confined to the run's artifact
/// directory by `write_file_for_tool`.
#[derive(Debug, Clone, Serialize)]
pub struct WriteToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

impl Default for WriteToolDef {
    fn default() -> Self {
        Self {
            name: "Write".to_string(),
            description: "Write a UTF-8 text file inside the run's artifact directory. Paths outside that directory are rejected.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Destination path. Either absolute (must be inside the artifact directory) or relative to it."
                    },
                    "content": {
                        "type": "string",
                        "description": "Full file content to write"
                    }
                },
                "required": ["file_path", "content"]
            }),
        }
    }
}

// ── Tool execution ─────────────────────────────────────────────────

/// Per-provider-call filesystem authority. Keeping it owned by the call is
/// what allows parallel steps to receive disjoint artifact views.
#[derive(Debug, Clone, Default)]
pub struct ToolAccess {
    read_roots: Vec<std::path::PathBuf>,
    write_root: Option<std::path::PathBuf>,
}

impl ToolAccess {
    pub fn new(read_roots: &[&str], write_root: Option<&str>) -> Self {
        let write_root = write_root
            .filter(|root| !root.trim().is_empty())
            .map(std::path::PathBuf::from);
        let mut readable: Vec<std::path::PathBuf> = read_roots
            .iter()
            .filter(|root| !root.trim().is_empty())
            .map(std::path::PathBuf::from)
            .collect();
        if let Some(root) = write_root.as_ref() {
            readable.push(root.clone());
        }
        Self {
            read_roots: readable,
            write_root,
        }
    }
}

/// Write calls consumed this run, reset when a run workspace is created.
static WRITE_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static WRITE_BYTES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

struct WriteReservation {
    bytes: usize,
    committed: bool,
}

impl WriteReservation {
    fn reserve(bytes: usize) -> Result<Self, String> {
        WRITE_COUNT
            .fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |used| (used < MAX_WRITES_PER_RUN).then_some(used + 1),
            )
            .map_err(|_| format!("Write limit reached ({MAX_WRITES_PER_RUN} files per run)"))?;
        if WRITE_BYTES
            .fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |used| {
                    used.checked_add(bytes)
                        .filter(|next| *next <= MAX_WRITE_BYTES_PER_RUN)
                },
            )
            .is_err()
        {
            let _ = WRITE_COUNT.fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |used| Some(used.saturating_sub(1)),
            );
            return Err(format!(
                "Run artifact quota reached ({} MB)",
                MAX_WRITE_BYTES_PER_RUN / 1024 / 1024
            ));
        }
        Ok(Self {
            bytes,
            committed: false,
        })
    }

    fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for WriteReservation {
    fn drop(&mut self) {
        if !self.committed {
            let _ = WRITE_COUNT.fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |used| Some(used.saturating_sub(1)),
            );
            let _ = WRITE_BYTES.fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |used| Some(used.saturating_sub(self.bytes)),
            );
        }
    }
}

/// Reset the run-wide aggregate write quota. Filesystem authority itself is
/// per call in [`ToolAccess`].
pub fn reset_write_budget() {
    WRITE_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);
    WRITE_BYTES.store(0, std::sync::atomic::Ordering::SeqCst);
}

/// Validate and perform a Write tool call. The destination must resolve
/// inside the configured write dir; `..` components, absolute paths outside
/// the dir, and symlinked destinations are rejected. Parent subdirectories
/// are created as needed.
pub fn write_file_for_tool(
    access: &ToolAccess,
    path: &str,
    content: &str,
) -> Result<String, String> {
    let raw_root = access
        .write_root
        .clone()
        .ok_or("File writing is not enabled for this step")?;
    // The run dir is created before any LLM call, so this canonicalizes.
    let root = raw_root
        .canonicalize()
        .map_err(|e| format!("Artifact directory unavailable: {e}"))?;

    if content.len() > MAX_WRITE_SIZE {
        return Err(format!(
            "Content too large ({} bytes, max {MAX_WRITE_SIZE})",
            content.len()
        ));
    }
    // Resolve to a path relative to the artifact root. Absolute paths must
    // already point inside it (accepting both the raw and canonical spelling
    // of the root, since models echo back whichever form the prompt used).
    let p = std::path::Path::new(path);
    let rel = if p.is_absolute() {
        p.strip_prefix(&root)
            .or_else(|_| p.strip_prefix(&raw_root))
            .map(|r| r.to_path_buf())
            .map_err(|_| format!("Access denied: {path} is outside the artifact directory"))?
    } else {
        p.to_path_buf()
    };

    // No traversal components, no re-rooting, and a sane depth.
    if rel.as_os_str().is_empty() {
        return Err("Destination path is empty".into());
    }
    if !rel
        .components()
        .all(|c| matches!(c, std::path::Component::Normal(_)))
    {
        return Err(format!("Access denied: {path} contains path traversal"));
    }
    if rel.components().count() > 8 {
        return Err("Destination path too deep".into());
    }

    let dest = root.join(&rel);
    // Refuse to write through a pre-existing symlink (e.g. planted by an
    // earlier shell command in a CLI-mode step of the same run).
    if let Ok(meta) = std::fs::symlink_metadata(&dest) {
        if meta.file_type().is_symlink() {
            return Err(format!("Access denied: {path} is a symlink"));
        }
        if !meta.is_file() {
            return Err(format!("Access denied: {path} is not a regular file"));
        }
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory for {path}: {e}"))?;
        // A symlinked intermediate directory could redirect the write even
        // though every component is Normal; canonicalize and re-check.
        let canon_parent = parent
            .canonicalize()
            .map_err(|e| format!("Cannot resolve directory for {path}: {e}"))?;
        if !canon_parent.starts_with(&root) {
            return Err(format!(
                "Access denied: {path} resolves outside the artifact directory"
            ));
        }
    }
    let parent = dest.parent().ok_or("Destination has no parent directory")?;
    let mut staged = tempfile::Builder::new()
        .prefix(".pipeline-write-")
        .tempfile_in(parent)
        .map_err(|e| format!("Failed to stage {path}: {e}"))?;
    staged
        .write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write {path}: {e}"))?;
    staged
        .as_file()
        .sync_all()
        .map_err(|e| format!("Failed to sync {path}: {e}"))?;

    // Reserve only once validation and the fallible data write have
    // succeeded. The reservation rolls back if the atomic commit fails.
    let reservation = WriteReservation::reserve(content.len())?;
    staged
        .into_temp_path()
        .persist(&dest)
        .map_err(|e| format!("Failed to commit {path}: {}", e.error))?;
    reservation.commit();

    Ok(format!(
        "Wrote {} bytes to {}",
        content.len(),
        dest.to_string_lossy().replace('\\', "/")
    ))
}

/// Validate a tool-read path: resolve symlinks, check it's under an allowed
/// directory, and enforce a size limit.  Returns the canonical path on success.
fn validate_tool_path(
    access: &ToolAccess,
    path: &str,
    max_size: usize,
) -> Result<std::path::PathBuf, String> {
    let p = std::path::Path::new(path);

    // Resolve to canonical path to prevent traversal via symlinks or ..
    let canonical = p
        .canonicalize()
        .map_err(|_| format!("File not found: {path}"))?;

    // Validate the path against the exact roots registered for this run. The
    // OS-wide temporary directory is deliberately not implicit: unrelated
    // applications commonly place credentials and private documents there.
    let is_allowed = access.read_roots.iter().any(|dir| {
        std::path::Path::new(dir)
            .canonicalize()
            .map(|d| canonical.starts_with(&d))
            .unwrap_or(false)
    });

    if !is_allowed {
        return Err(format!(
            "Access denied: {path} is outside allowed directories"
        ));
    }

    let metadata =
        std::fs::metadata(&canonical).map_err(|e| format!("Cannot read file metadata: {e}"))?;
    if !metadata.is_file() {
        return Err(format!("Access denied: {path} is not a regular file"));
    }
    if metadata.len() as usize > max_size {
        return Err(format!(
            "File too large ({} bytes, max {})",
            metadata.len(),
            max_size
        ));
    }

    Ok(canonical)
}

fn read_bytes_limited(path: &std::path::Path, limit: usize) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    // Symlink/FIFO-safe open; on Windows it also reopens non-symlink file
    // reparse points (OneDrive-style cloud placeholders) so they hydrate.
    let file = crate::safety::open_regular_file(path)?;
    let mut bytes = Vec::with_capacity(64 * 1024);
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    if bytes.len() > limit {
        return Err(format!(
            "File too large (more than {limit} bytes): {}",
            path.display()
        ));
    }
    Ok(bytes)
}

fn read_file_for_tool_limited(
    access: &ToolAccess,
    path: &str,
    limit: usize,
) -> Result<String, String> {
    let limit = MAX_READ_SIZE.min(limit);
    let canonical = validate_tool_path(access, path, limit)?;

    if path.to_lowercase().ends_with(".pdf") {
        return Err("Cannot read PDF as text. Use the extracted paper text instead.".into());
    }

    String::from_utf8(read_bytes_limited(&canonical, limit)?)
        .map_err(|e| format!("Failed to read {path} as UTF-8: {e}"))
}

#[derive(Debug)]
struct TextRange {
    content: String,
    start_offset: usize,
    end_offset: usize,
    next_offset: Option<usize>,
}

fn slice_text_range(
    text: &str,
    start_offset: usize,
    requested_end_offset: usize,
    max_bytes: usize,
) -> Result<TextRange, String> {
    if start_offset > text.len() {
        return Err(format!(
            "Offset {start_offset} is past the end of the {}-byte file",
            text.len()
        ));
    }
    if requested_end_offset < start_offset || requested_end_offset > text.len() {
        return Err("Requested text range is outside the file".to_string());
    }
    if !text.is_char_boundary(start_offset) {
        return Err(format!(
            "Offset {start_offset} is not a UTF-8 character boundary; use next_offset from a prior result"
        ));
    }
    let mut end = start_offset
        .saturating_add(max_bytes)
        .min(requested_end_offset);
    while end > start_offset && !text.is_char_boundary(end) {
        end -= 1;
    }
    if end == start_offset && start_offset < requested_end_offset {
        return Err(
            "max_bytes is too small to contain the next UTF-8 character at this offset".to_string(),
        );
    }
    Ok(TextRange {
        content: text[start_offset..end].to_string(),
        start_offset,
        end_offset: end,
        next_offset: (end < requested_end_offset).then_some(end),
    })
}

fn text_line_bounds(
    text: &str,
    requested_start_line: Option<usize>,
    requested_end_line: Option<usize>,
) -> Result<(usize, usize), String> {
    let start_line = requested_start_line.unwrap_or(1);
    let mut starts = Vec::with_capacity(text.len() / 64 + 1);
    if !text.is_empty() {
        starts.push(0);
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' && index + 1 < text.len() {
                starts.push(index + 1);
            }
        }
    }
    if start_line == 0 || start_line > starts.len() {
        return Err(format!(
            "start_line {start_line} is outside the file's 1..={} line range",
            starts.len()
        ));
    }
    if let Some(end_line) = requested_end_line {
        if end_line < start_line {
            return Err(format!(
                "end_line {end_line} must be greater than or equal to start_line {start_line}"
            ));
        }
        if end_line > starts.len() {
            return Err(format!(
                "end_line {end_line} is outside the file's 1..={} line range",
                starts.len()
            ));
        }
    }
    let start_offset = starts[start_line - 1];
    let end_offset = requested_end_line
        .filter(|end_line| *end_line < starts.len())
        .map(|end_line| starts[end_line])
        .unwrap_or(text.len());
    Ok((start_offset, end_offset))
}

/// Read a PDF file and return its contents as base64-encoded bytes.
fn read_pdf_for_tool(access: &ToolAccess, path: &str, limit: usize) -> Result<String, String> {
    let limit = MAX_TOOL_PDF_SIZE.min(limit);
    let canonical = validate_tool_path(access, path, limit)?;
    let bytes = read_bytes_limited(&canonical, limit)?;
    Ok(STANDARD.encode(&bytes))
}

fn read_image_for_tool(
    access: &ToolAccess,
    path: &str,
    limit: usize,
) -> Result<(String, String, usize), String> {
    let extension = PathBuf::from(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let media_type = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => {
            return Err("Unsupported document image type; use PNG, JPEG, GIF, or WebP".to_string())
        }
    };
    let limit = MAX_IMAGE_SIZE.min(limit);
    let canonical = validate_tool_path(access, path, limit)?;
    let bytes = read_bytes_limited(&canonical, limit)?;
    let raw_bytes = bytes.len();
    Ok((STANDARD.encode(bytes), media_type.to_string(), raw_bytes))
}

/// Read a PDF the app itself is attaching to a request (as opposed to one the
/// model asked for via the Read tool — that path goes through the allowed-dir
/// sandbox above). Only a size cap applies: the path comes from our own
/// extraction code, not from model output.
pub fn pdf_attachment_base64(path: &std::path::Path, max_size: usize) -> Result<String, String> {
    let metadata = std::fs::metadata(path).map_err(|e| format!("Cannot read PDF metadata: {e}"))?;
    if metadata.len() as usize > max_size {
        return Err(format!(
            "PDF too large to attach ({} MB, limit {} MB). Use a CLI provider or a native extraction method.",
            metadata.len() / 1_000_000,
            max_size / 1_000_000
        ));
    }
    let bytes = read_bytes_limited(path, max_size)?;
    Ok(STANDARD.encode(&bytes))
}

/// Attachment size caps. Anthropic and OpenAI accept requests up to 32 MB;
/// Google's inline-data path caps the whole request at 20 MB, and base64
/// inflates by 4/3, so the raw PDF must stay under ~14 MB there.
pub const MAX_ATTACH_PDF: usize = MAX_PDF_SIZE;
pub const MAX_ATTACH_PDF_GOOGLE: usize = 14 * 1024 * 1024;

/// Result of executing a tool call.
pub enum ToolResult {
    /// Plain text content.
    Text(String),
    /// PDF content as base64-encoded bytes.
    PdfBase64(String),
    /// Image content with the media type required by multimodal APIs.
    ImageBase64 { data: String, media_type: String },
    /// Several images plus compact JSON metadata describing their request
    /// indices, errors, and duplicate references.
    ImageBatch {
        metadata: String,
        images: Vec<ToolImage>,
    },
    /// Error message.
    Error(String),
}

pub struct ToolImage {
    pub data: String,
    pub media_type: String,
}

// ── Anthropic-specific types ───────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct AnthropicRequest {
    pub model: String,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    pub messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<serde_json::Value>,
    /// e.g. {"effort": "high"} — omitted entirely when no effort is configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct AnthropicResponse {
    /// Keep content blocks losslessly. Hosted tools add encrypted result and
    /// citation fields that must be echoed byte-for-byte on a continuation.
    pub content: Vec<serde_json::Value>,
    pub stop_reason: Option<String>,
    pub usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
pub struct AnthropicUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub server_tool_use: Option<AnthropicServerToolUsage>,
}

#[derive(Debug, Deserialize)]
pub struct AnthropicServerToolUsage {
    pub web_search_requests: Option<u64>,
}

// ── OpenAI-specific types ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct OpenAIRequest {
    pub model: String,
    pub messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
    /// Keeps requests with the same prepared paper/orientation prefix routed
    /// together for OpenAI's automatic prompt cache.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIMessage {
    pub role: String,
    /// A plain string for normal messages, or an array of content parts
    /// (text + file attachments) for multimodal user messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: OpenAIFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Deserialize)]
pub struct OpenAIResponse {
    pub choices: Vec<OpenAIChoice>,
    pub usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAIChoice {
    pub message: OpenAIMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAIUsage {
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub prompt_tokens_details: Option<OpenAIPromptTokenDetails>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAIPromptTokenDetails {
    pub cached_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
}

// ── Google-specific types ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct GoogleRequest {
    pub contents: Vec<GoogleContent>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "systemInstruction")]
    pub system_instruction: Option<GoogleContent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "toolConfig")]
    pub tool_config: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "generationConfig")]
    pub generation_config: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GoogleContent {
    #[serde(default)]
    pub role: String,
    /// Keep every part losslessly. Combined Google Search + custom-function
    /// responses carry server tool context and thought signatures that must be
    /// echoed unchanged on the next turn. Gemini omits `parts` entirely when
    /// thinking consumed the whole output budget (MAX_TOKENS).
    #[serde(default)]
    pub parts: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleFunctionCall {
    pub name: String,
    pub args: serde_json::Value,
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleFunctionResponse {
    pub name: String,
    pub response: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GoogleResponse {
    pub candidates: Option<Vec<GoogleCandidate>>,
    pub error: Option<GoogleError>,
    #[serde(rename = "usageMetadata")]
    pub usage_metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct GoogleCandidate {
    /// Omitted entirely on SAFETY/RECITATION-blocked candidates; default so
    /// the finish-reason diagnostics stay reachable instead of failing
    /// deserialization.
    #[serde(default)]
    pub content: GoogleContent,
    #[serde(rename = "finishReason")]
    pub finish_reason: Option<String>,
    #[serde(rename = "groundingMetadata")]
    pub grounding_metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct GoogleError {
    pub message: String,
    pub code: Option<u32>,
}

// ── Usage tracking ─────────────────────────────────────────────────

/// Accumulated token usage across all round-trips in a tool loop.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_write_input_tokens: u64,
    pub requests: u32,
    pub tool_calls: crate::models::ToolCallCounts,
}

impl Usage {
    pub fn add(&mut self, input: u64, output: u64, cached: u64, cache_write: u64) {
        self.input_tokens += input;
        self.output_tokens += output;
        self.cached_input_tokens += cached;
        self.cache_write_input_tokens += cache_write;
        self.requests += 1;
    }

    pub fn call_usage(&self) -> super::logging::CallUsage {
        super::logging::CallUsage {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cached_input_tokens: self.cached_input_tokens,
            cache_write_input_tokens: self.cache_write_input_tokens,
            model_round_trips: u64::from(self.requests),
            tool_calls: self.tool_calls,
            ..Default::default()
        }
    }

    pub fn merge(&mut self, other: Usage) {
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.cached_input_tokens = self
            .cached_input_tokens
            .saturating_add(other.cached_input_tokens);
        self.cache_write_input_tokens = self
            .cache_write_input_tokens
            .saturating_add(other.cache_write_input_tokens);
        self.requests = self.requests.saturating_add(other.requests);
        self.tool_calls.add_counts(other.tool_calls);
    }

    /// Format as a compact string for log lines.
    pub fn summary(&self) -> String {
        let total = self.input_tokens + self.output_tokens;
        if total == 0 {
            return String::new();
        }
        let cache = if self.cached_input_tokens > 0 || self.cache_write_input_tokens > 0 {
            format!(
                ", {} cached/{} cache-write",
                self.cached_input_tokens, self.cache_write_input_tokens
            )
        } else {
            String::new()
        };
        format!(
            ", {}+{} tokens{cache} ({} req)",
            self.input_tokens, self.output_tokens, self.requests
        )
    }
}

// ── Tool-use loop (generic) ────────────────────────────────────────

/// Extract text from an Anthropic response's content blocks.
pub fn anthropic_extract_text(blocks: &[serde_json::Value]) -> String {
    blocks
        .iter()
        .filter_map(|block| {
            (block.get("type").and_then(serde_json::Value::as_str) == Some("text"))
                .then(|| block.get("text").and_then(serde_json::Value::as_str))
                .flatten()
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Check if Anthropic response wants tool calls.
pub fn anthropic_has_tool_use(blocks: &[serde_json::Value]) -> bool {
    blocks
        .iter()
        .any(|block| block.get("type").and_then(serde_json::Value::as_str) == Some("tool_use"))
}

fn anthropic_output_incomplete(reason: Option<&str>) -> bool {
    reason == Some("max_tokens")
}

fn openai_output_incomplete(reason: Option<&str>) -> bool {
    matches!(reason, Some("length" | "content_filter"))
}

fn google_output_incomplete(reason: Option<&str>) -> bool {
    reason.is_some_and(|reason| reason != "STOP")
}

fn direct_tool_kind(name: &str) -> crate::models::ToolCallKind {
    match name {
        "Read" | "ReadTextBatch" | "Write" => crate::models::ToolCallKind::TextFile,
        "ReadDocumentAsset" | "ReadDocumentAssetsBatch" => crate::models::ToolCallKind::Image,
        other => super::logging::classify_tool_name(other),
    }
}

fn anthropic_hosted_search_count(usage: &AnthropicUsage) -> u64 {
    usage
        .server_tool_use
        .as_ref()
        .and_then(|server| server.web_search_requests)
        .unwrap_or(0)
}

fn google_hosted_search_count(candidate: &GoogleCandidate) -> u64 {
    let mut queries = std::collections::HashSet::new();
    let mut record_queries = |values: Option<&Vec<serde_json::Value>>| {
        for query in values.into_iter().flatten() {
            if let Some(query) = query
                .as_str()
                .map(str::trim)
                .filter(|query| !query.is_empty())
            {
                queries.insert(query.to_string());
            }
        }
    };
    record_queries(
        candidate
            .grounding_metadata
            .as_ref()
            .and_then(|metadata| metadata.get("webSearchQueries"))
            .and_then(serde_json::Value::as_array),
    );
    for part in &candidate.content.parts {
        let Some(tool_call) = part.get("toolCall") else {
            continue;
        };
        let is_google_search = tool_call
            .get("toolType")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|kind| kind.starts_with("GOOGLE_SEARCH"));
        if is_google_search {
            record_queries(
                tool_call
                    .get("args")
                    .and_then(|args| args.get("queries"))
                    .and_then(serde_json::Value::as_array),
            );
        }
    }
    queries.len() as u64
}

/// Run the tool-use loop for Anthropic. Returns (text, usage).
pub async fn anthropic_tool_loop(
    app: &crate::emit::EventBus,
    client: &reqwest::Client,
    api_key: &str,
    mut request: AnthropicRequest,
    timeout_secs: u64,
    label: &str,
    access: &ToolAccess,
) -> Result<(String, Usage), String> {
    let mut usage = Usage::default();
    let mut tool_budget = ToolBudget::default();
    let start = std::time::Instant::now();

    for iteration in 0..MAX_TOOL_ITERATIONS {
        if crate::commands::is_cancelled() {
            return Err("Pipeline cancelled".into());
        }
        validate_tool_history(&request.messages, "Anthropic")?;

        let elapsed = start.elapsed().as_secs();
        if elapsed + MIN_REMAINING_SECS > timeout_secs {
            return Err(format!(
                "Step timeout ({timeout_secs}s) exceeded after {iteration} API requests"
            ));
        }
        let request_timeout = timeout_secs - elapsed;

        let pass_key = super::logging::current_pass();
        let request_builder = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .timeout(std::time::Duration::from_secs(request_timeout))
            .json(&request);
        let resp = send_with_status_retry(
            app,
            "Anthropic",
            label,
            request_builder,
            pass_key.as_deref(),
        )
        .await?;

        let status = resp.status();
        if !status.is_success() {
            let bytes =
                response_bytes_limited(resp, MAX_API_ERROR_BYTES, pass_key.as_deref()).await?;
            return Err(format_api_error(
                "Anthropic",
                status.as_u16(),
                &String::from_utf8_lossy(&bytes),
            ));
        }

        let bytes =
            response_bytes_limited(resp, MAX_API_RESPONSE_BYTES, pass_key.as_deref()).await?;
        let body: AnthropicResponse = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Failed to parse Anthropic response: {e}"))?;

        if let Some(u) = &body.usage {
            let uncached = u.input_tokens.unwrap_or(0);
            let cached = u.cache_read_input_tokens.unwrap_or(0);
            let cache_write = u.cache_creation_input_tokens.unwrap_or(0);
            let inp = uncached.saturating_add(cached).saturating_add(cache_write);
            let out = u.output_tokens.unwrap_or(0);
            let web_searches = anthropic_hosted_search_count(u);
            usage.add(inp, out, cached, cache_write);
            usage
                .tool_calls
                .add_kind(crate::models::ToolCallKind::Web, web_searches);
            verbose_log(
                app,
                format!(
                    "[api] {label}: tokens in={inp} out={out} cached={cached} cache-write={cache_write} hosted-web-searches={web_searches}"
                ),
            );
        } else {
            usage.requests = usage.requests.saturating_add(1);
        }

        if body.stop_reason.as_deref() == Some("pause_turn") {
            request.messages.push(AnthropicMessage {
                role: "assistant".to_string(),
                content: serde_json::Value::Array(body.content),
            });
            continue;
        }

        if !anthropic_has_tool_use(&body.content) || body.stop_reason.as_deref() != Some("tool_use")
        {
            if anthropic_output_incomplete(body.stop_reason.as_deref()) {
                return Err(format!(
                    "Anthropic truncated {label} because the output-token limit was reached"
                ));
            }
            return Ok((anthropic_extract_text(&body.content), usage));
        }

        for block in &body.content {
            if block.get("type").and_then(serde_json::Value::as_str) == Some("tool_use") {
                let input = block.get("input").unwrap_or(&serde_json::Value::Null);
                let argument_bytes = serialized_size_limited(
                    input,
                    MAX_TOOL_ARGUMENT_BYTES,
                    "Anthropic tool-call arguments",
                )?;
                tool_budget.reserve_tool_call(argument_bytes)?;
                if let Some(name) = block.get("name").and_then(serde_json::Value::as_str) {
                    usage.tool_calls.add_kind(direct_tool_kind(name), 1);
                }
            }
        }

        // Build tool results
        let mut tool_results: Vec<serde_json::Value> = Vec::new();
        for block in &body.content {
            if block.get("type").and_then(serde_json::Value::as_str) != Some("tool_use") {
                continue;
            }
            let id = block
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let name = block
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let input = block.get("input").unwrap_or(&serde_json::Value::Null);
            let result = execute_tool(
                app,
                name,
                input,
                label,
                iteration,
                &mut tool_budget,
                access,
                &ANTHROPIC_MEDIA_POLICY,
            )
            .await;
            let (content, is_error) = anthropic_tool_result_content(result);
            let mut tool_result = serde_json::json!({
                "type": "tool_result",
                "tool_use_id": id,
                "content": content,
            });
            if is_error {
                tool_result["is_error"] = serde_json::Value::Bool(true);
            }
            tool_results.push(tool_result);
        }

        // Append assistant response + tool results to messages
        request.messages.push(AnthropicMessage {
            role: "assistant".to_string(),
            content: serde_json::Value::Array(body.content),
        });
        request.messages.push(AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::Value::Array(tool_results),
        });
    }

    Err(format!(
        "Tool loop exceeded {MAX_TOOL_ITERATIONS} iterations"
    ))
}

/// Run the tool-use loop for an OpenAI-compatible Chat Completions endpoint.
/// Returns (text, usage).
///
/// `base_url` is the API root without the `/chat/completions` suffix
/// (e.g. "https://api.openai.com/v1", "http://localhost:11434/v1").
/// `provider` names the endpoint in log/error messages.
/// When `drop_tools_on_400` is set (local servers), a 400 response to a
/// request that declared tools retries once without tools — many local
/// models don't support tool calling, and a hard failure would be opaque.
#[allow(clippy::too_many_arguments)]
pub async fn openai_tool_loop(
    app: &crate::emit::EventBus,
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    provider: &str,
    mut request: OpenAIRequest,
    timeout_secs: u64,
    label: &str,
    drop_tools_on_400: bool,
    access: &ToolAccess,
) -> Result<(String, Usage), String> {
    let mut usage = Usage::default();
    let mut tool_budget = ToolBudget::default();
    let start = std::time::Instant::now();
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let mut tools_retry_used = false;
    let mut effort_retry_used = false;

    for iteration in 0..MAX_TOOL_ITERATIONS {
        if crate::commands::is_cancelled() {
            return Err("Pipeline cancelled".into());
        }
        validate_tool_history(&request.messages, provider)?;

        let elapsed = start.elapsed().as_secs();
        if elapsed + MIN_REMAINING_SECS > timeout_secs {
            return Err(format!(
                "Step timeout ({timeout_secs}s) exceeded after {iteration} API requests"
            ));
        }
        let request_timeout = timeout_secs - elapsed;

        let mut req = client
            .post(&url)
            .timeout(std::time::Duration::from_secs(request_timeout))
            .json(&request);
        if !api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {api_key}"));
        }
        let pass_key = super::logging::current_pass();
        let resp = send_with_status_retry(app, provider, label, req, pass_key.as_deref()).await?;

        let status = resp.status();
        if !status.is_success() {
            let bytes =
                response_bytes_limited(resp, MAX_API_ERROR_BYTES, pass_key.as_deref()).await?;
            let body = String::from_utf8_lossy(&bytes);
            if !effort_retry_used
                && status.as_u16() == 400
                && request.reasoning_effort.is_some()
                && (body.contains("reasoning_effort") || body.contains("Unsupported parameter"))
            {
                log(
                    app,
                    format!(
                        "WARNING: {label}: {provider} rejected reasoning_effort for this model — retrying once without it"
                    ),
                );
                request.reasoning_effort = None;
                effort_retry_used = true;
                continue;
            }
            if drop_tools_on_400
                && !tools_retry_used
                && status.as_u16() == 400
                && !request.tools.is_empty()
            {
                log(app, format!(
                    "{label}: {provider} rejected the request with tools declared — retrying without tools. \
                     The model won't be able to read files; consider a tool-capable model."
                ));
                request.tools = Vec::new();
                tools_retry_used = true;
                continue;
            }
            return Err(format_api_error(provider, status.as_u16(), &body));
        }

        let bytes =
            response_bytes_limited(resp, MAX_API_RESPONSE_BYTES, pass_key.as_deref()).await?;
        let body: OpenAIResponse = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Failed to parse {provider} response: {e}"))?;

        if let Some(u) = &body.usage {
            let inp = u.prompt_tokens.unwrap_or(0);
            let out = u.completion_tokens.unwrap_or(0);
            let cached = u
                .prompt_tokens_details
                .as_ref()
                .and_then(|details| details.cached_tokens)
                .unwrap_or(0);
            let cache_write = u
                .prompt_tokens_details
                .as_ref()
                .and_then(|details| details.cache_write_tokens)
                .unwrap_or(0);
            usage.add(inp, out, cached, cache_write);
            verbose_log(
                app,
                format!(
                    "[api] {label}: tokens in={inp} out={out} cached={cached} cache-write={cache_write}"
                ),
            );
        } else {
            usage.requests = usage.requests.saturating_add(1);
        }

        let choice = body
            .choices
            .first()
            .ok_or_else(|| format!("{provider} returned no choices"))?;

        if let Some(tool_calls) = &choice.message.tool_calls {
            if !tool_calls.is_empty() {
                for tool_call in tool_calls {
                    tool_budget.reserve_tool_call(tool_call.function.arguments.len())?;
                    usage
                        .tool_calls
                        .add_kind(direct_tool_kind(&tool_call.function.name), 1);
                }
                // Append assistant message with tool calls
                request.messages.push(choice.message.clone());

                // Execute each tool and append results
                let mut pending_images = Vec::new();
                for tc in tool_calls {
                    let result =
                        match serde_json::from_str::<serde_json::Value>(&tc.function.arguments) {
                            Ok(input) => {
                                execute_tool(
                                    app,
                                    &tc.function.name,
                                    &input,
                                    label,
                                    iteration,
                                    &mut tool_budget,
                                    access,
                                    &OPENAI_MEDIA_POLICY,
                                )
                                .await
                            }
                            Err(error) => ToolResult::Error(format!(
                                "Tool arguments were not valid JSON: {error}"
                            )),
                        };
                    append_openai_tool_result(
                        &mut request.messages,
                        &tc.id,
                        result,
                        &mut pending_images,
                    );
                }
                // OpenAI requires the complete set of tool messages to follow
                // the assistant tool-call message before any new user content.
                append_openai_image_message(&mut request.messages, pending_images);
                continue;
            }
        }

        let text = choice
            .message
            .content
            .as_ref()
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if openai_output_incomplete(choice.finish_reason.as_deref()) {
            return Err(format!(
                "{provider} did not complete {label} (finish reason: {})",
                choice.finish_reason.as_deref().unwrap_or("unknown")
            ));
        }
        return Ok((text, usage));
    }

    Err(format!(
        "Tool loop exceeded {MAX_TOOL_ITERATIONS} iterations"
    ))
}

pub struct GoogleToolLoopContext<'a> {
    pub app: &'a crate::emit::EventBus,
    pub client: &'a reqwest::Client,
    pub api_key: &'a str,
    pub model: &'a str,
    pub timeout_secs: u64,
    pub label: &'a str,
    pub access: &'a ToolAccess,
}

/// Run the tool-use loop for Google. Returns (text, usage).
pub async fn google_tool_loop(
    context: GoogleToolLoopContext<'_>,
    mut request: GoogleRequest,
) -> Result<(String, Usage), String> {
    let GoogleToolLoopContext {
        app,
        client,
        api_key,
        model,
        timeout_secs,
        label,
        access,
    } = context;
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        model
    );
    let mut usage = Usage::default();
    let mut tool_budget = ToolBudget::default();
    let start = std::time::Instant::now();

    for iteration in 0..MAX_TOOL_ITERATIONS {
        if crate::commands::is_cancelled() {
            return Err("Pipeline cancelled".into());
        }
        validate_tool_history(&request.contents, "Google")?;

        let elapsed = start.elapsed().as_secs();
        if elapsed + MIN_REMAINING_SECS > timeout_secs {
            return Err(format!(
                "Step timeout ({timeout_secs}s) exceeded after {iteration} API requests"
            ));
        }
        let request_timeout = timeout_secs - elapsed;

        let pass_key = super::logging::current_pass();
        let request_builder = client
            .post(&url)
            .header("x-goog-api-key", api_key)
            .timeout(std::time::Duration::from_secs(request_timeout))
            .json(&request);
        let resp =
            send_with_status_retry(app, "Google", label, request_builder, pass_key.as_deref())
                .await?;

        let status = resp.status();
        if !status.is_success() {
            let bytes =
                response_bytes_limited(resp, MAX_API_ERROR_BYTES, pass_key.as_deref()).await?;
            return Err(format_api_error(
                "Google",
                status.as_u16(),
                &String::from_utf8_lossy(&bytes),
            ));
        }

        let bytes =
            response_bytes_limited(resp, MAX_API_RESPONSE_BYTES, pass_key.as_deref()).await?;
        let body: GoogleResponse = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Failed to parse Google response: {e}"))?;

        if let Some(error) = &body.error {
            return Err(format!("Google API error: {}", error.message));
        }

        // Google returns usageMetadata at the top level
        if let Some(um) = body.usage_metadata.as_ref() {
            let prompt = um
                .get("promptTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let tool_prompt = um
                .get("toolUsePromptTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let candidates = um
                .get("candidatesTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let thoughts = um
                .get("thoughtsTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let inp = prompt.saturating_add(tool_prompt);
            let out = candidates.saturating_add(thoughts);
            let cached = um
                .get("cachedContentTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            usage.add(inp, out, cached, 0);
            verbose_log(
                app,
                format!(
                    "[api] {label}: tokens in={inp} out={out} cached={cached} thoughts={thoughts} tool-prompt={tool_prompt}"
                ),
            );
        } else {
            usage.requests += 1;
        }

        let candidate = body
            .candidates
            .as_ref()
            .and_then(|c| c.first())
            .ok_or("Google returned no candidates")?;
        let hosted_web_searches = google_hosted_search_count(candidate);
        if hosted_web_searches > 0 {
            usage
                .tool_calls
                .add_kind(crate::models::ToolCallKind::Web, hosted_web_searches);
            verbose_log(
                app,
                format!("[api] {label}: hosted-web-searches={hosted_web_searches}"),
            );
        }

        // Check for function calls
        let function_calls: Vec<GoogleFunctionCall> = candidate
            .content
            .parts
            .iter()
            .filter_map(|part| part.get("functionCall"))
            .map(|call| {
                serde_json::from_value(call.clone())
                    .map_err(|error| format!("Failed to parse Google function call: {error}"))
            })
            .collect::<Result<_, _>>()?;

        if !function_calls.is_empty() {
            for function_call in &function_calls {
                let argument_bytes = serialized_size_limited(
                    &function_call.args,
                    MAX_TOOL_ARGUMENT_BYTES,
                    "Google tool-call arguments",
                )?;
                tool_budget.reserve_tool_call(argument_bytes)?;
                usage
                    .tool_calls
                    .add_kind(direct_tool_kind(&function_call.name), 1);
            }
            // Append model response to contents
            request.contents.push(candidate.content.clone());

            // Build function response parts
            let mut response_parts: Vec<serde_json::Value> =
                Vec::with_capacity(function_calls.len());
            let mut image_parts = Vec::new();
            for fc in &function_calls {
                let result = execute_tool(
                    app,
                    &fc.name,
                    &fc.args,
                    label,
                    iteration,
                    &mut tool_budget,
                    access,
                    &GOOGLE_MEDIA_POLICY,
                )
                .await;
                let (function_response, images) = google_tool_result_parts(fc, result);
                response_parts.push(function_response);
                image_parts.extend(images);
            }
            response_parts.extend(image_parts);

            request.contents.push(GoogleContent {
                role: "user".to_string(),
                parts: response_parts,
            });
            continue;
        }

        // Extract text
        let text: String = candidate
            .content
            .parts
            .iter()
            .filter_map(|part| part.get("text").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>()
            .join("");

        if google_output_incomplete(candidate.finish_reason.as_deref()) {
            return Err(format!(
                "Google did not complete {label} (finish reason: {})",
                candidate.finish_reason.as_deref().unwrap_or("unknown")
            ));
        }

        return Ok((text, usage));
    }

    Err(format!(
        "Tool loop exceeded {MAX_TOOL_ITERATIONS} iterations"
    ))
}

// ── Shared helpers ─────────────────────────────────────────────────

#[derive(Default)]
pub(crate) struct ToolBudget {
    read_calls: usize,
    read_bytes: usize,
    tool_calls: usize,
    argument_bytes: usize,
    encoded_media_bytes: usize,
}

impl ToolBudget {
    fn reserve_tool_call(&mut self, argument_bytes: usize) -> Result<(), String> {
        if argument_bytes > MAX_TOOL_ARGUMENT_BYTES {
            return Err(format!(
                "Tool-call arguments exceeded the {} MB per-call safety limit",
                MAX_TOOL_ARGUMENT_BYTES / 1024 / 1024
            ));
        }
        let next_calls = self
            .tool_calls
            .checked_add(1)
            .ok_or("Tool-call count overflowed")?;
        if next_calls > MAX_TOOL_CALLS {
            return Err(format!(
                "Tool-call count exceeded the {MAX_TOOL_CALLS}-call safety limit"
            ));
        }
        let next_bytes = self
            .argument_bytes
            .checked_add(argument_bytes)
            .ok_or("Tool-call argument byte count overflowed")?;
        if next_bytes > MAX_TOOL_ARGUMENT_BYTES_TOTAL {
            return Err(format!(
                "Tool-call arguments exceeded the {} MB cumulative safety limit",
                MAX_TOOL_ARGUMENT_BYTES_TOTAL / 1024 / 1024
            ));
        }
        self.tool_calls = next_calls;
        self.argument_bytes = next_bytes;
        Ok(())
    }

    fn reserve_read(&mut self) -> Result<usize, String> {
        if self.read_calls >= MAX_TOOL_READ_CALLS {
            return Err(format!(
                "Read limit reached ({MAX_TOOL_READ_CALLS} calls per model invocation)"
            ));
        }
        let remaining = MAX_TOOL_READ_BYTES.saturating_sub(self.read_bytes);
        if remaining == 0 {
            return Err(format!(
                "Read byte budget reached ({} MB per model invocation)",
                MAX_TOOL_READ_BYTES / 1024 / 1024
            ));
        }
        self.read_calls += 1;
        Ok(remaining)
    }

    fn record_read_bytes(&mut self, bytes: usize) {
        self.read_bytes = self
            .read_bytes
            .saturating_add(bytes)
            .min(MAX_TOOL_READ_BYTES);
    }
}

/// Base64-encoded image/PDF bytes this tool result would add to the request.
fn encoded_media_bytes(result: &ToolResult) -> usize {
    match result {
        ToolResult::PdfBase64(data) => data.len(),
        ToolResult::ImageBase64 { data, .. } => data.len(),
        ToolResult::ImageBatch { images, .. } => images.iter().map(|image| image.data.len()).sum(),
        ToolResult::Text(_) | ToolResult::Error(_) => 0,
    }
}

/// Replace a media-bearing result with a tool error once its payload would
/// push the conversation past the provider's per-request media budget.
fn enforce_media_budget(
    result: ToolResult,
    budget: &mut ToolBudget,
    media: &MediaPolicy,
) -> ToolResult {
    let encoded = encoded_media_bytes(&result);
    if encoded == 0 {
        return result;
    }
    let next = budget.encoded_media_bytes.saturating_add(encoded);
    if next > media.encoded_media_budget {
        return ToolResult::Error(format!(
            "The image/PDF budget for this request ({} MB encoded) is exhausted; reason from the already-provided content instead of requesting more media",
            media.encoded_media_budget / 1024 / 1024
        ));
    }
    budget.encoded_media_bytes = next;
    result
}

const TOOL_IO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
type TextBatchRequestKey = (String, Option<usize>, Option<usize>, Option<usize>, usize);

async fn run_blocking_tool<T, F>(operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    let pass_key = super::logging::current_pass();
    let task = tokio::task::spawn_blocking(operation);
    match tokio::time::timeout(TOOL_IO_TIMEOUT, await_or_cancel(task, pass_key.as_deref())).await {
        Ok(Ok(joined)) => joined.map_err(|e| format!("File tool task failed: {e}"))?,
        Ok(Err(cancelled)) => Err(cancelled),
        Err(_) => Err(format!(
            "File tool operation exceeded the {} second limit",
            TOOL_IO_TIMEOUT.as_secs()
        )),
    }
}

async fn execute_read_text_batch(
    input: &serde_json::Value,
    budget: &mut ToolBudget,
    access: &ToolAccess,
) -> ToolResult {
    let Some(requests) = input.get("requests").and_then(serde_json::Value::as_array) else {
        return ToolResult::Error("ReadTextBatch requires a requests array".to_string());
    };
    if requests.is_empty() || requests.len() > MAX_BATCH_TEXT_REQUESTS {
        return ToolResult::Error(format!(
            "ReadTextBatch accepts 1 to {MAX_BATCH_TEXT_REQUESTS} requests"
        ));
    }

    let mut items = Vec::with_capacity(requests.len());
    let mut seen: std::collections::HashMap<TextBatchRequestKey, usize> =
        std::collections::HashMap::new();
    // Cache both successful loads and validation/read errors. Distinct ranges
    // from one paper file therefore perform one filesystem read, while every
    // non-duplicate evidence range still consumes the existing read-call and
    // returned-byte budgets.
    let mut loaded_files: std::collections::HashMap<String, Result<String, String>> =
        std::collections::HashMap::new();
    let mut returned_bytes = 0usize;

    for (index, request) in requests.iter().enumerate() {
        let path = request
            .get("file_path")
            .or_else(|| request.get("path"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let offset = match request.get("offset") {
            Some(value) => match value.as_u64().and_then(|value| usize::try_from(value).ok()) {
                Some(value) => Some(value),
                None => {
                    items.push(serde_json::json!({
                        "index": index,
                        "error": "offset must be a non-negative integer"
                    }));
                    continue;
                }
            },
            None => None,
        };
        let start_line = match request.get("start_line") {
            Some(value) => match value.as_u64().and_then(|value| usize::try_from(value).ok()) {
                Some(value) if value > 0 => Some(value),
                _ => {
                    items.push(serde_json::json!({
                        "index": index,
                        "error": "start_line must be a positive 1-based integer"
                    }));
                    continue;
                }
            },
            None => None,
        };
        let end_line = match request.get("end_line") {
            Some(value) => match value.as_u64().and_then(|value| usize::try_from(value).ok()) {
                Some(value) if value > 0 => Some(value),
                _ => {
                    items.push(serde_json::json!({
                        "index": index,
                        "error": "end_line must be a positive 1-based integer"
                    }));
                    continue;
                }
            },
            None => None,
        };
        let max_bytes = match request.get("max_bytes") {
            Some(value) => match value.as_u64().and_then(|value| usize::try_from(value).ok()) {
                Some(value) if (1..=MAX_BATCH_TEXT_ITEM_BYTES).contains(&value) => value,
                _ => {
                    items.push(serde_json::json!({
                        "index": index,
                        "error": format!(
                            "max_bytes must be between 1 and {MAX_BATCH_TEXT_ITEM_BYTES}"
                        )
                    }));
                    continue;
                }
            },
            None => DEFAULT_BATCH_TEXT_ITEM_BYTES,
        };
        if path.is_empty() {
            items.push(serde_json::json!({
                "index": index,
                "error": "file_path is required"
            }));
            continue;
        }
        if offset.is_some() && (start_line.is_some() || end_line.is_some()) {
            items.push(serde_json::json!({
                "index": index,
                "error": "offset is mutually exclusive with start_line/end_line"
            }));
            continue;
        }
        if start_line
            .zip(end_line)
            .is_some_and(|(start, end)| end < start)
        {
            items.push(serde_json::json!({
                "index": index,
                "error": "end_line must be greater than or equal to start_line"
            }));
            continue;
        }

        let key = (path.to_string(), offset, start_line, end_line, max_bytes);
        if let Some(original) = seen.get(&key) {
            items.push(serde_json::json!({
                "index": index,
                "duplicate_of": original
            }));
            continue;
        }
        seen.insert(key, index);

        let batch_remaining = MAX_BATCH_TEXT_BYTES.saturating_sub(returned_bytes);
        if batch_remaining == 0 {
            items.push(serde_json::json!({
                "index": index,
                "error": format!(
                    "ReadTextBatch reached its {MAX_BATCH_TEXT_BYTES}-byte response budget"
                )
            }));
            continue;
        }
        let global_remaining = match budget.reserve_read() {
            Ok(remaining) => remaining,
            Err(error) => {
                items.push(serde_json::json!({ "index": index, "error": error }));
                continue;
            }
        };
        let limit = max_bytes.min(batch_remaining).min(global_remaining);
        if !loaded_files.contains_key(path) {
            let owned_path = path.to_string();
            let access = access.clone();
            let loaded = run_blocking_tool(move || {
                if owned_path.to_ascii_lowercase().ends_with(".pdf") {
                    return Err(
                        "Cannot read PDF as text. Use rendered page assets instead.".to_string()
                    );
                }
                read_file_for_tool_limited(&access, &owned_path, MAX_READ_SIZE)
            })
            .await;
            loaded_files.insert(path.to_string(), loaded);
        }
        let range = match loaded_files.get(path).expect("batch file cache populated") {
            Ok(text) => {
                let bounds = if start_line.is_some() || end_line.is_some() {
                    text_line_bounds(text, start_line, end_line)
                } else {
                    Ok((offset.unwrap_or(0), text.len()))
                };
                bounds.and_then(|(start, end)| slice_text_range(text, start, end, limit))
            }
            Err(error) => Err(error.clone()),
        };
        match range {
            Ok(range) => {
                let bytes = range.content.len();
                returned_bytes = returned_bytes.saturating_add(bytes);
                budget.record_read_bytes(bytes);
                items.push(serde_json::json!({
                    "index": index,
                    "start_offset": range.start_offset,
                    "end_offset": range.end_offset,
                    "requested_start_line": start_line,
                    "requested_end_line": end_line,
                    "next_offset": range.next_offset,
                    "truncated": range.next_offset.is_some(),
                    "content": range.content
                }));
            }
            Err(error) => items.push(serde_json::json!({ "index": index, "error": error })),
        }
    }

    match serde_json::to_string(&serde_json::json!({ "items": items })) {
        Ok(content) => ToolResult::Text(content),
        Err(error) => ToolResult::Error(format!("Failed to encode ReadTextBatch result: {error}")),
    }
}

async fn execute_read_assets_batch(
    input: &serde_json::Value,
    budget: &mut ToolBudget,
    access: &ToolAccess,
) -> ToolResult {
    let Some(paths) = input
        .get("file_paths")
        .or_else(|| input.get("paths"))
        .and_then(serde_json::Value::as_array)
    else {
        return ToolResult::Error(
            "ReadDocumentAssetsBatch requires a file_paths array".to_string(),
        );
    };
    if paths.is_empty() || paths.len() > MAX_BATCH_ASSET_REQUESTS {
        return ToolResult::Error(format!(
            "ReadDocumentAssetsBatch accepts 1 to {MAX_BATCH_ASSET_REQUESTS} paths"
        ));
    }

    let mut items = Vec::with_capacity(paths.len());
    let mut images = Vec::new();
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut returned_raw_bytes = 0usize;

    for (index, value) in paths.iter().enumerate() {
        let Some(path) = value.as_str().filter(|path| !path.is_empty()) else {
            items.push(serde_json::json!({
                "index": index,
                "error": "Every file_paths item must be a non-empty string"
            }));
            continue;
        };
        if let Some(original) = seen.get(path) {
            items.push(serde_json::json!({
                "index": index,
                "duplicate_of": original
            }));
            continue;
        }
        seen.insert(path.to_string(), index);

        let global_remaining = match budget.reserve_read() {
            Ok(remaining) => remaining,
            Err(error) => {
                items.push(serde_json::json!({ "index": index, "error": error }));
                continue;
            }
        };
        let batch_remaining = MAX_BATCH_ASSET_RAW_BYTES.saturating_sub(returned_raw_bytes);
        if batch_remaining == 0 {
            items.push(serde_json::json!({
                "index": index,
                "error": format!(
                    "ReadDocumentAssetsBatch reached its {} MiB raw-image budget",
                    MAX_BATCH_ASSET_RAW_BYTES / 1024 / 1024
                )
            }));
            continue;
        }
        let remaining = global_remaining.min(batch_remaining);
        let owned_path = path.to_string();
        let access = access.clone();
        match run_blocking_tool(move || read_image_for_tool(&access, &owned_path, remaining)).await
        {
            Ok((data, media_type, raw_bytes)) => {
                returned_raw_bytes = returned_raw_bytes.saturating_add(raw_bytes);
                budget.record_read_bytes(raw_bytes);
                let media_index = images.len();
                items.push(serde_json::json!({
                    "index": index,
                    "media_index": media_index,
                    "media_type": media_type
                }));
                images.push(ToolImage { data, media_type });
            }
            Err(error) => items.push(serde_json::json!({ "index": index, "error": error })),
        }
    }

    let metadata = match serde_json::to_string(&serde_json::json!({ "items": items })) {
        Ok(metadata) => metadata,
        Err(error) => {
            return ToolResult::Error(format!(
                "Failed to encode ReadDocumentAssetsBatch result: {error}"
            ))
        }
    };
    if images.is_empty() {
        ToolResult::Text(metadata)
    } else {
        ToolResult::ImageBatch { metadata, images }
    }
}

fn anthropic_tool_result_content(result: ToolResult) -> (serde_json::Value, bool) {
    match result {
        ToolResult::Text(text) => (serde_json::Value::String(text), false),
        ToolResult::PdfBase64(data) => (
            serde_json::json!([{
                "type": "document",
                "source": {
                    "type": "base64",
                    "media_type": "application/pdf",
                    "data": data
                }
            }]),
            false,
        ),
        ToolResult::ImageBase64 { data, media_type } => (
            serde_json::json!([{
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": media_type,
                    "data": data
                }
            }]),
            false,
        ),
        ToolResult::ImageBatch { metadata, images } => {
            let mut blocks = vec![serde_json::json!({ "type": "text", "text": metadata })];
            blocks.extend(images.into_iter().map(|image| {
                serde_json::json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": image.media_type,
                        "data": image.data
                    }
                })
            }));
            (serde_json::Value::Array(blocks), false)
        }
        ToolResult::Error(message) => (serde_json::Value::String(message), true),
    }
}

fn append_openai_tool_result(
    messages: &mut Vec<OpenAIMessage>,
    tool_call_id: &str,
    result: ToolResult,
    pending_images: &mut Vec<ToolImage>,
) {
    let (tool_text, images) = match result {
        ToolResult::Text(text) => (text, Vec::new()),
        ToolResult::PdfBase64(_) => (
            "Cannot read PDF visually via this API. Use the extracted paper text or rendered page assets instead.".to_string(),
            Vec::new(),
        ),
        ToolResult::ImageBase64 { data, media_type } => (
            "The requested document image is attached in the next message.".to_string(),
            vec![ToolImage { data, media_type }],
        ),
        ToolResult::ImageBatch { metadata, images } => (metadata, images),
        ToolResult::Error(message) => (message, Vec::new()),
    };
    messages.push(OpenAIMessage {
        role: "tool".to_string(),
        content: Some(serde_json::Value::String(tool_text)),
        tool_calls: None,
        tool_call_id: Some(tool_call_id.to_string()),
    });
    pending_images.extend(images);
}

fn append_openai_image_message(messages: &mut Vec<OpenAIMessage>, images: Vec<ToolImage>) {
    if images.is_empty() {
        return;
    }
    let mut parts = vec![serde_json::json!({
        "type": "text",
        "text": "Visual document assets returned by the preceding tool call, in media_index order."
    })];
    parts.extend(images.into_iter().map(|image| {
        serde_json::json!({
            "type": "image_url",
            "image_url": {
                "url": format!("data:{};base64,{}", image.media_type, image.data)
            }
        })
    }));
    messages.push(OpenAIMessage {
        role: "user".to_string(),
        content: Some(serde_json::Value::Array(parts)),
        tool_calls: None,
        tool_call_id: None,
    });
}

fn google_tool_result_parts(
    call: &GoogleFunctionCall,
    result: ToolResult,
) -> (serde_json::Value, Vec<serde_json::Value>) {
    let (content, images) = match result {
        ToolResult::Text(text) => (text, Vec::new()),
        ToolResult::PdfBase64(_) => (
            "Cannot read PDF visually via this tool. Use the extracted paper text or rendered page assets instead.".to_string(),
            Vec::new(),
        ),
        ToolResult::ImageBase64 { data, media_type } => (
            "The requested document image is included as inline visual data.".to_string(),
            vec![ToolImage { data, media_type }],
        ),
        ToolResult::ImageBatch { metadata, images } => (metadata, images),
        ToolResult::Error(message) => (message, Vec::new()),
    };
    let function_response = serde_json::json!({
        "functionResponse": GoogleFunctionResponse {
            name: call.name.clone(),
            response: serde_json::json!({ "content": content }),
            id: call.id.clone(),
        },
    });
    let image_parts = images
        .into_iter()
        .map(|image| {
            serde_json::json!({
                "inlineData": {
                    "data": image.data,
                    "mimeType": image.media_type,
                }
            })
        })
        .collect();
    (function_response, image_parts)
}

/// Execute a tool call. Filesystem work runs on the blocking pool so a slow
/// or hostile filesystem entry cannot stall the async API loop.
#[allow(clippy::too_many_arguments)]
async fn execute_tool(
    app: &crate::emit::EventBus,
    name: &str,
    input: &serde_json::Value,
    label: &str,
    iteration: usize,
    budget: &mut ToolBudget,
    access: &ToolAccess,
    media: &MediaPolicy,
) -> ToolResult {
    let result = match name {
        "Read" => {
            let path = input
                .get("file_path")
                .or_else(|| input.get("path"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            verbose_log(
                app,
                format!("[api] {label}: Read tool call #{} -> {path}", iteration + 1),
            );
            let is_pdf = path.to_lowercase().ends_with(".pdf");
            if is_pdf && !media.pdf_reads {
                // Reject before reading so the read budget is not charged for
                // bytes the tool-result converter would discard anyway.
                return ToolResult::Error(
                    "This provider cannot accept PDF content from Read. Use the extracted document text or rendered page assets instead.".to_string(),
                );
            }
            let remaining = match budget.reserve_read() {
                Ok(remaining) => remaining,
                Err(error) => return ToolResult::Error(error),
            };
            if is_pdf {
                let owned_path = path.to_string();
                let access = access.clone();
                match run_blocking_tool(move || read_pdf_for_tool(&access, &owned_path, remaining))
                    .await
                {
                    Ok(data) => {
                        budget.record_read_bytes(data.len().saturating_mul(3) / 4);
                        ToolResult::PdfBase64(data)
                    }
                    Err(e) => ToolResult::Error(e),
                }
            } else {
                let owned_path = path.to_string();
                let access = access.clone();
                match run_blocking_tool(move || {
                    read_file_for_tool_limited(&access, &owned_path, remaining)
                })
                .await
                {
                    Ok(content) => {
                        budget.record_read_bytes(content.len());
                        ToolResult::Text(content)
                    }
                    Err(e) => ToolResult::Error(e),
                }
            }
        }
        "ReadTextBatch" => {
            verbose_log(
                app,
                format!("[api] {label}: ReadTextBatch tool call #{}", iteration + 1),
            );
            execute_read_text_batch(input, budget, access).await
        }
        "ReadDocumentAsset" => {
            let path = input
                .get("file_path")
                .or_else(|| input.get("path"))
                .and_then(|value| value.as_str())
                .unwrap_or("");
            verbose_log(
                app,
                format!(
                    "[api] {label}: ReadDocumentAsset tool call #{} -> {path}",
                    iteration + 1
                ),
            );
            let remaining = match budget.reserve_read() {
                Ok(remaining) => remaining,
                Err(error) => return ToolResult::Error(error),
            };
            let owned_path = path.to_string();
            let access = access.clone();
            match run_blocking_tool(move || read_image_for_tool(&access, &owned_path, remaining))
                .await
            {
                Ok((data, media_type, raw_bytes)) => {
                    budget.record_read_bytes(raw_bytes);
                    ToolResult::ImageBase64 { data, media_type }
                }
                Err(error) => ToolResult::Error(error),
            }
        }
        "ReadDocumentAssetsBatch" => {
            verbose_log(
                app,
                format!(
                    "[api] {label}: ReadDocumentAssetsBatch tool call #{}",
                    iteration + 1
                ),
            );
            execute_read_assets_batch(input, budget, access).await
        }
        "Write" => {
            let path = input
                .get("file_path")
                .or_else(|| input.get("path"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let content = input.get("content").and_then(|v| v.as_str()).unwrap_or("");
            verbose_log(
                app,
                format!(
                    "[api] {label}: Write tool call #{} -> {path} ({} bytes)",
                    iteration + 1,
                    content.len()
                ),
            );
            let owned_path = path.to_string();
            let owned_content = content.to_string();
            let access = access.clone();
            match run_blocking_tool(move || {
                write_file_for_tool(&access, &owned_path, &owned_content)
            })
            .await
            {
                Ok(msg) => ToolResult::Text(msg),
                Err(e) => ToolResult::Error(e),
            }
        }
        _ => {
            verbose_log(
                app,
                format!("[api] {label}: unknown tool '{name}', skipping"),
            );
            ToolResult::Error(format!("Tool '{name}' is not available"))
        }
    };
    enforce_media_budget(result, budget, media)
}

/// Backoff before each retry of a transient status, when the response
/// carries no usable Retry-After header.
const TRANSIENT_STATUS_BACKOFF_SECS: [u64; 2] = [2, 8];
/// Ceiling on a server-requested Retry-After delay.
const MAX_RETRY_AFTER_SECS: u64 = 60;

/// Rate-limit and overload statuses worth an in-client retry.
fn transient_api_status(status: u16) -> bool {
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
async fn send_with_status_retry(
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

fn format_api_error(provider: &str, status: u16, body: &str) -> String {
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
        429 if super::provider_error::is_usage_limit_error(&detail) => {
            format!("{provider}: Usage limit reached: {detail}")
        }
        429 => format!("{provider}: Rate limited. Wait a moment and try again."),
        529 | 503 => format!("{provider}: Service overloaded. Try again in a few minutes."),
        _ => format!("{provider} API error (HTTP {status}): {detail}"),
    }
}

/// Bounded host tool execution used by native provider bridges. Authority is
/// supplied by the host invocation, never by model-provided identifiers.
pub(crate) async fn execute_native_tool(
    app: &crate::emit::EventBus,
    name: &str,
    input: &serde_json::Value,
    budget: &mut ToolBudget,
    access: &ToolAccess,
) -> ToolResult {
    let bytes = match serde_json::to_vec(input) {
        Ok(bytes) => bytes.len(),
        Err(error) => return ToolResult::Error(error.to_string()),
    };
    if let Err(error) = budget.reserve_tool_call(bytes) {
        return ToolResult::Error(error);
    }
    // Writes are bounded and atomic. Complete them in this poll so dropping
    // the invocation cannot leave a detached mutation racing artifact ingest.
    if name == "Write" {
        let path = input
            .get("file_path")
            .or_else(|| input.get("path"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let Some(content) = input.get("content").and_then(serde_json::Value::as_str) else {
            return ToolResult::Error("Write requires string content".into());
        };
        return match write_file_for_tool(access, path, content) {
            Ok(message) => ToolResult::Text(message),
            Err(error) => ToolResult::Error(error),
        };
    }
    let path = input
        .get("file_path")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if name == "ReadPdfPage" || (name == "Read" && path.to_ascii_lowercase().ends_with(".pdf")) {
        let page = input
            .get("page")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1);
        if page == 0 || page > crate::pipeline::extract::MAX_RENDERED_PDF_PAGES as u64 {
            return ToolResult::Error("PDF page is outside the supported range".into());
        }
        let remaining = match budget.reserve_read() {
            Ok(n) => n,
            Err(e) => return ToolResult::Error(e),
        };
        let access = access.clone();
        let path = path.to_string();
        let result = run_blocking_tool(move || {
            let canonical = validate_tool_path(&access, &path, MAX_TOOL_PDF_SIZE.min(remaining))?;
            let pdf = read_bytes_limited(&canonical, MAX_TOOL_PDF_SIZE.min(remaining))?;
            let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
            let staged = temp.path().join("input.pdf");
            std::fs::write(&staged, &pdf).map_err(|e| e.to_string())?;
            let output = temp.path().join("pages");
            let preview = crate::pipeline::extract::render_pdf_page_preview(&staged, &output, page as u32)?;
            let bytes = read_bytes_limited(&output.join(&preview.name), 4 * 1024 * 1024)?;
            Ok((pdf.len(), ToolResult::ImageBatch {
                metadata: serde_json::json!({"page":page,"has_next":preview.has_next,"next_page":preview.has_next.then_some(page+1),"instruction":"Call ReadPdfPage with file_path and page to inspect another page."}).to_string(),
                images: vec![ToolImage { data: STANDARD.encode(bytes), media_type: "image/jpeg".into() }],
            }))
        }).await;
        return match result {
            Ok((bytes, result)) => {
                budget.record_read_bytes(bytes);
                enforce_media_budget(result, budget, &OPENAI_MEDIA_POLICY)
            }
            Err(error) => ToolResult::Error(error),
        };
    }
    execute_tool(
        app,
        name,
        input,
        "App Server",
        budget.tool_calls,
        budget,
        access,
        &OPENAI_MEDIA_POLICY,
    )
    .await
}

#[cfg(test)]
mod tests;
