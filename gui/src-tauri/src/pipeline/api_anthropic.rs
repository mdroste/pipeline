//! Direct Anthropic Messages API client.

use super::api_common::*;
use super::claude::LlmOverrides;
use crate::settings::Settings;
use std::time::Instant;

/// Map a resolved model shorthand to a full Anthropic model ID. An empty
/// override uses the provider's automatic model.
fn resolve_model(_settings: &Settings, override_model: Option<&str>) -> String {
    let raw = override_model
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("");
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
        let def = ReadTextBatchToolDef::default();
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
        let def = DocumentAssetsBatchToolDef::default();
        tools.push(serde_json::json!({
            "name": def.name,
            "description": def.description,
            "input_schema": def.input_schema,
        }));
    }
    if allowed_tools.contains(&"WebSearch") {
        // Hosted search can execute several queries inside one Messages API
        // request. Keep a hard per-request cap so a broad literature prompt
        // cannot generate an unbounded number of billable searches.
        tools.push(serde_json::json!({
            "type": "web_search_20250305",
            "name": "web_search",
            "max_uses": MAX_HOSTED_WEB_SEARCH_USES,
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

fn has_hosted_search(request: &AnthropicRequest) -> bool {
    request.tools.iter().any(|tool| {
        tool.get("type")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|kind| kind.starts_with("web_search_"))
    })
}

fn remove_hosted_search(request: &mut AnthropicRequest) {
    request.tools.retain(|tool| {
        !tool
            .get("type")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|kind| kind.starts_with("web_search_"))
    });
}

/// Ceiling accepted by every current Claude model, used when a model rejects
/// the DEFAULT_STEP_MAX_OUTPUT_TOKENS default.
const FALLBACK_MAX_OUTPUT_TOKENS: u32 = 16_384;

/// A 400 whose body says max_tokens exceeds the model's output ceiling, e.g.
/// "max_tokens: 32000 > 16384, which is the maximum allowed number of output
/// tokens for ...".
fn max_tokens_capability_error(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("http 400") && error.contains("max_tokens") && error.contains("maximum")
}

fn hosted_search_capability_error(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    let validation_error = error.contains("http 400");
    let permission_error = error.contains("http 403")
        && ["permission", "not enabled", "access"]
            .iter()
            .any(|needle| error.contains(needle));
    if !validation_error && !permission_error {
        return false;
    }
    let identifies_search = error.contains("web_search") || error.contains("web search");
    let identifies_capability = [
        "not supported",
        "unsupported",
        "not available",
        "not enabled",
        "permission",
        "invalid tool",
        "unrecognized tool",
    ]
    .iter()
    .any(|needle| error.contains(needle));
    identifies_search && identifies_capability
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
        .unwrap_or("");
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
        max_tokens: overrides
            .max_output_tokens
            .unwrap_or(DEFAULT_STEP_MAX_OUTPUT_TOKENS),
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

    let mut fallback_request = request.clone();
    let hosted_search_enabled = has_hosted_search(&request);
    super::logging::record_provider_attempt();
    let result = anthropic_tool_loop(
        app,
        client,
        &settings.anthropic_api_key,
        request,
        timeout_secs,
        label,
        &access,
    )
    .await;
    let (text, mut usage) = match result {
        Err(error) if hosted_search_enabled && hosted_search_capability_error(&error) => {
            let retry_timeout = timeout_secs.saturating_sub(start.elapsed().as_secs());
            if retry_timeout == 0 {
                return Err(error);
            }
            log(
                app,
                format!(
                    "WARNING: {label}: Anthropic rejected hosted web search for this model or organization; retrying once without hosted search"
                ),
            );
            remove_hosted_search(&mut fallback_request);
            super::logging::record_provider_attempt();
            anthropic_tool_loop(
                app,
                client,
                &settings.anthropic_api_key,
                fallback_request,
                retry_timeout,
                label,
                &access,
            )
            .await?
        }
        Err(error)
            if fallback_request.max_tokens > FALLBACK_MAX_OUTPUT_TOKENS
                && max_tokens_capability_error(&error) =>
        {
            let retry_timeout = timeout_secs.saturating_sub(start.elapsed().as_secs());
            if retry_timeout == 0 {
                return Err(error);
            }
            log(
                app,
                format!(
                    "{label}: this model's output ceiling is below max_tokens={}; retrying once with {FALLBACK_MAX_OUTPUT_TOKENS}",
                    fallback_request.max_tokens
                ),
            );
            fallback_request.max_tokens = FALLBACK_MAX_OUTPUT_TOKENS;
            super::logging::record_provider_attempt();
            anthropic_tool_loop(
                app,
                client,
                &settings.anthropic_api_key,
                fallback_request,
                retry_timeout,
                label,
                &access,
            )
            .await?
        }
        result => result?,
    };
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

    #[test]
    fn tools_include_batch_pairs_and_bounded_hosted_search() {
        let tools = build_tools(&["Read", "ReadDocumentAsset", "WebSearch"]);
        let names = tools
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "Read",
                "ReadTextBatch",
                "ReadDocumentAsset",
                "ReadDocumentAssetsBatch",
                "web_search"
            ]
        );
        let hosted = tools.last().unwrap();
        assert_eq!(hosted["type"], "web_search_20250305");
        assert_eq!(hosted["max_uses"], MAX_HOSTED_WEB_SEARCH_USES);
    }

    #[test]
    fn max_tokens_fallback_matches_only_output_ceiling_rejections() {
        assert!(max_tokens_capability_error(
            "Anthropic API error (HTTP 400): max_tokens: 32000 > 16384, which is the maximum allowed number of output tokens for claude-haiku-4-5"
        ));
        assert!(!max_tokens_capability_error(
            "Anthropic API error (HTTP 400): max_tokens must be positive"
        ));
        assert!(!max_tokens_capability_error(
            "Anthropic API error (HTTP 500): internal error"
        ));
        assert!(!max_tokens_capability_error(
            "Anthropic API error (HTTP 400): web_search_20250305 is not enabled for this organization"
        ));
    }

    #[test]
    fn hosted_search_fallback_is_narrow_and_removes_only_server_tool() {
        assert!(hosted_search_capability_error(
            "Anthropic API error (HTTP 400): web_search_20250305 is not enabled for this organization"
        ));
        assert!(!hosted_search_capability_error(
            "Anthropic API error (HTTP 400): max_tokens must be positive"
        ));
        assert!(!hosted_search_capability_error(
            "Anthropic API error (HTTP 500): web_search is not available"
        ));
        assert!(hosted_search_capability_error(
            "Anthropic API error (HTTP 403): permission denied for web_search"
        ));

        let mut request = AnthropicRequest {
            model: "model".to_string(),
            max_tokens: 10,
            system: None,
            messages: Vec::new(),
            tools: build_tools(&["Read", "WebSearch"]),
            output_config: None,
        };
        assert!(has_hosted_search(&request));
        remove_hosted_search(&mut request);
        assert!(!has_hosted_search(&request));
        assert!(request.tools.iter().any(|tool| tool["name"] == "Read"));
    }
}
