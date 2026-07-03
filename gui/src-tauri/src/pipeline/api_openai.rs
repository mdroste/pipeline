//! Direct OpenAI Chat Completions API client.

use super::api_common::*;
use super::claude::LlmOverrides;
use crate::settings::Settings;
use std::time::Instant;
use tauri::AppHandle;

/// Map settings model shorthand to OpenAI model ID.
/// `override_model` (when non-empty) takes precedence over the global setting.
fn resolve_model(settings: &Settings, override_model: Option<&str>) -> String {
    let raw = override_model
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(settings.codex_model.as_str());
    match raw {
        "" => "gpt-4.1".to_string(),
        other => other.to_string(),
    }
}

/// Build OpenAI tools array from allowed tool names.
fn build_tools(allowed_tools: &[&str]) -> Vec<serde_json::Value> {
    let mut tools = Vec::new();
    if allowed_tools.iter().any(|t| *t == "Read") {
        let def = ReadToolDef::default();
        tools.push(serde_json::json!({
            "type": "function",
            "function": {
                "name": def.name,
                "description": def.description,
                "parameters": def.input_schema,
            }
        }));
    }
    tools
}

/// Call the OpenAI API directly, with tool-use loop for Read.
pub async fn call_openai_api(
    app: &AppHandle,
    prompt: &str,
    allowed_tools: &[&str],
    system_prompt: Option<&str>,
    timeout_secs: u64,
    label: &str,
    settings: &Settings,
    overrides: &LlmOverrides<'_>,
) -> Result<String, String> {
    let start = Instant::now();
    let model = resolve_model(settings, overrides.model);
    log(app, format!("{label} started (API: OpenAI, model: {model})"));

    let client = &*super::api_common::HTTP_CLIENT;
    let tools = build_tools(allowed_tools);

    let mut messages = Vec::new();
    if let Some(sys) = system_prompt {
        messages.push(OpenAIMessage {
            role: "system".to_string(),
            content: Some(serde_json::Value::String(sys.to_string())),
            tool_calls: None,
            tool_call_id: None,
        });
    }
    // With a PDF attachment, the user message is [file, text] content parts;
    // otherwise a plain string.
    let user_content = match overrides.pdf_attachment {
        Some(pdf) => {
            let data = pdf_attachment_base64(pdf, MAX_ATTACH_PDF)?;
            let filename = pdf
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "document.pdf".to_string());
            serde_json::json!([
                {
                    "type": "file",
                    "file": {
                        "filename": filename,
                        "file_data": format!("data:application/pdf;base64,{data}")
                    }
                },
                { "type": "text", "text": prompt }
            ])
        }
        None => serde_json::Value::String(prompt.to_string()),
    };
    messages.push(OpenAIMessage {
        role: "user".to_string(),
        content: Some(user_content),
        tool_calls: None,
        tool_call_id: None,
    });

    let effort_src = overrides
        .effort
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(settings.codex_effort.as_str());
    let reasoning_effort = if !effort_src.is_empty() {
        Some(effort_src.to_string())
    } else {
        None
    };

    let request = OpenAIRequest {
        model,
        messages,
        tools,
        reasoning_effort,
        max_completion_tokens: overrides.max_output_tokens,
    };

    let (text, usage) = openai_tool_loop(app, &client, &settings.openai_api_key, request, timeout_secs, label)
        .await?;

    let elapsed = start.elapsed().as_secs();
    log(app, format!("{label} finished ({elapsed}s, {} chars output{})", text.len(), usage.summary()));

    if text.trim().is_empty() {
        return Err(format!("{label}: OpenAI API returned empty output"));
    }

    Ok(text.trim().to_string())
}
