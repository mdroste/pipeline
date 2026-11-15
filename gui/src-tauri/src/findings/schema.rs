use super::{MAX_EVIDENCE, MAX_SOURCES};
use serde_json::Value;
use std::collections::HashSet;

pub const FINDING_SCHEMA_VERSION: u32 = 2;

/// Canonical report sections used by the Automatic Paper Review consolidation
/// contract. The schema enum is the source of truth; prompts reference these
/// labels rather than restating them as free text.
pub const CATEGORIES: [&str; 4] = [
    "Correctness and Internal Consistency",
    "Methodological and Evidentiary Concerns",
    "Contribution and Scope",
    "Exposition and Organization",
];

/// The shared evidence-locator array contract. Descriptions carry the locator
/// semantics so any profile adopting the schema gets the same guidance the
/// stock prompts rely on.
pub fn legacy_evidence_schema() -> Value {
    serde_json::json!({
        "type": "array",
        "maxItems": MAX_EVIDENCE,
        "description": "Locators supporting the finding. Use only references present in the supplied material; never invent a locator.",
        "items": {
            "type": "object",
            "properties": {
                "page": {"type": "integer", "description": "1-based page number in the rendered document."},
                "line_start": {"type": "integer", "description": "1-based first line of a cited source range."},
                "line_end": {"type": "integer", "description": "1-based last line of the range; at least line_start."},
                "node_id": {"type": "string", "description": "DocumentBundle node id from the structure index."},
                "asset_id": {"type": "string", "description": "DocumentBundle asset id of a cited figure, table, or page render."},
                "artifact_path": {"type": "string", "description": "Path relative to the saved run's artifacts."},
                "source_path": {"type": "string", "description": "Path relative to the selected source root."},
                "source_hash": {"type": "string", "description": "Content fingerprint of the cited source file, when known."},
                "description": {"type": "string", "description": "Human-readable location, such as 'Section 4.2, Theorem 3'."},
                "quote": {"type": "string", "description": "Short verbatim quote from the cited location."}
            }
        }
    })
}

/// The host-owned schema used by Automatic Paper Review and available to any
/// profile that explicitly publishes findings.
pub fn legacy_output_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "title": "Consolidated findings",
        "required": ["findings"],
        "properties": {
            "findings": {
                "type": "array",
                "maxItems": 40,
                "description": "Findings in decreasing order of centrality to the main claims; array order conveys importance.",
                "items": {
                    "type": "object",
                    "required": ["id", "title", "category", "body", "evidence"],
                    "properties": {
                        "id": {"type": "string", "minLength": 1, "description": "Stable descriptive identifier preserved across consolidation and validation; never an ordinal."},
                        "source_key": {"type": "string", "description": "Stable producer key (rule id, check id) when a deterministic producer supplies one."},
                        "sources": {
                            "type": "array",
                            "maxItems": MAX_SOURCES,
                            "description": "Report ids of the reviewer analyses supporting this finding.",
                            "items": {"type": "string"}
                        },
                        "title": {"type": "string", "minLength": 1, "description": "Specific one-line title."},
                        "category": {
                            "type": "string",
                            "enum": CATEGORIES,
                            "description": "Exactly one canonical report section."
                        },
                        "priority": {
                            "type": "string",
                            "enum": ["high", "medium", "low"],
                            "description": "Optional coarse priority; array order remains the authoritative ranking."
                        },
                        "body": {"type": "string", "description": "Markdown preserving the paper evidence, the problem, its consequence, and what would address it."},
                        "evidence": legacy_evidence_schema()
                    }
                }
            }
        }
    })
}

/// Strict v2 evidence contract. JSON Schema supplies the portable structural
/// constraints; [`validate_v2_semantics`] enforces that each object carries a
/// real locator instead of accepting a description-only placeholder.
pub fn evidence_schema() -> Value {
    serde_json::json!({
        "type": "array",
        "minItems": 1,
        "maxItems": MAX_EVIDENCE,
        "description": "Evidence locators supporting the finding. Every item needs a typed locator and begins unverified; only Pipeline's host verifier may promote it.",
        "items": {
            "type": "object",
            "required": ["evidence_type", "verification_status", "description"],
            "properties": {
                "evidence_type": {"type": "string", "enum": ["document", "source", "external", "call"]},
                "verification_status": {"type": "string", "enum": ["unverified"]},
                "page": {"type": "integer", "description": "1-based rendered page."},
                "line_start": {"type": "integer", "description": "1-based source line."},
                "line_end": {"type": "integer", "description": "Last source line, at least line_start."},
                "node_id": {"type": "string", "minLength": 1},
                "asset_id": {"type": "string", "minLength": 1},
                "artifact_path": {"type": "string", "minLength": 1},
                "source_path": {"type": "string", "minLength": 1},
                "source_hash": {"type": "string", "minLength": 1},
                "url": {"type": "string", "minLength": 1},
                "doi": {"type": "string", "minLength": 1},
                "publisher": {"type": "string", "minLength": 1},
                "accessed_at": {"type": "string", "minLength": 1},
                "query_id": {"type": "string", "minLength": 1},
                "call_id": {"type": "string", "minLength": 1},
                "description": {"type": "string", "minLength": 1},
                "quote": {"type": "string", "minLength": 1}
            }
        }
    })
}

/// Host-owned findings-v2 contract for the stock paper taxonomy.
pub fn output_schema() -> Value {
    output_schema_for_taxonomy(&CATEGORIES.map(str::to_string))
}

/// Build a strict findings-v2 contract with a workflow-declared taxonomy.
/// The declaration is retained as a host-only schema marker and must exactly
/// match the taxonomy returned in the artifact.
pub fn output_schema_for_taxonomy(taxonomy: &[String]) -> Value {
    let taxonomy = taxonomy
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    serde_json::json!({
        "type": "object",
        "title": "Canonical findings v2",
        crate::pipeline::structured::FINDINGS_VERSION_KEY: FINDING_SCHEMA_VERSION,
        crate::pipeline::structured::FINDINGS_TAXONOMY_KEY: taxonomy,
        "required": ["schema_version", "taxonomy", "findings"],
        "properties": {
            "schema_version": {"type": "integer", "enum": [FINDING_SCHEMA_VERSION]},
            "taxonomy": {
                "type": "array",
                "minItems": 1,
                "uniqueItems": true,
                "items": {"type": "string", "enum": taxonomy}
            },
            "findings": {
                "type": "array",
                "maxItems": 40,
                "description": "Findings in canonical global rank order.",
                "items": {
                    "type": "object",
                    "required": [
                        "rank", "id", "title", "category", "severity",
                        "confidence", "verification_status", "reviewer_ids",
                        "problem", "consequence", "recommended_action", "evidence"
                    ],
                    "properties": {
                        "rank": {"type": "integer", "description": "Canonical 1-based global rank, equal to array position."},
                        "id": {"type": "string", "minLength": 1, "description": "Stable descriptive identity; never an ordinal."},
                        "source_key": {"type": "string", "minLength": 1},
                        "reviewer_ids": {"type": "array", "minItems": 1, "maxItems": MAX_SOURCES, "uniqueItems": true, "items": {"type": "string", "minLength": 1}},
                        "source_call_ids": {"type": "array", "maxItems": MAX_SOURCES, "uniqueItems": true, "items": {"type": "string", "minLength": 1}},
                        "title": {"type": "string", "minLength": 1},
                        "category": {"type": "string", "enum": taxonomy},
                        "severity": {"type": "string", "enum": ["critical", "high", "medium", "low"]},
                        "confidence": {"type": "string", "enum": ["high", "medium", "low"]},
                        "verification_status": {"type": "string", "enum": ["unverified"]},
                        "problem": {"type": "string", "minLength": 1},
                        "consequence": {"type": "string", "minLength": 1},
                        "recommended_action": {"type": "string", "minLength": 1},
                        "evidence": evidence_schema()
                    }
                }
            }
        }
    })
}

pub fn validation_output_schema_for_taxonomy(taxonomy: &[String]) -> Value {
    let mut schema = output_schema_for_taxonomy(taxonomy);
    schema[crate::pipeline::structured::VALIDATION_LEDGER_KEY] =
        Value::String("required".to_string());
    let required = schema["required"]
        .as_array_mut()
        .expect("findings-v2 required fields");
    required.push(Value::String("validation_dispositions".to_string()));
    schema["properties"]["validation_dispositions"] = serde_json::json!({
        "type": "array",
        "description": "Exactly one machine-readable disposition for every input finding id, in input order.",
        "items": {
            "type": "object",
            "required": ["finding_id", "disposition", "reason"],
            "properties": {
                "finding_id": {"type": "string", "minLength": 1},
                "disposition": {"type": "string", "enum": [
                    "retained", "revised", "merged_into",
                    "rejected_false_positive", "unverified_missing_evidence",
                    "deferred_manual_review"
                ]},
                "reason": {"type": "string", "minLength": 1},
                "merged_into": {"type": "string", "minLength": 1},
                "before_hash": {"type": "string", "minLength": 1},
                "after_hash": {"type": "string", "minLength": 1}
            }
        }
    });
    schema
}

/// Cross-field findings-v2 invariants that the deliberately small portable
/// schema dialect cannot express.
pub fn validate_v2_semantics(schema: &Value, value: &Value) -> Result<(), String> {
    if schema
        .get(crate::pipeline::structured::FINDINGS_VERSION_KEY)
        .and_then(Value::as_u64)
        != Some(FINDING_SCHEMA_VERSION as u64)
    {
        return Ok(());
    }
    let expected_taxonomy = schema
        .get(crate::pipeline::structured::FINDINGS_TAXONOMY_KEY)
        .and_then(Value::as_array)
        .ok_or("findings-v2 schema is missing its taxonomy")?;
    if value.get("taxonomy").and_then(Value::as_array) != Some(expected_taxonomy) {
        return Err("$.taxonomy must exactly match the workflow-declared taxonomy".to_string());
    }
    let findings = value
        .get("findings")
        .and_then(Value::as_array)
        .ok_or("$.findings must be an array")?;
    let mut ids = HashSet::new();
    for (index, finding) in findings.iter().enumerate() {
        let object = finding
            .as_object()
            .ok_or_else(|| format!("$.findings[{index}] must be an object"))?;
        let rank = object.get("rank").and_then(Value::as_u64).unwrap_or(0);
        if rank != (index + 1) as u64 {
            return Err(format!(
                "$.findings[{index}].rank must be {} to preserve canonical global order",
                index + 1
            ));
        }
        let id = object.get("id").and_then(Value::as_str).unwrap_or_default();
        if !ids.insert(id) {
            return Err(format!(
                "$.findings[{index}].id duplicates stable id '{id}'"
            ));
        }
        let evidence = object
            .get("evidence")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("$.findings[{index}].evidence must be an array"))?;
        for (evidence_index, locator) in evidence.iter().enumerate() {
            let locator = locator.as_object().ok_or_else(|| {
                format!("$.findings[{index}].evidence[{evidence_index}] must be an object")
            })?;
            let has_locator = [
                "page",
                "line_start",
                "node_id",
                "asset_id",
                "artifact_path",
                "source_path",
                "url",
                "doi",
                "query_id",
                "call_id",
            ]
            .iter()
            .any(|key| {
                locator.get(*key).is_some_and(|value| match value {
                    Value::String(text) => !text.trim().is_empty(),
                    Value::Number(number) => number.as_u64().is_some_and(|number| number > 0),
                    _ => false,
                })
            });
            if !has_locator {
                return Err(format!(
                    "$.findings[{index}].evidence[{evidence_index}] needs a concrete page, line, node, asset, path, URL, DOI, query, or call locator"
                ));
            }
        }
    }
    Ok(())
}
