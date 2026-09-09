//! Schemas, access policy, and wire models for direct API providers.

use super::*;

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
pub(super) static WRITE_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
pub(super) static WRITE_BYTES: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

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
pub(super) fn validate_tool_path(
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

pub(super) fn read_bytes_limited(path: &std::path::Path, limit: usize) -> Result<Vec<u8>, String> {
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

pub(super) fn read_file_for_tool_limited(
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
pub(super) struct TextRange {
    pub(super) content: String,
    pub(super) start_offset: usize,
    pub(super) end_offset: usize,
    pub(super) next_offset: Option<usize>,
}

pub(super) fn slice_text_range(
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

pub(super) fn text_line_bounds(
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
pub(super) fn read_pdf_for_tool(
    access: &ToolAccess,
    path: &str,
    limit: usize,
) -> Result<String, String> {
    let limit = MAX_TOOL_PDF_SIZE.min(limit);
    let canonical = validate_tool_path(access, path, limit)?;
    let bytes = read_bytes_limited(&canonical, limit)?;
    Ok(STANDARD.encode(&bytes))
}

pub(super) fn read_image_for_tool(
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

    pub fn call_usage(&self) -> crate::pipeline::logging::CallUsage {
        crate::pipeline::logging::CallUsage {
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

pub(super) fn anthropic_output_incomplete(reason: Option<&str>) -> bool {
    reason == Some("max_tokens")
}

pub(super) fn openai_output_incomplete(reason: Option<&str>) -> bool {
    matches!(reason, Some("length" | "content_filter"))
}

pub(super) fn google_output_incomplete(reason: Option<&str>) -> bool {
    reason.is_some_and(|reason| reason != "STOP")
}

pub(super) fn direct_tool_kind(name: &str) -> crate::models::ToolCallKind {
    match name {
        "Read" | "ReadTextBatch" | "Write" => crate::models::ToolCallKind::TextFile,
        "ReadDocumentAsset" | "ReadDocumentAssetsBatch" => crate::models::ToolCallKind::Image,
        other => crate::pipeline::logging::classify_tool_name(other),
    }
}

pub(super) fn anthropic_hosted_search_count(usage: &AnthropicUsage) -> u64 {
    usage
        .server_tool_use
        .as_ref()
        .and_then(|server| server.web_search_requests)
        .unwrap_or(0)
}

pub(super) fn google_hosted_search_count(candidate: &GoogleCandidate) -> u64 {
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
