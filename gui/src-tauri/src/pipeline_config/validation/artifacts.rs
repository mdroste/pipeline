//! Artifact selections and explicitly published workflow products.

use super::super::{ArtifactSelector, Phase, ProfileData, StepArtifactPart};

pub(super) fn validate_artifact_context(profile: &ProfileData) -> Result<(), String> {
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

pub(super) fn validate_published_outputs(profile: &ProfileData) -> Result<(), String> {
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
