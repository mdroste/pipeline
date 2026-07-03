use super::claude::{call_llm, LlmOverrides};
use crate::models::{ExtractionQualityNote, ExtractionResult, OrientationMap};
use tauri::AppHandle;

const MAX_PAPER_TEXT: usize = 250_000;
const MAX_RETRIES: usize = 2;

/// Build the orientation map by calling Claude and validating the JSON output, retrying up to MAX_RETRIES times on parse failure.
///
/// `prompt_template` is the orientation prompt to use; `{paper_text}` is substituted
/// with the (possibly truncated) extracted paper text. Pass `None` to use the default
/// template loaded from prompts/orientation.md (or the user override at
/// ~/.pipeline/prompts/orientation.md).
pub async fn build_orientation_map(
    app: &AppHandle,
    extraction: &ExtractionResult,
    prompt_template: Option<&str>,
) -> Result<OrientationMap, String> {
    let paper_text = if extraction.text.len() > MAX_PAPER_TEXT {
        // Find a valid UTF-8 char boundary at or before MAX_PAPER_TEXT
        let mut boundary = MAX_PAPER_TEXT;
        while boundary > 0 && !extraction.text.is_char_boundary(boundary) {
            boundary -= 1;
        }
        &extraction.text[..boundary]
    } else {
        &extraction.text
    };

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
    let base_prompt = template.replace("{paper_text}", paper_text);

    let mut prompt = base_prompt.clone();
    let mut last_error = String::new();

    for attempt in 0..=MAX_RETRIES {
        let timeout = (crate::settings::load().step_timeout_secs / 2).max(60);
        let raw = call_llm(app, &prompt, &["Read"], None, "text", timeout, "Orientation map", None, None, &[], &LlmOverrides::default()).await?;
        let cleaned = strip_json_fences(&raw);

        match serde_json::from_str::<OrientationMap>(&cleaned) {
            Ok(mut omap) => {
                for note in &extraction.quality_notes {
                    omap.extraction_quality_notes.push(ExtractionQualityNote {
                        page_range: "global".to_string(),
                        description: note.clone(),
                    });
                }
                return Ok(omap);
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
}
