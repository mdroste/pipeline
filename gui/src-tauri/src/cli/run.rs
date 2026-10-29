//! Run, readiness-check, and batch command actions.

use super::args::{BatchArgs, CheckArgs, RunArgs};
use super::interrupt::{is_interruption, report_run_error, run_with_interrupt};
use super::io::{
    absolutize, absolutize_inputs, emit_stdout, failed_step_count, finish_single_run,
    report_markdown, validate_output_path, write_report_file,
};
use super::workflow::load_cli_workflow;
use pipeline_gui_lib::commands;
use pipeline_gui_lib::emit::{CliEvents, EventBus};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(super) async fn cmd_run(args: RunArgs) -> i32 {
    if let Some(path) = args.out.as_deref().filter(|path| *path != "-") {
        if let Err(error) = validate_output_path(Path::new(path), args.force) {
            eprintln!("Error: {error}");
            return 2;
        }
    }
    let workflow = match args
        .workflow_path
        .as_deref()
        .map(load_cli_workflow)
        .transpose()
    {
        Ok(workflow) => workflow,
        Err(error) => {
            eprintln!("Error: {error}");
            return 2;
        }
    };
    let (input_path, extra_inputs) = match args
        .input
        .as_deref()
        .map(absolutize)
        .transpose()
        .and_then(|input| absolutize_inputs(args.extra_inputs).map(|extra| (input, extra)))
    {
        Ok((input, extra)) => (input.unwrap_or_default(), extra),
        Err(error) => {
            eprintln!("Error: {error}");
            return 2;
        }
    };
    let options = commands::HeadlessRunOptions {
        profile_id: args.profile_id,
        workflow,
        input_path,
        input_interpretation: args.input_interpretation,
        variables: args.variables,
        extra_inputs,
        limits: args.limits.overrides(),
    };
    let bus: EventBus = Arc::new(CliEvents);
    match run_with_interrupt(bus, options, None).await {
        Ok(value) => finish_single_run(&value, args.out.as_deref(), args.force),
        Err(error) => report_run_error(&error),
    }
}

/// One-line rough usage estimate for `check`.
fn usage_line(estimate: &pipeline_gui_lib::pipeline::executor::budget::UsageEstimate) -> String {
    let calls = if estimate.calls_low == estimate.calls_high {
        estimate.calls_low.to_string()
    } else {
        format!("{}-{}", estimate.calls_low, estimate.calls_high)
    };
    let cost = match (estimate.api_cost_low, estimate.api_cost_high) {
        (Some(low), Some(high)) => format!("; API list price ${low:.2}-${high:.2}"),
        _ => String::new(),
    };
    format!("Estimated use (rough): {calls} model calls{cost}")
}

pub(super) async fn cmd_check(args: CheckArgs) -> i32 {
    let workflow = match args
        .workflow_path
        .as_deref()
        .map(load_cli_workflow)
        .transpose()
    {
        Ok(workflow) => workflow,
        Err(error) => {
            eprintln!("Error: {error}");
            return 2;
        }
    };
    let (input_path, extra_inputs) = match args
        .input
        .as_deref()
        .map(absolutize)
        .transpose()
        .and_then(|input| absolutize_inputs(args.extra_inputs).map(|extra| (input, extra)))
    {
        Ok((input, extra)) => (input.unwrap_or_default(), extra),
        Err(error) => {
            eprintln!("Error: {error}");
            return 2;
        }
    };
    let options = commands::HeadlessRunOptions {
        profile_id: args.profile_id,
        workflow,
        input_path,
        input_interpretation: args.input_interpretation,
        variables: args.variables,
        extra_inputs,
        limits: Default::default(),
    };
    let plan = match commands::check_headless_plan(&options).await {
        Ok(report) => report,
        Err(error) => {
            eprintln!("Error: {error}");
            return 2;
        }
    };
    if args.json {
        match serde_json::to_string_pretty(&plan) {
            Ok(encoded) => emit_stdout(&encoded),
            Err(error) => {
                eprintln!("Error: failed to encode dependency report: {error}");
                return 1;
            }
        }
    } else {
        emit_stdout(&format!("Workflow: {}", plan.profile_name));
        emit_stdout(&format!("Fingerprint: {}", plan.workflow_fingerprint));
        emit_stdout(&format!(
            "Plan: {} stages, up to {} step units and {} provider attempts",
            plan.stages.len(),
            plan.budget.step_units_upper_bound,
            plan.budget.provider_attempts_upper_bound
        ));
        emit_stdout(&usage_line(&plan.usage_estimate));
        let required = plan
            .readiness
            .deps
            .iter()
            .filter(|dependency| dependency.required)
            .collect::<Vec<_>>();
        if required.is_empty() {
            emit_stdout("No external dependencies are required for this run.");
        }
        for dependency in required {
            let ready = pipeline_gui_lib::deps::dependency_ready(dependency);
            let marker = if ready { "ok" } else { "missing" };
            let detail = if !dependency.version.is_empty() {
                format!(" ({})", dependency.version)
            } else {
                String::new()
            };
            emit_stdout(&format!("[{marker}] {}{detail}", dependency.name));
            if !ready {
                emit_stdout(&format!("  {}", dependency.hint));
            }
        }
        emit_stdout(&format!(
            "Dependency check: {}",
            if plan.readiness.ready {
                "ready"
            } else {
                "not ready"
            }
        ));
    }
    if plan.readiness.ready {
        0
    } else {
        1
    }
}

pub(super) async fn cmd_batch(args: BatchArgs) -> i32 {
    let workflow = match args
        .workflow_path
        .as_deref()
        .map(load_cli_workflow)
        .transpose()
    {
        Ok(workflow) => workflow,
        Err(error) => {
            eprintln!("Error: {error}");
            return 2;
        }
    };
    let selected_profile = args.profile_id.clone();
    let snapshot = match commands::HeadlessBatchSnapshot::capture(
        selected_profile.as_deref(),
        workflow.as_ref(),
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            eprintln!("Error: {error}");
            return 2;
        }
    };

    let files = match commands::scan_input_files(&args.input_dir).and_then(|files| {
        files
            .iter()
            .map(|file| absolutize(file))
            .collect::<Result<Vec<_>, _>>()
    }) {
        Ok(files) if !files.is_empty() => files,
        Ok(_) => {
            eprintln!(
                "No PDF, TeX, or DOCX files found directly inside {}",
                args.input_dir
            );
            return 2;
        }
        Err(error) => {
            eprintln!("Error: {error}");
            return 2;
        }
    };

    let output_paths = match prepare_batch_outputs(args.out_dir.as_deref(), &files, args.force) {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("Error: {error}");
            return 2;
        }
    };

    let extra_inputs = match absolutize_inputs(args.extra_inputs) {
        Ok(extra) => extra,
        Err(error) => {
            eprintln!("Error: {error}");
            return 2;
        }
    };
    let mut failures = 0;
    for (index, path) in files.iter().enumerate() {
        eprintln!("\n[{}/{}] {path}", index + 1, files.len());
        let bus: EventBus = Arc::new(CliEvents);
        let options = commands::HeadlessRunOptions {
            profile_id: selected_profile.clone(),
            workflow: workflow.clone(),
            input_path: path.clone(),
            input_interpretation: Some("document".to_string()),
            variables: args.variables.clone(),
            extra_inputs: extra_inputs.clone(),
            limits: args.limits.overrides(),
        };
        match run_with_interrupt(bus, options, Some(&snapshot)).await {
            Ok(value) => {
                let markdown = match report_markdown(&value) {
                    Ok(markdown) => markdown,
                    Err(error) => {
                        eprintln!("  failed: {error}");
                        failures += 1;
                        continue;
                    }
                };
                if let Some(output_path) = output_paths.as_ref().map(|paths| &paths[index]) {
                    match write_report_file(output_path, markdown, args.force) {
                        Ok(()) => eprintln!("  report: {}", output_path.display()),
                        Err(error) => {
                            eprintln!("  failed to save report: {error}");
                            failures += 1;
                            continue;
                        }
                    }
                }
                if let Some(run_id) = value.get("run_id").and_then(|id| id.as_str()) {
                    eprintln!("  run: {run_id}");
                }
                if failed_step_count(&value) > 0 {
                    eprintln!("  failed: one or more workflow steps failed");
                    failures += 1;
                }
            }
            Err(error) => {
                eprintln!("  failed: {error}");
                failures += 1;
                if is_interruption(&error) {
                    return 130;
                }
            }
        }
    }
    eprintln!(
        "\nBatch done: {} ok, {} failed",
        files.len() - failures,
        failures
    );
    if failures > 0 {
        1
    } else {
        0
    }
}

pub(super) fn prepare_batch_outputs(
    out_dir: Option<&str>,
    files: &[String],
    force: bool,
) -> Result<Option<Vec<PathBuf>>, String> {
    let Some(out_dir) = out_dir else {
        return Ok(None);
    };
    let out_dir = PathBuf::from(out_dir);
    std::fs::create_dir_all(&out_dir)
        .map_err(|error| format!("Could not create output directory: {error}"))?;
    if !out_dir.is_dir() {
        return Err(format!("{} is not a directory", out_dir.display()));
    }

    let mut seen = HashSet::new();
    let mut outputs = Vec::with_capacity(files.len());
    for input in files {
        let stem = Path::new(input)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .filter(|stem| !stem.is_empty())
            .ok_or_else(|| format!("Cannot derive an output name from '{input}'"))?;
        let output = out_dir.join(format!("{stem}.md"));
        let collision_key = output.to_string_lossy().to_lowercase();
        if !seen.insert(collision_key) {
            return Err(format!(
                "multiple inputs would write {}; use distinct input filenames",
                output.display()
            ));
        }
        validate_output_path(&output, force)?;
        outputs.push(output);
    }
    Ok(Some(outputs))
}
