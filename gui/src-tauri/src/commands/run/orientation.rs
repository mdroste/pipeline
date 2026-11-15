//! Review planning over an already persisted document bundle.

use super::super::{is_cancelled, write_run_input_file};
use crate::pipeline::orient;

pub(super) struct OrientedDocument {
    pub(super) orientation: serde_json::Value,
    pub(super) orientation_path: String,
    pub(super) orientation_sampling: orient::OrientationSample,
    pub(super) _orient_tmp: tempfile::NamedTempFile,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn prepare_orientation(
    app: &crate::emit::EventBus,
    recipe: &crate::commands::run_contract::OrientationRecipe,
    _input_mode: &str,
    config: &crate::pipeline_config::PipelineConfig,
    extraction: &crate::models::ExtractionResult,
    bundle: Option<&crate::document_bundle::DocumentBundle>,
    scoped_source_read_root: &Option<String>,
    run_input_dir: &tempfile::TempDir,
    run_writer: &mut Option<crate::runs::RunWriter>,
    author_note: Option<&str>,
    supporting_inputs: &[orient::SupportingInput],
) -> Result<OrientedDocument, String> {
    // Every workflow builds a document overview before its steps run.
    let orientation_label = if crate::auto_review::uses_auto_review_contract(config) {
        "Choosing reviewers"
    } else {
        "Identifying content"
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
    let survey_template = Some(recipe.template.clone());
    let survey_schema = recipe.schema.clone();
    if let Some(preparation) = orient::document_preparation_snapshot(
        extraction,
        bundle,
        survey_schema.as_ref(),
        supporting_inputs,
    )? {
        run_writer
            .as_mut()
            .ok_or("Durable run workspace disappeared during document preparation")?
            .add_text(
                orient::PREPARATION_PATH,
                "Document preparation",
                "context",
                &preparation,
            )?;
    }
    let prepared = orient::build_paper_orientation(
        app,
        extraction,
        bundle,
        survey_template.as_deref(),
        survey_schema.as_ref(),
        scoped_source_read_root.as_deref(),
        author_note,
        run_writer.as_ref().map(|writer| writer.dir()),
        supporting_inputs,
    )
    .await?;
    let orient_secs = orient_start.elapsed().as_secs();
    app.emit_event(
        "pipeline:log",
        serde_json::json!({
            "line": format!("Review plan prepared ({}s)", orient_secs)
        }),
    )
    .ok();
    if is_cancelled() {
        return Err("Pipeline cancelled".into());
    }
    prepared.persist(
        run_writer
            .as_mut()
            .ok_or("Durable run workspace disappeared during review planning")?,
    )?;
    let orientation = prepared.orientation;

    let orientation_json = serde_json::to_string(&orientation)
        .map_err(|e| format!("Failed to serialize document overview: {e}"))?;
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
            "Document overview",
            "context",
            &orientation_json,
        )
        .map_err(|error| format!("Could not save the document overview: {error}"))?;
    let orientation_sampling = orient::orientation_sample(&extraction.text);
    let sampling_json = serde_json::to_string_pretty(&orientation_sampling)
        .map_err(|error| format!("Could not serialize orientation coverage: {error}"))?;
    run_writer
        .as_mut()
        .ok_or_else(|| "Durable run workspace disappeared before orientation".to_string())?
        .add_text(
            orient::ORIENTATION_SAMPLING_PATH,
            "Document reading coverage",
            "context",
            &sampling_json,
        )
        .map_err(|error| format!("Could not save orientation sampling coverage: {error}"))?;
    let orient_bytes = orientation_json.len();
    app.emit_event(
        "pipeline:log",
        serde_json::json!({
            "line": format!("Document overview written to temp file ({orient_bytes} bytes)")
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
