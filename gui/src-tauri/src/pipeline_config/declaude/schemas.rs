//! Portable, bounded editorial records. Referees are unique scalar IDs rather
//! than objects: JSON Schema can then enforce uniqueness without trusting the
//! model to keep differently worded assignments from duplicating a specialty.

use serde_json::{json, Value};

fn text() -> Value {
    json!({"type": "string", "minLength": 1})
}

fn texts(max: usize) -> Value {
    json!({"type": "array", "maxItems": max, "items": text()})
}

pub(super) fn orientation_schema() -> Value {
    let roles: Vec<&str> = std::iter::once(super::GENERAL_TOPIC)
        .chain(crate::auto_review::SUBJECTS.iter().map(|role| role.id))
        .collect();
    json!({
        "type": "object",
        "title": "Declaude document and editing brief",
        "required": ["document", "style_target", "style_anchors", "referees", "selection_notes", "sections", "protected_content", "extraction_quality_notes"],
        "properties": {
            "document": {
                "type": "object",
                "required": ["type", "subject", "audience", "purpose", "language", "classification_evidence"],
                "properties": {
                    "type": {"type": "string", "enum": ["paper", "slides", "website", "documentation", "grant", "policy_memo", "referee_response", "mixed", "other", "unknown"]},
                    "subject": text(), "audience": text(), "purpose": text(), "language": text(),
                    "classification_evidence": text()
                }
            },
            "style_target": text(),
            "style_anchors": {
                "type": "array", "maxItems": 3,
                "items": {"type": "object", "required": ["location", "quote", "qualities"],
                    "properties": {"location": text(), "quote": text(), "qualities": text()}}
            },
            "referees": {"type": "array", "minItems": 1, "maxItems": 3, "uniqueItems": true,
                "description": "Ordered subject IDs. First covers the whole document; others cover distinct relevant topics.",
                "items": {"type": "string", "enum": roles}},
            "selection_notes": {"type": "string", "minLength": 1,
                "description": "Explain every selected ID with its readable specialty, evidence, and section coverage."},
            "sections": {"type": "array", "maxItems": 200,
                "items": {"type": "object", "required": ["location", "topic", "local_form"],
                    "properties": {"location": text(), "topic": text(), "local_form": text()}}},
            "protected_content": texts(100),
            "extraction_quality_notes": {
                "type": "array",
                "description": "Preserve all host extraction diagnostics; include any other limitations.",
                "items": {"type": "object", "required": ["page_range", "description"],
                    "properties": {"page_range": text(), "description": text()}}
            }
        }
    })
}

pub(super) fn edits_schema() -> Value {
    json!({
        "type": "object", "title": "Declaude proposed prose edits",
        "required": ["coverage", "edits", "unresolved"],
        "properties": {
            "coverage": {
                "type": "object",
                "required": ["reviewed", "limitations", "incomplete"],
                "properties": {
                    "reviewed": texts(200), "limitations": texts(100),
                    "incomplete": {"type": "boolean"}
                }
            },
            "edits": {
                "type": "array", "maxItems": 60,
                "description": "Useful edits in document order. Sixty is a ceiling, never a target; disclose omitted coverage.",
                "items": {
                    "type": "object",
                    "required": ["location", "original", "replacement", "operation", "local_form", "category", "reason", "preserved_content", "context_support", "priority", "confidence", "referee_ids"],
                    "properties": {
                        "location": text(), "original": text(),
                        "replacement": {"type": "string", "description": "Paste-ready text; empty only for deletion."},
                        "operation": {"type": "string", "enum": ["replace", "delete"]},
                        "local_form": text(),
                        "category": {"type": "string", "enum": ["stock_language", "staged_contrast", "defensive_framing", "misplaced_jargon", "promotional_framing", "signposting", "repetition", "paragraph_structure", "vague_reference", "other"]},
                        "reason": text(), "preserved_content": text(),
                        "context_support": texts(20),
                        "priority": {"type": "string", "enum": ["recommended", "optional"]},
                        "confidence": {"type": "string", "enum": ["high", "medium"], "description": "Confidence in the edit's fidelity and benefit, never AI authorship."},
                        "referee_ids": {"type": "array", "minItems": 1, "maxItems": 3, "uniqueItems": true, "items": text()}
                    }
                }
            },
            "unresolved": {"type": "array", "maxItems": 60,
                "items": {"type": "object", "required": ["location", "original", "issue"],
                    "properties": {"location": text(), "original": text(), "issue": text()}}}
        }
    })
}
