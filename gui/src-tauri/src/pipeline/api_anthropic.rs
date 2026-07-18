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
    let tools = build_tools(allowed_tools);

    // With a PDF attachment, the user message is [document, text] content
    // blocks; otherwise a plain string.
    let content = match overrides.pdf_attachment {
        Some(pdf) => {
            let data = pdf_attachment_base64(pdf, MAX_ATTACH_PDF)?;
            serde_json::json!([
                {
                    "type": "document",
                    "source": {
                        "type": "base64",
                        "media_type": "application/pdf",
                        "data": data
                    }
                },
                { "type": "text", "text": prompt }
            ])
        }
        None => serde_json::Value::String(prompt.to_string()),
    };
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

    let (text, usage) = anthropic_tool_loop(
        app,
        client,
        &settings.anthropic_api_key,
        request,
        timeout_secs,
        label,
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
    super::logging::emit_usage(app, usage.input_tokens, usage.output_tokens);

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
}
