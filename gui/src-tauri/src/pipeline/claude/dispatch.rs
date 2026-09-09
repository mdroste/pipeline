//! Provider dispatch.

use super::*;

/// Dispatch an LLM call to the configured provider (Claude, Codex,
/// Google, or a local OpenAI-compatible server).
/// All pipeline code should call this instead of provider-specific functions directly.
///
/// `cwd`: optional working directory for the subprocess. Pass the paper's source
/// directory when the LLM needs to read figures or other assets alongside the paper.
/// When `None`, the subprocess runs in the system temp dir.
pub(super) fn request_provider_label(provider: &str, transport: &str) -> &'static str {
    match (provider, transport) {
        ("claude", "api") => "Anthropic",
        ("claude", _) => "Claude Code",
        ("codex", "api") => "OpenAI",
        ("codex", _) => "Codex",
        ("antigravity", _) => "Google",
        ("local", _) => "Local server",
        _ => "Unknown provider",
    }
}

pub(super) fn request_effort(
    provider: &str,
    transport: &str,
    model: &str,
    _settings: &crate::settings::Settings,
    overrides: &LlmOverrides<'_>,
) -> String {
    let configured = match provider {
        "claude" | "codex" => overrides.effort.unwrap_or("").trim(),
        _ => return "Not configurable".to_string(),
    };
    if configured.is_empty() {
        return "Provider default".to_string();
    }
    if provider == "claude" && transport == "api" {
        if model.starts_with("claude-haiku") {
            return "Not sent (unsupported by model)".to_string();
        }
        if !matches!(configured, "low" | "medium" | "high" | "max") {
            return "Not sent (unsupported value)".to_string();
        }
    }
    configured.to_string()
}

#[allow(clippy::too_many_arguments)]
pub async fn call_llm(
    app: &crate::emit::EventBus,
    prompt: &str,
    allowed_tools: &[&str],
    system_prompt: Option<&str>,
    output_format: &str,
    timeout_secs: u64,
    label: &str,
    provider_override: Option<&str>,
    cwd: Option<&str>,
    extra_read_dirs: &[&str],
    overrides: &LlmOverrides<'_>,
) -> Result<String, String> {
    // Tag every log line this call produces with a unique session id so the
    // frontend can separate concurrently running headless invocations.
    let session_id = crate::pipeline::logging::next_session_id();
    // Type-erase the dispatcher body before entering the generic task-local
    // scope. This prevents TaskLocalFuture from embedding and duplicating the
    // complete cross-provider state machine in debug builds.
    let operation: BoxedProviderFuture<'_> = Box::pin(async move {
        let settings = overrides
            .settings
            .cloned()
            .unwrap_or_else(crate::settings::load);
        let provider = provider_override.unwrap_or(&settings.preferred_provider);
        let provider = if provider.is_empty() {
            "claude"
        } else {
            provider
        };

        // Resolve a durable policy at the dispatch boundary. API Automatic
        // becomes a concrete available ID; CLI Automatic intentionally omits
        // the model flag so an older installed CLI keeps using its own default.
        let requested = overrides
            .model
            .map(crate::settings::ModelSelection::from_legacy);
        let resolution = if overrides.model_resolved {
            None
        } else {
            Some(
                crate::commands::await_or_cancel(
                    crate::model_catalog::resolve(provider, &settings, requested.as_ref()),
                    None,
                )
                .await??,
            )
        };
        let mut effective_overrides = overrides.clone();
        if let Some(resolution) = &resolution {
            effective_overrides.model = resolution.command_model.as_deref();
            effective_overrides.model_resolved = true;
            if let Some(effort) = effective_overrides.effort {
                if !resolution.supported_efforts.is_empty()
                    && !resolution
                        .supported_efforts
                        .iter()
                        .any(|item| item == effort)
                {
                    effective_overrides.effort = None;
                }
            }
        }
        let overrides = &effective_overrides;

        let transport = settings.model_transport(provider);
        let model = overrides
            .model_display
            .filter(|value| !value.trim().is_empty())
            .or(overrides.model.filter(|value| !value.trim().is_empty()))
            .map(str::to_string)
            .or_else(|| {
                resolution
                    .as_ref()
                    .map(|value| value.resolved_model.clone())
            })
            .unwrap_or_else(|| "Automatic (provider CLI default)".to_string());
        let model_policy = overrides
            .model_policy
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .or_else(|| resolution.as_ref().map(|value| value.selection.label()))
            .unwrap_or_else(|| {
                if overrides.model.is_some() {
                    "Resolved model".to_string()
                } else {
                    "Automatic".to_string()
                }
            });
        let effort = request_effort(provider, transport, &model, &settings, overrides);
        let provider_label = request_provider_label(provider, transport);
        let prompt_chars = prompt.chars().count();
        let prompt_preview = event_text_preview(prompt);
        let system_prompt_chars = system_prompt
            .map(|value| value.chars().count())
            .unwrap_or(0);
        let system_prompt_preview = system_prompt.map(event_text_preview);
        let shared_context = overrides
            .shared_context
            .as_ref()
            .map(|context| context.content());
        let shared_context_chars = shared_context
            .map(|value| value.chars().count())
            .unwrap_or(0);
        let shared_context_preview = shared_context.map(event_text_preview);
        let max_output_tokens = if transport != "api" {
            None
        } else {
            match provider {
                "claude" | "antigravity" => Some(overrides.max_output_tokens.unwrap_or(16_384)),
                "codex" | "local" => overrides.max_output_tokens,
                _ => overrides.max_output_tokens,
            }
        };
        let summary = format!(
            "LLM request · {provider_label} {} · model {model} · effort {effort} · \
             {prompt_chars} prompt characters",
            transport.to_ascii_uppercase(),
        );
        crate::pipeline::logging::emit_request(
            app,
            summary,
            serde_json::json!({
                "provider": provider,
                "provider_label": provider_label,
                "transport": transport,
                "model": model,
                "model_policy": model_policy,
                "effort": effort,
                "tools": allowed_tools,
                "timeout_secs": timeout_secs,
                "max_output_tokens": max_output_tokens,
                "output_format": output_format,
                "structured_output": overrides.output_schema.is_some(),
                "prompt": prompt_preview.text,
                "prompt_truncated": prompt_preview.truncated,
                "prompt_chars": prompt_chars,
                "system_prompt": system_prompt_preview.as_ref().map(|preview| preview.text.as_str()),
                "system_prompt_truncated": system_prompt_preview.as_ref().is_some_and(|preview| preview.truncated),
                "system_prompt_chars": system_prompt_chars,
                "shared_context": shared_context_preview.as_ref().map(|preview| preview.text.as_str()),
                "shared_context_truncated": shared_context_preview.as_ref().is_some_and(|preview| preview.truncated),
                "shared_context_chars": shared_context_chars,
                "pdf_attached": overrides.pdf_attachment.is_some() && transport == "api",
                "write_enabled": overrides.write_dir.is_some(),
                "working_directory": cwd,
                "read_directories": extra_read_dirs,
                "write_directory": overrides.write_dir,
                "local_endpoint": (provider == "local").then_some(settings.local_base_url.as_str()),
            }),
        );

        // Direct API path is an explicit credential-mode choice. A saved key
        // may remain available while subscription mode is active, and must
        // not silently change the transport.
        match provider {
            "claude" if transport == "api" => {
                if settings.anthropic_api_key.trim().is_empty() {
                    return Err("Claude API mode is selected, but no Anthropic API key is configured. Add the key in Settings → API Keys or switch Claude to Subscription mode.".to_string());
                }
                return crate::pipeline::api_anthropic::call_anthropic_api(
                    app,
                    prompt,
                    allowed_tools,
                    system_prompt,
                    timeout_secs,
                    label,
                    &settings,
                    extra_read_dirs,
                    overrides,
                )
                .await;
            }
            "codex" if transport == "api" => {
                if settings.openai_api_key.trim().is_empty() {
                    return Err("ChatGPT API mode is selected, but no OpenAI API key is configured. Add the key in Settings → API Keys or switch ChatGPT to Subscription mode.".to_string());
                }
                return crate::pipeline::api_openai::call_openai_api(
                    app,
                    prompt,
                    allowed_tools,
                    system_prompt,
                    timeout_secs,
                    label,
                    &settings,
                    extra_read_dirs,
                    overrides,
                )
                .await;
            }
            "antigravity" => {
                if settings.google_api_key.trim().is_empty() {
                    return Err("No Google AI API key is configured. Google runs through the Gemini API — add the key in Settings → API Keys.".to_string());
                }
                return crate::pipeline::api_google::call_google_api(
                    app,
                    prompt,
                    allowed_tools,
                    system_prompt,
                    timeout_secs,
                    label,
                    &settings,
                    extra_read_dirs,
                    overrides,
                )
                .await;
            }
            // Local OpenAI-compatible server (Ollama, LM Studio, llama.cpp, vLLM).
            // Always direct HTTP — there is no CLI fallback for this provider.
            "local" => {
                return crate::pipeline::api_openai::call_local_api(
                    app,
                    prompt,
                    allowed_tools,
                    system_prompt,
                    timeout_secs,
                    label,
                    &settings,
                    extra_read_dirs,
                    overrides,
                )
                .await;
            }
            _ => {}
        }

        // Remaining subscription transports use their owning CLI adapters.
        let cli_call: BoxedProviderFuture<'_> = Box::pin(async {
            match provider {
                "codex" => {
                    if settings.codex_backend == "app_server" {
                        return crate::pipeline::codex_server::call_codex(
                            app,
                            prompt,
                            allowed_tools,
                            system_prompt,
                            timeout_secs,
                            label,
                            extra_read_dirs,
                            overrides,
                        )
                        .await;
                    }
                    let codex_cwd = cwd.or_else(|| {
                        extra_read_dirs
                            .iter()
                            .copied()
                            .find(|path| !path.trim().is_empty())
                    });
                    crate::pipeline::codex::call_codex(
                        app,
                        prompt,
                        allowed_tools,
                        system_prompt,
                        output_format,
                        timeout_secs,
                        label,
                        codex_cwd,
                        overrides,
                    )
                    .await
                }
                _ => {
                    call_claude(
                        app,
                        prompt,
                        allowed_tools,
                        system_prompt,
                        output_format,
                        timeout_secs,
                        label,
                        cwd,
                        extra_read_dirs,
                        overrides,
                    )
                    .await
                }
            }
        });
        if let Some(write_dir) = overrides.write_dir {
            supervise_artifact_writes(
                cli_call,
                write_dir,
                crate::pipeline::logging::current_pass(),
            )
            .await
        } else {
            cli_call.await
        }
    });
    crate::pipeline::logging::with_session(session_id, label, operation).await
}
