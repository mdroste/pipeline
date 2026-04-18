//! Unified pipeline executor.
//!
//! Walks the step list top-to-bottom, grouping adjacent Parallel steps
//! into waves. Sequential steps run one at a time after all prior steps.
//! Merge auto-triggers between a parallel wave and the next step when
//! any parallel step used multiple agents.

use super::claude::call_llm;
use super::merge;
use crate::models::{StepFailure, StepOutput};
use crate::output::{capitalize, strip_to_report};
use crate::pipeline_config::{Phase, PipelineConfig, StepConfig};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

/// Result of pipeline execution: successful outputs and any step failures.
pub struct ExecutionResult {
    pub outputs: Vec<StepOutput>,
    pub failed_steps: Vec<StepFailure>,
}

/// Execute all enabled steps in the pipeline.
///
/// Returns successful outputs and a list of any steps that failed.
/// A failed sequential step stops further execution but does not discard prior outputs.
pub async fn execute_steps(
    app: &AppHandle,
    config: &PipelineConfig,
    orientation_path: &str,
    paper_text_path: &str,
    source_path: &str,
    paper_type: &str,
) -> Result<ExecutionResult, String> {
    let settings = crate::settings::load();
    let semaphore = Arc::new(Semaphore::new(settings.max_workers.max(1) as usize));
    let mut all_outputs: Vec<StepOutput> = Vec::new();
    let mut failed_steps: Vec<StepFailure> = Vec::new();

    let enabled: Vec<&StepConfig> = config.steps.iter().filter(|s| s.enabled).collect();
    let waves = group_into_waves(&enabled);

    for wave in waves {
        match wave {
            Wave::Parallel(steps) => {
                app.emit(
                    "pipeline:stage",
                    serde_json::json!({"stage": "dispatching"}),
                )
                .ok();

                let mut wave_outputs =
                    run_parallel_wave(app, steps, &settings, &semaphore, orientation_path, paper_text_path, source_path, paper_type, &config.parallel_context_template)
                        .await?;

                let has_multi_agent = wave_outputs.iter().any(|o| o.step_id.contains('/'));
                if has_multi_agent && config.merge.enabled {
                    app.emit(
                        "pipeline:stage",
                        serde_json::json!({"stage": "merging"}),
                    )
                    .ok();
                    match merge::merge_step_outputs(app, wave_outputs.clone(), &config.merge, &semaphore).await {
                        Ok(merged) => wave_outputs = merged,
                        Err(e) => {
                            let _ = app.emit(
                                "pipeline:log",
                                serde_json::json!({ "line": format!("WARNING: merge failed: {e}. Using unmerged outputs.") }),
                            );
                        }
                    }
                }

                all_outputs.extend(wave_outputs);
            }
            Wave::Sequential(step) => {
                app.emit(
                    "pipeline:stage",
                    serde_json::json!({"stage": "synthesizing"}),
                )
                .ok();

                match run_sequential_step(
                    app,
                    step,
                    &all_outputs,
                    orientation_path,
                    paper_text_path,
                    source_path,
                )
                .await
                {
                    Ok(output) => {
                        let _ = app.emit(
                            "pipeline:pass",
                            serde_json::json!({
                                "name": step.id,
                                "status": "done"
                            }),
                        );
                        all_outputs.push(output);
                    }
                    Err(e) => {
                        let _ = app.emit(
                            "pipeline:pass",
                            serde_json::json!({
                                "name": step.id,
                                "status": "error"
                            }),
                        );
                        let _ = app.emit(
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
        }
    }

    if all_outputs.is_empty() {
        return Err("No steps produced output. Enable at least one step.".into());
    }

    Ok(ExecutionResult { outputs: all_outputs, failed_steps })
}

// ── Wave grouping ───────────────────────────────────────────────────

enum Wave<'a> {
    Parallel(Vec<&'a StepConfig>),
    Sequential(&'a StepConfig),
}

fn group_into_waves<'a>(steps: &[&'a StepConfig]) -> Vec<Wave<'a>> {
    let mut waves = Vec::new();
    let mut current_parallel: Vec<&'a StepConfig> = Vec::new();

    for step in steps {
        match step.phase {
            Phase::Parallel => {
                current_parallel.push(step);
            }
            Phase::Sequential => {
                if !current_parallel.is_empty() {
                    waves.push(Wave::Parallel(std::mem::take(&mut current_parallel)));
                }
                waves.push(Wave::Sequential(step));
            }
        }
    }

    if !current_parallel.is_empty() {
        waves.push(Wave::Parallel(current_parallel));
    }

    waves
}

// ── Parallel execution ──────────────────────────────────────────────

/// Build the prompt for a parallel step (referee-style).
fn build_parallel_prompt(
    step: &StepConfig,
    paper_type: &str,
    orientation_path: &str,
    paper_text_path: &str,
    source_path: &str,
    template: &str,
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
        format!(
            "The orientation map (JSON) is at: {normalized_orient}\n\
             Read it for the paper's structure, sections, formal results, tables, figures, and notation."
        )
    };

    template
        .replace("{step_prompt}", &step.prompt)
        .replace("{paper_type}", paper_type)
        .replace("{orientation}", &orientation_block)
        .replace("{paper_path}", &normalized_path)
        .replace("{figure_hint}", &figure_hint)
}

/// Run all parallel steps in a wave concurrently.
async fn run_parallel_wave(
    app: &AppHandle,
    steps: Vec<&StepConfig>,
    settings: &crate::settings::Settings,
    semaphore: &Arc<Semaphore>,
    orientation_path: &str,
    paper_text_path: &str,
    source_path: &str,
    paper_type: &str,
    context_template: &str,
) -> Result<Vec<StepOutput>, String> {
    let source_dir = std::path::Path::new(source_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string());

    let mut tasks: JoinSet<Result<((usize, String), StepOutput), (String, String)>> =
        JoinSet::new();

    for (idx, step) in steps.iter().enumerate() {
        let agents: Vec<String> = if step.agents.is_empty() {
            vec![settings.preferred_provider.clone()]
        } else {
            step.agents.clone()
        };
        let multi = agents.len() > 1;

        for agent in &agents {
            let prompt = build_parallel_prompt(
                step,
                paper_type,
                orientation_path,
                paper_text_path,
                source_path,
                context_template,
            );
            let id = step.id.clone();
            let label = step.label.clone();
            let agent_name = agent.clone();
            let tools = step.tools.clone();

            // For multi-agent, use composite key
            let step_key = if multi {
                format!("{}/{}", id, agent_name)
            } else {
                id.clone()
            };

            let _ = app.emit(
                "pipeline:pass",
                serde_json::json!({
                    "name": step_key,
                    "status": "running"
                }),
            );

            let app_handle = app.clone();
            let step_key_emit = step_key.clone();
            let log_label = if multi {
                format!("Step: {} ({})", label, capitalize(&agent_name))
            } else {
                format!("Step: {}", label)
            };
            let display_label = if multi {
                format!("{} ({})", label, capitalize(&agent_name))
            } else {
                label.clone()
            };
            let sort_key = (idx, agent_name.clone());
            let sem = semaphore.clone();
            let task_cwd = source_dir.clone();

            tasks.spawn(async move {
                let _permit = sem
                    .acquire()
                    .await
                    .map_err(|_| (step_key_emit.clone(), "Semaphore closed".to_string()))?;
                let tool_refs: Vec<&str> = tools.iter().map(|s| s.as_str()).collect();
                let settings = crate::settings::load();
                let timeout = settings.step_timeout_secs.max(60);
                let max_retries = settings.max_retries;

                let mut last_err = String::new();
                for attempt in 0..=max_retries {
                    if attempt > 0 {
                        let _ = app_handle.emit(
                            "pipeline:log",
                            serde_json::json!({ "line": format!("{log_label}: retry {attempt}/{max_retries} after failure: {last_err}") }),
                        );
                        let _ = app_handle.emit(
                            "pipeline:pass",
                            serde_json::json!({
                                "name": step_key_emit,
                                "status": "running"
                            }),
                        );
                    }
                    let extra: Vec<&str> = task_cwd.as_deref().into_iter().collect();
                    match call_llm(
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
                    )
                    .await
                    {
                        Ok(raw_text) => {
                            let _ = app_handle.emit(
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
                                    agent: agent_name,
                                    raw_text: strip_to_report(&raw_text),
                                },
                            ));
                        }
                        Err(e) => {
                            if e.contains("cancelled") {
                                let _ = app_handle.emit(
                                    "pipeline:pass",
                                    serde_json::json!({
                                        "name": step_key_emit,
                                        "status": "error"
                                    }),
                                );
                                return Err((step_key, e));
                            }
                            last_err = e;
                        }
                    }
                }

                let _ = app_handle.emit(
                    "pipeline:pass",
                    serde_json::json!({
                        "name": step_key_emit,
                        "status": "error"
                    }),
                );
                Err((step_key, last_err))
            });
        }
    }

    let mut results = Vec::new();
    let mut errors = Vec::new();

    while let Some(res) = tasks.join_next().await {
        match res {
            Ok(Ok(report)) => results.push(report),
            Ok(Err((name, e))) => errors.push(format!("{name}: {e}")),
            Err(e) => errors.push(format!("Task panicked: {e}")),
        }
    }

    if !errors.is_empty() {
        for err in &errors {
            let _ = app.emit(
                "pipeline:log",
                serde_json::json!({ "line": format!("WARNING: step failed: {err}") }),
            );
        }
        if results.is_empty() {
            return Err(format!("All steps failed: {}", errors.join("; ")));
        }
        let _ = app.emit(
            "pipeline:log",
            serde_json::json!({ "line": format!(
                "WARNING: {}/{} parallel steps succeeded. Downstream steps will receive incomplete inputs.",
                results.len(), results.len() + errors.len()
            )}),
        );
    }

    // Sort by original config order, then agent name
    results.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(results.into_iter().map(|(_, r)| r).collect())
}

// ── Sequential execution ────────────────────────────────────────────

/// Expand template placeholders in a sequential step's prompt.
fn expand_template(
    template: &str,
    orientation_path: &str,
    prior_outputs: &[StepOutput],
    paper_text_path: &str,
    source_path: &str,
) -> String {
    let orientation_ref = if orientation_path.is_empty() {
        String::new()
    } else {
        let normalized_orient = orientation_path.replace('\\', "/");
        format!(
            "The orientation map (JSON) is at: {normalized_orient}\n\
             Read it for the paper's structure, sections, formal results, tables, figures, and notation."
        )
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

    template
        .replace("{orientation}", &orientation_ref)
        .replace("{prior_outputs}", &prior_text)
        .replace("{referee_reports}", &prior_text) // backward-compatible alias
        .replace("{last_output}", last_output_text)
        .replace("{editor_synthesis}", last_output_text) // backward-compatible alias
        .replace("{paper_path}", paper_text_path)
        .replace("{source_path}", source_path)
}

/// Run a single sequential step.
async fn run_sequential_step(
    app: &AppHandle,
    step: &StepConfig,
    prior_outputs: &[StepOutput],
    orientation_path: &str,
    paper_text_path: &str,
    source_path: &str,
) -> Result<StepOutput, String> {
    let _ = app.emit(
        "pipeline:pass",
        serde_json::json!({
            "name": step.id,
            "status": "running"
        }),
    );

    let base_prompt = expand_template(
        &step.prompt,
        orientation_path,
        prior_outputs,
        paper_text_path,
        source_path,
    );

    let prompt = format!(
        "{base_prompt}\n\n\
         OUTPUT FORMAT:\n\
         Begin your report with exactly `<!-- REPORT START -->` and end with exactly `<!-- REPORT END -->`.\n\
         Include ONLY your markdown report between those markers — no preamble, no commentary, no acknowledgments outside them."
    );

    let tool_refs: Vec<&str> = step.tools.iter().map(|s| s.as_str()).collect();
    let agent = step.agents.first().map(|s| s.as_str());
    let log_label = format!("Step: {}", step.label);

    // Use the paper's parent directory as CWD for steps with Read access
    let source_dir = std::path::Path::new(source_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string());
    let settings = crate::settings::load();
    let timeout = settings.step_timeout_secs.max(60);
    let max_retries = settings.max_retries;

    let mut last_err = String::new();
    let mut raw_text = String::new();
    for attempt in 0..=max_retries {
        if attempt > 0 {
            let _ = app.emit(
                "pipeline:log",
                serde_json::json!({ "line": format!("{log_label}: retry {attempt}/{max_retries} after failure: {last_err}") }),
            );
            let _ = app.emit(
                "pipeline:pass",
                serde_json::json!({
                    "name": step.id,
                    "status": "running"
                }),
            );
        }
        let extra: Vec<&str> = source_dir.as_deref().into_iter().collect();
        match call_llm(app, &prompt, &tool_refs, None, "text", timeout, &log_label, agent, source_dir.as_deref(), &extra).await {
            Ok(text) => {
                raw_text = text;
                last_err.clear();
                break;
            }
            Err(e) => {
                if e.contains("cancelled") {
                    return Err(e);
                }
                last_err = e;
            }
        }
    }
    if !last_err.is_empty() {
        return Err(last_err);
    }

    Ok(StepOutput {
        step_id: step.id.clone(),
        step_label: step.label.clone(),
        phase: "sequential".to_string(),
        agent: agent.unwrap_or("").to_string(),
        raw_text: strip_to_report(&raw_text),
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
            prompt: String::new(),
            enabled: true,
            phase,
            tools: vec![],
            agents: vec![],
        }
    }

    // ── group_into_waves ───────────────────────────────────────────

    #[test]
    fn waves_all_parallel() {
        let steps = [
            make_step("a", Phase::Parallel),
            make_step("b", Phase::Parallel),
        ];
        let refs: Vec<&StepConfig> = steps.iter().collect();
        let waves = group_into_waves(&refs);
        assert_eq!(waves.len(), 1);
        assert!(matches!(&waves[0], Wave::Parallel(s) if s.len() == 2));
    }

    #[test]
    fn waves_all_sequential() {
        let steps = [
            make_step("a", Phase::Sequential),
            make_step("b", Phase::Sequential),
        ];
        let refs: Vec<&StepConfig> = steps.iter().collect();
        let waves = group_into_waves(&refs);
        assert_eq!(waves.len(), 2);
        assert!(matches!(&waves[0], Wave::Sequential(_)));
        assert!(matches!(&waves[1], Wave::Sequential(_)));
    }

    #[test]
    fn waves_mixed() {
        let steps = [
            make_step("p1", Phase::Parallel),
            make_step("p2", Phase::Parallel),
            make_step("s1", Phase::Sequential),
            make_step("p3", Phase::Parallel),
            make_step("s2", Phase::Sequential),
        ];
        let refs: Vec<&StepConfig> = steps.iter().collect();
        let waves = group_into_waves(&refs);
        assert_eq!(waves.len(), 4);
        assert!(matches!(&waves[0], Wave::Parallel(s) if s.len() == 2));
        assert!(matches!(&waves[1], Wave::Sequential(s) if s.id == "s1"));
        assert!(matches!(&waves[2], Wave::Parallel(s) if s.len() == 1));
        assert!(matches!(&waves[3], Wave::Sequential(s) if s.id == "s2"));
    }

    #[test]
    fn waves_empty() {
        let refs: Vec<&StepConfig> = vec![];
        let waves = group_into_waves(&refs);
        assert!(waves.is_empty());
    }

    // ── build_parallel_prompt ──────────────────────────────────────

    #[test]
    fn build_parallel_prompt_pdf() {
        let step = make_step("test", Phase::Parallel);
        let template = "{paper_type}\n{orientation}\n{step_prompt}\n{paper_path}\n{figure_hint}";
        let result = build_parallel_prompt(&step, "empirical", "/tmp/orient.json", "/tmp/paper.txt", "/tmp/paper.pdf", template);
        assert!(result.contains("empirical"));
        assert!(result.contains("/tmp/orient.json"));
        assert!(result.contains("/tmp/paper.txt"));
        assert!(result.contains("original PDF"));
    }

    #[test]
    fn build_parallel_prompt_latex() {
        let step = make_step("test", Phase::Parallel);
        let template = "{figure_hint}";
        let result = build_parallel_prompt(&step, "theory", "", "/tmp/paper.txt", "/home/user/papers/main.tex", template);
        assert!(result.contains("LaTeX source directory"));
    }

    #[test]
    fn build_parallel_prompt_empty_orientation() {
        let step = make_step("test", Phase::Parallel);
        let template = "[{orientation}]";
        let result = build_parallel_prompt(&step, "mixed", "", "/tmp/paper.txt", "/tmp/paper.pdf", template);
        assert_eq!(result, "[]");
    }

    // ── expand_template ────────────────────────────────────────────

    #[test]
    fn expand_template_basic() {
        let prior = vec![
            StepOutput { step_id: "s1".into(), step_label: "Step 1".into(), phase: "parallel".into(), agent: String::new(), raw_text: "output1".into() },
            StepOutput { step_id: "s2".into(), step_label: "Step 2".into(), phase: "parallel".into(), agent: String::new(), raw_text: "output2".into() },
        ];
        let template = "Prior:\n{prior_outputs}\n\nLast: {last_output}";
        let result = expand_template(template, "/orient.json", &prior, "/paper.txt", "/source.tex");
        assert!(result.contains("## Step 1"));
        assert!(result.contains("output1"));
        assert!(result.contains("output2"));
        assert!(result.contains("Last: output2"));
    }

    #[test]
    fn expand_template_backward_compat_aliases() {
        let prior = vec![
            StepOutput { step_id: "s1".into(), step_label: "S1".into(), phase: "parallel".into(), agent: String::new(), raw_text: "text".into() },
        ];
        let template = "{referee_reports} | {editor_synthesis}";
        let result = expand_template(template, "", &prior, "/paper.txt", "/source.tex");
        assert!(result.contains("## S1"));
        assert!(result.contains("text | text"));
    }

    #[test]
    fn expand_template_no_prior() {
        let template = "Last: {last_output}";
        let result = expand_template(template, "", &[], "/paper.txt", "/source.tex");
        assert!(result.contains("(not yet generated)"));
    }
}
