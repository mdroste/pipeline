//! Prompt construction, bounded substitutions and evidence instructions.
use crate::models::StepOutput;
use crate::output::{structured_output_format, text_artifact_output_format};
use crate::pipeline_config::StepConfig;

/// Build the OUTPUT FORMAT block appended to every step prompt.
pub(super) fn output_format_block(
    write_dir: Option<&str>,
    _report_nonce: &str,
    output_schema: Option<&serde_json::Value>,
) -> String {
    if output_schema.is_some() {
        structured_output_format(write_dir)
    } else {
        text_artifact_output_format(write_dir, "Markdown report")
    }
}

pub(super) fn append_shared_context_note(
    mut prompt: String,
    includes_primary_text: bool,
    includes_survey: bool,
) -> Result<String, String> {
    let material = match (includes_primary_text, includes_survey) {
        (true, true) => "extracted input text and survey",
        (true, false) => "extracted input text",
        (false, true) => "survey",
        (false, false) => "selected shared material",
    };
    crate::safety::push_str_limited(
        &mut prompt,
        &format!(
            "\n\nSHARED CONTEXT NOTE:\n\
             The selected {material} is already present in the shared context for this call. \
             Do not read its staged file again. You may still use the other artifacts listed \
             in this step's artifact context."
        ),
        crate::safety::MAX_EXPANDED_PROMPT_BYTES,
        "Shared-context prompt",
    )?;
    Ok(prompt)
}

pub(super) fn append_evidence_retrieval_guidance(
    mut prompt: String,
    tools: &[String],
) -> Result<String, String> {
    let can_read_text = tools.iter().any(|tool| tool == "Read");
    let can_read_visuals = tools.iter().any(|tool| tool == "ReadDocumentAsset");
    let can_search_web = tools.iter().any(|tool| tool == "WebSearch");
    if !can_read_text && !can_read_visuals && !can_search_web {
        return Ok(prompt);
    }

    crate::safety::push_str_limited(
        &mut prompt,
        "\n\nEVIDENCE RETRIEVAL:\n\
         For evidence not already present in shared context, identify the independent items you \
         need before calling tools.",
        crate::safety::MAX_EXPANDED_PROMPT_BYTES,
        "Evidence-retrieval prompt",
    )?;
    if can_read_text {
        crate::safety::push_str_limited(
            &mut prompt,
            "\nWhen multiple bounded text ranges are needed, prefer ReadTextBatch when it is \
             offered; otherwise issue independent bounded reads together in one tool turn when \
             supported.",
            crate::safety::MAX_EXPANDED_PROMPT_BYTES,
            "Evidence-retrieval prompt",
        )?;
    }
    if can_read_visuals {
        crate::safety::push_str_limited(
            &mut prompt,
            "\nWhen multiple visual assets are needed, prefer ReadDocumentAssetsBatch when it is \
             offered; otherwise inspect independent images together in one tool turn when \
             supported.",
            crate::safety::MAX_EXPANDED_PROMPT_BYTES,
            "Evidence-retrieval prompt",
        )?;
    }
    if can_search_web {
        crate::safety::push_str_limited(
            &mut prompt,
            "\nForm the complete set of independent web queries first and issue them together in \
             one tool turn when supported.",
            crate::safety::MAX_EXPANDED_PROMPT_BYTES,
            "Evidence-retrieval prompt",
        )?;
    }
    crate::safety::push_str_limited(
        &mut prompt,
        "\nIf batching or parallel calls are unavailable, or any item is missing, truncated, or \
         fails, continue sequentially until every item required by the review instructions has \
         been checked. Batching is only an efficiency optimization: never omit evidence.",
        crate::safety::MAX_EXPANDED_PROMPT_BYTES,
        "Evidence-retrieval prompt",
    )?;
    if can_read_visuals {
        crate::safety::push_str_limited(
            &mut prompt,
            " Never substitute extracted text for a required visual inspection.",
            crate::safety::MAX_EXPANDED_PROMPT_BYTES,
            "Evidence-retrieval prompt",
        )?;
    }
    Ok(prompt)
}

/// Derive filesystem tools from the resolved artifact view. Profiles declare
/// optional external capabilities; they never grant Read or Write directly.
pub(super) fn tools_with_write(
    step_tools: &[String],
    has_readable_artifacts: bool,
    has_visuals: bool,
    write_dir: Option<&str>,
) -> Vec<String> {
    let mut tools: Vec<String> = step_tools
        .iter()
        .filter(|tool| !matches!(tool.as_str(), "Read" | "Write"))
        .cloned()
        .collect();
    if has_readable_artifacts {
        tools.push("Read".to_string());
    }
    if has_visuals {
        tools.push("ReadDocumentAsset".to_string());
    }
    if write_dir.is_some() && !tools.iter().any(|t| t == "Write") {
        tools.push("Write".to_string());
    }
    tools
}

// ── Parallel execution ──────────────────────────────────────────────

/// Build the prompt for a parallel step (referee-style).
#[allow(clippy::too_many_arguments)]
pub(super) fn build_parallel_prompt(
    step: &StepConfig,
    paper_type: &str,
    orientation_path: &str,
    survey_hint: &str,
    paper_text_path: &str,
    document_bundle_path: &str,
    source_path: &str,
    template: &str,
    output_format: &str,
    artifact_root: Option<&str>,
) -> Result<String, String> {
    let normalized_path = paper_text_path.replace('\\', "/");
    let normalized_source = source_path.replace('\\', "/");

    let is_pdf = normalized_source.to_ascii_lowercase().ends_with(".pdf");
    let source_hint = if normalized_source.is_empty() {
        "The original source is not available to this step.".to_string()
    } else if is_pdf {
        format!(
            "The original PDF is at: {normalized_source}\n\
             When the orientation map lists a figure or table with a page number, you can read that page of the PDF to inspect the visual content."
        )
    } else {
        let source = std::path::Path::new(&normalized_source);
        let source_root = if source.is_dir() {
            normalized_source.clone()
        } else {
            source
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| normalized_source.clone())
        };
        format!(
            "The selected source context is at: {source_root}\n\
             For LaTeX inputs, this private view contains the main source and the bounded local dependencies discovered from it (such as included sections, bibliography files, and referenced figures)."
        )
    };
    let normalized_bundle = document_bundle_path.replace('\\', "/");
    let artifact_root = artifact_root.map(|directory| directory.replace('\\', "/"));
    let figure_hint = if normalized_bundle.is_empty() {
        source_hint
    } else {
        format!(
            "A compact DocumentBundle index (JSON) is at: {normalized_bundle}\n\
             Use the readable document for prose. Use this index selectively to locate equations, tables, figures, page references, provenance, and asset IDs; do not read it wholesale.\n\
             Asset rel_path values are relative to the run directory: {}\n\
             Inspect images with ReadDocumentAsset on direct APIs or the provider's native Read tool on CLI transports.\n{source_hint}",
            artifact_root
                .as_deref()
                .unwrap_or("(run artifact root unavailable)")
        )
    };

    let orientation_block = if orientation_path.is_empty() {
        String::new()
    } else {
        let normalized_orient = orientation_path.replace('\\', "/");
        format!("The orientation map (JSON) is at: {normalized_orient}\n{survey_hint}")
    };

    let limit = crate::safety::MAX_EXPANDED_PROMPT_BYTES;
    let mut expanded = template.to_string();
    for (needle, value) in [
        ("{step_prompt}", step.prompt.as_str()),
        ("{paper_type}", paper_type),
        ("{orientation}", orientation_block.as_str()),
        ("{paper_path}", normalized_path.as_str()),
        ("{input_path}", normalized_path.as_str()),
        ("{document_bundle}", normalized_bundle.as_str()),
        ("{figure_hint}", figure_hint.as_str()),
    ] {
        expanded =
            crate::safety::replace_all_limited(&expanded, needle, value, limit, "Parallel prompt")?;
    }

    // Custom templates created before `{output_format}` existed still need
    // the current nonce contract. Append it when there is no placeholder so a
    // stale static marker instruction cannot bypass validation.
    if expanded.contains("{output_format}") {
        crate::safety::replace_all_limited(
            &expanded,
            "{output_format}",
            output_format,
            limit,
            "Parallel prompt",
        )
    } else if output_format.is_empty() {
        Ok(expanded)
    } else {
        crate::safety::push_str_limited(&mut expanded, "\n\n", limit, "Parallel prompt")?;
        crate::safety::push_str_limited(&mut expanded, output_format, limit, "Parallel prompt")?;
        Ok(expanded)
    }
}

// ── Sequential execution ────────────────────────────────────────────

/// Expand template placeholders in a sequential step's prompt.
pub(super) fn expand_template(
    template: &str,
    orientation_path: &str,
    survey_hint: &str,
    prior_outputs: &[StepOutput],
    paper_text_path: &str,
    document_bundle_path: &str,
    source_path: &str,
) -> Result<String, String> {
    let orientation_ref = if orientation_path.is_empty() {
        String::new()
    } else {
        let normalized_orient = orientation_path.replace('\\', "/");
        format!("The orientation map (JSON) is at: {normalized_orient}\n{survey_hint}")
    };

    let limit = crate::safety::MAX_EXPANDED_PROMPT_BYTES;
    let mut prior_text = String::new();
    for (index, output) in prior_outputs
        .iter()
        .filter(|output| !output.skipped)
        .enumerate()
    {
        if index > 0 {
            crate::safety::push_str_limited(
                &mut prior_text,
                "\n\n---\n\n",
                limit,
                "Prior-step context",
            )?;
        }
        crate::safety::push_str_limited(&mut prior_text, "## ", limit, "Prior-step context")?;
        crate::safety::push_str_limited(
            &mut prior_text,
            &output.step_label,
            limit,
            "Prior-step context",
        )?;
        crate::safety::push_str_limited(&mut prior_text, "\n\n", limit, "Prior-step context")?;
        crate::safety::push_str_limited(
            &mut prior_text,
            &output.raw_text,
            limit,
            "Prior-step context",
        )?;
    }

    let last_output_text = prior_outputs
        .iter()
        .rev()
        .find(|output| !output.skipped)
        .map(|o| o.raw_text.as_str())
        .unwrap_or("(not yet generated)");

    // Substitute in ONE pass over the template only. Inserted values include
    // model-produced step outputs, which may quote placeholder-shaped text
    // from the reviewed document; rescanning them (as sequential
    // replace_all_limited calls did) would let that text pull in other step
    // reports, inject artifact paths, or balloon the prompt past the byte cap.
    // `{step:<id>}` references resolve exact composite ids ("technical/claude")
    // or base ids (joining all agents' outputs); unknown ids become a
    // parenthesized notice so the prompt stays readable.
    let replacements: [(&str, &str); 9] = [
        ("{orientation}", orientation_ref.as_str()),
        ("{prior_outputs}", prior_text.as_str()),
        ("{referee_reports}", prior_text.as_str()),
        ("{last_output}", last_output_text),
        ("{editor_synthesis}", last_output_text),
        ("{paper_path}", paper_text_path),
        ("{input_path}", paper_text_path),
        ("{document_bundle}", document_bundle_path),
        ("{source_path}", source_path),
    ];
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        crate::safety::push_str_limited(&mut out, &rest[..start], limit, "Sequential prompt")?;
        let candidate = &rest[start..];
        if let Some((needle, value)) = replacements
            .iter()
            .find(|(needle, _)| candidate.starts_with(needle))
        {
            crate::safety::push_str_limited(&mut out, value, limit, "Sequential prompt")?;
            rest = &candidate[needle.len()..];
        } else if let Some((after_open, end_rel)) = candidate
            .strip_prefix("{step:")
            .and_then(|after_open| after_open.find('}').map(|end| (after_open, end)))
        {
            let id = after_open[..end_rel].trim();
            append_step_ref(&mut out, id, prior_outputs, limit)?;
            rest = &after_open[end_rel + 1..];
        } else {
            crate::safety::push_str_limited(&mut out, "{", limit, "Sequential prompt")?;
            rest = &candidate[1..];
        }
    }
    crate::safety::push_str_limited(&mut out, rest, limit, "Sequential prompt")?;
    Ok(out)
}

/// A step output's contribution to a prompt: the whole artifact, or — for
/// `{step:<id>#/json/pointer}` references — the selected slice of its
/// canonical JSON. String slices insert verbatim; other values insert as
/// pretty JSON. Unresolvable pointers become a readable parenthesized notice.
pub(super) fn step_ref_text(output: &StepOutput, pointer: Option<&str>) -> String {
    let Some(pointer) = pointer else {
        return output.raw_text.clone();
    };
    if pointer.is_empty() {
        return output.raw_text.clone();
    }
    if !pointer.starts_with('/') {
        return format!(
            "(invalid JSON pointer '{pointer}' for step '{}'; pointers start with '/')",
            output.step_id
        );
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(output.raw_text.trim()) else {
        return format!(
            "(step '{}' output is not structured JSON, so '{pointer}' cannot be selected)",
            output.step_id
        );
    };
    match value.pointer(pointer) {
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(slice) => serde_json::to_string_pretty(slice).unwrap_or_else(|_| slice.to_string()),
        None => format!("(step '{}' has no value at '{pointer}')", output.step_id),
    }
}

pub(super) fn append_step_ref(
    out: &mut String,
    id: &str,
    prior_outputs: &[StepOutput],
    limit: usize,
) -> Result<(), String> {
    // `{step:<id>#/json/pointer}` selects into a structured artifact.
    let (id, pointer) = match id.split_once('#') {
        Some((id, pointer)) => (id.trim(), Some(pointer.trim())),
        None => (id, None),
    };
    if id.is_empty() {
        return crate::safety::push_str_limited(
            out,
            "(empty step reference)",
            limit,
            "Sequential prompt",
        );
    }
    // Exact id match wins (covers both "technical" and "technical/claude").
    if let Some(o) = prior_outputs.iter().find(|o| o.step_id == id) {
        return crate::safety::push_str_limited(
            out,
            &step_ref_text(o, pointer),
            limit,
            "Sequential prompt",
        );
    }
    // Otherwise, gather all step outputs whose base id (before any '/') matches.
    let matches: Vec<&StepOutput> = prior_outputs
        .iter()
        .filter(|o| o.step_id.split('/').next() == Some(id))
        .collect();
    if matches.is_empty() {
        return crate::safety::push_str_limited(
            out,
            &format!("(no output for step '{id}')"),
            limit,
            "Sequential prompt",
        );
    }
    if matches.len() == 1 {
        return crate::safety::push_str_limited(
            out,
            &step_ref_text(matches[0], pointer),
            limit,
            "Sequential prompt",
        );
    }
    for (index, output) in matches.iter().enumerate() {
        if index > 0 {
            crate::safety::push_str_limited(out, "\n\n---\n\n", limit, "Sequential prompt")?;
        }
        let text = step_ref_text(output, pointer);
        for value in ["### ", output.step_label.as_str(), "\n\n", text.as_str()] {
            crate::safety::push_str_limited(out, value, limit, "Sequential prompt")?;
        }
    }
    Ok(())
}

/// Replace `{<prefix>key}` placeholders using `map`. A declared optional
/// value that was not supplied remains visible rather than silently becoming
/// an empty string; undeclared placeholders are rejected during workflow
/// validation. `needle` includes the trailing colon, e.g. "{var:".
pub(super) fn substitute_placeholders(
    text: &str,
    needle: &str,
    map: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    if !text.contains(needle) {
        return Ok(text.to_string());
    }
    let limit = crate::safety::MAX_EXPANDED_PROMPT_BYTES;
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(needle) {
        crate::safety::push_str_limited(&mut out, &rest[..start], limit, "Run prompt")?;
        let after = &rest[start + needle.len()..];
        if let Some(end) = after.find('}') {
            let key = after[..end].trim();
            let missing = format!("(optional value '{key}' not supplied)");
            crate::safety::push_str_limited(
                &mut out,
                map.get(key).map(|s| s.as_str()).unwrap_or(&missing),
                limit,
                "Run prompt",
            )?;
            rest = &after[end + 1..];
        } else {
            crate::safety::push_str_limited(&mut out, &rest[start..], limit, "Run prompt")?;
            return Ok(out);
        }
    }
    crate::safety::push_str_limited(&mut out, rest, limit, "Run prompt")?;
    Ok(out)
}

/// Apply both run-time substitutions to a prompt: `{var:key}` (values) and
/// `{input:key}` (paths to extra named inputs' extracted text).
pub(super) fn substitute_run_context(
    text: &str,
    vars: &std::collections::HashMap<String, String>,
    inputs: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let t = substitute_placeholders(text, "{var:", vars)?;
    substitute_placeholders(&t, "{input:", inputs)
}

/// Replace specialist-schema JSON artifacts with their deterministic readable
/// rendering (plus a stable `Report id` line) when building sequential
/// context. Keyed by the producing step's schema, so canonical findings JSON
/// — for example a consolidation product feeding validation — passes through
/// verbatim.
pub(super) fn specialist_context_outputs(
    outputs: &[StepOutput],
    specialist_steps: &std::collections::HashSet<String>,
) -> Vec<StepOutput> {
    outputs
        .iter()
        .map(|output| {
            let base_id = output.step_id.split('/').next().unwrap_or(&output.step_id);
            if !output.skipped && specialist_steps.contains(base_id) {
                if let Some(rendered) =
                    crate::auto_review::specialist_context_text(&output.step_id, &output.raw_text)
                {
                    let mut rendered_output = output.clone();
                    rendered_output.raw_text = rendered;
                    return rendered_output;
                }
            }
            output.clone()
        })
        .collect()
}
