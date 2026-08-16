use super::call::{execute_text, OwnedRequest};
use crate::models::ExtractionResult;

const MAX_PAPER_TEXT: usize = 250_000;
const MAX_RETRIES: usize = 2;

fn stock_survey_name(profile_prompt: &str) -> Option<&'static str> {
    let trimmed = profile_prompt.trim();
    if trimmed.is_empty()
        || Some(trimmed) == crate::prompts::compiled_default("orientation").map(str::trim)
    {
        Some("orientation")
    } else if Some(trimmed)
        == crate::prompts::compiled_default("orientation_generic").map(str::trim)
    {
        Some("orientation_generic")
    } else if Some(trimmed) == crate::prompts::compiled_default("orientation_folder").map(str::trim)
    {
        Some("orientation_folder")
    } else {
        None
    }
}

fn effective_stock_survey_name(profile_prompt: &str, input_mode: &str) -> Option<&'static str> {
    let name = stock_survey_name(profile_prompt)?;
    if input_mode == "folder" && matches!(name, "orientation" | "orientation_generic") {
        Some("orientation_folder")
    } else {
        Some(name)
    }
}

/// Pick the survey template for a run, given the profile's survey prompt and
/// the effective input mode.
///
/// A user-customized prompt is always used verbatim. A stock prompt (empty,
/// or byte-equal to the compiled-in paper or generic survey template) adapts
/// to the input: folder inputs get the folder survey, which explores the tree
/// with the Read tool instead of surveying the file inventory text. Returns
/// `None` to mean "use the default paper template" (build_orientation_map
/// resolves it), matching the previous behavior for document inputs.
pub fn resolve_survey_template(profile_prompt: &str, input_mode: &str) -> Option<String> {
    let trimmed = profile_prompt.trim();
    if input_mode == "folder"
        && matches!(
            stock_survey_name(profile_prompt),
            Some("orientation" | "orientation_generic")
        )
    {
        return crate::prompts::load_prompt("orientation_folder").ok();
    }
    if trimmed.is_empty() {
        None
    } else {
        Some(profile_prompt.to_string())
    }
}

/// Resolve the schema together with the stock prompt variant. Explicit custom
/// schemas remain authoritative. When a stock paper or generic prompt adapts
/// to folder mode, its unmodified stock schema adapts with it; a user-edited
/// schema is preserved as an intentional custom contract.
pub fn resolve_survey_schema(
    profile_prompt: &str,
    configured_schema: Option<&serde_json::Value>,
    input_mode: &str,
) -> Option<serde_json::Value> {
    let configured_name = stock_survey_name(profile_prompt);
    let effective_name = effective_stock_survey_name(profile_prompt, input_mode);
    if let Some(schema) = configured_schema {
        let should_adapt = input_mode == "folder"
            && matches!(configured_name, Some("orientation" | "orientation_generic"))
            && configured_name
                .and_then(crate::orientation_contract::schema_for_prompt_name)
                .as_ref()
                == Some(schema);
        return if should_adapt {
            Some(crate::orientation_contract::folder_schema())
        } else {
            Some(schema.clone())
        };
    }
    effective_name.and_then(crate::orientation_contract::schema_for_prompt_name)
}

/// Build the survey (orientation map) by calling the LLM and validating that the
/// output is a JSON object, retrying up to MAX_RETRIES times on parse or schema
/// failure.
///
/// The result is returned as raw JSON — profiles may use any survey shape, so
/// no Rust struct is imposed here. A profile may supply an optional lightweight
/// JSON schema; paper-review results are interpreted via `models::paper_view`.
///
/// `prompt_template` is the survey prompt to use; `{input_text}` (and the legacy
/// alias `{paper_text}`) is substituted with the (possibly truncated) extracted
/// input text. Pass `None` to use the default template loaded from
/// prompts/orientation.md (or the user override at ~/.pipeline/prompts/orientation.md).
pub async fn build_orientation_map(
    app: &crate::emit::EventBus,
    extraction: &ExtractionResult,
    prompt_template: Option<&str>,
    output_schema: Option<&serde_json::Value>,
    source_read_root: Option<&str>,
) -> Result<serde_json::Value, String> {
    let terminal_schema = output_schema
        .map(|schema| {
            crate::auto_review::validate_schema_settings(schema)
                .map_err(|error| format!("Invalid orientation schema setting: {error}"))?;
            let resolved =
                crate::auto_review::resolve_schema_catalogs(schema).map_err(|error| {
                    format!("Invalid orientation schema catalog reference: {error}")
                })?;
            crate::pipeline::structured::provider_schema(&resolved)
                .map_err(|error| format!("Invalid orientation schema: {error}"))?;
            Ok::<serde_json::Value, String>(resolved)
        })
        .transpose()?
        .unwrap_or_else(|| serde_json::json!({"type": "object"}));
    let truncated = extraction.text.len() > MAX_PAPER_TEXT;
    let paper_text = if truncated {
        // Find a valid UTF-8 char boundary at or before MAX_PAPER_TEXT
        let mut boundary = MAX_PAPER_TEXT;
        while boundary > 0 && !extraction.text.is_char_boundary(boundary) {
            boundary -= 1;
        }
        &extraction.text[..boundary]
    } else {
        &extraction.text
    };

    let mut quality_notes = extraction.quality_notes.clone();
    if truncated {
        quality_notes.push(format!(
            "Survey built from the first {MAX_PAPER_TEXT} of {} characters; the survey may not cover the end of the input.",
            extraction.text.len()
        ));
    }

    // Resolve the template from caller, then user prompts dir, then compiled default.
    let template_owned;
    let template = match prompt_template {
        Some(t) if !t.trim().is_empty() => t,
        _ => {
            template_owned = crate::prompts::load_prompt("orientation")
                .map_err(|e| format!("Failed to load orientation prompt: {e}"))?;
            template_owned.as_str()
        }
    };
    // Auto Review profiles store a compact template. Expand its catalog
    // placeholders only for the provider call so the workflow editor remains
    // readable and saved profiles never duplicate the manifest catalog.
    let expanded_template;
    let template = if output_schema
        .and_then(|schema| schema.get("x-pipeline-contract"))
        .and_then(serde_json::Value::as_str)
        == Some(crate::auto_review::AUTO_REVIEW_CONTRACT)
    {
        expanded_template = crate::auto_review::expand_orientation_prompt(template);
        expanded_template.as_str()
    } else {
        template
    };
    let base_prompt = substitute_input_text(template, paper_text)?;
    let mut base_prompt =
        crate::auto_review::apply_agent_count_instruction(base_prompt, output_schema)?;
    append_quality_note_instruction(&mut base_prompt, &quality_notes, output_schema)?;

    let mut prompt = base_prompt.clone();
    let mut last_error = String::new();
    let settings = crate::settings::load();
    let provider = settings.orientation_agent().to_string();
    let selection = settings.orientation_model_selection(&provider);
    let resolution = crate::model_catalog::resolve(&provider, &settings, selection.as_ref())
        .await
        .map_err(|error| format!("Could not resolve the orientation model: {error}"))?;
    let effort = settings.orientation_effort(&provider);
    let timeout = (settings.step_timeout_secs / 2).max(60);
    for attempt in 0..=MAX_RETRIES {
        let mut request = OwnedRequest::new(
            app,
            "orientation",
            "Orientation map",
            prompt.clone(),
            timeout,
        );
        request.agent = Some(provider.clone());
        request.command_model = resolution.command_model.clone();
        request.display_model = Some(resolution.resolved_model.clone());
        request.model_policy = Some(resolution.selection.label());
        request.effort = (!effort.trim().is_empty()).then(|| effort.clone());
        request.model_resolved = true;
        request.output_schema = Some(terminal_schema.clone());
        request.settings = std::sync::Arc::new(settings.clone());
        if let Some(root) = source_read_root {
            request.tools = vec!["Read".to_string()];
            request.cwd = Some(root.to_string());
            request.read_dirs = vec![root.to_string()];
        }
        let raw = match execute_text(request).await {
            Ok(raw) => raw,
            Err(error) => {
                // Cancellation aborts immediately, but a transient provider or
                // transport failure on this single required call must not
                // discard the (possibly long) extraction that preceded it —
                // retry the identical prompt like any step call would.
                if crate::commands::is_cancelled()
                    || crate::commands::is_pass_cancelled("orientation")
                {
                    return Err(error);
                }
                if super::provider_error::is_usage_limit_error(&error) {
                    return Err(format!("Orientation map stopped: {error}"));
                }
                last_error = format!("provider call failed: {error}");
                if attempt < MAX_RETRIES {
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!("WARNING: orientation call failed ({error}); retrying")
                        }),
                    );
                }
                continue;
            }
        };
        // Every orientation uses native structured output, including custom
        // surveys that only require an arbitrary top-level object.
        let cleaned = raw.trim();

        match serde_json::from_str::<serde_json::Value>(cleaned) {
            Ok(value) if value.is_object() => {
                if let Some(schema) = output_schema {
                    let validation =
                        crate::pipeline::structured::validate(&terminal_schema, &value)
                            .and_then(|()| {
                                crate::auto_review::validate_contract_for_schema(schema, &value)
                            })
                            .and_then(|()| validate_quality_notes(&value, &quality_notes, schema));
                    if let Err(error) = validation {
                        last_error = format!("orientation did not satisfy its schema: {error}");
                        if attempt < MAX_RETRIES {
                            prompt = format!(
                                "Your previous structured response failed orientation validation: {error}\nCorrect the response so it satisfies the supplied schema and the task requirements.\n\n"
                            );
                            crate::safety::push_str_limited(
                                &mut prompt,
                                &base_prompt,
                                crate::safety::MAX_EXPANDED_PROMPT_BYTES,
                                "Orientation retry prompt",
                            )?;
                        }
                        continue;
                    }
                }
                return Ok(value);
            }
            Ok(_) => {
                last_error = "top-level JSON value is not an object".to_string();
                if attempt < MAX_RETRIES {
                    prompt = "Your previous structured response did not satisfy the supplied object-root schema. Correct it and complete the task again.\n\n".to_string();
                    crate::safety::push_str_limited(
                        &mut prompt,
                        &base_prompt,
                        crate::safety::MAX_EXPANDED_PROMPT_BYTES,
                        "Orientation retry prompt",
                    )?;
                }
            }
            Err(e) => {
                last_error = format!("{e}");
                if attempt < MAX_RETRIES {
                    prompt = format!(
                        "Pipeline could not decode your previous structured response: {e}\nCorrect it so it satisfies the supplied schema and complete the task again.\n\n"
                    );
                    crate::safety::push_str_limited(
                        &mut prompt,
                        &base_prompt,
                        crate::safety::MAX_EXPANDED_PROMPT_BYTES,
                        "Orientation retry prompt",
                    )?;
                }
            }
        }
    }

    Err(format!(
        "Failed to build orientation map after {} attempts. Last error: {last_error}",
        MAX_RETRIES + 1
    ))
}

fn schema_accepts_quality_notes(schema: &serde_json::Value) -> bool {
    let Some(node) = schema.pointer("/properties/extraction_quality_notes") else {
        return false;
    };
    node.get("type").and_then(serde_json::Value::as_str) == Some("array")
        && node
            .get("items")
            .and_then(|items| items.get("type"))
            .and_then(serde_json::Value::as_str)
            .is_none_or(|item_type| item_type == "object")
}

fn quality_note_values(notes: &[String]) -> Vec<serde_json::Value> {
    notes
        .iter()
        .map(|note| {
            serde_json::json!({
                "page_range": "global",
                "description": note,
            })
        })
        .collect()
}

/// Put immutable host diagnostics into the model's contract before generation.
/// The host never mutates a returned artifact after schema validation.
fn append_quality_note_instruction(
    prompt: &mut String,
    notes: &[String],
    schema: Option<&serde_json::Value>,
) -> Result<(), String> {
    if notes.is_empty() {
        return Ok(());
    }
    let values = quality_note_values(notes);
    let instruction = if schema.is_some_and(schema_accepts_quality_notes) {
        format!(
            "\n\nHOST EXTRACTION QUALITY NOTES\nThe host detected the following immutable extraction diagnostics. Include every object exactly once in the top-level `extraction_quality_notes` array. Do not alter their text. You may add other notes.\n{}\n",
            serde_json::to_string_pretty(&values)
                .map_err(|error| format!("Could not serialize extraction diagnostics: {error}"))?
        )
    } else {
        format!(
            "\n\nHOST EXTRACTION QUALITY NOTES\nUse these extraction diagnostics as context, but do not add fields forbidden by the requested schema:\n{}\n",
            serde_json::to_string_pretty(&values)
                .map_err(|error| format!("Could not serialize extraction diagnostics: {error}"))?
        )
    };
    crate::safety::push_str_limited(
        prompt,
        &instruction,
        crate::safety::MAX_EXPANDED_PROMPT_BYTES,
        "Orientation prompt",
    )
}

fn validate_quality_notes(
    survey: &serde_json::Value,
    notes: &[String],
    schema: &serde_json::Value,
) -> Result<(), String> {
    if notes.is_empty() || !schema_accepts_quality_notes(schema) {
        return Ok(());
    }
    let returned = survey
        .get("extraction_quality_notes")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "$.extraction_quality_notes: expected an array".to_string())?;
    for expected in quality_note_values(notes) {
        if !returned.iter().any(|candidate| candidate == &expected) {
            return Err(format!(
                "$.extraction_quality_notes is missing immutable host diagnostic {}",
                expected["description"]
            ));
        }
    }
    Ok(())
}

/// Extract a JSON object from LLM output that may contain preamble text or markdown fences.
/// Uses serde_json's streaming deserializer to find valid JSON — no hand-rolled parsing.
///
/// Prose around the survey may itself contain a small balanced object (an
/// echoed `{}`, a schema fragment), so of all valid top-level objects the
/// largest non-empty one wins — the real survey always contains any object
/// nested inside it, and an empty object is never an acceptable survey.
#[cfg(test)]
fn strip_json_fences(raw: &str) -> String {
    let trimmed = raw.trim();

    // Try each '{' as a candidate JSON start. The streaming deserializer
    // handles all string escapes, unicode, nested structures, etc. correctly.
    // Braces inside an already-accepted candidate are skipped: any object
    // there is strictly smaller than the candidate that contains it.
    let mut best: Option<&str> = None;
    let mut skip_until = 0usize;
    for (i, _) in trimmed.match_indices('{') {
        if i < skip_until {
            continue;
        }
        let candidate = &trimmed[i..];
        let mut stream =
            serde_json::Deserializer::from_str(candidate).into_iter::<serde_json::Value>();
        if let Some(Ok(value)) = stream.next() {
            let end = stream.byte_offset();
            skip_until = i + end;
            if !value.as_object().is_some_and(|object| !object.is_empty()) {
                continue;
            }
            let span = candidate[..end].trim_end();
            if best.is_none_or(|current| span.len() > current.len()) {
                best = Some(span);
            }
        }
    }
    if let Some(span) = best {
        return span.to_string();
    }

    trimmed.to_string()
}

/// Substitute `{input_text}` (and its legacy alias `{paper_text}`) in ONE
/// pass over the template only. The extracted document may itself contain a
/// literal placeholder token (e.g. a paper about prompt templates); rescanning
/// the first substitution's output would expand it a second time, duplicating
/// the document until the prompt byte cap aborts orientation.
fn substitute_input_text(template: &str, paper_text: &str) -> Result<String, String> {
    let limit = crate::safety::MAX_EXPANDED_PROMPT_BYTES;
    let mut out = String::with_capacity(template.len() + paper_text.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        crate::safety::push_str_limited(&mut out, &rest[..start], limit, "Orientation prompt")?;
        let candidate = &rest[start..];
        if let Some(after) = candidate
            .strip_prefix("{input_text}")
            .or_else(|| candidate.strip_prefix("{paper_text}"))
        {
            crate::safety::push_str_limited(&mut out, paper_text, limit, "Orientation prompt")?;
            rest = after;
        } else {
            crate::safety::push_str_limited(&mut out, "{", limit, "Orientation prompt")?;
            rest = &candidate[1..];
        }
    }
    crate::safety::push_str_limited(&mut out, rest, limit, "Orientation prompt")?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_json_fences_clean_json() {
        let input = r#"{"key": "value"}"#;
        assert_eq!(strip_json_fences(input), input);
    }

    #[test]
    fn strip_json_fences_with_markdown() {
        let input = "```json\n{\"key\": \"value\"}\n```";
        assert_eq!(strip_json_fences(input), r#"{"key": "value"}"#);
    }

    #[test]
    fn strip_json_fences_with_preamble() {
        let input = "Here is the JSON output:\n\n{\"metadata\": {\"title\": \"Test\"}}";
        assert_eq!(
            strip_json_fences(input),
            r#"{"metadata": {"title": "Test"}}"#
        );
    }

    #[test]
    fn strip_json_fences_nested_braces() {
        let input = r#"{"a": {"b": {"c": 1}}}"#;
        assert_eq!(strip_json_fences(input), input);
    }

    #[test]
    fn strip_json_fences_braces_in_strings() {
        let input = r#"{"text": "contains {braces} inside"}"#;
        assert_eq!(strip_json_fences(input), input);
    }

    #[test]
    fn strip_json_fences_no_json() {
        let input = "No JSON here at all";
        assert_eq!(strip_json_fences(input), input);
    }

    #[test]
    fn strip_json_fences_trailing_text() {
        let input = "{\"key\": 1}\n\nSome trailing commentary.";
        assert_eq!(strip_json_fences(input), r#"{"key": 1}"#);
    }

    #[test]
    fn strip_json_fences_prefers_the_survey_over_a_preamble_object() {
        // A small balanced object echoed in prose must not hijack the survey.
        let input = "I return {} when unsure. Actual survey:\n{\"metadata\": {\"title\": \"T\"}, \"sections\": []}";
        assert_eq!(
            strip_json_fences(input),
            r#"{"metadata": {"title": "T"}, "sections": []}"#
        );
        // Same when the junk object trails the real one.
        let input =
            "{\"metadata\": {\"title\": \"T\"}, \"sections\": []}\nAs requested: {\"ok\": 1}";
        assert_eq!(
            strip_json_fences(input),
            r#"{"metadata": {"title": "T"}, "sections": []}"#
        );
    }

    #[test]
    fn strip_json_fences_rejects_an_empty_object_as_the_survey() {
        // An empty object is not a survey; returning the raw text makes the
        // caller's parse fail and triggers the retry with the error appended.
        let input = "The result is {} — nothing else.";
        assert_eq!(strip_json_fences(input), input.trim());
    }

    #[test]
    fn strip_json_fences_invalid_then_valid() {
        // First brace starts invalid JSON, second starts valid
        let input = "text with {broken then {\"valid\": true}";
        assert_eq!(strip_json_fences(input), r#"{"valid": true}"#);
    }

    // ── extraction quality notes ──────────────────────────────────

    #[test]
    fn quality_notes_are_required_by_compatible_schemas_without_host_mutation() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {"extraction_quality_notes": {"type": "array"}}
        });
        let notes = ["math broken".to_string()];
        let good = serde_json::json!({
            "extraction_quality_notes": [
                {"page_range": "p. 3", "description": "garbled"},
                {"page_range": "global", "description": "math broken"}
            ]
        });
        assert!(validate_quality_notes(&good, &notes, &schema).is_ok());
        let bad = serde_json::json!({"extraction_quality_notes": []});
        assert!(validate_quality_notes(&bad, &notes, &schema).is_err());
    }

    #[test]
    fn quality_note_instruction_respects_schema_shape() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {"extraction_quality_notes": {"type": "array"}}
        });
        let mut prompt = "Survey".to_string();
        append_quality_note_instruction(
            &mut prompt,
            &["ligatures mangled".to_string()],
            Some(&schema),
        )
        .unwrap();
        assert!(prompt.contains("Include every object exactly once"));
        assert!(prompt.contains("ligatures mangled"));
    }

    // ── resolve_survey_template ────────────────────────────────────
    // Note: load_prompt checks ~/.pipeline/prompts/ overrides first, so
    // these assert against load_prompt output rather than literal text.

    #[test]
    fn resolve_empty_prompt_document_mode_uses_default() {
        assert!(resolve_survey_template("", "document").is_none());
        assert!(resolve_survey_template("  \n", "document").is_none());
    }

    #[test]
    fn resolve_empty_prompt_folder_mode_uses_folder_survey() {
        let resolved = resolve_survey_template("", "folder").expect("folder survey should load");
        let folder = crate::prompts::load_prompt("orientation_folder").unwrap();
        assert_eq!(resolved, folder);
    }

    #[test]
    fn resolve_stock_generic_prompt_swaps_in_folder_mode() {
        let generic = crate::prompts::compiled_default("orientation_generic").unwrap();
        let resolved =
            resolve_survey_template(generic, "folder").expect("folder survey should load");
        let folder = crate::prompts::load_prompt("orientation_folder").unwrap();
        assert_eq!(resolved, folder);
        // ...but stays generic for document inputs.
        assert_eq!(
            resolve_survey_template(generic, "document").as_deref(),
            Some(generic)
        );
    }

    #[test]
    fn resolve_stock_paper_prompt_swaps_in_folder_mode() {
        let paper = crate::prompts::compiled_default("orientation").unwrap();
        let resolved = resolve_survey_template(paper, "folder").expect("folder survey should load");
        let folder = crate::prompts::load_prompt("orientation_folder").unwrap();
        assert_eq!(resolved, folder);
    }

    #[test]
    fn stock_prompt_resolution_keeps_schema_in_lockstep() {
        assert_eq!(
            resolve_survey_schema("", None, "document"),
            Some(crate::orientation_contract::paper_schema())
        );
        assert_eq!(
            resolve_survey_schema("", None, "folder"),
            Some(crate::orientation_contract::folder_schema())
        );

        let generic_prompt = crate::prompts::compiled_default("orientation_generic").unwrap();
        let generic_schema = crate::orientation_contract::generic_schema();
        assert_eq!(
            resolve_survey_schema(generic_prompt, Some(&generic_schema), "folder"),
            Some(crate::orientation_contract::folder_schema())
        );
    }

    #[test]
    fn custom_prompt_and_schema_remain_authoritative() {
        let prompt = "Survey this input using the supplied contract. {input_text}";
        let schema = serde_json::json!({
            "type": "object",
            "required": ["custom"],
            "properties": {"custom": {"type": "string"}}
        });
        assert_eq!(resolve_survey_schema(prompt, None, "folder"), None);
        assert_eq!(
            resolve_survey_schema(prompt, Some(&schema), "folder"),
            Some(schema)
        );
    }

    #[test]
    fn resolve_custom_prompt_wins_in_any_mode() {
        let custom = "Survey this thing my way. {input_text}";
        assert_eq!(
            resolve_survey_template(custom, "folder").as_deref(),
            Some(custom)
        );
        assert_eq!(
            resolve_survey_template(custom, "document").as_deref(),
            Some(custom)
        );
    }

    #[test]
    fn document_text_containing_placeholder_tokens_is_not_rescanned() {
        let paper = "This paper studies {paper_text} and {input_text} as literals.";
        let prompt = substitute_input_text("Survey:\n{input_text}\nEnd.", paper).unwrap();
        assert_eq!(prompt, format!("Survey:\n{paper}\nEnd."));
        // Both aliases resolve, each exactly once, from the template only.
        let both = substitute_input_text("A {input_text} B {paper_text} C", "X").unwrap();
        assert_eq!(both, "A X B X C");
    }
}
