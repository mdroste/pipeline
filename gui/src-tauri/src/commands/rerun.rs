use super::*;

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
    let guard = acquire_pipeline_guard()?;
    let bus = crate::emit::from_app(app);
    PipelineTask::spawn_future(async move {
        let _guard = guard;
        rerun_run_inner(&bus, &run_id, from_step, only_failed).await
    })
    .join()
    .await
}

pub(super) fn read_run_file(run_id: &str, rel: &str) -> Result<String, String> {
    crate::runs::validate_run_id(run_id)?;
    if rel.is_empty()
        || std::path::Path::new(rel).is_absolute()
        || rel.split(['/', '\\']).any(|part| part == "..")
    {
        return Err("Invalid run-relative path".to_string());
    }
    let run_dir = crate::runs::runs_dir()?
        .join(run_id)
        .canonicalize()
        .map_err(|_| "Run not found".to_string())?;
    let path = run_dir
        .join(rel)
        .canonicalize()
        .map_err(|e| format!("Cannot resolve {rel} from run: {e}"))?;
    if !path.starts_with(&run_dir) {
        return Err("Run-relative path resolves outside the run directory".to_string());
    }
    let file = crate::safety::open_regular_file(&path)
        .map_err(|e| format!("Cannot read {rel} from run: {e}"))?;
    let mut bytes = Vec::with_capacity(64 * 1024);
    file.take(MAX_RUN_CONTEXT_SIZE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read {rel} from run: {e}"))?;
    if bytes.len() as u64 > MAX_RUN_CONTEXT_SIZE {
        return Err(format!(
            "{rel} exceeds the {} MB run-context safety limit",
            MAX_RUN_CONTEXT_SIZE / 1024 / 1024
        ));
    }
    String::from_utf8(bytes).map_err(|e| format!("{rel} is not valid UTF-8: {e}"))
}

pub(super) fn load_run_report(run_id: &str) -> Result<PipelineReport, String> {
    let json = read_run_file(run_id, "report.json")
        .map_err(|_| "This run predates comparison support (no report.json).".to_string())?;
    let mut report: PipelineReport =
        serde_json::from_str(&json).map_err(|e| format!("Invalid report.json: {e}"))?;
    crate::findings::ensure_legacy_products(&mut report);
    Ok(report)
}

pub(super) fn base_step_id(step_id: &str) -> String {
    step_id.split('/').next().unwrap_or(step_id).to_string()
}

pub(super) fn failed_base_ids(
    failures: &[crate::models::StepFailure],
) -> std::collections::HashSet<String> {
    failures
        .iter()
        .map(|failure| base_step_id(&failure.step_id))
        .collect()
}

pub(super) fn resume_seed_ids(
    enabled_ids: &[String],
    report: &PipelineReport,
) -> std::collections::HashSet<String> {
    let mut seeds = failed_base_ids(&report.failed_steps);
    let completed: std::collections::HashSet<String> = report
        .step_outputs
        .iter()
        .map(|output| base_step_id(&output.step_id))
        .collect();
    // Recovery reports use a synthetic run-level failure when the process
    // stopped without returning an ExecutionResult. Missing enabled steps are
    // therefore the authoritative restart point.
    seeds.extend(
        enabled_ids
            .iter()
            .filter(|step_id| !completed.contains(*step_id))
            .cloned(),
    );
    seeds
}

pub(super) fn collect_preloaded_outputs(
    outputs: &[crate::models::StepOutput],
    rerun: &std::collections::HashSet<String>,
) -> std::collections::HashMap<String, Vec<crate::models::StepOutput>> {
    let mut preloaded = std::collections::HashMap::<String, Vec<crate::models::StepOutput>>::new();
    for output in outputs.iter().filter(|output| !output.skipped) {
        let base = base_step_id(&output.step_id);
        if !rerun.contains(&base) {
            preloaded.entry(base).or_default().push(output.clone());
        }
    }
    preloaded
}

fn orientation_contract_error(
    config: &crate::pipeline_config::PipelineConfig,
    orientation: &serde_json::Value,
    input_mode: &str,
) -> Result<(), String> {
    let contract = crate::pipeline::orient::resolve_survey_schema(
        &config.orientation_prompt,
        config.orientation_schema.as_ref(),
        input_mode,
    )
    .unwrap_or_else(|| serde_json::json!({"type": "object"}));
    let resolved = crate::auto_review::resolve_schema_catalogs(&contract)?;
    crate::pipeline::structured::validate(&resolved, orientation)?;
    crate::auto_review::validate_contract_for_schema(&contract, orientation)?;
    Ok(())
}

pub(super) fn incompatible_reuse_ids(
    config: &crate::pipeline_config::PipelineConfig,
    outputs: &[crate::models::StepOutput],
) -> std::collections::HashSet<String> {
    let mut by_base = std::collections::HashMap::<&str, Vec<&crate::models::StepOutput>>::new();
    for output in outputs.iter().filter(|output| !output.skipped) {
        by_base
            .entry(output.step_id.split('/').next().unwrap_or(&output.step_id))
            .or_default()
            .push(output);
    }
    config
        .steps
        .iter()
        .filter(|step| step.enabled)
        .filter_map(|step| {
            let compatible = by_base.get(step.id.as_str()).is_some_and(|prior| {
                // A live schema reference must resolve before a cached
                // artifact can be judged against it; an unresolvable
                // reference simply reruns the step.
                let resolved_schema = step
                    .output_schema
                    .as_ref()
                    .map(crate::pipeline::structured::resolve_schema_reference);
                !prior.is_empty()
                    && prior.iter().all(|output| match resolved_schema.as_ref() {
                        Some(Ok(schema)) => {
                            output.structured_json
                                && crate::pipeline::structured::canonicalize(
                                    schema,
                                    &output.raw_text,
                                )
                                .is_ok()
                        }
                        Some(Err(_)) => false,
                        None => !output.structured_json && !output.raw_text.trim().is_empty(),
                    })
            });
            (!compatible).then(|| step.id.clone())
        })
        .collect()
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
            "Automatic Paper Review reviews a document, not a browsable source tree. Re-run this \
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

    // Determine which steps to re-run vs. reuse.
    let enabled_ids: Vec<String> = execution_config
        .steps
        .iter()
        .filter(|s| s.enabled)
        .map(|s| s.id.clone())
        .collect();
    let mut rerun: std::collections::HashSet<String> = if orientation_rebuilt {
        enabled_ids.iter().cloned().collect()
    } else if let Some(fs) = &from_step {
        match enabled_ids.iter().position(|id| id == fs) {
            Some(k) => {
                // Steps may be listed in any order while the executor schedules
                // by dependency graph, so a dependent of a re-run step can sit
                // earlier in the list. Extend the positional set with graph
                // dependents so no step is preloaded with stale upstream output.
                let seeds: std::collections::HashSet<String> =
                    enabled_ids[k..].iter().cloned().collect();
                let mut set = seeds.clone();
                set.extend(crate::pipeline::executor::dependents_of(
                    &execution_config,
                    &seeds,
                ));
                set
            }
            None => enabled_ids.iter().cloned().collect(), // unknown step → full re-run
        }
    } else if only_failed {
        let seeds = resume_seed_ids(&enabled_ids, &parent_report);
        let mut set = seeds.clone();
        set.extend(crate::pipeline::executor::dependents_of(
            &execution_config,
            &seeds,
        ));
        set
    } else {
        enabled_ids.iter().cloned().collect()
    };

    // Even when the user asks for a narrow partial re-run, never preload an
    // artifact produced under a different output mode or one that fails the
    // step's current schema. Its graph dependents are stale as well.
    let mut incompatible = incompatible_reuse_ids(&execution_config, &parent_report.step_outputs);
    // Missing retained producer trees cannot satisfy a downstream file selector.
    for step in execution_config.steps.iter().filter(|s| s.enabled) {
        for selector in &step.context.include {
            if let crate::pipeline_config::ArtifactSelector::Step {
                step: producer,
                parts,
                ..
            } = selector
            {
                if parts.contains(&crate::pipeline_config::StepArtifactPart::Files)
                    && !parent_dir
                        .join("artifacts/by-step")
                        .join(executor::step_slug(producer))
                        .is_dir()
                {
                    incompatible.insert(producer.clone());
                }
            }
        }
    }
    if !incompatible.is_empty() {
        rerun.extend(incompatible.iter().cloned());
        rerun.extend(crate::pipeline::executor::dependents_of(
            &execution_config,
            &incompatible,
        ));
        app.emit_event("pipeline:log", serde_json::json!({
            "line": format!(
                "Re-run: invalidating {} cached step artifact(s) that do not satisfy the active output contracts",
                incompatible.len()
            )
        })).ok();
    }

    // Preload the reused steps' outputs (keyed by base id).
    let mut preloaded = collect_preloaded_outputs(&parent_report.step_outputs, &rerun);
    let enabled: std::collections::HashSet<&str> = enabled_ids.iter().map(String::as_str).collect();
    preloaded.retain(|id, _| enabled.contains(id.as_str()));
    app.emit_event("pipeline:log", serde_json::json!({
        "line": format!("Re-run of {parent_run_id}: reusing {} step(s), re-running the rest", preloaded.len())
    })).ok();

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
    let mut rerun_bundle_json = None;
    let mut report_bundle = None;
    let mut rerun_document_text = document_text.clone();
    if let Some(w) = run_writer.as_mut() {
        w.add_text(
            crate::runs::DOCUMENT_TEXT_PATH,
            "Readable document",
            "document",
            &document_text,
        )
        .map_err(|error| format!("Could not save the restored document: {error}"))?;
        let orient_json = serde_json::to_string_pretty(&orientation_value)
            .map_err(|error| format!("Could not serialize the orientation map: {error}"))?;
        w.add_text(
            "context/orientation.json",
            "Orientation map",
            "context",
            &orient_json,
        )
        .map_err(|error| format!("Could not save the orientation map: {error}"))?;
        let sampling_json = serde_json::to_string_pretty(&orientation_sampling)
            .map_err(|error| format!("Could not serialize orientation coverage: {error}"))?;
        w.add_text(
            "context/orientation_sampling.json",
            "Orientation sampling coverage",
            "context",
            &sampling_json,
        )
        .map_err(|error| format!("Could not save orientation sampling coverage: {error}"))?;

        // Prefer the parent's canonical bundle so a partial re-run keeps the
        // exact document model it was based on. Copy its visual assets into
        // the new run so the bundle remains self-contained. Older runs fall
        // back to a bundle rebuilt from their captured document.
        let mut bundle = read_run_file(parent_run_id, "context/document_bundle.json")
            .ok()
            .and_then(|json| {
                serde_json::from_str::<crate::document_bundle::DocumentBundle>(&json).ok()
            })
            .filter(|bundle| bundle.validate().is_ok());
        if let Some(parent_bundle) = bundle.as_ref() {
            if let Ok(parent_root) = crate::runs::runs_dir() {
                let parent_root = parent_root.join(parent_run_id);
                for asset in &parent_bundle.assets {
                    let source = parent_root.join(&asset.rel_path);
                    let destination = w.dir().join(&asset.rel_path);
                    if !source.is_file() {
                        continue;
                    }
                    if let Some(directory) = destination.parent() {
                        let _ = std::fs::create_dir_all(directory);
                    }
                    if std::fs::copy(&source, &destination).is_ok() {
                        let group = if asset.kind == "page" {
                            "pages"
                        } else {
                            "figures"
                        };
                        let _ = w.register_existing(&asset.rel_path, &asset.label, group);
                    }
                }
            }
        } else if let Ok(build) = super::reuse_artifacts::build_bundle(
            &resumed_extraction,
            scoped_source.source_path.as_deref(),
            w.dir(),
        ) {
            for (rel_path, label, group) in &build.added_artifacts {
                let _ = w.register_existing(rel_path, label, group);
            }
            bundle = Some(build.bundle);
        }
        if let Some(bundle) = bundle.as_mut() {
            bundle.enrich_from_orientation(&orientation_value);
            if let Err(error) = bundle.validate() {
                let _ = app.emit_event(
                    "pipeline:log",
                    serde_json::json!({
                        "line": format!("WARNING: document bundle validation failed: {error}")
                    }),
                );
            } else {
                let markdown = bundle.to_markdown_with_text(&document_text);
                rerun_document_text = markdown.clone();
                match bundle
                    .to_json_pretty()
                    .and_then(|json| bundle.to_jsonl().map(|jsonl| (json, jsonl)))
                {
                    Ok((json, jsonl)) => {
                        w.add_text(
                            "context/document_bundle.json",
                            "Document bundle",
                            "document",
                            &json,
                        )
                        .map_err(|error| format!("Could not save the document bundle: {error}"))?;
                        w.add_text(
                            "context/blocks.jsonl",
                            "Document blocks",
                            "document",
                            &jsonl,
                        )
                        .map_err(|error| format!("Could not save document blocks: {error}"))?;
                        rerun_bundle_json = Some(json);
                        report_bundle = Some(bundle.clone());
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
    }

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

    // Rehydrate named inputs from the parent run's captured copies. Required
    // inputs from older runs that predate capture fail explicitly instead of
    // silently substituting an empty string into the prompt.
    let mut runtime_extra_sources = std::collections::HashMap::new();
    let mut resolved_inputs = std::collections::HashMap::new();
    let mut persisted_inputs = std::collections::HashMap::new();
    let mut extra_tmps: Vec<tempfile::NamedTempFile> = Vec::new();
    for (input_index, slot) in config.extraction.extra_inputs.iter().enumerate() {
        let Some(parent_rel) = parent.extra_inputs.get(&slot.key) else {
            if slot.required {
                let label = if slot.label.is_empty() {
                    &slot.key
                } else {
                    &slot.label
                };
                return Err(format!(
                    "Re-run requires named input '{label}', but the parent run did not capture it. Start a new run and select the input again."
                ));
            }
            continue;
        };
        let captured_source = super::reuse_artifacts::restore_named_source(
            &parent_dir,
            &slot.key,
            run_input_dir.path(),
            super::reuse_artifacts::requires_named_source(&execution_config, &slot.key),
        )?;
        if let Some(path) = captured_source.source_path.as_ref() {
            runtime_extra_sources.insert(slot.key.clone(), path.to_string_lossy().into_owned());
        }
        if let Some(writer) = run_writer.as_mut() {
            super::reuse_artifacts::retain_named_source(&slot.key, &captured_source, writer)?;
        }
        let content = read_run_file(parent_run_id, parent_rel)
            .map_err(|e| format!("Cannot restore named input '{}': {e}", slot.key))?;
        let rel_path = extra_input_artifact_path(input_index, &slot.key);
        let label = if slot.label.is_empty() {
            &slot.key
        } else {
            &slot.label
        };
        let writer = run_writer
            .as_mut()
            .ok_or_else(|| "Durable run workspace disappeared before named inputs".to_string())?;
        writer
            .add_text(&rel_path, label, "context", &content)
            .map_err(|error| format!("Could not save named input '{}': {error}", slot.key))?;
        writer
            .record_extra_input(&slot.key, &rel_path)
            .map_err(|error| format!("Could not index named input '{}': {error}", slot.key))?;
        persisted_inputs.insert(slot.key.clone(), rel_path);
        let (temp, path) = write_run_input_file(
            run_input_dir.path(),
            "pipeline_input_",
            ".txt",
            &content,
            &format!("restored input '{}'", slot.key),
        )?;
        resolved_inputs.insert(slot.key.clone(), path);
        extra_tmps.push(temp);
    }

    let paper_type = crate::models::paper_view(&orientation_value)
        .map(|v| v.metadata.paper_type.to_string())
        .unwrap_or_default();
    let survey_hint = crate::models::survey_hint(&orientation_value);
    let variables = parent.variables.clone();

    let result = executor::execute_steps(
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
