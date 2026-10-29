//! Runs an analyzer step on the host. Its JSON product is recorded like any
//! step output, with `pipeline` as the provider and no model usage.

use crate::models::{StepFailure, StepOutput};
use crate::pipeline::analyzers::{self, AnalyzerContext};
use crate::pipeline_config::{Phase, StepConfig};

pub(super) struct AnalyzerInputs<'a> {
    pub document_bundle_path: &'a str,
    pub paper_text_path: &'a str,
    pub source_path: &'a str,
}

pub(super) async fn run_analyzer_step(
    app: &crate::emit::EventBus,
    step: &StepConfig,
    inputs: &AnalyzerInputs<'_>,
) -> Result<StepOutput, String> {
    let spec = step
        .analyzer
        .as_ref()
        .ok_or_else(|| format!("Analyzer step '{}' names no analyzer", step.id))?;
    let _ = app.emit_event(
        "pipeline:pass",
        serde_json::json!({"name": step.id, "status": "running"}),
    );
    if let Some(info) = analyzers::registry::find(&spec.id, spec.version) {
        let network = analyzers::registry::uses_network(info, &spec.options);
        let _ = app.emit_event(
            "pipeline:log",
            serde_json::json!({ "line": format!(
                "{}: running on this computer{}",
                info.label,
                if network {
                    format!(" with lookups at {}", info.network_services.join(", "))
                } else {
                    String::new()
                }
            ) }),
        );
    }
    let started = std::time::Instant::now();
    let value = analyzers::run(
        spec,
        &AnalyzerContext {
            document_bundle_path: inputs.document_bundle_path,
            paper_text_path: inputs.paper_text_path,
            source_path: inputs.source_path,
        },
    )
    .await?;
    let raw_text = serde_json::to_string_pretty(&value)
        .map_err(|error| format!("Could not record analyzer output: {error}"))?;
    Ok(StepOutput {
        step_id: step.id.clone(),
        step_label: step.label.clone(),
        phase: match step.phase {
            Phase::Parallel => "parallel",
            Phase::Sequential => "sequential",
        }
        .to_string(),
        provider: "pipeline".to_string(),
        agent: "pipeline".to_string(),
        model: format!("{}@{}", spec.id, spec.version),
        raw_text,
        structured_json: true,
        duration_secs: started.elapsed().as_secs(),
        ..Default::default()
    })
}

/// An analyzer's output, or its recorded failure. Cancellation propagates.
pub(super) async fn analyzer_result(
    app: &crate::emit::EventBus,
    step: &StepConfig,
    inputs: &AnalyzerInputs<'_>,
) -> Result<Result<StepOutput, StepFailure>, String> {
    match run_analyzer_step(app, step, inputs).await {
        Ok(output) => Ok(Ok(output)),
        Err(error) if super::step_call::is_cancellation_error(&error) => Err(error),
        Err(error) => {
            let _ = app.emit_event(
                "pipeline:pass",
                serde_json::json!({"name": step.id, "status": "error"}),
            );
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!("WARNING: check '{}' failed: {error}", step.label) }),
            );
            Ok(Err(StepFailure {
                step_id: step.id.clone(),
                step_label: step.label.clone(),
                phase: match step.phase {
                    Phase::Parallel => "parallel",
                    Phase::Sequential => "sequential",
                }
                .to_string(),
                error,
            }))
        }
    }
}
