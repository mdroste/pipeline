use super::*;

pub(super) async fn run_pipeline_inner_with_snapshot(
    app: &crate::emit::EventBus,
    paper_path: &str,
    requested_input_interpretation: Option<&str>,
    diff: bool,
    provided_vars: std::collections::HashMap<String, String>,
    provided_inputs: std::collections::HashMap<String, String>,
    snapshot: Option<RunSnapshot>,
    preflight_cancel_epoch: u64,
) -> Result<serde_json::Value, String> {
    let _run_state = begin_run_state();
    // A batch "stop" that landed between the batch loop's pre-check and this
    // reset would otherwise be erased by begin_run_state, letting this job run
    // to completion after the user cancelled. Re-assert it so the job aborts at
    // its first cancellation checkpoint. BATCH_CANCEL is false outside an active
    // batch (reset at batch start and end), so foreground runs are unaffected.
    if BATCH_CANCEL.load(std::sync::atomic::Ordering::Acquire) {
        CANCEL_FLAG.store(true, std::sync::atomic::Ordering::Release);
        signal_cancellation();
    }
    // The same race exists for a foreground "Cancel report" click: the launch
    // command spends seconds in preflight (dependency probes, resumable-run
    // recovery) before this task resets CANCEL_FLAG above. The caller captures
    // the cancel epoch at command entry; if it advanced since, the user
    // cancelled this launch — re-assert instead of silently erasing the click.
    if current_cancel_epoch() != preflight_cancel_epoch {
        CANCEL_FLAG.store(true, std::sync::atomic::Ordering::Release);
        signal_cancellation();
    }
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
            "Auto Paper Review reviews a document, not a browsable source tree. Select the \
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
    let input_processing_label = executor::input_processing_label(input_interpretation);
    let preprocessing_log = start_preprocessing_log();
    let preprocessing_message = if input_mode == "none" {
        input_processing_label.to_string()
    } else {
        format!("{input_processing_label} from {paper_path}")
    };
    crate::pipeline::logging::emit(app, preprocessing_message);

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
        "folder" => extract::ingest_folder_async(paper_path).await,
        "none" => Ok(extract::ingest_none()),
        _ => extract::extract(app, paper_path, &config.extraction).await,
    };
    let extraction = match extraction_result {
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
            profile_id: settings.active_profile.clone(),
            profile_name: profile_name.clone(),
            provider: settings.preferred_provider.clone(),
            variables: variables.clone(),
            ..Default::default()
        },
        preprocessing_log.as_deref(),
    );
    if run_writer.is_some() {
        if let Some(path) = preprocessing_log.as_ref() {
            let _ = std::fs::remove_file(path);
        }
    }
    if let Some(w) = run_writer.as_mut() {
        if let Err(e) = w.add_text(
            crate::runs::DOCUMENT_TEXT_PATH,
            "Readable document",
            "document",
            &extraction.text,
        ) {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!("WARNING: {e}")
                }),
            );
        }
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

    // Render page images for PDF inputs, and for LaTeX projects that have a
    // compiled companion PDF. The semantic LaTeX source remains primary, but
    // the page renders preserve the visual evidence needed to inspect figures,
    // tables, equation layout, and extraction quality.
    let visual_pdf = if extraction
        .source_path
        .to_ascii_lowercase()
        .ends_with(".pdf")
    {
        Some(std::path::PathBuf::from(&extraction.source_path))
    } else {
        crate::document_bundle::companion_pdf(&extraction.source_path)
    };
    if input_mode == "document" {
        if let (Some(pdf), Some(w)) = (visual_pdf, run_writer.as_mut()) {
            let out_dir = w.dir().join("artifacts").join("pages");
            let rendered = tokio::task::spawn_blocking(move || {
                crate::pipeline::extract::render_pdf_pages(
                    &pdf,
                    &out_dir,
                    crate::pipeline::extract::MAX_RENDERED_PDF_PAGES,
                )
            })
            .await;
            match rendered {
                Ok(Ok(rendered)) => {
                    let names = rendered.names;
                    let count = names.len();
                    for name in &names {
                        let page_num = std::path::Path::new(name)
                            .file_stem()
                            .and_then(|stem| stem.to_str())
                            .unwrap_or(name)
                            .rsplit('-')
                            .next()
                            .and_then(|n| n.parse::<u32>().ok());
                        let label = match page_num {
                            Some(n) => format!("Page {n}"),
                            None => name.clone(),
                        };
                        if let Err(e) =
                            w.register_existing(&format!("artifacts/pages/{name}"), &label, "pages")
                        {
                            let _ = app.emit_event(
                                "pipeline:log",
                                serde_json::json!({
                                    "line": format!("WARNING: {e}")
                                }),
                            );
                        }
                    }
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!("Rendered {count} page images into the run artifacts")
                        }),
                    );
                    if rendered.truncated {
                        let _ = app.emit_event(
                            "pipeline:log",
                            serde_json::json!({
                                "line": format!(
                                    "WARNING: page image rendering reached the {}-page cap; a longer PDF is truncated in the artifact view",
                                    crate::pipeline::extract::MAX_RENDERED_PDF_PAGES
                                )
                            }),
                        );
                    }
                }
                Ok(Err(e)) => {
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!("WARNING: page image rendering skipped: {e}")
                        }),
                    );
                }
                Err(e) => {
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!("WARNING: page image rendering task failed: {e}")
                        }),
                    );
                }
            }
        }
    }

    if extraction.method == "paddleocr-vl-full" {
        if let Some(w) = run_writer.as_mut() {
            let inventory =
                match crate::pipeline::extract::paddle_full_image_inventory(&extraction.paper_hash)
                {
                    Ok(inventory) => inventory,
                    Err(error) => {
                        let _ = app.emit_event(
                            "pipeline:log",
                            serde_json::json!({
                                "line": format!(
                                    "WARNING: full-parser image discovery stopped: {error}"
                                )
                            }),
                        );
                        Default::default()
                    }
                };
            if !inventory.artifacts.is_empty() {
                let figures_dir = w.dir().join("artifacts").join("figures");
                if let Err(error) = std::fs::create_dir_all(&figures_dir) {
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!(
                                "WARNING: could not create full-parser figures directory: {error}"
                            )
                        }),
                    );
                } else {
                    let mut copied = 0usize;
                    for artifact in &inventory.artifacts {
                        let Some(name) = artifact
                            .source_path
                            .file_name()
                            .and_then(|name| name.to_str())
                        else {
                            continue;
                        };
                        if std::fs::copy(&artifact.source_path, figures_dir.join(name)).is_ok()
                            && w.register_existing(
                                &format!("artifacts/figures/{name}"),
                                &artifact.display_name,
                                "figures",
                            )
                            .is_ok()
                        {
                            copied += 1;
                        }
                    }
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!(
                                "Collected {copied} image artifact(s) from PaddleOCR-VL Full Parser{}",
                                if inventory.disambiguated_labels == 0 {
                                    String::new()
                                } else {
                                    format!(
                                        " (disambiguated {} repeated figure/table label(s))",
                                        inventory.disambiguated_labels
                                    )
                                }
                            )
                        }),
                    );
                }
            }
        }
    }
    if is_cancelled() {
        return Err("Pipeline cancelled".into());
    }

    // Build the durable, source-neutral document model after visual artifacts
    // have been collected. It will be enriched with orientation metadata and
    // persisted after the orientation stage below.
    let mut document_bundle = None;
    if let Some(w) = run_writer.as_mut() {
        match crate::document_bundle::build(&extraction, w.dir()) {
            Ok(build) => {
                for (rel_path, label, group) in &build.added_artifacts {
                    if let Err(error) = w.register_existing(rel_path, label, group) {
                        let _ = app.emit_event(
                            "pipeline:log",
                            serde_json::json!({
                                "line": format!("WARNING: {error}")
                            }),
                        );
                    }
                }
                document_bundle = Some(build.bundle);
            }
            Err(error) => {
                let _ = app.emit_event(
                    "pipeline:log",
                    serde_json::json!({
                        "line": format!(
                            "WARNING: could not build the structured document bundle: {error}"
                        )
                    }),
                );
            }
        }
    }

    // Keep every model-readable transient for this run under one private
    // directory. CLI providers can grant this root without exposing unrelated
    // files in the process-wide temp directory.
    let run_input_dir = tempfile::Builder::new()
        .prefix("pipeline_run_inputs_")
        .tempdir()
        .map_err(|e| format!("Failed to create run input directory: {e}"))?;
    let scoped_source =
        match extract::stage_selected_source(paper, input_mode, run_input_dir.path()) {
            Ok(context) => context,
            Err(error) => {
                app.emit_event(
                    "pipeline:log",
                    serde_json::json!({
                        "line": format!(
                            "WARNING: source context is unavailable to review steps: {error}"
                        )
                    }),
                )
                .ok();
                extract::ScopedSourceContext::default()
            }
        };
    let scoped_source_path = scoped_source
        .source_path
        .as_deref()
        .and_then(|path| crate::pipeline::claude::normalize_cli_root(&path.to_string_lossy()))
        .unwrap_or_default();
    let scoped_source_read_root = scoped_source
        .read_root
        .as_deref()
        .and_then(|path| crate::pipeline::claude::normalize_cli_root(&path.to_string_lossy()));
    // Every workflow builds an orientation map before its steps run.
    let orientation_label = if crate::auto_review::uses_auto_review_contract(&config) {
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
    let orientation = orient::build_orientation_map(
        app,
        &extraction,
        survey_template.as_deref(),
        config.orientation_schema.as_ref(),
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
    if let Some(w) = run_writer.as_mut() {
        if let Err(e) = w.add_text(
            "context/orientation.json",
            "Orientation map",
            "context",
            &orientation_json,
        ) {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!("WARNING: {e}")
                }),
            );
        }
    }
    let orient_bytes = orientation_json.len();
    app.emit_event(
        "pipeline:log",
        serde_json::json!({
            "line": format!("Orientation map written to temp file ({orient_bytes} bytes)")
        }),
    )
    .ok();
    let _orient_tmp = orient_file; // hold tempfile alive through step execution

    // Auto Review stores only its stable five-step skeleton. Once the
    // validated router has selected allowlisted IDs, assemble this run's
    // small concrete workflow before any step scheduling or budget checks.
    let mut execution_config = crate::auto_review::materialize_config(&config, &orientation)?;
    pipeline_config::apply_agent_defaults(&mut execution_config, &settings);
    crate::pipeline_config::validate_runtime_config(&execution_config)?;
    crate::safety::validate_run_budget(&execution_config, &settings)?;

    // Orientation supplies semantic labels and page references that are useful
    // additions to the deterministic extraction. Persist the structured JSON
    // and streaming JSONL views. The enriched Markdown projection remains a
    // model-facing transient; `context/document.md` stays the exact extraction.
    let mut bundle_json = None;
    let mut bundle_markdown = None;
    if let Some(bundle) = document_bundle.as_mut() {
        bundle.enrich_from_orientation(&orientation);
        if let Err(error) = bundle.validate() {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!("WARNING: document bundle validation failed: {error}")
                }),
            );
        } else {
            let markdown = bundle.to_markdown_with_text(&extraction.text);
            bundle_markdown = Some(markdown);
            match bundle
                .to_json_pretty()
                .and_then(|json| bundle.to_jsonl().map(|jsonl| (json, jsonl)))
            {
                Ok((json, jsonl)) => {
                    if let Some(w) = run_writer.as_mut() {
                        for result in [
                            w.add_text(
                                "context/document_bundle.json",
                                "Document bundle",
                                "document",
                                &json,
                            ),
                            w.add_text(
                                "context/blocks.jsonl",
                                "Document blocks",
                                "document",
                                &jsonl,
                            ),
                        ] {
                            if let Err(error) = result {
                                let _ = app.emit_event(
                                    "pipeline:log",
                                    serde_json::json!({
                                        "line": format!("WARNING: {error}")
                                    }),
                                );
                            }
                        }
                    }
                    bundle_json = Some(json);
                }
                Err(error) => {
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!(
                                "WARNING: structured document context was omitted: {error}"
                            )
                        }),
                    );
                }
            }
        }
    }

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

    // Extra named inputs (1.2.4): extract each declared slot's file through the
    // same cascade, write the text to a temp file, and expose its path to
    // prompts as {input:key}. Temp files are held alive until the run finishes.
    let mut resolved_inputs: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut persisted_inputs: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut extra_tmps: Vec<tempfile::NamedTempFile> = Vec::new();
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
        if let Some(writer) = run_writer.as_mut() {
            let _ = writer.record_extra_input_source(&slot.key, path);
        }
        let ex = match slot.mode.as_str() {
            "folder" => extract::ingest_folder_async(path).await?,
            _ => extract::extract(app, path, &config.extraction).await?,
        };
        if let Some(w) = run_writer.as_mut() {
            let rel_path = extra_input_artifact_path(input_index, &slot.key);
            let label = if slot.label.is_empty() {
                &slot.key
            } else {
                &slot.label
            };
            if w.add_text(&rel_path, label, "context", &ex.text).is_ok()
                && w.record_extra_input(&slot.key, &rel_path).is_ok()
            {
                persisted_inputs.insert(slot.key.clone(), rel_path);
            }
        }
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
    if is_cancelled() {
        return Err("Pipeline cancelled".into());
    }

    let result = executor::execute_steps(
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
        &extra_sources,
        &std::collections::HashMap::new(), // no preloaded steps for a fresh run
        artifact_write_dir.as_deref(),
        &settings,
    )
    .await?;
    drop(extra_tmps); // keep temp files alive until steps have run

    // Steps are done — close the write window before rendering/reconciling.

    if is_cancelled() {
        return Err("Pipeline cancelled".into());
    }

    let report = PipelineReport {
        orientation,
        step_outputs: result.outputs,
        failed_steps: result.failed_steps,
        referee_reports: vec![],
        editor: None,
        report_date: chrono::Local::now().date_naive(),
        paper_hash: extraction.paper_hash.clone(),
    };

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

    Ok(complete_run(
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
            profile_name,
            variables,
            extra_inputs: persisted_inputs,
            extra_input_sources: extra_sources,
            parent_run_id: None,
        },
    ))
}
