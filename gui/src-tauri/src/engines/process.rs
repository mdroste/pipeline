#[derive(Debug, Default)]
pub(super) struct InstallDrainStats {
    pub(super) bytes_read: u64,
    pub(super) bytes_emitted: usize,
    pub(super) truncated: bool,
}

use super::*;

pub(super) async fn drain_install_output<R, F>(
    mut reader: R,
    mut emit: F,
) -> Result<InstallDrainStats, String>
where
    R: tokio::io::AsyncRead + Unpin,
    F: FnMut(String),
{
    use tokio::io::AsyncReadExt as _;

    let mut stats = InstallDrainStats::default();
    let mut chunk = [0u8; 8192];
    let mut line = Vec::with_capacity(4096);
    let mut line_truncated = false;
    let mut truncation_reported = false;
    let flush = |line: &mut Vec<u8>,
                 line_truncated: &mut bool,
                 stats: &mut InstallDrainStats,
                 truncation_reported: &mut bool,
                 emit: &mut F| {
        while line.last() == Some(&b'\r') {
            line.pop();
        }
        if !line.is_empty() && stats.bytes_emitted < INSTALL_LOG_BYTES_PER_STREAM {
            let remaining = INSTALL_LOG_BYTES_PER_STREAM - stats.bytes_emitted;
            let count = line.len().min(remaining);
            if count > 0 {
                emit(String::from_utf8_lossy(&line[..count]).into_owned());
                stats.bytes_emitted += count;
            }
        }
        if *line_truncated || stats.bytes_emitted >= INSTALL_LOG_BYTES_PER_STREAM {
            stats.truncated = true;
            if !*truncation_reported {
                emit("[installer output truncated; remaining bytes were drained]".to_string());
                *truncation_reported = true;
            }
        }
        line.clear();
        *line_truncated = false;
    };

    loop {
        let count = reader
            .read(&mut chunk)
            .await
            .map_err(|error| format!("Failed to read installer output: {error}"))?;
        if count == 0 {
            break;
        }
        stats.bytes_read = stats.bytes_read.saturating_add(count as u64);
        for byte in &chunk[..count] {
            if *byte == b'\n' {
                flush(
                    &mut line,
                    &mut line_truncated,
                    &mut stats,
                    &mut truncation_reported,
                    &mut emit,
                );
            } else if line.len() < INSTALL_LOG_LINE_BYTES
                && stats.bytes_emitted < INSTALL_LOG_BYTES_PER_STREAM
            {
                line.push(*byte);
            } else {
                line_truncated = true;
            }
        }
    }
    if !line.is_empty() || line_truncated {
        flush(
            &mut line,
            &mut line_truncated,
            &mut stats,
            &mut truncation_reported,
            &mut emit,
        );
    }
    Ok(stats)
}

pub(super) async fn await_install_drains(
    stdout_task: &mut tokio::task::JoinHandle<Result<InstallDrainStats, String>>,
    stderr_task: &mut tokio::task::JoinHandle<Result<InstallDrainStats, String>>,
) -> Result<(), String> {
    (&mut *stdout_task)
        .await
        .map_err(|error| format!("Installer stdout task failed: {error}"))??;
    (&mut *stderr_task)
        .await
        .map_err(|error| format!("Installer stderr task failed: {error}"))??;
    Ok(())
}

pub(super) async fn wait_install_drains_finished(
    stdout_task: &tokio::task::JoinHandle<Result<InstallDrainStats, String>>,
    stderr_task: &tokio::task::JoinHandle<Result<InstallDrainStats, String>>,
) {
    while !stdout_task.is_finished() || !stderr_task.is_finished() {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

pub(super) async fn finish_install_drains(
    pid: u32,
    stdout_task: &mut tokio::task::JoinHandle<Result<InstallDrainStats, String>>,
    stderr_task: &mut tokio::task::JoinHandle<Result<InstallDrainStats, String>>,
    initial_grace: std::time::Duration,
    post_kill_grace: std::time::Duration,
) -> Result<(), String> {
    if tokio::time::timeout(
        initial_grace,
        wait_install_drains_finished(stdout_task, stderr_task),
    )
    .await
    .is_ok()
    {
        return await_install_drains(stdout_task, stderr_task).await;
    }

    if pid > 0 {
        crate::commands::kill_process(pid);
    }
    if tokio::time::timeout(
        post_kill_grace,
        wait_install_drains_finished(stdout_task, stderr_task),
    )
    .await
    .is_err()
    {
        stdout_task.abort();
        stderr_task.abort();
        return Err("Installer output did not close after terminating descendants".to_string());
    }
    await_install_drains(stdout_task, stderr_task).await?;
    Err("Installer descendants kept output pipes open after the command exited".to_string())
}

pub(super) fn unregister_install_pid(pid: u32) {
    if pid == 0 {
        return;
    }
    crate::commands::unregister_engine_child_pid(pid);
    let mut active = INSTALL_CHILD_PID
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if *active == Some(pid) {
        *active = None;
    }
}

pub(super) async fn run_install_step(
    app: &crate::emit::EventBus,
    program: &Path,
    args: &[String],
    environment: &[(String, String)],
    label: &str,
) -> Result<(), String> {
    let mut command = crate::pipeline::claude::build_silent_command(
        program
            .to_str()
            .ok_or_else(|| format!("{label} program path is not valid UTF-8"))?,
        None,
    );
    apply_managed_environment(command.as_std_mut(), environment);
    crate::pipeline::claude::configure_silent_command(command.as_std_mut());
    command.args(args);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("Failed to start {label}: {error}"))?;
    let pid = child.id().unwrap_or(0);
    if pid > 0 {
        // Engine-scoped tracking only: the installer must survive the end or
        // cancellation of an unrelated pipeline run (`kill_all_children`).
        crate::commands::register_engine_child_pid(pid);
        *INSTALL_CHILD_PID
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(pid);
    }

    let stdout = child
        .stdout
        .take()
        .ok_or("Installer stdout pipe was unavailable")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Installer stderr pipe was unavailable")?;
    let stdout_app = app.clone();
    let mut stdout_task = tokio::spawn(async move {
        drain_install_output(stdout, |line| {
            if !line.trim().is_empty() {
                log(&stdout_app, line);
            }
        })
        .await
    });
    let stderr_app = app.clone();
    let mut stderr_task = tokio::spawn(async move {
        drain_install_output(stderr, |line| {
            if !line.trim().is_empty() {
                log(&stderr_app, line);
            }
        })
        .await
    });

    let waited = tokio::time::timeout(
        std::time::Duration::from_secs(INSTALL_STEP_TIMEOUT_SECS),
        child.wait(),
    )
    .await;
    let timed_out = waited.is_err();
    let (status, wait_error) = match waited {
        Ok(Ok(status)) => (Some(status), None),
        Ok(Err(error)) => (None, Some(format!("Failed waiting for {label}: {error}"))),
        Err(_) => {
            if pid > 0 {
                crate::commands::kill_process(pid);
            }
            let _ = child.start_kill();
            (
                tokio::time::timeout(std::time::Duration::from_secs(5), child.wait())
                    .await
                    .ok()
                    .and_then(Result::ok),
                None,
            )
        }
    };
    let drain_result = finish_install_drains(
        pid,
        &mut stdout_task,
        &mut stderr_task,
        std::time::Duration::from_secs(INSTALL_OUTPUT_GRACE_SECS),
        std::time::Duration::from_secs(INSTALL_OUTPUT_POST_KILL_SECS),
    )
    .await;
    unregister_install_pid(pid);
    drain_result.map_err(|error| format!("{label}: {error}"))?;
    if timed_out {
        return Err(format!(
            "{label} timed out after {INSTALL_STEP_TIMEOUT_SECS}s"
        ));
    }
    if let Some(error) = wait_error {
        return Err(error);
    }
    if INSTALL_CANCEL.load(Ordering::Acquire) {
        return Err("Installation cancelled".to_string());
    }
    let status = status.ok_or_else(|| format!("{label} did not report an exit status"))?;
    if !status.success() {
        return Err(format!(
            "{label} failed (exit {}). Check the install log for details.",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}
