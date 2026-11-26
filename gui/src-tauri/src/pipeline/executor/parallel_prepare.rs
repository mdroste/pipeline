//! Owned wave inputs and blocking preparation, admitted by the provider semaphore.
use super::artifact_context::{
    prepare_selected_shared_context, resolve_artifact_context, ArtifactRuntime,
    ResolvedArtifactContext,
};
use super::outputs::effective_output_schema;
use super::paths::step_write_dir;
use super::prompts::{
    append_evidence_retrieval_guidance, append_shared_context_note,
    build_parallel_prompt_with_context, output_format_block, tools_with_write,
};
use super::units::Unit;
use crate::models::StepOutput;
use crate::output::new_report_nonce;
use crate::pipeline::context_cache::{PreparedContext, PreparedContextPool};
use crate::pipeline_config::StepConfig;
use std::{collections::HashMap, sync::Arc};

pub(super) struct WaveInputs {
    pub orientation_path: String,
    pub orientation_value: serde_json::Value,
    pub paper_text_path: String,
    pub document_bundle_path: String,
    pub source_path: String,
    pub paper_type: String,
    pub survey_hint: String,
    pub context_template: String,
    pub variables: HashMap<String, String>,
    pub extra_inputs: HashMap<String, String>,
    pub extra_input_sources: HashMap<String, String>,
    pub prior_outputs: Vec<StepOutput>,
    pub write_dir: Option<String>,
    pub context_cache_enabled: bool,
    pub shared_context_pool: PreparedContextPool,
}

pub(super) struct PreparedUnit {
    pub output_schema: Option<serde_json::Value>,
    pub resolved: ResolvedArtifactContext,
    pub shared_context: Option<Arc<PreparedContext>>,
    pub task_write_dir: Option<String>,
    pub tools: Vec<String>,
    pub report_nonce: String,
    pub prompt: String,
}

impl WaveInputs {
    pub fn prepare(
        &self,
        step: &StepConfig,
        unit: &Unit,
        step_key: &str,
        app: &crate::emit::EventBus,
    ) -> Result<PreparedUnit, String> {
        let Self {
            orientation_path,
            orientation_value,
            paper_text_path,
            document_bundle_path,
            source_path,
            paper_type,
            survey_hint,
            context_template,
            variables,
            extra_inputs,
            extra_input_sources,
            prior_outputs,
            write_dir,
            context_cache_enabled,
            shared_context_pool,
        } = self;
        let write_dir = write_dir.as_deref();
        let context_cache_enabled = *context_cache_enabled;
        let output_schema = effective_output_schema(step)?;
        let mut resolved = resolve_artifact_context(
            step,
            ArtifactRuntime {
                orientation_path,
                paper_text_path,
                document_bundle_path,
                source_path,
                extra_inputs,
                extra_input_sources,
                outputs: prior_outputs,
                run_artifact_dir: write_dir,
                context_pool: shared_context_pool,
            },
        )?;
        resolved.log_warnings(app);
        // File fan-out stages the matched file into the artifact view and
        // binds its path; artifact fan-out binds the element text itself.
        let item_path = match unit.item.as_deref() {
            Some(item) => resolved.stage_current_item(item)?,
            None => unit.inline_item.clone().unwrap_or_default(),
        };
        let shared_context = prepare_selected_shared_context(
            app,
            context_cache_enabled,
            &resolved,
            orientation_value,
            shared_context_pool,
        );
        let task_write_dir = step_write_dir(write_dir, step_key)?;
        let tools = tools_with_write(
            &step.tools,
            !resolved.read_dirs.is_empty(),
            resolved.has_visuals,
            task_write_dir.as_deref(),
        );
        let report_nonce = new_report_nonce()?;
        let output_format = output_format_block(
            task_write_dir.as_deref(),
            &report_nonce,
            output_schema.as_ref(),
        );
        let mut prompt = build_parallel_prompt_with_context(
            step,
            if resolved.includes_survey {
                paper_type
            } else {
                ""
            },
            &resolved.orientation_path,
            if resolved.includes_survey {
                survey_hint
            } else {
                ""
            },
            &resolved.paper_text_path,
            &resolved.document_bundle_path,
            &resolved.source_path,
            context_template,
            &output_format,
            resolved.artifact_root.as_deref(),
            variables,
            &resolved.extra_inputs,
            &item_path,
        )?;
        crate::safety::push_str_limited(
            &mut prompt,
            &format!("\n\n{}", resolved.manifest),
            crate::safety::MAX_EXPANDED_PROMPT_BYTES,
            "Parallel artifact context",
        )?;
        let prompt = if shared_context.is_some() {
            append_shared_context_note(
                prompt,
                resolved.includes_primary_text,
                resolved.includes_survey,
            )?
        } else {
            prompt
        };
        let prompt = append_evidence_retrieval_guidance(prompt, &tools)?;
        Ok(PreparedUnit {
            output_schema,
            resolved,
            shared_context,
            task_write_dir,
            tools,
            report_nonce,
            prompt,
        })
    }
}
