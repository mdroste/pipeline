//! Resolve the PDF reading method using the same settings as a prepared review.
use super::run_context::{bind_model_overrides, load_run_snapshot, RunModelOverrides};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfOptions {
    method: String,
}

fn effective_method(
    config: &crate::pipeline_config::PipelineConfig,
    settings: &crate::settings::Settings,
) -> String {
    let method = config.extraction.method.trim();
    crate::settings::resolve_pdf_extractor(if method.is_empty() || method == "auto" {
        &settings.pdf_extractor
    } else {
        method
    })
    .to_string()
}

#[tauri::command]
pub async fn review_pdf_options(
    run_parallel_overrides: Option<RunModelOverrides>,
) -> Result<PdfOptions, String> {
    let snapshot = bind_model_overrides(load_run_snapshot()?, run_parallel_overrides.as_ref())?;
    Ok(PdfOptions {
        method: effective_method(&snapshot.config, &snapshot.settings),
    })
}
