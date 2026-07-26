//! Direct OpenAI Chat Completions API client.

use super::api_common::*;
use super::claude::LlmOverrides;
use crate::settings::Settings;
use std::time::Instant;

/// Map settings model shorthand to OpenAI model ID.
/// `override_model` (when non-empty) takes precedence over the global setting.
fn resolve_model(settings: &Settings, override_model: Option<&str>) -> String {
    let raw = override_model
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(settings.codex_model.as_str());
    match raw {
        "" => "gpt-5.6-terra".to_string(),
        other => other.to_string(),
    }
}

/// Build OpenAI tools array from allowed tool names.
fn build_tools(allowed_tools: &[&str]) -> Vec<serde_json::Value> {
    let mut tools = Vec::new();
    if allowed_tools.contains(&"Read") {
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
    if allowed_tools.contains(&"ReadDocumentAsset") {
        let def = DocumentAssetToolDef::default();
        tools.push(serde_json::json!({
            "type": "function",
            "function": {
                "name": def.name,
                "description": def.description,
                "parameters": def.input_schema,
            }
        }));
    }
    if allowed_tools.contains(&"Write") {
        let def = WriteToolDef::default();
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

/// Build the system + user messages shared by the OpenAI and local providers.
/// With a PDF attachment, the user message is [file, text] content parts;
/// otherwise a plain string.
fn build_messages(
    prompt: &str,
    system_prompt: Option<&str>,
    pdf_attachment: Option<&std::path::Path>,
    shared_context: Option<&super::context_cache::PreparedContext>,
) -> Result<Vec<OpenAIMessage>, String> {
    let mut messages = Vec::new();
    if let Some(sys) = system_prompt {
        messages.push(OpenAIMessage {
            role: "system".to_string(),
            content: Some(serde_json::Value::String(sys.to_string())),
            tool_calls: None,
            tool_call_id: None,
        });
    }
    let user_content = if pdf_attachment.is_some() || shared_context.is_some() {
        let mut parts = Vec::new();
        if let Some(pdf) = pdf_attachment {
            let data = pdf_attachment_base64(pdf, MAX_ATTACH_PDF)?;
            let filename = pdf
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "document.pdf".to_string());
            parts.push(serde_json::json!({
                "type": "file",
                "file": {
                    "filename": filename,
                    "file_data": format!("data:application/pdf;base64,{data}")
                }
            }));
        }
        if let Some(context) = shared_context {
            parts.push(serde_json::json!({
                "type": "text",
                "text": context.content()
            }));
        }
        parts.push(serde_json::json!({ "type": "text", "text": prompt }));
        serde_json::Value::Array(parts)
    } else {
        serde_json::Value::String(prompt.to_string())
    };
    messages.push(OpenAIMessage {
        role: "user".to_string(),
        content: Some(user_content),
        tool_calls: None,
        tool_call_id: None,
    });
    Ok(messages)
}

/// Call the OpenAI API directly, with tool-use loop for Read.
#[allow(clippy::too_many_arguments)]
pub async fn call_openai_api(
    app: &crate::emit::EventBus,
    prompt: &str,
    allowed_tools: &[&str],
    system_prompt: Option<&str>,
    timeout_secs: u64,
    label: &str,
    settings: &Settings,
    read_dirs: &[&str],
    overrides: &LlmOverrides<'_>,
) -> Result<String, String> {
    let start = Instant::now();
    let model = resolve_model(settings, overrides.model);
    log(
        app,
        format!("{label} started (API: OpenAI, model: {model})"),
    );

    let client = &*super::api_common::HTTP_CLIENT;
    let access = ToolAccess::new(read_dirs, overrides.write_dir);
    let tools = build_tools(allowed_tools);
    let shared_context = overrides.shared_context.as_deref();
    let messages = build_messages(
        prompt,
        system_prompt,
        overrides.pdf_attachment,
        shared_context,
    )?;

    let effort_src = overrides
        .effort
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(settings.codex_effort.as_str());
    let reasoning_effort = if !effort_src.is_empty() {
        Some(effort_src.to_string())
    } else {
        None
    };
    let tools_key = serde_json::to_string(&tools).unwrap_or_default();
    let attachment_key = overrides
        .pdf_attachment
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let prompt_cache_key = shared_context.map(|context| {
        context.compatibility_key(
            "openai-api",
            [
                model.as_str(),
                effort_src,
                system_prompt.unwrap_or(""),
                tools_key.as_str(),
                attachment_key.as_str(),
            ],
        )
    });

    let mut request = OpenAIRequest {
        model,
        messages,
        tools,
        reasoning_effort,
        max_completion_tokens: overrides.max_output_tokens,
        prompt_cache_key,
    };

    let mut warm_usage = Usage::default();
    if let Some(context) = shared_context {
        let cache_key = request
            .prompt_cache_key
            .clone()
            .unwrap_or_else(|| context.key().to_string());
        let slot = context.slot(cache_key).await;
        let mut state = slot.lock().await;
        if state.is_none() {
            let mut warm_request = request.clone();
            warm_request.messages = build_messages(
                "Acknowledge this shared context by replying only: Context prepared. Do not call tools.",
                system_prompt,
                overrides.pdf_attachment,
                Some(context),
            )?;
            warm_request.max_completion_tokens = Some(32);
            let warm_label = format!("{label} · cache warm-up");
            super::logging::record_provider_attempt();
            match openai_tool_loop(
                app,
                client,
                "https://api.openai.com/v1",
                &settings.openai_api_key,
                "OpenAI",
                warm_request,
                timeout_secs,
                &warm_label,
                false,
                &access,
            )
            .await
            {
                Ok((_text, usage)) => {
                    warm_usage = usage;
                    *state = Some("ready".to_string());
                    log(app, format!("{label}: OpenAI shared context warmed"));
                }
                Err(error) => {
                    *state = Some("implicit".to_string());
                    // Older compatible endpoints may reject the routing hint
                    // even though ordinary prefix caching still works.
                    request.prompt_cache_key = None;
                    log(
                        app,
                        format!(
                            "WARNING: {label}: OpenAI cache warm-up was unavailable ({error}); continuing with automatic prefix caching"
                        ),
                    );
                }
            }
        }
        if state.as_deref() == Some("implicit") {
            request.prompt_cache_key = None;
        }
    }

    super::logging::record_provider_attempt();
    let (text, mut usage) = openai_tool_loop(
        app,
        client,
        "https://api.openai.com/v1",
        &settings.openai_api_key,
        "OpenAI",
        request,
        timeout_secs,
        label,
        false,
        &access,
    )
    .await?;
    usage.merge(warm_usage);

    let elapsed = start.elapsed().as_secs();
    log(
        app,
        format!(
            "{label} finished ({elapsed}s, {} chars output{})",
            text.len(),
            usage.summary()
        ),
    );
    super::logging::emit_usage(app, usage.call_usage());

    if text.trim().is_empty() {
        return Err(format!("{label}: OpenAI API returned empty output"));
    }

    Ok(text.trim().to_string())
}

/// Call a local OpenAI-compatible server (Ollama, LM Studio, llama.cpp,
/// vLLM) configured via `settings.local_base_url` / `local_model`.
///
/// Differences from the OpenAI path: no reasoning_effort (most local servers
/// reject or ignore it), no PDF attachments (no local server supports the
/// file content part — extraction should use pdftotext/marker instead), and
/// a retry-without-tools fallback for models without tool-calling support.
#[allow(clippy::too_many_arguments)]
pub async fn call_local_api(
    app: &crate::emit::EventBus,
    prompt: &str,
    allowed_tools: &[&str],
    system_prompt: Option<&str>,
    timeout_secs: u64,
    label: &str,
    settings: &Settings,
    read_dirs: &[&str],
    overrides: &LlmOverrides<'_>,
) -> Result<String, String> {
    let start = Instant::now();

    let base_url = settings.local_base_url.trim();
    if base_url.is_empty() {
        return Err(
            "Local provider: no server URL configured. Set it in Settings → Models.".into(),
        );
    }
    let model = overrides
        .model
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(settings.local_model.as_str())
        .trim()
        .to_string();
    if model.is_empty() {
        return Err(
            "Local provider: no model configured. Set one in Settings → Models \
             (e.g. `ollama pull llama3.3`, then enter \"llama3.3\")."
                .into(),
        );
    }
    if overrides.pdf_attachment.is_some() {
        return Err(
            "Local provider: PDF attachments are not supported by local servers. \
             Use marker or pdftotext extraction with the local provider."
                .into(),
        );
    }

    log(
        app,
        format!("{label} started (API: local, {base_url}, model: {model})"),
    );

    let client = &*super::api_common::HTTP_CLIENT;
    let access = ToolAccess::new(read_dirs, overrides.write_dir);
    let request = OpenAIRequest {
        model,
        messages: build_messages(
            prompt,
            system_prompt,
            None,
            overrides.shared_context.as_deref(),
        )?,
        tools: build_tools(allowed_tools),
        reasoning_effort: None,
        max_completion_tokens: overrides.max_output_tokens,
        prompt_cache_key: None,
    };

    super::logging::record_provider_attempt();
    let (text, usage) = openai_tool_loop(
        app,
        client,
        base_url,
        &settings.local_api_key,
        "Local server",
        request,
        timeout_secs,
        label,
        true,
        &access,
    )
    .await?;

    let elapsed = start.elapsed().as_secs();
    log(
        app,
        format!(
            "{label} finished ({elapsed}s, {} chars output{})",
            text.len(),
            usage.summary()
        ),
    );
    super::logging::emit_usage(app, usage.call_usage());

    if text.trim().is_empty() {
        return Err(format!("{label}: local server returned empty output"));
    }

    Ok(text.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_context_is_a_stable_prefix_before_the_task() {
        let context = super::super::context_cache::PreparedContext::new(
            "paper body",
            &serde_json::json!({"map": "orientation"}),
        )
        .unwrap();
        let messages = build_messages(
            "task-specific request",
            Some("system"),
            None,
            Some(&context),
        )
        .unwrap();
        assert_eq!(messages[0].role, "system");
        let parts = messages[1].content.as_ref().unwrap().as_array().unwrap();
        assert!(parts[0]["text"].as_str().unwrap().contains("paper body"));
        assert_eq!(parts[1]["text"], "task-specific request");
    }
}
