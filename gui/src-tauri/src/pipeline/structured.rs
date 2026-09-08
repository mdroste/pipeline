//! Pipeline's deliberately small, provider-portable JSON Schema dialect.
//!
//! Artifact schemas have an object root and may use `type`, `enum`, `required`,
//! `properties`, `items`, `minItems`, `maxItems`, `uniqueItems`, `title`, and
//! `description`, plus the documented `x-pipeline-*` extensions. The validator
//! fails closed on every other keyword: accepting a constraint that is later
//! discarded by a provider would make the editor promise an unenforced contract.

const MAX_SCHEMA_DEPTH: usize = 32;
/// Common inline-schema ceiling chosen to remain safe on every supported CLI,
/// including Windows' much smaller process command-line limit.
pub const MAX_PROVIDER_SCHEMA_BYTES: usize = 20 * 1024;
const MAX_PROVIDER_ENUM_VALUES: usize = 1_000;
const LARGE_ENUM_THRESHOLD: usize = 250;
const MAX_LARGE_ENUM_STRING_CHARS: usize = 15_000;
const SUPPORTED_TYPES: &[&str] = &[
    "object", "array", "string", "number", "integer", "boolean", "null",
];
const SUPPORTED_KEYWORDS: &[&str] = &[
    "type",
    "enum",
    "minItems",
    "maxItems",
    "minLength",
    "uniqueItems",
    "title",
    "description",
    "required",
    "properties",
    "items",
    "x-pipeline-contract",
    "x-pipeline-catalog",
    "x-pipeline-catalog-policy",
    "x-pipeline-adaptive-agent-count",
    FINDINGS_VERSION_KEY,
    FINDINGS_TAXONOMY_KEY,
    VALIDATION_LEDGER_KEY,
    PRESERVE_FINDINGS_KEY,
    SCHEMA_REFERENCE_KEY,
];

/// Root-only marker resolving to a live host-owned contract at dispatch, the
/// way catalog references resolve to live enums. A profile stores only the
/// reference, so host contract improvements reach saved workflows without a
/// migration. `findings-v1` remains readable for legacy workflows;
/// `findings-v2` is the strict release contract.
pub const SCHEMA_REFERENCE_KEY: &str = "x-pipeline-schema";
pub const FINDINGS_VERSION_KEY: &str = "x-pipeline-findings-version";
pub const FINDINGS_TAXONOMY_KEY: &str = "x-pipeline-findings-taxonomy";
pub const VALIDATION_LEDGER_KEY: &str = "x-pipeline-validation-ledger";

/// Resolve a step schema's live contract reference, carrying every other root
/// `x-pipeline-*` marker (for example the preserve-findings lineage marker)
/// onto the resolved contract. Schemas without the marker return unchanged.
pub fn resolve_schema_reference(schema: &serde_json::Value) -> Result<serde_json::Value, String> {
    // Validate before resolution so structural siblings cannot disappear when
    // the live contract replaces the stored reference. Every caller (save,
    // preview, rerun, and dispatch) consequently gets the same fail-closed
    // behavior.
    validate_schema(schema)?;
    let Some(reference) = schema.get(SCHEMA_REFERENCE_KEY) else {
        return Ok(schema.clone());
    };
    let reference = reference
        .as_str()
        .ok_or_else(|| format!("$.{SCHEMA_REFERENCE_KEY}: expected a string"))?;
    let mut resolved = match reference {
        "findings-v1" => crate::findings::legacy_output_schema(),
        "findings-v2" => {
            let taxonomy = schema
                .get(FINDINGS_TAXONOMY_KEY)
                .and_then(serde_json::Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| crate::findings::CATEGORIES.map(str::to_string).to_vec());
            crate::findings::output_schema_for_taxonomy(&taxonomy)
        }
        "findings-v2-validation" => {
            let taxonomy = schema
                .get(FINDINGS_TAXONOMY_KEY)
                .and_then(serde_json::Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| crate::findings::CATEGORIES.map(str::to_string).to_vec());
            crate::findings::validation_output_schema_for_taxonomy(&taxonomy)
        }
        other => {
            return Err(format!(
                "$.{SCHEMA_REFERENCE_KEY}: unknown Pipeline schema reference '{other}'"
            ))
        }
    };
    if let (Some(source), Some(target)) = (schema.as_object(), resolved.as_object_mut()) {
        for (key, value) in source {
            if key.starts_with("x-pipeline-") && key != SCHEMA_REFERENCE_KEY {
                target.insert(key.clone(), value.clone());
            }
        }
    }
    validate_schema(&resolved)?;
    Ok(resolved)
}

/// Root-only marker on a step output schema naming an upstream step whose
/// findings ids the response must preserve as an ordered subsequence. The
/// check is host-owned: providers never see the keyword, and a violating
/// response is rejected and retried like any schema failure.
pub const PRESERVE_FINDINGS_KEY: &str = "x-pipeline-preserve-findings-from";

/// Provider structured-output implementations accept overlapping but not
/// identical JSON Schema dialects. Compile Pipeline's validated portable
/// subset into the common structural core and keep stricter checks such as
/// `uniqueItems` host-owned. The original schema remains authoritative after
/// the provider returns.
pub fn provider_schema(schema: &serde_json::Value) -> Result<serde_json::Value, String> {
    validate_schema(schema)?;
    let projected = project_provider_schema(schema, "$", 0)?;
    let mut enum_values = 0usize;
    validate_provider_enums(&projected, "$", &mut enum_values)?;
    let size = serde_json::to_vec(&projected)
        .map_err(|error| format!("Failed to serialize provider schema: {error}"))?
        .len();
    if size > MAX_PROVIDER_SCHEMA_BYTES {
        return Err(format!(
            "Provider schema is {size} bytes; the portable limit is {MAX_PROVIDER_SCHEMA_BYTES} bytes"
        ));
    }
    Ok(projected)
}

fn validate_provider_enums(
    schema: &serde_json::Value,
    path: &str,
    total: &mut usize,
) -> Result<(), String> {
    let object = schema
        .as_object()
        .ok_or_else(|| format!("{path}: schema must be a JSON object"))?;
    if let Some(values) = object.get("enum").and_then(serde_json::Value::as_array) {
        *total = total.saturating_add(values.len());
        if *total > MAX_PROVIDER_ENUM_VALUES {
            return Err(format!(
                "Provider schema contains more than {MAX_PROVIDER_ENUM_VALUES} enum values"
            ));
        }
        if values.len() > LARGE_ENUM_THRESHOLD {
            let string_chars = values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(|value| value.chars().count())
                .sum::<usize>();
            if string_chars > MAX_LARGE_ENUM_STRING_CHARS {
                return Err(format!(
                    "{path}.enum contains {string_chars} string characters; enums with more than {LARGE_ENUM_THRESHOLD} values are limited to {MAX_LARGE_ENUM_STRING_CHARS}"
                ));
            }
        }
    }
    if let Some(properties) = object
        .get("properties")
        .and_then(serde_json::Value::as_object)
    {
        for (key, child) in properties {
            validate_provider_enums(child, &format!("{path}.properties.{key}"), total)?;
        }
    }
    if let Some(items) = object.get("items") {
        validate_provider_enums(items, &format!("{path}.items"), total)?;
    }
    Ok(())
}

fn project_provider_schema(
    schema: &serde_json::Value,
    path: &str,
    depth: usize,
) -> Result<serde_json::Value, String> {
    if depth > MAX_SCHEMA_DEPTH {
        return Err(format!(
            "{path}: schema nesting exceeds the {MAX_SCHEMA_DEPTH}-level safety limit"
        ));
    }
    let source = schema
        .as_object()
        .ok_or_else(|| format!("{path}: schema must be a JSON object"))?;
    if source.contains_key("x-pipeline-catalog") || source.contains_key("x-pipeline-catalog-policy")
    {
        return Err(format!(
            "{path}: catalog references must be resolved before compiling a provider schema"
        ));
    }
    if source.contains_key(SCHEMA_REFERENCE_KEY) {
        return Err(format!(
            "{path}: schema references must be resolved before compiling a provider schema"
        ));
    }
    let mut projected = serde_json::Map::new();

    for key in ["type", "enum", "minItems", "maxItems"] {
        if let Some(value) = source.get(key) {
            projected.insert(key.to_string(), value.clone());
        }
    }
    for key in ["title", "description"] {
        if let Some(serde_json::Value::String(value)) = source.get(key) {
            projected.insert(key.to_string(), serde_json::Value::String(value.clone()));
        }
    }
    if let Some(required) = source.get("required") {
        projected.insert("required".to_string(), required.clone());
    }
    if let Some(properties) = source.get("properties").and_then(|value| value.as_object()) {
        let mut children = serde_json::Map::new();
        for (key, child) in properties {
            children.insert(
                key.clone(),
                project_provider_schema(child, &format!("{path}.properties.{key}"), depth + 1)?,
            );
        }
        projected.insert(
            "properties".to_string(),
            serde_json::Value::Object(children),
        );
    }
    if let Some(items) = source.get("items") {
        projected.insert(
            "items".to_string(),
            project_provider_schema(items, &format!("{path}.items"), depth + 1)?,
        );
    }

    Ok(serde_json::Value::Object(projected))
}

/// Serialize a provider-compatible schema for CLIs that accept it inline.
pub fn provider_schema_json(schema: &serde_json::Value) -> Result<String, String> {
    serde_json::to_string(&provider_schema(schema)?)
        .map_err(|error| format!("Failed to serialize output schema: {error}"))
}

/// Compile the projected schema into OpenAI's strict dialect: every object
/// carries `additionalProperties: false` with all declared properties
/// required, and originally-optional properties widen to accept null (the
/// host strips null-valued optional fields before validating the original
/// schema — see `strip_optional_nulls`). Codex submits its --output-schema
/// file to the strict Responses API, which rejects anything looser.
///
/// Returns None for valid portable schemas the strict dialect cannot express
/// (an object with no declared properties, an array without an item schema,
/// or an untyped subschema): those calls proceed without a native constraint
/// and rely on the response-contract prompt plus host validation.
fn strictify(value: &serde_json::Value) -> Option<serde_json::Value> {
    let object = value.as_object()?;
    let mut strict = object.clone();
    match object.get("type").and_then(serde_json::Value::as_str) {
        Some("object") => {
            let properties = object.get("properties")?.as_object()?;
            if properties.is_empty() {
                return None;
            }
            let required: std::collections::HashSet<&str> = object
                .get("required")
                .and_then(serde_json::Value::as_array)
                .map(|keys| keys.iter().filter_map(serde_json::Value::as_str).collect())
                .unwrap_or_default();
            let mut strict_properties = serde_json::Map::new();
            for (key, child) in properties {
                let mut child = strictify(child)?;
                if !required.contains(key.as_str()) {
                    widen_nullable(&mut child);
                }
                strict_properties.insert(key.clone(), child);
            }
            strict.insert(
                "required".to_string(),
                serde_json::Value::Array(
                    properties
                        .keys()
                        .map(|key| serde_json::Value::String(key.clone()))
                        .collect(),
                ),
            );
            strict.insert(
                "properties".to_string(),
                serde_json::Value::Object(strict_properties),
            );
            strict.insert(
                "additionalProperties".to_string(),
                serde_json::Value::Bool(false),
            );
        }
        Some("array") => {
            let items = strictify(object.get("items")?)?;
            strict.insert("items".to_string(), items);
        }
        Some(_) => {}
        None => return None,
    }
    Some(serde_json::Value::Object(strict))
}

fn widen_nullable(schema: &mut serde_json::Value) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    if let Some(serde_json::Value::String(declared)) = object.get("type") {
        if declared != "null" {
            object.insert(
                "type".to_string(),
                serde_json::json!([declared.clone(), "null"]),
            );
        }
    }
    if let Some(serde_json::Value::Array(values)) = object.get_mut("enum") {
        if !values.iter().any(serde_json::Value::is_null) {
            values.push(serde_json::Value::Null);
        }
    }
}

/// Remove null-valued optional properties before validating a provider
/// response against the original portable schema. Strict transports cannot
/// omit a declared property, so absent optional fields come back as null.
pub fn strip_optional_nulls(schema: &serde_json::Value, value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            let Some(properties) = schema
                .get("properties")
                .and_then(serde_json::Value::as_object)
            else {
                return;
            };
            let required: std::collections::HashSet<&str> = schema
                .get("required")
                .and_then(serde_json::Value::as_array)
                .map(|keys| keys.iter().filter_map(serde_json::Value::as_str).collect())
                .unwrap_or_default();
            map.retain(|key, entry| {
                !(entry.is_null()
                    && properties.contains_key(key)
                    && !required.contains(key.as_str()))
            });
            for (key, entry) in map.iter_mut() {
                if let Some(child) = properties.get(key) {
                    strip_optional_nulls(child, entry);
                }
            }
        }
        serde_json::Value::Array(items) => {
            if let Some(item_schema) = schema.get("items") {
                for item in items {
                    strip_optional_nulls(item_schema, item);
                }
            }
        }
        _ => {}
    }
}

/// A private strict-dialect schema file for the Codex CLI, or None when the
/// portable schema cannot be expressed strictly and the call should proceed
/// without a native constraint.
pub fn codex_schema(schema: &serde_json::Value) -> Result<Option<serde_json::Value>, String> {
    Ok(strictify(&provider_schema(schema)?))
}

pub fn prepare_codex_schema_file(
    schema: &serde_json::Value,
) -> Result<Option<PreparedSchemaFile>, String> {
    let Some(strict) = codex_schema(schema)? else {
        return Ok(None);
    };
    let temp_dir = tempfile::Builder::new()
        .prefix("pipeline_schema_")
        .tempdir()
        .map_err(|error| format!("Failed to create output-schema directory: {error}"))?;
    let path = temp_dir.path().join("output-schema.json");
    let file = std::fs::File::create(&path)
        .map_err(|error| format!("Failed to create output-schema file: {error}"))?;
    serde_json::to_writer(file, &strict)
        .map_err(|error| format!("Failed to write output-schema file: {error}"))?;
    Ok(Some(PreparedSchemaFile {
        path: path.to_string_lossy().replace('\\', "/"),
        read_root: temp_dir.path().to_string_lossy().replace('\\', "/"),
        _temp_dir: temp_dir,
    }))
}

/// A private schema file retained for the lifetime of a CLI invocation.
pub struct PreparedSchemaFile {
    pub path: String,
    pub read_root: String,
    _temp_dir: tempfile::TempDir,
}

pub fn prepare_provider_schema_file(
    schema: &serde_json::Value,
) -> Result<PreparedSchemaFile, String> {
    let projected = provider_schema(schema)?;
    let temp_dir = tempfile::Builder::new()
        .prefix("pipeline_schema_")
        .tempdir()
        .map_err(|error| format!("Failed to create output-schema directory: {error}"))?;
    let path = temp_dir.path().join("output-schema.json");
    let file = std::fs::File::create(&path)
        .map_err(|error| format!("Failed to create output-schema file: {error}"))?;
    serde_json::to_writer(file, &projected)
        .map_err(|error| format!("Failed to write output-schema file: {error}"))?;
    Ok(PreparedSchemaFile {
        path: path.to_string_lossy().replace('\\', "/"),
        read_root: temp_dir.path().to_string_lossy().replace('\\', "/"),
        _temp_dir: temp_dir,
    })
}

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

/// Validate the portable schema dialect. Artifact roots must be objects and
/// unknown keywords are rejected rather than silently ignored.
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
    for keyword in object.keys() {
        if !SUPPORTED_KEYWORDS.contains(&keyword.as_str()) {
            return Err(format!(
                "{path}.{keyword}: unsupported keyword in Pipeline's portable schema dialect"
            ));
        }
    }
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
    if depth == 0 && declared_type != Some("object") {
        return Err(format!(
            "{path}.type: artifact schema root must explicitly be 'object' for all-provider portability"
        ));
    }

    for keyword in ["title", "description"] {
        if object.get(keyword).is_some_and(|value| !value.is_string()) {
            return Err(format!("{path}.{keyword}: expected a string"));
        }
    }
    for keyword in [
        "x-pipeline-contract",
        "x-pipeline-catalog",
        "x-pipeline-catalog-policy",
        PRESERVE_FINDINGS_KEY,
        SCHEMA_REFERENCE_KEY,
        VALIDATION_LEDGER_KEY,
    ] {
        if object.get(keyword).is_some_and(|value| !value.is_string()) {
            return Err(format!("{path}.{keyword}: expected a string"));
        }
    }
    if depth > 0 {
        for keyword in [
            "x-pipeline-contract",
            "x-pipeline-catalog-policy",
            "x-pipeline-adaptive-agent-count",
            FINDINGS_VERSION_KEY,
            FINDINGS_TAXONOMY_KEY,
            VALIDATION_LEDGER_KEY,
            PRESERVE_FINDINGS_KEY,
            SCHEMA_REFERENCE_KEY,
        ] {
            if object.contains_key(keyword) {
                return Err(format!("{path}.{keyword}: only valid at the schema root"));
            }
        }
    }
    if object.contains_key(SCHEMA_REFERENCE_KEY) {
        // A reference stands in for the whole contract; structural keywords
        // beside it would be silently discarded on resolution.
        if let Some(extra) = object.keys().find(|key| {
            !key.starts_with("x-pipeline-")
                && !matches!(key.as_str(), "type" | "title" | "description")
        }) {
            return Err(format!(
                "{path}.{extra}: a schema reference replaces the whole contract; remove structural keywords or the {SCHEMA_REFERENCE_KEY} marker"
            ));
        }
    }
    if let Some(contract) = object
        .get("x-pipeline-contract")
        .and_then(serde_json::Value::as_str)
    {
        if contract != crate::auto_review::AUTO_REVIEW_CONTRACT {
            return Err(format!(
                "{path}.x-pipeline-contract: unsupported contract '{contract}'"
            ));
        }
    }
    if let Some(catalog) = object
        .get("x-pipeline-catalog")
        .and_then(serde_json::Value::as_str)
    {
        if declared_type != Some("string") {
            return Err(format!(
                "{path}.x-pipeline-catalog: catalog-backed values must have type 'string'"
            ));
        }
        if ![
            crate::auto_review::SUBJECT_CATALOG,
            crate::auto_review::METHOD_CATALOG,
            crate::auto_review::GENRE_CATALOG,
        ]
        .contains(&catalog)
        {
            return Err(format!(
                "{path}.x-pipeline-catalog: unknown Pipeline catalog '{catalog}'"
            ));
        }
    }
    if object
        .get("x-pipeline-catalog-policy")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|policy| policy != "live")
    {
        return Err(format!(
            "{path}.x-pipeline-catalog-policy: only 'live' is supported"
        ));
    }
    if object
        .get("x-pipeline-adaptive-agent-count")
        .is_some_and(|value| value.as_u64().is_none())
    {
        return Err(format!(
            "{path}.x-pipeline-adaptive-agent-count: expected a non-negative integer"
        ));
    }
    if object
        .get(FINDINGS_VERSION_KEY)
        .is_some_and(|value| value.as_u64().is_none())
    {
        return Err(format!(
            "{path}.{FINDINGS_VERSION_KEY}: expected a non-negative integer"
        ));
    }
    if let Some(taxonomy) = object.get(FINDINGS_TAXONOMY_KEY) {
        let taxonomy = taxonomy
            .as_array()
            .ok_or_else(|| format!("{path}.{FINDINGS_TAXONOMY_KEY}: expected an array"))?;
        if taxonomy.is_empty()
            || taxonomy
                .iter()
                .any(|value| value.as_str().is_none_or(|value| value.trim().is_empty()))
        {
            return Err(format!(
                "{path}.{FINDINGS_TAXONOMY_KEY}: expected non-empty strings"
            ));
        }
        let unique = taxonomy
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<std::collections::HashSet<_>>();
        if unique.len() != taxonomy.len() {
            return Err(format!(
                "{path}.{FINDINGS_TAXONOMY_KEY}: taxonomy values must be unique"
            ));
        }
    }

    if let Some(values) = object.get("enum") {
        let values = values
            .as_array()
            .ok_or_else(|| format!("{path}.enum: expected a non-empty array"))?;
        if values.is_empty() {
            return Err(format!("{path}.enum: expected a non-empty array"));
        }
        let mut seen = std::collections::HashSet::new();
        for (index, value) in values.iter().enumerate() {
            let encoded = serde_json::to_string(value)
                .map_err(|error| format!("{path}.enum[{index}]: {error}"))?;
            if !seen.insert(encoded) {
                return Err(format!("{path}.enum: duplicate value at index {index}"));
            }
            if let Some(ty) = declared_type {
                if !type_matches(ty, value) {
                    return Err(format!(
                        "{path}.enum[{index}]: expected type {ty}, got {}",
                        json_type(value)
                    ));
                }
            }
        }
    }

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
    for keyword in ["minItems", "maxItems"] {
        if let Some(limit) = object.get(keyword) {
            if declared_type.is_some_and(|ty| ty != "array") {
                return Err(format!("{path}.{keyword}: only valid for an array schema"));
            }
            if limit.as_u64().is_none() {
                return Err(format!("{path}.{keyword}: expected a non-negative integer"));
            }
        }
    }
    if let Some(limit) = object.get("minLength") {
        // Host-owned like uniqueItems: enforced on the returned value and
        // deliberately not projected into provider schemas.
        if declared_type.is_some_and(|ty| ty != "string") {
            return Err(format!("{path}.minLength: only valid for a string schema"));
        }
        if limit.as_u64().is_none() {
            return Err(format!("{path}.minLength: expected a non-negative integer"));
        }
    }
    if let (Some(min), Some(max)) = (
        object.get("minItems").and_then(|value| value.as_u64()),
        object.get("maxItems").and_then(|value| value.as_u64()),
    ) {
        if min > max {
            return Err(format!("{path}: minItems cannot exceed maxItems"));
        }
    }
    if let Some(unique) = object.get("uniqueItems") {
        if declared_type.is_some_and(|ty| ty != "array") {
            return Err(format!(
                "{path}.uniqueItems: only valid for an array schema"
            ));
        }
        if !unique.is_boolean() {
            return Err(format!("{path}.uniqueItems: expected a boolean"));
        }
    }
    Ok(())
}

/// Validate `value` against the supported subset of `schema`. Returns Ok(()) or
/// a human-readable reason on the first violation.
pub fn validate(schema: &serde_json::Value, value: &serde_json::Value) -> Result<(), String> {
    validate_schema(schema)?;
    validate_at(schema, value, "$")?;
    crate::findings::validate_v2_semantics(schema, value)
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
    if let Some(allowed) = schema.get("enum").and_then(|entry| entry.as_array()) {
        if !allowed.iter().any(|candidate| candidate == value) {
            return Err(format!("{path}: value is not in the allowed enum"));
        }
    }
    if let (Some(minimum), Some(text)) = (
        schema.get("minLength").and_then(|entry| entry.as_u64()),
        value.as_str(),
    ) {
        if (text.chars().count() as u64) < minimum {
            return Err(format!(
                "{path}: expected at least {minimum} characters, got {}",
                text.chars().count()
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
            if let Some(minimum) = schema.get("minItems").and_then(|entry| entry.as_u64()) {
                if (items.len() as u64) < minimum {
                    return Err(format!(
                        "{path}: expected at least {minimum} items, got {}",
                        items.len()
                    ));
                }
            }
            if let Some(maximum) = schema.get("maxItems").and_then(|entry| entry.as_u64()) {
                if (items.len() as u64) > maximum {
                    return Err(format!(
                        "{path}: expected at most {maximum} items, got {}",
                        items.len()
                    ));
                }
            }
            if schema
                .get("uniqueItems")
                .and_then(|entry| entry.as_bool())
                .unwrap_or(false)
            {
                let mut seen = std::collections::HashSet::new();
                for (index, item) in items.iter().enumerate() {
                    let encoded = serde_json::to_string(item)
                        .map_err(|error| format!("{path}[{index}]: {error}"))?;
                    if !seen.insert(encoded) {
                        return Err(format!("{path}: duplicate array item at index {index}"));
                    }
                }
            }
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

/// Parse a provider's structured result as one exact JSON value, validate the
/// original Pipeline schema, and return host-owned canonical JSON. Unlike the
/// legacy extractor this deliberately rejects fences and surrounding prose.
/// Null-valued optional fields (a strict transport's spelling of "absent")
/// are removed before validation and never reach the durable artifact.
pub fn canonicalize(schema: &serde_json::Value, text: &str) -> Result<String, String> {
    let mut value = serde_json::from_str::<serde_json::Value>(text.trim())
        .map_err(|error| format!("output is not one complete JSON value: {error}"))?;
    strip_optional_nulls(schema, &mut value);
    validate(schema, &value)?;
    serde_json::to_string_pretty(&value)
        .map_err(|error| format!("failed to canonicalize structured output: {error}"))
}

#[cfg(test)]
mod tests;
