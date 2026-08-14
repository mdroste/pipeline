//! Unified pipeline executor.
//!
//! Schedules the explicit dependency graph. All ready Parallel steps form a
//! wave and run concurrently without access to any step output. Ready
//! Sequential steps run one at a time and see only their selected artifacts.
//! Merge auto-triggers when a multi-agent step needs its outputs combined.

use super::claude::{cli_parent_dir, normalize_cli_root};
use super::merge;
use crate::models::{StepFailure, StepOutput};
use crate::output::{
    capitalize, extract_report_envelope, new_report_nonce, normalize_math_delimiters,
    report_output_format, ReportRejectionKind, ReportValidationError,
};
use crate::pipeline_config::{
    ArtifactSelector, NamedInputArtifactPart, Phase, PipelineConfig, PrimaryArtifactPart,
    StepArtifactPart, StepConfig,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

/// Result of pipeline execution: successful outputs and any step failures.
pub struct ExecutionResult {
    pub outputs: Vec<StepOutput>,
    pub failed_steps: Vec<StepFailure>,
}

type ParallelTaskResult = Result<((usize, String), StepOutput), StepFailure>;

#[derive(Default)]
struct OutputBudget(AtomicUsize);

impl OutputBudget {
    fn reserve(&self, output: &StepOutput) -> Result<(), String> {
        let bytes = output.raw_text.len();
        let result = self
            .0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current
                    .checked_add(bytes)
                    .filter(|total| *total <= crate::safety::MAX_RUN_OUTPUT_BYTES)
            });
        result.map(|_| ()).map_err(|_| {
            format!(
                "Run outputs exceed the {} MB safety limit",
                crate::safety::MAX_RUN_OUTPUT_BYTES / 1024 / 1024
            )
        })
    }
}

/// Persist each completed output immediately. Finalization still writes the
/// human-facing numbered markdown artifacts, but these structured checkpoints
/// make an interrupted run recoverable before the final report exists.
async fn checkpoint_output(
    write_dir: Option<&str>,
    ordinal: usize,
    output: &StepOutput,
) -> Result<(), String> {
    let Some(write_dir) = write_dir else {
        return Ok(());
    };
    let directory = std::path::PathBuf::from(write_dir).join("checkpoints");
    let destination = directory.join(format!("{ordinal:04}_{}.json", step_slug(&output.step_id)));
    let output = output.clone();
    tokio::task::spawn_blocking(move || {
        use std::io::Write as _;
        std::fs::create_dir_all(&directory)
            .map_err(|error| format!("Failed to create checkpoint directory: {error}"))?;
        let json = serde_json::to_vec_pretty(&output)
            .map_err(|error| format!("Failed to serialize step checkpoint: {error}"))?;
        let mut temp = tempfile::NamedTempFile::new_in(&directory)
            .map_err(|error| format!("Failed to create checkpoint temp file: {error}"))?;
        temp.write_all(&json)
            .map_err(|error| format!("Failed to write step checkpoint: {error}"))?;
        temp.flush()
            .map_err(|error| format!("Failed to flush step checkpoint: {error}"))?;
        temp.as_file()
            .sync_all()
            .map_err(|error| format!("Failed to sync step checkpoint: {error}"))?;
        temp.persist(&destination)
            .map_err(|error| format!("Failed to publish step checkpoint: {}", error.error))?;
        Ok(())
    })
    .await
    .map_err(|error| format!("Step checkpoint task failed: {error}"))?
}

async fn checkpoint_outputs(
    app: &crate::emit::EventBus,
    write_dir: Option<&str>,
    start: usize,
    outputs: &[StepOutput],
) {
    for (offset, output) in outputs.iter().enumerate() {
        if let Err(error) = checkpoint_output(write_dir, start + offset, output).await {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!(
                        "WARNING: could not checkpoint completed step '{}': {error}",
                        output.step_label
                    )
                }),
            );
        }
    }
}

/// Remove checkpoint files at or above the given ordinal. Runs before a
/// wave's final rewrite so its provisional completion-order checkpoints
/// cannot linger next to the final ordering and double-load in recovery.
/// Failure checkpoints (`failure_*.json`) have no numeric prefix and are
/// never touched.
async fn remove_checkpoints_from(write_dir: Option<&str>, start: usize) {
    let Some(write_dir) = write_dir else {
        return;
    };
    let directory = std::path::PathBuf::from(write_dir).join("checkpoints");
    let _ = tokio::task::spawn_blocking(move || {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(ordinal) = name
                .split('_')
                .next()
                .and_then(|prefix| prefix.parse::<usize>().ok())
            else {
                continue;
            };
            if ordinal >= start {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    })
    .await;
}

async fn checkpoint_failure(write_dir: Option<&str>, failure: &StepFailure) -> Result<(), String> {
    let Some(write_dir) = write_dir else {
        return Ok(());
    };
    let directory = std::path::PathBuf::from(write_dir).join("checkpoints");
    let destination = directory.join(format!("failure_{}.json", step_slug(&failure.step_id)));
    let failure = failure.clone();
    tokio::task::spawn_blocking(move || {
        use std::io::Write as _;
        std::fs::create_dir_all(&directory)
            .map_err(|error| format!("Failed to create checkpoint directory: {error}"))?;
        let json = serde_json::to_vec_pretty(&serde_json::json!({ "failure": failure }))
            .map_err(|error| format!("Failed to serialize failure checkpoint: {error}"))?;
        let mut temp = tempfile::NamedTempFile::new_in(&directory)
            .map_err(|error| format!("Failed to create checkpoint temp file: {error}"))?;
        temp.write_all(&json)
            .map_err(|error| format!("Failed to write failure checkpoint: {error}"))?;
        temp.flush()
            .map_err(|error| format!("Failed to flush failure checkpoint: {error}"))?;
        temp.as_file()
            .sync_all()
            .map_err(|error| format!("Failed to sync failure checkpoint: {error}"))?;
        temp.persist(&destination)
            .map_err(|error| format!("Failed to publish failure checkpoint: {}", error.error))?;
        Ok(())
    })
    .await
    .map_err(|error| format!("Failure checkpoint task failed: {error}"))?
}

async fn checkpoint_failures(
    app: &crate::emit::EventBus,
    write_dir: Option<&str>,
    failures: &[StepFailure],
) {
    for failure in failures {
        if let Err(error) = checkpoint_failure(write_dir, failure).await {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!(
                        "WARNING: could not checkpoint failed step '{}': {error}",
                        failure.step_label
                    )
                }),
            );
        }
    }
}

/// Runtime inventory from which a step's private, selector-filtered artifact
/// view is constructed.
struct ArtifactRuntime<'a> {
    orientation_path: &'a str,
    paper_text_path: &'a str,
    document_bundle_path: &'a str,
    source_path: &'a str,
    extra_inputs: &'a std::collections::HashMap<String, String>,
    extra_input_sources: &'a std::collections::HashMap<String, String>,
    outputs: &'a [StepOutput],
    run_artifact_dir: Option<&'a str>,
}

/// Concrete, call-owned view of a step's selected artifacts. App-controlled
/// files are staged into one private root so directory-scoped CLI permissions
/// cannot expose unselected siblings from the run temp directory.
struct ResolvedArtifactContext {
    _view: tempfile::TempDir,
    orientation_path: String,
    paper_text_path: String,
    document_bundle_path: String,
    source_path: String,
    extra_inputs: std::collections::HashMap<String, String>,
    prior_outputs: Vec<StepOutput>,
    read_dirs: Vec<String>,
    artifact_root: Option<String>,
    manifest: String,
    has_visuals: bool,
    includes_primary_text: bool,
    includes_survey: bool,
}

impl ResolvedArtifactContext {
    fn stage_current_item(&mut self, source: &str) -> Result<String, String> {
        let name = std::path::Path::new(source)
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("item"));
        let staged = stage_artifact_file(
            self._view.path(),
            source,
            &std::path::Path::new("input/current-item").join(name),
        )?;
        self.manifest
            .push_str(&format!("\n- Current fan-out item: {staged}"));
        Ok(staged)
    }
}

fn normalized_path(path: &std::path::Path) -> String {
    normalize_cli_root(&path.to_string_lossy())
        .unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"))
}

fn stage_artifact_file(
    view: &std::path::Path,
    source: &str,
    relative: &std::path::Path,
) -> Result<String, String> {
    let source_path = std::path::Path::new(source);
    let destination = view.join(relative);
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create artifact view: {error}"))?;
    }
    if std::fs::hard_link(source_path, &destination).is_err() {
        std::fs::copy(source_path, &destination).map_err(|error| {
            format!(
                "Failed to stage selected artifact '{}': {error}",
                source_path.display()
            )
        })?;
    }
    Ok(normalized_path(&destination))
}

fn stage_document_index(
    root: &std::path::Path,
    source: &str,
    relative: &std::path::Path,
) -> Result<String, String> {
    let raw = std::fs::read_to_string(source)
        .map_err(|error| format!("Failed to read DocumentBundle for model index: {error}"))?;
    let bundle: crate::document_bundle::DocumentBundle = serde_json::from_str(&raw)
        .map_err(|error| format!("Failed to parse DocumentBundle for model index: {error}"))?;
    bundle.validate()?;
    let index = bundle.to_model_index_pretty()?;
    let destination = root.join(relative);
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create document-index directory: {error}"))?;
    }
    std::fs::write(&destination, index)
        .map_err(|error| format!("Failed to stage document index: {error}"))?;
    Ok(normalized_path(&destination))
}

fn add_read_root(read_dirs: &mut Vec<String>, path: &std::path::Path) {
    if path.exists() {
        read_dirs.push(normalized_path(path));
    }
}

fn selected_primary_parts(step: &StepConfig) -> std::collections::HashSet<PrimaryArtifactPart> {
    step.context
        .include
        .iter()
        .find_map(|selector| match selector {
            ArtifactSelector::Primary { parts } => Some(parts.iter().copied().collect()),
            _ => None,
        })
        .unwrap_or_default()
}

fn resolve_artifact_context(
    step: &StepConfig,
    runtime: ArtifactRuntime<'_>,
) -> Result<ResolvedArtifactContext, String> {
    if step.phase == Phase::Parallel
        && step
            .context
            .include
            .iter()
            .any(|selector| matches!(selector, ArtifactSelector::Step { .. }))
    {
        return Err(format!(
            "Parallel step '{}' cannot consume another step's output",
            step.id
        ));
    }
    let view = tempfile::Builder::new()
        .prefix("pipeline_step_context_")
        .tempdir()
        .map_err(|error| format!("Failed to create context view for '{}': {error}", step.id))?;
    let mut read_dirs = Vec::new();
    let mut manifest_lines = vec![format!("ARTIFACT CONTEXT FOR STEP '{}':", step.label)];
    let primary_parts = selected_primary_parts(step);

    let paper_text_path = if primary_parts.contains(&PrimaryArtifactPart::Text)
        && !runtime.paper_text_path.is_empty()
    {
        let path = stage_artifact_file(
            view.path(),
            runtime.paper_text_path,
            std::path::Path::new("input/main/document.md"),
        )?;
        manifest_lines.push(format!("- Primary readable document: {path}"));
        path
    } else {
        String::new()
    };

    let document_bundle_path = if primary_parts.contains(&PrimaryArtifactPart::Structure)
        && !runtime.document_bundle_path.is_empty()
    {
        let path = stage_document_index(
            view.path(),
            runtime.document_bundle_path,
            std::path::Path::new("input/main/document_index.json"),
        )?;
        manifest_lines.push(format!(
            "- Compact primary document structure index: {path}"
        ));
        path
    } else {
        String::new()
    };

    let includes_survey = step
        .context
        .include
        .iter()
        .any(|selector| matches!(selector, ArtifactSelector::Survey));
    let orientation_path = if includes_survey && !runtime.orientation_path.is_empty() {
        let path = stage_artifact_file(
            view.path(),
            runtime.orientation_path,
            std::path::Path::new("survey/orientation.json"),
        )?;
        manifest_lines.push(format!("- Survey: {path}"));
        path
    } else {
        String::new()
    };

    let source_path = if primary_parts.contains(&PrimaryArtifactPart::Source)
        && !runtime.source_path.is_empty()
    {
        let source = std::path::Path::new(runtime.source_path);
        if source.is_dir() {
            add_read_root(&mut read_dirs, source);
            let path = normalized_path(source);
            manifest_lines.push(format!("- Primary source tree: {path}"));
            path
        } else {
            let name = source
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("source"));
            let path = stage_artifact_file(
                view.path(),
                runtime.source_path,
                &std::path::Path::new("input/main/source").join(name),
            )?;
            manifest_lines.push(format!("- Primary source file: {path}"));
            path
        }
    } else {
        String::new()
    };

    let has_visuals = primary_parts.contains(&PrimaryArtifactPart::Visuals);
    let artifact_root = if has_visuals {
        runtime
            .run_artifact_dir
            .and_then(|directory| std::path::Path::new(directory).parent())
            .map(normalized_path)
    } else {
        None
    };
    if has_visuals {
        if let Some(root) = runtime.run_artifact_dir {
            let root = std::path::Path::new(root);
            for subdir in ["pages", "figures", "document"] {
                add_read_root(&mut read_dirs, &root.join(subdir));
            }
        }
        if let Some(root) = artifact_root.as_deref() {
            manifest_lines.push(format!(
                "- Primary visual assets (bundle-relative root): {root}"
            ));
        }
    }

    let mut extra_inputs = std::collections::HashMap::new();
    for selector in &step.context.include {
        let ArtifactSelector::NamedInput { key, parts } = selector else {
            continue;
        };
        if parts.contains(&NamedInputArtifactPart::Text) {
            if let Some(source) = runtime.extra_inputs.get(key) {
                let path = stage_artifact_file(
                    view.path(),
                    source,
                    &std::path::Path::new("input/named").join(format!("{key}.md")),
                )?;
                manifest_lines.push(format!("- Named input '{key}' text: {path}"));
                extra_inputs.insert(key.clone(), path);
            }
        }
        if parts.contains(&NamedInputArtifactPart::Source) {
            if let Some(source) = runtime.extra_input_sources.get(key) {
                let original = std::path::Path::new(source);
                if original.is_dir() {
                    add_read_root(&mut read_dirs, original);
                    manifest_lines.push(format!(
                        "- Named input '{key}' source tree: {}",
                        normalized_path(original)
                    ));
                } else {
                    let name = original
                        .file_name()
                        .unwrap_or_else(|| std::ffi::OsStr::new("source"));
                    let staged = stage_artifact_file(
                        view.path(),
                        source,
                        &std::path::Path::new("input/named")
                            .join(key)
                            .join("source")
                            .join(name),
                    )?;
                    manifest_lines.push(format!("- Named input '{key}' source file: {staged}"));
                }
            }
        }
    }

    let report_steps: std::collections::HashSet<&str> = step
        .context
        .include
        .iter()
        .filter_map(|selector| match selector {
            ArtifactSelector::Step { step, parts, .. }
                if parts.contains(&StepArtifactPart::Report) =>
            {
                Some(step.as_str())
            }
            _ => None,
        })
        .collect();
    let prior_outputs: Vec<StepOutput> = runtime
        .outputs
        .iter()
        .filter(|output| report_steps.contains(base_id(&output.step_id)))
        .cloned()
        .collect();
    for output in &prior_outputs {
        let relative = std::path::Path::new("steps")
            .join(step_slug(base_id(&output.step_id)))
            .join(step_slug(&output.step_id))
            .join("report.md");
        let destination = view.path().join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("Failed to stage upstream report: {error}"))?;
        }
        std::fs::write(&destination, &output.raw_text)
            .map_err(|error| format!("Failed to stage upstream report: {error}"))?;
        manifest_lines.push(format!(
            "- Upstream report '{}' [{}]: {}",
            output.step_label,
            output.step_id,
            normalized_path(&destination)
        ));
    }

    if let Some(run_root) = runtime.run_artifact_dir {
        for selector in &step.context.include {
            let ArtifactSelector::Step {
                step: producer,
                parts,
                glob,
            } = selector
            else {
                continue;
            };
            if !parts.contains(&StepArtifactPart::Files) {
                continue;
            }
            let producer_root = std::path::Path::new(run_root)
                .join("by-step")
                .join(step_slug(producer));
            let Ok(instances) = std::fs::read_dir(&producer_root) else {
                continue;
            };
            let mut unit_files: Vec<(String, std::path::PathBuf)> = instances
                .flatten()
                .filter_map(|instance| {
                    let files = instance.path().join("files");
                    files
                        .is_dir()
                        .then(|| (instance.file_name().to_string_lossy().into_owned(), files))
                })
                .collect();
            unit_files.sort();
            // A multi-agent or fan-out producer has several units; staging
            // their trees into one shared destination would let same-named
            // files overwrite each other in read_dir order. Namespace each
            // unit by its on-disk unit slug (the report-staging convention);
            // a single unit keeps the flat layout.
            let namespace_units = unit_files.len() > 1;
            for (unit, files) in &unit_files {
                if glob.is_empty() {
                    add_read_root(&mut read_dirs, files);
                    manifest_lines.push(format!(
                        "- Supporting files from '{producer}': {}",
                        normalized_path(files)
                    ));
                } else {
                    let expanded = crate::pipeline::glob::expand(files, glob, 500);
                    if expanded
                        .limited_by
                        .is_some_and(|reason| reason != "match limit")
                    {
                        return Err(format!(
                            "Supporting-file selection for '{}' was incomplete ({})",
                            producer,
                            expanded.limited_by.unwrap_or("unknown limit")
                        ));
                    }
                    for matched in expanded.matches {
                        let source = std::path::Path::new(&matched);
                        let relative = source
                            .strip_prefix(files)
                            .map_err(|_| "Selected supporting file escaped its producer root")?;
                        let mut destination =
                            std::path::Path::new("steps").join(step_slug(producer));
                        if namespace_units {
                            destination = destination.join(unit);
                        }
                        let staged = stage_artifact_file(
                            view.path(),
                            &matched,
                            &destination.join("files").join(relative),
                        )?;
                        manifest_lines
                            .push(format!("- Supporting file from '{producer}': {staged}"));
                    }
                }
            }
        }
    }

    // Every staged file is beneath this one root. Original source trees and
    // unfiltered producer file roots were added separately above.
    add_read_root(&mut read_dirs, view.path());
    read_dirs.sort();
    read_dirs.dedup();

    Ok(ResolvedArtifactContext {
        _view: view,
        orientation_path,
        paper_text_path,
        document_bundle_path,
        source_path,
        extra_inputs,
        prior_outputs,
        read_dirs,
        artifact_root,
        manifest: manifest_lines.join("\n"),
        has_visuals,
        includes_primary_text: primary_parts.contains(&PrimaryArtifactPart::Text),
        includes_survey,
    })
}

/// Shared-context reuse is an optimization, never a precondition: any
/// failure to assemble the shared prefix (unreadable staged text, or content
/// past the runtime-context safety cap — possible because LaTeX extraction
/// permits more than the 8 MB shared-context limit) degrades the affected
/// calls to ordinary self-contained prompts instead of failing the run.
fn prepare_selected_shared_context(
    app: &crate::emit::EventBus,
    enabled: bool,
    resolved: &ResolvedArtifactContext,
    orientation: &serde_json::Value,
    pool: &super::context_cache::PreparedContextPool,
) -> Option<Arc<super::context_cache::PreparedContext>> {
    if !enabled || (!resolved.includes_primary_text && !resolved.includes_survey) {
        return None;
    }
    let degrade = |error: &str| {
        let _ = app.emit_event(
            "pipeline:log",
            serde_json::json!({
                "line": format!(
                    "WARNING: shared input context unavailable ({error}); running without context reuse"
                )
            }),
        );
        None
    };
    let text = if resolved.includes_primary_text && !resolved.paper_text_path.is_empty() {
        match std::fs::read_to_string(&resolved.paper_text_path) {
            Ok(text) => text,
            Err(error) => return degrade(&error.to_string()),
        }
    } else {
        String::new()
    };
    let empty_survey = serde_json::Value::Null;
    let survey = if resolved.includes_survey {
        orientation
    } else {
        &empty_survey
    };
    match pool.prepare(&text, survey) {
        Ok(prepared) => Some(prepared),
        Err(error) => degrade(&error),
    }
}

/// One authoritative row in the profile's planned execution timeline.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPlanStage {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub step_ids: Vec<String>,
    pub step_labels: Vec<String>,
}

/// Human-facing work item for the input-preparation stage. The selected
/// container and its declared interpretation are distinct, so a folder used
/// as a source tree must not be described as a document bundle.
pub fn input_processing_label(input_interpretation: &str) -> &'static str {
    match input_interpretation.trim() {
        "folder" | "source_tree" => "Creating source-tree inventory",
        "none" => "Preparing workflow context",
        _ => "Creating document bundle",
    }
}

/// Simulate the exact readiness scheduler used by [`execute_steps`] without
/// running dynamic `run_if` predicates. The resulting waves are the canonical
/// preflight timeline exposed to the UI.
pub fn execution_plan(config: &PipelineConfig) -> Result<Vec<ExecutionPlanStage>, String> {
    let mut plan = vec![ExecutionPlanStage {
        id: "extracting".to_string(),
        kind: "extracting".to_string(),
        label: input_processing_label(&config.extraction.input_mode).to_string(),
        step_ids: Vec::new(),
        step_labels: Vec::new(),
    }];
    plan.push(ExecutionPlanStage {
        id: "orienting".to_string(),
        kind: "orienting".to_string(),
        label: if crate::auto_review::uses_auto_review_contract(config) {
            "Creating orientation map & review plan"
        } else {
            "Creating orientation map"
        }
        .to_string(),
        step_ids: Vec::new(),
        step_labels: Vec::new(),
    });

    let enabled: Vec<&StepConfig> = config.steps.iter().filter(|step| step.enabled).collect();
    let dependencies = resolve_dependencies(&enabled);
    let mut done = std::collections::HashSet::new();
    let mut remaining: Vec<usize> = (0..enabled.len()).collect();
    let mut schedule_index = 0usize;
    let mut parallel_number = 0usize;
    let mut sequential_number = 0usize;
    while !remaining.is_empty() {
        let ready = ready_indices(&remaining, &dependencies, &done);
        if ready.is_empty() {
            let stuck = remaining
                .iter()
                .map(|index| enabled[*index].label.as_str())
                .collect::<Vec<_>>();
            return Err(format!(
                "Pipeline plan has unsatisfiable dependencies: {}",
                stuck.join(", ")
            ));
        }
        schedule_index += 1;
        let ready_parallel = ready
            .iter()
            .copied()
            .filter(|index| enabled[*index].phase == Phase::Parallel)
            .collect::<Vec<_>>();
        if !ready_parallel.is_empty() {
            parallel_number += 1;
            let wave_steps = ready_parallel
                .iter()
                .map(|index| enabled[*index])
                .collect::<Vec<_>>();
            plan.push(ExecutionPlanStage {
                id: format!("wave-{schedule_index}-parallel"),
                kind: "dispatching".to_string(),
                label: "Parallel agent wave".to_string(),
                step_ids: wave_steps.iter().map(|step| step.id.clone()).collect(),
                step_labels: wave_steps.iter().map(|step| step.label.clone()).collect(),
            });
            let merged_step_ids = wave_steps
                .iter()
                .filter(|step| step.agents.len() > 1)
                .map(|step| step.id.clone())
                .collect::<Vec<_>>();
            if config.merge.enabled && !merged_step_ids.is_empty() {
                plan.push(ExecutionPlanStage {
                    id: format!("wave-{schedule_index}-merge"),
                    kind: "merging".to_string(),
                    label: format!("Merge parallel wave {parallel_number}"),
                    step_ids: merged_step_ids,
                    step_labels: wave_steps
                        .iter()
                        .filter(|step| step.agents.len() > 1)
                        .map(|step| step.label.clone())
                        .collect(),
                });
            }
            mark_steps_done(&mut done, &wave_steps);
            remaining.retain(|index| !ready_parallel.contains(index));
            continue;
        }

        let index = *ready.iter().min().expect("ready list is non-empty");
        let step = enabled[index];
        sequential_number += 1;
        plan.push(ExecutionPlanStage {
            id: format!("wave-{schedule_index}-sequential"),
            kind: "synthesizing".to_string(),
            label: "Sequential agent wave".to_string(),
            step_ids: vec![step.id.clone()],
            step_labels: vec![if step.label.is_empty() {
                format!("Sequential step {sequential_number}")
            } else {
                step.label.clone()
            }],
        });
        done.insert(step.id.clone());
        remaining.retain(|candidate| *candidate != index);
    }
    plan.push(ExecutionPlanStage {
        id: "done".to_string(),
        kind: "done".to_string(),
        label: "Complete".to_string(),
        step_ids: Vec::new(),
        step_labels: Vec::new(),
    });
    Ok(plan)
}

/// Execute all enabled steps in the pipeline.
///
/// Steps run on an explicit dependency schedule: `after` contributes
/// order-only edges, while upstream step selectors contribute dataflow edges.
/// A step's `run_if` guard can skip it; a skipped step still "completes" so its
/// dependents proceed. A failed sequential step stops further execution but
/// does not discard prior outputs.
#[allow(clippy::too_many_arguments)]
pub async fn execute_steps(
    app: &crate::emit::EventBus,
    config: &PipelineConfig,
    orientation_path: &str,
    orientation_value: &serde_json::Value,
    paper_text_path: &str,
    document_bundle_path: &str,
    source_path: &str,
    paper_type: &str,
    survey_hint: &str,
    variables: &std::collections::HashMap<String, String>,
    extra_inputs: &std::collections::HashMap<String, String>,
    extra_input_sources: &std::collections::HashMap<String, String>,
    preloaded: &std::collections::HashMap<String, Vec<StepOutput>>,
    write_dir: Option<&str>,
    settings: &crate::settings::Settings,
) -> Result<ExecutionResult, String> {
    let semaphore = Arc::new(Semaphore::new(settings.max_workers.max(1) as usize));
    let output_budget = Arc::new(OutputBudget::default());
    let shared_context_pool = super::context_cache::PreparedContextPool::default();
    let mut all_outputs: Vec<StepOutput> = Vec::new();
    let mut failed_steps: Vec<StepFailure> = Vec::new();

    if let Some(plan) = orientation_value
        .pointer("/review_plan")
        .cloned()
        .and_then(|value| serde_json::from_value::<crate::models::ReviewPlan>(value).ok())
    {
        let subject_ids = if plan.subject_specialist_ids.is_empty() {
            std::iter::once(plan.field_specialist_id.as_str())
                .filter(|id| !id.is_empty())
                .collect::<Vec<_>>()
        } else {
            plan.subject_specialist_ids
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
        };
        let method_ids = plan
            .method_specialist_ids
            .iter()
            .map(String::as_str)
            .filter(|id| !id.is_empty())
            .collect::<Vec<_>>();
        let subject_labels = subject_ids
            .iter()
            .map(|id| crate::auto_review::label_for(id).unwrap_or(id).to_string())
            .collect::<Vec<_>>();
        let method_labels = method_ids
            .iter()
            .map(|id| crate::auto_review::label_for(id).unwrap_or(id).to_string())
            .collect::<Vec<_>>();
        let ids = subject_ids
            .iter()
            .chain(method_ids.iter())
            .copied()
            .collect::<Vec<_>>();
        let labels = ids
            .iter()
            .map(|id| crate::auto_review::label_for(id).unwrap_or(id).to_string())
            .collect::<Vec<_>>();
        let _ = app.emit_event(
            "pipeline:routing",
            serde_json::json!({
                "primaryDomain": plan.primary_domain,
                "subject": plan.subject,
                "subjectIds": subject_ids,
                "subjectLabels": subject_labels,
                "methodIds": method_ids,
                "methodLabels": method_labels,
                "specialistIds": ids,
                "specialistLabels": labels,
            }),
        );
        let _ = app.emit_event(
            "pipeline:log",
            serde_json::json!({
                "line": format!(
                    "Auto review detected {} / {}; selected {}",
                    plan.primary_domain,
                    plan.subject,
                    labels.join(", ")
                )
            }),
        );
    }

    let enabled: Vec<&StepConfig> = config.steps.iter().filter(|s| s.enabled).collect();
    let deps = resolve_dependencies(&enabled);

    // Ids that have completed (including skipped) so dependents can start.
    let mut done: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut remaining: Vec<usize> = (0..enabled.len()).collect();
    let mut schedule_index = 0usize;
    let mut parallel_number = 0usize;
    let mut sequential_number = 0usize;

    while !remaining.is_empty() {
        let ready = ready_indices(&remaining, &deps, &done);
        if ready.is_empty() {
            let stuck: Vec<&str> = remaining
                .iter()
                .map(|&i| enabled[i].label.as_str())
                .collect();
            return Err(format!(
                "Pipeline stalled — steps have unsatisfiable dependencies (a cycle or a disabled upstream step): {}",
                stuck.join(", ")
            ));
        }
        schedule_index += 1;

        let ready_parallel: Vec<usize> = ready
            .iter()
            .copied()
            .filter(|&i| enabled[i].phase == Phase::Parallel)
            .collect();

        if !ready_parallel.is_empty() {
            parallel_number += 1;
            let wave_id = format!("wave-{schedule_index}-parallel");
            let wave_label = "Parallel agent wave";
            let wave_step_ids = ready_parallel
                .iter()
                .map(|index| enabled[*index].id.clone())
                .collect::<Vec<_>>();
            let wave_step_labels = ready_parallel
                .iter()
                .map(|index| enabled[*index].label.clone())
                .collect::<Vec<_>>();
            let merged_step_ids = ready_parallel
                .iter()
                .filter(|index| enabled[**index].agents.len() > 1)
                .map(|index| enabled[*index].id.clone())
                .collect::<Vec<_>>();
            let merged_step_labels = ready_parallel
                .iter()
                .filter(|index| enabled[**index].agents.len() > 1)
                .map(|index| enabled[*index].label.clone())
                .collect::<Vec<_>>();
            let planned_merge = config.merge.enabled && !merged_step_ids.is_empty();
            app.emit_event(
                "pipeline:stage",
                serde_json::json!({
                    "stage": "dispatching",
                    "id": wave_id,
                    "label": wave_label,
                    "stepIds": wave_step_ids,
                    "stepLabels": wave_step_labels,
                    "mergeStepIds": merged_step_ids,
                    "mergeStepLabels": merged_step_labels,
                }),
            )
            .ok();

            // Partition into steps whose run_if guard passes (dispatch) and
            // those it skips (record a placeholder so dependents proceed).
            let mut to_run: Vec<&StepConfig> = Vec::new();
            for &i in &ready_parallel {
                let step = enabled[i];
                // Resume: a preloaded step reuses the parent run's output.
                if let Some(cached) = preloaded.get(&step.id) {
                    for output in cached {
                        if let Err(error) = output_budget.reserve(output) {
                            let _ = app.emit_event(
                                "pipeline:pass",
                                serde_json::json!({"name": step.id, "status": "error"}),
                            );
                            return Err(error);
                        }
                        checkpoint_outputs(
                            app,
                            write_dir,
                            all_outputs.len(),
                            std::slice::from_ref(output),
                        )
                        .await;
                        all_outputs.push(output.clone());
                    }
                    let _ = app.emit_event(
                        "pipeline:pass",
                        serde_json::json!({"name": step.id, "status": "done"}),
                    );
                    done.insert(step.id.clone());
                    continue;
                }
                if let Some(cond) = &step.run_if {
                    if !crate::pipeline::conditions::condition_met(
                        cond,
                        orientation_value,
                        &all_outputs,
                    ) {
                        emit_skip(app, step);
                        all_outputs.push(skip_output(step));
                        done.insert(step.id.clone());
                        continue;
                    }
                }
                to_run.push(step);
            }
            remaining.retain(|i| !ready_parallel.contains(i));

            if !to_run.is_empty() {
                let (mut wave_outputs, wave_failures) = run_parallel_wave(
                    app,
                    &to_run,
                    settings,
                    &semaphore,
                    orientation_path,
                    orientation_value,
                    paper_text_path,
                    document_bundle_path,
                    source_path,
                    paper_type,
                    survey_hint,
                    &config.parallel_context_template,
                    variables,
                    extra_inputs,
                    extra_input_sources,
                    &all_outputs,
                    write_dir,
                    &output_budget,
                    config.context_cache.enabled,
                    &shared_context_pool,
                )
                .await?;
                // Every dispatched step is terminal once its wave returns. A
                // failed unit and a fan-out with zero matching units both lack
                // a StepOutput, so deriving completion from outputs alone
                // leaves their dependents permanently blocked.
                mark_steps_done(&mut done, &to_run);
                checkpoint_failures(app, write_dir, &wave_failures).await;
                failed_steps.extend(wave_failures);

                let has_multi_agent = wave_outputs.iter().any(|o| !o.merge_group.is_empty());
                if planned_merge {
                    app.emit_event(
                        "pipeline:stage",
                        serde_json::json!({
                            "stage": "merging",
                            "id": format!("wave-{schedule_index}-merge"),
                            "label": format!("Merge parallel wave {parallel_number}"),
                            "stepIds": merged_step_ids,
                            "stepLabels": merged_step_labels,
                            "skipped": !has_multi_agent,
                        }),
                    )
                    .ok();
                }
                if has_multi_agent && config.merge.enabled {
                    match merge::merge_step_outputs(
                        app,
                        wave_outputs.clone(),
                        &config.merge,
                        &semaphore,
                        write_dir,
                        settings,
                    )
                    .await
                    {
                        Ok(mut merged) => {
                            enforce_merge_output_schemas(app, &to_run, &wave_outputs, &mut merged);
                            // `merged` holds clones of already-reserved
                            // pass-through outputs plus newly synthesized
                            // per-group merge reports; only the latter are new
                            // bytes. A merge result that would exceed the run
                            // budget degrades to the (already reserved)
                            // unmerged outputs instead of failing the run.
                            let already_reserved: std::collections::HashSet<&str> = wave_outputs
                                .iter()
                                .map(|output| output.raw_text.as_str())
                                .collect();
                            let mut reserve_failure = None;
                            for output in &merged {
                                if already_reserved.contains(output.raw_text.as_str()) {
                                    continue;
                                }
                                if let Err(error) = output_budget.reserve(output) {
                                    reserve_failure = Some(error);
                                    break;
                                }
                            }
                            match reserve_failure {
                                None => wave_outputs = merged,
                                Some(error) => {
                                    let _ = app.emit_event(
                                        "pipeline:log",
                                        serde_json::json!({ "line": format!(
                                            "WARNING: merged outputs exceed the run output budget ({error}); keeping the unmerged analyses."
                                        ) }),
                                    );
                                }
                            }
                        }
                        Err(e) if is_cancellation_error(&e) => return Err(e),
                        Err(e) => {
                            let _ = app.emit_event(
                                "pipeline:log",
                                serde_json::json!({ "line": format!("WARNING: merge failed: {e}. Using unmerged outputs.") }),
                            );
                        }
                    }
                }
                remove_checkpoints_from(write_dir, all_outputs.len()).await;
                checkpoint_outputs(app, write_dir, all_outputs.len(), &wave_outputs).await;
                all_outputs.extend(wave_outputs);
            } else if planned_merge {
                app.emit_event(
                    "pipeline:stage",
                    serde_json::json!({
                        "stage": "merging",
                        "id": format!("wave-{schedule_index}-merge"),
                        "label": format!("Merge parallel wave {parallel_number}"),
                        "stepIds": merged_step_ids,
                        "stepLabels": merged_step_labels,
                        "skipped": true,
                    }),
                )
                .ok();
            }
            continue;
        }

        // No parallel steps ready — run one sequential step (lowest index).
        let i = *ready.iter().min().unwrap();
        let step = enabled[i];
        remaining.retain(|&j| j != i);
        sequential_number += 1;
        app.emit_event(
            "pipeline:stage",
            serde_json::json!({
                "stage": "synthesizing",
                "id": format!("wave-{schedule_index}-sequential"),
                "label": "Sequential agent wave",
                "stepIds": [step.id.clone()],
                "stepLabels": [if step.label.is_empty() {
                    format!("Sequential step {sequential_number}")
                } else {
                    step.label.clone()
                }],
            }),
        )
        .ok();

        // Resume: a preloaded step reuses the parent run's output.
        if let Some(cached) = preloaded.get(&step.id) {
            for output in cached {
                if let Err(error) = output_budget.reserve(output) {
                    let _ = app.emit_event(
                        "pipeline:pass",
                        serde_json::json!({"name": step.id, "status": "error"}),
                    );
                    return Err(error);
                }
                checkpoint_outputs(
                    app,
                    write_dir,
                    all_outputs.len(),
                    std::slice::from_ref(output),
                )
                .await;
                all_outputs.push(output.clone());
            }
            let _ = app.emit_event(
                "pipeline:pass",
                serde_json::json!({"name": step.id, "status": "done"}),
            );
            done.insert(step.id.clone());
            continue;
        }

        if let Some(cond) = &step.run_if {
            if !crate::pipeline::conditions::condition_met(cond, orientation_value, &all_outputs) {
                emit_skip(app, step);
                all_outputs.push(skip_output(step));
                done.insert(step.id.clone());
                continue;
            }
        }

        let resolved = resolve_artifact_context(
            step,
            ArtifactRuntime {
                orientation_path,
                paper_text_path,
                document_bundle_path,
                source_path,
                extra_inputs,
                extra_input_sources,
                outputs: &all_outputs,
                run_artifact_dir: write_dir,
            },
        )?;
        let shared_context = prepare_selected_shared_context(
            app,
            config.context_cache.enabled,
            &resolved,
            orientation_value,
            &shared_context_pool,
        );
        match run_sequential_step(
            app,
            step,
            &resolved,
            if resolved.includes_survey {
                survey_hint
            } else {
                ""
            },
            variables,
            write_dir,
            settings,
            shared_context,
        )
        .await
        {
            Ok(output) => {
                output_budget.reserve(&output)?;
                checkpoint_outputs(
                    app,
                    write_dir,
                    all_outputs.len(),
                    std::slice::from_ref(&output),
                )
                .await;
                let _ = app.emit_event(
                    "pipeline:pass",
                    serde_json::json!({"name": step.id, "status": "done"}),
                );
                done.insert(step.id.clone());
                all_outputs.push(output);
            }
            Err(e) => {
                let _ = app.emit_event(
                    "pipeline:pass",
                    serde_json::json!({"name": step.id, "status": "error"}),
                );
                let _ = app.emit_event(
                    "pipeline:log",
                    serde_json::json!({ "line": format!("WARNING: step '{}' failed: {e}. Returning prior outputs.", step.label) }),
                );
                let failure = StepFailure {
                    step_id: step.id.clone(),
                    step_label: step.label.clone(),
                    phase: "sequential".to_string(),
                    error: e,
                };
                checkpoint_failures(app, write_dir, std::slice::from_ref(&failure)).await;
                failed_steps.push(failure);
                break;
            }
        }
    }

    Ok(ExecutionResult {
        outputs: all_outputs,
        failed_steps,
    })
}

fn ready_indices(
    remaining: &[usize],
    deps: &[std::collections::HashSet<String>],
    done: &std::collections::HashSet<String>,
) -> Vec<usize> {
    remaining
        .iter()
        .copied()
        .filter(|&i| deps[i].iter().all(|d| done.contains(d)))
        .collect()
}

fn mark_steps_done(done: &mut std::collections::HashSet<String>, steps: &[&StepConfig]) {
    done.extend(steps.iter().map(|step| step.id.clone()));
}

/// Base id of a (possibly composite) step key: "technical/claude" → "technical".
fn base_id(step_key: &str) -> &str {
    step_key.split('/').next().unwrap_or(step_key)
}

fn is_cancellation_error(error: &str) -> bool {
    error.to_ascii_lowercase().contains("cancelled")
}

fn cancellation_error(pass_key: &str) -> Option<String> {
    if crate::commands::is_cancelled() {
        Some("Pipeline cancelled".to_string())
    } else if crate::commands::is_pass_cancelled(pass_key) {
        Some(format!("Pass '{pass_key}' cancelled"))
    } else {
        None
    }
}

/// Delegate to the configuration module so validation and scheduling use the
/// exact same effective dependency graph.
fn resolve_dependencies(enabled: &[&StepConfig]) -> Vec<std::collections::HashSet<String>> {
    crate::pipeline_config::resolve_dependencies(enabled)
}

/// The set of enabled steps that transitively depend on any of `seeds`
/// (i.e. everything downstream of the seeds, seeds excluded). Used by resume to
/// decide what must re-run when some upstream steps are re-executed.
pub fn dependents_of(
    config: &PipelineConfig,
    seeds: &std::collections::HashSet<String>,
) -> std::collections::HashSet<String> {
    let enabled: Vec<&StepConfig> = config.steps.iter().filter(|s| s.enabled).collect();
    let deps = resolve_dependencies(&enabled);
    // Reverse edges: dep -> steps that depend on it.
    let mut dependents: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();
    for (i, s) in enabled.iter().enumerate() {
        for d in &deps[i] {
            dependents
                .entry(d.as_str())
                .or_default()
                .push(s.id.as_str());
        }
    }
    let mut out: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut stack: Vec<String> = seeds.iter().cloned().collect();
    while let Some(node) = stack.pop() {
        if let Some(children) = dependents.get(node.as_str()) {
            for c in children {
                if out.insert(c.to_string()) {
                    stack.push(c.to_string());
                }
            }
        }
    }
    out
}

/// Placeholder output for a step skipped by its run_if guard.
fn skip_output(step: &StepConfig) -> StepOutput {
    StepOutput {
        step_id: step.id.clone(),
        step_label: step.label.clone(),
        phase: match step.phase {
            Phase::Parallel => "parallel".into(),
            Phase::Sequential => "sequential".into(),
        },
        raw_text: "_(skipped: run_if condition not met)_".to_string(),
        skipped: true,
        ..Default::default()
    }
}

fn emit_skip(app: &crate::emit::EventBus, step: &StepConfig) {
    let _ = app.emit_event(
        "pipeline:pass",
        serde_json::json!({"name": step.id, "status": "skipped"}),
    );
    let _ = app.emit_event(
        "pipeline:log",
        serde_json::json!({ "line": format!("Step '{}' skipped (run_if condition not met)", step.label) }),
    );
}

// ── Artifact write handoff ──────────────────────────────────────────

/// Collision-resistant filesystem key for a step. Replacement-based slugs
/// made valid IDs such as `a.b` and `a_b` share one report file, so one step
/// could silently ingest another step's output.
fn step_slug(step_key: &str) -> String {
    use sha2::{Digest as _, Sha256};
    let mut readable = String::new();
    let mut previous_dash = false;
    for character in step_key.chars() {
        let normalized = if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
            character
        } else {
            '-'
        };
        if normalized == '-' {
            if previous_dash {
                continue;
            }
            previous_dash = true;
        } else {
            previous_dash = false;
        }
        readable.push(normalized);
        if readable.len() >= 40 {
            break;
        }
    }
    let readable = readable.trim_matches('-');
    let readable = if readable.is_empty() {
        "step"
    } else {
        readable
    };
    let digest = format!("{:x}", Sha256::digest(step_key.as_bytes()));
    format!("{readable}--{}", &digest[..12])
}

/// Allocate one producer-owned write root. Parallel agents and fan-out units
/// never share a writable namespace.
fn step_write_dir(
    run_artifact_dir: Option<&str>,
    step_key: &str,
) -> Result<Option<String>, String> {
    let Some(root) = run_artifact_dir else {
        return Ok(None);
    };
    let base = base_id(step_key);
    let directory = std::path::Path::new(root)
        .join("by-step")
        .join(step_slug(base))
        .join(step_slug(step_key));
    std::fs::create_dir_all(&directory).map_err(|error| {
        format!("Failed to create artifact directory for step '{step_key}': {error}")
    })?;
    Ok(Some(
        normalize_cli_root(&directory.to_string_lossy())
            .unwrap_or_else(|| directory.to_string_lossy().replace('\\', "/")),
    ))
}

/// Build the OUTPUT FORMAT block appended to every step prompt.
fn output_format_block(write_dir: Option<&str>, report_nonce: &str) -> String {
    report_output_format(write_dir, report_nonce)
}

fn append_shared_context_note(
    mut prompt: String,
    includes_primary_text: bool,
    includes_survey: bool,
) -> Result<String, String> {
    let material = match (includes_primary_text, includes_survey) {
        (true, true) => "extracted input text and survey",
        (true, false) => "extracted input text",
        (false, true) => "survey",
        (false, false) => "selected shared material",
    };
    crate::safety::push_str_limited(
        &mut prompt,
        &format!(
            "\n\nSHARED CONTEXT NOTE:\n\
             The selected {material} is already present in the shared context for this call. \
             Do not read its staged file again. You may still use the other artifacts listed \
             in this step's artifact context."
        ),
        crate::safety::MAX_EXPANDED_PROMPT_BYTES,
        "Shared-context prompt",
    )?;
    Ok(prompt)
}

fn append_evidence_retrieval_guidance(
    mut prompt: String,
    tools: &[String],
) -> Result<String, String> {
    let can_read_text = tools.iter().any(|tool| tool == "Read");
    let can_read_visuals = tools.iter().any(|tool| tool == "ReadDocumentAsset");
    let can_search_web = tools.iter().any(|tool| tool == "WebSearch");
    if !can_read_text && !can_read_visuals && !can_search_web {
        return Ok(prompt);
    }

    crate::safety::push_str_limited(
        &mut prompt,
        "\n\nEVIDENCE RETRIEVAL:\n\
         For evidence not already present in shared context, identify the independent items you \
         need before calling tools.",
        crate::safety::MAX_EXPANDED_PROMPT_BYTES,
        "Evidence-retrieval prompt",
    )?;
    if can_read_text {
        crate::safety::push_str_limited(
            &mut prompt,
            "\nWhen multiple bounded text ranges are needed, prefer ReadTextBatch when it is \
             offered; otherwise issue independent bounded reads together in one tool turn when \
             supported.",
            crate::safety::MAX_EXPANDED_PROMPT_BYTES,
            "Evidence-retrieval prompt",
        )?;
    }
    if can_read_visuals {
        crate::safety::push_str_limited(
            &mut prompt,
            "\nWhen multiple visual assets are needed, prefer ReadDocumentAssetsBatch when it is \
             offered; otherwise inspect independent images together in one tool turn when \
             supported.",
            crate::safety::MAX_EXPANDED_PROMPT_BYTES,
            "Evidence-retrieval prompt",
        )?;
    }
    if can_search_web {
        crate::safety::push_str_limited(
            &mut prompt,
            "\nForm the complete set of independent web queries first and issue them together in \
             one tool turn when supported.",
            crate::safety::MAX_EXPANDED_PROMPT_BYTES,
            "Evidence-retrieval prompt",
        )?;
    }
    crate::safety::push_str_limited(
        &mut prompt,
        "\nIf batching or parallel calls are unavailable, or any item is missing, truncated, or \
         fails, continue sequentially until every item required by the review instructions has \
         been checked. Batching is only an efficiency optimization: never omit evidence.",
        crate::safety::MAX_EXPANDED_PROMPT_BYTES,
        "Evidence-retrieval prompt",
    )?;
    if can_read_visuals {
        crate::safety::push_str_limited(
            &mut prompt,
            " Never substitute extracted text for a required visual inspection.",
            crate::safety::MAX_EXPANDED_PROMPT_BYTES,
            "Evidence-retrieval prompt",
        )?;
    }
    Ok(prompt)
}

/// Read (and remove) a legacy model-written report file. New calls return the
/// report through the terminal response; this remains only as a validated
/// compatibility path for old sessions/templates.
fn ingest_report_file_blocking(write_dir: Option<&str>, report_rel: &str) -> Option<String> {
    use std::io::Read as _;
    let dir = write_dir?;
    let path = std::path::Path::new(dir).join(report_rel);
    let file = crate::safety::open_regular_file(&path).ok()?;
    let mut bytes = Vec::with_capacity(64 * 1024);
    file.take(super::claude::MAX_STDOUT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    let _ = std::fs::remove_file(&path);
    if bytes.len() > super::claude::MAX_STDOUT_BYTES {
        return None;
    }
    let content = String::from_utf8(bytes).ok()?;
    if content.trim().is_empty() {
        return None;
    }
    Some(content.trim().to_string())
}

async fn ingest_report_file(write_dir: Option<&str>, report_rel: &str) -> Option<String> {
    let directory = write_dir.map(str::to_string);
    let relative = report_rel.to_string();
    tokio::task::spawn_blocking(move || {
        ingest_report_file_blocking(directory.as_deref(), &relative)
    })
    .await
    .ok()
    .flatten()
}

/// Derive filesystem tools from the resolved artifact view. Profiles declare
/// optional external capabilities; they never grant Read or Write directly.
fn tools_with_write(
    step_tools: &[String],
    has_readable_artifacts: bool,
    has_visuals: bool,
    write_dir: Option<&str>,
) -> Vec<String> {
    let mut tools: Vec<String> = step_tools
        .iter()
        .filter(|tool| !matches!(tool.as_str(), "Read" | "Write"))
        .cloned()
        .collect();
    if has_readable_artifacts {
        tools.push("Read".to_string());
    }
    if has_visuals {
        tools.push("ReadDocumentAsset".to_string());
    }
    if write_dir.is_some() && !tools.iter().any(|t| t == "Write") {
        tools.push("Write".to_string());
    }
    tools
}

struct StepCallRequest<'a> {
    app: &'a crate::emit::EventBus,
    pass_key: &'a str,
    log_label: &'a str,
    prompt: &'a str,
    tools: &'a [String],
    agent: Option<&'a str>,
    cwd: Option<&'a str>,
    read_dirs: &'a [String],
    /// Host-owned run `artifacts/` root used for response journaling. This is
    /// deliberately broader than, and never granted as, the model write root.
    run_artifact_dir: Option<&'a str>,
    write_dir: Option<&'a str>,
    report_rel: &'a str,
    report_nonce: &'a str,
    output_schema: Option<&'a serde_json::Value>,
    command_model: Option<&'a str>,
    display_model: &'a str,
    model_policy: &'a str,
    effort: &'a str,
    settings: &'a crate::settings::Settings,
    shared_context: Option<Arc<super::context_cache::PreparedContext>>,
}

struct StepCallResult {
    text: String,
    duration_secs: u64,
    usage: crate::pipeline::logging::CallUsage,
    attempt_count: u32,
    usage_limit_fallbacks: Vec<super::call::UsageLimitFallback>,
    effective_fallback: Option<super::call::UsageLimitFallback>,
}

async fn capture_response(
    request: &StepCallRequest<'_>,
    attempt: u32,
    source: &str,
    text: &str,
) -> Option<super::response_journal::CapturedAttempt> {
    match super::response_journal::capture(
        request.run_artifact_dir,
        request.pass_key,
        attempt,
        source,
        text,
    )
    .await
    {
        Ok(capture) => capture,
        Err(error) => {
            let _ = request.app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!(
                    "WARNING: could not preserve {} response attempt {}: {error}",
                    request.log_label, attempt,
                )}),
            );
            None
        }
    }
}

async fn finish_response_capture(
    request: &StepCallRequest<'_>,
    capture: Option<super::response_journal::CapturedAttempt>,
    status: super::response_journal::AttemptStatus,
    reason: &str,
) {
    let Some(capture) = capture else {
        return;
    };
    if let Err(error) = capture.finish(status, reason).await {
        let _ = request.app.emit_event(
            "pipeline:log",
            serde_json::json!({ "line": format!(
                "WARNING: could not classify a preserved response for {}: {error}",
                request.log_label,
            )}),
        );
    }
}

fn rejection_attempt_status(
    error: &ReportValidationError,
) -> super::response_journal::AttemptStatus {
    match error.kind() {
        ReportRejectionKind::Envelope => super::response_journal::AttemptStatus::RejectedEnvelope,
        ReportRejectionKind::Content => super::response_journal::AttemptStatus::RejectedContent,
    }
}

/// Execute one logical step call, including retries, report-file handoff, and
/// structured-output validation. Scheduling and terminal pass events remain
/// with the parallel/sequential callers.
async fn execute_step_call(request: StepCallRequest<'_>) -> Result<StepCallResult, String> {
    let timeout = request.settings.step_timeout_secs.max(60);
    let max_retries = request.settings.max_retries;
    let mut last_error = String::new();
    let mut total_duration_secs = 0u64;
    let mut total_usage = crate::pipeline::logging::CallUsage::default();
    let mut usage_limit_fallbacks = Vec::new();

    for attempt in 0..=max_retries {
        let attempt_number = attempt.saturating_add(1);
        if let Some(error) = cancellation_error(request.pass_key) {
            return Err(error);
        }
        if attempt > 0 {
            let _ = request.app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!(
                    "{}: retry {attempt}/{max_retries} after failure: {last_error}",
                    request.log_label,
                )}),
            );
            let _ = request.app.emit_event(
                "pipeline:pass",
                serde_json::json!({ "name": request.pass_key, "status": "running" }),
            );
        }

        // Consume any stale compatibility file before starting this attempt.
        // New prompts never ask the model to write the report there, but old
        // provider sessions/templates may still do so.
        let _ = ingest_report_file(request.write_dir, request.report_rel).await;

        let mut retry_prompt = String::new();
        let prompt = if attempt > 0 {
            retry_prompt.push_str(request.prompt);
            crate::safety::push_str_limited(
                &mut retry_prompt,
                &format!(
                    "\n\nRETRY NOTICE:\nThe previous response was rejected: {last_error}\n\
                     Return the entire report again and obey the OUTPUT FORMAT contract exactly."
                ),
                crate::safety::MAX_EXPANDED_PROMPT_BYTES,
                "Step retry prompt",
            )?;
            retry_prompt.as_str()
        } else {
            request.prompt
        };

        let call = super::call::execute(super::call::Request {
            app: request.app,
            pass_key: request.pass_key,
            log_label: request.log_label,
            prompt,
            tools: request.tools,
            timeout_secs: timeout,
            agent: request.agent,
            cwd: request.cwd,
            read_dirs: request.read_dirs,
            write_dir: request.write_dir,
            command_model: request.command_model,
            display_model: request.display_model,
            model_policy: request.model_policy,
            effort: request.effort,
            settings: request.settings,
            shared_context: request.shared_context.clone(),
        })
        .await;
        total_duration_secs = total_duration_secs.saturating_add(call.duration_secs);
        total_usage.add_usage(call.usage);
        let call_fallback = call.usage_limit_fallback.clone();
        if let Some(fallback) = call_fallback.clone() {
            usage_limit_fallbacks.push(fallback);
        }

        let compatibility_file = ingest_report_file(request.write_dir, request.report_rel).await;
        // Preserve provider-returned text before interpreting it. A malformed
        // envelope or schema can reject control-plane use without erasing the
        // report a reader may still want to inspect.
        let mut terminal_capture = match call.output.as_ref() {
            Ok(stdout) => capture_response(&request, attempt_number, "terminal", stdout).await,
            Err(_) => None,
        };
        let mut file_capture = match compatibility_file.as_deref() {
            Some(report_file) => {
                capture_response(&request, attempt_number, "compatibility-file", report_file).await
            }
            None => None,
        };

        if let Some(error) = cancellation_error(request.pass_key) {
            finish_response_capture(
                &request,
                terminal_capture.take(),
                super::response_journal::AttemptStatus::Ignored,
                &error,
            )
            .await;
            finish_response_capture(
                &request,
                file_capture.take(),
                super::response_journal::AttemptStatus::Ignored,
                &error,
            )
            .await;
            return Err(error);
        }

        let (text, mut accepted_capture) = match call.output {
            Ok(stdout) => match extract_report_envelope(&stdout, request.report_nonce) {
                Ok(report) => {
                    finish_response_capture(
                        &request,
                        file_capture.take(),
                        super::response_journal::AttemptStatus::Ignored,
                        "The validated terminal response was selected instead.",
                    )
                    .await;
                    (report, terminal_capture.take())
                }
                Err(stdout_error) => {
                    let stdout_reason = stdout_error.to_string();
                    finish_response_capture(
                        &request,
                        terminal_capture.take(),
                        rejection_attempt_status(&stdout_error),
                        &stdout_reason,
                    )
                    .await;
                    if let Some(report_file) = compatibility_file {
                        match extract_report_envelope(&report_file, request.report_nonce) {
                            Ok(report) => {
                                let _ = request.app.emit_event(
                                    "pipeline:log",
                                    serde_json::json!({ "line": format!(
                                        "{}: terminal response was invalid; accepted a validated compatibility report file",
                                        request.log_label,
                                    )}),
                                );
                                (report, file_capture.take())
                            }
                            Err(file_error) => {
                                let file_reason = file_error.to_string();
                                finish_response_capture(
                                    &request,
                                    file_capture.take(),
                                    rejection_attempt_status(&file_error),
                                    &file_reason,
                                )
                                .await;
                                last_error = format!(
                                    "invalid terminal report ({stdout_error}); compatibility report file was also invalid ({file_error})"
                                );
                                continue;
                            }
                        }
                    } else {
                        last_error = format!("invalid terminal report: {stdout_error}");
                        continue;
                    }
                }
            },
            Err(error) => {
                if is_cancellation_error(&error) {
                    finish_response_capture(
                        &request,
                        file_capture.take(),
                        super::response_journal::AttemptStatus::Ignored,
                        &error,
                    )
                    .await;
                    return Err(error);
                }
                if let Some(report_file) = compatibility_file {
                    match extract_report_envelope(&report_file, request.report_nonce) {
                        Ok(report) => {
                            let _ = request.app.emit_event(
                                "pipeline:log",
                                serde_json::json!({ "line": format!(
                                    "{}: call reported an error but wrote a complete validated compatibility report; using it. ({error})",
                                    request.log_label,
                                )}),
                            );
                            (report, file_capture.take())
                        }
                        Err(file_error) => {
                            let file_reason = file_error.to_string();
                            finish_response_capture(
                                &request,
                                file_capture.take(),
                                rejection_attempt_status(&file_error),
                                &file_reason,
                            )
                            .await;
                            last_error = format!(
                                "{error}; compatibility report file did not contain a complete validated report ({file_error})"
                            );
                            continue;
                        }
                    }
                } else {
                    if super::provider_error::is_usage_limit_error(&error) {
                        return Err(error);
                    }
                    last_error = error;
                    continue;
                }
            }
        };
        let text = normalize_math_delimiters(&text);

        if let Some(schema) = request.output_schema {
            if let Err(reason) = crate::pipeline::structured::check(schema, &text) {
                finish_response_capture(
                    &request,
                    accepted_capture.take(),
                    super::response_journal::AttemptStatus::RejectedSchema,
                    &reason,
                )
                .await;
                last_error = if attempt < max_retries {
                    format!("output did not satisfy schema: {reason}")
                } else {
                    format!("output did not satisfy schema after {max_retries} retries: {reason}")
                };
                continue;
            }
        }

        finish_response_capture(
            &request,
            accepted_capture.take(),
            super::response_journal::AttemptStatus::Accepted,
            if request.output_schema.is_some() {
                "Validated report boundaries and output schema."
            } else {
                "Validated report boundaries."
            },
        )
        .await;

        return Ok(StepCallResult {
            text,
            duration_secs: total_duration_secs,
            usage: total_usage,
            attempt_count: u32::try_from(
                total_usage
                    .provider_attempts
                    .max(u64::from(attempt.saturating_add(1))),
            )
            .unwrap_or(u32::MAX),
            usage_limit_fallbacks,
            effective_fallback: call_fallback,
        });
    }

    Err(if last_error.is_empty() {
        "step produced no output".to_string()
    } else {
        last_error
    })
}

// ── Parallel execution ──────────────────────────────────────────────

/// Build the prompt for a parallel step (referee-style).
#[allow(clippy::too_many_arguments)]
fn build_parallel_prompt(
    step: &StepConfig,
    paper_type: &str,
    orientation_path: &str,
    survey_hint: &str,
    paper_text_path: &str,
    document_bundle_path: &str,
    source_path: &str,
    template: &str,
    output_format: &str,
    artifact_root: Option<&str>,
) -> Result<String, String> {
    let normalized_path = paper_text_path.replace('\\', "/");
    let normalized_source = source_path.replace('\\', "/");

    let is_pdf = normalized_source.to_ascii_lowercase().ends_with(".pdf");
    let source_hint = if normalized_source.is_empty() {
        "The original source is not available to this step.".to_string()
    } else if is_pdf {
        format!(
            "The original PDF is at: {normalized_source}\n\
             When the orientation map lists a figure or table with a page number, you can read that page of the PDF to inspect the visual content."
        )
    } else {
        let source = std::path::Path::new(&normalized_source);
        let source_root = if source.is_dir() {
            normalized_source.clone()
        } else {
            source
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| normalized_source.clone())
        };
        format!(
            "The selected source context is at: {source_root}\n\
             For LaTeX inputs, this private view contains the main source and the bounded local dependencies discovered from it (such as included sections, bibliography files, and referenced figures)."
        )
    };
    let normalized_bundle = document_bundle_path.replace('\\', "/");
    let artifact_root = artifact_root.map(|directory| directory.replace('\\', "/"));
    let figure_hint = if normalized_bundle.is_empty() {
        source_hint
    } else {
        format!(
            "A compact DocumentBundle index (JSON) is at: {normalized_bundle}\n\
             Use the readable document for prose. Use this index selectively to locate equations, tables, figures, page references, provenance, and asset IDs; do not read it wholesale.\n\
             Asset rel_path values are relative to the run directory: {}\n\
             Inspect images with ReadDocumentAsset on direct APIs or the provider's native Read tool on CLI transports.\n{source_hint}",
            artifact_root
                .as_deref()
                .unwrap_or("(run artifact root unavailable)")
        )
    };

    let orientation_block = if orientation_path.is_empty() {
        String::new()
    } else {
        let normalized_orient = orientation_path.replace('\\', "/");
        format!("The orientation map (JSON) is at: {normalized_orient}\n{survey_hint}")
    };

    let limit = crate::safety::MAX_EXPANDED_PROMPT_BYTES;
    let mut expanded = template.to_string();
    for (needle, value) in [
        ("{step_prompt}", step.prompt.as_str()),
        ("{paper_type}", paper_type),
        ("{orientation}", orientation_block.as_str()),
        ("{paper_path}", normalized_path.as_str()),
        ("{input_path}", normalized_path.as_str()),
        ("{document_bundle}", normalized_bundle.as_str()),
        ("{figure_hint}", figure_hint.as_str()),
    ] {
        expanded =
            crate::safety::replace_all_limited(&expanded, needle, value, limit, "Parallel prompt")?;
    }

    // Custom templates created before `{output_format}` existed still need
    // the current nonce contract. Append it when there is no placeholder so a
    // stale static marker instruction cannot bypass validation.
    if expanded.contains("{output_format}") {
        crate::safety::replace_all_limited(
            &expanded,
            "{output_format}",
            output_format,
            limit,
            "Parallel prompt",
        )
    } else if output_format.is_empty() {
        Ok(expanded)
    } else {
        crate::safety::push_str_limited(&mut expanded, "\n\n", limit, "Parallel prompt")?;
        crate::safety::push_str_limited(&mut expanded, output_format, limit, "Parallel prompt")?;
        Ok(expanded)
    }
}

/// One dispatch of a parallel step: either an agent (normal / multi-agent) or a
/// fan-out item (a file, with `{item}` bound). `suffix` is the composite-key
/// suffix ("" for a single plain run).
struct Unit {
    agent: String,
    item: Option<String>,
    suffix: String,
    display: String,
    /// Suffix shared by all agents analyzing the same logical item. Empty for
    /// ordinary non-fan-out work.
    item_suffix: String,
    merge_agents: bool,
}

#[allow(clippy::too_many_arguments)]
fn step_output(
    step_key: &str,
    display_label: &str,
    phase: &str,
    provider: &str,
    agent: &str,
    call: StepCallResult,
    resolution: &crate::model_catalog::ResolvedModel,
    effort: &str,
) -> StepOutput {
    let fallback_usage = call.usage_limit_fallbacks.iter().fold(
        crate::pipeline::logging::CallUsage::default(),
        |mut total, fallback| {
            total.add_usage(fallback.fallback_usage);
            total
        },
    );
    let fallback_duration = call
        .usage_limit_fallbacks
        .iter()
        .fold(0u64, |total, fallback| {
            total.saturating_add(fallback.fallback_duration_secs)
        });
    let subtract = |total: u64, fallback: u64| total.saturating_sub(fallback);
    let primary_usage = crate::pipeline::logging::CallUsage {
        input_tokens: subtract(call.usage.input_tokens, fallback_usage.input_tokens),
        output_tokens: subtract(call.usage.output_tokens, fallback_usage.output_tokens),
        cached_input_tokens: subtract(
            call.usage.cached_input_tokens,
            fallback_usage.cached_input_tokens,
        ),
        cache_write_input_tokens: subtract(
            call.usage.cache_write_input_tokens,
            fallback_usage.cache_write_input_tokens,
        ),
        provider_attempts: subtract(
            call.usage.provider_attempts,
            fallback_usage.provider_attempts,
        ),
        model_round_trips: subtract(
            call.usage.model_round_trips,
            fallback_usage.model_round_trips,
        ),
        tool_calls: crate::models::ToolCallCounts {
            text_file: subtract(
                call.usage.tool_calls.text_file,
                fallback_usage.tool_calls.text_file,
            ),
            image: subtract(call.usage.tool_calls.image, fallback_usage.tool_calls.image),
            web: subtract(call.usage.tool_calls.web, fallback_usage.tool_calls.web),
            shell_or_other: subtract(
                call.usage.tool_calls.shell_or_other,
                fallback_usage.tool_calls.shell_or_other,
            ),
            unknown: subtract(
                call.usage.tool_calls.unknown,
                fallback_usage.tool_calls.unknown,
            ),
        },
    };
    let effective = call.effective_fallback.as_ref();
    let effective_provider = effective.map_or(provider, |fallback| fallback.provider.as_str());
    let effective_agent = effective.map_or(agent, |fallback| fallback.provider.as_str());
    let effective_resolution = effective.map_or(resolution, |fallback| &fallback.resolution);
    let mut call_records = vec![crate::models::StepCallRecord {
        role: if effective.is_some() {
            "usage_limit_primary".to_string()
        } else {
            "step".to_string()
        },
        provider: provider.to_string(),
        agent: agent.to_string(),
        model: resolution.resolved_model.clone(),
        model_transport: resolution.transport.clone(),
        model_policy: resolution.selection.label(),
        model_source: resolution.source.clone(),
        model_catalog_updated_at: resolution.catalog_updated_at.clone(),
        effort: if effort.trim().is_empty() {
            "default".to_string()
        } else {
            effort.to_string()
        },
        duration_secs: call.duration_secs.saturating_sub(fallback_duration),
        input_tokens: primary_usage.input_tokens,
        output_tokens: primary_usage.output_tokens,
        cached_input_tokens: primary_usage.cached_input_tokens,
        cache_write_input_tokens: primary_usage.cache_write_input_tokens,
        model_round_trips: primary_usage.model_round_trips,
        tool_calls: primary_usage.tool_calls,
        attempt_count: u32::try_from(primary_usage.provider_attempts).unwrap_or(u32::MAX),
    }];
    call_records.extend(call.usage_limit_fallbacks.iter().map(|fallback| {
        crate::models::StepCallRecord {
            role: "usage_limit_fallback".to_string(),
            provider: fallback.provider.clone(),
            agent: fallback.provider.clone(),
            model: fallback.resolution.resolved_model.clone(),
            model_transport: fallback.resolution.transport.clone(),
            model_policy: fallback.resolution.selection.label(),
            model_source: fallback.resolution.source.clone(),
            model_catalog_updated_at: fallback.resolution.catalog_updated_at.clone(),
            effort: if fallback.effort.trim().is_empty() {
                "default".to_string()
            } else {
                fallback.effort.clone()
            },
            duration_secs: fallback.fallback_duration_secs,
            input_tokens: fallback.fallback_usage.input_tokens,
            output_tokens: fallback.fallback_usage.output_tokens,
            cached_input_tokens: fallback.fallback_usage.cached_input_tokens,
            cache_write_input_tokens: fallback.fallback_usage.cache_write_input_tokens,
            model_round_trips: fallback.fallback_usage.model_round_trips,
            tool_calls: fallback.fallback_usage.tool_calls,
            attempt_count: u32::try_from(fallback.fallback_usage.provider_attempts)
                .unwrap_or(u32::MAX),
        }
    }));
    StepOutput {
        step_id: step_key.to_string(),
        step_label: display_label.to_string(),
        phase: phase.to_string(),
        provider: effective_provider.to_string(),
        agent: effective_agent.to_string(),
        raw_text: call.text,
        duration_secs: call.duration_secs,
        input_tokens: call.usage.input_tokens,
        output_tokens: call.usage.output_tokens,
        cached_input_tokens: call.usage.cached_input_tokens,
        cache_write_input_tokens: call.usage.cache_write_input_tokens,
        model_round_trips: call.usage.model_round_trips,
        tool_calls: call.usage.tool_calls,
        attempt_count: call.attempt_count,
        model: effective_resolution.resolved_model.clone(),
        model_transport: effective_resolution.transport.clone(),
        model_policy: effective_resolution.selection.label(),
        model_source: effective_resolution.source.clone(),
        model_catalog_updated_at: effective_resolution.catalog_updated_at.clone(),
        calls: call_records,
        ..Default::default()
    }
}

/// A merge report replaces schema-validated unit outputs, so it must satisfy
/// the producing step's `output_schema` too. A non-conforming merge falls back
/// to the first unit's already-valid output — never the concatenation banner,
/// which would also break JSON consumers. The fallback keeps every unit call
/// record, retains the merge call as `failed_merge` (matching the merge-failure
/// convention) so its spend stays visible, and takes the retained text's
/// provider/model provenance from the first unit.
fn enforce_merge_output_schemas(
    app: &crate::emit::EventBus,
    steps: &[&StepConfig],
    unit_outputs: &[StepOutput],
    merged: &mut [StepOutput],
) {
    for output in merged {
        // A group replacement (synthesized merge or merge-failure banner
        // fallback) carries the merge group as its step_id — the step id, or
        // `{id}/{item}` for a fan-out item. Pass-through unit outputs never
        // match a merge group and were already validated by execute_step_call.
        let Some(first_unit) = unit_outputs
            .iter()
            .find(|unit| unit.merge_group == output.step_id)
        else {
            continue;
        };
        let Some(schema) = steps
            .iter()
            .find(|step| {
                step.id == output.step_id
                    || output
                        .step_id
                        .strip_prefix(&step.id)
                        .is_some_and(|rest| rest.starts_with('/'))
            })
            .and_then(|step| step.output_schema.as_ref())
        else {
            continue;
        };
        let reason = if output.calls.iter().any(|call| call.role == "merge") {
            match crate::pipeline::structured::check(schema, &output.raw_text) {
                Ok(()) => continue,
                Err(reason) => reason,
            }
        } else {
            // The merge already failed upstream; its banner concatenation is
            // never schema-shaped, so replace it without validating.
            "the merge failed and its concatenation fallback is not schema-shaped".to_string()
        };
        let _ = app.emit_event(
            "pipeline:log",
            serde_json::json!({ "line": format!(
                "WARNING: merged output for step '{}' did not satisfy the step's output schema ({reason}); keeping the first agent's validated output.",
                output.step_label
            )}),
        );
        output.raw_text = first_unit.raw_text.clone();
        output.agent = first_unit.agent.clone();
        output.provider = first_unit.provider.clone();
        output.model = first_unit.model.clone();
        output.model_transport = first_unit.model_transport.clone();
        output.model_policy = first_unit.model_policy.clone();
        output.model_source = first_unit.model_source.clone();
        output.model_catalog_updated_at = first_unit.model_catalog_updated_at.clone();
        for call in &mut output.calls {
            if call.role == "merge" {
                call.role = "failed_merge".to_string();
            }
        }
    }
}

/// The directory a fan-out glob is resolved against: the input folder itself,
/// or the parent directory of a single-file input.
fn fan_out_root(source_path: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(source_path);
    if p.is_dir() {
        p.to_path_buf()
    } else {
        p.parent()
            .map(|d| d.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("."))
    }
}

/// Build the units to run for a step. Fan-out and agent selection are
/// independent dimensions, so a multi-agent fan-out produces their Cartesian
/// product. Fan-out with no matches returns empty; incomplete discovery is an
/// error rather than being misreported as a valid zero-match scan.
fn build_units(
    step: &StepConfig,
    settings: &crate::settings::Settings,
    source_path: &str,
    app: &crate::emit::EventBus,
) -> Result<Vec<Unit>, String> {
    let agents: Vec<String> = if step.agents.is_empty() {
        vec![settings.preferred_provider.clone()]
    } else {
        step.agents.clone()
    };
    let multi = agents.len() > 1;

    let units = if let Some(fe) = &step.for_each {
        // No-input profiles and runs whose source staging failed have an
        // empty source root; fan_out_root("") would resolve to the process
        // cwd, so treat this as zero matches instead of scanning it.
        if source_path.is_empty() {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!(
                    "WARNING: fan-out step '{}' has no source root to scan; treating glob '{}' as matching no files", step.label, fe.glob
                )}),
            );
            return Ok(Vec::new());
        }
        let root = fan_out_root(source_path);
        let expansion = crate::pipeline::glob::expand(&root, &fe.glob, fe.max.max(1) as usize);
        let items = expansion.matches;
        if let Some(reason) = expansion.limited_by {
            if reason != "match limit" {
                return Err(if reason == "cancelled" {
                    "Pipeline cancelled during fan-out discovery".to_string()
                } else {
                    format!(
                        "Fan-out discovery for '{}' stopped at the {reason}; results may be incomplete",
                        step.label
                    )
                });
            }
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!(
                    "Fan-out step '{}' was bounded by {reason} at {} files (glob '{}')", step.label, items.len(), fe.glob
                )}),
            );
        }
        if items.is_empty() {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!(
                    "WARNING: fan-out step '{}' matched no files for glob '{}'", step.label, fe.glob
                )}),
            );
            return Ok(Vec::new());
        }
        let mut used = std::collections::HashSet::new();
        items
            .into_iter()
            .flat_map(|path| {
                let base = std::path::Path::new(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.clone());
                let mut suffix = step_slug(&base);
                let mut n = 2;
                while !used.insert(suffix.clone()) {
                    suffix = format!("{}_{}", step_slug(&base), n);
                    n += 1;
                }
                agents.iter().cloned().map(move |agent| Unit {
                    suffix: if multi {
                        format!("{suffix}/{agent}")
                    } else {
                        suffix.clone()
                    },
                    item_suffix: suffix.clone(),
                    display: base.clone(),
                    agent,
                    item: Some(path.clone()),
                    merge_agents: multi,
                })
            })
            .collect::<Vec<_>>()
    } else {
        agents
            .into_iter()
            .map(|a| Unit {
                suffix: if multi { a.clone() } else { String::new() },
                display: capitalize(&a),
                agent: a,
                item: None,
                item_suffix: String::new(),
                merge_agents: multi,
            })
            .collect::<Vec<_>>()
    };
    Ok(units)
}

/// Run all parallel steps in a wave concurrently.
#[allow(clippy::too_many_arguments)]
async fn run_parallel_wave(
    app: &crate::emit::EventBus,
    steps: &[&StepConfig],
    settings: &crate::settings::Settings,
    semaphore: &Arc<Semaphore>,
    orientation_path: &str,
    orientation_value: &serde_json::Value,
    paper_text_path: &str,
    document_bundle_path: &str,
    source_path: &str,
    paper_type: &str,
    survey_hint: &str,
    context_template: &str,
    variables: &std::collections::HashMap<String, String>,
    extra_inputs: &std::collections::HashMap<String, String>,
    extra_input_sources: &std::collections::HashMap<String, String>,
    prior_outputs: &[StepOutput],
    write_dir: Option<&str>,
    output_budget: &Arc<OutputBudget>,
    context_cache_enabled: bool,
    shared_context_pool: &super::context_cache::PreparedContextPool,
) -> Result<(Vec<StepOutput>, Vec<StepFailure>), String> {
    let mut tasks: JoinSet<ParallelTaskResult> = JoinSet::new();
    let mut immediate_results: Vec<((usize, String), StepOutput)> = Vec::new();

    for (idx, step) in steps.iter().enumerate() {
        let step_owned = (*step).clone();
        let settings_owned = settings.clone();
        let source_owned = source_path.to_string();
        let app_owned = app.clone();
        let units = crate::commands::await_or_cancel(
            tokio::task::spawn_blocking(move || {
                build_units(&step_owned, &settings_owned, &source_owned, &app_owned)
            }),
            Some(&step.id),
        )
        .await?
        .map_err(|e| format!("Fan-out discovery task failed for '{}': {e}", step.label))?
        .map_err(|e| format!("Fan-out discovery failed for '{}': {e}", step.label))?;
        if units.is_empty() {
            let _ = app.emit_event(
                "pipeline:pass",
                serde_json::json!({"name": step.id, "status": "skipped"}),
            );
            immediate_results.push((
                (idx, String::new()),
                StepOutput {
                    step_id: step.id.clone(),
                    step_label: step.label.clone(),
                    phase: "parallel".to_string(),
                    raw_text: "_(skipped: fan-out matched no files)_".to_string(),
                    skipped: true,
                    ..Default::default()
                },
            ));
            continue;
        }

        for unit in &units {
            let id = step.id.clone();
            let label = step.label.clone();
            let agent_name = unit.agent.clone();
            let model_selection = step.model_selection_for(settings, &agent_name);
            let effort_override = step.effort_for(settings, &agent_name);
            let output_schema = step.output_schema.clone();
            let fan_out_item = unit.item.clone();
            let merge_group = if unit.merge_agents {
                if unit.item_suffix.is_empty() {
                    id.clone()
                } else {
                    format!("{}/{}", id, unit.item_suffix)
                }
            } else {
                String::new()
            };

            // Composite key when this is one of several units (multi-agent or
            // fan-out); the bare step id when it's a single plain run.
            let step_key = if unit.suffix.is_empty() {
                id.clone()
            } else {
                format!("{}/{}", id, unit.suffix)
            };
            // Publish every concrete provider/fan-out unit before it waits for
            // a worker permit. The progress view can then distinguish queued
            // work from a logical step that never expanded into provider calls.
            let _ = app.emit_event(
                "pipeline:pass",
                serde_json::json!({
                    "name": step_key,
                    "status": "pending"
                }),
            );

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
                },
            )?;
            let item_path = match unit.item.as_deref() {
                Some(item) => resolved.stage_current_item(item)?,
                None => String::new(),
            };
            let shared_context = prepare_selected_shared_context(
                app,
                context_cache_enabled,
                &resolved,
                orientation_value,
                shared_context_pool,
            );
            let run_artifact_dir = write_dir.map(str::to_string);
            let task_write_dir = step_write_dir(write_dir, &step_key)?;
            let tools = tools_with_write(
                &step.tools,
                !resolved.read_dirs.is_empty(),
                resolved.has_visuals,
                task_write_dir.as_deref(),
            );
            let report_rel = "report.md".to_string();
            let report_nonce = new_report_nonce()?;
            let output_format = output_format_block(task_write_dir.as_deref(), &report_nonce);
            let mut prompt = build_parallel_prompt(
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
            )?;
            crate::safety::push_str_limited(
                &mut prompt,
                &format!("\n\n{}", resolved.manifest),
                crate::safety::MAX_EXPANDED_PROMPT_BYTES,
                "Parallel artifact context",
            )?;
            let prompt = substitute_run_context(&prompt, variables, &resolved.extra_inputs)?;
            // Fan-out: bind {item} to this unit's file (empty otherwise).
            let prompt = crate::safety::replace_all_limited(
                &prompt,
                "{item}",
                &item_path,
                crate::safety::MAX_EXPANDED_PROMPT_BYTES,
                "Parallel prompt",
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
            let task_read_dirs = resolved.read_dirs.clone();

            let app_handle = app.clone();
            let step_key_emit = step_key.clone();
            let log_label = if unit.suffix.is_empty() {
                format!("Step: {}", label)
            } else if unit.merge_agents {
                format!(
                    "Step: {} [{} · {}]",
                    label,
                    unit.display,
                    capitalize(&agent_name)
                )
            } else {
                format!("Step: {} [{}]", label, unit.display)
            };
            let display_label = if unit.suffix.is_empty() {
                label.clone()
            } else {
                format!("{} [{}]", label, unit.display)
            };
            let sort_key = (idx, unit.suffix.clone());
            let sem = semaphore.clone();
            let task_cwd = if resolved.source_path.is_empty() {
                Some(normalized_path(resolved._view.path()))
            } else {
                let source = std::path::Path::new(&resolved.source_path);
                if source.is_dir() {
                    Some(resolved.source_path.clone())
                } else {
                    cli_parent_dir(&resolved.source_path)
                }
            };
            // display_label is moved into the success StepOutput; keep a copy
            // for failure reporting.
            let fail_label = display_label.clone();
            let settings = settings.clone();
            let output_budget = output_budget.clone();
            // Keep the staged artifact view alive until the provider call and
            // all retries have completed.
            let context_view = resolved._view;

            tasks.spawn(async move {
                let _context_view = context_view;
                if let Some(error) = cancellation_error(&step_key_emit) {
                    return Err(StepFailure {
                        step_id: step_key_emit.clone(),
                        step_label: fail_label.clone(),
                        phase: "parallel".to_string(),
                        error,
                    });
                }
                let _permit = crate::commands::await_or_cancel(sem.acquire(), Some(&step_key_emit))
                    .await
                    .map_err(|error| StepFailure {
                        step_id: step_key_emit.clone(),
                        step_label: fail_label.clone(),
                        phase: "parallel".to_string(),
                        error,
                    })?
                    .map_err(|_| StepFailure {
                        step_id: step_key_emit.clone(),
                        step_label: fail_label.clone(),
                        phase: "parallel".to_string(),
                        error: "Semaphore closed".to_string(),
                    })?;
                // Cancellation can happen while this unit is queued for the
                // worker permit. Re-check after acquisition before starting a
                // provider call (and therefore before incurring cost).
                if let Some(error) = cancellation_error(&step_key_emit) {
                    return Err(StepFailure {
                        step_id: step_key_emit.clone(),
                        step_label: fail_label.clone(),
                        phase: "parallel".to_string(),
                        error,
                    });
                }
                let _ = app_handle.emit_event(
                    "pipeline:pass",
                    serde_json::json!({
                        "name": step_key_emit,
                        "status": "running"
                    }),
                );
                let provider = agent_name.clone();
                let resolution = crate::commands::await_or_cancel(
                    crate::model_catalog::resolve(&provider, &settings, model_selection.as_ref()),
                    Some(&step_key_emit),
                )
                .await
                .map_err(|error| StepFailure {
                    step_id: step_key_emit.clone(),
                    step_label: fail_label.clone(),
                    phase: "parallel".to_string(),
                    error,
                })?
                .map_err(|error| StepFailure {
                    step_id: step_key_emit.clone(),
                    step_label: fail_label.clone(),
                    phase: "parallel".to_string(),
                    error,
                })?;
                let model_policy = resolution.selection.label();
                let call = execute_step_call(StepCallRequest {
                    app: &app_handle,
                    pass_key: &step_key_emit,
                    log_label: &log_label,
                    prompt: &prompt,
                    tools: &tools,
                    agent: Some(&agent_name),
                    cwd: task_cwd.as_deref(),
                    read_dirs: &task_read_dirs,
                    run_artifact_dir: run_artifact_dir.as_deref(),
                    write_dir: task_write_dir.as_deref(),
                    report_rel: &report_rel,
                    report_nonce: &report_nonce,
                    output_schema: output_schema.as_ref(),
                    command_model: resolution.command_model.as_deref(),
                    display_model: &resolution.resolved_model,
                    model_policy: &model_policy,
                    effort: &effort_override,
                    settings: &settings,
                    shared_context,
                })
                .await;

                match call {
                    Ok(call) => {
                        let mut output = step_output(
                            &step_key,
                            &display_label,
                            "parallel",
                            &provider,
                            &agent_name,
                            call,
                            &resolution,
                            &effort_override,
                        );
                        output.merge_group = merge_group;
                        output.fan_out_item = fan_out_item;
                        if let Err(error) = output_budget.reserve(&output) {
                            let _ = app_handle.emit_event(
                                "pipeline:pass",
                                serde_json::json!({ "name": step_key_emit, "status": "error" }),
                            );
                            return Err(StepFailure {
                                step_id: step_key.clone(),
                                step_label: fail_label.clone(),
                                phase: "parallel".to_string(),
                                error,
                            });
                        }
                        let _ = app_handle.emit_event(
                            "pipeline:pass",
                            serde_json::json!({ "name": step_key_emit, "status": "done" }),
                        );
                        Ok((sort_key, output))
                    }
                    Err(error) => {
                        let _ = app_handle.emit_event(
                            "pipeline:pass",
                            serde_json::json!({ "name": step_key_emit, "status": "error" }),
                        );
                        Err(StepFailure {
                            step_id: step_key,
                            step_label: fail_label,
                            phase: "parallel".to_string(),
                            error,
                        })
                    }
                }
            });
        }
    }

    let mut results = immediate_results;
    let mut failures: Vec<StepFailure> = Vec::new();

    while let Some(res) = tasks.join_next().await {
        match res {
            Ok(Ok(report)) => {
                // Durably checkpoint each unit as it completes, so a crash
                // later in the wave cannot lose analyses that already
                // finished. The completion-order ordinal is provisional: the
                // caller clears this wave's ordinal range and rewrites it in
                // final order once the wave (and any merge) settles.
                checkpoint_outputs(
                    app,
                    write_dir,
                    prior_outputs.len() + results.len(),
                    std::slice::from_ref(&report.1),
                )
                .await;
                results.push(report);
            }
            Ok(Err(failure)) => failures.push(failure),
            Err(e) => failures.push(StepFailure {
                step_id: "internal".to_string(),
                step_label: "Internal task".to_string(),
                phase: "parallel".to_string(),
                error: format!("Task panicked: {e}"),
            }),
        }
    }

    if !failures.is_empty() {
        for f in &failures {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!("WARNING: step failed: {}: {}", f.step_label, f.error) }),
            );
        }
        let _ = app.emit_event(
            "pipeline:log",
            serde_json::json!({ "line": format!(
                "WARNING: {}/{} parallel steps succeeded. Downstream steps will receive incomplete inputs.",
                results.len(), results.len() + failures.len()
            )}),
        );
    }

    // Sort by original config order, then agent name
    results.sort_by(|a, b| a.0.cmp(&b.0));
    Ok((results.into_iter().map(|(_, r)| r).collect(), failures))
}

// ── Sequential execution ────────────────────────────────────────────

/// Expand template placeholders in a sequential step's prompt.
fn expand_template(
    template: &str,
    orientation_path: &str,
    survey_hint: &str,
    prior_outputs: &[StepOutput],
    paper_text_path: &str,
    document_bundle_path: &str,
    source_path: &str,
) -> Result<String, String> {
    let orientation_ref = if orientation_path.is_empty() {
        String::new()
    } else {
        let normalized_orient = orientation_path.replace('\\', "/");
        format!("The orientation map (JSON) is at: {normalized_orient}\n{survey_hint}")
    };

    let limit = crate::safety::MAX_EXPANDED_PROMPT_BYTES;
    let mut prior_text = String::new();
    for (index, output) in prior_outputs
        .iter()
        .filter(|output| !output.skipped)
        .enumerate()
    {
        if index > 0 {
            crate::safety::push_str_limited(
                &mut prior_text,
                "\n\n---\n\n",
                limit,
                "Prior-step context",
            )?;
        }
        crate::safety::push_str_limited(&mut prior_text, "## ", limit, "Prior-step context")?;
        crate::safety::push_str_limited(
            &mut prior_text,
            &output.step_label,
            limit,
            "Prior-step context",
        )?;
        crate::safety::push_str_limited(&mut prior_text, "\n\n", limit, "Prior-step context")?;
        crate::safety::push_str_limited(
            &mut prior_text,
            &output.raw_text,
            limit,
            "Prior-step context",
        )?;
    }

    let last_output_text = prior_outputs
        .iter()
        .rev()
        .find(|output| !output.skipped)
        .map(|o| o.raw_text.as_str())
        .unwrap_or("(not yet generated)");

    // Substitute in ONE pass over the template only. Inserted values include
    // model-produced step outputs, which may quote placeholder-shaped text
    // from the reviewed document; rescanning them (as sequential
    // replace_all_limited calls did) would let that text pull in other step
    // reports, inject artifact paths, or balloon the prompt past the byte cap.
    // `{step:<id>}` references resolve exact composite ids ("technical/claude")
    // or base ids (joining all agents' outputs); unknown ids become a
    // parenthesized notice so the prompt stays readable.
    let replacements: [(&str, &str); 9] = [
        ("{orientation}", orientation_ref.as_str()),
        ("{prior_outputs}", prior_text.as_str()),
        ("{referee_reports}", prior_text.as_str()),
        ("{last_output}", last_output_text),
        ("{editor_synthesis}", last_output_text),
        ("{paper_path}", paper_text_path),
        ("{input_path}", paper_text_path),
        ("{document_bundle}", document_bundle_path),
        ("{source_path}", source_path),
    ];
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        crate::safety::push_str_limited(&mut out, &rest[..start], limit, "Sequential prompt")?;
        let candidate = &rest[start..];
        if let Some((needle, value)) = replacements
            .iter()
            .find(|(needle, _)| candidate.starts_with(needle))
        {
            crate::safety::push_str_limited(&mut out, value, limit, "Sequential prompt")?;
            rest = &candidate[needle.len()..];
        } else if let Some((after_open, end_rel)) = candidate
            .strip_prefix("{step:")
            .and_then(|after_open| after_open.find('}').map(|end| (after_open, end)))
        {
            let id = after_open[..end_rel].trim();
            append_step_ref(&mut out, id, prior_outputs, limit)?;
            rest = &after_open[end_rel + 1..];
        } else {
            crate::safety::push_str_limited(&mut out, "{", limit, "Sequential prompt")?;
            rest = &candidate[1..];
        }
    }
    crate::safety::push_str_limited(&mut out, rest, limit, "Sequential prompt")?;
    Ok(out)
}

fn append_step_ref(
    out: &mut String,
    id: &str,
    prior_outputs: &[StepOutput],
    limit: usize,
) -> Result<(), String> {
    if id.is_empty() {
        return crate::safety::push_str_limited(
            out,
            "(empty step reference)",
            limit,
            "Sequential prompt",
        );
    }
    // Exact id match wins (covers both "technical" and "technical/claude").
    if let Some(o) = prior_outputs.iter().find(|o| o.step_id == id) {
        return crate::safety::push_str_limited(out, &o.raw_text, limit, "Sequential prompt");
    }
    // Otherwise, gather all step outputs whose base id (before any '/') matches.
    let matches: Vec<&StepOutput> = prior_outputs
        .iter()
        .filter(|o| o.step_id.split('/').next() == Some(id))
        .collect();
    if matches.is_empty() {
        return crate::safety::push_str_limited(
            out,
            &format!("(no output for step '{id}')"),
            limit,
            "Sequential prompt",
        );
    }
    if matches.len() == 1 {
        return crate::safety::push_str_limited(
            out,
            &matches[0].raw_text,
            limit,
            "Sequential prompt",
        );
    }
    for (index, output) in matches.iter().enumerate() {
        if index > 0 {
            crate::safety::push_str_limited(out, "\n\n---\n\n", limit, "Sequential prompt")?;
        }
        for value in [
            "### ",
            output.step_label.as_str(),
            "\n\n",
            output.raw_text.as_str(),
        ] {
            crate::safety::push_str_limited(out, value, limit, "Sequential prompt")?;
        }
    }
    Ok(())
}

/// Replace `{<prefix>key}` placeholders using `map`. Unknown keys become an
/// empty string. `prefix` includes the trailing colon, e.g. "{var:".
fn substitute_placeholders(
    text: &str,
    needle: &str,
    map: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    if !text.contains(needle) {
        return Ok(text.to_string());
    }
    let limit = crate::safety::MAX_EXPANDED_PROMPT_BYTES;
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(needle) {
        crate::safety::push_str_limited(&mut out, &rest[..start], limit, "Run prompt")?;
        let after = &rest[start + needle.len()..];
        if let Some(end) = after.find('}') {
            let key = after[..end].trim();
            crate::safety::push_str_limited(
                &mut out,
                map.get(key).map(|s| s.as_str()).unwrap_or(""),
                limit,
                "Run prompt",
            )?;
            rest = &after[end + 1..];
        } else {
            crate::safety::push_str_limited(&mut out, &rest[start..], limit, "Run prompt")?;
            return Ok(out);
        }
    }
    crate::safety::push_str_limited(&mut out, rest, limit, "Run prompt")?;
    Ok(out)
}

/// Apply both run-time substitutions to a prompt: `{var:key}` (values) and
/// `{input:key}` (paths to extra named inputs' extracted text).
fn substitute_run_context(
    text: &str,
    vars: &std::collections::HashMap<String, String>,
    inputs: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let t = substitute_placeholders(text, "{var:", vars)?;
    substitute_placeholders(&t, "{input:", inputs)
}

/// Run a single sequential step.
#[allow(clippy::too_many_arguments)]
async fn run_sequential_step(
    app: &crate::emit::EventBus,
    step: &StepConfig,
    artifacts: &ResolvedArtifactContext,
    survey_hint: &str,
    variables: &std::collections::HashMap<String, String>,
    write_dir: Option<&str>,
    settings: &crate::settings::Settings,
    shared_context: Option<Arc<super::context_cache::PreparedContext>>,
) -> Result<StepOutput, String> {
    let _ = app.emit_event(
        "pipeline:pass",
        serde_json::json!({
            "name": step.id,
            "status": "running"
        }),
    );

    let base_prompt = expand_template(
        &step.prompt,
        &artifacts.orientation_path,
        survey_hint,
        &artifacts.prior_outputs,
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
        &output_format_block(task_write_dir.as_deref(), &report_nonce),
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
        tools: &tools,
        agent,
        cwd: source_dir.as_deref(),
        read_dirs: &artifacts.read_dirs,
        run_artifact_dir: write_dir,
        write_dir: task_write_dir.as_deref(),
        report_rel,
        report_nonce: &report_nonce,
        output_schema: step.output_schema.as_ref(),
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
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline_config::Phase;
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

    #[test]
    fn input_processing_label_matches_the_declared_input_meaning() {
        assert_eq!(
            input_processing_label("document"),
            "Creating document bundle"
        );
        assert_eq!(
            input_processing_label("latex_project"),
            "Creating document bundle"
        );
        assert_eq!(
            input_processing_label("folder"),
            "Creating source-tree inventory"
        );
        assert_eq!(
            input_processing_label("source_tree"),
            "Creating source-tree inventory"
        );
        assert_eq!(input_processing_label("none"), "Preparing workflow context");
    }

    #[test]
    fn execution_plan_uses_the_runtime_readiness_scheduler() {
        let mut first = make_step("first", Phase::Parallel);
        first.agents = vec!["claude".into(), "codex".into()];
        let second = make_step("second", Phase::Parallel);
        let mut synthesis = make_step("synthesis", Phase::Sequential);
        synthesis.label = "Synthesize evidence".into();
        synthesis.after = vec!["first".into(), "second".into()];
        let mut follow_up = make_step("follow-up", Phase::Parallel);
        follow_up.after = vec!["synthesis".into()];
        follow_up.run_if = Some(crate::pipeline_config::RunCondition::SurveyPath {
            pointer: "/paper/type".into(),
            equals: Some(serde_json::json!("theory")),
            exists: None,
            contains: None,
        });
        let mut disabled = make_step("disabled", Phase::Sequential);
        disabled.enabled = false;
        let config = PipelineConfig {
            steps: vec![first, second, synthesis, follow_up, disabled],
            merge: Default::default(),
            context_cache: Default::default(),
            use_orientation: true,
            orientation_prompt: String::new(),
            orientation_schema: None,
            extraction: Default::default(),
            parallel_context_template: String::new(),
            variables: Vec::new(),
        };

        let plan = execution_plan(&config).unwrap();
        assert_eq!(
            plan.iter()
                .map(|stage| stage.kind.as_str())
                .collect::<Vec<_>>(),
            vec![
                "extracting",
                "orienting",
                "dispatching",
                "merging",
                "synthesizing",
                "dispatching",
                "done"
            ]
        );
        assert_eq!(plan[2].step_ids, vec!["first", "second"]);
        assert_eq!(plan[2].label, "Parallel agent wave");
        assert_eq!(plan[2].step_labels, vec!["first", "second"]);
        assert_eq!(plan[3].step_ids, vec!["first"]);
        assert_eq!(plan[4].label, "Sequential agent wave");
        assert_eq!(plan[4].step_labels, vec!["Synthesize evidence"]);
        assert_eq!(plan[5].step_ids, vec!["follow-up"]);
        assert!(plan
            .iter()
            .all(|stage| !stage.step_ids.contains(&"disabled".to_string())));

        let mut auto_config = config;
        auto_config.orientation_schema = Some(serde_json::json!({
            "x-pipeline-contract": crate::auto_review::AUTO_REVIEW_CONTRACT
        }));
        let auto_plan = execution_plan(&auto_config).unwrap();
        assert_eq!(auto_plan[1].label, "Creating orientation map & review plan");
    }

    #[test]
    fn resumed_runtime_emits_every_planned_scheduler_stage_id() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(assert_resumed_runtime_emits_every_planned_scheduler_stage_id());
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

    #[test]
    fn selected_artifacts_are_staged_without_exposing_unselected_inputs() {
        let runtime_dir = tempfile::tempdir().unwrap();
        let paper = runtime_dir.path().join("paper.md");
        let survey = runtime_dir.path().join("survey.json");
        let named = runtime_dir.path().join("rubric.md");
        let source_dir = runtime_dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(&paper, "paper").unwrap();
        std::fs::write(&survey, "{}").unwrap();
        std::fs::write(&named, "rubric").unwrap();
        std::fs::write(source_dir.join("main.tex"), "source").unwrap();

        let mut step = make_step("reader", Phase::Parallel);
        step.context.include = vec![
            ArtifactSelector::Primary {
                parts: vec![PrimaryArtifactPart::Text],
            },
            ArtifactSelector::NamedInput {
                key: "rubric".into(),
                parts: vec![NamedInputArtifactPart::Text],
            },
        ];
        let named_inputs = std::collections::HashMap::from([(
            "rubric".to_string(),
            named.to_string_lossy().to_string(),
        )]);
        let named_sources = std::collections::HashMap::from([(
            "rubric".to_string(),
            source_dir.to_string_lossy().to_string(),
        )]);
        let resolved = resolve_artifact_context(
            &step,
            ArtifactRuntime {
                orientation_path: survey.to_str().unwrap(),
                paper_text_path: paper.to_str().unwrap(),
                document_bundle_path: "",
                source_path: source_dir.to_str().unwrap(),
                extra_inputs: &named_inputs,
                extra_input_sources: &named_sources,
                outputs: &[],
                run_artifact_dir: None,
            },
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(&resolved.paper_text_path).unwrap(),
            "paper"
        );
        assert_eq!(
            std::fs::read_to_string(resolved.extra_inputs.get("rubric").unwrap()).unwrap(),
            "rubric"
        );
        assert!(resolved.orientation_path.is_empty());
        assert!(resolved.source_path.is_empty());
        assert!(!resolved
            .read_dirs
            .iter()
            .any(|root| root == &normalized_path(&source_dir)));
    }

    #[test]
    fn selected_source_folder_grants_only_that_folder() {
        let folder = tempfile::tempdir().unwrap();
        let source_dir = folder.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();
        let expected = normalized_path(&source_dir);
        let parent = normalized_path(folder.path());
        let mut step = make_step("source-reader", Phase::Parallel);
        step.context.include = vec![ArtifactSelector::Primary {
            parts: vec![PrimaryArtifactPart::Source],
        }];

        let resolved = resolve_artifact_context(
            &step,
            ArtifactRuntime {
                orientation_path: "",
                paper_text_path: "",
                document_bundle_path: "",
                source_path: source_dir.to_str().unwrap(),
                extra_inputs: &Default::default(),
                extra_input_sources: &Default::default(),
                outputs: &[],
                run_artifact_dir: None,
            },
        )
        .unwrap();

        assert!(resolved.read_dirs.iter().any(|root| root == &expected));
        assert!(!resolved.read_dirs.iter().any(|root| root == &parent));
    }

    #[test]
    fn supporting_file_glob_stages_only_matching_producer_files() {
        let run = tempfile::tempdir().unwrap();
        let artifacts = run.path().join("artifacts");
        let files = artifacts
            .join("by-step")
            .join(step_slug("producer"))
            .join(step_slug("producer"))
            .join("files");
        std::fs::create_dir_all(&files).unwrap();
        std::fs::write(files.join("selected.csv"), "selected").unwrap();
        std::fs::write(files.join("unselected.txt"), "unselected").unwrap();

        let mut step = make_step("consumer", Phase::Sequential);
        step.context.include = vec![ArtifactSelector::Step {
            step: "producer".into(),
            parts: vec![StepArtifactPart::Files],
            glob: "*.csv".into(),
        }];
        let resolved = resolve_artifact_context(
            &step,
            ArtifactRuntime {
                orientation_path: "",
                paper_text_path: "",
                document_bundle_path: "",
                source_path: "",
                extra_inputs: &Default::default(),
                extra_input_sources: &Default::default(),
                outputs: &[],
                run_artifact_dir: Some(artifacts.to_str().unwrap()),
            },
        )
        .unwrap();

        assert!(resolved.manifest.contains("selected.csv"));
        assert!(!resolved.manifest.contains("unselected.txt"));
        assert!(resolved
            ._view
            .path()
            .join("steps")
            .join(step_slug("producer"))
            .join("files")
            .join("selected.csv")
            .is_file());
        assert!(!resolved
            ._view
            .path()
            .join("steps")
            .join(step_slug("producer"))
            .join("files")
            .join("unselected.txt")
            .exists());
    }

    #[test]
    fn multi_unit_supporting_files_stage_into_per_unit_namespaces() {
        let run = tempfile::tempdir().unwrap();
        let artifacts = run.path().join("artifacts");
        let producer_root = artifacts.join("by-step").join(step_slug("producer"));
        let unit_a = step_slug("producer/claude");
        let unit_b = step_slug("producer/codex");
        for (unit, content) in [(&unit_a, "from claude"), (&unit_b, "from codex")] {
            let files = producer_root.join(unit).join("files");
            std::fs::create_dir_all(&files).unwrap();
            std::fs::write(files.join("table.csv"), content).unwrap();
        }

        let mut step = make_step("consumer", Phase::Sequential);
        step.context.include = vec![ArtifactSelector::Step {
            step: "producer".into(),
            parts: vec![StepArtifactPart::Files],
            glob: "*.csv".into(),
        }];
        let resolved = resolve_artifact_context(
            &step,
            ArtifactRuntime {
                orientation_path: "",
                paper_text_path: "",
                document_bundle_path: "",
                source_path: "",
                extra_inputs: &Default::default(),
                extra_input_sources: &Default::default(),
                outputs: &[],
                run_artifact_dir: Some(artifacts.to_str().unwrap()),
            },
        )
        .unwrap();

        let staged_root = resolved
            ._view
            .path()
            .join("steps")
            .join(step_slug("producer"));
        let staged_a = staged_root.join(&unit_a).join("files").join("table.csv");
        let staged_b = staged_root.join(&unit_b).join("files").join("table.csv");
        assert_eq!(std::fs::read_to_string(&staged_a).unwrap(), "from claude");
        assert_eq!(std::fs::read_to_string(&staged_b).unwrap(), "from codex");
        // Both staged paths are listed, each exactly once.
        assert_eq!(
            resolved
                .manifest
                .matches(&normalized_path(&staged_a))
                .count(),
            1
        );
        assert_eq!(
            resolved
                .manifest
                .matches(&normalized_path(&staged_b))
                .count(),
            1
        );
    }

    #[test]
    fn parallel_artifact_resolution_rejects_step_outputs() {
        let mut step = make_step("parallel-consumer", Phase::Parallel);
        step.context.include = vec![ArtifactSelector::Step {
            step: "producer".into(),
            parts: vec![StepArtifactPart::Report],
            glob: String::new(),
        }];
        let error = resolve_artifact_context(
            &step,
            ArtifactRuntime {
                orientation_path: "",
                paper_text_path: "",
                document_bundle_path: "",
                source_path: "",
                extra_inputs: &Default::default(),
                extra_input_sources: &Default::default(),
                outputs: &[],
                run_artifact_dir: None,
            },
        )
        .err()
        .unwrap();
        assert!(error.contains("cannot consume another step's output"));
    }

    // ── resolve_dependencies (explicit order + artifact dataflow) ──

    fn deps_of(steps: &[StepConfig]) -> Vec<std::collections::HashSet<String>> {
        let refs: Vec<&StepConfig> = steps.iter().collect();
        resolve_dependencies(&refs)
    }

    #[test]
    fn parallel_steps_without_edges_have_no_dependencies() {
        let steps = [
            make_step("a", Phase::Parallel),
            make_step("b", Phase::Parallel),
        ];
        let deps = deps_of(&steps);
        assert!(deps[0].is_empty());
        assert!(deps[1].is_empty());
    }

    #[test]
    fn isolated_sequential_has_no_implicit_dependencies() {
        let steps = [
            make_step("a", Phase::Parallel),
            make_step("s", Phase::Sequential),
        ];
        let deps = deps_of(&steps);
        assert!(deps[1].is_empty());
    }

    #[test]
    fn order_and_artifact_dependencies_are_unioned() {
        let a = make_step("a", Phase::Parallel);
        let c = make_step("c", Phase::Parallel);
        let mut b = make_step("b", Phase::Sequential);
        b.after = vec!["a".into()];
        b.context
            .include
            .push(crate::pipeline_config::ArtifactSelector::Step {
                step: "c".into(),
                parts: vec![crate::pipeline_config::StepArtifactPart::Report],
                glob: String::new(),
            });
        let deps = deps_of(&[a, c, b]);
        assert!(deps[0].is_empty());
        assert_eq!(
            deps[2],
            ["a".to_string(), "c".to_string()].into_iter().collect()
        );
    }

    #[test]
    fn resolve_dependencies_empty() {
        let deps = deps_of(&[]);
        assert!(deps.is_empty());
    }

    #[test]
    fn base_id_strips_agent() {
        assert_eq!(base_id("technical"), "technical");
        assert_eq!(base_id("technical/claude"), "technical");
    }

    // ── build_units (non-fan-out) ──────────────────────────────────

    #[test]
    fn build_units_single_agent_has_empty_suffix() {
        let step = make_step("s", Phase::Parallel);
        let settings = crate::settings::Settings::default();
        let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
        let units = build_units(&step, &settings, "/tmp/x.pdf", &bus).unwrap();
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].suffix, ""); // bare step id, no composite key
        assert_eq!(units[0].agent, settings.preferred_provider);
    }

    #[test]
    fn build_units_multi_agent_keys_by_agent() {
        let mut step = make_step("s", Phase::Parallel);
        step.agents = vec!["claude".into(), "antigravity".into()];
        let settings = crate::settings::Settings::default();
        let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
        let units = build_units(&step, &settings, "/tmp/x.pdf", &bus).unwrap();
        assert_eq!(units.len(), 2);
        assert_eq!(units[0].suffix, "claude");
        assert_eq!(units[1].suffix, "antigravity");
    }

    #[test]
    fn fan_out_builds_item_by_agent_cartesian_product() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("a")).unwrap();
        std::fs::create_dir_all(temp.path().join("b")).unwrap();
        std::fs::write(temp.path().join("a/note.md"), "a").unwrap();
        std::fs::write(temp.path().join("b/note.md"), "b").unwrap();

        let mut step = make_step("s", Phase::Parallel);
        step.agents = vec!["claude".into(), "antigravity".into()];
        step.for_each = Some(crate::pipeline_config::ForEach {
            glob: "**/*.md".into(),
            max: 10,
        });
        let settings = crate::settings::Settings::default();
        let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
        let units = build_units(&step, &settings, temp.path().to_str().unwrap(), &bus).unwrap();

        assert_eq!(units.len(), 4);
        assert_eq!(units.iter().filter(|u| u.agent == "claude").count(), 2);
        assert_eq!(units.iter().filter(|u| u.agent == "antigravity").count(), 2);
        let item_keys: std::collections::HashSet<&str> =
            units.iter().map(|u| u.item_suffix.as_str()).collect();
        assert_eq!(item_keys.len(), 2, "duplicate basenames need distinct keys");
        assert!(units.iter().all(|u| u.merge_agents));
    }

    #[test]
    fn fan_out_with_empty_source_root_matches_nothing() {
        let mut step = make_step("s", Phase::Parallel);
        step.for_each = Some(crate::pipeline_config::ForEach {
            glob: "**/*.md".into(),
            max: 10,
        });
        let settings = crate::settings::Settings::default();
        let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
        // An empty source root must not fall back to scanning the process cwd.
        let units = build_units(&step, &settings, "", &bus).unwrap();
        assert!(units.is_empty());
    }

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

    #[test]
    fn non_conforming_merge_falls_back_to_first_unit_output() {
        let mut step = make_step("s", Phase::Parallel);
        step.output_schema = Some(serde_json::json!({"type": "object", "required": ["issues"]}));
        let units = vec![
            schema_unit("claude", "{\"issues\": []}"),
            schema_unit("codex", "{\"issues\": [1]}"),
        ];
        let mut merged = vec![merge_output("A narrative merge, not issues JSON.")];
        let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
        enforce_merge_output_schemas(&bus, &[&step], &units, &mut merged);
        assert_eq!(merged[0].raw_text, "{\"issues\": []}");
        assert_eq!(merged[0].agent, "claude");
        assert!(merged[0].calls.iter().any(|c| c.role == "failed_merge"));
        assert!(merged[0].calls.iter().all(|c| c.role != "merge"));
    }

    #[test]
    fn banner_fallback_for_schema_step_is_replaced_by_first_unit() {
        let mut step = make_step("s", Phase::Parallel);
        step.output_schema = Some(serde_json::json!({"type": "object", "required": ["issues"]}));
        let units = vec![
            schema_unit("claude", "{\"issues\": []}"),
            schema_unit("codex", "{\"issues\": [1]}"),
        ];
        // merge.rs's merge-failure fallback: a banner plus concatenated unit
        // outputs (which contain extractable JSON) and no successful merge call.
        let mut banner = merge_output(
            "> **Note**: Multi-agent merge failed. Showing individual agent outputs.\n\n{\"issues\": []}\n\n---\n\n{\"issues\": [1]}",
        );
        banner.calls[1].role = "failed_merge".into();
        let mut merged = vec![banner];
        let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
        enforce_merge_output_schemas(&bus, &[&step], &units, &mut merged);
        assert_eq!(merged[0].raw_text, "{\"issues\": []}");
        assert_eq!(merged[0].agent, "claude");
    }

    #[test]
    fn conforming_merge_and_schemaless_step_are_untouched() {
        let mut schema_step = make_step("s", Phase::Parallel);
        schema_step.output_schema =
            Some(serde_json::json!({"type": "object", "required": ["issues"]}));
        let plain_step = make_step("p", Phase::Parallel);
        let units = vec![
            schema_unit("claude", "{\"issues\": []}"),
            StepOutput {
                step_id: "p/claude".into(),
                merge_group: "p".into(),
                agent: "claude".into(),
                raw_text: "plain unit".into(),
                ..Default::default()
            },
        ];
        let mut plain_merge = merge_output("Narrative merge of a schemaless step.");
        plain_merge.step_id = "p".into();
        let mut merged = vec![merge_output("{\"issues\": [1, 2]}"), plain_merge];
        let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
        enforce_merge_output_schemas(&bus, &[&schema_step, &plain_step], &units, &mut merged);
        assert_eq!(merged[0].raw_text, "{\"issues\": [1, 2]}");
        assert!(merged[0].calls.iter().any(|c| c.role == "merge"));
        assert_eq!(merged[1].raw_text, "Narrative merge of a schemaless step.");
    }

    #[test]
    fn terminal_parallel_failure_unblocks_dependent_step() {
        let mut downstream = make_step("downstream", Phase::Sequential);
        downstream.after = vec!["ok".into(), "failed".into()];
        let steps = [
            make_step("ok", Phase::Parallel),
            make_step("failed", Phase::Parallel),
            downstream,
        ];
        let refs: Vec<&StepConfig> = steps.iter().collect();
        let deps = resolve_dependencies(&refs);
        let mut done = std::collections::HashSet::new();

        // A returned wave is terminal even when only one member produced an
        // output. Completion must follow dispatch, not StepOutput presence.
        mark_steps_done(&mut done, &refs[..2]);

        assert_eq!(ready_indices(&[2], &deps, &done), vec![2]);
    }

    #[test]
    fn zero_match_fan_out_is_terminal_and_unblocks_dependent_step() {
        let temp = tempfile::tempdir().unwrap();
        let mut fan = make_step("fan", Phase::Parallel);
        fan.for_each = Some(crate::pipeline_config::ForEach {
            glob: "**/*.does-not-exist".into(),
            max: 20,
        });
        let mut downstream = make_step("downstream", Phase::Sequential);
        downstream.after = vec!["fan".into()];
        let steps = [fan, downstream];
        let refs: Vec<&StepConfig> = steps.iter().collect();
        let settings = crate::settings::Settings::default();
        let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);

        assert!(
            build_units(&steps[0], &settings, temp.path().to_str().unwrap(), &bus)
                .unwrap()
                .is_empty()
        );

        let deps = resolve_dependencies(&refs);
        let mut done = std::collections::HashSet::new();
        mark_steps_done(&mut done, &refs[..1]);
        assert_eq!(ready_indices(&[1], &deps, &done), vec![1]);
    }

    #[test]
    fn zero_match_fan_out_returns_visible_skipped_output() {
        let temp = tempfile::tempdir().unwrap();
        let mut fan = make_step("fan", Phase::Parallel);
        fan.for_each = Some(crate::pipeline_config::ForEach {
            glob: "**/*.does-not-exist".into(),
            max: 20,
        });
        let settings = crate::settings::Settings::default();
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(1));
        let output_budget = std::sync::Arc::new(OutputBudget::default());
        let shared_context_pool = crate::pipeline::context_cache::PreparedContextPool::default();
        let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
        let steps = [&fan];
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (outputs, failures) = runtime
            .block_on(run_parallel_wave(
                &bus,
                &steps,
                &settings,
                &semaphore,
                "",
                &serde_json::Value::Null,
                "",
                "",
                temp.path().to_str().unwrap(),
                "mixed",
                "",
                "{step_prompt}",
                &Default::default(),
                &Default::default(),
                &Default::default(),
                &[],
                None,
                &output_budget,
                false,
                &shared_context_pool,
            ))
            .unwrap();
        assert!(failures.is_empty());
        assert_eq!(outputs.len(), 1);
        assert!(outputs[0].skipped);
        assert_eq!(outputs[0].step_id, "fan");
        assert!(outputs[0].raw_text.contains("matched no files"));
    }

    // ── dependents_of ──────────────────────────────────────────────

    #[test]
    fn dependents_of_finds_transitive_downstream() {
        use crate::pipeline_config::{MergeConfig, PipelineConfig};
        let mut synthesis = make_step("s", Phase::Sequential);
        synthesis.context.include = vec![ArtifactSelector::Step {
            step: "a".into(),
            parts: vec![StepArtifactPart::Report],
            glob: String::new(),
        }];
        let config = PipelineConfig {
            steps: vec![
                make_step("a", Phase::Parallel),
                make_step("b", Phase::Parallel),
                synthesis,
            ],
            merge: MergeConfig::default(),
            context_cache: Default::default(),
            use_orientation: true,
            orientation_prompt: String::new(),
            orientation_schema: None,
            extraction: Default::default(),
            parallel_context_template: String::new(),
            variables: Vec::new(),
        };
        // Re-running 'a' means 's' (which consumes it) must re-run; 'b' need not.
        let seeds: std::collections::HashSet<String> = ["a".to_string()].into_iter().collect();
        let deps = dependents_of(&config, &seeds);
        assert!(deps.contains("s"));
        assert!(!deps.contains("b"));
        assert!(!deps.contains("a")); // seeds excluded
    }

    // ── substitution ───────────────────────────────────────────────

    #[test]
    fn substitute_replaces_known_and_blanks_unknown() {
        let mut vars = std::collections::HashMap::new();
        vars.insert("journal".to_string(), "AER".to_string());
        let out = substitute_placeholders(
            "Review for {var:journal}; persona {var:persona}.",
            "{var:",
            &vars,
        )
        .unwrap();
        assert_eq!(out, "Review for AER; persona .");
    }

    #[test]
    fn substitute_no_placeholders_is_unchanged() {
        let vars = std::collections::HashMap::new();
        assert_eq!(
            substitute_placeholders("plain prompt", "{var:", &vars).unwrap(),
            "plain prompt"
        );
    }

    #[test]
    fn substitute_unclosed_brace_passes_through() {
        let vars = std::collections::HashMap::new();
        assert_eq!(
            substitute_placeholders("oops {var:x", "{var:", &vars).unwrap(),
            "oops {var:x"
        );
    }

    #[test]
    fn substitute_run_context_applies_vars_and_inputs() {
        let mut vars = std::collections::HashMap::new();
        vars.insert("journal".to_string(), "QJE".to_string());
        let mut inputs = std::collections::HashMap::new();
        inputs.insert("letter".to_string(), "/tmp/letter.txt".to_string());
        let out = substitute_run_context(
            "Journal {var:journal}; read the response at {input:letter}.",
            &vars,
            &inputs,
        )
        .unwrap();
        assert_eq!(out, "Journal QJE; read the response at /tmp/letter.txt.");
    }

    // ── build_parallel_prompt ──────────────────────────────────────

    #[test]
    fn build_parallel_prompt_pdf() {
        let step = make_step("test", Phase::Parallel);
        let template = "{paper_type}\n{orientation}\n{step_prompt}\n{paper_path}\n{figure_hint}";
        let result = build_parallel_prompt(
            &step,
            "empirical",
            "/tmp/orient.json",
            "Read it for the paper's structure.",
            "/tmp/paper.txt",
            "",
            "/tmp/paper.pdf",
            template,
            "",
            None,
        )
        .unwrap();
        assert!(result.contains("empirical"));
        assert!(result.contains("/tmp/orient.json"));
        assert!(result.contains("Read it for the paper's structure."));
        assert!(result.contains("/tmp/paper.txt"));
        assert!(result.contains("original PDF"));
    }

    #[test]
    fn build_parallel_prompt_recognizes_uppercase_pdf_extension() {
        let step = make_step("test", Phase::Parallel);
        let result = build_parallel_prompt(
            &step,
            "",
            "",
            "",
            "/tmp/paper.txt",
            "",
            "C:\\Papers\\DRAFT.PDF",
            "{figure_hint}",
            "",
            None,
        )
        .unwrap();
        assert!(result.contains("original PDF"));
    }

    #[test]
    fn build_parallel_prompt_exposes_bundle_and_asset_root() {
        let step = make_step("test", Phase::Parallel);
        let result = build_parallel_prompt(
            &step,
            "empirical",
            "",
            "",
            "/tmp/document.md",
            "/tmp/document_bundle.json",
            "/tmp/paper.pdf",
            "{document_bundle}\n{figure_hint}",
            "",
            Some("/runs/r1/artifacts"),
        )
        .unwrap();
        assert!(result.contains("/tmp/document_bundle.json"));
        assert!(result.contains("/runs/r1"));
        assert!(result.contains("ReadDocumentAsset"));
    }

    #[test]
    fn input_path_alias_substitutes_like_paper_path() {
        let step = make_step("test", Phase::Parallel);
        let template = "old={paper_path} new={input_path}";
        let result = build_parallel_prompt(
            &step,
            "empirical",
            "",
            "",
            "/tmp/paper.txt",
            "",
            "/tmp/p.pdf",
            template,
            "",
            None,
        )
        .unwrap();
        assert!(result.contains("old=/tmp/paper.txt"));
        assert!(result.contains("new=/tmp/paper.txt"));
    }

    #[test]
    fn build_parallel_prompt_latex() {
        let step = make_step("test", Phase::Parallel);
        let template = "{figure_hint}";
        let result = build_parallel_prompt(
            &step,
            "theory",
            "",
            "",
            "/tmp/paper.txt",
            "",
            "/home/user/papers/main.tex",
            template,
            "",
            None,
        )
        .unwrap();
        assert!(result.contains("selected source context"));
        assert!(result.contains("bounded local dependencies"));
    }

    #[test]
    fn build_parallel_prompt_empty_orientation() {
        let step = make_step("test", Phase::Parallel);
        let template = "[{orientation}]";
        let result = build_parallel_prompt(
            &step,
            "mixed",
            "",
            "unused hint",
            "/tmp/paper.txt",
            "",
            "/tmp/paper.pdf",
            template,
            "",
            None,
        )
        .unwrap();
        assert_eq!(result, "[]");
    }

    #[test]
    fn build_parallel_prompt_output_format_substitution() {
        let step = make_step("test", Phase::Parallel);
        let template = "{step_prompt}\n{output_format}";
        let block = output_format_block(Some("/runs/r1/artifacts"), "testnonce");
        let result = build_parallel_prompt(
            &step,
            "",
            "",
            "",
            "/tmp/p.txt",
            "",
            "/tmp/p.pdf",
            template,
            &block,
            None,
        )
        .unwrap();
        assert!(result.contains("/runs/r1/artifacts/files/"));
        assert!(result.contains("PIPELINE REPORT testnonce START"));
        // Old templates without the placeholder receive the current contract.
        let old = "{step_prompt}\nREPORT START markers here";
        let result = build_parallel_prompt(
            &step,
            "",
            "",
            "",
            "/tmp/p.txt",
            "",
            "/tmp/p.pdf",
            old,
            &block,
            None,
        )
        .unwrap();
        assert!(!result.contains("{output_format}"));
        assert!(result.contains("REPORT START markers here"));
        assert!(result.contains("PIPELINE REPORT testnonce START"));
    }

    // ── write handoff helpers ──────────────────────────────────────

    #[test]
    fn step_file_keys_are_deterministic_and_collision_resistant() {
        assert_eq!(step_slug("technical"), step_slug("technical"));
        assert!(step_slug("technical").starts_with("technical--"));
        assert_ne!(step_slug("a.b"), step_slug("a_b"));
        assert_ne!(step_slug("technical"), step_slug("technical/claude"));
    }

    #[test]
    fn output_format_block_modes() {
        let write = output_format_block(Some("/runs/x/artifacts"), "nonce123");
        assert!(write.contains("/runs/x/artifacts/files/"));
        assert!(write.contains("PIPELINE REPORT nonce123 START"));
        assert!(write.contains("Do not write the report itself to a file"));
        let markers = output_format_block(None, "nonce456");
        assert!(markers.contains("PIPELINE REPORT nonce456 START"));
        assert!(!markers.contains("/runs/x/artifacts"));
    }

    #[test]
    fn shared_context_note_names_only_selected_material() {
        let prompt = append_shared_context_note(
            "Read the input at /tmp/paper.txt.".to_string(),
            true,
            false,
        )
        .unwrap();
        assert!(prompt.contains("selected extracted input text"));
        assert!(!prompt.contains("survey"));
        assert!(prompt.contains("other artifacts listed"));
    }

    #[test]
    fn evidence_retrieval_guidance_is_capability_aware_and_quality_preserving() {
        let text_only =
            append_evidence_retrieval_guidance("Task".into(), &["Read".to_string()]).unwrap();
        assert!(text_only.contains("ReadTextBatch"));
        assert!(!text_only.contains("ReadDocumentAssetsBatch"));
        assert!(!text_only.contains("web queries"));
        assert!(text_only.contains("continue sequentially"));
        assert!(text_only.contains("never omit evidence"));

        let all = append_evidence_retrieval_guidance(
            "Task".into(),
            &[
                "Read".to_string(),
                "ReadDocumentAsset".to_string(),
                "WebSearch".to_string(),
            ],
        )
        .unwrap();
        assert!(all.contains("ReadTextBatch"));
        assert!(all.contains("ReadDocumentAssetsBatch"));
        assert!(all.contains("complete set of independent web queries"));
        assert!(all.contains("Never substitute extracted text"));

        assert_eq!(
            append_evidence_retrieval_guidance("Task".into(), &[]).unwrap(),
            "Task"
        );
    }

    #[test]
    fn ingest_report_file_reads_and_removes() {
        let dir = tempfile::tempdir().unwrap();
        let wd = dir.path().to_string_lossy().to_string();

        // Absent file → None.
        assert!(ingest_report_file_blocking(Some(&wd), "steps/a.md").is_none());
        // Disabled write dir → None.
        assert!(ingest_report_file_blocking(None, "steps/a.md").is_none());

        // Present file → content, and the file is consumed.
        std::fs::create_dir_all(dir.path().join("steps")).unwrap();
        std::fs::write(dir.path().join("steps/a.md"), "# Report\nbody\n").unwrap();
        assert_eq!(
            ingest_report_file_blocking(Some(&wd), "steps/a.md").unwrap(),
            "# Report\nbody"
        );
        assert!(!dir.path().join("steps/a.md").exists());

        // Empty file → None (falls back to stdout).
        std::fs::write(dir.path().join("steps/b.md"), "  \n").unwrap();
        assert!(ingest_report_file_blocking(Some(&wd), "steps/b.md").is_none());
    }

    #[test]
    fn tools_with_write_appends_once() {
        let base = vec!["Read".to_string()];
        assert_eq!(
            tools_with_write(&base, true, true, Some("/d")),
            vec!["Read", "ReadDocumentAsset", "Write"]
        );
        assert_eq!(
            tools_with_write(&base, true, true, None),
            vec!["Read", "ReadDocumentAsset"]
        );
        let with = vec!["Read".to_string(), "Write".to_string()];
        assert_eq!(
            tools_with_write(&with, false, false, Some("/d")),
            vec!["Write"]
        );
    }

    // ── expand_template ────────────────────────────────────────────

    #[test]
    fn expand_template_basic() {
        let prior = vec![
            StepOutput {
                step_id: "s1".into(),
                step_label: "Step 1".into(),
                phase: "parallel".into(),
                agent: String::new(),
                raw_text: "output1".into(),
                ..Default::default()
            },
            StepOutput {
                step_id: "s2".into(),
                step_label: "Step 2".into(),
                phase: "parallel".into(),
                agent: String::new(),
                raw_text: "output2".into(),
                ..Default::default()
            },
        ];
        let template = "Prior:\n{prior_outputs}\n\nLast: {last_output}";
        let result = expand_template(
            template,
            "/orient.json",
            "Read it.",
            &prior,
            "/paper.txt",
            "",
            "/source.tex",
        )
        .unwrap();
        assert!(result.contains("## Step 1"));
        assert!(result.contains("output1"));
        assert!(result.contains("output2"));
        assert!(result.contains("Last: output2"));
    }

    #[test]
    fn expand_template_backward_compat_aliases() {
        let prior = vec![StepOutput {
            step_id: "s1".into(),
            step_label: "S1".into(),
            phase: "parallel".into(),
            agent: String::new(),
            raw_text: "text".into(),
            ..Default::default()
        }];
        let template = "{referee_reports} | {editor_synthesis}";
        let result =
            expand_template(template, "", "", &prior, "/paper.txt", "", "/source.tex").unwrap();
        assert!(result.contains("## S1"));
        assert!(result.contains("text | text"));
    }

    #[test]
    fn aggregate_placeholders_omit_skipped_reports_but_named_refs_keep_them() {
        let prior = vec![
            StepOutput {
                step_id: "selected".into(),
                step_label: "Selected".into(),
                raw_text: "useful finding".into(),
                ..Default::default()
            },
            StepOutput {
                step_id: "not_selected".into(),
                step_label: "Not Selected".into(),
                raw_text: "_(skipped: run_if condition not met)_".into(),
                skipped: true,
                ..Default::default()
            },
        ];
        let result = expand_template(
            "{prior_outputs}\nLAST={last_output}\nEXPLICIT={step:not_selected}",
            "",
            "",
            &prior,
            "",
            "",
            "",
        )
        .unwrap();
        assert!(result.contains("## Selected\n\nuseful finding"));
        assert!(!result.contains("## Not Selected"));
        assert!(result.contains("LAST=useful finding"));
        assert!(result.contains("EXPLICIT=_(skipped: run_if condition not met)_"));
    }

    #[test]
    fn expand_template_no_prior() {
        let template = "Last: {last_output}";
        let result =
            expand_template(template, "", "", &[], "/paper.txt", "", "/source.tex").unwrap();
        assert!(result.contains("(not yet generated)"));
    }

    #[test]
    fn inserted_step_outputs_are_not_rescanned_for_placeholders() {
        // Model output may quote placeholder-shaped text from the reviewed
        // document; it must reach the prompt verbatim, not be re-expanded.
        let prior = vec![StepOutput {
            step_id: "analysis".into(),
            step_label: "Analysis".into(),
            raw_text: "quotes {step:analysis}, {paper_path}, and {last_output}".into(),
            ..Default::default()
        }];
        let result = expand_template(
            "{prior_outputs}\nPATH={paper_path}\nREF={step:analysis}",
            "",
            "",
            &prior,
            "/paper.txt",
            "",
            "",
        )
        .unwrap();
        assert_eq!(
            result
                .matches("quotes {step:analysis}, {paper_path}, and {last_output}")
                .count(),
            2, // once via {prior_outputs}, once via the explicit {step:analysis}
        );
        assert!(result.contains("PATH=/paper.txt"));
    }

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

    #[test]
    fn step_ref_exact_match() {
        let prior = vec![
            out("technical", "Technical", "tech body"),
            out("empirical", "Empirical", "emp body"),
        ];
        let result =
            expand_template("Tech: {step:technical}", "", "", &prior, "p", "", "s").unwrap();
        assert!(result.contains("Tech: tech body"));
        assert!(!result.contains("emp body"));
    }

    #[test]
    fn step_ref_multi_agent_base_id_joins() {
        let prior = vec![
            out("technical/claude", "Technical (Claude)", "claude says"),
            out(
                "technical/antigravity",
                "Technical (Antigravity)",
                "antigravity says",
            ),
        ];
        let result =
            expand_template("All: {step:technical}", "", "", &prior, "p", "", "s").unwrap();
        assert!(result.contains("claude says"));
        assert!(result.contains("antigravity says"));
        assert!(result.contains("---"));
    }

    #[test]
    fn step_ref_multi_agent_specific_id() {
        let prior = vec![
            out("technical/claude", "Technical (Claude)", "claude says"),
            out(
                "technical/antigravity",
                "Technical (Antigravity)",
                "antigravity says",
            ),
        ];
        let result = expand_template(
            "Just one: {step:technical/claude}",
            "",
            "",
            &prior,
            "p",
            "",
            "s",
        )
        .unwrap();
        assert!(result.contains("claude says"));
        assert!(!result.contains("antigravity says"));
    }

    #[test]
    fn step_ref_unknown_id_emits_notice() {
        let prior = vec![out("technical", "Technical", "tech body")];
        let result =
            expand_template("Missing: {step:nonexistent}", "", "", &prior, "p", "", "s").unwrap();
        assert!(result.contains("(no output for step 'nonexistent')"));
    }

    #[test]
    fn step_ref_unclosed_brace_passes_through() {
        let prior = vec![out("a", "A", "aa")];
        let result = expand_template("Broken: {step:a", "", "", &prior, "p", "", "s").unwrap();
        assert!(result.contains("{step:a"));
    }

    #[test]
    fn step_ref_empty_id() {
        let prior = vec![out("a", "A", "aa")];
        let result = expand_template("{step:}", "", "", &prior, "p", "", "s").unwrap();
        assert!(result.contains("(empty step reference)"));
    }
}
