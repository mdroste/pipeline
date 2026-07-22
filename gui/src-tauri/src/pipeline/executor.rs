//! Unified pipeline executor.
//!
//! Walks the step list top-to-bottom, grouping adjacent Parallel steps
//! into waves. Sequential steps run one at a time after all prior steps.
//! Merge auto-triggers between a parallel wave and the next step when
//! any parallel step used multiple agents.

use super::claude::{call_llm, cli_parent_dir, normalize_cli_root, LlmOverrides};
use super::merge;
use crate::models::{StepFailure, StepOutput};
use crate::output::{capitalize, strip_to_report};
use crate::pipeline_config::{Phase, PipelineConfig, StepConfig};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

/// Result of pipeline execution: successful outputs and any step failures.
pub struct ExecutionResult {
    pub outputs: Vec<StepOutput>,
    pub failed_steps: Vec<StepFailure>,
}

type ParallelTaskResult = Result<((usize, String), StepOutput), StepFailure>;

/// Build the complete read-root set used by CLI providers for every step in a
/// run. The source tree, private run-temp files (paper, orientation, named
/// inputs), and any original named-input roots are all explicit. Provider
/// request planning later canonicalizes/deduplicates ancestors and keeps the
/// artifact directory separate as the writable cwd.
fn provider_read_dirs(
    source_path: &str,
    paper_text_path: &str,
    orientation_path: &str,
    extra_inputs: &std::collections::HashMap<String, String>,
    run_read_dirs: &[String],
) -> Vec<String> {
    let mut dirs: Vec<String> = run_read_dirs
        .iter()
        .filter_map(|path| normalize_cli_root(path))
        .collect();

    let source = std::path::Path::new(source_path);
    if source.is_dir() {
        if let Some(root) = normalize_cli_root(source_path) {
            dirs.push(root);
        }
    } else if let Some(root) = cli_parent_dir(source_path) {
        dirs.push(root);
    }

    for path in std::iter::once(paper_text_path)
        .chain((!orientation_path.is_empty()).then_some(orientation_path))
        .chain(extra_inputs.values().map(String::as_str))
    {
        if let Some(root) = cli_parent_dir(path) {
            dirs.push(root);
        }
    }

    dirs.sort();
    dirs.dedup();
    dirs
}

/// Execute all enabled steps in the pipeline.
///
/// Steps run on a dependency schedule: each step becomes "ready" once its
/// upstream steps have completed, and all ready parallel steps run as one wave
/// under the `max_workers` semaphore. With no explicit `inputs`, the implicit
/// adjacency dependencies reproduce the original wave behaviour exactly
/// (parallel steps run together; a sequential step waits for everything before
/// it). A step's `run_if` guard can skip it; a skipped step still "completes"
/// so its dependents proceed. A failed sequential step stops further execution
/// but does not discard prior outputs.
#[allow(clippy::too_many_arguments)]
pub async fn execute_steps(
    app: &crate::emit::EventBus,
    config: &PipelineConfig,
    orientation_path: &str,
    orientation_value: &serde_json::Value,
    paper_text_path: &str,
    source_path: &str,
    paper_type: &str,
    survey_hint: &str,
    variables: &std::collections::HashMap<String, String>,
    extra_inputs: &std::collections::HashMap<String, String>,
    run_read_dirs: &[String],
    preloaded: &std::collections::HashMap<String, Vec<StepOutput>>,
    write_dir: Option<&str>,
    settings: &crate::settings::Settings,
) -> Result<ExecutionResult, String> {
    let semaphore = Arc::new(Semaphore::new(settings.max_workers.max(1) as usize));
    let mut all_outputs: Vec<StepOutput> = Vec::new();
    let mut failed_steps: Vec<StepFailure> = Vec::new();
    let read_dirs = provider_read_dirs(
        source_path,
        paper_text_path,
        orientation_path,
        extra_inputs,
        run_read_dirs,
    );

    let enabled: Vec<&StepConfig> = config.steps.iter().filter(|s| s.enabled).collect();
    let deps = resolve_dependencies(&enabled);

    // Ids that have completed (including skipped) so dependents can start.
    let mut done: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut remaining: Vec<usize> = (0..enabled.len()).collect();

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

        let ready_parallel: Vec<usize> = ready
            .iter()
            .copied()
            .filter(|&i| enabled[i].phase == Phase::Parallel)
            .collect();

        if !ready_parallel.is_empty() {
            // Partition into steps whose run_if guard passes (dispatch) and
            // those it skips (record a placeholder so dependents proceed).
            let mut to_run: Vec<&StepConfig> = Vec::new();
            for &i in &ready_parallel {
                let step = enabled[i];
                // Resume: a preloaded step reuses the parent run's output.
                if let Some(cached) = preloaded.get(&step.id) {
                    let _ = app.emit_event(
                        "pipeline:pass",
                        serde_json::json!({"name": step.id, "status": "done"}),
                    );
                    all_outputs.extend(cached.iter().cloned());
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
                app.emit_event(
                    "pipeline:stage",
                    serde_json::json!({"stage": "dispatching"}),
                )
                .ok();
                let (mut wave_outputs, wave_failures) = run_parallel_wave(
                    app,
                    &to_run,
                    settings,
                    &semaphore,
                    orientation_path,
                    paper_text_path,
                    source_path,
                    paper_type,
                    survey_hint,
                    &config.parallel_context_template,
                    variables,
                    extra_inputs,
                    &read_dirs,
                    write_dir,
                )
                .await?;
                // Every dispatched step is terminal once its wave returns. A
                // failed unit and a fan-out with zero matching units both lack
                // a StepOutput, so deriving completion from outputs alone
                // leaves their dependents permanently blocked.
                mark_steps_done(&mut done, &to_run);
                failed_steps.extend(wave_failures);

                let has_multi_agent = wave_outputs.iter().any(|o| o.step_id.contains('/'));
                if has_multi_agent && config.merge.enabled {
                    app.emit_event("pipeline:stage", serde_json::json!({"stage": "merging"}))
                        .ok();
                    match merge::merge_step_outputs(
                        app,
                        wave_outputs.clone(),
                        &config.merge,
                        &semaphore,
                        settings,
                    )
                    .await
                    {
                        Ok(merged) => wave_outputs = merged,
                        Err(e) if is_cancellation_error(&e) => return Err(e),
                        Err(e) => {
                            let _ = app.emit_event(
                                "pipeline:log",
                                serde_json::json!({ "line": format!("WARNING: merge failed: {e}. Using unmerged outputs.") }),
                            );
                        }
                    }
                }
                all_outputs.extend(wave_outputs);
            }
            continue;
        }

        // No parallel steps ready — run one sequential step (lowest index).
        let i = *ready.iter().min().unwrap();
        let step = enabled[i];
        remaining.retain(|&j| j != i);

        // Resume: a preloaded step reuses the parent run's output.
        if let Some(cached) = preloaded.get(&step.id) {
            let _ = app.emit_event(
                "pipeline:pass",
                serde_json::json!({"name": step.id, "status": "done"}),
            );
            done.insert(step.id.clone());
            all_outputs.extend(cached.iter().cloned());
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

        app.emit_event(
            "pipeline:stage",
            serde_json::json!({"stage": "synthesizing"}),
        )
        .ok();
        // A step with explicit inputs sees only those upstream outputs; the
        // implicit-schedule case (empty inputs) sees everything prior, as before.
        let prior: Vec<StepOutput> = if step.inputs.is_empty() {
            all_outputs.clone()
        } else {
            all_outputs
                .iter()
                .filter(|o| deps[i].contains(base_id(&o.step_id)))
                .cloned()
                .collect()
        };
        match run_sequential_step(
            app,
            step,
            &prior,
            orientation_path,
            paper_text_path,
            source_path,
            survey_hint,
            variables,
            extra_inputs,
            &read_dirs,
            write_dir,
            settings,
        )
        .await
        {
            Ok(output) => {
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
                failed_steps.push(StepFailure {
                    step_id: step.id.clone(),
                    step_label: step.label.clone(),
                    error: e,
                });
                break;
            }
        }
    }

    // A run whose only outputs are skips produced no report content.
    if all_outputs.iter().all(|o| o.skipped) {
        return Err("No steps produced output. Enable at least one step (or check that run_if conditions can pass).".into());
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

/// Compute each enabled step's dependency set (of enabled step ids). Explicit
/// `inputs` win; otherwise the implicit adjacency schedule is used: a parallel
/// step waits for the most recent sequential step, a sequential step waits for
/// every step before it. Dependency ids that aren't enabled steps are dropped
/// (a disabled upstream can't be waited on).
fn resolve_dependencies(enabled: &[&StepConfig]) -> Vec<std::collections::HashSet<String>> {
    let enabled_ids: std::collections::HashSet<&str> =
        enabled.iter().map(|s| s.id.as_str()).collect();
    let mut deps = Vec::with_capacity(enabled.len());
    let mut all_prior: Vec<String> = Vec::new();
    let mut last_sequential: Option<String> = None;
    for s in enabled {
        let d: std::collections::HashSet<String> = if !s.inputs.is_empty() {
            s.inputs
                .iter()
                .filter(|id| enabled_ids.contains(id.as_str()) && id.as_str() != s.id)
                .cloned()
                .collect()
        } else {
            match s.phase {
                Phase::Parallel => last_sequential.iter().cloned().collect(),
                Phase::Sequential => all_prior.iter().cloned().collect(),
            }
        };
        deps.push(d);
        all_prior.push(s.id.clone());
        if s.phase == Phase::Sequential {
            last_sequential = Some(s.id.clone());
        }
    }
    deps
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
    format!("{:x}", Sha256::digest(step_key.as_bytes()))
}

/// Build the OUTPUT FORMAT block appended to every step prompt.
///
/// Write mode: the model saves its report into the run's artifact directory
/// (each provider confines writes to it — our own Write tool on the direct
/// API paths, permission rules for `claude -p`, the OS sandbox for codex,
/// the workspace boundary for gemini). A stdout-marker escape hatch remains
/// for models that cannot write files; the executor accepts either.
/// Read-only mode (no run directory): markers on stdout, as before.
fn output_format_block(write_dir: Option<&str>, report_rel: &str) -> String {
    match write_dir {
        Some(dir) => format!(
            "OUTPUT FORMAT:\n\
             Write your complete markdown report to this file (create it with your file-writing tool):\n\
             {dir}/{report_rel}\n\
             Supporting files (data tables, extracted figures) may be saved under {dir}/files/ and referenced from the report by relative path.\n\
             Do not print the report to stdout — after writing the file, reply with one line confirming it was written.\n\
             Only if you have no file-writing tool available: print the report to stdout between `<!-- REPORT START -->` and `<!-- REPORT END -->` markers instead."
        ),
        None => "OUTPUT FORMAT:\n\
             Begin your report with exactly `<!-- REPORT START -->` and end with exactly `<!-- REPORT END -->`.\n\
             Include ONLY your markdown report between those markers — no preamble, no commentary, no acknowledgments outside them."
            .to_string(),
    }
}

/// Read (and remove) a model-written report file. Returns `None` when the
/// file is absent or empty — callers then fall back to stdout output. The
/// file is removed because the canonical copy (with the step header) is
/// written into the run artifacts when the run finishes; leaving it would
/// duplicate every report in the artifact explorer.
fn ingest_report_file(write_dir: Option<&str>, report_rel: &str) -> Option<String> {
    let dir = write_dir?;
    let path = std::path::Path::new(dir).join(report_rel);
    let content = std::fs::read_to_string(&path).ok()?;
    if content.trim().is_empty() {
        return None;
    }
    let _ = std::fs::remove_file(&path);
    Some(content.trim().to_string())
}

/// Step tool list, extended with Write when this run supports file handoff.
fn tools_with_write(step_tools: &[String], write_dir: Option<&str>) -> Vec<String> {
    let mut tools = step_tools.to_vec();
    if write_dir.is_some() && !tools.iter().any(|t| t == "Write") {
        tools.push("Write".to_string());
    }
    tools
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
    source_path: &str,
    template: &str,
    output_format: &str,
) -> String {
    let normalized_path = paper_text_path.replace('\\', "/");
    let normalized_source = source_path.replace('\\', "/");

    let is_pdf = normalized_source.ends_with(".pdf");
    let figure_hint = if is_pdf {
        format!(
            "The original PDF is at: {normalized_source}\n\
             When the orientation map lists a figure or table with a page number, you can read that page of the PDF to inspect the visual content."
        )
    } else {
        let source_dir = std::path::Path::new(&normalized_source)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| normalized_source.clone());
        format!(
            "The LaTeX source directory is: {source_dir}\n\
             Figure files (PNG, PDF, etc.) referenced by \\includegraphics are in this directory or its subdirectories. You can read them to inspect visual content."
        )
    };

    let orientation_block = if orientation_path.is_empty() {
        String::new()
    } else {
        let normalized_orient = orientation_path.replace('\\', "/");
        format!("The orientation map (JSON) is at: {normalized_orient}\n{survey_hint}")
    };

    let expanded = template
        .replace("{step_prompt}", &step.prompt)
        .replace("{paper_type}", paper_type)
        .replace("{orientation}", &orientation_block)
        .replace("{paper_path}", &normalized_path)
        .replace("{input_path}", &normalized_path) // vocabulary-neutral alias
        .replace("{figure_hint}", &figure_hint);

    // Templates from before the file-handoff change carry a hardcoded
    // marker instruction instead of the placeholder; they keep working
    // through the stdout fallback.
    if expanded.contains("{output_format}") {
        expanded.replace("{output_format}", output_format)
    } else {
        expanded
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

/// Build the units to run for a step: one per matching file (fan-out) or one
/// per agent (normal). Fan-out with no matches returns empty (a warning is
/// logged) so the step is simply skipped.
fn build_units(
    step: &StepConfig,
    settings: &crate::settings::Settings,
    source_path: &str,
    app: &crate::emit::EventBus,
) -> Vec<Unit> {
    let provider = step
        .agents
        .first()
        .cloned()
        .unwrap_or_else(|| settings.preferred_provider.clone());

    if let Some(fe) = &step.for_each {
        let root = fan_out_root(source_path);
        let (items, truncated) =
            crate::pipeline::glob::expand(&root, &fe.glob, fe.max.max(1) as usize);
        if items.is_empty() {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!(
                    "WARNING: fan-out step '{}' matched no files for glob '{}'", step.label, fe.glob
                )}),
            );
            return Vec::new();
        }
        if truncated {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!(
                    "Fan-out step '{}' capped at {} files (glob '{}')", step.label, fe.max, fe.glob
                )}),
            );
        }
        let mut used = std::collections::HashSet::new();
        items
            .into_iter()
            .map(|path| {
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
                Unit {
                    agent: provider.clone(),
                    item: Some(path),
                    suffix,
                    display: base,
                }
            })
            .collect()
    } else {
        let agents: Vec<String> = if step.agents.is_empty() {
            vec![provider]
        } else {
            step.agents.clone()
        };
        let multi = agents.len() > 1;
        agents
            .into_iter()
            .map(|a| Unit {
                suffix: if multi { a.clone() } else { String::new() },
                display: capitalize(&a),
                agent: a,
                item: None,
            })
            .collect()
    }
}

/// Run all parallel steps in a wave concurrently.
#[allow(clippy::too_many_arguments)]
async fn run_parallel_wave(
    app: &crate::emit::EventBus,
    steps: &[&StepConfig],
    settings: &crate::settings::Settings,
    semaphore: &Arc<Semaphore>,
    orientation_path: &str,
    paper_text_path: &str,
    source_path: &str,
    paper_type: &str,
    survey_hint: &str,
    context_template: &str,
    variables: &std::collections::HashMap<String, String>,
    extra_inputs: &std::collections::HashMap<String, String>,
    read_dirs: &[String],
    write_dir: Option<&str>,
) -> Result<(Vec<StepOutput>, Vec<StepFailure>), String> {
    let source = std::path::Path::new(source_path);
    let source_dir = if source.is_dir() {
        normalize_cli_root(source_path)
    } else {
        cli_parent_dir(source_path)
    };

    let mut tasks: JoinSet<ParallelTaskResult> = JoinSet::new();

    for (idx, step) in steps.iter().enumerate() {
        let units = build_units(step, settings, source_path, app);

        for unit in &units {
            let id = step.id.clone();
            let label = step.label.clone();
            let agent_name = unit.agent.clone();
            let tools = tools_with_write(&step.tools, write_dir);
            let model_selection = step.model_selection_for(settings, &agent_name);
            let effort_override = step.effort_for(settings, &agent_name);
            let output_schema = step.output_schema.clone();

            // Composite key when this is one of several units (multi-agent or
            // fan-out); the bare step id when it's a single plain run.
            let step_key = if unit.suffix.is_empty() {
                id.clone()
            } else {
                format!("{}/{}", id, unit.suffix)
            };

            let report_rel = format!("steps/{}.md", step_slug(&step_key));
            let output_format = output_format_block(write_dir, &report_rel);
            let prompt = build_parallel_prompt(
                step,
                paper_type,
                orientation_path,
                survey_hint,
                paper_text_path,
                source_path,
                context_template,
                &output_format,
            );
            let prompt = substitute_run_context(&prompt, variables, extra_inputs);
            // Fan-out: bind {item} to this unit's file (empty otherwise).
            let prompt = prompt.replace("{item}", unit.item.as_deref().unwrap_or(""));
            let task_write_dir = write_dir.map(|s| s.to_string());
            let task_read_dirs = read_dirs.to_vec();

            let _ = app.emit_event(
                "pipeline:pass",
                serde_json::json!({
                    "name": step_key,
                    "status": "running"
                }),
            );

            let app_handle = app.clone();
            let step_key_emit = step_key.clone();
            let log_label = if unit.suffix.is_empty() {
                format!("Step: {}", label)
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
            let task_cwd = source_dir.clone();
            // display_label is moved into the success StepOutput; keep a copy
            // for failure reporting.
            let fail_label = display_label.clone();
            let settings = settings.clone();

            tasks.spawn(async move {
                if let Some(error) = cancellation_error(&step_key_emit) {
                    return Err(StepFailure {
                        step_id: step_key_emit.clone(),
                        step_label: fail_label.clone(),
                        error,
                    });
                }
                let _permit = sem.acquire().await.map_err(|_| StepFailure {
                    step_id: step_key_emit.clone(),
                    step_label: fail_label.clone(),
                    error: "Semaphore closed".to_string(),
                })?;
                // Cancellation can happen while this unit is queued for the
                // worker permit. Re-check after acquisition before starting a
                // provider call (and therefore before incurring cost).
                if let Some(error) = cancellation_error(&step_key_emit) {
                    return Err(StepFailure {
                        step_id: step_key_emit.clone(),
                        step_label: fail_label.clone(),
                        error,
                    });
                }
                let tool_refs: Vec<&str> = tools.iter().map(|s| s.as_str()).collect();
                let timeout = settings.step_timeout_secs.max(60);
                let max_retries = settings.max_retries;
                let provider = agent_name.clone();
                let resolution = crate::model_catalog::resolve(
                    &provider,
                    &settings,
                    model_selection.as_ref(),
                )
                .await
                .map_err(|error| StepFailure {
                    step_id: step_key_emit.clone(),
                    step_label: fail_label.clone(),
                    error,
                })?;
                let effective_model = resolution.resolved_model.clone();
                let command_model = resolution.command_model.clone();

                let mut last_err = String::new();
                for attempt in 0..=max_retries {
                    if let Some(error) = cancellation_error(&step_key_emit) {
                        return Err(StepFailure {
                            step_id: step_key_emit.clone(),
                            step_label: fail_label.clone(),
                            error,
                        });
                    }
                    if attempt > 0 {
                        let _ = app_handle.emit_event(
                            "pipeline:log",
                            serde_json::json!({ "line": format!("{log_label}: retry {attempt}/{max_retries} after failure: {last_err}") }),
                        );
                        let _ = app_handle.emit_event(
                            "pipeline:pass",
                            serde_json::json!({
                                "name": step_key_emit,
                                "status": "running"
                            }),
                        );
                    }
                    let extra: Vec<&str> = task_read_dirs.iter().map(String::as_str).collect();
                    let mut overrides = LlmOverrides::from_step_strings(
                        command_model.as_deref().unwrap_or(""),
                        &effort_override,
                    );
                    overrides.model_resolved = true;
                    overrides.write_dir = task_write_dir.as_deref();
                    overrides.settings = Some(&settings);
                    let call_start = std::time::Instant::now();
                    let (call_result, usage) = crate::pipeline::logging::with_pass(
                        step_key_emit.clone(),
                        crate::pipeline::logging::measure_usage(call_llm(
                            &app_handle,
                            &prompt,
                            &tool_refs,
                            None,
                            "text",
                            timeout,
                            &log_label,
                            Some(&agent_name),
                            task_cwd.as_deref(),
                            &extra,
                            &overrides,
                        )),
                    )
                    .await;
                    let duration_secs = call_start.elapsed().as_secs();
                    // Per-pass cancel: stop retrying and fail this pass only.
                    if crate::commands::is_pass_cancelled(&step_key_emit) {
                        let _ = app_handle.emit_event(
                            "pipeline:pass",
                            serde_json::json!({ "name": step_key_emit, "status": "error" }),
                        );
                        return Err(StepFailure {
                            step_id: step_key.clone(),
                            step_label: fail_label.clone(),
                            error: "Cancelled by user".to_string(),
                        });
                    }
                    match call_result {
                        Ok(raw_text) => {
                            let text = match ingest_report_file(task_write_dir.as_deref(), &report_rel) {
                                Some(file_text) => file_text,
                                None => strip_to_report(&raw_text),
                            };
                            // Structured output is a contract, not a hint. A
                            // malformed final attempt must fail the step.
                            if let Some(schema) = &output_schema {
                                if let Err(why) = crate::pipeline::structured::check(schema, &text) {
                                    if attempt < max_retries {
                                        last_err = format!("output did not satisfy schema: {why}");
                                        continue;
                                    }
                                    last_err = format!(
                                        "output did not satisfy schema after {max_retries} retries: {why}"
                                    );
                                    continue;
                                }
                            }
                            let _ = app_handle.emit_event(
                                "pipeline:pass",
                                serde_json::json!({
                                    "name": step_key_emit,
                                    "status": "done"
                                }),
                            );
                            return Ok((
                                sort_key,
                                StepOutput {
                                    step_id: step_key,
                                    step_label: display_label,
                                    phase: "parallel".to_string(),
                                    provider,
                                    agent: agent_name,
                                    raw_text: text,
                                    duration_secs,
                                    input_tokens: usage.input_tokens,
                                    output_tokens: usage.output_tokens,
                                    model: effective_model,
                                    model_transport: resolution.transport,
                                    model_policy: resolution.selection.label(),
                                    model_source: resolution.source,
                                    model_catalog_updated_at: resolution.catalog_updated_at,
                                    ..Default::default()
                                },
                            ));
                        }
                        Err(e) => {
                            if e.contains("cancelled") {
                                let _ = app_handle.emit_event(
                                    "pipeline:pass",
                                    serde_json::json!({
                                        "name": step_key_emit,
                                        "status": "error"
                                    }),
                                );
                                return Err(StepFailure {
                                    step_id: step_key.clone(),
                                    step_label: fail_label.clone(),
                                    error: e,
                                });
                            }
                            // The call may have failed after the report was
                            // written (e.g. the empty-stdout quirk when the
                            // model obeyed "don't print the report"). A
                            // non-empty report file counts as success.
                            if let Some(file_text) = ingest_report_file(task_write_dir.as_deref(), &report_rel) {
                                if let Some(schema) = &output_schema {
                                    if let Err(why) = crate::pipeline::structured::check(schema, &file_text) {
                                        last_err = format!("report file did not satisfy schema: {why}");
                                        continue;
                                    }
                                }
                                let _ = app_handle.emit_event(
                                    "pipeline:log",
                                    serde_json::json!({ "line": format!("{log_label}: call reported an error but the report file was written; using it. ({e})") }),
                                );
                                let _ = app_handle.emit_event(
                                    "pipeline:pass",
                                    serde_json::json!({
                                        "name": step_key_emit,
                                        "status": "done"
                                    }),
                                );
                                return Ok((
                                    sort_key,
                                    StepOutput {
                                        step_id: step_key,
                                        step_label: display_label,
                                        phase: "parallel".to_string(),
                                        provider,
                                        agent: agent_name,
                                        raw_text: file_text,
                                        duration_secs,
                                        input_tokens: usage.input_tokens,
                                        output_tokens: usage.output_tokens,
                                        model: effective_model,
                                        ..Default::default()
                                    },
                                ));
                            }
                            last_err = e;
                        }
                    }
                }

                let _ = app_handle.emit_event(
                    "pipeline:pass",
                    serde_json::json!({
                        "name": step_key_emit,
                        "status": "error"
                    }),
                );
                Err(StepFailure {
                    step_id: step_key,
                    step_label: fail_label,
                    error: last_err,
                })
            });
        }
    }

    let mut results = Vec::new();
    let mut failures: Vec<StepFailure> = Vec::new();

    while let Some(res) = tasks.join_next().await {
        match res {
            Ok(Ok(report)) => results.push(report),
            Ok(Err(failure)) => failures.push(failure),
            Err(e) => failures.push(StepFailure {
                step_id: "internal".to_string(),
                step_label: "Internal task".to_string(),
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
        if results.is_empty() {
            let summary: Vec<String> = failures
                .iter()
                .map(|f| format!("{}: {}", f.step_label, f.error))
                .collect();
            return Err(format!("All steps failed: {}", summary.join("; ")));
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
    source_path: &str,
) -> String {
    let orientation_ref = if orientation_path.is_empty() {
        String::new()
    } else {
        let normalized_orient = orientation_path.replace('\\', "/");
        format!("The orientation map (JSON) is at: {normalized_orient}\n{survey_hint}")
    };

    let prior_text: String = prior_outputs
        .iter()
        .map(|o| format!("## {}\n\n{}", o.step_label, o.raw_text))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    let last_output_text = prior_outputs
        .last()
        .map(|o| o.raw_text.as_str())
        .unwrap_or("(not yet generated)");

    let mut expanded = template
        .replace("{orientation}", &orientation_ref)
        .replace("{prior_outputs}", &prior_text)
        .replace("{referee_reports}", &prior_text) // backward-compatible alias
        .replace("{last_output}", last_output_text)
        .replace("{editor_synthesis}", last_output_text) // backward-compatible alias
        .replace("{paper_path}", paper_text_path)
        .replace("{input_path}", paper_text_path) // vocabulary-neutral alias
        .replace("{source_path}", source_path);

    expanded = substitute_named_step_refs(&expanded, prior_outputs);
    expanded
}

/// Replace `{step:<id>}` placeholders with the matching prior step's raw_text.
/// Multi-agent runs produce composite IDs like "technical/claude"; both the base
/// id ("technical") and the full id ("technical/claude") are matchable. When a
/// base id has multiple agents, their outputs are joined with a separator.
/// Unknown ids are replaced with a parenthesized notice so the prompt remains
/// readable rather than leaking the literal `{step:foo}` to the LLM.
fn substitute_named_step_refs(template: &str, prior_outputs: &[StepOutput]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    let needle = "{step:";

    while let Some(start) = rest.find(needle) {
        out.push_str(&rest[..start]);
        let after_open = &rest[start + needle.len()..];
        let Some(end_rel) = after_open.find('}') else {
            // No closing brace — emit the rest verbatim.
            out.push_str(&rest[start..]);
            return out;
        };
        let id = after_open[..end_rel].trim();
        let resolved = resolve_step_ref(id, prior_outputs);
        out.push_str(&resolved);
        rest = &after_open[end_rel + 1..];
    }
    out.push_str(rest);
    out
}

fn resolve_step_ref(id: &str, prior_outputs: &[StepOutput]) -> String {
    if id.is_empty() {
        return "(empty step reference)".to_string();
    }
    // Exact id match wins (covers both "technical" and "technical/claude").
    if let Some(o) = prior_outputs.iter().find(|o| o.step_id == id) {
        return o.raw_text.clone();
    }
    // Otherwise, gather all step outputs whose base id (before any '/') matches.
    let matches: Vec<&StepOutput> = prior_outputs
        .iter()
        .filter(|o| o.step_id.split('/').next() == Some(id))
        .collect();
    if matches.is_empty() {
        return format!("(no output for step '{id}')");
    }
    if matches.len() == 1 {
        return matches[0].raw_text.clone();
    }
    matches
        .iter()
        .map(|o| format!("### {}\n\n{}", o.step_label, o.raw_text))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

/// Replace `{<prefix>key}` placeholders using `map`. Unknown keys become an
/// empty string. `prefix` includes the trailing colon, e.g. "{var:".
fn substitute_placeholders(
    text: &str,
    needle: &str,
    map: &std::collections::HashMap<String, String>,
) -> String {
    if !text.contains(needle) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(needle) {
        out.push_str(&rest[..start]);
        let after = &rest[start + needle.len()..];
        if let Some(end) = after.find('}') {
            let key = after[..end].trim();
            out.push_str(map.get(key).map(|s| s.as_str()).unwrap_or(""));
            rest = &after[end + 1..];
        } else {
            out.push_str(&rest[start..]);
            return out;
        }
    }
    out.push_str(rest);
    out
}

/// Apply both run-time substitutions to a prompt: `{var:key}` (values) and
/// `{input:key}` (paths to extra named inputs' extracted text).
fn substitute_run_context(
    text: &str,
    vars: &std::collections::HashMap<String, String>,
    inputs: &std::collections::HashMap<String, String>,
) -> String {
    let t = substitute_placeholders(text, "{var:", vars);
    substitute_placeholders(&t, "{input:", inputs)
}

/// Run a single sequential step.
#[allow(clippy::too_many_arguments)]
async fn run_sequential_step(
    app: &crate::emit::EventBus,
    step: &StepConfig,
    prior_outputs: &[StepOutput],
    orientation_path: &str,
    paper_text_path: &str,
    source_path: &str,
    survey_hint: &str,
    variables: &std::collections::HashMap<String, String>,
    extra_inputs: &std::collections::HashMap<String, String>,
    read_dirs: &[String],
    write_dir: Option<&str>,
    settings: &crate::settings::Settings,
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
        orientation_path,
        survey_hint,
        prior_outputs,
        paper_text_path,
        source_path,
    );

    let base_prompt = substitute_run_context(&base_prompt, variables, extra_inputs);
    let report_rel = format!("steps/{}.md", step_slug(&step.id));
    let prompt = format!(
        "{base_prompt}\n\n{}",
        output_format_block(write_dir, &report_rel)
    );

    let tools = tools_with_write(&step.tools, write_dir);
    let tool_refs: Vec<&str> = tools.iter().map(|s| s.as_str()).collect();
    let agent = step.agents.first().map(|s| s.as_str());
    let log_label = format!("Step: {}", step.label);

    // Use the paper's parent directory as CWD for steps with Read access
    let source = std::path::Path::new(source_path);
    let source_dir = if source.is_dir() {
        normalize_cli_root(source_path)
    } else {
        cli_parent_dir(source_path)
    };
    let timeout = settings.step_timeout_secs.max(60);
    let max_retries = settings.max_retries;
    let provider = agent
        .map(|a| a.to_string())
        .unwrap_or_else(|| settings.preferred_provider.clone());
    let model_selection = step.model_selection_for(settings, &provider);
    let resolution =
        crate::model_catalog::resolve(&provider, settings, model_selection.as_ref()).await?;
    let effective_model = resolution.resolved_model.clone();
    let command_model = resolution.command_model.clone();
    let effort = step.effort_for(settings, &provider);

    let mut last_err = String::new();
    let mut final_text: Option<String> = None;
    let mut usage = crate::pipeline::logging::CallUsage::default();
    let mut duration_secs = 0u64;
    for attempt in 0..=max_retries {
        if let Some(error) = cancellation_error(&step.id) {
            return Err(error);
        }
        if attempt > 0 {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!("{log_label}: retry {attempt}/{max_retries} after failure: {last_err}") }),
            );
            let _ = app.emit_event(
                "pipeline:pass",
                serde_json::json!({
                    "name": step.id,
                    "status": "running"
                }),
            );
        }
        let extra: Vec<&str> = read_dirs.iter().map(String::as_str).collect();
        let mut overrides =
            LlmOverrides::from_step_strings(command_model.as_deref().unwrap_or(""), &effort);
        overrides.model_resolved = true;
        overrides.write_dir = write_dir;
        overrides.settings = Some(settings);
        let call_start = std::time::Instant::now();
        let (call_result, call_usage) = crate::pipeline::logging::with_pass(
            step.id.clone(),
            crate::pipeline::logging::measure_usage(call_llm(
                app,
                &prompt,
                &tool_refs,
                None,
                "text",
                timeout,
                &log_label,
                agent,
                source_dir.as_deref(),
                &extra,
                &overrides,
            )),
        )
        .await;
        if crate::commands::is_pass_cancelled(&step.id) {
            return Err("Cancelled by user".to_string());
        }
        // Resolve this attempt's text (from the report file or stdout), or None
        // if the call failed with nothing written.
        let candidate: Option<String> = match call_result {
            Ok(text) => Some(
                ingest_report_file(write_dir, &report_rel)
                    .unwrap_or_else(|| strip_to_report(&text)),
            ),
            Err(e) => {
                if e.contains("cancelled") {
                    return Err(e);
                }
                // A non-empty report file counts as success even when the
                // call errored (e.g. empty stdout after an obedient write).
                if let Some(t) = ingest_report_file(write_dir, &report_rel) {
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({ "line": format!("{log_label}: call reported an error but the report file was written; using it. ({e})") }),
                    );
                    Some(t)
                } else {
                    last_err = e;
                    None
                }
            }
        };
        if let Some(text) = candidate {
            // Structured output is a hard contract.
            if let Some(schema) = &step.output_schema {
                if let Err(why) = crate::pipeline::structured::check(schema, &text) {
                    if attempt < max_retries {
                        last_err = format!("output did not satisfy schema: {why}");
                        continue;
                    }
                    last_err =
                        format!("output did not satisfy schema after {max_retries} retries: {why}");
                    continue;
                }
            }
            final_text = Some(text);
            usage = call_usage;
            duration_secs = call_start.elapsed().as_secs();
            last_err.clear();
            break;
        }
    }

    let text = match final_text {
        Some(t) => t,
        None => {
            return Err(if last_err.is_empty() {
                "step produced no output".to_string()
            } else {
                last_err
            });
        }
    };

    Ok(StepOutput {
        step_id: step.id.clone(),
        step_label: step.label.clone(),
        phase: "sequential".to_string(),
        provider,
        agent: agent.unwrap_or("").to_string(),
        raw_text: text,
        duration_secs,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        model: effective_model,
        model_transport: resolution.transport,
        model_policy: resolution.selection.label(),
        model_source: resolution.source,
        model_catalog_updated_at: resolution.catalog_updated_at,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline_config::Phase;

    fn make_step(id: &str, phase: Phase) -> StepConfig {
        StepConfig {
            id: id.into(),
            label: id.into(),
            phase,
            ..Default::default()
        }
    }

    #[test]
    fn provider_roots_include_paper_orientation_named_input_and_source() {
        let inputs = std::collections::HashMap::from([(
            "response".to_string(),
            "/private/tmp/pipeline_run/named/response.txt".to_string(),
        )]);
        let roots = provider_read_dirs(
            "/Users/Mike/Documents/Paper/main.tex",
            "/private/tmp/pipeline_run/paper/paper.txt",
            "/private/tmp/pipeline_run/orientation/orientation.json",
            &inputs,
            &["/Users/Mike/Documents/Named Source".to_string()],
        );

        for expected in [
            "/Users/Mike/Documents/Paper",
            "/Users/Mike/Documents/Named Source",
            "/private/tmp/pipeline_run/paper",
            "/private/tmp/pipeline_run/orientation",
            "/private/tmp/pipeline_run/named",
        ] {
            assert!(
                roots.iter().any(|root| root == expected),
                "missing {expected}"
            );
        }
    }

    #[test]
    fn provider_roots_normalize_windows_path_shapes() {
        let inputs = std::collections::HashMap::from([(
            "rubric".to_string(),
            r"C:\Users\Mike\AppData\Local\Temp\pipeline_run\named\rubric.txt".to_string(),
        )]);
        let roots = provider_read_dirs(
            r"C:\Users\Mike\Documents\Paper\main.pdf",
            r"C:\Users\Mike\AppData\Local\Temp\pipeline_run\paper\paper.txt",
            r"C:\Users\Mike\AppData\Local\Temp\pipeline_run\orientation\orientation.json",
            &inputs,
            &[r"D:\Shared Inputs\Data".to_string()],
        );

        for expected in [
            "C:/Users/Mike/Documents/Paper",
            "C:/Users/Mike/AppData/Local/Temp/pipeline_run/paper",
            "C:/Users/Mike/AppData/Local/Temp/pipeline_run/orientation",
            "C:/Users/Mike/AppData/Local/Temp/pipeline_run/named",
            "D:/Shared Inputs/Data",
        ] {
            assert!(
                roots.iter().any(|root| root == expected),
                "missing {expected}"
            );
        }
    }

    #[test]
    fn folder_source_grants_the_folder_not_its_parent() {
        let folder = tempfile::tempdir().unwrap();
        let canonical = folder.path().canonicalize().unwrap();
        let roots = provider_read_dirs(
            folder.path().to_str().unwrap(),
            "/private/tmp/pipeline_run/paper.txt",
            "",
            &std::collections::HashMap::new(),
            &[],
        );
        let expected = canonical.to_string_lossy().replace('\\', "/");
        assert!(roots.iter().any(|root| root == &expected));
        assert!(!roots.iter().any(|root| {
            canonical
                .parent()
                .is_some_and(|parent| root == &parent.to_string_lossy().replace('\\', "/"))
        }));
    }

    // ── resolve_dependencies (implicit adjacency schedule) ─────────
    //
    // With no explicit `inputs`, the dependency sets must reproduce the old
    // wave behaviour: parallel steps wait only for the most recent sequential
    // step; a sequential step waits for everything before it.

    fn deps_of(steps: &[StepConfig]) -> Vec<std::collections::HashSet<String>> {
        let refs: Vec<&StepConfig> = steps.iter().collect();
        resolve_dependencies(&refs)
    }

    #[test]
    fn implicit_parallel_wave_has_no_deps() {
        let steps = [
            make_step("a", Phase::Parallel),
            make_step("b", Phase::Parallel),
        ];
        let deps = deps_of(&steps);
        assert!(deps[0].is_empty());
        assert!(deps[1].is_empty());
    }

    #[test]
    fn implicit_sequential_waits_for_all_prior() {
        let steps = [
            make_step("a", Phase::Parallel),
            make_step("b", Phase::Parallel),
            make_step("s", Phase::Sequential),
        ];
        let deps = deps_of(&steps);
        assert_eq!(
            deps[2],
            ["a".to_string(), "b".to_string()].into_iter().collect()
        );
    }

    #[test]
    fn implicit_second_wave_waits_for_last_sequential_only() {
        let steps = [
            make_step("p1", Phase::Parallel),
            make_step("s1", Phase::Sequential),
            make_step("p3", Phase::Parallel),
            make_step("s2", Phase::Sequential),
        ];
        let deps = deps_of(&steps);
        assert!(deps[0].is_empty()); // p1
        assert_eq!(deps[1], ["p1".to_string()].into_iter().collect()); // s1 waits for p1
        assert_eq!(deps[2], ["s1".to_string()].into_iter().collect()); // p3 waits for s1 only
        assert_eq!(
            deps[3],
            ["p1".to_string(), "s1".to_string(), "p3".to_string()]
                .into_iter()
                .collect()
        );
    }

    #[test]
    fn explicit_inputs_override_and_drop_unknown() {
        let mut a = make_step("a", Phase::Parallel);
        let mut b = make_step("b", Phase::Sequential);
        b.inputs = vec!["a".into(), "ghost".into(), "b".into()]; // ghost unknown, b is self
        a.inputs = vec![];
        let deps = deps_of(&[a, b]);
        assert!(deps[0].is_empty());
        // Only the real, non-self dependency survives.
        assert_eq!(deps[1], ["a".to_string()].into_iter().collect());
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
        let units = build_units(&step, &settings, "/tmp/x.pdf", &bus);
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].suffix, ""); // bare step id, no composite key
        assert_eq!(units[0].agent, settings.preferred_provider);
    }

    #[test]
    fn build_units_multi_agent_keys_by_agent() {
        let mut step = make_step("s", Phase::Parallel);
        step.agents = vec!["claude".into(), "gemini".into()];
        let settings = crate::settings::Settings::default();
        let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
        let units = build_units(&step, &settings, "/tmp/x.pdf", &bus);
        assert_eq!(units.len(), 2);
        assert_eq!(units[0].suffix, "claude");
        assert_eq!(units[1].suffix, "gemini");
    }

    #[test]
    fn terminal_parallel_failure_unblocks_dependent_step() {
        let mut downstream = make_step("downstream", Phase::Sequential);
        downstream.inputs = vec!["ok".into(), "failed".into()];
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
        downstream.inputs = vec!["fan".into()];
        let steps = [fan, downstream];
        let refs: Vec<&StepConfig> = steps.iter().collect();
        let settings = crate::settings::Settings::default();
        let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);

        assert!(build_units(&steps[0], &settings, temp.path().to_str().unwrap(), &bus).is_empty());

        let deps = resolve_dependencies(&refs);
        let mut done = std::collections::HashSet::new();
        mark_steps_done(&mut done, &refs[..1]);
        assert_eq!(ready_indices(&[1], &deps, &done), vec![1]);
    }

    // ── dependents_of ──────────────────────────────────────────────

    #[test]
    fn dependents_of_finds_transitive_downstream() {
        use crate::pipeline_config::{MergeConfig, PipelineConfig};
        let config = PipelineConfig {
            steps: vec![
                make_step("a", Phase::Parallel),
                make_step("b", Phase::Parallel),
                make_step("s", Phase::Sequential), // implicitly depends on a, b
            ],
            merge: MergeConfig::default(),
            use_orientation: true,
            orientation_prompt: String::new(),
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
        );
        assert_eq!(out, "Review for AER; persona .");
    }

    #[test]
    fn substitute_no_placeholders_is_unchanged() {
        let vars = std::collections::HashMap::new();
        assert_eq!(
            substitute_placeholders("plain prompt", "{var:", &vars),
            "plain prompt"
        );
    }

    #[test]
    fn substitute_unclosed_brace_passes_through() {
        let vars = std::collections::HashMap::new();
        assert_eq!(
            substitute_placeholders("oops {var:x", "{var:", &vars),
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
        );
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
            "/tmp/paper.pdf",
            template,
            "",
        );
        assert!(result.contains("empirical"));
        assert!(result.contains("/tmp/orient.json"));
        assert!(result.contains("Read it for the paper's structure."));
        assert!(result.contains("/tmp/paper.txt"));
        assert!(result.contains("original PDF"));
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
            "/tmp/p.pdf",
            template,
            "",
        );
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
            "/home/user/papers/main.tex",
            template,
            "",
        );
        assert!(result.contains("LaTeX source directory"));
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
            "/tmp/paper.pdf",
            template,
            "",
        );
        assert_eq!(result, "[]");
    }

    #[test]
    fn build_parallel_prompt_output_format_substitution() {
        let step = make_step("test", Phase::Parallel);
        let template = "{step_prompt}\n{output_format}";
        let block = output_format_block(Some("/runs/r1/artifacts"), "steps/test.md");
        let result = build_parallel_prompt(
            &step,
            "",
            "",
            "",
            "/tmp/p.txt",
            "/tmp/p.pdf",
            template,
            &block,
        );
        assert!(result.contains("/runs/r1/artifacts/steps/test.md"));
        // Old templates without the placeholder pass through untouched.
        let old = "{step_prompt}\nREPORT START markers here";
        let result =
            build_parallel_prompt(&step, "", "", "", "/tmp/p.txt", "/tmp/p.pdf", old, &block);
        assert!(!result.contains("{output_format}"));
        assert!(result.contains("REPORT START markers here"));
    }

    // ── write handoff helpers ──────────────────────────────────────

    #[test]
    fn step_file_keys_are_deterministic_and_collision_resistant() {
        assert_eq!(step_slug("technical"), step_slug("technical"));
        assert_eq!(step_slug("technical").len(), 64);
        assert_ne!(step_slug("a.b"), step_slug("a_b"));
        assert_ne!(step_slug("technical"), step_slug("technical/claude"));
    }

    #[test]
    fn output_format_block_modes() {
        let write = output_format_block(Some("/runs/x/artifacts"), "steps/s.md");
        assert!(write.contains("/runs/x/artifacts/steps/s.md"));
        assert!(write.contains("REPORT START")); // escape hatch stays available
        let markers = output_format_block(None, "steps/s.md");
        assert!(markers.contains("REPORT START"));
        assert!(!markers.contains("steps/s.md"));
    }

    #[test]
    fn ingest_report_file_reads_and_removes() {
        let dir = tempfile::tempdir().unwrap();
        let wd = dir.path().to_string_lossy().to_string();

        // Absent file → None.
        assert!(ingest_report_file(Some(&wd), "steps/a.md").is_none());
        // Disabled write dir → None.
        assert!(ingest_report_file(None, "steps/a.md").is_none());

        // Present file → content, and the file is consumed.
        std::fs::create_dir_all(dir.path().join("steps")).unwrap();
        std::fs::write(dir.path().join("steps/a.md"), "# Report\nbody\n").unwrap();
        assert_eq!(
            ingest_report_file(Some(&wd), "steps/a.md").unwrap(),
            "# Report\nbody"
        );
        assert!(!dir.path().join("steps/a.md").exists());

        // Empty file → None (falls back to stdout), file left in place.
        std::fs::write(dir.path().join("steps/b.md"), "  \n").unwrap();
        assert!(ingest_report_file(Some(&wd), "steps/b.md").is_none());
    }

    #[test]
    fn tools_with_write_appends_once() {
        let base = vec!["Read".to_string()];
        assert_eq!(tools_with_write(&base, Some("/d")), vec!["Read", "Write"]);
        assert_eq!(tools_with_write(&base, None), vec!["Read"]);
        let with = vec!["Read".to_string(), "Write".to_string()];
        assert_eq!(tools_with_write(&with, Some("/d")), vec!["Read", "Write"]);
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
            "/source.tex",
        );
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
        let result = expand_template(template, "", "", &prior, "/paper.txt", "/source.tex");
        assert!(result.contains("## S1"));
        assert!(result.contains("text | text"));
    }

    #[test]
    fn expand_template_no_prior() {
        let template = "Last: {last_output}";
        let result = expand_template(template, "", "", &[], "/paper.txt", "/source.tex");
        assert!(result.contains("(not yet generated)"));
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
        let result = expand_template("Tech: {step:technical}", "", "", &prior, "p", "s");
        assert!(result.contains("Tech: tech body"));
        assert!(!result.contains("emp body"));
    }

    #[test]
    fn step_ref_multi_agent_base_id_joins() {
        let prior = vec![
            out("technical/claude", "Technical (Claude)", "claude says"),
            out("technical/gemini", "Technical (Gemini)", "gemini says"),
        ];
        let result = expand_template("All: {step:technical}", "", "", &prior, "p", "s");
        assert!(result.contains("claude says"));
        assert!(result.contains("gemini says"));
        assert!(result.contains("---"));
    }

    #[test]
    fn step_ref_multi_agent_specific_id() {
        let prior = vec![
            out("technical/claude", "Technical (Claude)", "claude says"),
            out("technical/gemini", "Technical (Gemini)", "gemini says"),
        ];
        let result = expand_template(
            "Just one: {step:technical/claude}",
            "",
            "",
            &prior,
            "p",
            "s",
        );
        assert!(result.contains("claude says"));
        assert!(!result.contains("gemini says"));
    }

    #[test]
    fn step_ref_unknown_id_emits_notice() {
        let prior = vec![out("technical", "Technical", "tech body")];
        let result = expand_template("Missing: {step:nonexistent}", "", "", &prior, "p", "s");
        assert!(result.contains("(no output for step 'nonexistent')"));
    }

    #[test]
    fn step_ref_unclosed_brace_passes_through() {
        let prior = vec![out("a", "A", "aa")];
        let result = expand_template("Broken: {step:a", "", "", &prior, "p", "s");
        assert!(result.contains("{step:a"));
    }

    #[test]
    fn step_ref_empty_id() {
        let prior = vec![out("a", "A", "aa")];
        let result = expand_template("{step:}", "", "", &prior, "p", "s");
        assert!(result.contains("(empty step reference)"));
    }
}
