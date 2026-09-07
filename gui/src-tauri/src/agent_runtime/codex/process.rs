//! Independently owned Codex process tree and isolated runtime environment.

use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout};
use tokio::sync::Mutex;

pub(crate) const SECRET_ENVIRONMENT_KEYS: &[&str] = &[
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "AZURE_OPENAI_API_KEY",
    "AZURE_OPENAI_ENDPOINT",
    "ANTHROPIC_API_KEY",
    "GOOGLE_API_KEY",
    "GEMINI_API_KEY",
    "CODEX_API_KEY",
];

pub(crate) struct ProcessPipes {
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
    pub stderr: ChildStderr,
}

pub(crate) struct OwnedProcess {
    pid: u32,
    child: Mutex<Child>,
    running: AtomicBool,
}

impl OwnedProcess {
    pub(crate) fn spawn(
        mut command: std::process::Command,
    ) -> Result<(Self, ProcessPipes), String> {
        crate::process::configure_isolation(&mut command);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = tokio::process::Command::from(command)
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| format!("Failed to start Codex App Server: {error}"))?;
        let pid = child
            .id()
            .filter(|pid| *pid > 0)
            .ok_or("Codex App Server did not report a process id")?;
        crate::commands::lifecycle::register_independent_child_pid(pid);
        let pipes = ProcessPipes {
            stdin: child.stdin.take().ok_or("Codex App Server has no stdin")?,
            stdout: child
                .stdout
                .take()
                .ok_or("Codex App Server has no stdout")?,
            stderr: child
                .stderr
                .take()
                .ok_or("Codex App Server has no stderr")?,
        };
        Ok((
            Self {
                pid,
                child: Mutex::new(child),
                running: AtomicBool::new(true),
            },
            pipes,
        ))
    }

    pub(crate) fn pid(&self) -> u32 {
        self.pid
    }

    pub(crate) async fn wait(&self) -> Result<ExitStatus, String> {
        let status = self
            .child
            .lock()
            .await
            .wait()
            .await
            .map_err(|error| format!("Failed waiting for Codex App Server: {error}"))?;
        self.finish_tracking();
        Ok(status)
    }

    pub(crate) async fn terminate(&self) {
        if !self.running.swap(false, Ordering::AcqRel) {
            return;
        }
        crate::commands::lifecycle::kill_independent_process(self.pid);
        let mut child = self.child.lock().await;
        let _ = child.kill().await;
        let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
        crate::commands::lifecycle::unregister_independent_child_pid(self.pid);
    }

    fn finish_tracking(&self) {
        if self.running.swap(false, Ordering::AcqRel) {
            crate::commands::lifecycle::unregister_independent_child_pid(self.pid);
        }
    }
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        if self.running.swap(false, Ordering::AcqRel) {
            crate::commands::lifecycle::kill_independent_process(self.pid);
            crate::commands::lifecycle::unregister_independent_child_pid(self.pid);
        }
    }
}

pub(crate) fn prepare_isolated_command(
    resolved: &crate::deps::ResolvedCommand,
    codex_home: &Path,
    cwd: &Path,
) -> Result<std::process::Command, String> {
    let mut command =
        resolved.canonical_command(["app-server", "--listen", "stdio://", "--strict-config"])?;
    command
        .current_dir(cwd)
        .env("CODEX_HOME", codex_home)
        .env("PATH", crate::env::full_path());
    for key in SECRET_ENVIRONMENT_KEYS {
        command.env_remove(key);
    }
    Ok(command)
}

pub(crate) async fn capture_stderr(mut stderr: ChildStderr) -> Vec<String> {
    const LIMIT: usize = 4 * 1024 * 1024;
    const TAIL_LINES: usize = 50;
    let mut tail = std::collections::VecDeque::new();
    let mut reader = tokio::io::BufReader::new(&mut stderr);
    let mut total = 0usize;
    while let Ok(Some(record)) =
        crate::pipeline::logging::next_bounded_line(&mut reader, 256 * 1024).await
    {
        total = total.saturating_add(record.text.len());
        if total > LIMIT {
            continue;
        }
        let line = if record.truncated {
            format!("{}… [line truncated]", record.text)
        } else {
            record.text
        };
        if !line.trim().is_empty() {
            if tail.len() == TAIL_LINES {
                tail.pop_front();
            }
            tail.push_back(line);
        }
    }
    tail.into_iter().collect()
}
