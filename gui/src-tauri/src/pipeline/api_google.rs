//! Direct Google Gemini API client.

use super::api_common::*;
use super::claude::LlmOverrides;
use crate::settings::Settings;
use std::time::Instant;

/// Resolve the Google model ID for this call. `override_model` carries the
/// dispatcher-resolved selection; empty means the current default.
fn resolve_model(override_model: Option<&str>) -> String {
    match override_model.filter(|s| !s.trim().is_empty()) {
        Some(other) => other.to_string(),
        None => "gemini-3.6-flash".to_string(),
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
        let def = ReadTextBatchToolDef::default();
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
        let def = DocumentAssetsBatchToolDef::default();
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
    let mut tools = Vec::new();
    if !declarations.is_empty() {
        tools.push(serde_json::json!({ "functionDeclarations": declarations }));
    }
    if allowed_tools.contains(&"WebSearch") {
        // Gemini may issue multiple grounding queries within this one
        // GenerateContent request; usage is recovered from grounding metadata.
        tools.push(serde_json::json!({ "googleSearch": {} }));
    }
    tools
}

fn has_hosted_search(request: &GoogleRequest) -> bool {
    request
        .tools
        .iter()
        .any(|tool| tool.get("googleSearch").is_some())
}

fn has_custom_functions(request: &GoogleRequest) -> bool {
    request
        .tools
        .iter()
        .any(|tool| tool.get("functionDeclarations").is_some())
}

fn remove_hosted_search(request: &mut GoogleRequest) {
    request
        .tools
        .retain(|tool| tool.get("googleSearch").is_none());
    request.tool_config = None;
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
    let identifies_search = error.contains("googlesearch")
        || error.contains("google_search")
        || error.contains("google search")
        || error.contains("includeserversidetoolinvocations")
        || error.contains("include_server_side_tool_invocations")
        || error.contains("tool use with function calling")
        || (error.contains("search") && error.contains("function"));
    let identifies_capability = [
        "not supported",
        "unsupported",
        "not available",
        "not enabled",
        "cannot be combined",
        "only one tool",
        "invalid tool",
    ]
    .iter()
    .any(|needle| error.contains(needle));
    identifies_search && identifies_capability
}

fn build_contents(
    prompt: &str,
    pdf_attachment: Option<&std::path::Path>,
    shared_context: Option<&super::context_cache::PreparedContext>,
) -> Result<Vec<GoogleContent>, String> {
    let mut parts = Vec::new();
    if let Some(pdf) = pdf_attachment {
        let data = pdf_attachment_base64(pdf, MAX_ATTACH_PDF_GOOGLE)?;
        parts.push(serde_json::json!({
            "inlineData": {
                "mimeType": "application/pdf",
                "data": data,
            }
        }));
    }
    if let Some(context) = shared_context {
        parts.push(serde_json::json!({ "text": context.content() }));
    }
    parts.push(serde_json::json!({ "text": prompt }));
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
    read_dirs: &[&str],
    overrides: &LlmOverrides<'_>,
) -> Result<String, String> {
    let start = Instant::now();
    // Google's Gemini API doesn't expose an effort/thinking flag in this client,
    // so overrides.effort is ignored here.
    let model = resolve_model(overrides.model);
    log(
        app,
        format!("{label} started (API: Google, model: {model})"),
    );

    let client = &*super::api_common::HTTP_CLIENT;
    let access = ToolAccess::new(read_dirs, overrides.write_dir);
    let tools = build_tools(allowed_tools);

    let system_instruction = system_prompt.map(|s| GoogleContent {
        role: "user".to_string(),
        parts: vec![serde_json::json!({ "text": s })],
    });

    // With a PDF attachment, the user content is [inline PDF, text] parts;
    // otherwise just text. Google's inline-data path has a smaller request
    // cap than the other providers (see MAX_ATTACH_PDF_GOOGLE).
    let shared_context = overrides.shared_context.as_deref();
    let contents = build_contents(prompt, overrides.pdf_attachment, shared_context)?;

    let mut request = GoogleRequest {
        contents,
        system_instruction,
        tools,
        tool_config: None,
        generation_config: Some(serde_json::json!({
            "maxOutputTokens": overrides
                .max_output_tokens
                .unwrap_or(DEFAULT_STEP_MAX_OUTPUT_TOKENS),
        })),
    };
    if has_hosted_search(&request) && has_custom_functions(&request) {
        request.tool_config = Some(serde_json::json!({
            "includeServerSideToolInvocations": true
        }));
    }

    let mut warm_usage = Usage::default();
    if let Some(context) = shared_context {
        let tools_key = serde_json::to_string(&(request.tools.as_slice(), &request.tool_config))
            .unwrap_or_default();
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
                GoogleToolLoopContext {
                    app,
                    client,
                    api_key: &settings.google_api_key,
                    model: &model,
                    timeout_secs,
                    label: &warm_label,
                    access: &access,
                },
                warm_request,
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

    let mut fallback_request = request.clone();
    let hosted_search_enabled = has_hosted_search(&request);
    super::logging::record_provider_attempt();
    let result = google_tool_loop(
        GoogleToolLoopContext {
            app,
            client,
            api_key: &settings.google_api_key,
            model: &model,
            timeout_secs,
            label,
            access: &access,
        },
        request,
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
                    "WARNING: {label}: Google rejected hosted web search for this model or tool combination; retrying once without hosted search"
                ),
            );
            remove_hosted_search(&mut fallback_request);
            super::logging::record_provider_attempt();
            google_tool_loop(
                GoogleToolLoopContext {
                    app,
                    client,
                    api_key: &settings.google_api_key,
                    model: &model,
                    timeout_secs: retry_timeout,
                    label,
                    access: &access,
                },
                fallback_request,
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
        let shared = contents[0].parts[0]["text"].as_str().unwrap();
        let task = contents[0].parts[1]["text"].as_str().unwrap();
        assert!(shared.contains("paper body"));
        assert_eq!(task, "task-specific request");
    }

    #[test]
    fn tools_include_batch_pairs_and_google_search() {
        let tools = build_tools(&["Read", "ReadDocumentAsset", "WebSearch"]);
        let declarations = tools[0]["functionDeclarations"].as_array().unwrap();
        let names = declarations
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "Read",
                "ReadTextBatch",
                "ReadDocumentAsset",
                "ReadDocumentAssetsBatch"
            ]
        );
        assert!(tools[1]["googleSearch"].is_object());
    }

    #[test]
    fn hosted_search_fallback_is_narrow_and_removes_only_google_tool() {
        assert!(hosted_search_capability_error(
            "Google API error (HTTP 400): googleSearch is unsupported with function declarations"
        ));
        assert!(!hosted_search_capability_error(
            "Google API error (HTTP 400): maxOutputTokens is invalid"
        ));
        assert!(!hosted_search_capability_error(
            "Google API error (HTTP 500): googleSearch is unsupported"
        ));
        assert!(hosted_search_capability_error(
            "Google API error (HTTP 403): access to googleSearch is not enabled"
        ));

        let mut request = GoogleRequest {
            contents: Vec::new(),
            system_instruction: None,
            tools: build_tools(&["Read", "WebSearch"]),
            tool_config: Some(serde_json::json!({
                "includeServerSideToolInvocations": true
            })),
            generation_config: None,
        };
        assert!(has_hosted_search(&request));
        assert!(has_custom_functions(&request));
        remove_hosted_search(&mut request);
        assert!(!has_hosted_search(&request));
        assert!(request.tool_config.is_none());
        assert!(request.tools[0]["functionDeclarations"].is_array());
    }
}
