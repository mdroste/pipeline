//! Required orientation stage and its durable and temporary context.

use super::super::{is_cancelled, write_run_input_file};
use crate::pipeline::orient;

pub(super) struct OrientedDocument {
    pub(super) orientation: serde_json::Value,
    pub(super) orientation_path: String,
    pub(super) orientation_sampling: orient::OrientationSample,
    pub(super) _orient_tmp: tempfile::NamedTempFile,
}

pub(super) async fn prepare_orientation(
    app: &crate::emit::EventBus,
    config: &crate::pipeline_config::PipelineConfig,
    input_mode: &str,
    extraction: &crate::models::ExtractionResult,
    scoped_source_read_root: &Option<String>,
    run_input_dir: &tempfile::TempDir,
    run_writer: &mut Option<crate::runs::RunWriter>,
) -> Result<OrientedDocument, String> {
    // Every workflow builds an orientation map before its steps run.
    let orientation_label = if crate::auto_review::uses_auto_review_contract(config) {
        "Creating orientation map & review plan"
    } else {
        "Creating orientation map"
    };
    app.emit_event(
        "pipeline:stage",
        serde_json::json!({
            "stage": "orienting",
            "id": "orienting",
            "label": orientation_label,
            "stepIds": [],
            "stepLabels": [],
        }),
    )
    .ok();
    let orient_start = std::time::Instant::now();
    let survey_template = orient::resolve_survey_template(&config.orientation_prompt, input_mode);
    let survey_schema = orient::resolve_survey_schema(
        &config.orientation_prompt,
        config.orientation_schema.as_ref(),
        input_mode,
    );
    let orientation = orient::build_orientation_map(
        app,
        extraction,
        survey_template.as_deref(),
        survey_schema.as_ref(),
        scoped_source_read_root.as_deref(),
    )
    .await?;
    let orient_secs = orient_start.elapsed().as_secs();
    app.emit_event(
        "pipeline:log",
        serde_json::json!({
            "line": format!("Orientation map built ({}s)", orient_secs)
        }),
    )
    .ok();
    if is_cancelled() {
        return Err("Pipeline cancelled".into());
    }

    let orientation_json = serde_json::to_string(&orientation)
        .map_err(|e| format!("Failed to serialize orientation map: {e}"))?;
    let (orient_file, orientation_path) = write_run_input_file(
        run_input_dir.path(),
        "pipeline_orient_",
        ".json",
        &orientation_json,
        "orientation",
    )?;
    run_writer
        .as_mut()
        .ok_or_else(|| "Durable run workspace disappeared before orientation".to_string())?
        .add_text(
            "context/orientation.json",
            "Orientation map",
            "context",
            &orientation_json,
        )
        .map_err(|error| format!("Could not save the orientation map: {error}"))?;
    let orientation_sampling = orient::orientation_sample(&extraction.text);
    let sampling_json = serde_json::to_string_pretty(&orientation_sampling)
        .map_err(|error| format!("Could not serialize orientation coverage: {error}"))?;
    run_writer
        .as_mut()
        .ok_or_else(|| "Durable run workspace disappeared before orientation".to_string())?
        .add_text(
            "context/orientation_sampling.json",
            "Orientation sampling coverage",
            "context",
            &sampling_json,
        )
        .map_err(|error| format!("Could not save orientation sampling coverage: {error}"))?;
    let orient_bytes = orientation_json.len();
    app.emit_event(
        "pipeline:log",
        serde_json::json!({
            "line": format!("Orientation map written to temp file ({orient_bytes} bytes)")
        }),
    )
    .ok();
    let _orient_tmp = orient_file; // hold tempfile alive through step execution

    Ok(OrientedDocument {
        orientation,
        orientation_path,
        orientation_sampling,
        _orient_tmp,
    })
}
