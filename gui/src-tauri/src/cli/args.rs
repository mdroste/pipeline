//! Command arguments and the existing handwritten CLI parser.

use super::help::{
    BATCH_HELP, CHECK_HELP, ENGINES_HELP, MAIN_HELP, PROFILES_HELP, RUN_HELP, WORKFLOW_HELP,
};
use std::collections::HashMap;

#[derive(Debug)]
#[allow(clippy::large_enum_variant)] // One short-lived parse result per CLI process.
pub(super) enum ParseAction {
    Execute(Command),
    Help(&'static str),
    Version,
}

#[derive(Debug)]
pub(super) enum Command {
    Run(RunArgs),
    Check(CheckArgs),
    Batch(BatchArgs),
    Workflow(WorkflowArgs),
    Profiles(ProfilesArgs),
    Engines(EnginesArgs),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct RunArgs {
    pub(super) input: Option<String>,
    pub(super) input_interpretation: Option<String>,
    pub(super) profile_id: Option<String>,
    pub(super) workflow_path: Option<String>,
    pub(super) variables: HashMap<String, String>,
    pub(super) extra_inputs: HashMap<String, String>,
    pub(super) out: Option<String>,
    pub(super) force: bool,
    pub(super) limits: LimitArgs,
}

/// `--max-cost`, `--max-calls`, `--max-minutes`, validated at parse time.
/// Cost is kept in millionths of a dollar so the arguments stay comparable.
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub(super) struct LimitArgs {
    pub(super) max_cost_micro_usd: Option<u64>,
    pub(super) max_calls: Option<u32>,
    pub(super) max_minutes: Option<u32>,
}

impl LimitArgs {
    pub(super) fn overrides(self) -> pipeline_gui_lib::commands::ReviewLimitOverrides {
        pipeline_gui_lib::commands::ReviewLimitOverrides {
            max_cost_usd: self
                .max_cost_micro_usd
                .map(|micro| micro as f64 / 1_000_000.0),
            max_calls: self.max_calls,
            max_minutes: self.max_minutes,
        }
    }
}

fn set_limit<T>(slot: &mut Option<T>, value: T, flag: &str) -> Result<(), String> {
    if slot.replace(value).is_some() {
        return Err(format!("{flag} may be specified only once"));
    }
    Ok(())
}

/// Parses one limit flag; returns false when `flag` is not a limit flag.
fn parse_limit(
    flag: &str,
    args: &[String],
    index: &mut usize,
    limits: &mut LimitArgs,
    command: &str,
) -> Result<bool, String> {
    let count = |value: &str| -> Result<u32, String> {
        value
            .trim()
            .parse::<u32>()
            .map_err(|_| format!("{command}: {flag} needs a whole number (0 means no limit)"))
    };
    match flag {
        "--max-cost" => {
            let value = next_value(args, index, flag)?;
            let usd = value
                .trim()
                .trim_start_matches('$')
                .parse::<f64>()
                .ok()
                .filter(|usd| usd.is_finite() && *usd >= 0.0 && *usd <= 1_000_000.0)
                .ok_or_else(|| format!("{command}: --max-cost needs an amount in US dollars"))?;
            let micro = (usd * 1_000_000.0).round() as u64;
            set_limit(&mut limits.max_cost_micro_usd, micro, flag)?;
        }
        "--max-calls" => {
            let value = count(&next_value(args, index, flag)?)?;
            set_limit(&mut limits.max_calls, value, flag)?;
        }
        "--max-minutes" => {
            let value = count(&next_value(args, index, flag)?)?;
            set_limit(&mut limits.max_minutes, value, flag)?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct BatchArgs {
    pub(super) input_dir: String,
    pub(super) profile_id: Option<String>,
    pub(super) workflow_path: Option<String>,
    pub(super) variables: HashMap<String, String>,
    pub(super) extra_inputs: HashMap<String, String>,
    pub(super) out_dir: Option<String>,
    pub(super) force: bool,
    pub(super) limits: LimitArgs,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct CheckArgs {
    pub(super) input: Option<String>,
    pub(super) input_interpretation: Option<String>,
    pub(super) profile_id: Option<String>,
    pub(super) workflow_path: Option<String>,
    pub(super) variables: HashMap<String, String>,
    pub(super) extra_inputs: HashMap<String, String>,
    pub(super) json: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum ProfilesArgs {
    List { json: bool },
    Show { id: String },
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum WorkflowArgs {
    Schema,
    Template,
    Validate { source: String, json: bool },
    Install { source: String, json: bool },
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum EnginesArgs {
    Status { json: bool },
    Install { id: String },
    Uninstall { id: String },
}

pub(super) fn parse_cli(args: &[String]) -> Result<ParseAction, String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Ok(ParseAction::Help(MAIN_HELP));
    };
    match command {
        "run" => parse_run(&args[1..]),
        "check" => parse_check(&args[1..]),
        "batch" => parse_batch(&args[1..]),
        "workflow" => parse_workflow(&args[1..]),
        "profiles" => parse_profiles(&args[1..]),
        "engines" => parse_engines(&args[1..]),
        "help" => parse_help(&args[1..]),
        "--help" | "-h" => {
            reject_trailing(&args[1..], "--help")?;
            Ok(ParseAction::Help(MAIN_HELP))
        }
        "--version" | "-V" => {
            reject_trailing(&args[1..], "--version")?;
            Ok(ParseAction::Version)
        }
        other => Err(format!("unknown command '{other}'")),
    }
}

fn parse_help(args: &[String]) -> Result<ParseAction, String> {
    match args {
        [] => Ok(ParseAction::Help(MAIN_HELP)),
        [command] if command == "run" => Ok(ParseAction::Help(RUN_HELP)),
        [command] if command == "check" => Ok(ParseAction::Help(CHECK_HELP)),
        [command] if command == "batch" => Ok(ParseAction::Help(BATCH_HELP)),
        [command] if command == "workflow" => Ok(ParseAction::Help(WORKFLOW_HELP)),
        [command] if command == "profiles" => Ok(ParseAction::Help(PROFILES_HELP)),
        [command] if command == "engines" => Ok(ParseAction::Help(ENGINES_HELP)),
        [command] => Err(format!("unknown command '{command}'")),
        _ => Err("help accepts at most one command name".to_string()),
    }
}

fn parse_check(args: &[String]) -> Result<ParseAction, String> {
    let mut parsed = CheckArgs::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return Ok(ParseAction::Help(CHECK_HELP)),
            "--input" | "-i" => {
                let value = next_value(args, &mut index, "--input")?;
                set_once(&mut parsed.input, value, "--input")?;
            }
            "--interpret-as" => {
                let value = next_value(args, &mut index, "--interpret-as")?;
                let value = normalize_input_interpretation(&value)?;
                set_once(&mut parsed.input_interpretation, value, "--interpret-as")?;
            }
            "--profile" | "-p" => {
                let value = next_value(args, &mut index, "--profile")?;
                set_once(&mut parsed.profile_id, value, "--profile")?;
            }
            "--workflow" | "-w" => {
                let value = next_value(args, &mut index, "--workflow")?;
                set_once(&mut parsed.workflow_path, value, "--workflow")?;
            }
            "--var" => {
                let value = next_value(args, &mut index, "--var")?;
                insert_assignment(&mut parsed.variables, &value, "--var", true)?;
            }
            "--extra-input" => {
                let value = next_value(args, &mut index, "--extra-input")?;
                insert_assignment(&mut parsed.extra_inputs, &value, "--extra-input", false)?;
            }
            "--json" if !parsed.json => parsed.json = true,
            "--json" => return Err("check: --json may be specified only once".to_string()),
            unknown => return Err(format!("check: unknown argument '{unknown}'")),
        }
        index += 1;
    }
    if parsed.input_interpretation.is_some() && parsed.input.is_none() {
        return Err("check: --interpret-as requires --input".to_string());
    }
    reject_profile_and_workflow("check", &parsed.profile_id, &parsed.workflow_path)?;
    Ok(ParseAction::Execute(Command::Check(parsed)))
}

fn parse_run(args: &[String]) -> Result<ParseAction, String> {
    let mut parsed = RunArgs::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return Ok(ParseAction::Help(RUN_HELP)),
            "--input" | "-i" => {
                let value = next_value(args, &mut index, "--input")?;
                set_once(&mut parsed.input, value, "--input")?;
            }
            "--interpret-as" => {
                let value = next_value(args, &mut index, "--interpret-as")?;
                let value = normalize_input_interpretation(&value)?;
                set_once(&mut parsed.input_interpretation, value, "--interpret-as")?;
            }
            "--profile" | "-p" => {
                let value = next_value(args, &mut index, "--profile")?;
                set_once(&mut parsed.profile_id, value, "--profile")?;
            }
            "--workflow" | "-w" => {
                let value = next_value(args, &mut index, "--workflow")?;
                set_once(&mut parsed.workflow_path, value, "--workflow")?;
            }
            "--var" => {
                let value = next_value(args, &mut index, "--var")?;
                insert_assignment(&mut parsed.variables, &value, "--var", true)?;
            }
            "--extra-input" => {
                let value = next_value(args, &mut index, "--extra-input")?;
                insert_assignment(&mut parsed.extra_inputs, &value, "--extra-input", false)?;
            }
            "--out" | "-o" => {
                let value = next_value(args, &mut index, "--out")?;
                set_once(&mut parsed.out, value, "--out")?;
            }
            "--force" if !parsed.force => parsed.force = true,
            "--force" => return Err("run: --force may be specified only once".to_string()),
            flag if parse_limit(flag, args, &mut index, &mut parsed.limits, "run")? => {}
            unknown => return Err(format!("run: unknown argument '{unknown}'")),
        }
        index += 1;
    }
    if parsed.input_interpretation.is_some() && parsed.input.is_none() {
        return Err("run: --interpret-as requires --input".to_string());
    }
    if parsed.force && matches!(parsed.out.as_deref(), None | Some("-")) {
        return Err("run: --force requires a file destination in --out".to_string());
    }
    reject_profile_and_workflow("run", &parsed.profile_id, &parsed.workflow_path)?;
    Ok(ParseAction::Execute(Command::Run(parsed)))
}

fn parse_batch(args: &[String]) -> Result<ParseAction, String> {
    let mut parsed = BatchArgs::default();
    let mut input_dir = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return Ok(ParseAction::Help(BATCH_HELP)),
            "--input-dir" | "-i" => {
                let value = next_value(args, &mut index, "--input-dir")?;
                set_once(&mut input_dir, value, "--input-dir")?;
            }
            "--profile" | "-p" => {
                let value = next_value(args, &mut index, "--profile")?;
                set_once(&mut parsed.profile_id, value, "--profile")?;
            }
            "--workflow" | "-w" => {
                let value = next_value(args, &mut index, "--workflow")?;
                set_once(&mut parsed.workflow_path, value, "--workflow")?;
            }
            "--var" => {
                let value = next_value(args, &mut index, "--var")?;
                insert_assignment(&mut parsed.variables, &value, "--var", true)?;
            }
            "--extra-input" => {
                let value = next_value(args, &mut index, "--extra-input")?;
                insert_assignment(&mut parsed.extra_inputs, &value, "--extra-input", false)?;
            }
            "--out-dir" | "-o" => {
                let value = next_value(args, &mut index, "--out-dir")?;
                set_once(&mut parsed.out_dir, value, "--out-dir")?;
            }
            "--force" if !parsed.force => parsed.force = true,
            "--force" => return Err("batch: --force may be specified only once".to_string()),
            flag if parse_limit(flag, args, &mut index, &mut parsed.limits, "batch")? => {}
            unknown => return Err(format!("batch: unknown argument '{unknown}'")),
        }
        index += 1;
    }
    parsed.input_dir = input_dir.ok_or("batch: --input-dir <DIR> is required")?;
    if parsed.force && parsed.out_dir.is_none() {
        return Err("batch: --force requires --out-dir".to_string());
    }
    reject_profile_and_workflow("batch", &parsed.profile_id, &parsed.workflow_path)?;
    Ok(ParseAction::Execute(Command::Batch(parsed)))
}

fn reject_profile_and_workflow(
    command: &str,
    profile: &Option<String>,
    workflow: &Option<String>,
) -> Result<(), String> {
    if profile.is_some() && workflow.is_some() {
        Err(format!(
            "{command}: --profile and --workflow are mutually exclusive"
        ))
    } else {
        Ok(())
    }
}

fn parse_workflow(args: &[String]) -> Result<ParseAction, String> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        return Ok(ParseAction::Help(WORKFLOW_HELP));
    }
    match args {
        [action] if action == "schema" => Ok(ParseAction::Execute(Command::Workflow(
            WorkflowArgs::Schema,
        ))),
        [action] if action == "template" => Ok(ParseAction::Execute(Command::Workflow(
            WorkflowArgs::Template,
        ))),
        [action, source] if action == "validate" => Ok(ParseAction::Execute(Command::Workflow(
            WorkflowArgs::Validate {
                source: source.clone(),
                json: false,
            },
        ))),
        [action, source, flag] if action == "validate" && flag == "--json" => Ok(
            ParseAction::Execute(Command::Workflow(WorkflowArgs::Validate {
                source: source.clone(),
                json: true,
            })),
        ),
        [action, source] if action == "install" => Ok(ParseAction::Execute(Command::Workflow(
            WorkflowArgs::Install {
                source: source.clone(),
                json: false,
            },
        ))),
        [action, source, flag] if action == "install" && flag == "--json" => Ok(
            ParseAction::Execute(Command::Workflow(WorkflowArgs::Install {
                source: source.clone(),
                json: true,
            })),
        ),
        [] => Err("workflow: expected schema, template, validate, or install".to_string()),
        [action, ..] => Err(format!("workflow: invalid arguments for '{action}'")),
    }
}

fn parse_profiles(args: &[String]) -> Result<ParseAction, String> {
    if args.first().map(String::as_str) == Some("show") {
        return match &args[1..] {
            [flag] if matches!(flag.as_str(), "--help" | "-h") => {
                Ok(ParseAction::Help(PROFILES_HELP))
            }
            [id] if !id.starts_with('-') => Ok(ParseAction::Execute(Command::Profiles(
                ProfilesArgs::Show { id: id.clone() },
            ))),
            [] => Err("profiles show: <ID> is required".to_string()),
            _ => Err("profiles show: expected exactly one profile ID".to_string()),
        };
    }

    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--help" | "-h" => return Ok(ParseAction::Help(PROFILES_HELP)),
            "--json" if !json => json = true,
            "--json" => return Err("profiles: --json may be specified only once".to_string()),
            unknown => return Err(format!("profiles: unknown argument '{unknown}'")),
        }
    }
    Ok(ParseAction::Execute(Command::Profiles(
        ProfilesArgs::List { json },
    )))
}

fn parse_engines(args: &[String]) -> Result<ParseAction, String> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        return Ok(ParseAction::Help(ENGINES_HELP));
    }
    let Some(action) = args.first().map(String::as_str) else {
        return Ok(ParseAction::Execute(Command::Engines(
            EnginesArgs::Status { json: false },
        )));
    };
    match action {
        "status" => match &args[1..] {
            [] => Ok(ParseAction::Execute(Command::Engines(
                EnginesArgs::Status { json: false },
            ))),
            [flag] if flag == "--json" => Ok(ParseAction::Execute(Command::Engines(
                EnginesArgs::Status { json: true },
            ))),
            _ => Err("engines status: expected only optional --json".to_string()),
        },
        "--json" if args.len() == 1 => Ok(ParseAction::Execute(Command::Engines(
            EnginesArgs::Status { json: true },
        ))),
        "install" => match &args[1..] {
            [id] => Ok(ParseAction::Execute(Command::Engines(
                EnginesArgs::Install {
                    id: normalize_engine_id(id)?,
                },
            ))),
            [] => Err("engines install: engine ID or 'paddle' is required".to_string()),
            _ => Err("engines install: expected exactly one engine ID".to_string()),
        },
        "uninstall" => {
            let mut id = None;
            let mut confirmed = false;
            for arg in &args[1..] {
                match arg.as_str() {
                    "--yes" if !confirmed => confirmed = true,
                    "--yes" => {
                        return Err(
                            "engines uninstall: --yes may be specified only once".to_string()
                        )
                    }
                    value if !value.starts_with('-') && id.is_none() => id = Some(value),
                    unknown => {
                        return Err(format!(
                            "engines uninstall: unexpected argument '{unknown}'"
                        ))
                    }
                }
            }
            let id = id.ok_or("engines uninstall: engine ID or 'paddle' is required")?;
            if !confirmed {
                return Err(
                    "engines uninstall: pass --yes to confirm removal of the managed Paddle bundle"
                        .to_string(),
                );
            }
            Ok(ParseAction::Execute(Command::Engines(
                EnginesArgs::Uninstall {
                    id: normalize_engine_id(id)?,
                },
            )))
        }
        unknown => Err(format!("engines: unknown command or argument '{unknown}'")),
    }
}

fn normalize_engine_id(value: &str) -> Result<String, String> {
    match value {
        "paddle" | "paddleocr-vl-full" | "paddleocr-vl-parser" => {
            Ok("paddleocr-vl-parser".to_string())
        }
        _ => Err(format!(
            "unknown engine '{value}'; expected 'paddle' or 'paddleocr-vl-parser'"
        )),
    }
}

fn next_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    let value = args
        .get(*index)
        .filter(|value| !value.starts_with('-') || value.as_str() == "-")
        .ok_or_else(|| format!("{flag} requires a value"))?;
    if value.is_empty() {
        return Err(format!("{flag} requires a non-empty value"));
    }
    Ok(value.clone())
}

fn set_once(slot: &mut Option<String>, value: String, flag: &str) -> Result<(), String> {
    if slot.replace(value).is_some() {
        return Err(format!("{flag} may be specified only once"));
    }
    Ok(())
}

fn insert_assignment(
    values: &mut HashMap<String, String>,
    assignment: &str,
    flag: &str,
    allow_empty_value: bool,
) -> Result<(), String> {
    let Some((key, value)) = assignment.split_once('=') else {
        return Err(format!("{flag} expects KEY=VALUE, got '{assignment}'"));
    };
    let key = key.trim();
    if key.is_empty() || (!allow_empty_value && value.trim().is_empty()) {
        return Err(format!("{flag} expects a non-empty key and value"));
    }
    if values.insert(key.to_string(), value.to_string()).is_some() {
        return Err(format!("{flag} repeats key '{key}'"));
    }
    Ok(())
}

fn normalize_input_interpretation(value: &str) -> Result<String, String> {
    match value {
        "document" => Ok("document".to_string()),
        "latex-project" | "latex_project" => Ok("latex_project".to_string()),
        "source-tree" | "source_tree" => Ok("source_tree".to_string()),
        _ => Err(format!(
            "--interpret-as expects document, latex-project, or source-tree; got '{value}'"
        )),
    }
}

fn reject_trailing(args: &[String], flag: &str) -> Result<(), String> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(format!("{flag} does not accept additional arguments"))
    }
}
