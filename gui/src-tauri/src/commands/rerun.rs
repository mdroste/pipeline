use super::*;

mod document;
mod files;
mod inputs;
mod reuse;

pub(super) use files::{load_run_report, load_run_report_in, read_run_file, read_run_file_in};
use reuse::orientation_contract_error;
#[cfg(test)]
pub(super) use reuse::{
    collect_preloaded_outputs, failed_base_ids, incompatible_reuse_ids, resume_seed_ids,
};

// --- Resume / partial re-run (1.3.2) ---

/// Re-run a past run, reusing its captured document and orientation and (for
/// partial modes) its successful step outputs, so only the necessary steps
/// re-execute. Uses the *active* profile's steps, so editing a prompt and
/// re-running is cheap. Modes:
///   - `from_step = Some(id)`: reuse steps before `id`; re-run `id` onward.
///   - `only_failed = true`: reuse successful steps except those downstream of
///     a failed step.
///   - neither: reuse only extraction/orientation; re-run every step.
#[tauri::command]
pub async fn rerun_run(
    app: AppHandle,
    run_id: String,
    from_step: Option<String>,
    only_failed: bool,
) -> Result<serde_json::Value, String> {
    crate::runs::recover_resumable_runs()?;
    let guard = acquire_run_slot()?;
    let run = crate::run_scope::ActiveScope::begin(super::run_queue::next_run_key());
    let bus = crate::emit::from_app(app);
    PipelineTask::spawn_future(run.scope().clone(), async move {
        let _guard = guard;
        rerun_run_inner(&bus, &run_id, from_step, only_failed).await
    })
    .join()
    .await
}

pub(super) async fn rerun_run_inner(
    app: &crate::emit::EventBus,
    parent_run_id: &str,
    from_step: Option<String>,
    only_failed: bool,
) -> Result<serde_json::Value, String> {
    let _run_state = begin_run_state();
    let start = std::time::Instant::now();
    let settings = crate::settings::load_persisted_required()
        .map_err(|e| format!("Cannot re-run because settings could not be loaded safely: {e}"))?;
    let _settings_snapshot = crate::settings::freeze_for_run(settings.clone());

    let parent = crate::runs::load_manifest(parent_run_id)?;
    preflight_run_storage(std::path::Path::new(&parent.input_path))?;
    let report_json = read_run_file(parent_run_id, "report.json")
        .map_err(|_| "This run predates re-run support (no report.json). Re-run is only available for runs created after upgrading.".to_string())?;
    let parent_report: PipelineReport = serde_json::from_str(&report_json)
        .map_err(|e| format!("Invalid parent report.json: {e}"))?;
    let input_identity = crate::runs::input_identity(
        &parent.input_path,
        &parent.input_interpretation,
        &parent_report.paper_hash,
        Some(&parent.input_identity),
    );
    let parent_dir = crate::runs::runs_dir()?.join(parent_run_id);
    let document_rel = crate::runs::captured_document_rel_path(&parent_dir, &parent)?;
    let document_text = read_run_file(parent_run_id, document_rel)?;
    let mut orientation_value: serde_json::Value = parent_report.orientation.clone();

    // Active profile drives the re-run (edited prompts take effect).
    let workflow = pipeline_config::load_required_workflow_for(&settings.active_profile)?;
    let mut config = workflow.config.clone();
    let profile_name = workflow.name.clone();
    let workflow_source = format!("profile:{}", settings.active_profile);
    let workflow_fingerprint = workflow.fingerprint.clone();
    let workflow_json = workflow.canonical_json.clone();
    pipeline_config::validate_enabled_sequential_step(&config.steps)?;
    crate::auto_review::validate_auto_review_preflight(&config)?;
    let specialist_catalog_revision = if crate::auto_review::uses_auto_review_contract(&config) {
        crate::auto_review::catalog_revision().to_string()
    } else {
        String::new()
    };
    // A source-tree parent captured only a file inventory as its document
    // text; the adaptive router and reviewers would treat that inventory as
    // the paper (same rule as a fresh launch).
    let parent_is_source_tree = parent.input_interpretation == "source_tree"
        || (parent.input_interpretation.is_empty() && parent.input_mode == "folder");
    if parent_is_source_tree && crate::auto_review::uses_auto_review_contract(&config) {
        return Err(
            "This review reads a single document, not a browsable source tree. Re-run this \
             folder input with a folder-oriented workflow, or start a new run on the paper \
             file itself."
                .to_string(),
        );
    }
    pipeline_config::apply_agent_defaults(&mut config, &settings);
    crate::safety::validate_runtime_context(&parent.variables, "Re-run variables")?;
    crate::safety::validate_runtime_context(
        &parent.extra_input_sources,
        "Re-run named input paths",
    )?;
    crate::safety::validate_runtime_context(&parent.extra_inputs, "Re-run captured input paths")?;
    validate_runtime_bindings(
        &config,
        &parent.variables,
        &parent.extra_input_sources,
        true,
    )?;
    let durable_variables = durable_variables(&config, &parent.variables);
    crate::safety::validate_run_budget(&config, &settings)?;

    // Keep all reconstructed model-readable inputs in one private root.
    // If the original selection has since disappeared, captured text can still
    // support steps that did not select the source artifact.
    let run_input_dir = tempfile::Builder::new()
        .prefix("pipeline_run_inputs_")
        .tempdir()
        .map_err(|e| format!("Failed to create run input directory: {e}"))?;
    let source_required = super::reuse_artifacts::requires_primary_source(&config);
    let scoped_source =
        super::reuse_artifacts::restore_source(&parent_dir, run_input_dir.path(), source_required)?;
    let scoped_source_path = scoped_source
        .source_path
        .as_deref()
        .and_then(|path| crate::pipeline::claude::normalize_cli_root(&path.to_string_lossy()))
        .unwrap_or_default();
    let scoped_source_read_root = scoped_source
        .read_root
        .as_deref()
        .and_then(|path| crate::pipeline::claude::normalize_cli_root(&path.to_string_lossy()));

    // The active profile drives a re-run, so a cached orientation is reusable
    // only when it still satisfies that profile's current schema and semantic
    // contract. Rebuilding it invalidates every downstream step.
    let orientation_error =
        orientation_contract_error(&config, &orientation_value, &parent.input_mode).err();
    let orientation_rebuilt = orientation_error.is_some();
    let usage_before_orientation = crate::pipeline::logging::run_usage();
    if let Some(contract_error) = orientation_error {
        let orientation_label = if crate::auto_review::uses_auto_review_contract(&config) {
            "Creating orientation map & review plan"
        } else {
            "Creating orientation map"
        };
        app.emit_event("pipeline:log", serde_json::json!({
            "line": format!(
                "Re-run: rebuilding the parent orientation because it does not satisfy the active workflow ({contract_error})"
            )
        }))
            .ok();
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
        let extraction = crate::models::ExtractionResult {
            text: document_text.clone(),
            method: "resumed-document".to_string(),
            source_path: parent.input_path.clone(),
            paper_hash: parent_report.paper_hash.clone(),
            quality_notes: Vec::new(),
        };
        let survey_template =
            orient::resolve_survey_template(&config.orientation_prompt, &parent.input_mode);
        let survey_schema = orient::resolve_survey_schema(
            &config.orientation_prompt,
            config.orientation_schema.as_ref(),
            &parent.input_mode,
        );
        orientation_value = orient::build_orientation_map(
            app,
            &extraction,
            survey_template.as_deref(),
            survey_schema.as_ref(),
            scoped_source_read_root.as_deref(),
        )
        .await?;
        if is_cancelled() {
            return Err("Pipeline cancelled".into());
        }
    }
    // Review limits count from the start of the re-run, including a rebuilt
    // orientation.
    let budget_start = executor::budget::BudgetStart::after_orientation(
        start,
        &usage_before_orientation,
        &settings,
    )
    .await;

    // Reconstruct the same bounded Auto Review workflow before deciding what
    // can be reused. Dynamic specialist IDs are ordinary steps from here on.
    let mut execution_config = crate::auto_review::materialize_config(&config, &orientation_value)?;
    pipeline_config::apply_agent_defaults(&mut execution_config, &settings);
    crate::pipeline_config::validate_runtime_config(&execution_config)?;
    crate::safety::validate_run_budget(&execution_config, &settings)?;

    if scoped_source_path.is_empty()
        && super::reuse_artifacts::requires_primary_source(&execution_config)
    {
        return Err("The expanded workflow requires source files that this run did not capture. Start a new run.".into());
    }

    let preloaded = reuse::select_reusable_outputs(
        app,
        parent_run_id,
        &parent_dir,
        &execution_config,
        &parent_report,
        orientation_rebuilt,
        from_step,
        only_failed,
    );

    let extra_input_sources = parent.extra_input_sources.clone();
    let (mut run_writer, artifact_write_dir) = create_run_workspace(
        app,
        &parent_report.paper_hash,
        crate::runs::RunFinishMeta {
            input_path: parent.input_path.clone(),
            input_mode: parent.input_mode.clone(),
            input_interpretation: parent.input_interpretation.clone(),
            input_identity: input_identity.clone(),
            profile_id: settings.active_profile.clone(),
            profile_name: profile_name.clone(),
            workflow_source: workflow_source.clone(),
            workflow_fingerprint: workflow_fingerprint.clone(),
            specialist_catalog_revision: specialist_catalog_revision.clone(),
            provider: settings.preferred_provider.clone(),
            variables: durable_variables.clone(),
            parent_run_id: Some(parent_run_id.to_string()),
            ..Default::default()
        },
        None,
    )?;
    if let Some(root) = artifact_write_dir.as_deref() {
        super::reuse_artifacts::restore_producers(
            &parent_dir,
            std::path::Path::new(root),
            preloaded.keys().cloned(),
        )?;
    }
    if let Some(writer) = run_writer.as_mut() {
        super::reuse_artifacts::retain_source(&scoped_source, writer)?;
        writer
            .add_text(
                "context/workflow.json",
                "Workflow snapshot",
                "context",
                &workflow_json,
            )
            .map_err(|error| format!("Could not save the workflow snapshot: {error}"))?;
        for (key, path) in &extra_input_sources {
            writer
                .record_extra_input_source(key, path)
                .map_err(|error| format!("Could not record named input '{key}': {error}"))?;
        }
    }
    let resumed_extraction = crate::models::ExtractionResult {
        text: document_text.clone(),
        method: "resumed-document".to_string(),
        source_path: parent.input_path.clone(),
        paper_hash: parent_report.paper_hash.clone(),
        quality_notes: Vec::new(),
    };
    let orientation_sampling = orient::orientation_sample(&document_text);
    let document::RestoredDocument {
        rerun_bundle_json,
        report_bundle,
        rerun_document_text,
    } = document::restore_document_context(
        app,
        parent_run_id,
        &document_text,
        &orientation_value,
        &orientation_sampling,
        &resumed_extraction,
        &scoped_source,
        &mut run_writer,
    )?;

    // Document + orientation + named-input temp files for the steps to
    // Read, confined to the same private root as the staged source context.
    let (_text_tmp, paper_text_path) = write_run_input_file(
        run_input_dir.path(),
        "pipeline_document_",
        ".md",
        &rerun_document_text,
        "document-view",
    )?;
    let mut _bundle_tmp = None;
    let document_bundle_path = if let Some(json) = rerun_bundle_json.as_deref() {
        let (file, path) = write_run_input_file(
            run_input_dir.path(),
            "pipeline_document_bundle_",
            ".json",
            json,
            "document-bundle",
        )?;
        _bundle_tmp = Some(file);
        path
    } else {
        String::new()
    };

    let orient_json = serde_json::to_string(&orientation_value).unwrap_or_default();
    let (_orient_tmp, orientation_path) = write_run_input_file(
        run_input_dir.path(),
        "pipeline_orient_",
        ".json",
        &orient_json,
        "orientation",
    )?;

    let inputs::RestoredInputs {
        runtime_extra_sources,
        resolved_inputs,
        persisted_inputs,
        extra_tmps,
    } = inputs::restore_named_inputs(
        parent_run_id,
        &parent_dir,
        &parent,
        &config,
        &execution_config,
        &run_input_dir,
        &mut run_writer,
    )?;

    let paper_type = crate::models::paper_view(&orientation_value)
        .map(|v| v.metadata.paper_type.to_string())
        .unwrap_or_default();
    let survey_hint = crate::models::survey_hint(&orientation_value);
    let variables = parent.variables.clone();

    let result = executor::budget::counted_from(
        budget_start,
        executor::execute_steps(
            app,
            &execution_config,
            &orientation_path,
            &orientation_value,
            &paper_text_path,
            &document_bundle_path,
            &scoped_source_path,
            &paper_type,
            &survey_hint,
            &variables,
            &resolved_inputs,
            &runtime_extra_sources,
            &preloaded,
            artifact_write_dir.as_deref(),
            &settings,
        ),
    )
    .await?;
    drop(extra_tmps);
    if is_cancelled() {
        return Err("Pipeline cancelled".into());
    }

    let mut products = crate::findings::build_run_products(&execution_config, &result.outputs);
    if let Some(writer) = run_writer.as_mut() {
        for warning in
            writer.capture_source_evidence(&mut products, std::path::Path::new(&scoped_source_path))
        {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!("WARNING: {warning}") }),
            );
        }
    }
    crate::pipeline::analyzers::quotes::verify_run(
        &mut products,
        report_bundle.as_ref(),
        &resumed_extraction,
        run_writer.as_ref().map(|writer| writer.dir()),
    );
    let quality = output::build_report_quality(
        &resumed_extraction,
        report_bundle.as_ref(),
        &orientation_sampling,
        &execution_config,
        &result.outputs,
        &result.failed_steps,
        &products,
    );
    let report = PipelineReport {
        orientation: orientation_value,
        step_outputs: result.outputs,
        failed_steps: result.failed_steps,
        products,
        quality,
        referee_reports: vec![],
        editor: None,
        report_date: chrono::Local::now().date_naive(),
        paper_hash: parent_report.paper_hash.clone(),
    };
    let elapsed = start.elapsed();
    let markdown = output::render_markdown(&report, None, elapsed, &settings);

    complete_run(
        app,
        run_writer,
        &report,
        &markdown,
        &document_text,
        elapsed,
        &settings,
        RunCompletion {
            input_path: parent.input_path,
            input_mode: parent.input_mode,
            input_interpretation: parent.input_interpretation,
            input_identity,
            profile_name,
            workflow_source,
            workflow_fingerprint,
            specialist_catalog_revision,
            variables: durable_variables,
            extra_inputs: persisted_inputs,
            extra_input_sources,
            parent_run_id: Some(parent_run_id.to_string()),
        },
    )
}
