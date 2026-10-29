//! Named-input restoration from immutable parent-run artifacts.

use super::super::{extra_input_artifact_path, write_run_input_file};
use super::files::read_run_file;
use std::collections::HashMap;

pub(super) struct RestoredInputs {
    pub(super) runtime_extra_sources: HashMap<String, String>,
    pub(super) resolved_inputs: HashMap<String, String>,
    pub(super) persisted_inputs: HashMap<String, String>,
    pub(super) extra_tmps: Vec<tempfile::NamedTempFile>,
}

pub(super) fn restore_named_inputs(
    parent_run_id: &str,
    parent_dir: &std::path::Path,
    parent: &crate::runs::RunManifest,
    config: &crate::pipeline_config::PipelineConfig,
    execution_config: &crate::pipeline_config::PipelineConfig,
    run_input_dir: &tempfile::TempDir,
    run_writer: &mut Option<crate::runs::RunWriter>,
) -> Result<RestoredInputs, String> {
    // Rehydrate named inputs from the parent run's captured copies. Required
    // inputs from older runs that predate capture fail explicitly instead of
    // silently substituting an empty string into the prompt.
    let mut runtime_extra_sources = std::collections::HashMap::new();
    let mut resolved_inputs = std::collections::HashMap::new();
    let mut persisted_inputs = std::collections::HashMap::new();
    let mut extra_tmps: Vec<tempfile::NamedTempFile> = Vec::new();
    for (input_index, slot) in config.extraction.extra_inputs.iter().enumerate() {
        let Some(parent_rel) = parent.extra_inputs.get(&slot.key) else {
            if slot.required {
                let label = if slot.label.is_empty() {
                    &slot.key
                } else {
                    &slot.label
                };
                return Err(format!(
                    "Re-run requires named input '{label}', but the parent run did not capture it. Start a new run and select the input again."
                ));
            }
            continue;
        };
        let captured_source = crate::commands::reuse_artifacts::restore_named_source(
            parent_dir,
            &slot.key,
            run_input_dir.path(),
            crate::commands::reuse_artifacts::requires_named_source(execution_config, &slot.key),
        )?;
        if let Some(path) = captured_source.source_path.as_ref() {
            runtime_extra_sources.insert(slot.key.clone(), path.to_string_lossy().into_owned());
        }
        if let Some(writer) = run_writer.as_mut() {
            crate::commands::reuse_artifacts::retain_named_source(
                &slot.key,
                &captured_source,
                writer,
            )?;
        }
        let content = read_run_file(parent_run_id, parent_rel)
            .map_err(|e| format!("Cannot restore named input '{}': {e}", slot.key))?;
        let rel_path = extra_input_artifact_path(input_index, &slot.key);
        let label = if slot.label.is_empty() {
            &slot.key
        } else {
            &slot.label
        };
        let writer = run_writer
            .as_mut()
            .ok_or_else(|| "Durable run workspace disappeared before named inputs".to_string())?;
        writer
            .add_text(&rel_path, label, "context", &content)
            .map_err(|error| format!("Could not save named input '{}': {error}", slot.key))?;
        writer
            .record_extra_input(&slot.key, &rel_path)
            .map_err(|error| format!("Could not index named input '{}': {error}", slot.key))?;
        persisted_inputs.insert(slot.key.clone(), rel_path);
        let (temp, path) = write_run_input_file(
            run_input_dir.path(),
            "pipeline_input_",
            ".txt",
            &content,
            &format!("restored input '{}'", slot.key),
        )?;
        resolved_inputs.insert(slot.key.clone(), path);
        extra_tmps.push(temp);
    }

    Ok(RestoredInputs {
        runtime_extra_sources,
        resolved_inputs,
        persisted_inputs,
        extra_tmps,
    })
}

#[cfg(test)]
mod tests;
