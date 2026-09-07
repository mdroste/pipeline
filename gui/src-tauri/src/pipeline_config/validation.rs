use super::*;

/// Step ids key the executor's pass events and the merge grouping
/// (`{id}/{agent}`), so duplicates silently collide. Reject them at save and
/// import time. Deliberately not enforced on load/migration, so an existing
/// profile with duplicates can still be opened and repaired in the editor.
pub fn validate_unique_step_ids(steps: &[StepConfig]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for step in steps {
        if step.id.is_empty()
            || step.id.len() > 64
            || !step
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return Err(format!(
                "Invalid step id '{}'. Use 1–64 ASCII letters, numbers, '.', '-', or '_'.",
                step.id
            ));
        }
        if !seen.insert(step.id.as_str()) {
            return Err(format!(
                "Duplicate step id '{}' — step ids must be unique",
                step.id
            ));
        }
    }
    Ok(())
}

pub fn validate_workflow_semantics(steps: &[StepConfig]) -> Result<(), String> {
    for step in steps {
        if step.phase == Phase::Sequential && step.agents.len() > 1 {
            return Err(format!(
                "Sequential step '{}' selects multiple agents, but sequential multi-agent execution is unsupported.",
                step.id
            ));
        }
        if step.phase == Phase::Sequential && step.for_each.is_some() {
            return Err(format!(
                "Sequential step '{}' uses fan-out, but sequential fan-out is unsupported.",
                step.id
            ));
        }
    }
    Ok(())
}

/// Require a terminal-capable workflow without making legacy invalid profiles
/// impossible to open and repair. Call this at explicit save/import and run
/// boundaries, rather than from the structural load validator.
pub fn validate_enabled_sequential_step(steps: &[StepConfig]) -> Result<(), String> {
    if steps
        .iter()
        .any(|step| step.enabled && step.phase == Phase::Sequential)
    {
        Ok(())
    } else {
        Err(
            "A workflow must have at least one enabled Sequential step to produce a final report."
                .to_string(),
        )
    }
}

pub(super) fn validate_profile_steps(steps: &[StepConfig]) -> Result<(), String> {
    if steps.len() > MAX_PROFILE_STEPS {
        return Err(format!(
            "Profile has {} steps; the safety limit is {MAX_PROFILE_STEPS}",
            steps.len()
        ));
    }
    validate_unique_step_ids(steps)?;
    validate_dependencies(steps)?;
    validate_workflow_semantics(steps)?;
    validate_run_conditions(steps)?;
    for step in steps {
        if step.label.chars().count() > MAX_STEP_LABEL_CHARS {
            return Err(format!(
                "Step '{}' label exceeds {MAX_STEP_LABEL_CHARS} characters",
                step.id
            ));
        }
        if step.prompt.len() > MAX_STEP_PROMPT_BYTES
            || step.system_prompt.len() > MAX_STEP_PROMPT_BYTES
        {
            return Err(format!(
                "Step '{}' prompt exceeds the {} MB safety limit",
                step.id,
                MAX_STEP_PROMPT_BYTES / 1024 / 1024
            ));
        }
        if step.tools.len() > ALLOWED_TOOLS.len() {
            return Err(format!("Step '{}' declares too many tools", step.id));
        }
        let mut tools = std::collections::HashSet::new();
        for tool in &step.tools {
            if !ALLOWED_TOOLS.contains(&tool.as_str()) {
                return Err(format!(
                    "Step '{}' requests unsupported tool '{}'. Allowed tools: {}",
                    step.id,
                    tool,
                    ALLOWED_TOOLS.join(", ")
                ));
            }
            if !tools.insert(tool.as_str()) {
                return Err(format!(
                    "Step '{}' lists tool '{}' more than once",
                    step.id, tool
                ));
            }
        }
        if step.agents.len() > ALLOWED_AGENTS.len() {
            return Err(format!("Step '{}' declares too many agents", step.id));
        }
        let mut agents = std::collections::HashSet::new();
        for agent in &step.agents {
            if !ALLOWED_AGENTS.contains(&agent.as_str()) {
                return Err(format!(
                    "Step '{}' names unsupported agent '{}'. Allowed agents: {}",
                    step.id,
                    agent,
                    ALLOWED_AGENTS.join(", ")
                ));
            }
            if !agents.insert(agent.as_str()) {
                return Err(format!(
                    "Step '{}' lists agent '{}' more than once",
                    step.id, agent
                ));
            }
        }
        if let Some(for_each) = &step.for_each {
            if let Some(artifact) = &for_each.artifact {
                if !for_each.glob.trim().is_empty() {
                    return Err(format!(
                        "Step '{}' fan-out must use either a glob or an upstream artifact, not both",
                        step.id
                    ));
                }
                let target = artifact.step.trim();
                if target.is_empty() || target == step.id {
                    return Err(format!(
                        "Step '{}' fan-out artifact must name another step in this workflow",
                        step.id
                    ));
                }
                if !steps.iter().any(|candidate| candidate.id == target) {
                    return Err(format!(
                        "Step '{}' fans out over unknown step '{}'",
                        step.id, target
                    ));
                }
                if !artifact.pointer.is_empty() && !artifact.pointer.starts_with('/') {
                    return Err(format!(
                        "Step '{}' fan-out pointer must be an RFC 6901 JSON pointer starting with '/'",
                        step.id
                    ));
                }
                if artifact.pointer.len() > 512 {
                    return Err(format!(
                        "Step '{}' fan-out pointer must contain at most 512 bytes",
                        step.id
                    ));
                }
            } else {
                if for_each.glob.trim().is_empty() || for_each.glob.len() > 1024 {
                    return Err(format!(
                        "Step '{}' fan-out glob must contain 1–1024 bytes",
                        step.id
                    ));
                }
                if for_each.glob.contains('\\') {
                    return Err(format!(
                        "Step '{}' has an invalid fan-out glob; use '/' as the path separator",
                        step.id
                    ));
                }
            }
            if !(1..=MAX_FAN_OUT_ITEMS).contains(&for_each.max) {
                return Err(format!(
                    "Step '{}' fan-out maximum must be between 1 and {MAX_FAN_OUT_ITEMS}",
                    step.id
                ));
            }
        }
        if let Some(schema) = &step.output_schema {
            if schema.get("x-pipeline-contract").is_some()
                || schema
                    .get(crate::auto_review::ADAPTIVE_AGENT_COUNT_KEY)
                    .is_some()
            {
                return Err(format!(
                    "Step '{}' output schema uses an orientation-only x-pipeline contract setting",
                    step.id
                ));
            }
            let bytes = serde_json::to_vec(schema)
                .map_err(|e| format!("Step '{}' output schema is invalid: {e}", step.id))?;
            if bytes.len() > MAX_OUTPUT_SCHEMA_BYTES {
                return Err(format!(
                    "Step '{}' output schema exceeds the {} MB safety limit",
                    step.id,
                    MAX_OUTPUT_SCHEMA_BYTES / 1024 / 1024
                ));
            }
            // Live contract references resolve before the provider preflight,
            // so an unknown reference fails at save rather than dispatch.
            let resolved = crate::pipeline::structured::resolve_schema_reference(schema)
                .map_err(|e| format!("Step '{}' output schema is invalid: {e}", step.id))?;
            crate::pipeline::structured::provider_schema(&resolved)
                .map_err(|e| format!("Step '{}' output schema is invalid: {e}", step.id))?;
            if let Some(target) = schema.get(crate::pipeline::structured::PRESERVE_FINDINGS_KEY) {
                let target = target.as_str().unwrap_or_default().trim();
                if target.is_empty()
                    || target == step.id
                    || !steps.iter().any(|candidate| candidate.id == target)
                {
                    return Err(format!(
                        "Step '{}' preserves findings from '{}', which is not another step in this workflow",
                        step.id, target
                    ));
                }
                if !step
                    .artifact_dependencies()
                    .any(|dependency| dependency == target)
                {
                    return Err(format!(
                        "Step '{}' preserves findings from '{}', but does not select that step as an artifact source",
                        step.id, target
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_artifact_context(profile: &ProfileData) -> Result<(), String> {
    const MAX_SELECTORS_PER_STEP: usize = 256;
    let named_inputs: std::collections::HashSet<&str> = profile
        .extraction
        .extra_inputs
        .iter()
        .map(|input| input.key.as_str())
        .collect();

    for step in &profile.steps {
        if step.context.include.len() > MAX_SELECTORS_PER_STEP {
            return Err(format!(
                "Step '{}' selects more than {MAX_SELECTORS_PER_STEP} artifact sources",
                step.id
            ));
        }
        let mut after = std::collections::HashSet::new();
        for dependency in &step.after {
            if !after.insert(dependency.as_str()) {
                return Err(format!(
                    "Step '{}' lists order dependency '{}' more than once",
                    step.id, dependency
                ));
            }
        }

        let mut sources = std::collections::HashSet::new();
        for selector in &step.context.include {
            let source_key = match selector {
                ArtifactSelector::Primary { parts } => {
                    if parts.is_empty() {
                        return Err(format!(
                            "Step '{}' selects the primary input without selecting any representations",
                            step.id
                        ));
                    }
                    if profile.extraction.input_mode == "none" {
                        return Err(format!(
                            "Step '{}' selects the primary input, but this workflow has no input",
                            step.id
                        ));
                    }
                    let unique: std::collections::HashSet<_> = parts.iter().collect();
                    if unique.len() != parts.len() {
                        return Err(format!(
                            "Step '{}' repeats a primary-input representation",
                            step.id
                        ));
                    }
                    "primary".to_string()
                }
                ArtifactSelector::Survey => {
                    if !profile.use_orientation {
                        return Err(format!(
                            "Step '{}' selects the survey, but this workflow disables the survey",
                            step.id
                        ));
                    }
                    "survey".to_string()
                }
                ArtifactSelector::NamedInput { key, parts } => {
                    if !named_inputs.contains(key.as_str()) {
                        return Err(format!(
                            "Step '{}' selects unknown named input '{}'",
                            step.id, key
                        ));
                    }
                    if parts.is_empty() {
                        return Err(format!(
                            "Step '{}' selects named input '{}' without a representation",
                            step.id, key
                        ));
                    }
                    let unique: std::collections::HashSet<_> = parts.iter().collect();
                    if unique.len() != parts.len() {
                        return Err(format!(
                            "Step '{}' repeats a representation for named input '{}'",
                            step.id, key
                        ));
                    }
                    format!("named:{key}")
                }
                ArtifactSelector::Step {
                    step: producer,
                    parts,
                    glob,
                } => {
                    if step.phase == Phase::Parallel {
                        return Err(format!(
                            "Parallel step '{}' cannot select output from step '{}'; parallel steps run independently",
                            step.id, producer
                        ));
                    }
                    if parts.is_empty() {
                        return Err(format!(
                            "Step '{}' selects artifacts from '{}' without selecting report or files",
                            step.id, producer
                        ));
                    }
                    let unique: std::collections::HashSet<_> = parts.iter().collect();
                    if unique.len() != parts.len() {
                        return Err(format!(
                            "Step '{}' repeats an artifact type from '{}'",
                            step.id, producer
                        ));
                    }
                    if !glob.is_empty() {
                        if !parts.contains(&StepArtifactPart::Files) {
                            return Err(format!(
                                "Step '{}' sets a file glob for '{}' without selecting files",
                                step.id, producer
                            ));
                        }
                        if glob.len() > 1024
                            || glob.starts_with('/')
                            || glob.contains('\\')
                            || glob.split('/').any(|part| part == "..")
                            || glob.contains(':')
                        {
                            return Err(format!(
                                "Step '{}' has an invalid supporting-file glob for '{}'",
                                step.id, producer
                            ));
                        }
                    }
                    format!("step:{producer}")
                }
            };
            if !sources.insert(source_key.clone()) {
                return Err(format!(
                    "Step '{}' selects artifact source '{}' more than once; combine its parts in one entry",
                    step.id, source_key
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_profile_data(profile: &ProfileData) -> Result<(), String> {
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

fn validate_published_outputs(profile: &ProfileData) -> Result<(), String> {
    for (role, step_id) in [
        ("primary report", profile.outputs.primary_step.trim()),
        ("findings", profile.outputs.findings_step.trim()),
    ] {
        if step_id.is_empty() {
            continue;
        }
        let step = profile
            .steps
            .iter()
            .find(|step| step.id == step_id)
            .ok_or_else(|| format!("Published {role} names unknown step '{step_id}'"))?;
        if !step.enabled {
            return Err(format!("Published {role} step '{step_id}' is disabled"));
        }
        if step.phase != Phase::Sequential {
            return Err(format!(
                "Published {role} step '{step_id}' must be Sequential"
            ));
        }
        if role == "findings" && step.output_schema.is_none() {
            return Err(format!(
                "Published findings step '{step_id}' must define an output JSON schema"
            ));
        }
    }
    let mut product_keys = std::collections::HashSet::new();
    for product in &profile.outputs.named {
        if product.key.is_empty()
            || product.key.len() > 64
            || !product
                .key
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            return Err(format!("Invalid named product key '{}'", product.key));
        }
        if !product_keys.insert(product.key.as_str()) {
            return Err(format!("Duplicate named product key '{}'", product.key));
        }
        let step = profile
            .steps
            .iter()
            .find(|step| step.id == product.step)
            .ok_or_else(|| {
                format!(
                    "Named product '{}' references unknown step '{}'",
                    product.key, product.step
                )
            })?;
        if !step.enabled {
            return Err(format!(
                "Named product '{}' references disabled step '{}'",
                product.key, product.step
            ));
        }
        if !matches!(
            product.viewer.as_str(),
            "" | "text" | "markdown" | "json" | "artifact"
        ) {
            return Err(format!(
                "Named product '{}' has unsupported viewer '{}'",
                product.key, product.viewer
            ));
        }
        if !matches!(
            product.export_policy.as_str(),
            "" | "full" | "redacted" | "disabled"
        ) {
            return Err(format!(
                "Named product '{}' has unsupported export policy '{}'",
                product.key, product.export_policy
            ));
        }
        if !matches!(
            product.sensitivity.as_str(),
            "" | "public" | "internal" | "confidential" | "secret"
        ) {
            return Err(format!(
                "Named product '{}' has unsupported sensitivity '{}'",
                product.key, product.sensitivity
            ));
        }
        if let Some(schema) = product.schema.as_ref() {
            crate::pipeline::structured::provider_schema(schema).map_err(|error| {
                format!("Named product '{}' schema is invalid: {error}", product.key)
            })?;
            if step.output_schema.is_none() {
                return Err(format!(
                    "Named product '{}' declares a schema but step '{}' has no output schema",
                    product.key, product.step
                ));
            }
        }
    }
    Ok(())
}

/// Validate a host-materialized execution config through the same rules used
/// for saved profiles. This is a defense-in-depth check after deterministic
/// runtime assembly, not an additional import surface.
pub(crate) fn validate_runtime_config(config: &PipelineConfig) -> Result<(), String> {
    validate_profile_data(&ProfileData::from_config("Runtime workflow", config))?;
    validate_enabled_sequential_step(&config.steps)
}

impl StepConfig {
    /// Step ids whose artifacts this step consumes.
    pub fn artifact_dependencies(&self) -> impl Iterator<Item = &str> {
        self.context
            .include
            .iter()
            .filter_map(|selector| match selector {
                ArtifactSelector::Step { step, .. } => Some(step.as_str()),
                _ => None,
            })
            .chain(
                // Fanning out over an upstream artifact is dataflow: the step
                // becomes ready only after its item source completes.
                self.for_each
                    .as_ref()
                    .and_then(|for_each| for_each.artifact.as_ref())
                    .map(|artifact| artifact.step.as_str()),
            )
    }

    /// Dependencies whose successful completion is part of this step's
    /// execution contract. They also establish scheduling edges.
    pub fn policy_dependencies(&self) -> impl Iterator<Item = &str> {
        self.dependency_policy
            .required
            .iter()
            .chain(self.dependency_policy.quorum.iter())
            .map(String::as_str)
    }
}

/// Compute each enabled step's dependency set. `after` contributes order-only
/// edges; upstream artifact selections contribute data edges.
pub(crate) fn resolve_dependencies(
    enabled: &[&StepConfig],
) -> Vec<std::collections::HashSet<String>> {
    enabled
        .iter()
        .map(|step| {
            step.after
                .iter()
                .map(String::as_str)
                .chain(step.artifact_dependencies())
                .chain(step.policy_dependencies())
                .map(str::to_string)
                .collect()
        })
        .collect()
}

/// Reject enabled dependency graphs that cannot run. Disabled steps may retain
/// stale settings while being edited, but an enabled step may never wait on a
/// disabled or unknown upstream.
pub fn validate_dependencies(steps: &[StepConfig]) -> Result<(), String> {
    use std::collections::{HashMap, HashSet};
    let ids: HashSet<&str> = steps.iter().map(|s| s.id.as_str()).collect();
    let enabled_ids: HashSet<&str> = steps
        .iter()
        .filter(|step| step.enabled)
        .map(|step| step.id.as_str())
        .collect();

    // Unknown / self dependencies.
    for s in steps.iter().filter(|step| step.enabled) {
        for dep in s
            .after
            .iter()
            .map(String::as_str)
            .chain(s.artifact_dependencies())
            .chain(s.policy_dependencies())
        {
            if dep == s.id {
                return Err(format!("Step '{}' lists itself as a dependency.", s.id));
            }
            if !ids.contains(dep) {
                return Err(format!(
                    "Step '{}' depends on unknown step '{}'.",
                    s.id, dep
                ));
            }
            if !enabled_ids.contains(dep) {
                return Err(format!(
                    "Step '{}' depends on disabled step '{}'. Enable it or remove the dependency.",
                    s.id, dep
                ));
            }
        }

        let mut required = HashSet::new();
        for dependency in &s.dependency_policy.required {
            if !required.insert(dependency.as_str()) {
                return Err(format!(
                    "Step '{}' lists required dependency '{}' more than once.",
                    s.id, dependency
                ));
            }
        }
        let mut quorum = HashSet::new();
        for dependency in &s.dependency_policy.quorum {
            if !quorum.insert(dependency.as_str()) {
                return Err(format!(
                    "Step '{}' lists quorum dependency '{}' more than once.",
                    s.id, dependency
                ));
            }
            if required.contains(dependency.as_str()) {
                return Err(format!(
                    "Step '{}' lists dependency '{}' as both required and quorum.",
                    s.id, dependency
                ));
            }
        }
        let minimum = s.dependency_policy.minimum_successes as usize;
        if quorum.is_empty() && minimum != 0 {
            return Err(format!(
                "Step '{}' sets a quorum minimum without quorum dependencies.",
                s.id
            ));
        }
        if !quorum.is_empty() && (minimum == 0 || minimum > quorum.len()) {
            return Err(format!(
                "Step '{}' quorum minimum must be between 1 and {}.",
                s.id,
                quorum.len()
            ));
        }
    }

    // Cycle detection over the complete effective graph (DFS with a colour
    // map), including both order-only and artifact-dataflow dependencies.
    let enabled: Vec<&StepConfig> = steps.iter().filter(|step| step.enabled).collect();
    let resolved = resolve_dependencies(&enabled);
    let graph: HashMap<&str, Vec<&str>> = enabled
        .iter()
        .zip(&resolved)
        .map(|(step, deps)| (step.id.as_str(), deps.iter().map(String::as_str).collect()))
        .collect();
    #[derive(PartialEq, Clone, Copy)]
    enum Mark {
        Visiting,
        Done,
    }
    let mut marks: HashMap<&str, Mark> = HashMap::new();
    // Iterative DFS so deep graphs can't blow the stack.
    for start in graph.keys().copied() {
        if marks.contains_key(start) {
            continue;
        }
        let mut stack: Vec<(&str, usize)> = vec![(start, 0)];
        marks.insert(start, Mark::Visiting);
        while let Some((node, idx)) = stack.last().copied() {
            let neighbours = graph.get(node).map(|v| v.as_slice()).unwrap_or(&[]);
            if idx < neighbours.len() {
                stack.last_mut().unwrap().1 += 1;
                let next = neighbours[idx];
                match marks.get(next) {
                    Some(Mark::Visiting) => {
                        return Err(format!(
                            "Step dependencies form a cycle involving '{next}'."
                        ));
                    }
                    Some(Mark::Done) => {}
                    None => {
                        marks.insert(next, Mark::Visiting);
                        stack.push((next, 0));
                    }
                }
            } else {
                marks.insert(node, Mark::Done);
                stack.pop();
            }
        }
    }
    Ok(())
}

fn validate_json_pointer(pointer: &str) -> bool {
    if pointer.is_empty() {
        return true;
    }
    if !pointer.starts_with('/') {
        return false;
    }
    let bytes = pointer.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'~' {
            if index + 1 >= bytes.len() || !matches!(bytes[index + 1], b'0' | b'1') {
                return false;
            }
            index += 2;
        } else {
            index += 1;
        }
    }
    true
}

/// Validate run guards against the effective dependency graph. An output guard
/// may only read a transitive upstream, which guarantees the referent has
/// completed before the scheduler evaluates the guard.
pub(super) fn validate_run_conditions(steps: &[StepConfig]) -> Result<(), String> {
    let enabled: Vec<&StepConfig> = steps.iter().filter(|step| step.enabled).collect();
    let deps = resolve_dependencies(&enabled);
    let index: std::collections::HashMap<&str, usize> = enabled
        .iter()
        .enumerate()
        .map(|(i, step)| (step.id.as_str(), i))
        .collect();

    for (step_index, step) in enabled.iter().enumerate() {
        let Some(condition) = &step.run_if else {
            continue;
        };
        match condition {
            RunCondition::OutputMatches {
                step: target,
                pattern,
                ..
            } => {
                if target.is_empty() {
                    return Err(format!(
                        "Step '{}' output condition must select an upstream step.",
                        step.id
                    ));
                }
                if pattern.len() > MAX_RUN_IF_PATTERN_BYTES {
                    return Err(format!(
                        "Step '{}' output condition exceeds the {MAX_RUN_IF_PATTERN_BYTES}-byte pattern limit.",
                        step.id
                    ));
                }
                regex::Regex::new(pattern).map_err(|error| {
                    format!(
                        "Step '{}' output condition has an invalid regular expression: {error}",
                        step.id
                    )
                })?;
                let Some(&target_index) = index.get(target.as_str()) else {
                    return Err(format!(
                        "Step '{}' output condition refers to an unknown or disabled step '{}'.",
                        step.id, target
                    ));
                };
                let mut pending: Vec<usize> = deps[step_index]
                    .iter()
                    .filter_map(|id| index.get(id.as_str()).copied())
                    .collect();
                let mut upstream = std::collections::HashSet::new();
                while let Some(current) = pending.pop() {
                    if upstream.insert(current) {
                        pending.extend(
                            deps[current]
                                .iter()
                                .filter_map(|id| index.get(id.as_str()).copied()),
                        );
                    }
                }
                if !upstream.contains(&target_index) {
                    return Err(format!(
                        "Step '{}' output condition reads '{}', but that step is not an upstream dependency.",
                        step.id, target
                    ));
                }
            }
            RunCondition::SurveyPath { pointer, .. } => {
                if pointer.len() > MAX_JSON_POINTER_BYTES {
                    return Err(format!(
                        "Step '{}' survey pointer exceeds the {MAX_JSON_POINTER_BYTES}-byte limit.",
                        step.id
                    ));
                }
                if !validate_json_pointer(pointer) {
                    return Err(format!(
                        "Step '{}' survey condition uses an invalid JSON pointer '{}'.",
                        step.id, pointer
                    ));
                }
            }
        }
    }
    Ok(())
}
