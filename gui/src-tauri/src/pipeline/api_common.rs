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

/// Shared HTTP client for all direct API calls.
/// `reqwest::Client` wraps an `Arc` internally, so cloning is cheap.
/// Reusing a single client enables TCP/TLS connection pooling across
/// pipeline steps that hit the same API host.
pub static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

/// Maximum text file size for Read tool calls (5 MB).
const MAX_READ_SIZE: usize = 5 * 1024 * 1024;

/// Maximum PDF file size for Read tool calls (32 MB — matches Anthropic's document limit).
const MAX_PDF_SIZE: usize = 32 * 1024 * 1024;
/// Maximum image size for a visual document-asset read.
const MAX_IMAGE_SIZE: usize = 20 * 1024 * 1024;
/// Cumulative direct-API Read budget per model call. This bounds repeated
/// reads of the same large PDF across tool iterations.
const MAX_TOOL_READ_BYTES: usize = 40 * 1024 * 1024;
const MAX_TOOL_READ_CALLS: usize = 32;

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

async fn response_bytes_limited(
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

/// Allowed directories for API tool reads, set before each pipeline run.
/// This restricts the LLM to reading only temp files and the paper's source directory.
static ALLOWED_DIRS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Set the allowed directories for file reads via the direct API path.
/// Call this before starting a pipeline run.
pub fn set_allowed_dirs(dirs: Vec<String>) {
    if let Ok(mut allowed) = ALLOWED_DIRS.lock() {
        *allowed = dirs;
    }
}

/// The single directory Write tool calls may target, set before each run
/// (the run's `artifacts/` dir). `None` disables the Write tool entirely.
static WRITE_DIR: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

/// Write calls consumed this run, reset by `set_write_dir`.
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

/// Set (or clear) the directory Write tool calls are confined to.
/// Call this before starting a pipeline run; pass `None` to disable writes.
pub fn set_write_dir(dir: Option<std::path::PathBuf>) {
    if let Ok(mut wd) = WRITE_DIR.lock() {
        *wd = dir;
    }
    WRITE_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);
    WRITE_BYTES.store(0, std::sync::atomic::Ordering::SeqCst);
}

/// Validate and perform a Write tool call. The destination must resolve
/// inside the configured write dir; `..` components, absolute paths outside
/// the dir, and symlinked destinations are rejected. Parent subdirectories
/// are created as needed.
pub fn write_file_for_tool(path: &str, content: &str) -> Result<String, String> {
    let root = WRITE_DIR
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or("File writing is not enabled for this run")?;
    // The run dir is created before any LLM call, so this canonicalizes.
    let root = root
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
        let raw = WRITE_DIR
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .unwrap_or_default();
        p.strip_prefix(&root)
            .or_else(|_| p.strip_prefix(&raw))
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
fn validate_tool_path(path: &str, max_size: usize) -> Result<std::path::PathBuf, String> {
    let p = std::path::Path::new(path);

    // Resolve to canonical path to prevent traversal via symlinks or ..
    let canonical = p
        .canonicalize()
        .map_err(|_| format!("File not found: {path}"))?;

    // Validate the path against the exact roots registered for this run. The
    // OS-wide temporary directory is deliberately not implicit: unrelated
    // applications commonly place credentials and private documents there.
    let allowed = ALLOWED_DIRS.lock().unwrap_or_else(|e| e.into_inner());
    let is_allowed = allowed.iter().any(|dir| {
        std::path::Path::new(dir)
            .canonicalize()
            .map(|d| canonical.starts_with(&d))
            .unwrap_or(false)
    });
    drop(allowed);

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
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options
        .open(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|e| format!("Failed to inspect {}: {e}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "Access denied: {} is not a regular file",
            path.display()
        ));
    }
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

fn read_file_for_tool_limited(path: &str, limit: usize) -> Result<String, String> {
    let limit = MAX_READ_SIZE.min(limit);
    let canonical = validate_tool_path(path, limit)?;

    if path.to_lowercase().ends_with(".pdf") {
        return Err("Cannot read PDF as text. Use the extracted paper text instead.".into());
    }

    String::from_utf8(read_bytes_limited(&canonical, limit)?)
        .map_err(|e| format!("Failed to read {path} as UTF-8: {e}"))
}

/// Read a PDF file and return its contents as base64-encoded bytes.
fn read_pdf_for_tool(path: &str, limit: usize) -> Result<String, String> {
    let limit = MAX_PDF_SIZE.min(limit);
    let canonical = validate_tool_path(path, limit)?;
    let bytes = read_bytes_limited(&canonical, limit)?;
    Ok(STANDARD.encode(&bytes))
}

fn read_image_for_tool(path: &str, limit: usize) -> Result<(String, String), String> {
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
    let canonical = validate_tool_path(path, limit)?;
    let bytes = read_bytes_limited(&canonical, limit)?;
    Ok((STANDARD.encode(bytes), media_type.to_string()))
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
    /// Error message.
    Error(String),
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
    pub content: Vec<AnthropicContentBlock>,
    pub stop_reason: Option<String>,
    pub usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
pub struct AnthropicUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        /// String for text results, array of content blocks for PDFs/documents.
        content: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        #[serde(default)]
        signature: String,
    },
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
}

// ── Google-specific types ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct GoogleRequest {
    pub contents: Vec<GoogleContent>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "systemInstruction")]
    pub system_instruction: Option<GoogleContent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "generationConfig")]
    pub generation_config: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleContent {
    pub role: String,
    pub parts: Vec<GooglePart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GooglePart {
    Text {
        text: String,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GoogleFunctionCall,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: GoogleFunctionResponse,
    },
    InlineData {
        #[serde(rename = "inlineData")]
        inline_data: GoogleInlineData,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleInlineData {
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleFunctionCall {
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleFunctionResponse {
    pub name: String,
    pub response: serde_json::Value,
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
    pub content: GoogleContent,
    #[serde(rename = "finishReason")]
    pub finish_reason: Option<String>,
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
pub fn anthropic_extract_text(blocks: &[AnthropicContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|b| match b {
            AnthropicContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Check if Anthropic response wants tool calls.
pub fn anthropic_has_tool_use(blocks: &[AnthropicContentBlock]) -> bool {
    blocks
        .iter()
        .any(|b| matches!(b, AnthropicContentBlock::ToolUse { .. }))
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

/// Run the tool-use loop for Anthropic. Returns (text, usage).
pub async fn anthropic_tool_loop(
    app: &crate::emit::EventBus,
    client: &reqwest::Client,
    api_key: &str,
    mut request: AnthropicRequest,
    timeout_secs: u64,
    label: &str,
) -> Result<(String, Usage), String> {
    let mut usage = Usage::default();
    let mut tool_budget = ToolBudget::default();
    let start = std::time::Instant::now();

    for iteration in 0..MAX_TOOL_ITERATIONS {
        if crate::commands::is_cancelled() {
            return Err("Pipeline cancelled".into());
        }

        let elapsed = start.elapsed().as_secs();
        if elapsed + MIN_REMAINING_SECS > timeout_secs {
            return Err(format!(
                "Step timeout ({timeout_secs}s) exceeded after {iteration} API requests"
            ));
        }
        let request_timeout = timeout_secs - elapsed;

        let pass_key = super::logging::current_pass();
        let request_future = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .timeout(std::time::Duration::from_secs(request_timeout))
            .json(&request)
            .send();
        let resp = await_or_cancel(request_future, pass_key.as_deref())
            .await?
            .map_err(|e| format_http_error("Anthropic", &e))?;

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
            usage.add(inp, out, cached, cache_write);
            verbose_log(
                app,
                format!(
                    "[api] {label}: tokens in={inp} out={out} cached={cached} cache-write={cache_write}"
                ),
            );
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

        // Build tool results
        let mut tool_results: Vec<AnthropicContentBlock> = Vec::new();
        for block in &body.content {
            if let AnthropicContentBlock::ToolUse { id, name, input } = block {
                let result =
                    execute_tool(app, name, input, label, iteration, &mut tool_budget).await;
                let (content, is_error) = match result {
                    ToolResult::Text(text) => (serde_json::Value::String(text), None),
                    ToolResult::PdfBase64(data) => (
                        serde_json::json!([{
                            "type": "document",
                            "source": {
                                "type": "base64",
                                "media_type": "application/pdf",
                                "data": data
                            }
                        }]),
                        None,
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
                        None,
                    ),
                    ToolResult::Error(msg) => (serde_json::Value::String(msg), Some(true)),
                };
                tool_results.push(AnthropicContentBlock::ToolResult {
                    tool_use_id: id.clone(),
                    content,
                    is_error,
                });
            }
        }

        // Append assistant response + tool results to messages
        let assistant_content: Vec<serde_json::Value> = body
            .content
            .iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to serialize content block: {e}"))?;
        request.messages.push(AnthropicMessage {
            role: "assistant".to_string(),
            content: serde_json::Value::Array(assistant_content),
        });
        let results_content: Vec<serde_json::Value> = tool_results
            .iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to serialize tool result: {e}"))?;
        request.messages.push(AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::Value::Array(results_content),
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
) -> Result<(String, Usage), String> {
    let mut usage = Usage::default();
    let mut tool_budget = ToolBudget::default();
    let start = std::time::Instant::now();
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let mut tools_retry_used = false;

    for iteration in 0..MAX_TOOL_ITERATIONS {
        if crate::commands::is_cancelled() {
            return Err("Pipeline cancelled".into());
        }

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
        let resp = await_or_cancel(req.send(), pass_key.as_deref())
            .await?
            .map_err(|e| format_http_error(provider, &e))?;

        let status = resp.status();
        if !status.is_success() {
            let bytes =
                response_bytes_limited(resp, MAX_API_ERROR_BYTES, pass_key.as_deref()).await?;
            let body = String::from_utf8_lossy(&bytes);
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
            usage.add(inp, out, cached, 0);
            verbose_log(
                app,
                format!("[api] {label}: tokens in={inp} out={out} cached={cached}"),
            );
        }

        let choice = body
            .choices
            .first()
            .ok_or_else(|| format!("{provider} returned no choices"))?;

        if let Some(tool_calls) = &choice.message.tool_calls {
            if !tool_calls.is_empty() {
                // Append assistant message with tool calls
                request.messages.push(choice.message.clone());

                // Execute each tool and append results
                for tc in tool_calls {
                    let input: serde_json::Value =
                        serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                    let result = execute_tool(
                        app,
                        &tc.function.name,
                        &input,
                        label,
                        iteration,
                        &mut tool_budget,
                    )
                    .await;
                    match result {
                        ToolResult::ImageBase64 { data, media_type } => {
                            request.messages.push(OpenAIMessage {
                                role: "tool".to_string(),
                                content: Some(serde_json::Value::String(
                                    "The requested document image is attached in the next message."
                                        .to_string(),
                                )),
                                tool_calls: None,
                                tool_call_id: Some(tc.id.clone()),
                            });
                            request.messages.push(OpenAIMessage {
                                role: "user".to_string(),
                                content: Some(serde_json::json!([
                                    {
                                        "type": "text",
                                        "text": "Visual document asset requested by the preceding tool call."
                                    },
                                    {
                                        "type": "image_url",
                                        "image_url": {
                                            "url": format!("data:{media_type};base64,{data}")
                                        }
                                    }
                                ])),
                                tool_calls: None,
                                tool_call_id: None,
                            });
                        }
                        other => {
                            let content = match other {
                                ToolResult::Text(text) => text,
                                ToolResult::PdfBase64(_) => {
                                    "Cannot read PDF visually via this API. Use the extracted paper text or rendered page assets instead.".to_string()
                                }
                                ToolResult::Error(msg) => msg,
                                ToolResult::ImageBase64 { .. } => unreachable!(),
                            };
                            request.messages.push(OpenAIMessage {
                                role: "tool".to_string(),
                                content: Some(serde_json::Value::String(content)),
                                tool_calls: None,
                                tool_call_id: Some(tc.id.clone()),
                            });
                        }
                    }
                }
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

/// Run the tool-use loop for Google. Returns (text, usage).
pub async fn google_tool_loop(
    app: &crate::emit::EventBus,
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    mut request: GoogleRequest,
    timeout_secs: u64,
    label: &str,
) -> Result<(String, Usage), String> {
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

        let elapsed = start.elapsed().as_secs();
        if elapsed + MIN_REMAINING_SECS > timeout_secs {
            return Err(format!(
                "Step timeout ({timeout_secs}s) exceeded after {iteration} API requests"
            ));
        }
        let request_timeout = timeout_secs - elapsed;

        let pass_key = super::logging::current_pass();
        let request_future = client
            .post(&url)
            .header("x-goog-api-key", api_key)
            .timeout(std::time::Duration::from_secs(request_timeout))
            .json(&request)
            .send();
        let resp = await_or_cancel(request_future, pass_key.as_deref())
            .await?
            .map_err(|e| format_http_error("Google", &e))?;

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
            let inp = um
                .get("promptTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let out = um
                .get("candidatesTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cached = um
                .get("cachedContentTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            usage.add(inp, out, cached, 0);
            verbose_log(
                app,
                format!("[api] {label}: tokens in={inp} out={out} cached={cached}"),
            );
        } else {
            usage.requests += 1;
        }

        let candidate = body
            .candidates
            .as_ref()
            .and_then(|c| c.first())
            .ok_or("Google returned no candidates")?;

        // Check for function calls
        let function_calls: Vec<&GoogleFunctionCall> = candidate
            .content
            .parts
            .iter()
            .filter_map(|p| match p {
                GooglePart::FunctionCall { function_call } => Some(function_call),
                _ => None,
            })
            .collect();

        if !function_calls.is_empty() {
            // Append model response to contents
            request.contents.push(candidate.content.clone());

            // Build function response parts
            let mut response_parts: Vec<GooglePart> = Vec::with_capacity(function_calls.len());
            for fc in function_calls {
                let result =
                    execute_tool(app, &fc.name, &fc.args, label, iteration, &mut tool_budget).await;
                let (content, image) = match result {
                    ToolResult::Text(text) => (text, None),
                    ToolResult::PdfBase64(_) => (
                        "Cannot read PDF visually via this tool. Use the extracted paper text or rendered page assets instead.".to_string(),
                        None,
                    ),
                    ToolResult::ImageBase64 { data, media_type } => (
                        "The requested document image is included as inline visual data."
                            .to_string(),
                        Some(GoogleInlineData { data, mime_type: media_type }),
                    ),
                    ToolResult::Error(msg) => (msg, None),
                };
                response_parts.push(GooglePart::FunctionResponse {
                    function_response: GoogleFunctionResponse {
                        name: fc.name.clone(),
                        response: serde_json::json!({ "content": content }),
                    },
                });
                if let Some(inline_data) = image {
                    response_parts.push(GooglePart::InlineData { inline_data });
                }
            }

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
            .filter_map(|p| match p {
                GooglePart::Text { text } => Some(text.as_str()),
                _ => None,
            })
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
struct ToolBudget {
    read_calls: usize,
    read_bytes: usize,
}

const TOOL_IO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

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

/// Execute a tool call. Filesystem work runs on the blocking pool so a slow
/// or hostile filesystem entry cannot stall the async API loop.
async fn execute_tool(
    app: &crate::emit::EventBus,
    name: &str,
    input: &serde_json::Value,
    label: &str,
    iteration: usize,
    budget: &mut ToolBudget,
) -> ToolResult {
    match name {
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
            if budget.read_calls >= MAX_TOOL_READ_CALLS {
                return ToolResult::Error(format!(
                    "Read limit reached ({MAX_TOOL_READ_CALLS} calls per model invocation)"
                ));
            }
            let remaining = MAX_TOOL_READ_BYTES.saturating_sub(budget.read_bytes);
            if remaining == 0 {
                return ToolResult::Error(format!(
                    "Read byte budget reached ({} MB per model invocation)",
                    MAX_TOOL_READ_BYTES / 1024 / 1024
                ));
            }
            budget.read_calls += 1;
            if path.to_lowercase().ends_with(".pdf") {
                let owned_path = path.to_string();
                match run_blocking_tool(move || read_pdf_for_tool(&owned_path, remaining)).await {
                    Ok(data) => {
                        budget.read_bytes += data.len().saturating_mul(3) / 4;
                        ToolResult::PdfBase64(data)
                    }
                    Err(e) => ToolResult::Error(e),
                }
            } else {
                let owned_path = path.to_string();
                match run_blocking_tool(move || read_file_for_tool_limited(&owned_path, remaining))
                    .await
                {
                    Ok(content) => {
                        budget.read_bytes += content.len();
                        ToolResult::Text(content)
                    }
                    Err(e) => ToolResult::Error(e),
                }
            }
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
            if budget.read_calls >= MAX_TOOL_READ_CALLS {
                return ToolResult::Error(format!(
                    "Read limit reached ({MAX_TOOL_READ_CALLS} calls per model invocation)"
                ));
            }
            let remaining = MAX_TOOL_READ_BYTES.saturating_sub(budget.read_bytes);
            if remaining == 0 {
                return ToolResult::Error(format!(
                    "Read byte budget reached ({} MB per model invocation)",
                    MAX_TOOL_READ_BYTES / 1024 / 1024
                ));
            }
            budget.read_calls += 1;
            let owned_path = path.to_string();
            match run_blocking_tool(move || read_image_for_tool(&owned_path, remaining)).await {
                Ok((data, media_type)) => {
                    budget.read_bytes += data.len().saturating_mul(3) / 4;
                    ToolResult::ImageBase64 { data, media_type }
                }
                Err(error) => ToolResult::Error(error),
            }
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
            match run_blocking_tool(move || write_file_for_tool(&owned_path, &owned_content)).await
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
    }
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
        429 => format!("{provider}: Rate limited. Wait a moment and try again."),
        529 | 503 => format!("{provider}: Service overloaded. Try again in a few minutes."),
        _ => format!("{provider} API error (HTTP {status}): {detail}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static READ_DIR_TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn incomplete_provider_responses_are_identified() {
        assert!(anthropic_output_incomplete(Some("max_tokens")));
        assert!(!anthropic_output_incomplete(Some("end_turn")));
        assert!(openai_output_incomplete(Some("length")));
        assert!(openai_output_incomplete(Some("content_filter")));
        assert!(!openai_output_incomplete(Some("stop")));
        assert!(google_output_incomplete(Some("MAX_TOKENS")));
        assert!(!google_output_incomplete(Some("STOP")));
    }

    #[test]
    fn validate_tool_path_allows_explicit_private_temp_root() {
        let _guard = READ_DIR_TEST_MUTEX.lock().unwrap();
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(b"paper text").unwrap();
        tmp.flush().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        set_allowed_dirs(vec![tmp
            .path()
            .parent()
            .unwrap()
            .to_string_lossy()
            .to_string()]);
        validate_tool_path(&path, 1024).expect("temp file should be readable");
        set_allowed_dirs(vec![]);
    }

    #[test]
    fn validate_tool_path_rejects_outside_allowed_dirs() {
        let _guard = READ_DIR_TEST_MUTEX.lock().unwrap();
        // Cargo.toml in the crate root exists but is neither in the temp dir
        // nor in ALLOWED_DIRS, so it must be denied.
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        set_allowed_dirs(vec![]);
        let err = validate_tool_path(path, usize::MAX).unwrap_err();
        assert!(err.contains("Access denied"), "{err}");
    }

    #[test]
    fn document_asset_reader_returns_multimodal_image_data() {
        let _guard = READ_DIR_TEST_MUTEX.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("figure.png");
        std::fs::write(&image, b"\x89PNG\r\n\x1a\nvisual-bytes").unwrap();
        set_allowed_dirs(vec![dir.path().to_string_lossy().to_string()]);
        let (data, media_type) =
            read_image_for_tool(&image.to_string_lossy(), MAX_IMAGE_SIZE).unwrap();
        assert_eq!(media_type, "image/png");
        assert_eq!(
            STANDARD.decode(data).unwrap(),
            b"\x89PNG\r\n\x1a\nvisual-bytes"
        );
        set_allowed_dirs(Vec::new());
    }

    #[test]
    fn in_flight_future_is_interrupted_by_pass_cancellation() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let key = "api-common-cancellation-test";
                crate::commands::cancel_pass(key.to_string()).await.unwrap();
                let result = tokio::time::timeout(
                    std::time::Duration::from_millis(100),
                    await_or_cancel(std::future::pending::<()>(), Some(key)),
                )
                .await
                .expect("cancellation should resolve promptly");
                assert!(result.unwrap_err().contains("cancelled"));
            });
    }

    #[test]
    fn api_body_limit_rejects_a_chunk_before_growth() {
        let mut bytes = vec![0u8; 4];
        assert!(append_api_chunk(&mut bytes, &[1, 2], 6).is_ok());
        assert_eq!(bytes.len(), 6);
        assert!(append_api_chunk(&mut bytes, &[3], 6).is_err());
        assert_eq!(bytes.len(), 6);
    }

    #[test]
    fn disk_reader_enforces_limit_even_after_metadata_validation() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), b"123456789").unwrap();
        assert_eq!(read_bytes_limited(temp.path(), 9).unwrap(), b"123456789");
        assert!(read_bytes_limited(temp.path(), 8).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn disk_reader_rejects_fifo_without_blocking() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt as _;

        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("pipe");
        let path = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        let started = std::time::Instant::now();
        let error = read_bytes_limited(&fifo, 1024).unwrap_err();
        assert!(error.contains("not a regular file"), "{error}");
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    // Write-tool tests share the WRITE_DIR static, so they run under one
    // test to avoid interleaving set_write_dir calls across threads.
    #[test]
    fn write_tool_confinement() {
        let dir = tempfile::tempdir().unwrap();
        set_write_dir(Some(dir.path().to_path_buf()));

        // Relative path lands inside the dir, subdirs created.
        let msg = write_file_for_tool("steps/report.md", "# hi").unwrap();
        assert!(msg.contains("report.md"));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("steps/report.md")).unwrap(),
            "# hi"
        );

        // Absolute path inside the dir is accepted (raw, non-canonical form).
        let abs = dir.path().join("notes.md");
        write_file_for_tool(&abs.to_string_lossy(), "n").unwrap();
        assert!(abs.exists());

        // Traversal and outside-absolute paths are rejected.
        assert!(write_file_for_tool("../escape.md", "x")
            .unwrap_err()
            .contains("Access denied"));
        assert!(write_file_for_tool("a/../../escape.md", "x")
            .unwrap_err()
            .contains("Access denied"));
        let outside = std::env::temp_dir().join("pipeline_write_escape.md");
        assert!(write_file_for_tool(&outside.to_string_lossy(), "x").is_err());
        assert!(!outside.exists());

        // Oversized content is rejected.
        let big = "x".repeat(MAX_WRITE_SIZE + 1);
        assert!(write_file_for_tool("big.md", &big)
            .unwrap_err()
            .contains("too large"));

        // Symlinked destination is refused.
        #[cfg(unix)]
        {
            let target = dir.path().join("target.md");
            std::fs::write(&target, "t").unwrap();
            let link = dir.path().join("link.md");
            std::os::unix::fs::symlink(&target, &link).unwrap();
            assert!(write_file_for_tool("link.md", "x")
                .unwrap_err()
                .contains("symlink"));

            use std::ffi::CString;
            use std::os::unix::ffi::OsStrExt as _;
            let fifo = dir.path().join("fifo.md");
            let fifo_c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
            assert!(write_file_for_tool("fifo.md", "x")
                .unwrap_err()
                .contains("not a regular file"));
        }

        // Rejected writes do not consume either quota.
        assert_eq!(WRITE_COUNT.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(WRITE_BYTES.load(std::sync::atomic::Ordering::SeqCst), 5);

        // Disabled state rejects everything.
        set_write_dir(None);
        assert!(write_file_for_tool("steps/report.md", "x").is_err());
    }
}
