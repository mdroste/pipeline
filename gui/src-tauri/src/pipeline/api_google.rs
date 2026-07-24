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
        "" => "gemini-3.6-flash".to_string(),
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
    if allowed_tools.contains(&"ReadDocumentAsset") {
        let def = DocumentAssetToolDef::default();
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

fn build_contents(
    prompt: &str,
    pdf_attachment: Option<&std::path::Path>,
    shared_context: Option<&super::context_cache::PreparedContext>,
) -> Result<Vec<GoogleContent>, String> {
    let mut parts = Vec::new();
    if let Some(pdf) = pdf_attachment {
        let data = pdf_attachment_base64(pdf, MAX_ATTACH_PDF_GOOGLE)?;
        parts.push(GooglePart::InlineData {
            inline_data: GoogleInlineData {
                mime_type: "application/pdf".to_string(),
                data,
            },
        });
    }
    if let Some(context) = shared_context {
        parts.push(GooglePart::Text {
            text: context.content().to_string(),
        });
    }
    parts.push(GooglePart::Text {
        text: prompt.to_string(),
    });
    Ok(vec![GoogleContent {
        role: "user".to_string(),
        parts,
    }])
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
    let shared_context = overrides.shared_context.as_deref();
    let contents = build_contents(prompt, overrides.pdf_attachment, shared_context)?;

    let request = GoogleRequest {
        contents,
        system_instruction,
        tools,
        generation_config: Some(serde_json::json!({
            "maxOutputTokens": overrides.max_output_tokens.unwrap_or(16384),
        })),
    };

    let mut warm_usage = Usage::default();
    if let Some(context) = shared_context {
        let tools_key = serde_json::to_string(&request.tools).unwrap_or_default();
        let system_key = serde_json::to_string(&request.system_instruction).unwrap_or_default();
        let attachment_key = overrides
            .pdf_attachment
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        let cache_key = context.compatibility_key(
            "google-api",
            [
                model.as_str(),
                system_key.as_str(),
                tools_key.as_str(),
                attachment_key.as_str(),
            ],
        );
        let slot = context.slot(cache_key).await;
        let mut state = slot.lock().await;
        if state.is_none() {
            let mut warm_request = request.clone();
            warm_request.contents = build_contents(
                "Acknowledge this shared context by replying only: Context prepared. Do not call tools.",
                overrides.pdf_attachment,
                Some(context),
            )?;
            warm_request.generation_config = Some(serde_json::json!({ "maxOutputTokens": 32 }));
            let warm_label = format!("{label} · cache warm-up");
            super::logging::record_provider_attempt();
            match google_tool_loop(
                app,
                client,
                &settings.google_api_key,
                &model,
                warm_request,
                timeout_secs,
                &warm_label,
            )
            .await
            {
                Ok((_text, usage)) => {
                    warm_usage = usage;
                    *state = Some("ready".to_string());
                    log(app, format!("{label}: Google shared context warmed"));
                }
                Err(error) => {
                    *state = Some("implicit".to_string());
                    log(
                        app,
                        format!(
                            "WARNING: {label}: Google cache warm-up was unavailable ({error}); continuing with implicit prefix caching"
                        ),
                    );
                }
            }
        }
    }

    super::logging::record_provider_attempt();
    let (text, mut usage) = google_tool_loop(
        app,
        client,
        &settings.google_api_key,
        &model,
        request,
        timeout_secs,
        label,
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
        return Err(format!("{label}: Google API returned empty output"));
    }

    Ok(text.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_context_precedes_google_task_content() {
        let context = super::super::context_cache::PreparedContext::new(
            "paper body",
            &serde_json::json!({"map": "orientation"}),
        )
        .unwrap();
        let contents = build_contents("task-specific request", None, Some(&context)).unwrap();
        let GooglePart::Text { text: shared } = &contents[0].parts[0] else {
            panic!("expected text prefix")
        };
        let GooglePart::Text { text: task } = &contents[0].parts[1] else {
            panic!("expected task text")
        };
        assert!(shared.contains("paper body"));
        assert_eq!(task, "task-specific request");
    }
}
