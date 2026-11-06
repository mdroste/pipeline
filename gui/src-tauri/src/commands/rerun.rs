use super::*;

mod agents;
mod document;
pub(super) mod files;
mod inputs;
mod reuse;
mod workflow;

pub(super) use agents::save_agent_overrides;
use files::read_parent_report;
pub(super) use files::{load_run_report, load_run_report_in, read_run_file, read_run_file_in};
use reuse::orientation_contract_error;
#[cfg(test)]
pub(super) use reuse::{
    collect_preloaded_outputs, failed_base_ids, incompatible_reuse_ids, resume_seed_ids,
    select_reusable_outputs,
};

// --- Resume / partial re-run (1.3.2) ---

/// Re-run a past run, reusing its captured document and orientation and (for
/// partial modes) its successful step outputs, so only the necessary steps
/// re-execute. `rerun/workflow.rs` decides which workflow runs: normally the
/// run's own, so `variables` carries only re-entered secrets, which a run
/// never retains. Modes:
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
    variables: Option<std::collections::HashMap<String, String>>,
) -> Result<serde_json::Value, String> {
    // Recovery is best effort: a failure here concerns other runs too, and
    // this run's own files decide below whether it can continue.
    let _ = crate::runs::recover_resumable_runs();
    let guard = acquire_run_slot()?;
    let run = crate::run_scope::ActiveScope::begin(super::run_queue::next_run_key());
    let bus = crate::emit::from_app(app);
    PipelineTask::spawn_future(run.scope().clone(), async move {
        let _guard = guard;
        let parent = crate::runs::load_manifest(&run_id)?;
        let plan = RerunPlan::prepare(&parent, only_failed)?;
        rerun_run_inner(
            &bus,
            &run_id,
            from_step,
            only_failed,
            variables.unwrap_or_default(),
            plan,
        )
        .await
    })
    .join()
    .await
}

/// Execution choices captured before a re-run enters the queue. Device keys
/// are loaded only by the worker, separate from the approved settings.
pub(super) struct RerunPlan {
    workflow: workflow::RerunWorkflow,
    settings: crate::settings::Settings,
    agent_overrides: Option<RunModelOverrides>,
}

impl RerunPlan {
    pub(super) fn prepare(
        parent: &crate::runs::RunManifest,
        only_failed: bool,
    ) -> Result<Self, String> {
        let mut settings = crate::settings::load_persisted_required().map_err(|e| {
            format!("Cannot re-run because settings could not be loaded safely: {e}")
        })?;
        let mut workflow = workflow::load_rerun_workflow(
            &parent.run_id,
            parent,
            &settings.active_profile,
            only_failed,
        )?;
        let agent_overrides = agents::saved_agent_overrides(&parent.run_id)?;
        if let Some(overrides) = agent_overrides.as_ref() {
            agents::reapply(overrides, &mut settings, &mut workflow.document.config)?;
        }
        settings.anthropic_api_key.clear();
        settings.openai_api_key.clear();
        settings.google_api_key.clear();
        settings.local_api_key.clear();
        Ok(Self {
            settings,
            workflow,
            agent_overrides,
        })
    }

    pub(super) fn fingerprint(&self) -> Result<String, String> {
        super::run_context::stable_content_fingerprint(&(
            &self.workflow.document.fingerprint,
            &self.workflow.source,
            &self.workflow.notice,
            &self.agent_overrides,
            super::run_context::settings_for_snapshot_fingerprint(&self.settings),
        ))
        .map_err(|e| e.to_string())
    }

    pub(super) fn verify(&self, expected: Option<&str>) -> Result<(), String> {
        if let Some(expected) = expected {
            if self.fingerprint()? != expected {
                return Err("The review design or settings changed. Open the re-run again to confirm its current configuration.".into());
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn fixture() -> Self {
        Self {
            settings: Default::default(),
            agent_overrides: None,
            workflow: workflow::RerunWorkflow {
                document: crate::pipeline_config::workflow_template().unwrap(),
                profile_id: "a".into(),
                source: "profile:a".into(),
                notice: None,
            },
        }
    }
}

/// A secret a re-run must ask for again before it can start.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RerunSecret {
    pub key: String,
    pub label: String,
}

/// What the researcher must see or supply before a re-run can start.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RerunRequirements {
    pub fingerprint: String,
    pub secrets: Vec<RerunSecret>,
    pub notice: Option<String>,
    /// The one-report agents this re-run uses again, when the run had any.
    pub agent_notice: Option<String>,
}

/// Check a re-run before it is queued: this refuses a run whose workflow can
/// no longer be established, and lists what to ask the researcher.
#[tauri::command]
pub async fn rerun_requirements(
    run_id: String,
    only_failed: bool,
) -> Result<RerunRequirements, String> {
    let parent = crate::runs::load_manifest(&run_id)?;
    let plan = RerunPlan::prepare(&parent, only_failed)?;
    let fingerprint = plan.fingerprint()?;
    let RerunPlan {
        workflow,
        agent_overrides,
        ..
    } = plan;
    let secrets = secrets_to_reenter(&workflow.document.config, &parent.variables)
        .into_iter()
        .map(|spec| RerunSecret {
            key: spec.key.clone(),
            label: if spec.label.trim().is_empty() {
                spec.key.clone()
            } else {
                spec.label.clone()
            },
        })
        .collect();
    Ok(RerunRequirements {
        fingerprint,
        secrets,
        notice: {
            let mut notices = workflow.notice.into_iter().collect::<Vec<_>>();
            if super::run_contract::ExecutionContract::load(&crate::runs::runs_dir()?.join(&run_id))
                .is_none()
            {
                notices.push("This run has no usable execution record. Pipeline will reuse the captured document and recompute orientation and the workflow steps.".into());
            }
            (!notices.is_empty()).then(|| notices.join(" "))
        },
        agent_notice: agent_overrides.as_ref().map(agents::notice),
    })
}

pub(super) async fn rerun_run_inner(
    app: &crate::emit::EventBus,
    parent_run_id: &str,
    from_step: Option<String>,
    only_failed: bool,
    secret_variables: std::collections::HashMap<String, String>,
    plan: RerunPlan,
) -> Result<serde_json::Value, String> {
    let _run_state = begin_run_state();
    // Held for the whole re-run: another review's retention must not move the
    // parent to Trash while its files are being read.
    let _parent_hold = crate::runs::hold_from_retention(parent_run_id);
    let start = std::time::Instant::now();
    let RerunPlan {
        mut settings,
        workflow,
        agent_overrides,
    } = plan;
    let device = crate::settings::load_persisted_required()
        .map_err(|e| format!("Cannot read Review credentials: {e}"))?;
    if !device.local_api_key.is_empty()
        && settings.local_base_url.trim_end_matches('/')
            != device.local_base_url.trim_end_matches('/')
    {
        return Err("The local model endpoint changed after this Review was queued. Prepare the re-run again.".into());
    }
    settings.anthropic_api_key = device.anthropic_api_key;
    settings.openai_api_key = device.openai_api_key;
    settings.google_api_key = device.google_api_key;
    settings.local_api_key = device.local_api_key;
    // Store-wide recovery waits until no review is running, so a parent that
    // stopped since then may not have its report yet.
    let parent_recovery = crate::runs::recover_stopped_run(parent_run_id);
    let parent = crate::runs::load_manifest(parent_run_id)?;
    let author_note = crate::pipeline::author_note::normalize(parent.author_note.as_deref())?;
    // As for a one-off CLI run, the run records the workflow it executes
    // without changing the persisted active profile.
    settings.active_profile = workflow.profile_id.clone();
    let _settings_snapshot = crate::settings::freeze_for_run(settings.clone());

    preflight_run_storage(std::path::Path::new(&parent.input_path))?;
    let parent_dir = crate::runs::runs_dir()?.join(parent_run_id);
    let report_json = read_parent_report(&parent_dir, parent_recovery.err())?;
    let parent_report: PipelineReport = serde_json::from_str(&report_json)
        .map_err(|e| format!("Invalid parent report.json: {e}"))?;
    let input_identity = crate::runs::input_identity(
        &parent.input_path,
        &parent.input_interpretation,
        &parent_report.paper_hash,
        Some(&parent.input_identity),
    );
    let document_rel = crate::runs::captured_document_rel_path(&parent_dir, &parent)?;
    let document_text = files::read_captured_text(&parent, document_rel)?;
    let mut orientation_value: serde_json::Value = parent_report.orientation.clone();

    let workflow_notice = workflow.notice;
    if let Some(notice) = &workflow_notice {
        app.emit_event(
            "pipeline:log",
            serde_json::json!({ "line": format!("WARNING: {notice}") }),
        )
        .ok();
    }
    if let Some(overrides) = agent_overrides.as_ref() {
        app.emit_event(
            "pipeline:log",
            serde_json::json!({ "line": agents::notice(overrides) }),
        )
        .ok();
    }
    let workflow_source = workflow.source;
    let workflow = workflow.document;
    let mut config = workflow.config.clone();
    let profile_name = workflow.name.clone();
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
    let variables = rerun_variables(&config, &parent.variables, secret_variables)?;
    crate::safety::validate_runtime_context(&variables, "Re-run variables")?;
    crate::safety::validate_runtime_context(
        &parent.extra_input_sources,
        "Re-run named input paths",
    )?;
    crate::safety::validate_runtime_context(&parent.extra_inputs, "Re-run captured input paths")?;
    validate_runtime_bindings(&config, &variables, &parent.extra_input_sources, true)?;
    let durable_variables = durable_variables(&config, &variables);
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

    // Regenerate may run an edited workflow, so a cached orientation is
    // reusable only when it still satisfies that workflow's schema and semantic
    // contract. Rebuilding it invalidates every downstream step.
    let parent_contract = super::run_contract::ExecutionContract::load(&parent_dir);
    let orientation_notes = parent_contract
        .as_ref()
        .map(|c| c.orientation_notes.clone())
        .unwrap_or_else(|| parent_report.quality.extraction_notes.clone());
    let source_record =
        super::rerun::files::read_run_file_in(&parent_dir, "context/source-snapshot.json").ok();
    let recipe = super::run_contract::OrientationRecipe::new(
        &config,
        &settings,
        &parent.input_mode,
        &document_text,
        source_record.as_deref(),
        author_note.as_deref(),
        &orientation_notes,
    )?;
    let orientation_error =
        orientation_contract_error(&config, &orientation_value, &parent.input_mode).err();
    let same_recipe = parent_contract
        .as_ref()
        .map(|c| c.matches_orientation(&recipe.key, &orientation_value))
        .transpose()?
        .unwrap_or(false);
    let orientation_error = orientation_error.or_else(|| (!same_recipe).then(|| "the orientation inputs or instructions changed, or the parent predates execution contracts".to_string()));
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
                "Re-run: rebuilding the parent orientation because it does not satisfy this workflow ({contract_error})"
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
            quality_notes: orientation_notes.clone(),
        };
        orientation_value = orient::build_orientation_map(
            app,
            &extraction,
            Some(&recipe.template),
            recipe.schema.as_ref(),
            scoped_source_read_root.as_deref(),
            author_note.as_deref(),
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

    let extra_input_sources = parent.extra_input_sources.clone();
    let (mut run_writer, artifact_write_dir) = create_run_workspace(
        app,
        &parent_report.paper_hash,
        crate::runs::RunFinishMeta {
            author_note: author_note.clone(),
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
    if let Some(writer) = run_writer.as_mut() {
        crate::pipeline::author_note::save(writer, author_note.as_deref())?;
        super::reuse_artifacts::retain_source(&scoped_source, writer)?;
        writer
            .add_text(
                workflow::WORKFLOW_SNAPSHOT_PATH,
                "Workflow snapshot",
                "context",
                &workflow_json,
            )
            .map_err(|error| format!("Could not save the workflow snapshot: {error}"))?;
        save_agent_overrides(writer, agent_overrides.as_ref())?;
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
        // The report keeps what the researcher was told before continuing.
        quality_notes: workflow_notice.into_iter().collect(),
    };
    let orientation_sampling = orient::orientation_sample(&document_text);
    let document::RestoredDocument {
        rerun_bundle_json,
        report_bundle,
        rerun_document_text,
    } = document::restore_document_context(
        app,
        &parent,
        &document_text,
        &orientation_value,
        &orientation_sampling,
        &resumed_extraction,
        &scoped_source,
        &mut run_writer,
    )?;

    document::require_visual_input(&execution_config, report_bundle.as_ref())?;

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

    let writer = run_writer
        .as_mut()
        .ok_or("Missing durable re-run workspace")?;
    let mut contract = super::run_contract::ExecutionContract::build(
        &execution_config,
        &settings,
        &recipe,
        &orientation_value,
        orientation_notes,
        author_note.as_deref(),
        &variables,
        &persisted_inputs,
        writer,
    )?;
    let mut changed = contract.changed_steps(parent_contract.as_ref());
    let preloaded = if !orientation_rebuilt && (only_failed || from_step.is_some()) {
        let config = execution_config.clone();
        let id = parent_run_id.to_owned();
        let bus = app.clone();
        let child_root = writer.dir().join("artifacts");
        crate::run_scope::spawn_blocking(move || {
            let parent_root = parent_dir.join("artifacts");
            for step in config.steps.iter().filter(|step| step.enabled) {
                if !changed.contains(&step.id) && !crate::runs::receipts::matches(&parent_root, &parent_root, &step.id, &parent_report.step_outputs) {
                    changed.insert(step.id.clone());
                }
            }
            let cached = reuse::select_reusable_outputs(&bus, &id, &parent_dir, &config, &parent_report, false, &changed, from_step, only_failed);
            super::reuse_artifacts::restore_producers(&parent_dir, &child_root, cached.keys().cloned())?;
            // Verify the copy as well as the parent. A replaced file between
            // verification and copying cannot feed a consumer of cached work.
            for (step, outputs) in &cached {
                if !crate::runs::receipts::matches(&parent_root, &child_root, step, outputs) {
                    return Err(format!("The retained files for step '{step}' changed during restoration. Regenerate the review."));
                }
            }
            Ok::<_, String>(cached)
        }).await.map_err(|e|e.to_string())??
    } else {
        Default::default()
    };
    contract.save(
        writer,
        preloaded
            .keys()
            .map(|id| (id.clone(), parent_run_id.to_owned())),
    )?;

    let paper_type = crate::models::paper_view(&orientation_value)
        .map(|v| v.metadata.paper_type.to_string())
        .unwrap_or_default();
    let survey_hint = crate::models::survey_hint(&orientation_value);

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
            author_note.as_deref(),
        ),
    )
    .await?;
    drop(extra_tmps);
    if is_cancelled() {
        return Err("Pipeline cancelled".into());
    }

    let report = super::run::report::assemble_report(
        app,
        &mut run_writer,
        &execution_config,
        result,
        &resumed_extraction,
        &report_bundle,
        &orientation_sampling,
        orientation_value,
        &scoped_source_path,
    );
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
            author_note,
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

#[cfg(test)]
mod plan_tests {
    use super::*;
    #[test]
    fn approved_plan_rejects_design_edits_and_active_profile_switches() {
        let mut plan = RerunPlan::fixture();
        let approved = plan.fingerprint().unwrap();
        plan.verify(Some(&approved)).unwrap();
        plan.settings.active_profile = "b".into();
        assert!(plan.verify(Some(&approved)).is_err());
        let mut edited = RerunPlan::fixture();
        edited.workflow.document.fingerprint = "edited".into();
        assert!(edited.verify(Some(&approved)).is_err());
    }
}
