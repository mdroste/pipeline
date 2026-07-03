//! Direct Anthropic Messages API client.

use super::api_common::*;
use super::claude::LlmOverrides;
use crate::settings::Settings;
use std::time::Instant;
use tauri::AppHandle;

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

/// Build Anthropic tools array from allowed tool names.
fn build_tools(allowed_tools: &[&str]) -> Vec<serde_json::Value> {
    let mut tools = Vec::new();
    if allowed_tools.iter().any(|t| *t == "Read") {
        let def = ReadToolDef::default();
        tools.push(serde_json::json!({
            "name": def.name,
            "description": def.description,
            "input_schema": def.input_schema,
        }));
    }
    tools
}

/// Call the Anthropic API directly, with tool-use loop for Read.
pub async fn call_anthropic_api(
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
    // Anthropic Messages API doesn't expose a "thinking effort" field on
    // current models, so overrides.effort is intentionally ignored here.
    let model = resolve_model(settings, overrides.model);
    log(app, format!("{label} started (API: Anthropic, model: {model})"));

    let client = &*super::api_common::HTTP_CLIENT;
    let tools = build_tools(allowed_tools);

    let messages = vec![AnthropicMessage {
        role: "user".to_string(),
        content: serde_json::Value::String(prompt.to_string()),
    }];

    let request = AnthropicRequest {
        model,
        max_tokens: 16384,
        system: system_prompt.map(|s| s.to_string()),
        messages,
        tools,
    };

    let (text, usage) = anthropic_tool_loop(app, &client, &settings.anthropic_api_key, request, timeout_secs, label)
        .await?;

    let elapsed = start.elapsed().as_secs();
    log(app, format!("{label} finished ({elapsed}s, {} chars output{})", text.len(), usage.summary()));

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
}
