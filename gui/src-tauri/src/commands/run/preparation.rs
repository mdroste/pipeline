//! Persist an explicit partial outcome when a budget stops preparation.
use super::super::{complete_run, RunCompletion};
use crate::models::{ExtractionResult, PipelineReport, ReportQuality, StepFailure};

fn stopped_report(
    config: &crate::pipeline_config::PipelineConfig,
    extraction: &ExtractionResult,
    error: &str,
) -> PipelineReport {
    let mut failed_steps = vec![StepFailure {
        step_id: "preparation".into(),
        step_label: "Document preparation".into(),
        phase: "preparation".into(),
        error: error.to_string(),
    }];
    failed_steps.extend(config.steps.iter().filter(|step| step.enabled).map(|step| {
        StepFailure {
            step_id: step.id.clone(),
            step_label: step.label.clone(),
            phase: match step.phase {
                crate::pipeline_config::Phase::Parallel => "parallel",
                crate::pipeline_config::Phase::Sequential => "sequential",
            }
            .into(),
            error: error.to_string(),
        }
    }));
    PipelineReport {
        orientation: serde_json::Value::Null,
        quality: ReportQuality {
            schema_version: 1,
            status: "partial".into(),
            extraction_method: extraction.method.clone(),
            extraction_notes: extraction.quality_notes.clone(),
            failed_steps: failed_steps.iter().map(|step| step.step_label.clone()).collect(),
            limitations: vec![format!("Document preparation is incomplete. Start a new review with a higher limit to finish it. {error}")],
            ..Default::default()
        },
        failed_steps,
        step_outputs: vec![],
        products: Default::default(),
        referee_reports: vec![],
        editor: None,
        report_date: chrono::Local::now().date_naive(),
        paper_hash: extraction.paper_hash.clone(),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn finish(
    app: &crate::emit::EventBus,
    writer: Option<crate::runs::RunWriter>,
    config: &crate::pipeline_config::PipelineConfig,
    extraction: &ExtractionResult,
    settings: &crate::settings::Settings,
    completion: RunCompletion,
    elapsed: std::time::Duration,
    error: &str,
) -> Result<serde_json::Value, String> {
    let report = stopped_report(config, extraction, error);
    let markdown = crate::output::render_markdown(&report, None, elapsed, settings);
    complete_run(
        app,
        writer,
        &report,
        &markdown,
        &extraction.text,
        elapsed,
        settings,
        completion,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn incomplete_preparation_is_never_a_successful_empty_review() {
        let workflow = crate::pipeline_config::workflow_template().unwrap();
        let extraction = ExtractionResult {
            text: String::new(),
            method: "incomplete".into(),
            source_path: "paper.pdf".into(),
            paper_hash: "1234567890abcdef".into(),
            quality_notes: vec![],
        };
        let report = stopped_report(
            &workflow.config,
            &extraction,
            "Not started: this Review reached its limit of 1 model calls",
        );
        assert_eq!(report.quality.status, "partial");
        assert!(report.step_outputs.is_empty());
        assert_eq!(
            report.failed_steps.len(),
            1 + workflow
                .config
                .steps
                .iter()
                .filter(|step| step.enabled)
                .count()
        );
        assert!(report.quality.limitations[0].contains("Start a new review"));

        let root = tempfile::tempdir().unwrap();
        let writer =
            crate::runs::RunWriter::create_in(root.path(), "budget-preparation-test").unwrap();
        let directory = writer.dir().to_owned();
        let completion = RunCompletion {
            author_note: None,
            input_path: extraction.source_path.clone(),
            input_mode: "paper".into(),
            input_interpretation: String::new(),
            input_identity: Default::default(),
            profile_name: "Fixture".into(),
            workflow_source: String::new(),
            workflow_fingerprint: String::new(),
            specialist_catalog_revision: String::new(),
            variables: Default::default(),
            extra_inputs: Default::default(),
            extra_input_sources: Default::default(),
            parent_run_id: None,
        };
        let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
        let settings = crate::settings::Settings {
            max_saved_runs: 0,
            max_saved_run_bytes: 0,
            ..Default::default()
        };
        let run = crate::run_scope::ActiveScope::begin("persist-budget-preparation-test");
        let response = crate::run_scope::scoped(run.scope().clone(), async {
            finish(
                &bus,
                Some(writer),
                &workflow.config,
                &extraction,
                &settings,
                completion,
                std::time::Duration::from_secs(1),
                &report.failed_steps[0].error,
            )
        })
        .await
        .unwrap();
        assert_eq!(response["status"], "partial");
        let saved: PipelineReport =
            serde_json::from_slice(&std::fs::read(directory.join("report.json")).unwrap()).unwrap();
        assert_eq!(saved.quality.status, "partial");
        assert_eq!(saved.failed_steps.len(), report.failed_steps.len());
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["status"], "partial");
        assert!(std::fs::read_to_string(directory.join("report.md"))
            .unwrap()
            .contains("Start a new review"));
    }
}
