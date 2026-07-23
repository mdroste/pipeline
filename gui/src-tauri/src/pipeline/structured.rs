//! Lightweight validation of a step's JSON output against a declared shape.
//!
//! This is intentionally NOT a full JSON Schema implementation (that would pull
//! in a heavy dependency). It supports the subset the pipeline actually uses:
//!
//! - top-level `type`: "object" | "array" | "string" | "number" | "integer" |
//!   "boolean"
//! - for objects: `required` (a list of keys that must be present) and
//!   `properties` (each property's `type` is checked, one level deep)
//! - for arrays: `items` with a `type` (and, if the item type is object, its
//!   `required` keys are checked on every element)
//!
//! Enough to enforce "an array of issue objects each with id/severity/body"
//! without a schema engine. Unknown schema keywords are ignored, so a stricter
//! validator could be swapped in later without breaking stored schemas.

const MAX_SCHEMA_DEPTH: usize = 32;
const SUPPORTED_TYPES: &[&str] = &[
    "object", "array", "string", "number", "integer", "boolean", "null",
];

/// Pull a JSON value out of a model's text response: the whole string, any
/// fenced block, or any object/array embedded in prose. Candidates are tried
/// in source order, so malformed prose before a valid payload cannot hide it.
pub fn extract_json(text: &str) -> Option<serde_json::Value> {
    let trimmed = text.trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Some(v);
    }
    // Fenced blocks also support primitive JSON values, which do not start
    // with an object/array delimiter.
    for inner in fenced_blocks(trimmed) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(inner.trim()) {
            return Some(v);
        }
    }
    // `StreamDeserializer` consumes exactly one value and handles nesting,
    // strings, and mixed object/array delimiters without a hand-written parser.
    for (offset, ch) in trimmed.char_indices() {
        if matches!(ch, '{' | '[') {
            let mut stream = serde_json::Deserializer::from_str(&trimmed[offset..])
                .into_iter::<serde_json::Value>();
            if let Some(Ok(v)) = stream.next() {
                return Some(v);
            }
        }
    }
    None
}

fn fenced_blocks(mut text: &str) -> Vec<&str> {
    let mut blocks = Vec::new();
    while let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        let body_start = after.find('\n').map_or(0, |i| i + 1);
        let body = &after[body_start..];
        let Some(end) = body.find("```") else {
            break;
        };
        blocks.push(&body[..end]);
        text = &body[end + 3..];
    }
    blocks
}

/// Validate the supported schema subset itself. Unknown keywords are retained
/// for forwards compatibility, but every supported keyword must be well
/// formed and semantically compatible with the declared type.
pub fn validate_schema(schema: &serde_json::Value) -> Result<(), String> {
    validate_schema_at(schema, "$", 0)
}

fn validate_schema_at(schema: &serde_json::Value, path: &str, depth: usize) -> Result<(), String> {
    if depth > MAX_SCHEMA_DEPTH {
        return Err(format!(
            "{path}: schema nesting exceeds the {MAX_SCHEMA_DEPTH}-level safety limit"
        ));
    }
    let object = schema
        .as_object()
        .ok_or_else(|| format!("{path}: schema must be a JSON object"))?;
    let declared_type = match object.get("type") {
        Some(serde_json::Value::String(ty)) if SUPPORTED_TYPES.contains(&ty.as_str()) => {
            Some(ty.as_str())
        }
        Some(serde_json::Value::String(ty)) => {
            return Err(format!("{path}.type: unsupported type '{ty}'"));
        }
        Some(_) => return Err(format!("{path}.type: expected a string")),
        None => None,
    };

    if let Some(required) = object.get("required") {
        if declared_type.is_some_and(|ty| ty != "object") {
            return Err(format!("{path}.required: only valid for an object schema"));
        }
        let required = required
            .as_array()
            .ok_or_else(|| format!("{path}.required: expected an array of unique strings"))?;
        let mut seen = std::collections::HashSet::new();
        for (index, key) in required.iter().enumerate() {
            let key = key
                .as_str()
                .ok_or_else(|| format!("{path}.required[{index}]: expected a string"))?;
            if !seen.insert(key) {
                return Err(format!("{path}.required: duplicate property '{key}'"));
            }
        }
    }

    if let Some(properties) = object.get("properties") {
        if declared_type.is_some_and(|ty| ty != "object") {
            return Err(format!(
                "{path}.properties: only valid for an object schema"
            ));
        }
        let properties = properties
            .as_object()
            .ok_or_else(|| format!("{path}.properties: expected an object"))?;
        for (key, child) in properties {
            validate_schema_at(child, &format!("{path}.properties.{key}"), depth + 1)?;
        }
    }

    if let Some(items) = object.get("items") {
        if declared_type.is_some_and(|ty| ty != "array") {
            return Err(format!("{path}.items: only valid for an array schema"));
        }
        validate_schema_at(items, &format!("{path}.items"), depth + 1)?;
    }
    Ok(())
}

/// Validate `value` against the supported subset of `schema`. Returns Ok(()) or
/// a human-readable reason on the first violation.
pub fn validate(schema: &serde_json::Value, value: &serde_json::Value) -> Result<(), String> {
    validate_schema(schema)?;
    validate_at(schema, value, "$")
}

fn validate_at(
    schema: &serde_json::Value,
    value: &serde_json::Value,
    path: &str,
) -> Result<(), String> {
    if let Some(ty) = schema.get("type").and_then(|t| t.as_str()) {
        if !type_matches(ty, value) {
            return Err(format!(
                "{path}: expected type {ty}, got {}",
                json_type(value)
            ));
        }
    }
    match value {
        serde_json::Value::Object(map) => {
            if let Some(req) = schema.get("required").and_then(|r| r.as_array()) {
                for key in req.iter().filter_map(|k| k.as_str()) {
                    if !map.contains_key(key) {
                        return Err(format!("{path}: missing required property '{key}'"));
                    }
                }
            }
            if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
                for (key, subschema) in props {
                    if let Some(v) = map.get(key) {
                        validate_at(subschema, v, &format!("{path}.{key}"))?;
                    }
                }
            }
        }
        serde_json::Value::Array(items) => {
            if let Some(item_schema) = schema.get("items") {
                for (i, v) in items.iter().enumerate() {
                    validate_at(item_schema, v, &format!("{path}[{i}]"))?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn type_matches(ty: &str, value: &serde_json::Value) -> bool {
    match ty {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.is_i64() || value.is_u64(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => false,
    }
}

fn json_type(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Object(_) => "object",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Null => "null",
    }
}

/// Convenience: extract JSON from `text` and validate it against `schema`.
pub fn check(schema: &serde_json::Value, text: &str) -> Result<(), String> {
    let value = extract_json(text).ok_or_else(|| "output is not valid JSON".to_string())?;
    validate(schema, &value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_plain_json() {
        assert_eq!(extract_json("{\"a\":1}"), Some(serde_json::json!({"a":1})));
        assert_eq!(extract_json("[1,2,3]"), Some(serde_json::json!([1, 2, 3])));
    }

    #[test]
    fn extract_from_fence_and_prose() {
        let fenced = "Here you go:\n```json\n{\"a\": 1}\n```\nDone.";
        assert_eq!(extract_json(fenced), Some(serde_json::json!({"a":1})));

        let prose = "The result is {\"ok\": true} as requested.";
        assert_eq!(extract_json(prose), Some(serde_json::json!({"ok":true})));
    }

    #[test]
    fn extract_balanced_span_ignores_braces_in_strings() {
        let text = "prefix {\"note\": \"a } brace in a string\"} suffix";
        assert_eq!(
            extract_json(text),
            Some(serde_json::json!({"note": "a } brace in a string"}))
        );
    }

    #[test]
    fn extract_skips_malformed_candidates_and_fences() {
        let text = "broken [not json]\n```json\n{still broken}\n```\nresult: [{\"ok\":true}]";
        assert_eq!(extract_json(text), Some(serde_json::json!([{"ok": true}])));
    }

    #[test]
    fn extract_handles_mixed_nested_delimiters() {
        let text = r#"prefix {"items":[{"text":"} ]"}]} suffix"#;
        assert_eq!(
            extract_json(text),
            Some(serde_json::json!({"items": [{"text": "} ]"}]}))
        );
    }

    #[test]
    fn extract_returns_none_for_non_json() {
        assert!(extract_json("just some prose, no json here").is_none());
    }

    #[test]
    fn validate_object_required_and_types() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["issues"],
            "properties": { "issues": { "type": "array" } }
        });
        assert!(validate(&schema, &serde_json::json!({"issues": []})).is_ok());
        let err = validate(&schema, &serde_json::json!({"other": 1})).unwrap_err();
        assert!(err.contains("missing required property 'issues'"));
        let err2 = validate(&schema, &serde_json::json!({"issues": "nope"})).unwrap_err();
        assert!(err2.contains("expected type array"));
    }

    #[test]
    fn validate_array_items_required_keys() {
        let schema = serde_json::json!({
            "type": "array",
            "items": {
                "type": "object",
                "required": ["id", "severity"]
            }
        });
        let good = serde_json::json!([{"id": "1", "severity": "high"}]);
        assert!(validate(&schema, &good).is_ok());
        let bad = serde_json::json!([{"id": "1"}]);
        assert!(validate(&schema, &bad).unwrap_err().contains("severity"));
    }

    #[test]
    fn check_end_to_end() {
        let schema = serde_json::json!({ "type": "object", "required": ["ok"] });
        assert!(check(&schema, "```json\n{\"ok\": true}\n```").is_ok());
        assert!(check(&schema, "not json").is_err());
        assert!(check(&schema, "{\"nope\": 1}").is_err());
    }

    #[test]
    fn malformed_and_unknown_schemas_are_rejected() {
        assert!(validate_schema(&serde_json::json!([])).is_err());
        assert!(validate_schema(&serde_json::json!({"type": "date"}))
            .unwrap_err()
            .contains("unsupported type"));
        assert!(validate_schema(&serde_json::json!({"required": ["id", 2]})).is_err());
        assert!(validate_schema(&serde_json::json!({"type": "string", "items": {}})).is_err());
    }
}
