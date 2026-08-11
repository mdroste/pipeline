use super::*;

pub(super) struct RunCompletion {
    pub(super) input_path: String,
    pub(super) input_mode: String,
    pub(super) input_interpretation: String,
    pub(super) profile_name: String,
    pub(super) variables: std::collections::HashMap<String, String>,
    pub(super) extra_inputs: std::collections::HashMap<String, String>,
    pub(super) extra_input_sources: std::collections::HashMap<String, String>,
    pub(super) parent_run_id: Option<String>,
}

/// Finalize persistence, retention, events, and the public response in one
/// place so fresh runs and re-runs cannot drift in status or metadata rules.
#[allow(clippy::too_many_arguments)]
pub(super) fn complete_run(
    app: &crate::emit::EventBus,
    writer: Option<crate::runs::RunWriter>,
    report: &PipelineReport,
    markdown: &str,
    extracted_text: &str,
    elapsed: std::time::Duration,
    settings: &crate::settings::Settings,
    completion: RunCompletion,
) -> serde_json::Value {
    let status = if report.failed_steps.is_empty() {
        "done"
    } else {
        "partial"
    };
    let meta = crate::runs::RunFinishMeta {
        input_path: completion.input_path,
        input_mode: completion.input_mode,
        input_interpretation: completion.input_interpretation,
        profile_id: settings.active_profile.clone(),
        profile_name: completion.profile_name,
        provider: settings.preferred_provider.clone(),
        status: status.to_string(),
        duration_secs: elapsed.as_secs(),
        usage: crate::pipeline::logging::run_usage(),
        step_count: report.all_outputs().len() as u32,
        failed_steps: report
            .failed_steps
            .iter()
            .map(|failure| failure.step_label.clone())
            .collect(),
        variables: completion.variables,
        extra_inputs: completion.extra_inputs,
        extra_input_sources: completion.extra_input_sources,
        parent_run_id: completion.parent_run_id,
    };
    let run_id = writer.and_then(|writer| finalize_run(app, writer, report, markdown, meta));
    enforce_retention(
        app,
        settings.max_saved_runs as usize,
        settings.max_saved_run_bytes,
    );
    app.emit_event(
        "pipeline:stage",
        serde_json::json!({
            "stage": "done",
            "id": "done",
            "label": "Complete",
            "stepIds": [],
        }),
    )
    .ok();

    serde_json::json!({
        "report": report,
        "markdown": markdown,
        "extracted_text": extracted_text,
        "run_id": run_id,
        "status": status,
    })
}

#[tauri::command]
pub async fn run_pipeline(
    app: AppHandle,
    paper_path: String,
    input_interpretation: Option<String>,
    diff: bool,
    variables: Option<std::collections::HashMap<String, String>>,
    extra_inputs: Option<std::collections::HashMap<String, String>>,
    expected_profile_snapshot_id: Option<String>,
) -> Result<serde_json::Value, String> {
    // Prevent concurrent pipeline runs from corrupting shared state
    let _ = crate::runs::recover_resumable_runs();
    let variables = variables.unwrap_or_default();
    let extra_inputs = extra_inputs.unwrap_or_default();
    crate::safety::validate_runtime_context(&variables, "Run variables")?;
    crate::safety::validate_runtime_context(&extra_inputs, "Named input paths")?;
    let snapshot = load_run_snapshot()?;
    validate_primary_input_selection(
        &snapshot.config,
        Some(&paper_path),
        input_interpretation.as_deref(),
    )?;
    validate_named_input_paths(&snapshot.config, &extra_inputs, true)?;
    let snapshot = bind_runtime_snapshot(snapshot, &variables, &extra_inputs)?;
    let snapshot =
        bind_foreground_launch(snapshot, &paper_path, input_interpretation.as_deref(), diff)?;
    let expected_profile_snapshot_id = expected_profile_snapshot_id
        .as_deref()
        .filter(|expected| !expected.is_empty())
        .ok_or(
            "Run setup has not been prepared. Review the execution plan before launching the run.",
        )?;
    if expected_profile_snapshot_id != snapshot.fingerprint {
        return Err(
            "The active profile or run options changed after the execution plan was prepared. Review the updated plan and run again."
                .to_string(),
        );
    }
    let dependencies =
        check_snapshot_dependencies(&snapshot, diff, Some(&paper_path), &extra_inputs).await?;
    require_snapshot_dependencies(&dependencies)?;
    let guard = acquire_pipeline_guard()?;
    let bus = crate::emit::from_app(app);
    PipelineTask::spawn(
        guard,
        PipelineRequest {
            bus,
            paper_path,
            input_interpretation,
            diff,
            variables,
            extra_inputs,
            snapshot: Some(snapshot),
        },
    )
    .join()
    .await
}

/// Complete invocation state for a headless run. Unlike the desktop command,
/// a CLI caller may select a profile without changing the persisted active
/// profile. An empty `input_path` is valid only for a no-input workflow.
#[derive(Debug, Clone, Default)]
pub struct HeadlessRunOptions {
    pub profile_id: Option<String>,
    pub input_path: String,
    pub input_interpretation: Option<String>,
    pub variables: std::collections::HashMap<String, String>,
    pub extra_inputs: std::collections::HashMap<String, String>,
}

async fn prepare_headless_run(
    options: &HeadlessRunOptions,
) -> Result<(RunSnapshot, crate::deps::DepsReport), String> {
    crate::safety::validate_runtime_context(&options.variables, "Run variables")?;
    crate::safety::validate_runtime_context(&options.extra_inputs, "Named input paths")?;
    let snapshot = load_run_snapshot_for_profile(options.profile_id.as_deref())?;
    validate_primary_input_selection(
        &snapshot.config,
        Some(&options.input_path),
        options.input_interpretation.as_deref(),
    )?;
    validate_named_input_paths(&snapshot.config, &options.extra_inputs, true)?;
    let snapshot = bind_runtime_snapshot(snapshot, &options.variables, &options.extra_inputs)?;
    let snapshot = bind_foreground_launch(
        snapshot,
        &options.input_path,
        options.input_interpretation.as_deref(),
        false,
    )?;
    // Passing the concrete path is what makes `deps::check_snapshot` require
    // the selected PDF parser for PDF inputs while leaving TeX/DOCX runs free
    // of PDF-only dependencies.
    let dependency_input =
        (!options.input_path.trim().is_empty()).then_some(options.input_path.as_str());
    let dependencies =
        check_snapshot_dependencies(&snapshot, false, dependency_input, &options.extra_inputs)
            .await?;
    Ok((snapshot, dependencies))
}

/// Check the exact dependency set for one prospective headless run without
/// starting extraction or contacting a model. The CLI `check` command and the
/// execution path both call this preparation routine.
pub async fn check_headless_dependencies(
    options: &HeadlessRunOptions,
) -> Result<crate::deps::DepsReport, String> {
    prepare_headless_run(options)
        .await
        .map(|(_, dependencies)| dependencies)
}

/// Headless entry point for the CLI. It performs the same concrete input,
/// named-input, run-budget, and provider dependency preflight as the GUI, then
/// runs one immutable settings/profile snapshot without a Tauri `AppHandle`.
pub async fn run_headless_with_options(
    bus: crate::emit::EventBus,
    options: HeadlessRunOptions,
) -> Result<serde_json::Value, String> {
    let _ = crate::runs::recover_resumable_runs();
    let (snapshot, dependencies) = prepare_headless_run(&options).await?;
    require_snapshot_dependencies(&dependencies)?;
    let guard = acquire_pipeline_guard()?;
    PipelineTask::spawn(
        guard,
        PipelineRequest {
            bus,
            paper_path: options.input_path,
            input_interpretation: options.input_interpretation,
            diff: false,
            variables: options.variables,
            extra_inputs: options.extra_inputs,
            snapshot: Some(snapshot),
        },
    )
    .join()
    .await
}

/// Compatibility wrapper for existing crate callers. New headless clients
/// should use `run_headless_with_options` so they can select a profile and an
/// explicit input interpretation.
pub async fn run_headless(
    bus: crate::emit::EventBus,
    paper_path: &str,
    variables: std::collections::HashMap<String, String>,
    extra_inputs: std::collections::HashMap<String, String>,
) -> Result<serde_json::Value, String> {
    run_headless_with_options(
        bus,
        HeadlessRunOptions {
            input_path: paper_path.to_string(),
            variables,
            extra_inputs,
            ..Default::default()
        },
    )
    .await
}
