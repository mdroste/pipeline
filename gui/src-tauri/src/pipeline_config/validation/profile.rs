//! Ordered profile validation and the stricter runtime admission boundary.

use super::super::{
    PipelineConfig, ProfileData, ALLOWED_AGENTS, MAX_EXTRA_INPUTS, MAX_OUTPUT_SCHEMA_BYTES,
    MAX_PROFILE_NAME_CHARS, MAX_PROFILE_TEXT_BYTES, MAX_VARIABLES,
};
use super::artifacts::{validate_artifact_context, validate_published_outputs};
use super::steps::{validate_enabled_sequential_step, validate_profile_steps};

pub(in crate::pipeline_config) fn validate_profile_data(
    profile: &ProfileData,
) -> Result<(), String> {
    if profile.name.trim().is_empty() || profile.name.chars().count() > MAX_PROFILE_NAME_CHARS {
        return Err(format!(
            "Profile name must contain 1–{MAX_PROFILE_NAME_CHARS} characters"
        ));
    }
    if !profile.use_orientation {
        return Err("Every workflow must build an orientation map".to_string());
    }
    validate_profile_steps(&profile.steps)?;
    crate::auto_review::validate_profile_contract_identity(
        &profile.steps,
        profile.orientation_schema.as_ref(),
    )?;
    validate_artifact_context(profile)?;
    validate_published_outputs(profile)?;

    for (label, value) in [
        ("orientation prompt", profile.orientation_prompt.as_str()),
        (
            "parallel context template",
            profile.parallel_context_template.as_str(),
        ),
        ("merge prompt", profile.merge.prompt.as_str()),
    ] {
        if value.len() > MAX_PROFILE_TEXT_BYTES {
            return Err(format!(
                "Profile {label} exceeds the {} MB safety limit",
                MAX_PROFILE_TEXT_BYTES / 1024 / 1024
            ));
        }
    }
    if let Some(schema) = &profile.orientation_schema {
        let bytes = serde_json::to_vec(schema)
            .map_err(|error| format!("Failed to serialize orientation schema: {error}"))?;
        if bytes.len() > MAX_OUTPUT_SCHEMA_BYTES {
            return Err(format!(
                "Profile orientation schema exceeds the {} MB safety limit",
                MAX_OUTPUT_SCHEMA_BYTES / 1024 / 1024
            ));
        }
        crate::auto_review::validate_schema_settings(schema)
            .map_err(|error| format!("Invalid adaptive-review setting: {error}"))?;
        let resolved = crate::auto_review::resolve_schema_catalogs(schema)
            .map_err(|error| format!("Invalid orientation schema catalog reference: {error}"))?;
        crate::pipeline::structured::provider_schema(&resolved)
            .map_err(|error| format!("Invalid orientation schema: {error}"))?;
    }
    if profile.merge.agents.len() > 1 {
        return Err("Merge supports at most one selected agent".to_string());
    }
    for agent in &profile.merge.agents {
        if !ALLOWED_AGENTS.contains(&agent.as_str()) {
            return Err(format!(
                "Merge names unsupported agent '{}'. Allowed agents: {}",
                agent,
                ALLOWED_AGENTS.join(", ")
            ));
        }
    }

    if !matches!(
        profile.extraction.method.as_str(),
        "" | "auto" | "llm" | "paddleocr-vl-full" | "pdftotext"
    ) {
        return Err(format!(
            "Invalid profile extraction method '{}'",
            profile.extraction.method
        ));
    }
    if !matches!(
        profile.extraction.input_mode.as_str(),
        "" | "document" | "folder" | "none"
    ) {
        return Err(format!(
            "Invalid profile input mode '{}'",
            profile.extraction.input_mode
        ));
    }

    let valid_key = |key: &str| {
        !key.is_empty()
            && key.len() <= 64
            && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    let mut variable_keys = std::collections::HashSet::new();
    if profile.variables.len() > MAX_VARIABLES {
        return Err(format!(
            "Profile has too many variables (maximum {MAX_VARIABLES})"
        ));
    }
    for variable in &profile.variables {
        if !valid_key(&variable.key) {
            return Err(format!(
                "Invalid variable key '{}'. Use 1–64 ASCII letters, numbers, or underscores.",
                variable.key
            ));
        }
        if !variable_keys.insert(variable.key.as_str()) {
            return Err(format!("Duplicate variable key '{}'", variable.key));
        }
        if !matches!(variable.kind.as_str(), "text" | "choice" | "file") {
            return Err(format!(
                "Invalid kind '{}' for variable '{}'",
                variable.kind, variable.key
            ));
        }
        if variable.label.len() > 1_024 {
            return Err(format!("Variable '{}' label is too long", variable.key));
        }
        if variable.default.len() > crate::safety::MAX_RUNTIME_VALUE_BYTES {
            return Err(format!(
                "Variable '{}' default exceeds the 1 MB safety limit",
                variable.key
            ));
        }
        if variable.choices.len() > 100
            || variable.choices.iter().any(|choice| choice.len() > 65_536)
        {
            return Err(format!(
                "Variable '{}' has too many or oversized choices",
                variable.key
            ));
        }
        if variable.kind == "choice"
            && !variable.default.is_empty()
            && !variable.choices.contains(&variable.default)
        {
            return Err(format!(
                "Variable '{}' default is not one of its declared choices",
                variable.key
            ));
        }
        if variable.secret && variable.kind == "file" {
            return Err(format!(
                "Variable '{}' cannot be both a secret and a file path",
                variable.key
            ));
        }
        if variable.secret && !variable.default.is_empty() {
            return Err(format!(
                "Secret variable '{}' cannot have a saved default; supply it when the workflow runs",
                variable.key
            ));
        }
        let validation = &variable.validation;
        if validation
            .min_length
            .zip(validation.max_length)
            .is_some_and(|(minimum, maximum)| minimum > maximum)
        {
            return Err(format!(
                "Variable '{}' minimum length exceeds its maximum length",
                variable.key
            ));
        }
        if validation
            .max_length
            .is_some_and(|maximum| maximum > 1_048_576)
        {
            return Err(format!(
                "Variable '{}' maximum length exceeds the 1 MB safety limit",
                variable.key
            ));
        }
        if !validation.pattern.is_empty() {
            regex::Regex::new(&validation.pattern).map_err(|error| {
                format!(
                    "Variable '{}' has an invalid validation pattern: {error}",
                    variable.key
                )
            })?;
        }
    }

    let mut input_keys = std::collections::HashSet::new();
    if profile.extraction.extra_inputs.len() > MAX_EXTRA_INPUTS {
        return Err(format!(
            "Profile has too many named inputs (maximum {MAX_EXTRA_INPUTS})"
        ));
    }
    for input in &profile.extraction.extra_inputs {
        if !valid_key(&input.key) {
            return Err(format!(
                "Invalid input key '{}'. Use 1–64 ASCII letters, numbers, or underscores.",
                input.key
            ));
        }
        if input.label.len() > 1_024 {
            return Err(format!("Named input '{}' label is too long", input.key));
        }
        if !input_keys.insert(input.key.as_str()) {
            return Err(format!("Duplicate input key '{}'", input.key));
        }
        if !matches!(input.mode.as_str(), "document" | "folder") {
            return Err(format!(
                "Invalid mode '{}' for input '{}'",
                input.mode, input.key
            ));
        }
        if input.extensions.len() > 100
            || input.extensions.iter().any(|extension| {
                extension.is_empty()
                    || extension.starts_with('.')
                    || !extension
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric())
            })
        {
            return Err(format!(
                "Named input '{}' has invalid extensions; use lowercase names without a leading dot",
                input.key
            ));
        }
        if input
            .extensions
            .iter()
            .any(|extension| *extension != extension.to_ascii_lowercase())
        {
            return Err(format!(
                "Named input '{}' extensions must be lowercase",
                input.key
            ));
        }
        if input.mime_types.len() > 100
            || input
                .mime_types
                .iter()
                .any(|mime| !mime.contains('/') || mime.len() > 255)
        {
            return Err(format!(
                "Named input '{}' has an invalid MIME type declaration",
                input.key
            ));
        }
        if input.max_bytes > 256 * 1024 * 1024 {
            return Err(format!(
                "Named input '{}' maximum size exceeds Pipeline's input safety limit",
                input.key
            ));
        }
        if !matches!(
            input.sensitivity.as_str(),
            "" | "public" | "internal" | "confidential" | "secret"
        ) {
            return Err(format!(
                "Named input '{}' has unsupported sensitivity '{}'",
                input.key, input.sensitivity
            ));
        }
    }

    let prompt_surfaces = profile
        .steps
        .iter()
        .map(|step| (format!("step '{}' prompt", step.id), step.prompt.as_str()))
        .chain([
            (
                "orientation prompt".to_string(),
                profile.orientation_prompt.as_str(),
            ),
            (
                "parallel context template".to_string(),
                profile.parallel_context_template.as_str(),
            ),
            ("merge prompt".to_string(), profile.merge.prompt.as_str()),
        ]);
    for (surface, text) in prompt_surfaces {
        for key in placeholder_keys(text, "{var:", &surface)? {
            if !variable_keys.contains(key.as_str()) {
                return Err(format!(
                    "{surface} references undeclared variable '{{var:{key}}}'"
                ));
            }
        }
        for key in placeholder_keys(text, "{input:", &surface)? {
            if !input_keys.contains(key.as_str()) {
                return Err(format!(
                    "{surface} references undeclared named input '{{input:{key}}}'"
                ));
            }
        }
    }
    Ok(())
}

fn placeholder_keys(text: &str, needle: &str, surface: &str) -> Result<Vec<String>, String> {
    let mut keys = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(needle) {
        let after = &rest[start + needle.len()..];
        let end = after
            .find('}')
            .ok_or_else(|| format!("{surface} contains an unterminated {needle} placeholder"))?;
        let key = after[..end].trim();
        if key.is_empty() {
            return Err(format!("{surface} contains an empty {needle} placeholder"));
        }
        keys.push(key.to_string());
        rest = &after[end + 1..];
    }
    Ok(keys)
}

/// Validate a host-materialized execution config through the same rules used
/// for saved profiles. This is a defense-in-depth check after deterministic
/// runtime assembly, not an additional import surface.
pub(crate) fn validate_runtime_config(config: &PipelineConfig) -> Result<(), String> {
    validate_profile_data(&ProfileData::from_config("Runtime workflow", config))?;
    validate_enabled_sequential_step(&config.steps)
}
