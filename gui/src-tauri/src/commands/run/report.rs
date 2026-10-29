//! Fresh-run product evidence, quality, and report assembly.

use crate::models::PipelineReport;
use crate::output;

#[allow(clippy::too_many_arguments)]
pub(super) fn assemble_report(
    app: &crate::emit::EventBus,
    run_writer: &mut Option<crate::runs::RunWriter>,
    execution_config: &crate::pipeline_config::PipelineConfig,
    result: crate::pipeline::executor::ExecutionResult,
    extraction: &crate::models::ExtractionResult,
    document_bundle: &Option<crate::document_bundle::DocumentBundle>,
    orientation_sampling: &crate::pipeline::orient::OrientationSample,
    orientation: serde_json::Value,
    scoped_source_path: &str,
) -> PipelineReport {
    let mut products = crate::findings::build_run_products(execution_config, &result.outputs);
    if let Some(writer) = run_writer.as_mut() {
        for warning in
            writer.capture_source_evidence(&mut products, std::path::Path::new(scoped_source_path))
        {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!("WARNING: {warning}") }),
            );
        }
    }
    crate::pipeline::analyzers::quotes::verify_run(
        &mut products,
        document_bundle.as_ref(),
        extraction,
        run_writer.as_ref().map(|writer| writer.dir()),
    );
    let quality = output::build_report_quality(
        extraction,
        document_bundle.as_ref(),
        orientation_sampling,
        execution_config,
        &result.outputs,
        &result.failed_steps,
        &products,
    );
    PipelineReport {
        orientation,
        step_outputs: result.outputs,
        failed_steps: result.failed_steps,
        products,
        quality,
        referee_reports: vec![],
        editor: None,
        report_date: chrono::Local::now().date_naive(),
        paper_hash: extraction.paper_hash.clone(),
    }
}
