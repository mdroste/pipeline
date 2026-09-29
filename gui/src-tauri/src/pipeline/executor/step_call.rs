//! One supervised unit call: retries, validation and response journaling.
use super::findings::{check_finding_lineage, check_validation_dispositions, PreservedFinding};
use crate::output::{extract_report_envelope, normalize_math_delimiters};
use std::sync::Arc;

pub(super) fn is_cancellation_error(error: &str) -> bool {
    crate::commands::is_pipeline_cancellation_error(error)
}

pub(super) fn cancellation_error(pass_key: &str) -> Option<String> {
    if crate::commands::is_cancelled() {
        Some("Pipeline cancelled".to_string())
    } else if crate::commands::is_pass_cancelled(pass_key) {
        Some(format!("Pass '{pass_key}' cancelled"))
    } else {
        None
    }
}

/// Read (and remove) a legacy model-written report file. New calls return the
/// report through the terminal response; this remains only as a validated
/// compatibility path for old sessions/templates.
pub(super) fn ingest_report_file_blocking(
    write_dir: Option<&str>,
    report_rel: &str,
) -> Option<String> {
    use std::io::Read as _;
    let dir = write_dir?;
    let path = std::path::Path::new(dir).join(report_rel);
    let file = crate::safety::open_regular_file(&path).ok()?;
    let mut bytes = Vec::with_capacity(64 * 1024);
    file.take(crate::pipeline::claude::MAX_STDOUT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    let _ = std::fs::remove_file(&path);
    if bytes.len() > crate::pipeline::claude::MAX_STDOUT_BYTES {
        return None;
    }
    let content = String::from_utf8(bytes).ok()?;
    if content.trim().is_empty() {
        return None;
    }
    Some(content.trim().to_string())
}

pub(super) async fn ingest_report_file(
    write_dir: Option<&str>,
    report_rel: &str,
) -> Option<String> {
    let directory = write_dir.map(str::to_string);
    let relative = report_rel.to_string();
    tokio::task::spawn_blocking(move || {
        ingest_report_file_blocking(directory.as_deref(), &relative)
    })
    .await
    .ok()
    .flatten()
}

pub(super) struct StepCallRequest<'a> {
    pub(super) app: &'a crate::emit::EventBus,
    pub(super) pass_key: &'a str,
    pub(super) log_label: &'a str,
    pub(super) prompt: &'a str,
    pub(super) system_prompt: Option<&'a str>,
    pub(super) tools: &'a [String],
    pub(super) agent: Option<&'a str>,
    pub(super) cwd: Option<&'a str>,
    pub(super) read_dirs: &'a [String],
    /// Host-owned run `artifacts/` root used for response journaling. This is
    /// deliberately broader than, and never granted as, the model write root.
    pub(super) run_artifact_dir: Option<&'a str>,
    pub(super) write_dir: Option<&'a str>,
    pub(super) report_rel: &'a str,
    pub(super) report_nonce: &'a str,
    pub(super) output_schema: Option<&'a serde_json::Value>,
    /// Ordered finding ids from the upstream artifact named by the schema's
    /// `x-pipeline-preserve-findings-from` marker. When present, the response
    /// must keep its findings' ids as an ordered subsequence of these.
    pub(super) preserved_finding_ids: Option<Vec<PreservedFinding>>,
    pub(super) command_model: Option<&'a str>,
    pub(super) display_model: &'a str,
    pub(super) model_policy: &'a str,
    pub(super) effort: &'a str,
    pub(super) settings: &'a crate::settings::Settings,
    pub(super) shared_context: Option<Arc<crate::pipeline::context_cache::PreparedContext>>,
}

pub(super) struct StepCallResult {
    pub(super) text: String,
    pub(super) duration_secs: u64,
    pub(super) usage: crate::pipeline::logging::CallUsage,
    pub(super) attempt_count: u32,
    pub(super) usage_limit_fallbacks: Vec<crate::pipeline::call::UsageLimitFallback>,
    pub(super) effective_fallback: Option<crate::pipeline::call::UsageLimitFallback>,
}

pub(super) async fn capture_response(
    request: &StepCallRequest<'_>,
    attempt: u32,
    source: &str,
    text: &str,
) -> Result<Option<crate::pipeline::response_journal::CapturedAttempt>, String> {
    crate::pipeline::response_journal::capture(
        request.run_artifact_dir,
        request.pass_key,
        attempt,
        source,
        text,
    )
    .await
    .map_err(|error| {
        format!(
            "{}: response could not be preserved; step stopped: {error}",
            request.log_label
        )
    })
}

pub(super) async fn finish_response_capture(
    request: &StepCallRequest<'_>,
    capture: Option<crate::pipeline::response_journal::CapturedAttempt>,
    status: crate::pipeline::response_journal::AttemptStatus,
    reason: &str,
) -> Result<(), String> {
    if let Some(capture) = capture {
        capture.finish(status, reason).await.map_err(|error| {
            format!(
                "{}: preserved response could not be classified; step stopped: {error}",
                request.log_label
            )
        })?;
    }
    Ok(())
}

/// Execute one logical step call, including retries, report-file handoff, and
/// structured-output validation. Scheduling and terminal pass events remain
/// with the parallel/sequential callers.
pub(super) async fn execute_step_call(
    request: StepCallRequest<'_>,
) -> Result<StepCallResult, String> {
    let timeout = request.settings.step_timeout_secs.max(60);
    let max_retries = request.settings.max_retries;
    let text_artifact_schema = request
        .output_schema
        .is_none()
        .then(crate::output::text_artifact_schema);
    let terminal_schema = request
        .output_schema
        .or(text_artifact_schema.as_ref())
        .expect("every step artifact has a terminal JSON schema");
    let mut last_error = String::new();
    let mut total_duration_secs = 0u64;
    let mut total_usage = crate::pipeline::logging::CallUsage::default();
    let mut usage_limit_fallbacks = Vec::new();

    for attempt in 0..=max_retries {
        let attempt_number = attempt.saturating_add(1);
        if let Some(error) = cancellation_error(request.pass_key) {
            return Err(error);
        }
        if attempt > 0 {
            let _ = request.app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!(
                    "{}: retry {attempt}/{max_retries} after failure: {last_error}",
                    request.log_label,
                )}),
            );
            let _ = request.app.emit_event(
                "pipeline:pass",
                serde_json::json!({ "name": request.pass_key, "status": "running" }),
            );
        }

        // Consume any stale compatibility file before starting this attempt.
        // New prompts never ask the model to write the report there, but old
        // provider sessions/templates may still do so.
        let _ = ingest_report_file(request.write_dir, request.report_rel).await;

        let mut retry_prompt = String::new();
        let prompt = if attempt > 0 {
            retry_prompt.push_str(request.prompt);
            crate::safety::push_str_limited(
                &mut retry_prompt,
                &format!(
                    "\n\nRETRY NOTICE:\nThe previous response was rejected: {last_error}\n\
                     Return the entire report again and satisfy the supplied response schema."
                ),
                crate::safety::MAX_EXPANDED_PROMPT_BYTES,
                "Step retry prompt",
            )?;
            retry_prompt.as_str()
        } else {
            request.prompt
        };

        let call = crate::pipeline::call::execute(crate::pipeline::call::Request {
            app: request.app,
            pass_key: request.pass_key,
            log_label: request.log_label,
            prompt,
            system_prompt: request.system_prompt,
            tools: request.tools,
            output_schema: Some(terminal_schema),
            timeout_secs: timeout,
            agent: request.agent,
            cwd: request.cwd,
            read_dirs: request.read_dirs,
            write_dir: request.write_dir,
            command_model: request.command_model,
            display_model: request.display_model,
            model_policy: request.model_policy,
            effort: request.effort,
            settings: request.settings,
            shared_context: request.shared_context.clone(),
        })
        .await;
        total_duration_secs = total_duration_secs.saturating_add(call.duration_secs);
        total_usage.add_usage(call.usage);
        let call_fallback = call.usage_limit_fallback.clone();
        if let Some(fallback) = call_fallback.clone() {
            usage_limit_fallbacks.push(fallback);
        }

        let compatibility_file = ingest_report_file(request.write_dir, request.report_rel).await;
        // Preserve provider-returned text before interpreting it. A malformed
        // envelope or schema can reject control-plane use without erasing the
        // report a reader may still want to inspect.
        let mut terminal_capture = match call.output.as_ref() {
            Ok(stdout) => capture_response(&request, attempt_number, "terminal", stdout).await?,
            Err(_) => None,
        };
        let mut file_capture = match compatibility_file.as_deref() {
            Some(report_file) => {
                capture_response(&request, attempt_number, "compatibility-file", report_file)
                    .await?
            }
            None => None,
        };

        if let Some(error) = cancellation_error(request.pass_key) {
            finish_response_capture(
                &request,
                terminal_capture.take(),
                crate::pipeline::response_journal::AttemptStatus::Ignored,
                &error,
            )
            .await?;
            finish_response_capture(
                &request,
                file_capture.take(),
                crate::pipeline::response_journal::AttemptStatus::Ignored,
                &error,
            )
            .await?;
            return Err(error);
        }

        let (text, mut accepted_capture) = match call.output {
            Ok(stdout) => {
                match crate::pipeline::structured::canonicalize(terminal_schema, &stdout) {
                    Ok(canonical) => {
                        finish_response_capture(
                            &request,
                            file_capture.take(),
                            crate::pipeline::response_journal::AttemptStatus::Ignored,
                            "The validated terminal structured response was selected instead.",
                        )
                        .await?;
                        (canonical, terminal_capture.take())
                    }
                    Err(stdout_error) => {
                        finish_response_capture(
                            &request,
                            terminal_capture.take(),
                            crate::pipeline::response_journal::AttemptStatus::RejectedSchema,
                            &stdout_error,
                        )
                        .await?;
                        if let Some(report_file) = compatibility_file {
                            let file_artifact = crate::pipeline::structured::canonicalize(
                                terminal_schema,
                                &report_file,
                            )
                            .or_else(|structured_error| {
                                if request.output_schema.is_some() {
                                    return Err(structured_error);
                                }
                                extract_report_envelope(&report_file, request.report_nonce)
                                    .map(|content| {
                                        serde_json::json!({"content": content}).to_string()
                                    })
                                    .map_err(|legacy_error| {
                                        format!(
                                            "{structured_error}; legacy report envelope was also invalid ({legacy_error})"
                                        )
                                    })
                            });
                            match file_artifact {
                                Ok(canonical) => {
                                    let _ = request.app.emit_event(
                                        "pipeline:log",
                                        serde_json::json!({ "line": format!(
                                            "{}: terminal response did not satisfy the output schema; accepted a validated compatibility artifact file",
                                            request.log_label,
                                        )}),
                                    );
                                    (canonical, file_capture.take())
                                }
                                Err(file_error) => {
                                    finish_response_capture(
                                        &request,
                                        file_capture.take(),
                                        crate::pipeline::response_journal::AttemptStatus::RejectedSchema,
                                        &file_error,
                                    )
                                    .await?;
                                    last_error = format!(
                                        "invalid terminal structured output ({stdout_error}); compatibility artifact file was also invalid ({file_error})"
                                    );
                                    continue;
                                }
                            }
                        } else {
                            last_error =
                                format!("invalid terminal structured output: {stdout_error}");
                            continue;
                        }
                    }
                }
            }
            Err(error) => {
                if is_cancellation_error(&error) {
                    finish_response_capture(
                        &request,
                        file_capture.take(),
                        crate::pipeline::response_journal::AttemptStatus::Ignored,
                        &error,
                    )
                    .await?;
                    return Err(error);
                }
                if let Some(report_file) = compatibility_file {
                    if request.output_schema.is_some() {
                        (report_file.trim().to_string(), file_capture.take())
                    } else {
                        let file_artifact = crate::pipeline::structured::canonicalize(
                            terminal_schema,
                            &report_file,
                        )
                        .or_else(|structured_error| {
                            extract_report_envelope(&report_file, request.report_nonce)
                                .map(|content| serde_json::json!({"content": content}).to_string())
                                .map_err(|legacy_error| {
                                    format!(
                                        "{structured_error}; legacy report envelope was also invalid ({legacy_error})"
                                    )
                                })
                        });
                        match file_artifact {
                            Ok(artifact) => {
                                let _ = request.app.emit_event(
                                    "pipeline:log",
                                    serde_json::json!({ "line": format!(
                                        "{}: call reported an error but wrote a complete validated compatibility report; using it. ({error})",
                                        request.log_label,
                                    )}),
                                );
                                (artifact, file_capture.take())
                            }
                            Err(file_error) => {
                                finish_response_capture(
                                    &request,
                                    file_capture.take(),
                                    crate::pipeline::response_journal::AttemptStatus::RejectedSchema,
                                    &file_error,
                                )
                                .await?;
                                last_error = format!(
                                    "{error}; compatibility report file did not contain a complete validated report ({file_error})"
                                );
                                continue;
                            }
                        }
                    }
                } else {
                    if crate::pipeline::provider_error::is_usage_limit_error(&error)
                        || crate::pipeline::provider_error::is_non_retryable_error(&error)
                    {
                        return Err(error);
                    }
                    last_error = error;
                    continue;
                }
            }
        };
        let text = if let Some(schema) = request.output_schema {
            let checked =
                crate::pipeline::structured::canonicalize(schema, &text).and_then(|canonical| {
                    match request.preserved_finding_ids.as_deref() {
                        Some(expected)
                            if schema
                                .get(crate::pipeline::structured::VALIDATION_LEDGER_KEY)
                                .is_some() =>
                        {
                            check_finding_lineage(expected, &canonical)?;
                            check_validation_dispositions(expected, &canonical)
                        }
                        Some(expected) => {
                            check_finding_lineage(expected, &canonical).map(|()| canonical)
                        }
                        None => Ok(canonical),
                    }
                });
            match checked {
                Ok(canonical) => canonical,
                Err(reason) => {
                    finish_response_capture(
                        &request,
                        accepted_capture.take(),
                        crate::pipeline::response_journal::AttemptStatus::RejectedSchema,
                        &reason,
                    )
                    .await?;
                    last_error = if attempt < max_retries {
                        format!("output did not satisfy schema: {reason}")
                    } else {
                        format!(
                            "output did not satisfy schema after {max_retries} retries: {reason}"
                        )
                    };
                    continue;
                }
            }
        } else {
            match crate::output::extract_text_artifact(&text) {
                Ok(content) => normalize_math_delimiters(&content),
                Err(reason) => {
                    finish_response_capture(
                        &request,
                        accepted_capture.take(),
                        crate::pipeline::response_journal::AttemptStatus::RejectedContent,
                        &reason,
                    )
                    .await?;
                    last_error = if attempt < max_retries {
                        format!("text artifact was unusable: {reason}")
                    } else {
                        format!("text artifact was unusable after {max_retries} retries: {reason}")
                    };
                    continue;
                }
            }
        };

        finish_response_capture(
            &request,
            accepted_capture.take(),
            crate::pipeline::response_journal::AttemptStatus::Accepted,
            if request.output_schema.is_some() {
                "Validated and canonicalized native structured output."
            } else {
                "Validated and extracted a schema-backed text artifact."
            },
        )
        .await?;

        return Ok(StepCallResult {
            text,
            duration_secs: total_duration_secs,
            usage: total_usage,
            attempt_count: u32::try_from(
                total_usage
                    .provider_attempts
                    .max(u64::from(attempt.saturating_add(1))),
            )
            .unwrap_or(u32::MAX),
            usage_limit_fallbacks,
            effective_fallback: call_fallback,
        });
    }

    Err(if last_error.is_empty() {
        "step produced no output".to_string()
    } else {
        last_error
    })
}

#[cfg(test)]
#[path = "tests/response_capture.rs"]
mod response_capture_tests;
