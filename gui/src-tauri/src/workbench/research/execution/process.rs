//! Authorized process launch, independent process groups, and bounded pipe draining.

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

pub(crate) fn run_profile_process(
    execution_id: &str,
    profile: &ExecutionProfile,
    authorized_launch: &Value,
) -> Result<ExecutionOutput, String> {
    let launch = launch_manifest(profile).map_err(|e| e.message)?;
    if &launch != authorized_launch {
        return Err("The launch program changed after authorization".into());
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
        .current_dir(&profile.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
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
    let mut child = command
        .spawn()
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
    let stdout_thread = std::thread::spawn(move || {
        stdout
            .map(|pipe| drain_capped(pipe, 4 * 1024 * 1024, &stdout_id, "stdout"))
            .unwrap_or_default()
    });
    let stderr_thread = std::thread::spawn(move || {
        stderr
            .map(|pipe| drain_capped(pipe, 4 * 1024 * 1024, &stderr_id, "stderr"))
            .unwrap_or_default()
    });
    let started = Instant::now();
    let mut wait_error = None;
    let mut stop_requested: Option<(Instant, &str)> = None;
    let mut wrapper_completed_after_stop = false;
    let (outcome, status) = loop {
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
            // oldstata has no signal trap. Let it return normally through Legacy Time Off.
            // Forced termination is explicitly recorded as requiring cleanup verification.
            if profile.adapter != "stata" || requested_at.elapsed() >= Duration::from_secs(30) {
                crate::commands::lifecycle::kill_independent_process(pid);
                let _ = child.wait();
                break (reason.to_string(), None);
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if let Some((_, reason)) = stop_requested {
                    wrapper_completed_after_stop = profile.adapter == "stata";
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
    let (stdout, stdout_truncated) = stdout_thread.join().unwrap_or_default();
    let (mut stderr, stderr_truncated) = stderr_thread.join().unwrap_or_default();
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
