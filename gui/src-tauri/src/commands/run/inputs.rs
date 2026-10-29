//! Fresh named-input capture, extraction, and model-readable temporary files.

use super::super::{extra_input_artifact_path, write_run_input_file};
use crate::pipeline::{executor, extract};
use std::collections::HashMap;

pub(super) struct NamedInputs {
    pub(super) resolved_inputs: HashMap<String, String>,
    pub(super) persisted_inputs: HashMap<String, String>,
    pub(super) extra_tmps: Vec<tempfile::NamedTempFile>,
    pub(super) runtime_extra_sources: HashMap<String, String>,
    pub(super) extra_sources: HashMap<String, String>,
}

pub(super) async fn capture_named_inputs(
    app: &crate::emit::EventBus,
    config: &crate::pipeline_config::PipelineConfig,
    provided_inputs: &std::collections::HashMap<String, String>,
    run_input_dir: &tempfile::TempDir,
    run_writer: &mut Option<crate::runs::RunWriter>,
) -> Result<NamedInputs, String> {
    // Extra named inputs (1.2.4): extract each declared slot's file through the
    // same cascade, write the text to a temp file, and expose its path to
    // prompts as {input:key}. Temp files are held alive until the run finishes.
    let mut resolved_inputs: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut persisted_inputs: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut extra_tmps: Vec<tempfile::NamedTempFile> = Vec::new();
    let mut runtime_extra_sources: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut extra_sources: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (input_index, slot) in config.extraction.extra_inputs.iter().enumerate() {
        let provided = provided_inputs
            .get(&slot.key)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());
        let path = match provided {
            Some(p) => p,
            None => {
                if slot.required {
                    let name = if slot.label.is_empty() {
                        &slot.key
                    } else {
                        &slot.label
                    };
                    return Err(format!("Missing required input '{name}'"));
                }
                continue;
            }
        };
        extra_sources.insert(slot.key.clone(), path.to_string());
        run_writer
            .as_mut()
            .ok_or_else(|| "Durable run workspace disappeared before named inputs".to_string())?
            .record_extra_input_source(&slot.key, path)
            .map_err(|error| format!("Could not record named input '{}': {error}", slot.key))?;
        let private = run_input_dir
            .path()
            .join("named")
            .join(executor::step_slug(&slot.key));
        let staged =
            extract::stage_selected_source(std::path::Path::new(path), &slot.mode, &private)?;
        let captured = crate::commands::reuse_artifacts::snapshot_source(&staged, &private)?;
        let captured_path = captured
            .source_path
            .as_ref()
            .ok_or("Named input has no captured source")?
            .to_string_lossy()
            .into_owned();
        crate::commands::reuse_artifacts::retain_named_source(
            &slot.key,
            &captured,
            run_writer.as_mut().ok_or("Missing durable run workspace")?,
        )?;
        runtime_extra_sources.insert(slot.key.clone(), captured_path.clone());
        let ex = match slot.mode.as_str() {
            "folder" => extract::ingest_folder_async(&captured_path).await?,
            _ => extract::extract(app, &captured_path, &config.extraction).await?,
        };
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
            .add_text(&rel_path, label, "context", &ex.text)
            .map_err(|error| format!("Could not save named input '{}': {error}", slot.key))?;
        writer
            .record_extra_input(&slot.key, &rel_path)
            .map_err(|error| format!("Could not index named input '{}': {error}", slot.key))?;
        persisted_inputs.insert(slot.key.clone(), rel_path);
        let (tf, p) = write_run_input_file(
            run_input_dir.path(),
            "pipeline_input_",
            ".txt",
            &ex.text,
            &format!("input '{}'", slot.key),
        )?;
        resolved_inputs.insert(slot.key.clone(), p);
        extra_tmps.push(tf);
        app.emit_event("pipeline:log", serde_json::json!({
            "line": format!("Extra input '{}' extracted via {} ({} chars)", slot.key, ex.method, ex.text.len())
        })).ok();
    }
    Ok(NamedInputs {
        resolved_inputs,
        persisted_inputs,
        extra_tmps,
        runtime_extra_sources,
        extra_sources,
    })
}

#[cfg(test)]
mod tests;
