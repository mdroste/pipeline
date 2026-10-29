//! Headless CLI for Pipeline.
//!
//! The CLI captures the same settings/profile snapshot and runs the same
//! dependency preflight and execution engine as the desktop app. Progress is
//! printed to stderr; a single report goes to stdout or `--out`.

// Helpers stay outside src/bin: the Tauri bundler treats src/bin/<dir>
// as another binary, even when it only contains private source modules.
#[path = "../cli/args.rs"]
mod args;
#[path = "../cli/engines.rs"]
mod engines;
#[path = "../cli/help.rs"]
mod help;
#[path = "../cli/interrupt.rs"]
mod interrupt;
#[path = "../cli_io.rs"]
mod io;
#[path = "../cli/profiles.rs"]
mod profiles;
#[path = "../cli/run.rs"]
mod run;
#[path = "../cli/workflow.rs"]
mod workflow;

use args::{parse_cli, Command, ParseAction};
use engines::cmd_engines;
use io::{emit_stdout, output_exit_code};
use profiles::cmd_profiles;
use run::{cmd_batch, cmd_check, cmd_run};
use workflow::cmd_workflow;

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
            emit_stdout(help.trim_end_matches('\n'));
            std::process::exit(output_exit_code(0));
        }
        ParseAction::Version => {
            emit_stdout(&format!("pipeline-cli {}", env!("CARGO_PKG_VERSION")));
            std::process::exit(output_exit_code(0));
        }
        ParseAction::Execute(command) => command,
    };

    pipeline_gui_lib::env::initialize_resources(None);
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
    std::process::exit(output_exit_code(code));
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

#[cfg(test)]
#[path = "../cli_tests.rs"]
mod tests;
