//! Shared bounded runner for short-lived synchronous subprocess probes.
//!
//! Provider execution has an async streaming path. Startup, PATH, and managed
//! engine probes are synchronous by design, but must still have the same
//! timeout, output, process-tree, and PID cleanup guarantees.

use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct BoundedOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

fn configure_isolation(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
}

fn drain_capped<R: std::io::Read>(mut reader: R, limit: usize) -> (Vec<u8>, bool) {
    let mut kept = Vec::with_capacity(limit.min(16 * 1024));
    let mut truncated = false;
    let mut chunk = [0u8; 16 * 1024];
    loop {
        let count = match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(count) => count,
        };
        let remaining = limit.saturating_sub(kept.len());
        let take = remaining.min(count);
        kept.extend_from_slice(&chunk[..take]);
        truncated |= take < count;
    }
    (kept, truncated)
}

/// Run a command to completion with a wall-clock deadline and bounded capture.
/// Output continues to be drained after the cap so a full pipe cannot block the
/// child. On every non-normal path the child tree is terminated and reaped.
pub fn run_bounded(
    command: &mut Command,
    timeout: Duration,
    output_limit: usize,
) -> Result<BoundedOutput, String> {
    configure_isolation(command);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to start subprocess: {e}"))?;
    let pid = child.id();
    if pid > 0 {
        crate::commands::register_child_pid(pid);
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (stdout_tx, stdout_rx) = std::sync::mpsc::sync_channel(1);
    let (stderr_tx, stderr_rx) = std::sync::mpsc::sync_channel(1);
    let stdout_thread = std::thread::spawn(move || {
        let output = stdout
            .map(|pipe| drain_capped(pipe, output_limit))
            .unwrap_or_default();
        let _ = stdout_tx.send(output);
    });
    let stderr_thread = std::thread::spawn(move || {
        let output = stderr
            .map(|pipe| drain_capped(pipe, output_limit))
            .unwrap_or_default();
        let _ = stderr_tx.send(output);
    });

    let started = Instant::now();
    let result = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if started.elapsed() >= timeout => {
                if pid > 0 {
                    crate::commands::kill_process(pid);
                }
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!("Subprocess timed out after {}s", timeout.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(e) => {
                if pid > 0 {
                    crate::commands::kill_process(pid);
                }
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!("Failed waiting for subprocess: {e}"));
            }
        }
    };

    // A leader can exit after spawning a descendant that inherited its pipe.
    // Do not let that descendant make the probe join forever. Give normal EOF
    // a short grace period, then terminate the owned process tree.
    let drain_grace = Duration::from_secs(1);
    let mut stdout_result = stdout_rx.recv_timeout(drain_grace).ok();
    let mut stderr_result = stderr_rx.recv_timeout(drain_grace).ok();
    if stdout_result.is_none() || stderr_result.is_none() {
        if pid > 0 {
            crate::commands::kill_process(pid);
        }
        let _ = child.kill();
        let _ = child.wait();
        if stdout_result.is_none() {
            stdout_result = stdout_rx.recv_timeout(Duration::from_secs(2)).ok();
        }
        if stderr_result.is_none() {
            stderr_result = stderr_rx.recv_timeout(Duration::from_secs(2)).ok();
        }
    }
    if pid > 0 {
        crate::commands::unregister_child_pid(pid);
    }
    if stdout_result.is_some() {
        let _ = stdout_thread.join();
    }
    if stderr_result.is_some() {
        let _ = stderr_thread.join();
    }
    if stdout_result.is_none() || stderr_result.is_none() {
        return Err("Subprocess output pipes did not close after termination".to_string());
    }
    let (stdout, stdout_truncated) = stdout_result.unwrap_or_default();
    let (stderr, stderr_truncated) = stderr_result.unwrap_or_default();
    let status = result?;
    Ok(BoundedOutput {
        status,
        stdout,
        stderr,
        stdout_truncated,
        stderr_truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn caps_output_while_draining_to_exit() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "printf 123456789"]);
        let output = run_bounded(&mut command, Duration::from_secs(2), 4).unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"1234");
        assert!(output.stdout_truncated);
    }

    #[test]
    #[cfg(unix)]
    fn times_out_promptly() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "sleep 5"]);
        let started = Instant::now();
        let error = run_bounded(&mut command, Duration::from_millis(100), 1024).unwrap_err();
        assert!(error.contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
