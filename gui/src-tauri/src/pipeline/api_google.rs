//! Direct Google Gemini API client.

use super::api_common::*;
use super::claude::LlmOverrides;
use crate::settings::Settings;
use std::time::Instant;

/// Map settings model shorthand to Google model ID.
/// `override_model` (when non-empty) takes precedence over the global setting.
fn resolve_model(settings: &Settings, override_model: Option<&str>) -> String {
    let raw = override_model
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(settings.gemini_model.as_str());
    match raw {
        "" => "gemini-2.5-flash".to_string(),
        other => other.to_string(),
    }
}

/// Build Google tools array from allowed tool names.
fn build_tools(allowed_tools: &[&str]) -> Vec<serde_json::Value> {
    let mut declarations = Vec::new();
    if allowed_tools.contains(&"Read") {
        let def = ReadToolDef::default();
        declarations.push(serde_json::json!({
            "name": def.name,
            "description": def.description,
            "parameters": def.input_schema,
        }));
    }
    if allowed_tools.contains(&"Write") {
        let def = WriteToolDef::default();
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
#[allow(clippy::too_many_arguments)]
pub async fn call_google_api(
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
    // Google's Gemini API doesn't expose an effort/thinking flag in this client,
    // so overrides.effort is ignored here.
    let model = resolve_model(settings, overrides.model);
    log(
        app,
        format!("{label} started (API: Google, model: {model})"),
    );

    let client = &*super::api_common::HTTP_CLIENT;
    let tools = build_tools(allowed_tools);

    let system_instruction = system_prompt.map(|s| GoogleContent {
        role: "user".to_string(),
        parts: vec![GooglePart::Text {
            text: s.to_string(),
        }],
    });

    // With a PDF attachment, the user content is [inline PDF, text] parts;
    // otherwise just text. Google's inline-data path has a smaller request
    // cap than the other providers (see MAX_ATTACH_PDF_GOOGLE).
    let mut parts = Vec::new();
    if let Some(pdf) = overrides.pdf_attachment {
        let data = pdf_attachment_base64(pdf, MAX_ATTACH_PDF_GOOGLE)?;
        parts.push(GooglePart::InlineData {
            inline_data: GoogleInlineData {
                mime_type: "application/pdf".to_string(),
                data,
            },
        });
    }
    parts.push(GooglePart::Text {
        text: prompt.to_string(),
    });
    let contents = vec![GoogleContent {
        role: "user".to_string(),
        parts,
    }];

    let request = GoogleRequest {
        contents,
        system_instruction,
        tools,
        generation_config: Some(serde_json::json!({
            "maxOutputTokens": overrides.max_output_tokens.unwrap_or(16384),
        })),
    };

    let (text, usage) = google_tool_loop(
        app,
        client,
        &settings.google_api_key,
        &model,
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
        return Err(format!("{label}: Google API returned empty output"));
    }

    Ok(text.trim().to_string())
}
