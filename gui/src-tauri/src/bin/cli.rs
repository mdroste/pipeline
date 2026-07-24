//! Headless CLI for Pipeline.
//!
//! Reuses the same engine as the GUI (the pipeline depends only on an
//! `EventBus`, not a Tauri `AppHandle`), so runs happen without a window or
//! webview — suitable for cron jobs, CI, and scripting. Progress is printed to
//! stderr; the report goes to stdout or `--out`.
//!
//!   pipeline-cli run --input paper.pdf [--profile deep-review] [--var k=v]... [--extra-input k=path]... [--out report.md]
//!   pipeline-cli batch --input-dir ./papers [--profile grading] [--var k=v]... [--extra-input k=path]...
//!   pipeline-cli profiles

use pipeline_gui_lib::emit::{CliEvents, EventBus};
use pipeline_gui_lib::{commands, pipeline_config};
use std::collections::HashMap;
use std::sync::Arc;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to start runtime: {e}");
            std::process::exit(1);
        }
    };
    std::process::exit(rt.block_on(dispatch(&args)));
}

async fn dispatch(args: &[String]) -> i32 {
    match args.get(1).map(String::as_str) {
        Some("run") => cmd_run(&args[2..]).await,
        Some("batch") => cmd_batch(&args[2..]).await,
        Some("profiles") => cmd_profiles(),
        Some("help") | Some("--help") | Some("-h") | None => {
            print_help();
            0
        }
        Some(other) => {
            eprintln!("Unknown command: {other}\n");
            print_help();
            2
        }
    }
}

fn print_help() {
    eprintln!(
        "Pipeline CLI\n\n\
         USAGE:\n  \
         pipeline-cli run --input <file> [--profile <id>] [--var k=v]... [--extra-input k=path]... [--out <file>]\n  \
         pipeline-cli batch --input-dir <dir> [--profile <id>] [--var k=v]... [--extra-input k=path]...\n  \
         pipeline-cli profiles\n\n\
         Uses the same profiles and settings as the desktop app (~/.pipeline/).\n\
         Progress prints to stderr; the report prints to stdout unless --out is given.\n\
         Exit code is non-zero if any step fails."
    );
}

/// Value following `--name`, if present.
fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

/// All values for a repeatable `--name`.
fn repeated(args: &[String], name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == name {
            if let Some(v) = args.get(i + 1) {
                out.push(v.clone());
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

fn select_profile(args: &[String]) -> Result<(), i32> {
    if let Some(profile) = flag(args, "--profile") {
        if let Err(e) = pipeline_config::switch_profile(profile) {
            eprintln!("Failed to select profile '{profile}': {e}");
            return Err(2);
        }
    }
    Ok(())
}

fn key_values(args: &[String], name: &str) -> Result<HashMap<String, String>, String> {
    let mut values = HashMap::new();
    for value in repeated(args, name) {
        let Some((key, value)) = value.split_once('=') else {
            return Err(format!("{name} expects key=value, got '{value}'"));
        };
        if key.trim().is_empty() || value.trim().is_empty() {
            return Err(format!("{name} expects a non-empty key and value"));
        }
        values.insert(key.to_string(), value.to_string());
    }
    Ok(values)
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
    input: &str,
    vars: HashMap<String, String>,
    extra_inputs: HashMap<String, String>,
) -> Result<serde_json::Value, String> {
    use std::future::Future as _;
    let mut run = std::pin::pin!(commands::run_headless(bus, input, vars, extra_inputs));
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
    let error = error.to_ascii_lowercase();
    error.contains("cancel") || error.contains("interrupt")
}

async fn cmd_run(args: &[String]) -> i32 {
    let Some(input) = flag(args, "--input") else {
        eprintln!("run: --input <file> is required");
        return 2;
    };
    if let Err(code) = select_profile(args) {
        return code;
    }
    let vars = match key_values(args, "--var") {
        Ok(values) => values,
        Err(error) => {
            eprintln!("run: {error}");
            return 2;
        }
    };
    let extra_inputs = match key_values(args, "--extra-input") {
        Ok(values) => values,
        Err(error) => {
            eprintln!("run: {error}");
            return 2;
        }
    };

    let bus: EventBus = Arc::new(CliEvents);
    match run_with_interrupt(bus, input, vars, extra_inputs).await {
        Ok(v) => {
            let markdown = v.get("markdown").and_then(|m| m.as_str()).unwrap_or("");
            match flag(args, "--out") {
                Some(out) => {
                    if let Err(e) = std::fs::write(out, markdown) {
                        eprintln!("Failed to write {out}: {e}");
                        return 1;
                    }
                    eprintln!("Wrote report to {out}");
                }
                None => println!("{markdown}"),
            }
            if let Some(run_id) = v.get("run_id").and_then(|r| r.as_str()) {
                eprintln!("Run: {run_id}");
            }
            let failed = v
                .get("report")
                .and_then(|r| r.get("failed_steps"))
                .and_then(|f| f.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            if failed > 0 {
                eprintln!("{failed} step(s) failed");
                1
            } else {
                0
            }
        }
        Err(e) => {
            eprintln!("Error: {e}");
            if is_interruption(&e) {
                130
            } else {
                1
            }
        }
    }
}

async fn cmd_batch(args: &[String]) -> i32 {
    let Some(dir) = flag(args, "--input-dir") else {
        eprintln!("batch: --input-dir <dir> is required");
        return 2;
    };
    if let Err(code) = select_profile(args) {
        return code;
    }
    let vars = match key_values(args, "--var") {
        Ok(values) => values,
        Err(error) => {
            eprintln!("batch: {error}");
            return 2;
        }
    };
    let extra_inputs = match key_values(args, "--extra-input") {
        Ok(values) => values,
        Err(error) => {
            eprintln!("batch: {error}");
            return 2;
        }
    };
    let files = match scan_inputs(dir) {
        Ok(f) if !f.is_empty() => f,
        Ok(_) => {
            eprintln!("No .pdf, .tex, or .docx files found in {dir}");
            return 2;
        }
        Err(e) => {
            eprintln!("Cannot read {dir}: {e}");
            return 2;
        }
    };

    let mut failures = 0;
    for (i, path) in files.iter().enumerate() {
        eprintln!("\n[{}/{}] {path}", i + 1, files.len());
        let bus: EventBus = Arc::new(CliEvents);
        match run_with_interrupt(bus, path, vars.clone(), extra_inputs.clone()).await {
            Ok(v) => {
                if let Some(run_id) = v.get("run_id").and_then(|r| r.as_str()) {
                    eprintln!("  → {run_id}");
                }
                if v.get("status").and_then(|s| s.as_str()) == Some("partial") {
                    eprintln!("  failed: one or more pipeline steps failed");
                    failures += 1;
                }
            }
            Err(e) => {
                eprintln!("  failed: {e}");
                failures += 1;
                if is_interruption(&e) {
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

fn scan_inputs(dir: &str) -> std::io::Result<Vec<String>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        if matches!(ext.as_str(), "pdf" | "tex" | "docx") {
            files.push(path.to_string_lossy().replace('\\', "/"));
        }
    }
    files.sort();
    Ok(files)
}

fn cmd_profiles() -> i32 {
    match pipeline_config::list_profiles() {
        Ok(profiles) => {
            let active = pipeline_config::get_active_profile_id();
            for p in profiles {
                let marker = if p.id == active { "*" } else { " " };
                println!("{marker} {:22} {} ({} steps)", p.id, p.name, p.step_count);
            }
            0
        }
        Err(e) => {
            eprintln!("Error: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_value_flags_support_named_inputs_and_reject_malformed_values() {
        let args = vec![
            "--extra-input".to_string(),
            "rubric=/tmp/rubric.pdf".to_string(),
        ];
        assert_eq!(
            key_values(&args, "--extra-input").unwrap().get("rubric"),
            Some(&"/tmp/rubric.pdf".to_string())
        );
        assert!(key_values(&["--var".to_string(), "broken".to_string()], "--var").is_err());
    }

    #[test]
    fn interruption_errors_map_to_shell_interrupt_status() {
        assert!(is_interruption("Pipeline cancelled by interrupt"));
        assert!(is_interruption("Pipeline cancellation timed out"));
        assert!(!is_interruption("provider authentication failed"));
    }
}
