use super::artifact_context::{normalized_path, resolve_artifact_context, ArtifactRuntime};
use super::checkpoints::OutputBudget;
use super::findings::{
    check_finding_lineage, check_validation_dispositions, finding_lineage, preserved_finding_ids,
};
use super::outputs::enforce_merge_output_schemas;
use super::outputs::step_output;
use super::parallel::run_parallel_wave;
use super::paths::step_slug;
use super::prompts::{
    append_evidence_retrieval_guidance, append_shared_context_note, build_parallel_prompt,
    expand_template, output_format_block, specialist_context_outputs, step_ref_text,
    substitute_placeholders, substitute_run_context, tools_with_write,
};
use super::schedule::{
    base_id, dependency_policy_failure, dependents_of, execution_plan, input_processing_label,
    mark_steps_done, ready_indices, resolve_dependencies,
};
use super::step_call::{ingest_report_file_blocking, StepCallResult};
use super::units::build_units;
use super::*;
use crate::models::StepOutput;
use crate::pipeline_config::{
    ArtifactSelector, NamedInputArtifactPart, Phase, PrimaryArtifactPart, StepArtifactPart,
};
use crate::pipeline_config::{PipelineConfig, StepConfig};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct RecordingEvents(Mutex<Vec<(String, serde_json::Value)>>);

impl crate::emit::Events for RecordingEvents {
    fn emit_event(
        &self,
        event: &str,
        payload: serde_json::Value,
    ) -> Result<(), crate::emit::EmitError> {
        self.0.lock().unwrap().push((event.to_string(), payload));
        Ok(())
    }
}

fn make_step(id: &str, phase: Phase) -> StepConfig {
    StepConfig {
        id: id.into(),
        label: id.into(),
        phase,
        ..Default::default()
    }
}

async fn assert_resumed_runtime_emits_every_planned_scheduler_stage_id() {
    let mut first = make_step("first", Phase::Parallel);
    first.agents = vec!["claude".into(), "codex".into()];
    let second = make_step("second", Phase::Parallel);
    let mut synthesis = make_step("synthesis", Phase::Sequential);
    synthesis.after = vec!["first".into(), "second".into()];
    let config = PipelineConfig {
        steps: vec![first, second, synthesis],
        merge: Default::default(),
        outputs: Default::default(),
        context_cache: Default::default(),
        use_orientation: true,
        orientation_prompt: String::new(),
        orientation_schema: None,
        extraction: Default::default(),
        parallel_context_template: String::new(),
        variables: Vec::new(),
    };
    let preloaded = ["first", "second", "synthesis"]
        .into_iter()
        .map(|id| {
            (
                id.to_string(),
                vec![StepOutput {
                    step_id: id.to_string(),
                    step_label: id.to_string(),
                    raw_text: "cached".to_string(),
                    ..Default::default()
                }],
            )
        })
        .collect();
    let recorder = Arc::new(RecordingEvents::default());
    let bus: crate::emit::EventBus = recorder.clone();
    let empty = std::collections::HashMap::new();

    execute_steps(
        &bus,
        &config,
        "",
        &serde_json::Value::Null,
        "",
        "",
        "",
        "",
        "",
        &empty,
        &empty,
        &empty,
        &preloaded,
        None,
        &crate::settings::Settings::default(),
    )
    .await
    .unwrap();

    let events = recorder.0.lock().unwrap();
    let stages = events
        .iter()
        .filter(|(event, _)| event == "pipeline:stage")
        .map(|(_, payload)| payload)
        .collect::<Vec<_>>();
    assert_eq!(
        stages
            .iter()
            .filter_map(|payload| payload["id"].as_str())
            .collect::<Vec<_>>(),
        vec!["wave-1-parallel", "wave-1-merge", "wave-2-sequential"]
    );
    assert_eq!(stages[1]["skipped"], true);
    assert_eq!(stages[0]["mergeStepIds"], serde_json::json!(["first"]));
    assert_eq!(stages[0]["mergeStepLabels"], serde_json::json!(["first"]));
    assert!(stages.iter().all(|payload| payload["stepIds"].is_array()));
    assert!(stages
        .iter()
        .all(|payload| payload["stepLabels"].is_array()));
}

// ── resolve_dependencies (explicit order + artifact dataflow) ──

fn deps_of(steps: &[StepConfig]) -> Vec<std::collections::HashSet<String>> {
    let refs: Vec<&StepConfig> = steps.iter().collect();
    resolve_dependencies(&refs)
}

// ── structured references and lineage ──────────────────────────

fn findings_output(id: &str, text: &str) -> StepOutput {
    StepOutput {
        step_id: id.into(),
        step_label: id.into(),
        phase: "sequential".into(),
        raw_text: text.into(),
        structured_json: true,
        ..Default::default()
    }
}

// ── build_units (non-fan-out) ──────────────────────────────────

// ── merge output-schema enforcement ────────────────────────────

fn schema_unit(agent: &str, text: &str) -> StepOutput {
    StepOutput {
        step_id: format!("s/{agent}"),
        merge_group: "s".into(),
        agent: agent.into(),
        raw_text: text.into(),
        ..Default::default()
    }
}

fn merge_output(text: &str) -> StepOutput {
    StepOutput {
        step_id: "s".into(),
        step_label: "s".into(),
        agent: "claude+codex".into(),
        raw_text: text.into(),
        calls: vec![
            crate::models::StepCallRecord {
                role: "step".into(),
                ..Default::default()
            },
            crate::models::StepCallRecord {
                role: "merge".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

// ── dependents_of ──────────────────────────────────────────────

// ── substitution ───────────────────────────────────────────────

// ── build_parallel_prompt ──────────────────────────────────────

// ── write handoff helpers ──────────────────────────────────────

// ── expand_template ────────────────────────────────────────────

// ── named step references ──────────────────────────────────────

fn out(id: &str, label: &str, text: &str) -> StepOutput {
    StepOutput {
        step_id: id.into(),
        step_label: label.into(),
        phase: "parallel".into(),
        agent: String::new(),
        raw_text: text.into(),
        ..Default::default()
    }
}

mod artifacts;

mod findings;

mod outputs;

mod placeholders;

mod prompts;

mod schedule;

mod units;
