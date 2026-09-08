//! Sequential dispatch with selected prior outputs.
use super::artifact_context::{normalized_path, ResolvedArtifactContext};
use super::findings::preserved_finding_ids;
use super::outputs::{effective_output_schema, step_output};
use super::paths::step_write_dir;
use super::prompts::{
    append_evidence_retrieval_guidance, append_shared_context_note, expand_template,
    output_format_block, specialist_context_outputs, substitute_run_context, tools_with_write,
};
use super::step_call::{execute_step_call, StepCallRequest};
use crate::models::StepOutput;
use crate::output::new_report_nonce;
use crate::pipeline::claude::{cli_parent_dir, normalize_cli_root};
use crate::pipeline_config::StepConfig;
use std::sync::Arc;

/// Run a single sequential step.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_sequential_step(
    app: &crate::emit::EventBus,
    step: &StepConfig,
    artifacts: &ResolvedArtifactContext,
    survey_hint: &str,
    variables: &std::collections::HashMap<String, String>,
    write_dir: Option<&str>,
    settings: &crate::settings::Settings,
    shared_context: Option<Arc<crate::pipeline::context_cache::PreparedContext>>,
    specialist_steps: &std::collections::HashSet<String>,
) -> Result<StepOutput, String> {
    let _ = app.emit_event(
        "pipeline:pass",
        serde_json::json!({
            "name": step.id,
            "status": "running"
        }),
    );

    let output_schema = effective_output_schema(step)?;

    // The lineage baseline reads the untransformed upstream artifact; the
    // prompt context below may render specialist JSON into readable reports.
    let preserved_finding_ids =
        preserved_finding_ids(&step.id, output_schema.as_ref(), &artifacts.prior_outputs)?;

    let prior_outputs = specialist_context_outputs(&artifacts.prior_outputs, specialist_steps);
    let base_prompt = expand_template(
        &step.prompt,
        &artifacts.orientation_path,
        survey_hint,
        &prior_outputs,
        &artifacts.paper_text_path,
        &artifacts.document_bundle_path,
        &artifacts.source_path,
    )?;

    let base_prompt = substitute_run_context(&base_prompt, variables, &artifacts.extra_inputs)?;
    let task_write_dir = step_write_dir(write_dir, &step.id)?;
    let tools = tools_with_write(
        &step.tools,
        !artifacts.read_dirs.is_empty(),
        artifacts.has_visuals,
        task_write_dir.as_deref(),
    );
    let report_rel = "report.md";
    let report_nonce = new_report_nonce()?;
    let mut prompt = base_prompt;
    if !artifacts.document_bundle_path.is_empty() {
        let artifact_root = artifacts
            .artifact_root
            .clone()
            .unwrap_or_else(|| "(visual assets not selected)".to_string());
        crate::safety::push_str_limited(
            &mut prompt,
            &format!(
                "\n\nDOCUMENT ACCESS:\nA compact DocumentBundle index is at: {}\n\
                 Its asset rel_path values are relative to: {artifact_root}\n\
                 Use the readable document for prose. Consult the index selectively to locate equations, tables, figures, page renders, and provenance; do not read it wholesale. \
                 Inspect images with ReadDocumentAsset on direct APIs or the provider's native Read tool on CLI transports.",
                artifacts.document_bundle_path.replace('\\', "/")
            ),
            crate::safety::MAX_EXPANDED_PROMPT_BYTES,
            "Sequential prompt",
        )?;
    }
    crate::safety::push_str_limited(
        &mut prompt,
        "\n\n",
        crate::safety::MAX_EXPANDED_PROMPT_BYTES,
        "Sequential prompt",
    )?;
    crate::safety::push_str_limited(
        &mut prompt,
        &format!("\n\n{}", artifacts.manifest),
        crate::safety::MAX_EXPANDED_PROMPT_BYTES,
        "Sequential artifact context",
    )?;
    if shared_context.is_some() {
        prompt = append_shared_context_note(
            prompt,
            artifacts.includes_primary_text,
            artifacts.includes_survey,
        )?;
    }
    prompt = append_evidence_retrieval_guidance(prompt, &tools)?;
    crate::safety::push_str_limited(
        &mut prompt,
        &output_format_block(
            task_write_dir.as_deref(),
            &report_nonce,
            output_schema.as_ref(),
        ),
        crate::safety::MAX_EXPANDED_PROMPT_BYTES,
        "Sequential prompt",
    )?;

    let agent = step.agents.first().map(|s| s.as_str());
    if step.agents.len() > 1 {
        // Multi-agent execution (and its merge) exists only for Parallel
        // steps; a Sequential step runs exactly one call. Say so instead of
        // silently ignoring the extra agents.
        let _ = app.emit_event(
            "pipeline:log",
            serde_json::json!({ "line": format!(
                "WARNING: sequential step '{}' lists {} agents; only the first ('{}') runs. Use a Parallel step with a merge to combine multiple agents.",
                step.id,
                step.agents.len(),
                agent.unwrap_or_default()
            ) }),
        );
    }
    let log_label = format!("Step: {}", step.label);

    // Use the paper's parent directory as CWD for steps with Read access
    let source = std::path::Path::new(&artifacts.source_path);
    let source_dir = if artifacts.source_path.is_empty() {
        Some(normalized_path(artifacts._view.path()))
    } else if source.is_dir() {
        normalize_cli_root(&artifacts.source_path)
    } else {
        cli_parent_dir(&artifacts.source_path)
    };
    let provider = agent
        .map(|a| a.to_string())
        .unwrap_or_else(|| settings.preferred_provider.clone());
    let model_selection = step.model_selection_for(settings, &provider);
    let resolution = crate::commands::await_or_cancel(
        crate::model_catalog::resolve(&provider, settings, model_selection.as_ref()),
        Some(&step.id),
    )
    .await??;
    let effort = step.effort_for(settings, &provider);
    let model_policy = resolution.selection.label();
    let call = execute_step_call(StepCallRequest {
        app,
        pass_key: &step.id,
        log_label: &log_label,
        prompt: &prompt,
        system_prompt: (!step.system_prompt.is_empty()).then_some(step.system_prompt.as_str()),
        tools: &tools,
        agent,
        cwd: source_dir.as_deref(),
        read_dirs: &artifacts.read_dirs,
        run_artifact_dir: write_dir,
        write_dir: task_write_dir.as_deref(),
        report_rel,
        report_nonce: &report_nonce,
        output_schema: output_schema.as_ref(),
        preserved_finding_ids,
        command_model: resolution.command_model.as_deref(),
        display_model: &resolution.resolved_model,
        model_policy: &model_policy,
        effort: &effort,
        settings,
        shared_context,
    })
    .await?;

    Ok(step_output(
        &step.id,
        &step.label,
        "sequential",
        &provider,
        agent.unwrap_or(""),
        call,
        &resolution,
        &effort,
        step.output_schema.is_some(),
    ))
}
