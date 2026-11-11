//! Provider dispatch.

use artifacts::{event_text_preview, supervise_artifact_writes};
mod request;
pub use request::LlmOverrides;
pub(super) mod artifacts;
use std::{future::Future, pin::Pin};
pub(super) type BoxedProviderFuture<'a> =
    Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>>;

use super::claude::call_claude;

/// Dispatch an LLM call to the configured provider (Claude, Codex,
/// Google, or a local OpenAI-compatible server).
/// All pipeline code should call this instead of provider-specific functions directly.
///
/// `cwd`: optional working directory for the subprocess. Pass the paper's source
/// directory when the LLM needs to read figures or other assets alongside the paper.
/// When `None`, the subprocess runs in the system temp dir.
pub(crate) fn request_provider_label(provider: &str, transport: &str) -> &'static str {
    crate::model_catalog::model_chooser(provider, transport)
}

pub(crate) fn request_effort(
    provider: &str,
    transport: &str,
    model: &str,
    _settings: &crate::settings::Settings,
    overrides: &LlmOverrides<'_>,
) -> String {
    let configured = match provider {
        "claude" | "codex" | "antigravity" | "local" => overrides.effort.unwrap_or("").trim(),
        _ => return "Not configurable".to_string(),
    };
    if configured.is_empty() {
        // Name who decides, so an unset level never reads as a bare "default".
        return format!("{} default", request_provider_label(provider, transport));
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

        let connection = crate::connections::ConnectionId::for_review(provider, &settings)?;
        let transport = connection.legacy_transport();
        // CLI Automatic sends no model flag, so the CLI picks. Name the model
        // it is expected to pick (from its model list or an earlier call) and
        // check it against what the CLI reports once the call finishes.
        let cli_chooses =
            transport != "api" && overrides.model.map(str::trim).is_none_or(str::is_empty);
        let chooser = request_provider_label(provider, transport);
        let expected_model = cli_chooses
            .then(|| crate::model_catalog::cli_default_model(provider))
            .flatten();
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
        crate::pipeline::logging::record_resolved_model(&model);
        let provider_label = chooser;
        let (model_text, model_basis) = if cli_chooses {
            match expected_model.as_deref() {
                Some(expected) => (format!("{expected} ({chooser}'s choice)"), "expected"),
                None => (format!("chosen by {chooser}"), "unknown"),
            }
        } else {
            (model.clone(), "requested")
        };
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
            "LLM request · {provider_label} {} · model {model_text} · effort {effort} · \
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
                "model": if cli_chooses {
                    expected_model.as_deref().unwrap_or("")
                } else {
                    model.as_str()
                },
                "model_basis": model_basis,
                "model_chosen_by": cli_chooses.then_some(chooser),
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

        let dispatch: BoxedProviderFuture<'_> = Box::pin(async {
            // Direct API path is an explicit credential-mode choice. A saved key
            // may remain available while subscription mode is active, and must
            // not silently change the transport.
            use crate::connections::ConnectionId;
            if let Some(api) = connection.api() {
                api.key(&settings)?;
            }
            match connection {
                ConnectionId::AnthropicApi => {
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
                ConnectionId::OpenAiApi => {
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
                ConnectionId::GeminiApi => {
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
                ConnectionId::Local => {
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
                        if connection.transport(&settings)
                            == crate::connections::Transport::CodexAppServer
                        {
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
        let (result, reported) = crate::pipeline::logging::observe_reported_model(dispatch).await;
        if let Some(reported) = reported.as_deref() {
            // Hand the report on to the caller's call record.
            crate::pipeline::logging::record_reported_model(reported);
            if cli_chooses {
                note_cli_choice(
                    app,
                    label,
                    provider,
                    chooser,
                    expected_model.as_deref(),
                    reported,
                );
            }
        }
        result
    });
    crate::pipeline::logging::with_session(session_id, label, operation).await
}

/// Report which model a CLI picked on its own. A model that differs from the
/// one Pipeline expected is a warning: what the app showed before the call
/// was wrong.
fn note_cli_choice(
    app: &crate::emit::EventBus,
    label: &str,
    provider: &str,
    chooser: &str,
    expected: Option<&str>,
    reported: &str,
) {
    crate::model_catalog::remember_cli_default(provider, reported);
    match expected {
        Some(expected) if expected == reported => {}
        Some(expected) => crate::pipeline::logging::emit(
            app,
            format!(
                "WARNING: {label}: {chooser} used {reported}, not {expected} as expected. \
                 Its default model may have changed."
            ),
        ),
        None => crate::pipeline::logging::emit(app, format!("{label}: {chooser} chose {reported}")),
    }
}
