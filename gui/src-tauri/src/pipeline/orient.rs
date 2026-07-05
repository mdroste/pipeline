use super::claude::{call_llm, LlmOverrides};
use crate::models::ExtractionResult;
use tauri::AppHandle;

const MAX_PAPER_TEXT: usize = 250_000;
const MAX_RETRIES: usize = 2;

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
    let is_stock = trimmed.is_empty()
        || Some(trimmed) == crate::prompts::compiled_default("orientation").map(str::trim)
        || Some(trimmed) == crate::prompts::compiled_default("orientation_generic").map(str::trim);
    if input_mode == "folder" && is_stock {
        return crate::prompts::load_prompt("orientation_folder").ok();
    }
    if trimmed.is_empty() {
        None
    } else {
        Some(profile_prompt.to_string())
    }
}

/// Build the survey (orientation map) by calling the LLM and validating that the
/// output is a JSON object, retrying up to MAX_RETRIES times on parse failure.
///
/// The result is returned as raw JSON — profiles may use any survey schema, so
/// no struct validation happens here. The paper-review profiles produce the
/// paper schema, which callers interpret via `models::paper_view`.
///
/// `prompt_template` is the survey prompt to use; `{input_text}` (and the legacy
/// alias `{paper_text}`) is substituted with the (possibly truncated) extracted
/// input text. Pass `None` to use the default template loaded from
/// prompts/orientation.md (or the user override at ~/.pipeline/prompts/orientation.md).
pub async fn build_orientation_map(
    app: &AppHandle,
    extraction: &ExtractionResult,
    prompt_template: Option<&str>,
) -> Result<serde_json::Value, String> {
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
    let base_prompt = template
        .replace("{input_text}", paper_text)
        .replace("{paper_text}", paper_text);

    let mut prompt = base_prompt.clone();
    let mut last_error = String::new();

    for attempt in 0..=MAX_RETRIES {
        let timeout = (crate::settings::load().step_timeout_secs / 2).max(60);
        let raw = call_llm(app, &prompt, &["Read"], None, "text", timeout, "Orientation map", None, None, &[], &LlmOverrides::default()).await?;
        let cleaned = strip_json_fences(&raw);

        match serde_json::from_str::<serde_json::Value>(&cleaned) {
            Ok(mut value) if value.is_object() => {
                append_quality_notes(&mut value, &quality_notes);
                return Ok(value);
            }
            Ok(_) => {
                last_error = "top-level JSON value is not an object".to_string();
                if attempt < MAX_RETRIES {
                    prompt = format!(
                        "Your previous response was not a JSON object. \
                         Please try again. Return ONLY a single JSON object, no markdown fences.\n\n{base_prompt}"
                    );
                }
            }
            Err(e) => {
                last_error = format!("{e}");
                if attempt < MAX_RETRIES {
                    prompt = format!(
                        "Your previous response was not valid JSON. Error: {e}\n\
                         Please try again. Return ONLY valid JSON, no markdown fences.\n\n{base_prompt}"
                    );
                }
            }
        }
    }

    Err(format!(
        "Failed to build orientation map after {} attempts. Last error: {last_error}",
        MAX_RETRIES + 1
    ))
}

/// Append extraction-quality warnings to the survey's `extraction_quality_notes`
/// array (creating it if the survey schema doesn't have one), so garbled-input
/// warnings reach the steps regardless of survey shape.
fn append_quality_notes(survey: &mut serde_json::Value, notes: &[String]) {
    if notes.is_empty() {
        return;
    }
    let Some(obj) = survey.as_object_mut() else { return };
    let entry = obj
        .entry("extraction_quality_notes")
        .or_insert_with(|| serde_json::Value::Array(vec![]));
    if let Some(arr) = entry.as_array_mut() {
        for note in notes {
            arr.push(serde_json::json!({
                "page_range": "global",
                "description": note,
            }));
        }
    }
}

/// Extract a JSON object from LLM output that may contain preamble text or markdown fences.
/// Uses serde_json's streaming deserializer to find valid JSON — no hand-rolled parsing.
fn strip_json_fences(raw: &str) -> String {
    let trimmed = raw.trim();

    // Try each '{' as a candidate JSON start. The streaming deserializer
    // handles all string escapes, unicode, nested structures, etc. correctly.
    for (i, _) in trimmed.match_indices('{') {
        let candidate = &trimmed[i..];
        let mut stream =
            serde_json::Deserializer::from_str(candidate).into_iter::<serde_json::Value>();
        if let Some(Ok(_)) = stream.next() {
            let end = stream.byte_offset();
            return candidate[..end].trim_end().to_string();
        }
    }

    trimmed.to_string()
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
    fn strip_json_fences_invalid_then_valid() {
        // First brace starts invalid JSON, second starts valid
        let input = "text with {broken then {\"valid\": true}";
        assert_eq!(strip_json_fences(input), r#"{"valid": true}"#);
    }

    // ── append_quality_notes ───────────────────────────────────────

    #[test]
    fn quality_notes_appended_to_existing_array() {
        let mut survey = serde_json::json!({
            "extraction_quality_notes": [{"page_range": "p. 3", "description": "garbled"}]
        });
        append_quality_notes(&mut survey, &["math broken".to_string()]);
        let notes = survey["extraction_quality_notes"].as_array().unwrap();
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[1]["page_range"], "global");
        assert_eq!(notes[1]["description"], "math broken");
    }

    #[test]
    fn quality_notes_create_key_on_custom_survey() {
        // Custom (non-paper) survey schemas still receive extraction warnings.
        let mut survey = serde_json::json!({"overview": "a codebase"});
        append_quality_notes(&mut survey, &["ligatures mangled".to_string()]);
        let notes = survey["extraction_quality_notes"].as_array().unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(survey["overview"], "a codebase");
    }

    #[test]
    fn quality_notes_noop_when_empty() {
        let mut survey = serde_json::json!({"overview": "x"});
        append_quality_notes(&mut survey, &[]);
        assert!(survey.get("extraction_quality_notes").is_none());
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
        let resolved =
            resolve_survey_template(paper, "folder").expect("folder survey should load");
        let folder = crate::prompts::load_prompt("orientation_folder").unwrap();
        assert_eq!(resolved, folder);
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
}
