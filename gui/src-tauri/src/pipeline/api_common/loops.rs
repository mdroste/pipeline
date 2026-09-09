//! Provider tool loops for direct API providers.

use super::*;

// Every exit (including a later HTTP/tool failure or cancellation) accounts for
// the responses already received. Successful callers use the returned totals
// for display only, avoiding double counting warm-up and consumer calls.
struct UsageReceipt<'a> {
    app: &'a crate::emit::EventBus,
    usage: Usage,
}
impl std::ops::Deref for UsageReceipt<'_> {
    type Target = Usage;
    fn deref(&self) -> &Usage {
        &self.usage
    }
}
impl std::ops::DerefMut for UsageReceipt<'_> {
    fn deref_mut(&mut self) -> &mut Usage {
        &mut self.usage
    }
}
impl Drop for UsageReceipt<'_> {
    fn drop(&mut self) {
        crate::pipeline::logging::emit_usage(self.app, self.usage.call_usage());
    }
}

/// Run the tool-use loop for Anthropic. Returns (text, usage).
pub async fn anthropic_tool_loop(
    app: &crate::emit::EventBus,
    client: &reqwest::Client,
    api_key: &str,
    mut request: AnthropicRequest,
    timeout_secs: u64,
    label: &str,
    access: &ToolAccess,
) -> Result<(String, Usage), String> {
    let mut usage = UsageReceipt {
        app,
        usage: Usage::default(),
    };
    let mut tool_budget = ToolBudget::default();
    let start = std::time::Instant::now();

    for iteration in 0..MAX_TOOL_ITERATIONS {
        if crate::commands::is_cancelled() {
            return Err("Pipeline cancelled".into());
        }
        validate_tool_history(&request.messages, "Anthropic")?;

        let elapsed = start.elapsed().as_secs();
        if elapsed + MIN_REMAINING_SECS > timeout_secs {
            return Err(format!(
                "Step timeout ({timeout_secs}s) exceeded after {iteration} API requests"
            ));
        }
        let request_timeout = timeout_secs - elapsed;

        let pass_key = crate::pipeline::logging::current_pass();
        let request_builder = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .timeout(std::time::Duration::from_secs(request_timeout))
            .json(&request);
        let resp = send_with_status_retry(
            app,
            "Anthropic",
            label,
            request_builder,
            pass_key.as_deref(),
        )
        .await?;

        let status = resp.status();
        if !status.is_success() {
            let bytes =
                response_bytes_limited(resp, MAX_API_ERROR_BYTES, pass_key.as_deref()).await?;
            return Err(format_api_error(
                "Anthropic",
                status.as_u16(),
                &String::from_utf8_lossy(&bytes),
            ));
        }

        let bytes =
            response_bytes_limited(resp, MAX_API_RESPONSE_BYTES, pass_key.as_deref()).await?;
        let body: AnthropicResponse = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Failed to parse Anthropic response: {e}"))?;

        if let Some(u) = &body.usage {
            let uncached = u.input_tokens.unwrap_or(0);
            let cached = u.cache_read_input_tokens.unwrap_or(0);
            let cache_write = u.cache_creation_input_tokens.unwrap_or(0);
            let inp = uncached.saturating_add(cached).saturating_add(cache_write);
            let out = u.output_tokens.unwrap_or(0);
            let web_searches = anthropic_hosted_search_count(u);
            usage.add(inp, out, cached, cache_write);
            usage
                .tool_calls
                .add_kind(crate::models::ToolCallKind::Web, web_searches);
            verbose_log(
                app,
                format!(
                    "[api] {label}: tokens in={inp} out={out} cached={cached} cache-write={cache_write} hosted-web-searches={web_searches}"
                ),
            );
        } else {
            usage.requests = usage.requests.saturating_add(1);
        }

        if body.stop_reason.as_deref() == Some("pause_turn") {
            request.messages.push(AnthropicMessage {
                role: "assistant".to_string(),
                content: serde_json::Value::Array(body.content),
            });
            continue;
        }

        if !anthropic_has_tool_use(&body.content) || body.stop_reason.as_deref() != Some("tool_use")
        {
            if anthropic_output_incomplete(body.stop_reason.as_deref()) {
                return Err(format!(
                    "Anthropic truncated {label} because the output-token limit was reached"
                ));
            }
            return Ok((anthropic_extract_text(&body.content), usage.usage.clone()));
        }

        for block in &body.content {
            if block.get("type").and_then(serde_json::Value::as_str) == Some("tool_use") {
                let input = block.get("input").unwrap_or(&serde_json::Value::Null);
                let argument_bytes = serialized_size_limited(
                    input,
                    MAX_TOOL_ARGUMENT_BYTES,
                    "Anthropic tool-call arguments",
                )?;
                tool_budget.reserve_tool_call(argument_bytes)?;
                if let Some(name) = block.get("name").and_then(serde_json::Value::as_str) {
                    usage.tool_calls.add_kind(direct_tool_kind(name), 1);
                }
            }
        }

        // Build tool results
        let mut tool_results: Vec<serde_json::Value> = Vec::new();
        for block in &body.content {
            if block.get("type").and_then(serde_json::Value::as_str) != Some("tool_use") {
                continue;
            }
            let id = block
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let name = block
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let input = block.get("input").unwrap_or(&serde_json::Value::Null);
            let result = execute_tool(
                app,
                name,
                input,
                label,
                iteration,
                &mut tool_budget,
                access,
                &ANTHROPIC_MEDIA_POLICY,
            )
            .await;
            let (content, is_error) = anthropic_tool_result_content(result);
            let mut tool_result = serde_json::json!({
                "type": "tool_result",
                "tool_use_id": id,
                "content": content,
            });
            if is_error {
                tool_result["is_error"] = serde_json::Value::Bool(true);
            }
            tool_results.push(tool_result);
        }

        // Append assistant response + tool results to messages
        request.messages.push(AnthropicMessage {
            role: "assistant".to_string(),
            content: serde_json::Value::Array(body.content),
        });
        request.messages.push(AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::Value::Array(tool_results),
        });
    }

    Err(format!(
        "Tool loop exceeded {MAX_TOOL_ITERATIONS} iterations"
    ))
}

/// Run the tool-use loop for an OpenAI-compatible Chat Completions endpoint.
/// Returns (text, usage).
///
/// `base_url` is the API root without the `/chat/completions` suffix
/// (e.g. "https://api.openai.com/v1", "http://localhost:11434/v1").
/// `provider` names the endpoint in log/error messages.
/// When `drop_tools_on_400` is set (local servers), a 400 response to a
/// request that declared tools retries once without tools — many local
/// models don't support tool calling, and a hard failure would be opaque.
#[allow(clippy::too_many_arguments)]
pub async fn openai_tool_loop(
    app: &crate::emit::EventBus,
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    provider: &str,
    mut request: OpenAIRequest,
    timeout_secs: u64,
    label: &str,
    drop_tools_on_400: bool,
    access: &ToolAccess,
) -> Result<(String, Usage), String> {
    let mut usage = UsageReceipt {
        app,
        usage: Usage::default(),
    };
    let mut tool_budget = ToolBudget::default();
    let start = std::time::Instant::now();
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let mut tools_retry_used = false;
    let mut effort_retry_used = false;

    for iteration in 0..MAX_TOOL_ITERATIONS {
        if crate::commands::is_cancelled() {
            return Err("Pipeline cancelled".into());
        }
        validate_tool_history(&request.messages, provider)?;

        let elapsed = start.elapsed().as_secs();
        if elapsed + MIN_REMAINING_SECS > timeout_secs {
            return Err(format!(
                "Step timeout ({timeout_secs}s) exceeded after {iteration} API requests"
            ));
        }
        let request_timeout = timeout_secs - elapsed;

        let mut req = client
            .post(&url)
            .timeout(std::time::Duration::from_secs(request_timeout))
            .json(&request);
        if !api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {api_key}"));
        }
        let pass_key = crate::pipeline::logging::current_pass();
        let resp = send_with_status_retry(app, provider, label, req, pass_key.as_deref()).await?;

        let status = resp.status();
        if !status.is_success() {
            let bytes =
                response_bytes_limited(resp, MAX_API_ERROR_BYTES, pass_key.as_deref()).await?;
            let body = String::from_utf8_lossy(&bytes);
            if !effort_retry_used
                && status.as_u16() == 400
                && request.reasoning_effort.is_some()
                && (body.contains("reasoning_effort") || body.contains("Unsupported parameter"))
            {
                log(
                    app,
                    format!(
                        "WARNING: {label}: {provider} rejected reasoning_effort for this model — retrying once without it"
                    ),
                );
                request.reasoning_effort = None;
                effort_retry_used = true;
                continue;
            }
            if drop_tools_on_400
                && !tools_retry_used
                && status.as_u16() == 400
                && !request.tools.is_empty()
            {
                log(app, format!(
                    "{label}: {provider} rejected the request with tools declared — retrying without tools. \
                     The model won't be able to read files; consider a tool-capable model."
                ));
                request.tools = Vec::new();
                tools_retry_used = true;
                continue;
            }
            return Err(format_api_error(provider, status.as_u16(), &body));
        }

        let bytes =
            response_bytes_limited(resp, MAX_API_RESPONSE_BYTES, pass_key.as_deref()).await?;
        let body: OpenAIResponse = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Failed to parse {provider} response: {e}"))?;

        if let Some(u) = &body.usage {
            let inp = u.prompt_tokens.unwrap_or(0);
            let out = u.completion_tokens.unwrap_or(0);
            let cached = u
                .prompt_tokens_details
                .as_ref()
                .and_then(|details| details.cached_tokens)
                .unwrap_or(0);
            let cache_write = u
                .prompt_tokens_details
                .as_ref()
                .and_then(|details| details.cache_write_tokens)
                .unwrap_or(0);
            usage.add(inp, out, cached, cache_write);
            verbose_log(
                app,
                format!(
                    "[api] {label}: tokens in={inp} out={out} cached={cached} cache-write={cache_write}"
                ),
            );
        } else {
            usage.requests = usage.requests.saturating_add(1);
        }

        let choice = body
            .choices
            .first()
            .ok_or_else(|| format!("{provider} returned no choices"))?;

        if let Some(tool_calls) = &choice.message.tool_calls {
            if !tool_calls.is_empty() {
                for tool_call in tool_calls {
                    tool_budget.reserve_tool_call(tool_call.function.arguments.len())?;
                    usage
                        .tool_calls
                        .add_kind(direct_tool_kind(&tool_call.function.name), 1);
                }
                // Append assistant message with tool calls
                request.messages.push(choice.message.clone());

                // Execute each tool and append results
                let mut pending_images = Vec::new();
                for tc in tool_calls {
                    let result =
                        match serde_json::from_str::<serde_json::Value>(&tc.function.arguments) {
                            Ok(input) => {
                                execute_tool(
                                    app,
                                    &tc.function.name,
                                    &input,
                                    label,
                                    iteration,
                                    &mut tool_budget,
                                    access,
                                    &OPENAI_MEDIA_POLICY,
                                )
                                .await
                            }
                            Err(error) => ToolResult::Error(format!(
                                "Tool arguments were not valid JSON: {error}"
                            )),
                        };
                    append_openai_tool_result(
                        &mut request.messages,
                        &tc.id,
                        result,
                        &mut pending_images,
                    );
                }
                // OpenAI requires the complete set of tool messages to follow
                // the assistant tool-call message before any new user content.
                append_openai_image_message(&mut request.messages, pending_images);
                continue;
            }
        }

        let text = choice
            .message
            .content
            .as_ref()
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if openai_output_incomplete(choice.finish_reason.as_deref()) {
            return Err(format!(
                "{provider} did not complete {label} (finish reason: {})",
                choice.finish_reason.as_deref().unwrap_or("unknown")
            ));
        }
        return Ok((text, usage.usage.clone()));
    }

    Err(format!(
        "Tool loop exceeded {MAX_TOOL_ITERATIONS} iterations"
    ))
}

pub struct GoogleToolLoopContext<'a> {
    pub app: &'a crate::emit::EventBus,
    pub client: &'a reqwest::Client,
    pub api_key: &'a str,
    pub model: &'a str,
    pub timeout_secs: u64,
    pub label: &'a str,
    pub access: &'a ToolAccess,
}

/// Run the tool-use loop for Google. Returns (text, usage).
pub async fn google_tool_loop(
    context: GoogleToolLoopContext<'_>,
    mut request: GoogleRequest,
) -> Result<(String, Usage), String> {
    let GoogleToolLoopContext {
        app,
        client,
        api_key,
        model,
        timeout_secs,
        label,
        access,
    } = context;
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        model
    );
    let mut usage = UsageReceipt {
        app,
        usage: Usage::default(),
    };
    let mut tool_budget = ToolBudget::default();
    let start = std::time::Instant::now();

    for iteration in 0..MAX_TOOL_ITERATIONS {
        if crate::commands::is_cancelled() {
            return Err("Pipeline cancelled".into());
        }
        validate_tool_history(&request.contents, "Google")?;

        let elapsed = start.elapsed().as_secs();
        if elapsed + MIN_REMAINING_SECS > timeout_secs {
            return Err(format!(
                "Step timeout ({timeout_secs}s) exceeded after {iteration} API requests"
            ));
        }
        let request_timeout = timeout_secs - elapsed;

        let pass_key = crate::pipeline::logging::current_pass();
        let request_builder = client
            .post(&url)
            .header("x-goog-api-key", api_key)
            .timeout(std::time::Duration::from_secs(request_timeout))
            .json(&request);
        let resp =
            send_with_status_retry(app, "Google", label, request_builder, pass_key.as_deref())
                .await?;

        let status = resp.status();
        if !status.is_success() {
            let bytes =
                response_bytes_limited(resp, MAX_API_ERROR_BYTES, pass_key.as_deref()).await?;
            return Err(format_api_error(
                "Google",
                status.as_u16(),
                &String::from_utf8_lossy(&bytes),
            ));
        }

        let bytes =
            response_bytes_limited(resp, MAX_API_RESPONSE_BYTES, pass_key.as_deref()).await?;
        let body: GoogleResponse = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Failed to parse Google response: {e}"))?;

        if let Some(error) = &body.error {
            return Err(format!("Google API error: {}", error.message));
        }

        // Google returns usageMetadata at the top level
        if let Some(um) = body.usage_metadata.as_ref() {
            let prompt = um
                .get("promptTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let tool_prompt = um
                .get("toolUsePromptTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let candidates = um
                .get("candidatesTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let thoughts = um
                .get("thoughtsTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let inp = prompt.saturating_add(tool_prompt);
            let out = candidates.saturating_add(thoughts);
            let cached = um
                .get("cachedContentTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            usage.add(inp, out, cached, 0);
            verbose_log(
                app,
                format!(
                    "[api] {label}: tokens in={inp} out={out} cached={cached} thoughts={thoughts} tool-prompt={tool_prompt}"
                ),
            );
        } else {
            usage.requests += 1;
        }

        let candidate = body
            .candidates
            .as_ref()
            .and_then(|c| c.first())
            .ok_or("Google returned no candidates")?;
        let hosted_web_searches = google_hosted_search_count(candidate);
        if hosted_web_searches > 0 {
            usage
                .tool_calls
                .add_kind(crate::models::ToolCallKind::Web, hosted_web_searches);
            verbose_log(
                app,
                format!("[api] {label}: hosted-web-searches={hosted_web_searches}"),
            );
        }

        // Check for function calls
        let function_calls: Vec<GoogleFunctionCall> = candidate
            .content
            .parts
            .iter()
            .filter_map(|part| part.get("functionCall"))
            .map(|call| {
                serde_json::from_value(call.clone())
                    .map_err(|error| format!("Failed to parse Google function call: {error}"))
            })
            .collect::<Result<_, _>>()?;

        if !function_calls.is_empty() {
            for function_call in &function_calls {
                let argument_bytes = serialized_size_limited(
                    &function_call.args,
                    MAX_TOOL_ARGUMENT_BYTES,
                    "Google tool-call arguments",
                )?;
                tool_budget.reserve_tool_call(argument_bytes)?;
                usage
                    .tool_calls
                    .add_kind(direct_tool_kind(&function_call.name), 1);
            }
            // Append model response to contents
            request.contents.push(candidate.content.clone());

            // Build function response parts
            let mut response_parts: Vec<serde_json::Value> =
                Vec::with_capacity(function_calls.len());
            let mut image_parts = Vec::new();
            for fc in &function_calls {
                let result = execute_tool(
                    app,
                    &fc.name,
                    &fc.args,
                    label,
                    iteration,
                    &mut tool_budget,
                    access,
                    &GOOGLE_MEDIA_POLICY,
                )
                .await;
                let (function_response, images) = google_tool_result_parts(fc, result);
                response_parts.push(function_response);
                image_parts.extend(images);
            }
            response_parts.extend(image_parts);

            request.contents.push(GoogleContent {
                role: "user".to_string(),
                parts: response_parts,
            });
            continue;
        }

        // Extract text
        let text: String = candidate
            .content
            .parts
            .iter()
            .filter_map(|part| part.get("text").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>()
            .join("");

        if google_output_incomplete(candidate.finish_reason.as_deref()) {
            return Err(format!(
                "Google did not complete {label} (finish reason: {})",
                candidate.finish_reason.as_deref().unwrap_or("unknown")
            ));
        }

        return Ok((text, usage.usage.clone()));
    }

    Err(format!(
        "Tool loop exceeded {MAX_TOOL_ITERATIONS} iterations"
    ))
}

// ── Shared helpers ─────────────────────────────────────────────────

#[cfg(test)]
mod receipt_tests {
    use super::*;
    #[tokio::test]
    async fn failed_consumer_retains_warmup_and_partial_loop_usage_once() {
        let app: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
        let (result, total) = crate::pipeline::logging::measure_usage(async {
            {
                let mut warm = UsageReceipt {
                    app: &app,
                    usage: Usage::default(),
                };
                warm.add(11, 2, 0, 0);
            }
            let mut consumer = UsageReceipt {
                app: &app,
                usage: Usage::default(),
            };
            consumer.add(7, 3, 4, 0);
            consumer
                .tool_calls
                .add_kind(crate::models::ToolCallKind::Web, 1);
            Err::<(), _>("a later HTTP request failed")
        })
        .await;
        assert!(result.is_err());
        assert_eq!(total.input_tokens, 18);
        assert_eq!(total.output_tokens, 5);
        assert_eq!(total.cached_input_tokens, 4);
        assert_eq!(total.model_round_trips, 2);
        assert!(!total.tool_calls.is_empty());
    }
}
