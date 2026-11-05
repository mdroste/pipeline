//! Readable specialist findings and downstream context rendering.

const SPECIALIST_EMPTY_REPORT: &str = "No material issues identified.";
const MAX_RENDERED_QUOTE_CHARS: usize = 300;

fn evidence_citation(evidence: &serde_json::Value) -> Option<String> {
    let object = evidence.as_object()?;
    let text = |key: &str| {
        object
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let mut parts = Vec::new();
    if let Some(page) = object.get("page").and_then(serde_json::Value::as_u64) {
        if page > 0 {
            parts.push(format!("p. {page}"));
        }
    }
    if let Some(path) = text("source_path") {
        let lines = match (
            object.get("line_start").and_then(serde_json::Value::as_u64),
            object.get("line_end").and_then(serde_json::Value::as_u64),
        ) {
            (Some(start), Some(end)) if end > start => format!(":{start}–{end}"),
            (Some(start), _) => format!(":{start}"),
            _ => String::new(),
        };
        parts.push(format!("{path}{lines}"));
    }
    if let Some(description) = text("description") {
        parts.push(description.to_string());
    }
    for (key, label) in [("node_id", "node"), ("asset_id", "asset")] {
        if let Some(id) = text(key) {
            parts.push(format!("{label} {id}"));
        }
    }
    if let Some(quote) = text("quote") {
        let mut quote = quote.split_whitespace().collect::<Vec<_>>().join(" ");
        if quote.chars().count() > MAX_RENDERED_QUOTE_CHARS {
            quote = quote.chars().take(MAX_RENDERED_QUOTE_CHARS).collect();
            quote.push('…');
        }
        parts.push(format!("“{quote}”"));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// Deterministic readable rendering of a specialist findings artifact — the
/// exact referee-report format the earlier Markdown prompts specified. Returns
/// None when the value is not specialist-shaped.
pub fn specialist_report_markdown(value: &serde_json::Value) -> Option<String> {
    let findings = value.as_object()?.get("findings")?.as_array()?;
    if findings.is_empty() {
        return Some(SPECIALIST_EMPTY_REPORT.to_string());
    }
    let mut markdown = String::new();
    for (index, finding) in findings.iter().enumerate() {
        let object = finding.as_object()?;
        let field = |key: &str| {
            object
                .get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        // Every specialist field is schema-required; a value without the
        // distinctive fields is not a specialist artifact.
        if !object.contains_key("problem") || !object.contains_key("what_would_help") {
            return None;
        }
        if index > 0 {
            markdown.push('\n');
        }
        markdown.push_str(&format!("**#{}. {}**\n\n", index + 1, field("title")));
        for (key, label) in [
            ("in_the_paper", "In the paper"),
            ("problem", "The problem"),
            ("consequence", "Consequence"),
            ("what_would_help", "What would help"),
        ] {
            let text = field(key);
            if !text.is_empty() {
                markdown.push_str(&format!("- **{label}:** {text}\n"));
            }
        }
        let citations = object
            .get("evidence")
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(evidence_citation)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !citations.is_empty() {
            markdown.push_str(&format!("- **Location:** {}\n", citations.join("; ")));
        }
    }
    Some(markdown.trim_end().to_string())
}

/// Render one specialist step output for downstream sequential context. The
/// `Report id` line gives consolidation a stable identifier to carry into each
/// finding's `sources`.
pub fn specialist_context_text(step_id: &str, raw_text: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(raw_text.trim()).ok()?;
    let report = specialist_report_markdown(&value)?;
    let base_id = step_id.split('/').next().unwrap_or(step_id);
    Some(format!("Report id: {base_id}\n\n{report}"))
}
