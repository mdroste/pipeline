use super::*;
use sha2::{Digest as _, Sha256};

/// A validated, normalized, portable workflow. The canonical JSON is the
/// immutable representation embedded in run artifacts and hashed for
/// provenance; it never includes user settings or credentials.
#[derive(Debug, Clone)]
pub struct WorkflowDocument {
    pub schema_version: u32,
    pub name: String,
    pub config: PipelineConfig,
    pub canonical_json: String,
    pub fingerprint: String,
}

impl WorkflowDocument {
    pub fn from_profile_data(profile: ProfileData) -> Result<Self, String> {
        validation::validate_profile_data(&profile)?;
        validate_enabled_sequential_step(&profile.steps)?;
        let name = profile.name.clone();
        let config: PipelineConfig = profile.clone().into();
        let envelope = ExportEnvelope::Profile {
            schema_version: CURRENT_SCHEMA_VERSION,
            name: profile.name,
            steps: profile.steps,
            merge: profile.merge,
            outputs: profile.outputs,
            context_cache: profile.context_cache,
            use_orientation: true,
            orientation_prompt: profile.orientation_prompt,
            orientation_schema: profile.orientation_schema,
            extraction: profile.extraction,
            parallel_context_template: profile.parallel_context_template,
            variables: profile.variables,
        };
        let canonical_json = canonical_pretty_json(&envelope)?;
        let fingerprint = workflow_fingerprint(canonical_json.as_bytes());
        Ok(Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            name,
            config,
            canonical_json,
            fingerprint,
        })
    }
}

fn workflow_fingerprint(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("sha256:{digest:x}")
}

fn sort_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            let mut entries = std::mem::take(object).into_iter().collect::<Vec<_>>();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            for (key, mut child) in entries {
                sort_json(&mut child);
                object.insert(key, child);
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(sort_json),
        _ => {}
    }
}

fn canonical_pretty_json<T: Serialize>(value: &T) -> Result<String, String> {
    let mut value = serde_json::to_value(value)
        .map_err(|error| format!("Could not normalize workflow: {error}"))?;
    sort_json(&mut value);
    let mut encoded = serde_json::to_string_pretty(&value)
        .map_err(|error| format!("Could not encode workflow: {error}"))?;
    encoded.push('\n');
    Ok(encoded)
}

fn object<'a>(
    value: &'a serde_json::Value,
    path: &str,
) -> Result<&'a serde_json::Map<String, serde_json::Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{path} must be a JSON object"))
}

fn reject_unknown(value: &serde_json::Value, path: &str, allowed: &[&str]) -> Result<(), String> {
    for key in object(value, path)?.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("Unknown workflow field '{path}.{key}'"));
        }
    }
    Ok(())
}

fn each_array<F>(
    parent: &serde_json::Value,
    key: &str,
    path: &str,
    mut validate: F,
) -> Result<(), String>
where
    F: FnMut(&serde_json::Value, &str) -> Result<(), String>,
{
    let Some(value) = parent.get(key) else {
        return Ok(());
    };
    let items = value
        .as_array()
        .ok_or_else(|| format!("{path}.{key} must be an array"))?;
    for (index, item) in items.iter().enumerate() {
        validate(item, &format!("{path}.{key}[{index}]"))?;
    }
    Ok(())
}

fn validate_model_selection(value: &serde_json::Value, path: &str) -> Result<(), String> {
    reject_unknown(value, path, &["mode", "role", "model"])?;
    let fields = object(value, path)?.len();
    match value.get("mode").and_then(serde_json::Value::as_str) {
        Some("automatic") if fields == 1 => Ok(()),
        Some("role")
            if fields == 2
                && value
                    .get("role")
                    .and_then(serde_json::Value::as_str)
                    .is_some() =>
        {
            Ok(())
        }
        Some("pinned")
            if fields == 2
                && value
                    .get("model")
                    .and_then(serde_json::Value::as_str)
                    .is_some() =>
        {
            Ok(())
        }
        _ => Err(format!(
            "{path} must be an automatic, role, or pinned model selection"
        )),
    }
}

fn validate_model_map(value: &serde_json::Value, path: &str) -> Result<(), String> {
    for (key, selection) in object(value, path)? {
        validate_model_selection(selection, &format!("{path}.{key}"))?;
    }
    Ok(())
}

fn validate_selector(value: &serde_json::Value, path: &str) -> Result<(), String> {
    match value.get("kind").and_then(serde_json::Value::as_str) {
        Some("primary") => reject_unknown(value, path, &["kind", "parts"]),
        Some("survey") => reject_unknown(value, path, &["kind"]),
        Some("named_input") => reject_unknown(value, path, &["kind", "key", "parts"]),
        Some("step") => reject_unknown(value, path, &["kind", "step", "parts", "glob"]),
        Some(kind) => Err(format!("{path}.kind has unsupported selector '{kind}'")),
        None => Err(format!("{path}.kind is required")),
    }
}

fn validate_condition(value: &serde_json::Value, path: &str) -> Result<(), String> {
    match value.get("kind").and_then(serde_json::Value::as_str) {
        Some("output_matches") => {
            reject_unknown(value, path, &["kind", "step", "pattern", "negate"])
        }
        Some("survey_path") => reject_unknown(
            value,
            path,
            &["kind", "pointer", "equals", "exists", "contains"],
        ),
        Some(kind) => Err(format!("{path}.kind has unsupported condition '{kind}'")),
        None => Err(format!("{path}.kind is required")),
    }
}

fn validate_step(value: &serde_json::Value, path: &str) -> Result<(), String> {
    reject_unknown(
        value,
        path,
        &[
            "id",
            "label",
            "prompt",
            "system_prompt",
            "enabled",
            "phase",
            "tools",
            "agents",
            "model",
            "model_overrides",
            "effort",
            "effort_overrides",
            "after",
            "dependency_policy",
            "context",
            "run_if",
            "output_schema",
            "for_each",
        ],
    )?;
    if let Some(overrides) = value.get("model_overrides") {
        validate_model_map(overrides, &format!("{path}.model_overrides"))?;
    }
    if let Some(policy) = value.get("dependency_policy") {
        reject_unknown(
            policy,
            &format!("{path}.dependency_policy"),
            &["required", "quorum", "minimum_successes"],
        )?;
    }
    if let Some(context) = value.get("context") {
        let context_path = format!("{path}.context");
        reject_unknown(context, &context_path, &["include"])?;
        each_array(context, "include", &context_path, validate_selector)?;
    }
    if let Some(condition) = value.get("run_if") {
        validate_condition(condition, &format!("{path}.run_if"))?;
    }
    if let Some(fan_out) = value.get("for_each") {
        let fan_out_path = format!("{path}.for_each");
        reject_unknown(fan_out, &fan_out_path, &["glob", "max", "artifact"])?;
        if let Some(artifact) = fan_out.get("artifact") {
            reject_unknown(
                artifact,
                &format!("{fan_out_path}.artifact"),
                &["step", "pointer"],
            )?;
        }
    }
    Ok(())
}

fn validate_strict_shape(value: &serde_json::Value) -> Result<(), String> {
    reject_unknown(
        value,
        "$",
        &[
            "type",
            "schema_version",
            "name",
            "steps",
            "merge",
            "outputs",
            "context_cache",
            "use_orientation",
            "orientation_prompt",
            "orientation_schema",
            "extraction",
            "parallel_context_template",
            "variables",
        ],
    )?;
    if value.get("type").and_then(serde_json::Value::as_str) != Some("profile") {
        return Err("Workflow JSON must have top-level type 'profile'".to_string());
    }
    each_array(value, "steps", "$", validate_step)?;
    if let Some(merge) = value.get("merge") {
        reject_unknown(merge, "$.merge", &["enabled", "prompt", "agents"])?;
    }
    if let Some(outputs) = value.get("outputs") {
        reject_unknown(
            outputs,
            "$.outputs",
            &["primary_step", "findings_step", "named"],
        )?;
        each_array(outputs, "named", "$.outputs", |product, path| {
            reject_unknown(
                product,
                path,
                &[
                    "key",
                    "step",
                    "media_type",
                    "viewer",
                    "export_policy",
                    "sensitivity",
                    "schema",
                ],
            )
        })?;
    }
    if let Some(cache) = value.get("context_cache") {
        reject_unknown(cache, "$.context_cache", &["enabled"])?;
    }
    if let Some(extraction) = value.get("extraction") {
        reject_unknown(
            extraction,
            "$.extraction",
            &["method", "input_mode", "extra_inputs"],
        )?;
        each_array(extraction, "extra_inputs", "$.extraction", |slot, path| {
            reject_unknown(
                slot,
                path,
                &[
                    "key",
                    "label",
                    "mode",
                    "required",
                    "extensions",
                    "mime_types",
                    "max_bytes",
                    "sensitivity",
                ],
            )
        })?;
    }
    each_array(value, "variables", "$", |variable, path| {
        reject_unknown(
            variable,
            path,
            &[
                "key",
                "label",
                "kind",
                "default",
                "choices",
                "required",
                "secret",
                "validation",
            ],
        )
        .and_then(|()| {
            if let Some(validation) = variable.get("validation") {
                reject_unknown(
                    validation,
                    &format!("{path}.validation"),
                    &["min_length", "max_length", "pattern"],
                )?;
            }
            Ok(())
        })
    })?;
    Ok(())
}

/// Parse the portable profile envelope used as a workflow document by agents
/// and the CLI. Unlike the GUI's historical import path, this entry point is
/// intentionally strict so misspelled generated fields cannot be ignored.
pub fn parse_workflow_document_strict(json: &str) -> Result<WorkflowDocument, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|error| format!("Invalid workflow JSON: {error}"))?;
    validate_strict_shape(&value)?;
    let envelope: ExportEnvelope = serde_json::from_value(value)
        .map_err(|error| format!("Invalid workflow document: {error}"))?;
    let ExportEnvelope::Profile {
        schema_version,
        name,
        steps,
        merge,
        outputs,
        context_cache,
        use_orientation,
        orientation_prompt,
        orientation_schema,
        extraction,
        parallel_context_template,
        variables,
    } = envelope
    else {
        return Err("Workflow JSON must contain a profile export".to_string());
    };
    if schema_version == 0 || schema_version > CURRENT_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported workflow schema v{schema_version}; this Pipeline supports v1–v{CURRENT_SCHEMA_VERSION}"
        ));
    }
    let profile = ProfileData {
        name,
        steps,
        merge,
        outputs,
        context_cache,
        use_orientation,
        orientation_prompt,
        orientation_schema,
        extraction,
        parallel_context_template,
        variables,
    };
    let document = WorkflowDocument::from_profile_data(profile)?;
    // Catch obviously excessive generated workflows during standalone
    // validation. The run/check path repeats this with the user's exact
    // retry, fallback, and inherited-agent settings.
    let mut budget_config = document.config.clone();
    let default_settings = crate::settings::Settings::default();
    apply_agent_defaults(&mut budget_config, &default_settings);
    crate::safety::validate_run_budget(&budget_config, &default_settings)?;
    Ok(document)
}

/// A small, valid workflow agents can modify rather than inventing the wire
/// format. Host defaults remain explicit in the normalized output.
pub fn workflow_template() -> Result<WorkflowDocument, String> {
    let analysis = StepConfig {
        id: "analysis".into(),
        label: "Independent analysis".into(),
        prompt: "Analyze the document for the requested question. State concrete findings, cite the relevant passages, and distinguish errors from judgment calls.".into(),
        enabled: true,
        phase: Phase::Parallel,
        agents: vec!["claude".into(), "codex".into()],
        context: StepContext {
            include: vec![ArtifactSelector::Primary {
                parts: vec![PrimaryArtifactPart::Text, PrimaryArtifactPart::Visuals],
            }],
        },
        ..Default::default()
    };
    let synthesis = StepConfig {
        id: "synthesis".into(),
        label: "Synthesis".into(),
        prompt: "Synthesize the independent analyses into a single prioritized report. Resolve disagreements explicitly and preserve supporting evidence.\n\n{prior_outputs}".into(),
        enabled: true,
        phase: Phase::Sequential,
        agents: vec!["claude".into()],
        context: StepContext {
            include: vec![ArtifactSelector::Step {
                step: "analysis".into(),
                parts: vec![StepArtifactPart::Report],
                glob: String::new(),
            }],
        },
        ..Default::default()
    };
    WorkflowDocument::from_profile_data(ProfileData {
        name: "Generated workflow".into(),
        steps: vec![analysis, synthesis],
        merge: MergeConfig {
            enabled: true,
            prompt: "Merge the independent analyses of {topic} into one evidence-based report. Reconcile contradictions, combine duplicate findings, preserve well-supported minority findings, and do not discuss the merge process.\n\n{agent_reports}".into(),
            agents: Vec::new(),
        },
        outputs: OutputConfig {
            primary_step: "synthesis".into(),
            findings_step: String::new(),
            named: Vec::new(),
        },
        context_cache: ContextCacheConfig::default(),
        use_orientation: true,
        orientation_prompt: crate::prompts::compiled_default("orientation_generic")
            .unwrap_or_default()
            .to_string(),
        orientation_schema: Some(crate::orientation_contract::generic_schema()),
        extraction: ExtractionConfig {
            input_mode: "document".into(),
            ..Default::default()
        },
        parallel_context_template: crate::prompts::compiled_default("parallel_context_generic")
            .unwrap_or_default()
            .to_string(),
        variables: Vec::new(),
    })
}

/// Draft 2020-12 schema for agent tooling and editor integrations. Semantic
/// graph and safety checks remain authoritative in `workflow validate`.
pub fn workflow_json_schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://pipeline.local/schemas/workflow-v11.json",
        "title": "Pipeline portable workflow",
        "type": "object",
        "additionalProperties": false,
        "required": ["type", "schema_version", "name", "steps"],
        "properties": {
            "type": { "const": "profile" },
            "schema_version": { "const": CURRENT_SCHEMA_VERSION },
            "name": { "type": "string", "minLength": 1, "maxLength": MAX_PROFILE_NAME_CHARS },
            "steps": { "type": "array", "maxItems": MAX_PROFILE_STEPS, "items": { "$ref": "#/$defs/step" } },
            "merge": { "$ref": "#/$defs/merge" },
            "outputs": { "$ref": "#/$defs/outputs" },
            "context_cache": { "$ref": "#/$defs/contextCache" },
            "use_orientation": { "type": "boolean" },
            "orientation_prompt": { "type": "string" },
            "orientation_schema": { "oneOf": [{ "$ref": "#/$defs/artifactSchema" }, { "type": "null" }] },
            "extraction": { "$ref": "#/$defs/extraction" },
            "parallel_context_template": { "type": "string" },
            "variables": { "type": "array", "maxItems": MAX_VARIABLES, "items": { "$ref": "#/$defs/variable" } }
        },
        "$defs": {
            "schemaNode": {
                "type": "object",
                "additionalProperties": false,
                "allOf": [
                    {
                        "if": { "required": ["x-pipeline-adaptive-agent-count"] },
                        "then": {
                            "required": ["x-pipeline-contract"],
                            "properties": { "x-pipeline-contract": { "const": crate::auto_review::AUTO_REVIEW_CONTRACT } }
                        }
                    },
                    {
                        "if": { "required": [crate::pipeline::structured::SCHEMA_REFERENCE_KEY] },
                        "then": {
                            "not": {
                                "anyOf": [
                                    { "required": ["enum"] },
                                    { "required": ["minItems"] },
                                    { "required": ["maxItems"] },
                                    { "required": ["minLength"] },
                                    { "required": ["uniqueItems"] },
                                    { "required": ["required"] },
                                    { "required": ["properties"] },
                                    { "required": ["items"] }
                                ]
                            }
                        }
                    }
                ],
                "properties": {
                    "type": { "enum": ["object", "array", "string", "number", "integer", "boolean", "null"] },
                    "enum": { "type": "array", "minItems": 1, "uniqueItems": true },
                    "minItems": { "type": "integer", "minimum": 0 },
                    "maxItems": { "type": "integer", "minimum": 0 },
                    "minLength": { "type": "integer", "minimum": 0 },
                    "uniqueItems": { "type": "boolean" },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "required": { "type": "array", "uniqueItems": true, "items": { "type": "string" } },
                    "properties": { "type": "object", "additionalProperties": { "$ref": "#/$defs/schemaNode" } },
                    "items": { "$ref": "#/$defs/schemaNode" },
                    "x-pipeline-contract": { "const": crate::auto_review::AUTO_REVIEW_CONTRACT },
                    "x-pipeline-catalog": { "enum": ["auto-review.subjects", "auto-review.methods", "auto-review.genres"] },
                    "x-pipeline-catalog-policy": { "const": "live" },
                    "x-pipeline-adaptive-agent-count": { "type": "integer", "minimum": 0 },
                    "x-pipeline-findings-version": { "type": "integer", "minimum": 1 },
                    "x-pipeline-findings-taxonomy": { "type": "array", "minItems": 1, "uniqueItems": true, "items": { "type": "string", "minLength": 1 } },
                    "x-pipeline-validation-ledger": { "type": "string", "minLength": 1 },
                    "x-pipeline-preserve-findings-from": { "type": "string", "minLength": 1 },
                    "x-pipeline-schema": { "enum": ["findings-v1", "findings-v2", "findings-v2-validation"] }
                }
            },
            "artifactSchema": {
                "allOf": [
                    { "$ref": "#/$defs/schemaNode" },
                    { "required": ["type"], "properties": { "type": { "const": "object" } } }
                ]
            },
            "stepArtifactSchema": {
                "allOf": [
                    { "$ref": "#/$defs/artifactSchema" },
                    {
                        "not": {
                            "anyOf": [
                                { "required": ["x-pipeline-contract"] },
                                { "required": ["x-pipeline-catalog"] },
                                { "required": ["x-pipeline-catalog-policy"] },
                                { "required": ["x-pipeline-adaptive-agent-count"] }
                            ]
                        }
                    }
                ]
            },
            "agent": { "enum": ALLOWED_AGENTS },
            "modelSelection": {
                "oneOf": [
                    { "type": "object", "additionalProperties": false, "required": ["mode"], "properties": { "mode": { "const": "automatic" } } },
                    { "type": "object", "additionalProperties": false, "required": ["mode", "role"], "properties": { "mode": { "const": "role" }, "role": { "type": "string" } } },
                    { "type": "object", "additionalProperties": false, "required": ["mode", "model"], "properties": { "mode": { "const": "pinned" }, "model": { "type": "string" } } }
                ]
            },
            "selector": {
                "oneOf": [
                    { "type": "object", "additionalProperties": false, "required": ["kind", "parts"], "properties": { "kind": { "const": "primary" }, "parts": { "type": "array", "items": { "enum": ["text", "structure", "visuals", "source"] } } } },
                    { "type": "object", "additionalProperties": false, "required": ["kind"], "properties": { "kind": { "const": "survey" } } },
                    { "type": "object", "additionalProperties": false, "required": ["kind", "key", "parts"], "properties": { "kind": { "const": "named_input" }, "key": { "type": "string" }, "parts": { "type": "array", "items": { "enum": ["text", "source"] } } } },
                    { "type": "object", "additionalProperties": false, "required": ["kind", "step", "parts"], "properties": { "kind": { "const": "step" }, "step": { "type": "string" }, "parts": { "type": "array", "items": { "enum": ["report", "files"] } }, "glob": { "type": "string" } } }
                ]
            },
            "condition": {
                "oneOf": [
                    { "type": "object", "additionalProperties": false, "required": ["kind", "step", "pattern"], "properties": { "kind": { "const": "output_matches" }, "step": { "type": "string" }, "pattern": { "type": "string" }, "negate": { "type": "boolean" } } },
                    { "type": "object", "additionalProperties": false, "required": ["kind", "pointer"], "properties": { "kind": { "const": "survey_path" }, "pointer": { "type": "string" }, "equals": {}, "exists": { "type": "boolean" }, "contains": {} } }
                ]
            },
            "step": {
                "type": "object", "additionalProperties": false,
                "required": ["id", "label", "prompt", "enabled", "phase"],
                "properties": {
                    "id": { "type": "string", "pattern": "^[A-Za-z0-9._-]{1,64}$" },
                    "label": { "type": "string" }, "prompt": { "type": "string" }, "system_prompt": { "type": "string" },
                    "enabled": { "type": "boolean" }, "phase": { "enum": ["parallel", "sequential"] },
                    "tools": { "type": "array", "uniqueItems": true, "items": { "enum": ALLOWED_TOOLS } },
                    "agents": { "type": "array", "uniqueItems": true, "items": { "$ref": "#/$defs/agent" } },
                    "model": { "type": "string" },
                    "model_overrides": { "type": "object", "additionalProperties": { "$ref": "#/$defs/modelSelection" } },
                    "effort": { "type": "string" }, "effort_overrides": { "type": "object", "additionalProperties": { "type": "string" } },
                    "after": { "type": "array", "items": { "type": "string" } },
                    "dependency_policy": { "type": "object", "additionalProperties": false, "properties": { "required": { "type": "array", "uniqueItems": true, "items": { "type": "string" } }, "quorum": { "type": "array", "uniqueItems": true, "items": { "type": "string" } }, "minimum_successes": { "type": "integer", "minimum": 0 } } },
                    "context": { "type": "object", "additionalProperties": false, "properties": { "include": { "type": "array", "items": { "$ref": "#/$defs/selector" } } } },
                    "run_if": { "$ref": "#/$defs/condition" },
                    "output_schema": { "oneOf": [{ "$ref": "#/$defs/stepArtifactSchema" }, { "type": "null" }] },
                    "for_each": { "type": "object", "additionalProperties": false, "properties": { "glob": { "type": "string" }, "max": { "type": "integer", "minimum": 1, "maximum": MAX_FAN_OUT_ITEMS }, "artifact": { "type": "object", "additionalProperties": false, "required": ["step"], "properties": { "step": { "type": "string" }, "pointer": { "type": "string" } } } } }
                }
            },
            "merge": { "type": "object", "additionalProperties": false, "required": ["enabled", "prompt"], "properties": { "enabled": { "type": "boolean" }, "prompt": { "type": "string" }, "agents": { "type": "array", "maxItems": 1, "items": { "$ref": "#/$defs/agent" } } } },
            "outputs": { "type": "object", "additionalProperties": false, "properties": { "primary_step": { "type": "string" }, "findings_step": { "type": "string" }, "named": { "type": "array", "items": { "$ref": "#/$defs/namedProduct" } } } },
            "namedProduct": { "type": "object", "additionalProperties": false, "required": ["key", "step"], "properties": { "key": { "type": "string", "pattern": "^[A-Za-z0-9_]{1,64}$" }, "step": { "type": "string" }, "media_type": { "type": "string" }, "viewer": { "enum": ["", "text", "markdown", "json", "artifact"] }, "export_policy": { "enum": ["", "full", "redacted", "disabled"] }, "sensitivity": { "enum": ["", "public", "internal", "confidential", "secret"] }, "schema": { "oneOf": [{ "$ref": "#/$defs/artifactSchema" }, { "type": "null" }] } } },
            "contextCache": { "type": "object", "additionalProperties": false, "properties": { "enabled": { "type": "boolean" } } },
            "inputSlot": { "type": "object", "additionalProperties": false, "required": ["key"], "properties": { "key": { "type": "string" }, "label": { "type": "string" }, "mode": { "enum": ["document", "folder"] }, "required": { "type": "boolean" }, "extensions": { "type": "array", "uniqueItems": true, "items": { "type": "string", "pattern": "^[a-z0-9]+$" } }, "mime_types": { "type": "array", "uniqueItems": true, "items": { "type": "string", "pattern": "^[^/]+/[^/]+$" } }, "max_bytes": { "type": "integer", "minimum": 0, "maximum": 268435456 }, "sensitivity": { "enum": ["", "public", "internal", "confidential", "secret"] } } },
            "extraction": { "type": "object", "additionalProperties": false, "properties": { "method": { "enum": ["", "auto", "llm", "paddleocr-vl-full", "pdftotext"] }, "input_mode": { "enum": ["", "document", "folder", "none"] }, "extra_inputs": { "type": "array", "maxItems": MAX_EXTRA_INPUTS, "items": { "$ref": "#/$defs/inputSlot" } } } },
            "variableValidation": { "type": "object", "additionalProperties": false, "properties": { "min_length": { "type": "integer", "minimum": 0 }, "max_length": { "type": "integer", "minimum": 0, "maximum": 1048576 }, "pattern": { "type": "string" } } },
            "variable": { "type": "object", "additionalProperties": false, "required": ["key"], "properties": { "key": { "type": "string" }, "label": { "type": "string" }, "kind": { "enum": ["text", "choice", "file"] }, "default": { "type": "string" }, "choices": { "type": "array", "items": { "type": "string" } }, "required": { "type": "boolean" }, "secret": { "type": "boolean" }, "validation": { "$ref": "#/$defs/variableValidation" } } }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_round_trips_through_strict_parser() {
        let template = workflow_template().unwrap();
        let reparsed = parse_workflow_document_strict(&template.canonical_json).unwrap();
        assert_eq!(template.fingerprint, reparsed.fingerprint);
        assert_eq!(reparsed.config.outputs.primary_step, "synthesis");
    }

    #[test]
    fn strict_parser_rejects_unknown_generated_fields() {
        let mut value: serde_json::Value =
            serde_json::from_str(&workflow_template().unwrap().canonical_json).unwrap();
        value["steps"][0]["agnet"] = serde_json::json!(["claude"]);
        let error = parse_workflow_document_strict(&value.to_string()).unwrap_err();
        assert!(error.contains("agnet"), "{error}");
    }

    #[test]
    fn reviewer_instructions_are_portable_literal_and_fingerprinted() {
        let template = workflow_template().unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&template.canonical_json).unwrap();
        value["steps"][0]["system_prompt"] =
            serde_json::json!("Review evidence. Literal {notation} is allowed.");
        let parsed = parse_workflow_document_strict(&value.to_string()).unwrap();
        assert_ne!(parsed.fingerprint, template.fingerprint);
        let normalized: serde_json::Value = serde_json::from_str(&parsed.canonical_json).unwrap();
        assert_eq!(
            normalized["steps"][0]["system_prompt"],
            value["steps"][0]["system_prompt"]
        );
        let schema = workflow_json_schema();
        assert_eq!(
            schema["$defs"]["step"]["properties"]["system_prompt"]["type"],
            "string"
        );
    }

    #[test]
    fn canonical_hash_ignores_json_object_key_order() {
        let template = workflow_template().unwrap();
        let compact: serde_json::Value = serde_json::from_str(&template.canonical_json).unwrap();
        let reparsed = parse_workflow_document_strict(&compact.to_string()).unwrap();
        assert_eq!(template.fingerprint, reparsed.fingerprint);
    }

    #[test]
    fn strict_parser_rejects_nonportable_artifact_schemas() {
        let mut value: serde_json::Value =
            serde_json::from_str(&workflow_template().unwrap().canonical_json).unwrap();
        value["steps"][0]["output_schema"] = serde_json::json!({
            "type": "object",
            "additionalProperties": false
        });
        let error = parse_workflow_document_strict(&value.to_string()).unwrap_err();
        assert!(error.contains("unsupported keyword"), "{error}");

        value["steps"][0]["output_schema"] = serde_json::json!({
            "type": "array",
            "items": {"type": "string"}
        });
        let error = parse_workflow_document_strict(&value.to_string()).unwrap_err();
        assert!(
            error.contains("root must explicitly be 'object'"),
            "{error}"
        );
    }

    #[test]
    fn published_workflow_schema_references_the_portable_artifact_dialect() {
        let schema = workflow_json_schema();
        assert_eq!(
            schema.pointer("/properties/orientation_schema/oneOf/0/$ref"),
            Some(&serde_json::json!("#/$defs/artifactSchema"))
        );
        assert_eq!(
            schema.pointer("/$defs/step/properties/output_schema/oneOf/0/$ref"),
            Some(&serde_json::json!("#/$defs/stepArtifactSchema"))
        );
        assert_eq!(
            schema.pointer("/$defs/artifactSchema/allOf/1/properties/type/const"),
            Some(&serde_json::json!("object"))
        );
        assert_eq!(
            schema.pointer("/$defs/schemaNode/additionalProperties"),
            Some(&serde_json::json!(false))
        );
        assert_eq!(
            schema.pointer("/$defs/schemaNode/properties/minLength/type"),
            Some(&serde_json::json!("integer"))
        );
        assert_eq!(
            schema.pointer("/$defs/schemaNode/properties/x-pipeline-schema/enum"),
            Some(&serde_json::json!([
                "findings-v1",
                "findings-v2",
                "findings-v2-validation"
            ]))
        );

        let validator = jsonschema::draft202012::options().build(&schema).unwrap();
        let mut workflow: serde_json::Value =
            serde_json::from_str(&workflow_template().unwrap().canonical_json).unwrap();
        validator.validate(&workflow).unwrap();

        workflow["steps"][0]["output_schema"] = serde_json::json!({
            "type": "object",
            "additionalProperties": false
        });
        assert!(validator.validate(&workflow).is_err());
        workflow["steps"][0]["output_schema"] = serde_json::json!({
            "type": "array",
            "items": {"type": "string"}
        });
        assert!(validator.validate(&workflow).is_err());

        let auto_review = WorkflowDocument::from_profile_data(auto_review_profile()).unwrap();
        let auto_review: serde_json::Value =
            serde_json::from_str(&auto_review.canonical_json).unwrap();
        validator.validate(&auto_review).unwrap();
    }
}
