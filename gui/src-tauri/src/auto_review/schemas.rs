//! Host schema construction and bounded catalog-reference expansion.
use super::catalog::{GENRES, METHODS, RESEARCH_ARTICLE_GENRE, SLIDE_DECK_GENRE, SUBJECTS};
use super::{
    AUTO_REVIEW_CONTRACT, CATALOG_POLICY_KEY, CATALOG_REFERENCE_KEY, GENRE_CATALOG, METHOD_CATALOG,
    SUBJECT_CATALOG,
};

fn catalog_values(reference: &str) -> Result<Vec<serde_json::Value>, String> {
    let strings = match reference {
        SUBJECT_CATALOG => SUBJECTS.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        METHOD_CATALOG => METHODS.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        GENRE_CATALOG => std::iter::once(RESEARCH_ARTICLE_GENRE)
            .chain(GENRES.iter().map(|entry| entry.id))
            .collect::<Vec<_>>(),
        _ => return Err(format!("unknown Pipeline catalog reference '{reference}'")),
    };
    Ok(strings
        .into_iter()
        .map(|value| serde_json::Value::String(value.to_string()))
        .collect())
}

fn resolve_schema_catalogs_at(
    schema: &mut serde_json::Value,
    path: &str,
    depth: usize,
) -> Result<(), String> {
    if depth > 32 {
        return Err(format!(
            "{path}: schema catalog resolution exceeds 32 levels"
        ));
    }
    let object = schema
        .as_object_mut()
        .ok_or_else(|| format!("{path}: schema must be a JSON object"))?;
    if let Some(policy) = object.remove(CATALOG_POLICY_KEY) {
        if policy.as_str() != Some("live") {
            return Err(format!(
                "{path}.{CATALOG_POLICY_KEY}: only the 'live' catalog policy is supported"
            ));
        }
    }
    if let Some(reference) = object.remove(CATALOG_REFERENCE_KEY) {
        let reference = reference
            .as_str()
            .ok_or_else(|| format!("{path}.{CATALOG_REFERENCE_KEY}: expected a string"))?;
        if object.get("type").and_then(serde_json::Value::as_str) != Some("string") {
            return Err(format!(
                "{path}.{CATALOG_REFERENCE_KEY}: catalog-backed values must have type 'string'"
            ));
        }
        if object.contains_key("enum") {
            return Err(format!(
                "{path}: use either enum or {CATALOG_REFERENCE_KEY}, not both"
            ));
        }
        object.insert(
            "enum".to_string(),
            serde_json::Value::Array(catalog_values(reference)?),
        );
    }
    if let Some(properties) = object
        .get_mut("properties")
        .and_then(serde_json::Value::as_object_mut)
    {
        for (key, child) in properties {
            resolve_schema_catalogs_at(child, &format!("{path}.properties.{key}"), depth + 1)?;
        }
    }
    if let Some(items) = object.get_mut("items") {
        resolve_schema_catalogs_at(items, &format!("{path}.items"), depth + 1)?;
    }
    Ok(())
}

/// Compile Pipeline catalog references into ordinary JSON Schema enums. The
/// saved workflow stays compact; providers and host validation receive this
/// self-contained snapshot for the current run.
pub fn resolve_schema_catalogs(schema: &serde_json::Value) -> Result<serde_json::Value, String> {
    let mut resolved = schema.clone();
    resolve_schema_catalogs_at(&mut resolved, "$", 0)?;
    crate::pipeline::structured::validate_schema(&resolved)?;
    Ok(resolved)
}

/// The structured output contract shared by every core reviewer and
/// materialized specialist. The item fields are the referee comment structure
/// the earlier Markdown format enforced through prose; evidence is captured
/// as typed locators at the source, where the reviewer has the page open.
/// An empty findings array replaces the former sentinel sentence.
pub fn specialist_schema() -> serde_json::Value {
    let mut evidence = crate::findings::evidence_schema();
    evidence["minItems"] = serde_json::json!(1);
    serde_json::json!({
        "type": "object",
        "title": "Referee findings",
        "description": "Independent referee findings from one reviewer pass. An empty findings array means no material issue survived checking.",
        "required": ["findings"],
        "properties": {
            "findings": {
                "type": "array",
                "maxItems": 7,
                "description": "Material, well-supported findings in decreasing order of importance.",
                "items": {
                    "type": "object",
                    "required": [
                        "title", "in_the_paper", "problem", "consequence",
                        "what_would_help", "evidence"
                    ],
                    "properties": {
                        "title": {"type": "string", "minLength": 1, "description": "Specific descriptive title naming the issue."},
                        "in_the_paper": {"type": "string", "minLength": 1, "description": "Quote or close paraphrase of the claim or result and the evidence it relies on."},
                        "problem": {"type": "string", "minLength": 1, "description": "The specific analysis, in Markdown: the logic, comparison, calculation, or counterexample rather than a generic concern."},
                        "consequence": {"type": "string", "minLength": 1, "description": "Exactly which conclusion, interpretation, or scope claim is affected."},
                        "what_would_help": {"type": "string", "minLength": 1, "description": "The smallest credible correction, test, comparison, qualification, or additional argument."},
                        "evidence": evidence
                    }
                }
            }
        }
    })
}

/// Structural check for the specialist findings contract, keyed on the item
/// fields rather than a marker so user copies of specialist steps keep their
/// readable rendering.
pub fn is_specialist_schema(schema: &serde_json::Value) -> bool {
    schema
        .pointer("/properties/findings/items/required")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|required| {
            ["problem", "what_would_help"].iter().all(|key| {
                required
                    .iter()
                    .any(|candidate| candidate.as_str() == Some(key))
            })
        })
}

/// Schema for the auto-review router. `subject_specialist_ids` is ordered: the
/// first element is primary and the optional second element is secondary.
pub fn orientation_schema() -> serde_json::Value {
    let mut schema = crate::orientation_contract::paper_schema();
    let root = schema
        .as_object_mut()
        .expect("the stock paper orientation schema has an object root");
    root.insert(
        "x-pipeline-contract".to_string(),
        serde_json::json!(AUTO_REVIEW_CONTRACT),
    );
    root.insert(CATALOG_POLICY_KEY.to_string(), serde_json::json!("live"));
    root.get_mut("required")
        .and_then(serde_json::Value::as_array_mut)
        .expect("the stock paper orientation schema declares required fields")
        .insert(1, serde_json::json!("review_plan"));
    root.get_mut("properties")
        .and_then(serde_json::Value::as_object_mut)
        .expect("the stock paper orientation schema declares properties")
        .insert(
            "review_plan".to_string(),
            serde_json::json!({
                "type": "object",
                "description": "Bounded paper classification and specialist plan.",
                "required": [
                    "primary_domain", "subject", "paper_forms",
                    "subject_specialist_ids", "method_specialist_ids",
                    "genre", "selection_notes", "routing_uncertainty"
                ],
                "properties": {
                    "primary_domain": {"type": "string"},
                    "subject": {"type": "string"},
                    "paper_forms": {
                        "type": "array",
                        "description": "Forms central to the paper's claims.",
                        "minItems": 1,
                        "uniqueItems": true,
                        "items": {
                            "type": "string",
                            "enum": [
                                "formal_theory", "causal_empirical", "quantitative_model",
                                "descriptive", "experimental", "algorithmic", "qualitative",
                                "interpretive", "historical", "clinical", "engineering_design"
                            ]
                        }
                    },
                    "subject_specialist_ids": {
                        "type": "array",
                        "description": "Primary, then optional secondary subject specialist.",
                        "minItems": 1,
                        "maxItems": 2,
                        "uniqueItems": true,
                        "items": {"type": "string", CATALOG_REFERENCE_KEY: SUBJECT_CATALOG}
                    },
                    "method_specialist_ids": {
                        "type": "array",
                        "description": "Smallest nonduplicative method panel.",
                        "minItems": 1,
                        "maxItems": 4,
                        "uniqueItems": true,
                        "items": {"type": "string", CATALOG_REFERENCE_KEY: METHOD_CATALOG}
                    },
                    "genre": {
                        "type": "string",
                        CATALOG_REFERENCE_KEY: GENRE_CATALOG
                    },
                    "selection_notes": {
                        "type": "array",
                        "description": "One paper-specific reason per selected specialist.",
                        "minItems": 2,
                        "maxItems": 6,
                        "items": {
                            "type": "object",
                            "required": ["id", "reason"],
                            "properties": {
                                "id": {"type": "string"},
                                "reason": {"type": "string", "description": "Concrete paper-specific selection reason."}
                            }
                        }
                    },
                    "routing_uncertainty": {
                        "type": "array",
                        "description": "Material ambiguity; empty when none.",
                        "maxItems": 5,
                        "items": {"type": "string"}
                    }
                }
            }),
        );
    schema
}

/// Quick uses the same allowlisted router and subject range as Full, but caps
/// methods at two. Together these schema bounds allow 2–4 adaptive agents:
/// 1–2 subject specialists plus 1–2 method specialists.
pub fn quick_orientation_schema() -> serde_json::Value {
    let mut schema = orientation_schema();
    schema["properties"]["review_plan"]["properties"]["method_specialist_ids"]["maxItems"] =
        serde_json::json!(2);
    schema["properties"]["review_plan"]["properties"]["selection_notes"]["maxItems"] =
        serde_json::json!(4);
    schema
}

/// Slide Review keeps the Full panel bounds but pins `genre` to the slide-deck
/// genre, so its host-owned context reaches every core reviewer, specialist,
/// consolidation, and validation step regardless of the router's judgment.
pub fn slide_orientation_schema() -> serde_json::Value {
    let mut schema = orientation_schema();
    schema["properties"]["review_plan"]["properties"]["genre"] = serde_json::json!({
        "type": "string",
        "description": "Fixed for Slide Review: the document is a slide deck.",
        "enum": [SLIDE_DECK_GENRE]
    });
    schema
}
