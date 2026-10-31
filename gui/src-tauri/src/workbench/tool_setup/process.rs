//! Install subprocesses retain bounded output and are killed on cancellation.
use std::{
    process::{Command, Stdio},
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncReadExt};

struct OwnedChild {
    child: tokio::process::Child,
    pid: u32,
    stopped: bool,
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.stopped {
            crate::commands::lifecycle::kill_independent_process(self.pid);
        }
        let _ = self.child.start_kill();
        crate::commands::lifecycle::unregister_independent_child_pid(self.pid);
    }
}
async fn drain(mut reader: impl AsyncRead + Unpin) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let n = reader.read(&mut buffer).await.map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(bytes);
        }
        bytes.extend_from_slice(&buffer[..n.min((64 * 1024usize).saturating_sub(bytes.len()))]);
    }
}
pub(super) async fn run(mut command: Command, label: &str) -> Result<Vec<u8>, String> {
    crate::process::configure_isolation(&mut command);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = tokio::process::Command::from(command)
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("{label}: {e}"))?;
    let pid = child.id().ok_or("Setup process has no ID")?;
    crate::commands::lifecycle::register_independent_child_pid(pid);
    let mut child = OwnedChild {
        child,
        pid,
        stopped: false,
    };
    let stdout = child.child.stdout.take().unwrap();
    let stderr = child.child.stderr.take().unwrap();
    let result = tokio::time::timeout(Duration::from_secs(300), async {
        let wait = async {
            let status = child.child.wait().await.map_err(|e| e.to_string())?;
            crate::commands::lifecycle::kill_independent_process(pid);
            child.stopped = true;
            Ok::<_, String>(status)
        };
        tokio::try_join!(wait, drain(stdout), drain(stderr))
    })
    .await
    .map_err(|_| format!("{label} timed out. Check your internet connection and retry."))??;
    if !result.0.success() {
        return Err(format!(
            "{label} failed. Check your internet connection and retry. {}",
            String::from_utf8_lossy(&result.2)
                .chars()
                .take(2000)
                .collect::<String>()
        ));
    }
    Ok(result.1)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancelling_setup_stops_its_background_children() {
        let dir = tempfile::tempdir().unwrap();
        let mut command = Command::new("/bin/sh");
        command.current_dir(dir.path()).args([
            "-c",
            "echo ready > started; (sleep 1; echo leaked > leaked) & wait",
        ]);
        let task = tokio::spawn(run(command, "Fixture"));
        tokio::time::timeout(Duration::from_secs(5), async {
            while !dir.path().join("started").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        task.abort();
        let _ = task.await;
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert!(!dir.path().join("leaked").exists());
    }
}
