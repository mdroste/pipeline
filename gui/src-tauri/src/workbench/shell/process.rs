//! The SDK applies containment; macOS also tracks inherited sandbox identity
//! because descendants can leave the SDK's process session. A drop guard signals
//! cancellation even if the async tool future is abandoned during launch. Readers drain
//! concurrently and retain at most 64 KiB each. No captured SDK run API is used
//! because its output collection is unbounded.
use mxc_sdk::v1::{self, ContainerRequest};
use serde_json::{json, Value};
use std::{
    io::Read,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

static ACTIVE: Mutex<Vec<std::sync::Weak<Control>>> = Mutex::new(Vec::new());

struct Control {
    cancelled: AtomicBool,
    child: Mutex<Option<Child>>,
}

struct Child {
    process: v1::MxcProcess,
    #[cfg(target_os = "macos")]
    tree: super::macos::process_tree::Tree,
    terminated: bool,
}

impl Child {
    fn kill(&mut self) -> Result<(), String> {
        if self.terminated {
            return Ok(());
        }
        let native = self.process.kill().map_err(|e| e.to_string());
        #[cfg(target_os = "macos")]
        let native = {
            // Always sweep, including when the foreground process has exited
            // or the SDK's session cleanup failed.
            let detached = self.tree.terminate();
            native.and(detached)
        };
        self.terminated = native.is_ok();
        native
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}
impl Control {
    fn stop(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        // App exit must signal the live OS tree before terminating the host.
        // Polling holds this lock only for try_wait, never while sleeping.
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(child) = child.as_mut() {
            let _ = child.kill();
        }
    }
}

pub(super) fn cancel_all() {
    let mut active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
    active.retain(|flag| {
        if let Some(flag) = flag.upgrade() {
            flag.stop();
            true
        } else {
            false
        }
    });
}

struct Cancel(Arc<Control>);
impl Drop for Cancel {
    fn drop(&mut self) {
        self.0.stop();
    }
}

pub(super) async fn run(
    request: ContainerRequest,
    scratch: tempfile::TempDir,
) -> Result<Value, String> {
    let flag = Arc::new(Control {
        cancelled: AtomicBool::new(false),
        child: Mutex::new(None),
    });
    {
        let mut active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
        active.retain(|flag| flag.strong_count() > 0);
        active.push(Arc::downgrade(&flag));
    }
    let _cancel = Cancel(flag.clone());
    tokio::task::spawn_blocking(move || {
        // Keep scratch alive until the OS sandbox and every reader have stopped.
        let _scratch = scratch;
        execute(request, flag)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn capture(mut reader: Box<dyn Read + Send>) -> Result<(String, bool), String> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    let mut truncated = false;
    loop {
        let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        let keep = n.min((64 * 1024usize).saturating_sub(bytes.len()));
        bytes.extend_from_slice(&buffer[..keep]);
        truncated |= keep < n;
    }
    let mut output = String::from_utf8_lossy(&bytes).into_owned();
    if output.len() > 64 * 1024 {
        let mut end = 64 * 1024;
        while !output.is_char_boundary(end) {
            end -= 1;
        }
        output.truncate(end);
        truncated = true;
    }
    Ok((output, truncated))
}

fn execute(request: ContainerRequest, cancelled: Arc<Control>) -> Result<Value, String> {
    let start = Instant::now();
    let timeout = Duration::from_millis(request.timeout_ms.unwrap_or(120_000).min(120_000).into());
    if cancelled.cancelled.load(Ordering::SeqCst) {
        return Err("Command cancelled.".into());
    }
    #[cfg(target_os = "macos")]
    let (request, tree) = {
        let mut request = request;
        let tree = super::macos::process_tree::Tree::prepare(&mut request)?;
        (request, tree)
    };
    let process = v1::spawn(
        request,
        v1::SpawnOptions {
            telemetry: Some(v1::TelemetryConfig {
                enabled: Some(false),
            }),
        },
    )
    .map_err(|e| format!("The command sandbox could not start: {e}"))?;
    let mut child = Child {
        process,
        #[cfg(target_os = "macos")]
        tree,
        terminated: false,
    };
    #[cfg(target_os = "macos")]
    child.tree.verify(child.process.id())?;
    let mut stdin = child
        .process
        .take_stdin()
        .ok_or("Command start pipe is missing")?;
    let stdout = child
        .process
        .take_stdout()
        .ok_or("Command output pipe is missing")?;
    let stderr = child
        .process
        .take_stderr()
        .ok_or("Command error pipe is missing")?;
    let stdout_closer = child
        .process
        .stdout_closer()
        .ok_or("Command output cannot be cancelled")?;
    let stderr_closer = child
        .process
        .stderr_closer()
        .ok_or("Command error output cannot be cancelled")?;
    {
        let mut owned = cancelled.child.lock().unwrap_or_else(|e| e.into_inner());
        *owned = Some(child);
        if !cancelled.cancelled.load(Ordering::SeqCst) {
            // The child cannot parse or execute the user's command until this
            // owner can synchronously stop it. Cancellation during SDK startup
            // leaves the gate closed, even if spawning finishes much later.
            std::io::Write::write_all(&mut stdin, b"pipeline-command-start\n")
                .map_err(|e| e.to_string())?;
            std::io::Write::flush(&mut stdin).map_err(|e| e.to_string())?;
        }
    }
    drop(stdin);
    let result = std::thread::scope(|scope| {
        let (finished, readers) = std::sync::mpsc::channel();
        let out_finished = finished.clone();
        let stdout = scope.spawn(move || {
            let output = capture(stdout);
            let _ = out_finished.send(());
            output
        });
        let stderr = scope.spawn(move || {
            let output = capture(stderr);
            let _ = finished.send(());
            output
        });
        let status = loop {
            if cancelled.cancelled.load(Ordering::SeqCst) {
                break Err("Command cancelled.".to_owned());
            }
            if start.elapsed() >= timeout {
                break Err(format!(
                    "Command timed out after {} seconds",
                    timeout.as_secs_f64()
                ));
            }
            let status = cancelled
                .child
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_mut()
                .unwrap()
                .process
                .try_wait();
            match status {
                Ok(Some(code)) => break Ok(code),
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(error) => break Err(error.to_string()),
            }
        };
        // Stop owned descendants even after the shell exits.
        let mut child = cancelled
            .child
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .unwrap();
        let killed = child.kill();
        let waited = child.process.wait().map_err(|e| e.to_string());
        // Even a successful foreground exit must not leave a blocking join
        // behind an inherited pipe. Give buffered output one shared grace
        // period, then interrupt both readers and report incomplete cleanup.
        let output_closed = if status.is_ok() && killed.is_ok() && waited.is_ok() {
            let deadline = Instant::now() + Duration::from_secs(1);
            (0..2).all(|_| {
                readers
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .is_ok()
            })
        } else {
            false
        };
        if !output_closed {
            stdout_closer.close();
            stderr_closer.close();
        }
        let stdout = stdout.join().map_err(|_| "Command output reader failed")?;
        let stderr = stderr.join().map_err(|_| "Command error reader failed")?;
        let code = status?;
        killed?;
        waited?;
        if !output_closed {
            return Err("Command output pipes did not close after termination".into());
        }
        let stdout = stdout?;
        let stderr = stderr?;
        Ok(
            json!({"exitCode":code,"stdout":stdout.0,"stderr":stderr.0,"truncated":stdout.1||stderr.1}),
        )
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_utf8_cannot_expand_past_the_output_limit() {
        let (output, truncated) =
            capture(Box::new(std::io::Cursor::new(vec![255; 64 * 1024]))).unwrap();
        assert!(output.len() <= 64 * 1024);
        assert!(truncated);
    }
}
