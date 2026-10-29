use super::*;

mod document;
mod inputs;
mod orientation;
mod report;
mod visuals;

// This orchestration boundary carries the immutable launch snapshot together
// with the user-supplied input maps and cancellation generation.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_pipeline_inner_with_snapshot(
    app: &crate::emit::EventBus,
    paper_path: &str,
    requested_input_interpretation: Option<&str>,
    diff: bool,
    provided_vars: std::collections::HashMap<String, String>,
    provided_inputs: std::collections::HashMap<String, String>,
    snapshot: Option<RunSnapshot>,
) -> Result<serde_json::Value, String> {
    // A stop that landed before this point (during launch preflight, or while
    // queued) is already recorded in the run scope, which begin_run_state never
    // clears, so the run aborts at its first cancellation checkpoint.
    let _run_state = begin_run_state();
    let pipeline_start = std::time::Instant::now();
    // One immutable settings/profile snapshot defines the entire run. UI
    // edits made while providers are working take effect only on the next run.
    let snapshot = match snapshot {
        Some(snapshot) => snapshot,
        None => load_run_snapshot()?,
    };
    validate_primary_input_selection(
        &snapshot.config,
        Some(paper_path),
        requested_input_interpretation,
    )?;
    validate_named_input_paths(&snapshot.config, &provided_inputs, true)?;
    let RunSnapshot {
        settings,
        mut config,
        profile_name,
        workflow_source,
        workflow_fingerprint,
        workflow_json,
        specialist_catalog_revision,
        ..
    } = snapshot;
    pipeline_config::apply_agent_defaults(&mut config, &settings);
    let _settings_snapshot = crate::settings::freeze_for_run(settings.clone());

    let paper = std::path::Path::new(paper_path);

    // Effective run-time variables: the profile's declared defaults, overlaid
    // with whatever the caller provided. Unknown provided keys are kept (a
    // prompt may reference an ad-hoc var).
    let mut variables: std::collections::HashMap<String, String> = config
        .variables
        .iter()
        .map(|v| (v.key.clone(), v.default.clone()))
        .collect();
    variables.extend(provided_vars);
    validate_runtime_bindings(&config, &variables, &provided_inputs, true)?;
    let durable_variables = durable_variables(&config, &variables);
    crate::safety::validate_runtime_context(&variables, "Run variables")?;
    crate::safety::validate_runtime_context(&provided_inputs, "Named input paths")?;
    crate::safety::validate_run_budget(&config, &settings)?;
    let input_interpretation = resolved_input_interpretation(
        &config.extraction.input_mode,
        paper_path,
        requested_input_interpretation,
    );
    // Legacy callers omit the interpretation, so a folder resolves to
    // source_tree here without passing through the explicit-interpretation
    // validation above; re-apply the auto-contract rule on the resolved value.
    if input_interpretation == "source_tree"
        && crate::auto_review::uses_auto_review_contract(&config)
    {
        return Err(
            "This review reads a single document, not a browsable source tree. Select the \
             folder as a LaTeX project (if it contains the paper's TeX source), pick the paper \
             file directly, or switch to a folder-oriented workflow."
                .to_string(),
        );
    }
    let input_mode = match input_interpretation {
        "source_tree" => "folder",
        "none" => "none",
        _ => "document",
    };
    preflight_run_storage(paper)?;
    let input_processing_label = executor::input_processing_label(input_interpretation);
    let preprocessing_log = start_preprocessing_log();
    let preprocessing_message = if input_mode == "none" {
        input_processing_label.to_string()
    } else {
        format!("{input_processing_label} from {paper_path}")
    };
    crate::pipeline::logging::emit(app, preprocessing_message);

    let run_input_dir = tempfile::Builder::new()
        .prefix("pipeline_run_inputs_")
        .tempdir()
        .map_err(|e| format!("Failed to create run input directory: {e}"))?;
    let staged = extract::stage_selected_source(paper, input_mode, run_input_dir.path())?;
    let scoped_source = super::reuse_artifacts::snapshot_source(&staged, run_input_dir.path())?;
    let captured_path = scoped_source.source_path.as_deref().unwrap_or(paper);
    let captured_path_text = captured_path.to_string_lossy();
    let captured_visual_pdf = if captured_path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    {
        Some(captured_path.to_path_buf())
    } else if let Some(companion) = crate::document_bundle::companion_pdf(paper_path) {
        let destination = run_input_dir.path().join("visual-companion.pdf");
        super::reuse_artifacts::copy_tree(&companion, &destination)?;
        Some(destination)
    } else {
        None
    };

    // Extract paper text
    app.emit_event(
        "pipeline:stage",
        serde_json::json!({
            "stage": "extracting",
            "id": "extracting",
            "label": input_processing_label,
            "stepIds": [],
            "stepLabels": [],
        }),
    )
    .ok();
    let extract_start = std::time::Instant::now();
    let extraction_result = match input_mode {
        "folder" => extract::ingest_folder_async(&captured_path_text).await,
        "none" => Ok(extract::ingest_none()),
        _ => extract::extract(app, &captured_path_text, &config.extraction).await,
    };
    let mut extraction = match extraction_result {
        Ok(value) => value,
        Err(error) => {
            crate::pipeline::logging::emit(
                app,
                format!("ERROR: document preparation failed: {error}"),
            );
            if let Some(path) = preprocessing_log.as_ref() {
                crate::pipeline::logging::emit(
                    app,
                    format!("Preprocessing log retained at {}", path.display()),
                );
            }
            return Err(error);
        }
    };
    let captured_document_path = std::path::PathBuf::from(&extraction.source_path);
    extraction.source_path = paper_path.to_string();
    let extract_secs = extract_start.elapsed().as_secs();
    crate::pipeline::logging::emit(
        app,
        format!(
            "Extracted via {} ({} chars, {}s)",
            extraction.method,
            extraction.text.len(),
            extract_secs
        ),
    );
    for note in &extraction.quality_notes {
        crate::pipeline::logging::emit(app, format!("WARNING: {note}"));
    }
    let input_identity = crate::runs::input_identity(
        paper_path,
        input_interpretation,
        &extraction.paper_hash,
        None,
    );

    // Close/flush the preprocessing sink before copying it into the newly
    // addressable run directory.
    crate::pipeline::logging::set_log_sink(None);
    let (mut run_writer, artifact_write_dir) = create_run_workspace(
        app,
        &extraction.paper_hash,
        crate::runs::RunFinishMeta {
            input_path: paper_path.to_string(),
            input_mode: input_mode.to_string(),
            input_interpretation: input_interpretation.to_string(),
            input_identity: input_identity.clone(),
            profile_id: settings.active_profile.clone(),
            profile_name: profile_name.clone(),
            workflow_source: workflow_source.clone(),
            workflow_fingerprint: workflow_fingerprint.clone(),
            specialist_catalog_revision: specialist_catalog_revision.clone(),
            provider: settings.preferred_provider.clone(),
            variables: durable_variables.clone(),
            ..Default::default()
        },
        preprocessing_log.as_deref(),
    )?;
    if run_writer.is_some() {
        if let Some(path) = preprocessing_log.as_ref() {
            let _ = std::fs::remove_file(path);
        }
    }
    if let Some(w) = run_writer.as_mut() {
        w.add_text(
            "context/workflow.json",
            "Workflow snapshot",
            "context",
            &workflow_json,
        )
        .map_err(|error| format!("Could not save the workflow snapshot: {error}"))?;
        w.add_text(
            crate::runs::DOCUMENT_TEXT_PATH,
            "Readable document",
            "document",
            &extraction.text,
        )
        .map_err(|error| format!("Could not save the extracted document: {error}"))?;
        if extraction.method == "paddleocr-vl-full" {
            match crate::pipeline::extract::read_paddle_structure_json_for_method(
                &extraction.paper_hash,
                &extraction.method,
            ) {
                Ok(Some(structure)) => {
                    if let Err(e) = w.add_text(
                        "context/paddle_structure.json",
                        "PaddleOCR-VL full-parser structure",
                        "context",
                        &structure,
                    ) {
                        let _ = app.emit_event(
                            "pipeline:log",
                            serde_json::json!({
                                "line": format!("WARNING: {e}")
                            }),
                        );
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!(
                                "WARNING: could not retain PaddleOCR-VL structure: {e}"
                            )
                        }),
                    );
                }
            }
        }
    }

    visuals::collect_visual_artifacts(
        app,
        input_mode,
        captured_visual_pdf,
        &extraction,
        &mut run_writer,
    )
    .await;

    if is_cancelled() {
        return Err("Pipeline cancelled".into());
    }

    let mut document_bundle =
        document::build_document_bundle(app, &extraction, &captured_document_path, &mut run_writer);

    if let Some(writer) = run_writer.as_mut() {
        super::reuse_artifacts::retain_source(&scoped_source, writer)?;
    }
    let scoped_source_path = scoped_source
        .source_path
        .as_deref()
        .and_then(|path| crate::pipeline::claude::normalize_cli_root(&path.to_string_lossy()))
        .unwrap_or_default();
    let scoped_source_read_root = scoped_source
        .read_root
        .as_deref()
        .and_then(|path| crate::pipeline::claude::normalize_cli_root(&path.to_string_lossy()));
    let usage_before_orientation = crate::pipeline::logging::run_usage();
    let orientation::OrientedDocument {
        orientation,
        orientation_path,
        orientation_sampling,
        _orient_tmp,
    } = orientation::prepare_orientation(
        app,
        &config,
        input_mode,
        &extraction,
        &scoped_source_read_root,
        &run_input_dir,
        &mut run_writer,
    )
    .await?;
    // Review limits count document preparation and orientation too.
    let budget_start = executor::budget::BudgetStart::after_orientation(
        pipeline_start,
        &usage_before_orientation,
        &settings,
    )
    .await;

    // Adaptive reviews store only their stable core skeleton. Once the
    // validated router has selected allowlisted IDs, assemble this run's
    // small concrete workflow before any step scheduling or budget checks.
    let mut execution_config = crate::auto_review::materialize_config(&config, &orientation)?;
    pipeline_config::apply_agent_defaults(&mut execution_config, &settings);
    crate::pipeline_config::validate_runtime_config(&execution_config)?;
    crate::safety::validate_run_budget(&execution_config, &settings)?;

    let (bundle_json, bundle_markdown) = document::persist_oriented_bundle(
        app,
        &mut document_bundle,
        &orientation,
        &extraction,
        &mut run_writer,
    )?;

    // New steps receive the readable bundle projection. Runs where bundle
    // construction was unavailable receive the exact canonical document.
    let paper_document_text = bundle_markdown.as_deref().unwrap_or(&extraction.text);
    let (_paper_tmp, paper_text_path) = write_run_input_file(
        run_input_dir.path(),
        "pipeline_document_",
        ".md",
        paper_document_text,
        "document-view",
    )?;
    let mut _bundle_tmp = None;
    let document_bundle_path = if let Some(json) = bundle_json.as_deref() {
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

    // {paper_type} resolves only for paper-shaped surveys; empty otherwise.
    let paper_type = crate::models::paper_view(&orientation)
        .map(|v| v.metadata.paper_type.to_string())
        .unwrap_or_default();
    let survey_hint = crate::models::survey_hint(&orientation);

    let inputs::NamedInputs {
        resolved_inputs,
        persisted_inputs,
        extra_tmps,
        runtime_extra_sources,
        extra_sources,
    } = inputs::capture_named_inputs(
        app,
        &config,
        &provided_inputs,
        &run_input_dir,
        &mut run_writer,
    )
    .await?;
    if is_cancelled() {
        return Err("Pipeline cancelled".into());
    }

    let result = executor::budget::counted_from(
        budget_start,
        executor::execute_steps(
            app,
            &execution_config,
            &orientation_path,
            &orientation,
            &paper_text_path,
            &document_bundle_path,
            &scoped_source_path,
            &paper_type,
            &survey_hint,
            &variables,
            &resolved_inputs,
            &runtime_extra_sources,
            &std::collections::HashMap::new(), // no preloaded steps for a fresh run
            artifact_write_dir.as_deref(),
            &settings,
        ),
    )
    .await?;
    drop(extra_tmps); // keep temp files alive until steps have run

    // Steps are done — close the write window before rendering/reconciling.

    if is_cancelled() {
        return Err("Pipeline cancelled".into());
    }

    let report = report::assemble_report(
        app,
        &mut run_writer,
        &execution_config,
        result,
        &extraction,
        &document_bundle,
        &orientation_sampling,
        orientation,
        &scoped_source_path,
    );

    // Optional diff
    let mut diff_text = None;
    if diff || settings.auto_revision_reconciliation {
        if let Ok(Some(prior)) =
            crate::runs::load_latest_report_for_input(paper_path, &extraction.paper_hash)
        {
            if let Ok(dt) = reconcile::reconcile(app, &prior, &report).await {
                diff_text = Some(dt);
            }
        }
    }
    let elapsed = pipeline_start.elapsed();
    let markdown = output::render_markdown(&report, diff_text.as_deref(), elapsed, &settings);

    complete_run(
        app,
        run_writer,
        &report,
        &markdown,
        &extraction.text,
        elapsed,
        &settings,
        RunCompletion {
            input_path: paper_path.to_string(),
            input_mode: input_mode.to_string(),
            input_interpretation: input_interpretation.to_string(),
            input_identity,
            profile_name,
            workflow_source,
            workflow_fingerprint,
            specialist_catalog_revision,
            variables: durable_variables,
            extra_inputs: persisted_inputs,
            extra_input_sources: extra_sources,
            parent_run_id: None,
        },
    )
}
