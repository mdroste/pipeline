//! Bounded host-tool execution for direct API providers.

use super::*;

#[derive(Default)]
pub(crate) struct ToolBudget {
    pub(super) read_calls: usize,
    pub(super) read_bytes: usize,
    tool_calls: usize,
    argument_bytes: usize,
    pub(super) encoded_media_bytes: usize,
}

impl ToolBudget {
    pub(super) fn reserve_tool_call(&mut self, argument_bytes: usize) -> Result<(), String> {
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
pub(super) fn enforce_media_budget(
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
    let pass_key = crate::pipeline::logging::current_pass();
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

pub(super) async fn execute_read_text_batch(
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

pub(super) async fn execute_read_assets_batch(
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

pub(super) fn anthropic_tool_result_content(result: ToolResult) -> (serde_json::Value, bool) {
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

pub(super) fn append_openai_tool_result(
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

pub(super) fn append_openai_image_message(
    messages: &mut Vec<OpenAIMessage>,
    images: Vec<ToolImage>,
) {
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

pub(super) fn google_tool_result_parts(
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
pub(super) async fn execute_tool(
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
            let Some(content) = input.get("content").and_then(|v| v.as_str()) else {
                return ToolResult::Error("Write requires string content".into());
            };
            verbose_log(
                app,
                format!(
                    "[api] {label}: Write tool call #{} -> {path} ({} bytes)",
                    iteration + 1,
                    content.len()
                ),
            );
            // The bounded atomic write must settle before this future can be dropped.
            // A detached blocking task could otherwise commit after run finalization.
            match write_file_for_tool(access, path, content) {
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
mod write_ownership_tests;
