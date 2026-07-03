//! Shared types and the tool-use loop for direct API calls.

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use tauri::{AppHandle, Emitter};

/// Maximum tool-call round-trips before giving up.
const MAX_TOOL_ITERATIONS: usize = 15;

/// Minimum remaining seconds before starting another API request.
/// Avoids wasting tokens on a request that will almost certainly time out.
const MIN_REMAINING_SECS: u64 = 10;

/// Shared HTTP client for all direct API calls.
/// `reqwest::Client` wraps an `Arc` internally, so cloning is cheap.
/// Reusing a single client enables TCP/TLS connection pooling across
/// pipeline steps that hit the same API host.
pub static HTTP_CLIENT: LazyLock<reqwest::Client> =
    LazyLock::new(reqwest::Client::new);

/// Maximum text file size for Read tool calls (5 MB).
const MAX_READ_SIZE: usize = 5 * 1024 * 1024;

/// Maximum PDF file size for Read tool calls (32 MB — matches Anthropic's document limit).
const MAX_PDF_SIZE: usize = 32 * 1024 * 1024;

// ── Logging ────────────────────────────────────────────────────────

pub fn log(app: &AppHandle, line: impl Into<String>) {
    app.emit("pipeline:log", serde_json::json!({ "line": line.into() })).ok();
}

pub fn verbose_log(app: &AppHandle, line: impl Into<String>) {
    if crate::settings::load().verbose_logging {
        log(app, line);
    }
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

/// Validate a tool-read path: resolve symlinks, check it's under an allowed
/// directory, and enforce a size limit.  Returns the canonical path on success.
fn validate_tool_path(path: &str, max_size: usize) -> Result<std::path::PathBuf, String> {
    let p = std::path::Path::new(path);

    // Resolve to canonical path to prevent traversal via symlinks or ..
    let canonical = p.canonicalize()
        .map_err(|_| format!("File not found: {path}"))?;

    // Validate the path is under an allowed directory.
    // env::temp_dir() must be canonicalized like the file path: on macOS
    // $TMPDIR lives under /var which is a symlink to /private/var, and on
    // Windows canonicalize() returns \\?\-prefixed paths — comparing a
    // canonical path against the raw temp dir never matches on either.
    let temp_dir = std::env::temp_dir()
        .canonicalize()
        .unwrap_or_else(|_| std::env::temp_dir());
    let allowed = ALLOWED_DIRS.lock().unwrap_or_else(|e| e.into_inner());
    let is_allowed = canonical.starts_with(&temp_dir)
        || allowed.iter().any(|dir| {
            std::path::Path::new(dir)
                .canonicalize()
                .map(|d| canonical.starts_with(&d))
                .unwrap_or(false)
        });
    drop(allowed);

    if !is_allowed {
        return Err(format!("Access denied: {path} is outside allowed directories"));
    }

    let metadata = std::fs::metadata(&canonical)
        .map_err(|e| format!("Cannot read file metadata: {e}"))?;
    if metadata.len() as usize > max_size {
        return Err(format!(
            "File too large ({} bytes, max {})",
            metadata.len(),
            max_size
        ));
    }

    Ok(canonical)
}

/// Read a text file from disk for a tool call.
pub fn read_file_for_tool(path: &str) -> Result<String, String> {
    let canonical = validate_tool_path(path, MAX_READ_SIZE)?;

    if path.to_lowercase().ends_with(".pdf") {
        return Err("Cannot read PDF as text. Use the extracted paper text instead.".into());
    }

    std::fs::read_to_string(&canonical)
        .map_err(|e| format!("Failed to read {path}: {e}"))
}

/// Read a PDF file and return its contents as base64-encoded bytes.
fn read_pdf_for_tool(path: &str) -> Result<String, String> {
    let canonical = validate_tool_path(path, MAX_PDF_SIZE)?;
    let bytes = std::fs::read(&canonical)
        .map_err(|e| format!("Failed to read {path}: {e}"))?;
    Ok(STANDARD.encode(&bytes))
}

/// Result of executing a tool call.
pub enum ToolResult {
    /// Plain text content.
    Text(String),
    /// PDF content as base64-encoded bytes.
    PdfBase64(String),
    /// Error message.
    Error(String),
}

// ── Anthropic-specific types ───────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AnthropicRequest {
    pub model: String,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    pub messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<serde_json::Value>,
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

#[derive(Debug, Deserialize)]
pub struct AnthropicError {
    pub error: Option<AnthropicErrorDetail>,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AnthropicErrorDetail {
    #[serde(rename = "type")]
    pub error_type: Option<String>,
    pub message: String,
}

// ── OpenAI-specific types ──────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct OpenAIRequest {
    pub model: String,
    pub messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
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
}

// ── Google-specific types ──────────────────────────────────────────

#[derive(Debug, Serialize)]
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
    pub requests: u32,
}

impl Usage {
    pub fn add(&mut self, input: u64, output: u64) {
        self.input_tokens += input;
        self.output_tokens += output;
        self.requests += 1;
    }

    /// Format as a compact string for log lines.
    pub fn summary(&self) -> String {
        let total = self.input_tokens + self.output_tokens;
        if total == 0 {
            return String::new();
        }
        format!(
            ", {}+{} tokens ({} req)",
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
    blocks.iter().any(|b| matches!(b, AnthropicContentBlock::ToolUse { .. }))
}

/// Run the tool-use loop for Anthropic. Returns (text, usage).
pub async fn anthropic_tool_loop(
    app: &AppHandle,
    client: &reqwest::Client,
    api_key: &str,
    mut request: AnthropicRequest,
    timeout_secs: u64,
    label: &str,
) -> Result<(String, Usage), String> {
    let mut usage = Usage::default();
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

        let resp = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .timeout(std::time::Duration::from_secs(request_timeout))
            .json(&request)
            .send()
            .await
            .map_err(|e| format_http_error("Anthropic", &e))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format_api_error("Anthropic", status.as_u16(), &body));
        }

        let body: AnthropicResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse Anthropic response: {e}"))?;

        if let Some(u) = &body.usage {
            let inp = u.input_tokens.unwrap_or(0);
            let out = u.output_tokens.unwrap_or(0);
            usage.add(inp, out);
            verbose_log(app, format!("[api] {label}: tokens in={inp} out={out}"));
        }

        if !anthropic_has_tool_use(&body.content) || body.stop_reason.as_deref() != Some("tool_use") {
            return Ok((anthropic_extract_text(&body.content), usage));
        }

        // Build tool results
        let mut tool_results: Vec<AnthropicContentBlock> = Vec::new();
        for block in &body.content {
            if let AnthropicContentBlock::ToolUse { id, name, input } = block {
                let result = execute_tool(app, name, input, label, iteration);
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
            .map(|b| serde_json::to_value(b))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to serialize content block: {e}"))?;
        request.messages.push(AnthropicMessage {
            role: "assistant".to_string(),
            content: serde_json::Value::Array(assistant_content),
        });
        let results_content: Vec<serde_json::Value> = tool_results
            .iter()
            .map(|b| serde_json::to_value(b))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to serialize tool result: {e}"))?;
        request.messages.push(AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::Value::Array(results_content),
        });
    }

    Err(format!("Tool loop exceeded {MAX_TOOL_ITERATIONS} iterations"))
}

/// Run the tool-use loop for OpenAI. Returns (text, usage).
pub async fn openai_tool_loop(
    app: &AppHandle,
    client: &reqwest::Client,
    api_key: &str,
    mut request: OpenAIRequest,
    timeout_secs: u64,
    label: &str,
) -> Result<(String, Usage), String> {
    let mut usage = Usage::default();
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

        let resp = client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {api_key}"))
            .timeout(std::time::Duration::from_secs(request_timeout))
            .json(&request)
            .send()
            .await
            .map_err(|e| format_http_error("OpenAI", &e))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format_api_error("OpenAI", status.as_u16(), &body));
        }

        let body: OpenAIResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse OpenAI response: {e}"))?;

        if let Some(u) = &body.usage {
            let inp = u.prompt_tokens.unwrap_or(0);
            let out = u.completion_tokens.unwrap_or(0);
            usage.add(inp, out);
            verbose_log(app, format!("[api] {label}: tokens in={inp} out={out}"));
        }

        let choice = body.choices.first().ok_or("OpenAI returned no choices")?;

        if let Some(tool_calls) = &choice.message.tool_calls {
            if !tool_calls.is_empty() {
                // Append assistant message with tool calls
                request.messages.push(choice.message.clone());

                // Execute each tool and append results
                for tc in tool_calls {
                    let input: serde_json::Value =
                        serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                    let result = execute_tool(app, &tc.function.name, &input, label, iteration);
                    let content = match result {
                        ToolResult::Text(text) => text,
                        ToolResult::PdfBase64(_) => {
                            "Cannot read PDF visually via this API. Use the extracted paper text file instead.".to_string()
                        }
                        ToolResult::Error(msg) => msg,
                    };
                    request.messages.push(OpenAIMessage {
                        role: "tool".to_string(),
                        content: Some(content),
                        tool_calls: None,
                        tool_call_id: Some(tc.id.clone()),
                    });
                }
                continue;
            }
        }

        return Ok((choice.message.content.clone().unwrap_or_default(), usage));
    }

    Err(format!("Tool loop exceeded {MAX_TOOL_ITERATIONS} iterations"))
}

/// Run the tool-use loop for Google. Returns (text, usage).
pub async fn google_tool_loop(
    app: &AppHandle,
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

        let resp = client
            .post(&url)
            .header("x-goog-api-key", api_key)
            .timeout(std::time::Duration::from_secs(request_timeout))
            .json(&request)
            .send()
            .await
            .map_err(|e| format_http_error("Google", &e))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format_api_error("Google", status.as_u16(), &body));
        }

        let body: GoogleResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse Google response: {e}"))?;

        if let Some(error) = &body.error {
            return Err(format!("Google API error: {}", error.message));
        }

        // Google returns usageMetadata at the top level
        if let Some(um) = body.usage_metadata.as_ref() {
            let inp = um.get("promptTokenCount").and_then(|v| v.as_u64()).unwrap_or(0);
            let out = um.get("candidatesTokenCount").and_then(|v| v.as_u64()).unwrap_or(0);
            usage.add(inp, out);
            verbose_log(app, format!("[api] {label}: tokens in={inp} out={out}"));
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
            let response_parts: Vec<GooglePart> = function_calls
                .iter()
                .map(|fc| {
                    let result = execute_tool(app, &fc.name, &fc.args, label, iteration);
                    let content = match result {
                        ToolResult::Text(text) => text,
                        ToolResult::PdfBase64(_) => {
                            "Cannot read PDF visually via this API. Use the extracted paper text file instead.".to_string()
                        }
                        ToolResult::Error(msg) => msg,
                    };
                    GooglePart::FunctionResponse {
                        function_response: GoogleFunctionResponse {
                            name: fc.name.clone(),
                            response: serde_json::json!({ "content": content }),
                        },
                    }
                })
                .collect();

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

        return Ok((text, usage));
    }

    Err(format!("Tool loop exceeded {MAX_TOOL_ITERATIONS} iterations"))
}

// ── Shared helpers ─────────────────────────────────────────────────

/// Execute a tool call.
fn execute_tool(
    app: &AppHandle,
    name: &str,
    input: &serde_json::Value,
    label: &str,
    iteration: usize,
) -> ToolResult {
    match name {
        "Read" => {
            let path = input
                .get("file_path")
                .or_else(|| input.get("path"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            verbose_log(app, format!("[api] {label}: Read tool call #{} -> {path}", iteration + 1));
            if path.to_lowercase().ends_with(".pdf") {
                match read_pdf_for_tool(path) {
                    Ok(data) => ToolResult::PdfBase64(data),
                    Err(e) => ToolResult::Error(e),
                }
            } else {
                match read_file_for_tool(path) {
                    Ok(content) => ToolResult::Text(content),
                    Err(e) => ToolResult::Error(e),
                }
            }
        }
        _ => {
            verbose_log(app, format!("[api] {label}: unknown tool '{name}', skipping"));
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
                .and_then(|e| e.get("message").or_else(|| Some(e)))
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
    use std::io::Write as _;

    #[test]
    fn validate_tool_path_allows_temp_files() {
        // Regression: env::temp_dir() must be canonicalized before the prefix
        // check — on macOS $TMPDIR is under /var (a symlink to /private/var),
        // so the raw comparison rejected every temp file in direct-API mode.
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(b"paper text").unwrap();
        tmp.flush().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        validate_tool_path(&path, 1024).expect("temp file should be readable");
    }

    #[test]
    fn validate_tool_path_rejects_outside_allowed_dirs() {
        // Cargo.toml in the crate root exists but is neither in the temp dir
        // nor in ALLOWED_DIRS, so it must be denied.
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        let err = validate_tool_path(path, usize::MAX).unwrap_err();
        assert!(err.contains("Access denied"), "{err}");
    }
}
