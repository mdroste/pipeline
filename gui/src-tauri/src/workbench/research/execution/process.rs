//! Authorized process launch, independent process groups, and bounded pipe draining.

use super::adapters;
use super::launch::launch_manifest;
use super::types::{ExecutionOutput, ExecutionProfile};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

static EXECUTION_PIDS: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
static CANCELLED_EXECUTIONS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn execution_pids() -> &'static Mutex<HashMap<String, u32>> {
    EXECUTION_PIDS.get_or_init(|| Mutex::new(HashMap::new()))
}
pub(crate) fn cancelled_executions() -> &'static Mutex<HashSet<String>> {
    CANCELLED_EXECUTIONS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn drain_capped<R: Read>(mut reader: R, limit: usize, job: &str, stream: &str) -> (Vec<u8>, bool) {
    let mut kept = Vec::new();
    let mut truncated = false;
    let mut buffer = [0u8; 16 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(count) => {
                crate::workbench::research::jobs::append_log(job, stream, &buffer[..count]);
                let take = count.min(limit.saturating_sub(kept.len()));
                kept.extend_from_slice(&buffer[..take]);
                truncated |= take < count;
            }
        }
    }
    (kept, truncated)
}

/// On Windows, Python encodes a piped stdout in the ANSI code page and stops
/// at the first character outside it (`β`, `≤`, …), although the same script
/// prints fine in a console window. Pipeline reads this output as UTF-8, so
/// ask for UTF-8 stdio. Only stdio changes: a script's own `open()` calls keep
/// the system default, as in the researcher's terminal. A profile that sets
/// the variable itself keeps its value; `-E`/`-I` in its argv ignore both.
pub(super) fn python_stdio_default(
    environment: &Value,
    windows: bool,
) -> Option<(&'static str, &'static str)> {
    const KEY: &str = "PYTHONIOENCODING";
    let configured = environment
        .as_object()
        .is_some_and(|values| values.keys().any(|key| key.eq_ignore_ascii_case(KEY)));
    (windows && !configured).then_some((KEY, "utf-8"))
}

pub(crate) fn run_profile_process(
    execution_id: &str,
    profile: &ExecutionProfile,
    authorized_launch: &Value,
) -> Result<ExecutionOutput, String> {
    let launch = launch_manifest(profile).map_err(|e| e.message)?;
    if &launch != authorized_launch {
        return Err(
            "The launch program or its environment files changed after authorization; review the command again"
                .into(),
        );
    }
    let program = launch["invocationPath"]
        .as_str()
        .ok_or("Execution program was not resolved")?;
    let mut command = Command::new(program);
    let prefix: Vec<&str> = launch["prefixArgs"]
        .as_array()
        .ok_or("Missing launch arguments")?
        .iter()
        .map(|v| v.as_str().ok_or("Invalid launch argument"))
        .collect::<Result<_, _>>()?;
    command.args(prefix);
    // Multicall tools such as pdflatex -> pdftex select behavior from argv[0].
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.arg0(&profile.argv[0]);
    }
    command
        .args(&profile.argv[1..])
        // The saved cwd is canonical, which on Windows is a `\\?\C:\…` path.
        // cmd.exe refuses that as a working folder and runs in C:\Windows, so
        // latexmk, Stata `shell`, and Python `shell=True` would all start in
        // the wrong place. Containment was checked on the canonical form.
        .current_dir(crate::file_viewer::system_path(std::path::Path::new(
            &profile.cwd,
        )))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some((key, value)) = python_stdio_default(&profile.environment, cfg!(windows)) {
        command.env(key, value);
    }
    if let Some(environment) = profile.environment.as_object() {
        for (key, value) in environment {
            if let Some(value) = value.as_str() {
                command.env(key, value);
            }
        }
    }
    command.env(
        "PATH",
        launch["path"].as_str().ok_or("Missing execution PATH")?,
    );
    for secret in [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "GOOGLE_API_KEY",
        "AWS_SECRET_ACCESS_KEY",
        "GITHUB_TOKEN",
    ] {
        command.env_remove(secret);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(0x08000000);
    }
    let mut child = crate::commands::lifecycle::spawn_independent(&mut command)
        .map_err(|error| format!("Failed to start research execution: {error}"))?;
    let pid = child.id();
    crate::commands::lifecycle::register_independent_child_pid(pid);
    execution_pids()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(execution_id.to_string(), pid);
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_id = execution_id.to_string();
    let stderr_id = execution_id.to_string();
    let (stdout_tx, stdout_rx) = std::sync::mpsc::sync_channel(1);
    let (stderr_tx, stderr_rx) = std::sync::mpsc::sync_channel(1);
    let stdout_thread = std::thread::spawn(move || {
        let output = stdout
            .map(|pipe| drain_capped(pipe, 4 * 1024 * 1024, &stdout_id, "stdout"))
            .unwrap_or_default();
        let _ = stdout_tx.send(output);
    });
    let stderr_thread = std::thread::spawn(move || {
        let output = stderr
            .map(|pipe| drain_capped(pipe, 4 * 1024 * 1024, &stderr_id, "stderr"))
            .unwrap_or_default();
        let _ = stderr_tx.send(output);
    });
    let started = Instant::now();
    let waits_for_launcher = adapters::waits_for_stata_launcher(&profile.adapter, &profile.argv);
    let mut wait_error = None;
    let mut stop_requested: Option<(Instant, &str)> = None;
    let mut wrapper_completed_after_stop = false;
    let (mut outcome, status) = loop {
        if cancelled_executions()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .contains(execution_id)
        {
            stop_requested.get_or_insert((Instant::now(), "interrupted"));
        } else if started.elapsed() >= Duration::from_secs(profile.timeout_seconds) {
            stop_requested.get_or_insert((Instant::now(), "timed_out"));
        }
        if let Some((requested_at, reason)) = stop_requested {
            // A Stata custom launcher may restore machine state only on a normal
            // return (the oldstata clock wrapper has no signal trap), so it gets
            // 30 seconds before forced termination, which is recorded as
            // requiring cleanup verification. Direct Stata stops immediately.
            if !waits_for_launcher || requested_at.elapsed() >= Duration::from_secs(30) {
                crate::commands::lifecycle::kill_independent_process(pid);
                let _ = child.wait();
                break (reason.to_string(), None);
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if let Some((_, reason)) = stop_requested {
                    wrapper_completed_after_stop = waits_for_launcher;
                    break (reason.to_string(), status.code());
                }
                break (
                    (if status.success() {
                        "completed"
                    } else {
                        "failed"
                    })
                    .to_string(),
                    status.code(),
                );
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                crate::commands::lifecycle::kill_independent_process(pid);
                let _ = child.wait();
                wait_error = Some(format!(
                    "Failed while waiting for research execution: {error}"
                ));
                break ("outcome_unknown".to_string(), None);
            }
        }
    };
    execution_pids()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(execution_id);
    cancelled_executions()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(execution_id);
    // A completed launcher may leave descendants holding inherited output pipes.
    // Reap this execution's independent group before joining capture threads.
    crate::commands::lifecycle::kill_independent_process(pid);
    crate::commands::lifecycle::unregister_independent_child_pid(pid);
    // Even a failed OS cleanup must not strand the execution in a pipe join.
    let stdout_result = stdout_rx.recv_timeout(Duration::from_secs(2)).ok();
    let stderr_result = stderr_rx.recv_timeout(Duration::from_secs(2)).ok();
    if stdout_result.is_some() {
        let _ = stdout_thread.join();
    }
    if stderr_result.is_some() {
        let _ = stderr_thread.join();
    }
    if stdout_result.is_none() || stderr_result.is_none() {
        outcome = "outcome_unknown".into();
        wait_error = Some("Output pipes did not close after stopping the process tree. Check for a detached helper before running again.".into());
    }
    let (stdout, stdout_truncated) = stdout_result.unwrap_or_else(|| (Vec::new(), true));
    let (mut stderr, stderr_truncated) = stderr_result.unwrap_or_else(|| (Vec::new(), true));
    if let Some(error) = wait_error {
        stderr.extend_from_slice(error.as_bytes());
    }
    Ok(ExecutionOutput {
        outcome,
        status,
        stdout,
        stderr,
        stdout_truncated,
        stderr_truncated,
        wrapper_completed_after_stop,
        process_started: true,
    })
}
