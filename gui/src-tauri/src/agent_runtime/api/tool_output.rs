//! Tool output records and provider media encoding. File access and permission
//! checks stay with the calling runtime; these functions only transform data.
use super::records::{GoogleFunctionCall, GoogleFunctionResponse, OpenAIMessage};
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

pub(crate) fn anthropic_tool_result_content(result: ToolResult) -> (serde_json::Value, bool) {
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

pub(crate) fn append_openai_tool_result(
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

pub(crate) fn append_openai_image_message(
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

pub(crate) fn google_tool_result_parts(
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
