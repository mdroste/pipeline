use super::call::{execute_text, OwnedRequest};
use crate::models::ExtractionResult;

mod document_type;
mod index;
mod planning;
mod preparation;
pub(crate) use preparation::{
    document_preparation_snapshot, PreparedReview, SupportingInput, PREPARATION_PATH,
    REGISTRY_PATH, REVIEW_PLAN_PATH,
};

mod sampling;

use index::ManuscriptIndex;
pub(crate) use index::{
    FOLLOW_UP_BYTES, PATH as ORIENTATION_INDEX_PATH, PROMPT_BYTES as INDEX_PROMPT_BYTES,
};
pub use sampling::{orientation_sample, OrientationSample, OrientationSampleRange};
pub(crate) use sampling::{
    INDEXED_ORIENTATION_NOTICE, ORIENTATION_EXCERPTS_NOTICE, ORIENTATION_SAMPLING_PATH,
};

const MAX_RETRIES: usize = 2;
/// Below every stock schema's 50-item cap, so host notes never fill the array.
const MAX_HOST_QUALITY_NOTES: usize = 40;

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
    } else if Some(trimmed) == crate::prompts::compiled_default("orientation_grant").map(str::trim)
    {
        Some("orientation_grant")
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
    author_note: Option<&str>,
) -> Result<serde_json::Value, String> {
    if output_schema
        .and_then(document_type::expected_document)
        .is_some()
    {
        return build_paper_orientation(
            app,
            extraction,
            None,
            prompt_template,
            output_schema,
            source_read_root,
            author_note,
            None,
            &[],
        )
        .await
        .map(|result| result.orientation);
    }
    build_indexed_orientation_map(
        app,
        extraction,
        prompt_template,
        output_schema,
        source_read_root,
        author_note,
        None,
    )
    .await
}

pub(crate) fn uses_manuscript_index(schema: Option<&serde_json::Value>) -> bool {
    schema.and_then(document_type::expected_document).is_some()
}

/// Run Paper Review orientation against an immutable extraction and its local
/// structure, returning the saved index alongside the downstream map.
#[allow(clippy::too_many_arguments)]
pub async fn build_paper_orientation(
    app: &crate::emit::EventBus,
    extraction: &ExtractionResult,
    bundle: Option<&crate::document_bundle::DocumentBundle>,
    prompt_template: Option<&str>,
    output_schema: Option<&serde_json::Value>,
    source_read_root: Option<&str>,
    author_note: Option<&str>,
    artifact_root: Option<&std::path::Path>,
    supporting_inputs: &[SupportingInput],
) -> Result<PreparedReview, String> {
    if let Some(schema) =
        output_schema.filter(|schema| document_type::expected_document(schema).is_some())
    {
        return planning::build(
            app,
            extraction,
            bundle,
            prompt_template,
            schema,
            source_read_root,
            author_note,
            artifact_root,
            supporting_inputs,
        )
        .await;
    }
    let mut index = uses_manuscript_index(output_schema)
        .then(|| ManuscriptIndex::new(&extraction.text, bundle, artifact_root))
        .flatten();
    let map = build_indexed_orientation_map(
        app,
        extraction,
        prompt_template,
        output_schema,
        source_read_root,
        author_note,
        index.as_mut(),
    )
    .await?;
    Ok(PreparedReview {
        orientation: map,
        index: index.map(|index| index.serialize()).transpose()?,
        preparation: None,
        registry: None,
    })
}

async fn build_indexed_orientation_map(
    app: &crate::emit::EventBus,
    extraction: &ExtractionResult,
    prompt_template: Option<&str>,
    output_schema: Option<&serde_json::Value>,
    source_read_root: Option<&str>,
    author_note: Option<&str>,
    mut manuscript: Option<&mut ManuscriptIndex<'_>>,
) -> Result<serde_json::Value, String> {
    let mut terminal_schema = output_schema
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
    if manuscript.is_some() {
        ManuscriptIndex::extend_schema(&mut terminal_schema);
    }
    let expected_document = document_type::expected_document(&terminal_schema);
    let response_schema = match expected_document {
        Some(_) => document_type::response_schema(terminal_schema.clone()),
        None => terminal_schema.clone(),
    };
    let follow_up_schema = manuscript
        .is_some()
        .then(|| index::follow_up_schema(&terminal_schema));
    if let Some(follow_up) = &follow_up_schema {
        for schema in [&response_schema, follow_up] {
            crate::pipeline::structured::provider_schema(schema)
                .map_err(|error| format!("Invalid indexed orientation schema: {error}"))?;
        }
    }
    let sample = orientation_sample(&extraction.text);
    let paper_text = sample.text.as_str();

    // The model echoes a bounded digest; the run's quality ledger keeps every
    // note because it reads `extraction.quality_notes` directly.
    let note_limit = host_note_limit(output_schema);
    let mut quality_notes = bounded_host_notes(
        &extraction.quality_notes,
        note_limit.saturating_sub(usize::from(sample.truncated)),
    );
    if sample.truncated && note_limit > 0 {
        quality_notes.push(
            if manuscript.is_some() {
                INDEXED_ORIENTATION_NOTICE
            } else {
                ORIENTATION_EXCERPTS_NOTICE
            }
            .to_string(),
        );
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
    if let Some(index) = manuscript.as_deref() {
        crate::safety::push_str_limited(
            &mut base_prompt,
            &index.prompt(),
            crate::safety::MAX_EXPANDED_PROMPT_BYTES,
            "Orientation index",
        )?;
    }
    append_quality_note_instruction(&mut base_prompt, &quality_notes, output_schema)?;
    if let Some(expected) = expected_document {
        let mut instruction = document_type::instruction(expected);
        crate::safety::push_str_limited(
            &mut instruction,
            &base_prompt,
            crate::safety::MAX_EXPANDED_PROMPT_BYTES,
            "Orientation prompt",
        )?;
        base_prompt = instruction;
    }

    let mut prompt = base_prompt.clone();
    let mut last_error = String::new();
    let settings = crate::settings::load();
    let provider = settings.orientation_agent().to_string();
    let selection = settings.orientation_model_selection(&provider);
    let resolution = crate::model_catalog::resolve(&provider, &settings, selection.as_ref())
        .await
        .map_err(|error| format!("Could not resolve the orientation model: {error}"))?;
    let effort = if manuscript.is_some() {
        let effort = settings.paper_orientation_effort(&provider);
        if resolution.supported_efforts.is_empty() || resolution.supported_efforts.contains(&effort)
        {
            effort
        } else {
            settings.orientation_effort(&provider)
        }
    } else {
        settings.orientation_effort(&provider)
    };
    let timeout = (settings.step_timeout_secs / 2).max(60);
    let mut initial = None;
    let mut targeted_files = None;
    for attempt in 0..=MAX_RETRIES + 1 {
        let follow_up = initial.is_some();
        if attempt > MAX_RETRIES && !follow_up {
            break;
        }
        let mut request = OwnedRequest::new(
            app,
            "orientation",
            if follow_up {
                "Document evidence lookup"
            } else {
                "Document bundle preparation"
            },
            prompt.clone(),
            timeout,
        );
        request.author_note = author_note.map(str::to_string);
        request.agent = Some(provider.clone());
        request.command_model = resolution.command_model.clone();
        request.display_model = Some(resolution.resolved_model.clone());
        request.model_policy = Some(resolution.selection.label());
        request.effort = (!effort.trim().is_empty()).then(|| effort.clone());
        request.model_resolved = true;
        request.output_schema = Some(if follow_up {
            follow_up_schema
                .as_ref()
                .expect("indexed follow-up schema")
                .clone()
        } else {
            response_schema.clone()
        });
        request.settings = std::sync::Arc::new(settings.clone());
        if let Some(root) = source_read_root.filter(|_| !follow_up) {
            request.tools = vec!["Read".to_string()];
            request.cwd = Some(root.to_string());
            request.read_dirs = vec![root.to_string()];
        }
        if let Some(root) = targeted_files.as_ref().and_then(|dir: &tempfile::TempDir| {
            super::claude::normalize_cli_root(&dir.path().to_string_lossy())
        }) {
            request.tools = vec!["Read".to_string()];
            request.cwd = Some(root.clone());
            request.read_dirs = vec![root];
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
                if follow_up {
                    last_error = error;
                    break;
                }
                if super::provider_error::is_usage_limit_error(&error)
                    || super::provider_error::is_non_retryable_error(&error)
                {
                    return Err(format!("Document bundle preparation stopped: {error}"));
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
                let response = match (&initial, expected_document) {
                    (Some(initial), _) => index::apply_follow_up(
                        initial,
                        value,
                        follow_up_schema.as_ref().expect("indexed follow-up schema"),
                        &quality_note_values(&quality_notes),
                    )
                    .map(|mut value| {
                        manuscript
                            .as_deref()
                            .expect("indexed follow-up")
                            .retain_valid_revisions(&mut value, initial);
                        document_type::Response::Oriented(value)
                    }),
                    (None, Some(expected)) => document_type::decode(value, expected),
                    (None, None) => Ok(document_type::Response::Oriented(value)),
                };
                let validation = response.and_then(|response| match response {
                    document_type::Response::Rejected(_) => Ok(response),
                    document_type::Response::Oriented(mut value) => {
                        // Strict transports return null for absent optional fields.
                        crate::pipeline::structured::strip_optional_nulls(
                            &terminal_schema,
                            &mut value,
                        );
                        if let Some(schema) = output_schema {
                            crate::pipeline::structured::validate(&terminal_schema, &value)?;
                            crate::auto_review::validate_contract_for_schema(schema, &value)?;
                            validate_quality_notes(&value, &quality_notes, schema)?;
                        }
                        if let Some(index) = manuscript.as_deref() {
                            index.validate(&value, !follow_up)?;
                        }
                        Ok(document_type::Response::Oriented(value))
                    }
                });
                match validation {
                    Ok(document_type::Response::Oriented(mut value)) => {
                        if let Some(index) = manuscript.as_deref_mut() {
                            if follow_up {
                                index.follow_up_completed = true;
                            } else if let Some((next_prompt, files)) =
                                index.follow_up_prompt(&value)
                            {
                                prompt = next_prompt;
                                targeted_files = files;
                                if let Some(notes) = value.as_object_mut() {
                                    notes.remove("follow_up");
                                }
                                initial = Some(value);
                                continue;
                            }
                        }
                        if manuscript.is_some() {
                            value
                                .as_object_mut()
                                .expect("validated map")
                                .remove("follow_up");
                            value["orientation_indexed"] = serde_json::Value::Bool(true);
                        }
                        return Ok(value);
                    }
                    Ok(document_type::Response::Rejected(message)) => {
                        if follow_up {
                            last_error = message;
                            break;
                        }
                        super::logging::emit(app, message.clone());
                        return Err(message);
                    }
                    Err(error) => {
                        last_error = format!("orientation did not satisfy its schema: {error}");
                        if follow_up {
                            break;
                        }
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
                    }
                }
            }
            Ok(_) => {
                last_error = "top-level JSON value is not an object".to_string();
                if follow_up {
                    break;
                }
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
                if follow_up {
                    break;
                }
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

    if let Some(mut initial) = initial {
        if let Some(index) = manuscript {
            index.follow_up_incomplete = true;
        }
        initial["orientation_indexed"] = serde_json::Value::Bool(true);
        initial["orientation_follow_up_incomplete"] = serde_json::Value::Bool(true);
        super::logging::emit(app, format!("Orientation kept its initial plan. The requested follow-up could not be completed: {last_error}"));
        return Ok(initial);
    }

    Err(format!(
        "Failed to build document overview after {} attempts. Last error: {last_error}",
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

/// The most host notes one orientation must repeat. A schema's own `maxItems`
/// lowers this further so the model always has a satisfiable contract.
fn host_note_limit(schema: Option<&serde_json::Value>) -> usize {
    let declared = schema
        .filter(|schema| schema_accepts_quality_notes(schema))
        .and_then(|schema| schema.pointer("/properties/extraction_quality_notes/maxItems"))
        .and_then(serde_json::Value::as_u64)
        .map(|max_items| usize::try_from(max_items).unwrap_or(usize::MAX));
    match declared {
        // Leave one slot for a note of the model's own when there is room.
        Some(max_items) if max_items > 1 => MAX_HOST_QUALITY_NOTES.min(max_items - 1),
        Some(max_items) => max_items,
        None => MAX_HOST_QUALITY_NOTES,
    }
}

fn page_note(note: &str) -> Option<(u32, &str)> {
    let (page, detail) = note.strip_prefix("Page ")?.split_once(": ")?;
    Some((page.parse().ok()?, detail))
}

/// Page notes that differ only in their numbers describe the same problem.
fn page_note_shape(detail: &str) -> String {
    let mut shape = String::with_capacity(detail.len());
    for character in detail.chars() {
        if !character.is_ascii_digit() {
            shape.push(character);
        } else if !shape.ends_with('#') {
            shape.push('#');
        }
    }
    shape
}

fn page_ranges(pages: &[u32]) -> String {
    let mut sorted = pages.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for page in sorted {
        match ranges.last_mut() {
            Some((_, end)) if end.saturating_add(1) == page => *end = page,
            _ => ranges.push((page, page)),
        }
    }
    ranges
        .iter()
        .map(|(start, end)| {
            if start == end {
                start.to_string()
            } else {
                format!("{start}-{end}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Reduce extraction notes to at most `limit` for the orientation contract:
/// same-shaped page notes become one page-range note, then any excess becomes
/// one count. Notes under the limit pass through unchanged.
fn bounded_host_notes(notes: &[String], limit: usize) -> Vec<String> {
    if notes.len() <= limit {
        return notes.to_vec();
    }
    // Each entry is a standalone note, or the pages of one shape in document
    // order; the first note of the shape stands for the group.
    let mut digest: Vec<(&str, Vec<u32>, String)> = Vec::new();
    for note in notes {
        let Some((page, detail)) = page_note(note) else {
            digest.push((note, Vec::new(), String::new()));
            continue;
        };
        let shape = page_note_shape(detail);
        match digest
            .iter_mut()
            .find(|(_, pages, known)| !pages.is_empty() && *known == shape)
        {
            Some((_, pages, _)) => pages.push(page),
            None => digest.push((note, vec![page], shape)),
        }
    }
    let mut bounded: Vec<String> = digest
        .into_iter()
        .map(|(note, pages, _)| match (pages.len(), page_note(note)) {
            (2.., Some((_, detail))) => format!(
                "Pages {}: {} similar notes, for example: {detail}",
                page_ranges(&pages),
                pages.len(),
            ),
            _ => note.to_string(),
        })
        .collect();
    if bounded.len() > limit {
        let kept = limit.saturating_sub(1);
        let omitted = bounded.len() - kept;
        bounded.truncate(kept);
        if limit > 0 {
            bounded.push(format!(
                "{omitted} further extraction notes are recorded with this run's quality notes."
            ));
        }
    }
    bounded
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
            "\n\nHOST EXTRACTION QUALITY NOTES\nThe host detected the following immutable extraction diagnostics. Include every object exactly once in the orientation map's `extraction_quality_notes` array. Do not alter their text. You may add other notes.\n{}\n",
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
mod tests;
