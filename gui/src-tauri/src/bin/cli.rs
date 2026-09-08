//! Headless CLI for Pipeline.
//!
//! The CLI captures the same settings/profile snapshot and runs the same
//! dependency preflight and execution engine as the desktop app. Progress is
//! printed to stderr; a single report goes to stdout or `--out`.

use pipeline_gui_lib::emit::{CliEvents, EventBus};
use pipeline_gui_lib::{commands, engines, pipeline_config};
use std::collections::{HashMap, HashSet};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAIN_HELP: &str = "\
Pipeline CLI

USAGE:
  pipeline-cli run [OPTIONS]
  pipeline-cli check [OPTIONS]
  pipeline-cli batch --input-dir <DIR> [OPTIONS]
  pipeline-cli workflow <schema|template|validate|install> ...
  pipeline-cli profiles [--json]
  pipeline-cli profiles show <ID>
  pipeline-cli engines [status] [--json]
  pipeline-cli engines install paddle
  pipeline-cli engines uninstall paddle --yes

COMMANDS:
  run       Run one workflow
  check     Check dependencies for one prospective run
  batch     Run one workflow over each supported document in a folder
  workflow  Author, validate, or explicitly install portable workflow JSON
  profiles  List or inspect installed workflows
  engines   Inspect or manage the PaddleOCR-VL Full Parser bundle
  help      Show help for Pipeline or one command

Run `pipeline-cli help <COMMAND>` for command-specific help.
The CLI uses settings and profiles from ~/.pipeline/ and saves every run there.
";

const RUN_HELP: &str = "\
Run one Pipeline workflow

USAGE:
  pipeline-cli run [--input <PATH>] [--interpret-as <KIND>]
                       [--profile <ID> | --workflow <FILE|->]
                       [--var <KEY=VALUE>]...
                       [--extra-input <KEY=PATH>]... [--out <FILE>] [--force]

OPTIONS:
  -i, --input <PATH>          Primary document or folder. Omit only for a
                              workflow configured with no primary input.
      --interpret-as <KIND>   document, latex-project, or source-tree.
  -p, --profile <ID>          Workflow for this run. Does not change the
                              desktop app's active workflow.
  -w, --workflow <FILE|->     Portable workflow JSON file, or '-' for stdin.
                              Runs ephemerally and is not installed.
      --var <KEY=VALUE>       Workflow variable; repeat for multiple values.
      --extra-input <KEY=PATH>
                              Named input; repeat for multiple inputs.
  -o, --out <FILE>            Write Markdown to FILE instead of stdout.
      --force                 Replace an existing --out file.
  -h, --help                  Show this help.

Progress goes to stderr. A non-zero exit status indicates invalid arguments,
failed preflight, interruption, or one or more failed workflow steps.
";

const BATCH_HELP: &str = "\
Run one Pipeline workflow over a folder of documents

USAGE:
  pipeline-cli batch --input-dir <DIR> [--profile <ID> | --workflow <FILE|->]
                     [--var <KEY=VALUE>]...
                     [--extra-input <KEY=PATH>]... [--out-dir <DIR>] [--force]

OPTIONS:
  -i, --input-dir <DIR>       Folder containing PDF, TeX, or DOCX inputs.
  -p, --profile <ID>          Workflow for this batch. Does not change the
                              desktop app's active workflow.
  -w, --workflow <FILE|->     Portable workflow JSON file, or '-' for stdin.
      --var <KEY=VALUE>       Workflow variable; repeat for multiple values.
      --extra-input <KEY=PATH>
                              Named input shared by every run; repeatable.
  -o, --out-dir <DIR>         Also write each Markdown report to DIR.
      --force                 Replace existing reports in --out-dir.
  -h, --help                  Show this help.

Folder discovery is non-recursive and skips hidden files. Every run is also
saved in Pipeline's normal run store under ~/.pipeline/runs/.
";

const CHECK_HELP: &str = "\
Check dependencies for one prospective Pipeline run

USAGE:
  pipeline-cli check [--input <PATH>] [--interpret-as <KIND>]
                     [--profile <ID> | --workflow <FILE|->]
                     [--var <KEY=VALUE>]...
                     [--extra-input <KEY=PATH>]... [--json]

OPTIONS:
  -i, --input <PATH>          Primary document or folder. For a PDF, this
                              checks the workflow's selected PDF parser.
      --interpret-as <KIND>   document, latex-project, or source-tree.
  -p, --profile <ID>          Workflow to check without changing the desktop
                              app's active workflow.
  -w, --workflow <FILE|->     Portable workflow JSON file, or '-' for stdin.
      --var <KEY=VALUE>       Workflow variable; repeatable.
      --extra-input <KEY=PATH>
                              Named input; repeatable. Named PDFs also trigger
                              the selected PDF-parser dependency check.
      --json                  Print the complete dependency report as JSON.
  -h, --help                  Show this help.

No extraction or model call is made. With --json, output includes the
normalized workflow hash, scheduler waves, work bounds, and dependencies.
Exit status 0 means the exact run is
ready; exit status 1 means one or more required dependencies are unavailable.
";

const WORKFLOW_HELP: &str = "\
Author and use portable Pipeline workflow JSON

USAGE:
  pipeline-cli workflow schema
  pipeline-cli workflow template
  pipeline-cli workflow validate <FILE|-> [--json]
  pipeline-cli workflow install <FILE|-> [--json]

COMMANDS:
  schema    Print the current JSON Schema.
  template  Print a small valid workflow agents can modify.
  validate  Strictly parse and semantically validate without installing.
  install   Validate, then save as a new installed profile.

Unknown fields are rejected. '-' reads at most 10 MB of UTF-8 JSON from
stdin. `run`, `check`, and `batch` use --workflow ephemerally; only this
explicit install command changes the profile store.
";

const PROFILES_HELP: &str = "\
List or inspect Pipeline workflows

USAGE:
  pipeline-cli profiles [--json]
  pipeline-cli profiles show <ID>

OPTIONS:
      --json  Print the profile-summary list as JSON.
  -h, --help  Show this help.

The active desktop workflow is marked with `*` in the human-readable list.
`profiles show` prints the workflow's current export JSON.
";

const ENGINES_HELP: &str = "\
Inspect or manage Pipeline's local PaddleOCR-VL Full Parser bundle

USAGE:
  pipeline-cli engines [status] [--json]
  pipeline-cli engines install paddle
  pipeline-cli engines uninstall paddle --yes

COMMANDS:
  status      Show availability, installation state, version, and disk usage.
  install     Install or repair the checksum-pinned managed bundle.
  uninstall   Remove both app-owned Paddle bundle roots. Requires --yes.

`paddle` is an alias for the registry ID `paddleocr-vl-parser`. Installation
downloads roughly 2.9 GB and requires roughly 3.8 GB after installation. All
files remain under ~/.pipeline/native/; no system Python, pip, Conda, Docker,
or llama.cpp installation is used.
";

#[derive(Debug)]
#[allow(clippy::large_enum_variant)] // One short-lived parse result per CLI process.
enum ParseAction {
    Execute(Command),
    Help(&'static str),
    Version,
}

#[derive(Debug)]
enum Command {
    Run(RunArgs),
    Check(CheckArgs),
    Batch(BatchArgs),
    Workflow(WorkflowArgs),
    Profiles(ProfilesArgs),
    Engines(EnginesArgs),
}

#[derive(Debug, Default, PartialEq, Eq)]
struct RunArgs {
    input: Option<String>,
    input_interpretation: Option<String>,
    profile_id: Option<String>,
    workflow_path: Option<String>,
    variables: HashMap<String, String>,
    extra_inputs: HashMap<String, String>,
    out: Option<String>,
    force: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct BatchArgs {
    input_dir: String,
    profile_id: Option<String>,
    workflow_path: Option<String>,
    variables: HashMap<String, String>,
    extra_inputs: HashMap<String, String>,
    out_dir: Option<String>,
    force: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct CheckArgs {
    input: Option<String>,
    input_interpretation: Option<String>,
    profile_id: Option<String>,
    workflow_path: Option<String>,
    variables: HashMap<String, String>,
    extra_inputs: HashMap<String, String>,
    json: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum ProfilesArgs {
    List { json: bool },
    Show { id: String },
}

#[derive(Debug, PartialEq, Eq)]
enum WorkflowArgs {
    Schema,
    Template,
    Validate { source: String, json: bool },
    Install { source: String, json: bool },
}

#[derive(Debug, PartialEq, Eq)]
enum EnginesArgs {
    Status { json: bool },
    Install { id: String },
    Uninstall { id: String },
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let action = match parse_cli(&args) {
        Ok(action) => action,
        Err(error) => {
            eprintln!("error: {error}\n\nTry `pipeline-cli --help` for usage.");
            std::process::exit(2);
        }
    };

    let command = match action {
        ParseAction::Help(help) => {
            print!("{help}");
            return;
        }
        ParseAction::Version => {
            println!("pipeline-cli {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        ParseAction::Execute(command) => command,
    };

    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(error) => {
            eprintln!("Failed to start runtime: {error}");
            std::process::exit(1);
        }
    };
    let code = rt.block_on(async {
        let code = dispatch(command).await;
        pipeline_gui_lib::pipeline::codex_server::shutdown().await;
        code
    });
    std::process::exit(code);
}

fn parse_cli(args: &[String]) -> Result<ParseAction, String> {
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

async fn dispatch(command: Command) -> i32 {
    match command {
        Command::Run(args) => cmd_run(args).await,
        Command::Check(args) => cmd_check(args).await,
        Command::Batch(args) => cmd_batch(args).await,
        Command::Workflow(args) => cmd_workflow(args),
        Command::Profiles(args) => cmd_profiles(args),
        Command::Engines(args) => cmd_engines(args).await,
    }
}

#[cfg(unix)]
async fn shutdown_signal() {
    use std::future::Future as _;
    use tokio::signal::unix::{signal, SignalKind};
    match signal(SignalKind::terminate()) {
        Ok(mut terminate) => {
            let mut ctrl_c = std::pin::pin!(tokio::signal::ctrl_c());
            let mut terminate = std::pin::pin!(terminate.recv());
            std::future::poll_fn(|cx| {
                if ctrl_c.as_mut().poll(cx).is_ready() || terminate.as_mut().poll(cx).is_ready() {
                    std::task::Poll::Ready(())
                } else {
                    std::task::Poll::Pending
                }
            })
            .await;
        }
        Err(_) => {
            let _ = tokio::signal::ctrl_c().await;
        }
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

async fn run_with_interrupt(
    bus: EventBus,
    options: commands::HeadlessRunOptions,
) -> Result<serde_json::Value, String> {
    use std::future::Future as _;
    let mut run = std::pin::pin!(commands::run_headless_with_options(bus, options));
    let mut signal = std::pin::pin!(shutdown_signal());
    let completed = std::future::poll_fn(|cx| {
        if let std::task::Poll::Ready(result) = run.as_mut().poll(cx) {
            return std::task::Poll::Ready(Some(result));
        }
        if signal.as_mut().poll(cx).is_ready() {
            return std::task::Poll::Ready(None);
        }
        std::task::Poll::Pending
    })
    .await;
    match completed {
        Some(result) => result,
        None => {
            eprintln!("Interrupt received; cancelling the active run…");
            let _ = commands::cancel_pipeline().await;
            match tokio::time::timeout(std::time::Duration::from_secs(15), run.as_mut()).await {
                Ok(_) => Err("Pipeline cancelled by interrupt".to_string()),
                Err(_) => Err(
                    "Pipeline cancellation timed out; child processes were terminated".to_string(),
                ),
            }
        }
    }
}

fn is_interruption(error: &str) -> bool {
    commands::is_pipeline_cancellation_error(error)
        || matches!(
            error,
            "Pipeline cancelled by interrupt"
                | "Pipeline cancellation timed out; child processes were terminated"
                | "Engine installation cancelled by interrupt"
                | "Engine installation cancellation timed out; child processes were terminated"
                | "Install cancelled"
                | "Installation cancelled"
        )
}

async fn cmd_run(args: RunArgs) -> i32 {
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
    let options = commands::HeadlessRunOptions {
        profile_id: args.profile_id,
        workflow,
        input_path: args.input.unwrap_or_default(),
        input_interpretation: args.input_interpretation,
        variables: args.variables,
        extra_inputs: args.extra_inputs,
    };
    let bus: EventBus = Arc::new(CliEvents);
    match run_with_interrupt(bus, options).await {
        Ok(value) => finish_single_run(&value, args.out.as_deref(), args.force),
        Err(error) => report_run_error(&error),
    }
}

async fn cmd_check(args: CheckArgs) -> i32 {
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
    let options = commands::HeadlessRunOptions {
        profile_id: args.profile_id,
        workflow,
        input_path: args.input.unwrap_or_default(),
        input_interpretation: args.input_interpretation,
        variables: args.variables,
        extra_inputs: args.extra_inputs,
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
            Ok(encoded) => println!("{encoded}"),
            Err(error) => {
                eprintln!("Error: failed to encode dependency report: {error}");
                return 1;
            }
        }
    } else {
        println!("Workflow: {}", plan.profile_name);
        println!("Fingerprint: {}", plan.workflow_fingerprint);
        println!(
            "Plan: {} stages, up to {} step units and {} provider attempts",
            plan.stages.len(),
            plan.budget.step_units_upper_bound,
            plan.budget.provider_attempts_upper_bound
        );
        let required = plan
            .readiness
            .deps
            .iter()
            .filter(|dependency| dependency.required)
            .collect::<Vec<_>>();
        if required.is_empty() {
            println!("No external dependencies are required for this run.");
        }
        for dependency in required {
            let ready = pipeline_gui_lib::deps::dependency_ready(dependency);
            let marker = if ready { "ok" } else { "missing" };
            let detail = if !dependency.version.is_empty() {
                format!(" ({})", dependency.version)
            } else {
                String::new()
            };
            println!("[{marker}] {}{detail}", dependency.name);
            if !ready {
                println!("  {}", dependency.hint);
            }
        }
        println!(
            "Dependency check: {}",
            if plan.readiness.ready {
                "ready"
            } else {
                "not ready"
            }
        );
    }
    if plan.readiness.ready {
        0
    } else {
        1
    }
}

fn finish_single_run(value: &serde_json::Value, out: Option<&str>, force: bool) -> i32 {
    let markdown = match report_markdown(value) {
        Ok(markdown) => markdown,
        Err(error) => {
            eprintln!("Error: {error}");
            return 1;
        }
    };
    match out {
        Some("-") => println!("{markdown}"),
        Some(path) => {
            if let Err(error) = write_report_file(Path::new(path), markdown, force) {
                eprintln!("Error: {error}");
                return 1;
            }
            eprintln!("Wrote report to {path}");
        }
        None => println!("{markdown}"),
    }
    if let Some(run_id) = value.get("run_id").and_then(|id| id.as_str()) {
        eprintln!("Run: {run_id}");
    }
    let failed = failed_step_count(value);
    if failed > 0 {
        eprintln!("{failed} step(s) failed");
        1
    } else {
        0
    }
}

fn report_markdown(value: &serde_json::Value) -> Result<&str, String> {
    value
        .get("markdown")
        .and_then(|markdown| markdown.as_str())
        .filter(|markdown| !markdown.is_empty())
        .ok_or_else(|| "Pipeline returned no Markdown report".to_string())
}

fn failed_step_count(value: &serde_json::Value) -> usize {
    value
        .get("report")
        .and_then(|report| report.get("failed_steps"))
        .and_then(|failed| failed.as_array())
        .map(Vec::len)
        .unwrap_or(0)
}

fn report_run_error(error: &str) -> i32 {
    eprintln!("Error: {error}");
    if is_interruption(error) {
        130
    } else {
        1
    }
}

async fn cmd_batch(args: BatchArgs) -> i32 {
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
    let config = if let Some(workflow) = workflow.as_ref() {
        workflow.document.config.clone()
    } else {
        let profile_id = selected_profile
            .clone()
            .unwrap_or_else(pipeline_config::get_active_profile_id);
        match pipeline_config::load_required_profile_for(&profile_id) {
            Ok((config, _)) => config,
            Err(error) => {
                eprintln!("Error: {error}");
                return 2;
            }
        }
    };
    if config.extraction.input_mode.trim() == "none" {
        eprintln!("Error: batch processing requires a workflow that accepts input");
        return 2;
    }

    let files = match commands::scan_input_files(&args.input_dir) {
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
            extra_inputs: args.extra_inputs.clone(),
        };
        match run_with_interrupt(bus, options).await {
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

fn prepare_batch_outputs(
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

fn validate_output_path(path: &Path, force: bool) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "{} is a symbolic link; choose a regular output path",
            path.display()
        )),
        Ok(metadata) if !metadata.is_file() => {
            Err(format!("{} is not a regular file", path.display()))
        }
        Ok(_) if !force => Err(format!(
            "{} already exists; pass --force to replace it",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                if !parent.is_dir() {
                    return Err(format!(
                        "output directory {} does not exist or is not a directory",
                        parent.display()
                    ));
                }
            }
            Ok(())
        }
        Err(error) => Err(format!(
            "Could not inspect output path {}: {error}",
            path.display()
        )),
    }
}

fn write_report_file(path: &Path, markdown: &str, force: bool) -> Result<(), String> {
    if force {
        return std::fs::write(path, markdown)
            .map_err(|error| format!("Failed to write {}: {error}", path.display()));
    }
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                format!(
                    "{} already exists; pass --force to replace it",
                    path.display()
                )
            } else {
                format!("Failed to create {}: {error}", path.display())
            }
        })?;
    output
        .write_all(markdown.as_bytes())
        .map_err(|error| format!("Failed to write {}: {error}", path.display()))
}

const MAX_WORKFLOW_BYTES: usize = 10_000_000;

fn read_workflow_source(source: &str) -> Result<String, String> {
    let mut bytes = Vec::new();
    if source == "-" {
        std::io::stdin()
            .lock()
            .take(MAX_WORKFLOW_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Failed to read workflow from stdin: {error}"))?;
    } else {
        let file = pipeline_gui_lib::safety::open_regular_file(Path::new(source))
            .map_err(|error| format!("Failed to read workflow '{source}': {error}"))?;
        file.take(MAX_WORKFLOW_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Failed to read workflow '{source}': {error}"))?;
    }
    if bytes.len() > MAX_WORKFLOW_BYTES {
        return Err("Workflow exceeds the 10 MB safety limit".to_string());
    }
    String::from_utf8(bytes).map_err(|error| format!("Workflow is not valid UTF-8: {error}"))
}

fn load_cli_workflow(source: &str) -> Result<commands::HeadlessWorkflow, String> {
    let content = read_workflow_source(source)?;
    let document = pipeline_config::parse_workflow_document_strict(&content)?;
    let source = if source == "-" {
        "stdin".to_string()
    } else {
        Path::new(source)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(source))
            .to_string_lossy()
            .to_string()
    };
    Ok(commands::HeadlessWorkflow { source, document })
}

fn workflow_validation_summary(workflow: &commands::HeadlessWorkflow) -> serde_json::Value {
    let agents = workflow
        .document
        .config
        .steps
        .iter()
        .flat_map(|step| step.agents.iter())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    serde_json::json!({
        "valid": true,
        "schema_version": workflow.document.schema_version,
        "name": workflow.document.name,
        "fingerprint": workflow.document.fingerprint,
        "step_count": workflow.document.config.steps.len(),
        "agents": agents,
        "source": workflow.source,
    })
}

fn cmd_workflow(args: WorkflowArgs) -> i32 {
    match args {
        WorkflowArgs::Schema => {
            match serde_json::to_string_pretty(&pipeline_config::workflow_json_schema()) {
                Ok(schema) => {
                    println!("{schema}");
                    0
                }
                Err(error) => {
                    eprintln!("Error: failed to encode workflow schema: {error}");
                    1
                }
            }
        }
        WorkflowArgs::Template => match pipeline_config::workflow_template() {
            Ok(template) => {
                print!("{}", template.canonical_json);
                0
            }
            Err(error) => {
                eprintln!("Error: {error}");
                1
            }
        },
        WorkflowArgs::Validate { source, json } => {
            let workflow = match load_cli_workflow(&source) {
                Ok(workflow) => workflow,
                Err(error) => {
                    if json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({
                                "valid": false,
                                "error": error,
                            }))
                            .expect("workflow validation error is serializable")
                        );
                    } else {
                        eprintln!("Error: {error}");
                    }
                    return 1;
                }
            };
            let summary = workflow_validation_summary(&workflow);
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&summary)
                        .expect("workflow validation summary is serializable")
                );
            } else {
                println!(
                    "Valid workflow '{}' ({} steps, {})",
                    workflow.document.name,
                    workflow.document.config.steps.len(),
                    workflow.document.fingerprint
                );
            }
            0
        }
        WorkflowArgs::Install { source, json } => {
            let workflow = match load_cli_workflow(&source) {
                Ok(workflow) => workflow,
                Err(error) => {
                    if json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({
                                "installed": false,
                                "error": error,
                            }))
                            .expect("workflow install error is serializable")
                        );
                    } else {
                        eprintln!("Error: {error}");
                    }
                    return 1;
                }
            };
            let installed = match pipeline_config::install_workflow_document(&workflow.document) {
                Ok(installed) => installed,
                Err(error) => {
                    if json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({
                                "installed": false,
                                "error": error,
                            }))
                            .expect("workflow install error is serializable")
                        );
                    } else {
                        eprintln!("Error: {error}");
                    }
                    return 1;
                }
            };
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "installed": true,
                        "id": installed.id,
                        "name": installed.name,
                        "step_count": installed.step_count,
                        "fingerprint": workflow.document.fingerprint,
                    }))
                    .expect("workflow install summary is serializable")
                );
            } else {
                println!(
                    "Installed workflow '{}' as {}",
                    installed.name, installed.id
                );
            }
            0
        }
    }
}

fn cmd_profiles(args: ProfilesArgs) -> i32 {
    match args {
        ProfilesArgs::Show { id } => match pipeline_config::export_profile_data(&id) {
            Ok(profile) => {
                println!("{profile}");
                0
            }
            Err(error) => {
                eprintln!("Error: {error}");
                1
            }
        },
        ProfilesArgs::List { json } => match pipeline_config::list_profiles() {
            Ok(profiles) if profiles.is_empty() => {
                eprintln!(
                    "Error: no profiles were found; Pipeline may not have been able to initialize ~/.pipeline/profiles"
                );
                1
            }
            Ok(profiles) if json => match serde_json::to_string_pretty(&profiles) {
                Ok(encoded) => {
                    println!("{encoded}");
                    0
                }
                Err(error) => {
                    eprintln!("Error: failed to encode profiles: {error}");
                    1
                }
            },
            Ok(profiles) => {
                let active = pipeline_config::get_active_profile_id();
                for profile in profiles {
                    let marker = if profile.id == active { "*" } else { " " };
                    println!(
                        "{marker} {:22} {} ({} steps)",
                        profile.id, profile.name, profile.step_count
                    );
                }
                0
            }
            Err(error) => {
                eprintln!("Error: {error}");
                1
            }
        },
    }
}

async fn cmd_engines(args: EnginesArgs) -> i32 {
    match args {
        EnginesArgs::Status { json } => cmd_engine_status(json).await,
        EnginesArgs::Install { id } => {
            let status = tokio::task::spawn_blocking(engines::engine_statuses)
                .await
                .ok()
                .and_then(|statuses| statuses.into_iter().find(|status| status.id == id));
            if let Some(status) = status.as_ref() {
                if !status.available {
                    eprintln!("Error: {}", status.unavailable_reason);
                    return 1;
                }
                eprintln!(
                    "{} {} (about {} MB download, {} MB installed)",
                    if status.installed {
                        "Repairing"
                    } else {
                        "Installing"
                    },
                    status.label,
                    status.est_download_mb,
                    status.est_disk_mb
                );
            }
            let bus: EventBus = Arc::new(CliEvents);
            match install_engine_with_interrupt(bus, &id).await {
                Ok(()) => {
                    eprintln!("PaddleOCR-VL Full Parser installation complete.");
                    0
                }
                Err(error) => report_run_error(&error),
            }
        }
        EnginesArgs::Uninstall { id } => {
            let bus: EventBus = Arc::new(CliEvents);
            match engines::uninstall_engine(&bus, &id).await {
                Ok(()) => {
                    eprintln!("PaddleOCR-VL Full Parser bundle removed.");
                    0
                }
                Err(error) => {
                    eprintln!("Error: {error}");
                    1
                }
            }
        }
    }
}

async fn cmd_engine_status(json: bool) -> i32 {
    let statuses = match tokio::task::spawn_blocking(engines::engine_statuses).await {
        Ok(statuses) => statuses,
        Err(error) => {
            eprintln!("Error: engine status task failed: {error}");
            return 1;
        }
    };
    if json {
        match serde_json::to_string_pretty(&statuses) {
            Ok(encoded) => println!("{encoded}"),
            Err(error) => {
                eprintln!("Error: failed to encode engine status: {error}");
                return 1;
            }
        }
        return 0;
    }
    for status in statuses {
        let state = if !status.available {
            "unavailable"
        } else if status.installing {
            "installing"
        } else if status.installed {
            "installed"
        } else {
            "not installed"
        };
        println!("{}: {state}", status.label);
        println!("  id: {}", status.id);
        if !status.version.is_empty() {
            println!("  version: {}", status.version);
        }
        if !status.entry_path.is_empty() {
            println!("  entry: {}", status.entry_path);
        }
        println!(
            "  estimated download/install: {} MB / {} MB",
            status.est_download_mb, status.est_disk_mb
        );
        println!("  managed stack on disk: {} MB", status.managed_stack_mb);
        if !status.unavailable_reason.is_empty() {
            println!("  reason: {}", status.unavailable_reason);
        }
    }
    0
}

async fn install_engine_with_interrupt(bus: EventBus, id: &str) -> Result<(), String> {
    use std::future::Future as _;
    let mut install = std::pin::pin!(engines::install_engine(&bus, id));
    let mut signal = std::pin::pin!(shutdown_signal());
    let completed = std::future::poll_fn(|cx| {
        if let std::task::Poll::Ready(result) = install.as_mut().poll(cx) {
            return std::task::Poll::Ready(Some(result));
        }
        if signal.as_mut().poll(cx).is_ready() {
            return std::task::Poll::Ready(None);
        }
        std::task::Poll::Pending
    })
    .await;
    match completed {
        Some(result) => result,
        None => {
            eprintln!("Interrupt received; cancelling the engine installation…");
            engines::cancel_install();
            match tokio::time::timeout(std::time::Duration::from_secs(15), install.as_mut()).await {
                Ok(_) => Err("Engine installation cancelled by interrupt".to_string()),
                Err(_) => Err(
                    "Engine installation cancellation timed out; child processes were terminated"
                        .to_string(),
                ),
            }
        }
    }
}

#[cfg(test)]
#[path = "cli/tests.rs"]
mod tests;
