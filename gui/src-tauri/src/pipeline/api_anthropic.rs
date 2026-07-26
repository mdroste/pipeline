//! Direct Anthropic Messages API client.

use super::api_common::*;
use super::claude::LlmOverrides;
use crate::settings::Settings;
use std::time::Instant;

/// Map settings model shorthand to full Anthropic model ID.
/// `override_model` (when non-empty) takes precedence over the global setting.
fn resolve_model(settings: &Settings, override_model: Option<&str>) -> String {
    let raw = override_model
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(settings.claude_model.as_str());
    // Dateless aliases track the current model in each tier; dated snapshots
    // get retired (the 20250514 snapshots died 2026-06-15 and broke this path).
    // These tiers don't emit thinking blocks when `thinking` is omitted, which
    // the tool loop in api_common.rs relies on when echoing assistant content.
    match raw {
        "" | "sonnet" => "claude-sonnet-4-6".to_string(),
        "opus" => "claude-opus-4-6".to_string(),
        "haiku" => "claude-haiku-4-5".to_string(),
        other => other.to_string(),
    }
}

/// Map the effort setting to the Messages API's `output_config`.
/// Effort is GA on Sonnet 4.6 / Opus 4.6+ but rejected by Haiku models,
/// so it is silently skipped there. Unknown values are skipped rather than
/// sent, so a value tuned for the CLI path can't 400 the whole step.
fn effort_config(model: &str, effort: &str) -> Option<serde_json::Value> {
    if model.starts_with("claude-haiku") {
        return None;
    }
    match effort {
        "low" | "medium" | "high" | "max" => Some(serde_json::json!({ "effort": effort })),
        _ => None,
    }
}

/// Build Anthropic tools array from allowed tool names.
fn build_tools(allowed_tools: &[&str]) -> Vec<serde_json::Value> {
    let mut tools = Vec::new();
    if allowed_tools.contains(&"Read") {
        let def = ReadToolDef::default();
        tools.push(serde_json::json!({
            "name": def.name,
            "description": def.description,
            "input_schema": def.input_schema,
        }));
    }
    if allowed_tools.contains(&"ReadDocumentAsset") {
        let def = DocumentAssetToolDef::default();
        tools.push(serde_json::json!({
            "name": def.name,
            "description": def.description,
            "input_schema": def.input_schema,
        }));
    }
    if allowed_tools.contains(&"Write") {
        let def = WriteToolDef::default();
        tools.push(serde_json::json!({
            "name": def.name,
            "description": def.description,
            "input_schema": def.input_schema,
        }));
    }
    tools
}

fn build_content(
    prompt: &str,
    pdf_attachment: Option<&std::path::Path>,
    shared_context: Option<&super::context_cache::PreparedContext>,
) -> Result<serde_json::Value, String> {
    if pdf_attachment.is_none() && shared_context.is_none() {
        return Ok(serde_json::Value::String(prompt.to_string()));
    }
    let mut blocks = Vec::new();
    if let Some(pdf) = pdf_attachment {
        let data = pdf_attachment_base64(pdf, MAX_ATTACH_PDF)?;
        blocks.push(serde_json::json!({
            "type": "document",
            "source": {
                "type": "base64",
                "media_type": "application/pdf",
                "data": data
            }
        }));
    }
    if let Some(context) = shared_context {
        blocks.push(serde_json::json!({
            "type": "text",
            "text": context.content(),
            "cache_control": { "type": "ephemeral" }
        }));
    }
    blocks.push(serde_json::json!({ "type": "text", "text": prompt }));
    Ok(serde_json::Value::Array(blocks))
}

/// Call the Anthropic API directly, with tool-use loop for Read.
#[allow(clippy::too_many_arguments)]
pub async fn call_anthropic_api(
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
    let effort = overrides
        .effort
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(settings.claude_effort.as_str());
    let output_config = effort_config(&model, effort.trim());
    log(
        app,
        format!("{label} started (API: Anthropic, model: {model})"),
    );

    let client = &*super::api_common::HTTP_CLIENT;
    let access = ToolAccess::new(read_dirs, overrides.write_dir);
    let tools = build_tools(allowed_tools);

    // With a PDF attachment, the user message is [document, text] content
    // blocks; otherwise a plain string.
    let shared_context = overrides.shared_context.as_deref();
    let content = build_content(prompt, overrides.pdf_attachment, shared_context)?;
    let messages = vec![AnthropicMessage {
        role: "user".to_string(),
        content,
    }];

    let request = AnthropicRequest {
        model,
        max_tokens: overrides.max_output_tokens.unwrap_or(16384),
        system: system_prompt.map(|s| s.to_string()),
        messages,
        tools,
        output_config,
    };

    let mut warm_usage = Usage::default();
    if let Some(context) = shared_context {
        let tools_key = serde_json::to_string(&request.tools).unwrap_or_default();
        let attachment_key = overrides
            .pdf_attachment
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        let cache_key = context.compatibility_key(
            "anthropic-api",
            [
                request.model.as_str(),
                effort,
                system_prompt.unwrap_or(""),
                tools_key.as_str(),
                attachment_key.as_str(),
            ],
        );
        let slot = context.slot(cache_key).await;
        let mut state = slot.lock().await;
        if state.is_none() {
            let mut warm_request = request.clone();
            warm_request.max_tokens = 32;
            warm_request.messages = vec![AnthropicMessage {
                role: "user".to_string(),
                content: build_content(
                    "Acknowledge this shared context by replying only: Context prepared. Do not call tools.",
                    overrides.pdf_attachment,
                    Some(context),
                )?,
            }];
            let warm_label = format!("{label} · cache warm-up");
            super::logging::record_provider_attempt();
            match anthropic_tool_loop(
                app,
                client,
                &settings.anthropic_api_key,
                warm_request,
                timeout_secs,
                &warm_label,
                &access,
            )
            .await
            {
                Ok((_text, usage)) => {
                    warm_usage = usage;
                    *state = Some("ready".to_string());
                    log(app, format!("{label}: Anthropic shared context warmed"));
                }
                Err(error) => {
                    *state = Some("implicit".to_string());
                    log(
                        app,
                        format!(
                            "WARNING: {label}: Anthropic cache warm-up was unavailable ({error}); continuing with the cached prefix"
                        ),
                    );
                }
            }
        }
    }

    super::logging::record_provider_attempt();
    let (text, mut usage) = anthropic_tool_loop(
        app,
        client,
        &settings.anthropic_api_key,
        request,
        timeout_secs,
        label,
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
        return Err(format!("{label}: Anthropic API returned empty output"));
    }

    Ok(text.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_model_uses_dateless_aliases() {
        // Regression: the dated 20250514 snapshots were retired 2026-06-15 and
        // started returning 404. Aliases must not carry date suffixes.
        let settings = Settings::default();
        for (shorthand, expected) in [
            ("", "claude-sonnet-4-6"),
            ("sonnet", "claude-sonnet-4-6"),
            ("opus", "claude-opus-4-6"),
            ("haiku", "claude-haiku-4-5"),
        ] {
            assert_eq!(resolve_model(&settings, Some(shorthand)), expected);
        }
        // Explicit full model IDs pass through untouched.
        assert_eq!(
            resolve_model(&settings, Some("claude-opus-4-8")),
            "claude-opus-4-8"
        );
    }

    #[test]
    fn effort_config_maps_known_values_and_skips_haiku() {
        assert_eq!(
            effort_config("claude-sonnet-4-6", "high"),
            Some(serde_json::json!({ "effort": "high" }))
        );
        assert_eq!(
            effort_config("claude-opus-4-6", "max"),
            Some(serde_json::json!({ "effort": "max" }))
        );
        // Haiku models reject the effort parameter — never send it.
        assert_eq!(effort_config("claude-haiku-4-5", "high"), None);
        // Empty or unrecognized values are dropped rather than sent.
        assert_eq!(effort_config("claude-sonnet-4-6", ""), None);
        assert_eq!(effort_config("claude-sonnet-4-6", "xhigh"), None);
    }

    #[test]
    fn shared_context_block_has_anthropic_cache_breakpoint() {
        let context = super::super::context_cache::PreparedContext::new(
            "paper body",
            &serde_json::json!({"map": "orientation"}),
        )
        .unwrap();
        let content = build_content("task-specific request", None, Some(&context)).unwrap();
        let blocks = content.as_array().unwrap();
        assert!(blocks[0]["text"].as_str().unwrap().contains("paper body"));
        assert_eq!(blocks[0]["cache_control"]["type"], "ephemeral");
        assert_eq!(blocks[1]["text"], "task-specific request");
    }
}
