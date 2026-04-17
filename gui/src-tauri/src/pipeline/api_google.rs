//! Direct Google Gemini API client.

use super::api_common::*;
use crate::settings::Settings;
use std::time::Instant;
use tauri::AppHandle;

/// Map settings model shorthand to Google model ID.
fn resolve_model(settings: &Settings) -> String {
    match settings.gemini_model.as_str() {
        "" => "gemini-2.5-flash".to_string(),
        other => other.to_string(),
    }
}

/// Build Google tools array from allowed tool names.
fn build_tools(allowed_tools: &[&str]) -> Vec<serde_json::Value> {
    let mut declarations = Vec::new();
    if allowed_tools.iter().any(|t| *t == "Read") {
        let def = ReadToolDef::default();
        declarations.push(serde_json::json!({
            "name": def.name,
            "description": def.description,
            "parameters": def.input_schema,
        }));
    }
    if declarations.is_empty() {
        Vec::new()
    } else {
        vec![serde_json::json!({ "functionDeclarations": declarations })]
    }
}

/// Call the Google Gemini API directly, with tool-use loop for Read.
pub async fn call_google_api(
    app: &AppHandle,
    prompt: &str,
    allowed_tools: &[&str],
    system_prompt: Option<&str>,
    timeout_secs: u64,
    label: &str,
    settings: &Settings,
) -> Result<String, String> {
    let start = Instant::now();
    let model = resolve_model(settings);
    log(app, format!("{label} started (API: Google, model: {model})"));

    let client = &*super::api_common::HTTP_CLIENT;
    let tools = build_tools(allowed_tools);

    let system_instruction = system_prompt.map(|s| GoogleContent {
        role: "user".to_string(),
        parts: vec![GooglePart::Text {
            text: s.to_string(),
        }],
    });

    let contents = vec![GoogleContent {
        role: "user".to_string(),
        parts: vec![GooglePart::Text {
            text: prompt.to_string(),
        }],
    }];

    let request = GoogleRequest {
        contents,
        system_instruction,
        tools,
        generation_config: Some(serde_json::json!({
            "maxOutputTokens": 16384,
        })),
    };

    let (text, usage) = google_tool_loop(
        app,
        &client,
        &settings.google_api_key,
        &model,
        request,
        timeout_secs,
        label,
    )
    .await?;

    let elapsed = start.elapsed().as_secs();
    log(app, format!("{label} finished ({elapsed}s, {} chars output{})", text.len(), usage.summary()));

    if text.trim().is_empty() {
        return Err(format!("{label}: Google API returned empty output"));
    }

    Ok(text.trim().to_string())
}
